//! Runtime scheduling of the vocabulary: the runtime calls
//! [`Scheduler::advance`] once per frame with its own clock's step —
//! the scheduler never reads a clock (P3), so a fixed step replays
//! bit-identically (E5 goldens at t = 0 / 0.5 / 1, E6).

use crate::vocabulary::{Keyframe, PropKey, TransitionSpec, VariantTransition};

/// Spring rest thresholds (crate constants at v0.4 — track values are
/// pixel-scaled there; promoting them to spec data is deferred).
///
/// The absolute constants are floors; the effective thresholds scale
/// with the animation's magnitude (see the Spring arm of
/// [`Track::advance`], issue #68). Absolute-only thresholds never trip
/// for large-magnitude targets: at |to| >= ~1.6e4 the f32 ulp exceeds
/// `REST_DELTA`, so `position` freezes an ulp short of `to` and the
/// joint rest test is never satisfied.
const REST_DELTA: f32 = 1e-3;
const REST_VELOCITY: f32 = 1e-3;
/// Relative rest tolerance, applied to the animation's characteristic
/// magnitude. Chosen to exceed the f32 relative precision (2^-23 ≈
/// 1.2e-7) with a safety margin, so the delta test always trips at any
/// scale, and to place the crossover with the absolute floors at
/// magnitude 100 (pixel scale) — below that, the absolute floors govern,
/// so pixel-scale springs keep their existing behavior.
const REST_REL: f32 = 1e-5;

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
    /// docs/decisions/staged-mutation-v01-scope.md).
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

    /// True when no track will produce a further sample — every track has
    /// finished, or there are none. A track that finished this frame lingers
    /// until the next `advance` sweeps it (see `advance`), so a settled
    /// scheduler is not necessarily empty.
    pub fn is_settled(&self) -> bool {
        self.tracks.iter().all(|t| t.finished)
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
                // Semi-implicit Euler: IEEE basic ops + sqrt only, so a
                // fixed step replays bit-identically on every machine
                // (no libm transcendentals). The caller's step is split
                // into equal substeps below the stability bound
                // h < 1 / ((2ζ + 1)·ω) — that keeps c·h < 1 and
                // ω·h ≤ 1, so a frame hitch (one large dt) cannot make
                // the integration diverge. For frame-scale steps within
                // the bound this is a single substep, i.e. the plain
                // Euler step.
                let omega = stiffness.sqrt();
                let damping = 2.0 * damping_ratio * omega;
                let h_max = 1.0 / ((2.0 * damping_ratio + 1.0) * omega);
                let substeps = ((dt / h_max).ceil() as u64).max(1);
                let h = dt / substeps as f32;
                // Rest thresholds scale with the animation's characteristic
                // magnitude (#68), so a spring settles in bounded time at any
                // scale. `max(|to - from|, |to|)` covers both a large span and
                // a large target reached from near it; taking the max of the
                // absolute floor and the relative term keeps small/normal
                // springs on their existing absolute thresholds. The velocity
                // threshold reuses the same magnitude scale — a relative
                // rest-velocity heuristic, matching how spring runtimes size
                // rest velocity against the animation distance (dashcue has no
                // per-prop visibility threshold yet; that is deferred spec
                // data).
                let scale = (self.to - self.from).abs().max(self.to.abs());
                let rest_delta = REST_DELTA.max(REST_REL * scale);
                let rest_velocity = REST_VELOCITY.max(REST_REL * scale);
                for _ in 0..substeps {
                    let acceleration =
                        -stiffness * (self.position - self.to) - damping * self.velocity;
                    self.velocity += acceleration * h;
                    self.position += self.velocity * h;
                    if (self.position - self.to).abs() < rest_delta
                        && self.velocity.abs() < rest_velocity
                    {
                        self.position = self.to;
                        self.finished = true;
                        break;
                    }
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
            // An undamped spring (damping_ratio == 0) conserves its
            // oscillation and never reaches rest, so it is rejected here
            // — Compose, which the parameters are calibrated against,
            // also requires dampingRatio > 0 (#72).
            assert!(
                damping_ratio.is_finite() && *damping_ratio > 0.0,
                "spring damping_ratio must be finite and > 0"
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
