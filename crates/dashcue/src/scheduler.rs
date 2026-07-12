//! Runtime scheduling of the vocabulary: the runtime calls
//! [`Scheduler::advance`] once per frame with its own clock's step —
//! the scheduler never reads a clock (P3), so a fixed step replays
//! bit-identically (E5 goldens at t = 0 / 0.5 / 1, E6).

use crate::vocabulary::{Keyframe, PropKey, TransitionSpec, VariantTransition};

/// Spring rest thresholds (crate constants at v0.4 — track values are
/// pixel-scaled there; promoting them to spec data is deferred).
const REST_DELTA: f32 = 1e-3;
const REST_VELOCITY: f32 = 1e-3;

struct Track {
    key: PropKey,
    from: f32,
    to: f32,
    spec: TransitionSpec,
    /// Stagger delay still to consume before track time runs.
    delay: f32,
    /// Elapsed track time (past the delay). Unused by springs.
    elapsed: f32,
    /// Current sampled value; starts at `from`, ends at exactly `to`.
    position: f32,
    /// Spring state; zero for tween/keyframes tracks.
    velocity: f32,
    finished: bool,
}

/// Advances active animation tracks from caller-supplied time steps.
///
/// Frame contract: `advance(dt)`, then read [`Scheduler::sample`] /
/// [`Scheduler::samples`]. A track that finishes during an `advance`
/// samples at exactly its target until the next `advance` removes it.
/// Each track advances in O(1) with no allocation (R4 bounded cost);
/// storage is a linearly scanned `Vec` — fine at v0.4 scale, revisit
/// with the v0.8 stress corpus.
#[derive(Default)]
pub struct Scheduler {
    tracks: Vec<Track>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts the track for `key`, or retargets it if one is live (R4):
    /// the new track then starts from the old one's current sample
    /// (`from` is ignored), and a new spring inherits an old spring's
    /// velocity. See the design's "Retarget" section.
    ///
    /// Panics on a spec no valid document can contain — specs are
    /// validated upstream eventually (P4); the panic for a broken
    /// contract between crates is centralized here.
    pub fn start(&mut self, key: PropKey, from: f32, to: f32, spec: TransitionSpec, delay: f32) {
        validate_spec(&spec);
        assert!(from.is_finite() && to.is_finite(), "from/to must be finite");
        assert!(
            delay.is_finite() && delay >= 0.0,
            "delay must be finite and >= 0"
        );

        let (from, velocity) = match self.tracks.iter().position(|t| t.key == key) {
            Some(i) => {
                let old = self.tracks.remove(i);
                let velocity = match (&old.spec, &spec) {
                    (TransitionSpec::Spring { .. }, TransitionSpec::Spring { .. }) => old.velocity,
                    _ => 0.0,
                };
                (old.position, velocity)
            }
            None => (from, 0.0),
        };
        self.tracks.push(Track {
            key,
            from,
            to,
            spec,
            delay,
            elapsed: 0.0,
            position: from,
            velocity,
            finished: false,
        });
    }

    /// Binds a declared variant transition at commit time: one
    /// [`Scheduler::start`] per track, in declaration order, with
    /// `delay = stagger * index`. `bind` supplies each prop's resolved
    /// `(from, to)` — resolved values never live in the vocabulary
    /// (P1); the engine binds them from the variant switch (issue #22,
    /// SCOPE_DECISIONS.md §9).
    pub fn start_transition(
        &mut self,
        transition: &VariantTransition,
        mut bind: impl FnMut(PropKey) -> (f32, f32),
    ) {
        assert!(
            transition.stagger.is_finite() && transition.stagger >= 0.0,
            "stagger must be finite and >= 0"
        );
        for (index, track) in transition.tracks.iter().enumerate() {
            let (from, to) = bind(track.prop);
            let delay = transition.stagger * index as f32;
            self.start(track.prop, from, to, track.spec.clone(), delay);
        }
    }

    /// Advances every track by `dt` seconds. Tracks that finished
    /// before this call are dropped first.
    pub fn advance(&mut self, dt: f32) {
        assert!(dt.is_finite() && dt >= 0.0, "dt must be finite and >= 0");
        self.tracks.retain(|t| !t.finished);
        for track in &mut self.tracks {
            track.advance(dt);
        }
    }

    /// Current sampled value of a live track.
    pub fn sample(&self, key: PropKey) -> Option<f32> {
        self.tracks
            .iter()
            .find(|t| t.key == key)
            .map(|t| t.position)
    }

    /// Live tracks, in start order (a retarget re-enters at the back).
    pub fn samples(&self) -> impl Iterator<Item = (PropKey, f32)> + '_ {
        self.tracks.iter().map(|t| (t.key, t.position))
    }

    /// Live tracks — including any that finished this frame and will be
    /// dropped by the next `advance`.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

impl Track {
    fn advance(&mut self, dt: f32) {
        // Consume the stagger delay first; only the remainder advances
        // track time. While delayed, the track holds at `from`.
        let eaten = self.delay.min(dt);
        self.delay -= eaten;
        let dt = dt - eaten;
        if dt <= 0.0 && self.delay > 0.0 {
            return;
        }
        match &self.spec {
            TransitionSpec::Tween { duration, easing } => {
                self.elapsed += dt;
                if self.elapsed >= *duration {
                    self.position = self.to;
                    self.finished = true;
                } else {
                    let p = easing.apply(self.elapsed / duration);
                    self.position = self.from + p * (self.to - self.from);
                }
            }
            TransitionSpec::Keyframes { duration, frames } => {
                self.elapsed += dt;
                if self.elapsed >= *duration {
                    self.position = self.to;
                    self.finished = true;
                } else {
                    let p = keyframes_progress(frames, self.elapsed / duration);
                    self.position = self.from + p * (self.to - self.from);
                }
            }
            TransitionSpec::Spring {
                stiffness,
                damping_ratio,
            } => {
                // Semi-implicit Euler at the caller's step: IEEE basic
                // ops + sqrt only, so a fixed step replays bit-identically
                // on every machine (no libm transcendentals).
                let damping = 2.0 * damping_ratio * stiffness.sqrt();
                let acceleration = -stiffness * (self.position - self.to) - damping * self.velocity;
                self.velocity += acceleration * dt;
                self.position += self.velocity * dt;
                if (self.position - self.to).abs() < REST_DELTA
                    && self.velocity.abs() < REST_VELOCITY
                {
                    self.position = self.to;
                    self.finished = true;
                }
            }
        }
    }
}

/// Piecewise-linear progress through the declared frames, with the
/// implicit endpoints (0, 0) and (1, 1). `t` is in [0, 1).
fn keyframes_progress(frames: &[Keyframe], t: f32) -> f32 {
    let (mut t0, mut v0) = (0.0, 0.0);
    for frame in frames {
        if t < frame.t {
            return v0 + (t - t0) / (frame.t - t0) * (frame.value - v0);
        }
        (t0, v0) = (frame.t, frame.value);
    }
    v0 + (t - t0) / (1.0 - t0) * (1.0 - v0)
}

fn validate_spec(spec: &TransitionSpec) {
    match spec {
        TransitionSpec::Tween { duration, .. } => {
            assert!(
                duration.is_finite() && *duration > 0.0,
                "tween duration must be finite and > 0"
            );
        }
        TransitionSpec::Spring {
            stiffness,
            damping_ratio,
        } => {
            assert!(
                stiffness.is_finite() && *stiffness > 0.0,
                "spring stiffness must be finite and > 0"
            );
            assert!(
                damping_ratio.is_finite() && *damping_ratio >= 0.0,
                "spring damping_ratio must be finite and >= 0"
            );
        }
        TransitionSpec::Keyframes { duration, frames } => {
            assert!(
                duration.is_finite() && *duration > 0.0,
                "keyframes duration must be finite and > 0"
            );
            let mut previous = 0.0;
            for frame in frames {
                assert!(
                    frame.t.is_finite() && frame.t > previous && frame.t < 1.0,
                    "keyframe t values must be strictly increasing inside (0, 1)"
                );
                assert!(frame.value.is_finite(), "keyframe values must be finite");
                previous = frame.t;
            }
        }
    }
}
