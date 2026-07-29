//! Runtime scheduling of the vocabulary: the runtime calls
//! [`Scheduler::advance`] once per frame with its own clock's step —
//! the scheduler never reads a clock (P3), so a fixed step replays
//! bit-identically (E5 goldens at t = 0 / 0.5 / 1, E6).

use crate::vocabulary::{Easing, Keyframe, PropKey, TransitionSpec, VariantTransition};

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
/// storage is a linearly scanned `Vec`, the deliberate choice recorded
/// in `docs/decisions/dashcue-scheduler-storage-stays-vec.md` (debt
/// #488) rather than a deferred revisit: no measurement has ever shown
/// the scan to cost anything, and `Vec`'s push/remove is what gives
/// [`Scheduler::samples`] its insertion-order guarantee (#77) for free.
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
    /// contract between crates is centralized here. `from` must be
    /// finite on a fresh start; on the retarget path it is discarded and
    /// therefore not checked (#71). The span the track actually runs
    /// over must be finite too: two finite endpoints can still span more
    /// than f32 holds (#70).
    pub fn start(&mut self, key: PropKey, from: f32, to: f32, spec: TransitionSpec, delay: f32) {
        validate_spec(&spec);
        assert!(to.is_finite(), "to must be finite");
        assert!(
            delay.is_finite() && delay >= 0.0,
            "delay must be finite and >= 0"
        );

        // The retarget path ignores the caller's `from`, so only a fresh
        // start checks it (#71). The live track is read here and removed
        // below, after the remaining checks, so a rejected `start` leaves
        // it untouched.
        let live = self.tracks.iter().position(|t| t.key == key);
        let (from, velocity) = match live {
            Some(i) => {
                let old = &self.tracks[i];
                let velocity = match (&old.spec, &spec) {
                    (TransitionSpec::Spring { .. }, TransitionSpec::Spring { .. }) => old.velocity,
                    _ => 0.0,
                };
                (old.position, velocity)
            }
            None => {
                assert!(from.is_finite(), "from must be finite");
                (from, 0.0)
            }
        };
        // Two finite endpoints can still span more than f32 holds: `to -
        // from` overflows to infinity and every interpolated sample is
        // infinite or NaN until the finish frame snaps to `to` (#70).
        assert!((to - from).is_finite(), "the from/to span must be finite");
        if let Some(i) = live {
            self.tracks.remove(i);
        }
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
    ///
    /// `bind` returns `None` to decline a track, which then does not
    /// start: the caller owns what counts as an unchanged prop, and a
    /// declined prop would otherwise hold a live constant-value track for
    /// the spec's whole duration (#74). Declining does not shift the
    /// remaining tracks — `delay` follows the declaration index — so the
    /// declared stagger describes the same schedule either way.
    ///
    /// Panics when two tracks declare the same prop: the second would
    /// take the retarget path and drop the first track's spec and delay
    /// with no diagnostic (P4, #69).
    pub fn start_transition(
        &mut self,
        transition: &VariantTransition,
        mut bind: impl FnMut(PropKey) -> Option<(f32, f32)>,
    ) {
        assert!(
            transition.stagger.is_finite() && transition.stagger >= 0.0,
            "stagger must be finite and >= 0"
        );
        // Checked over the whole list before anything starts, so a
        // rejected transition leaves the scheduler untouched. Linear scan
        // per track, matching the scheduler's own storage — a transition
        // declares a handful of tracks.
        for (index, track) in transition.tracks.iter().enumerate() {
            assert!(
                !transition.tracks[..index]
                    .iter()
                    .any(|earlier| earlier.prop == track.prop),
                "duplicate prop key {:?} in one variant transition",
                track.prop
            );
        }
        for (index, track) in transition.tracks.iter().enumerate() {
            let Some((from, to)) = bind(track.prop) else {
                continue;
            };
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
    /// That order is part of the contract, not an artifact of the
    /// current `Vec` storage (#77): consumers emit frame output in it,
    /// and the goldens depend on it being deterministic (E5/E6).
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
        // One path for both time-parameterised specs (#75): the elapsed
        // accumulation, the finish-and-snap at `duration` and the
        // `from -> to` interpolation are shared; only the progress
        // function differs.
        if let Some((duration, progress)) = timed(&self.spec) {
            self.elapsed += dt;
            if self.elapsed >= duration {
                self.position = self.to;
                self.finished = true;
            } else {
                let p = progress.at(self.elapsed / duration);
                self.position = self.from + p * (self.to - self.from);
            }
            return;
        }
        match &self.spec {
            // Handled by the timed path above.
            TransitionSpec::Tween { .. } | TransitionSpec::Keyframes { .. } => {}
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
                // threshold takes the same relative fraction of the spring's
                // characteristic velocity `ω · scale` rather than of the
                // position scale, so it is sized in velocity units (#214);
                // sized in position units it became the binding condition and
                // held a large-magnitude track open past the point the
                // position gate was satisfied. The absolute floors stay
                // per-prop-threshold placeholders (deferred spec data).
                let scale = (self.to - self.from).abs().max(self.to.abs());
                let rest_delta = REST_DELTA.max(REST_REL * scale);
                let rest_velocity = REST_VELOCITY.max(REST_REL * omega * scale);
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

/// How a timed spec turns the elapsed fraction into progress: the only
/// thing the tween and keyframes paths differ in (#75).
enum Progress<'a> {
    Eased(Easing),
    Frames(&'a [Keyframe]),
}

impl Progress<'_> {
    /// Progress at elapsed fraction `t`, in [0, 1).
    fn at(&self, t: f32) -> f32 {
        match self {
            Progress::Eased(easing) => easing.apply(t),
            Progress::Frames(frames) => keyframes_progress(frames, t),
        }
    }
}

/// A time-parameterised spec's duration and progress function — `None`
/// for a spring, which is integrated rather than sampled by elapsed
/// fraction.
fn timed(spec: &TransitionSpec) -> Option<(f32, Progress<'_>)> {
    match spec {
        TransitionSpec::Tween { duration, easing } => Some((*duration, Progress::Eased(*easing))),
        TransitionSpec::Keyframes { duration, frames } => {
            Some((*duration, Progress::Frames(frames)))
        }
        TransitionSpec::Spring { .. } => None,
    }
}

/// Piecewise-linear progress through the declared frames, with the
/// implicit endpoints (0, 0) and (1, 1). `t` is in [0, 1).
fn keyframes_progress(frames: &[Keyframe], t: f32) -> f32 {
    /// The implicit terminal endpoint, chained onto the declared frames
    /// so the segment interpolation has one site (#76).
    const END: Keyframe = Keyframe { t: 1.0, value: 1.0 };

    let (mut t0, mut v0) = (0.0, 0.0);
    for frame in frames.iter().chain(std::iter::once(&END)) {
        if t < frame.t {
            return v0 + (t - t0) / (frame.t - t0) * (frame.value - v0);
        }
        (t0, v0) = (frame.t, frame.value);
    }
    // `t` is in [0, 1), so the chained endpoint always terminates the
    // loop above; at or past it, progress holds the final value.
    v0
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
