# dashcue v0.4 (vocabulary + scheduler) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `dashcue`'s variant-transition vocabulary (tween /
spring / keyframes specs, stagger) and the caller-advanced scheduler
with defined mid-flight retarget (issue #21, spec:
`docs/wip/2026-07-12-dashcue-design.md`).

**Architecture:** One dependency-free library crate, two modules:
`src/vocabulary.rs` (data types + easing evaluation) and
`src/scheduler.rs` (tracks, advance, retarget); `src/lib.rs` re-exports
everything flat. Integration-style tests exercise the public API only.

**Tech Stack:** Rust (edition 2024), no dependencies. Gate: `just build`
(test + clippy -D warnings + fmt --check + dprint + markdownlint).

## Global Constraints

- No dependencies in `crates/dashcue/Cargo.toml` — in particular no
  `dashscene-core`, no `dashbuf` (spec: "The seam").
- Animated values are `f32` scalars; props are opaque `PropKey(u64)`.
- The scheduler never reads a clock — time arrives only as the `dt`
  argument of `advance` (P3, determinism).
- `advance` math is IEEE basic ops + `sqrt` only — no `exp`, `sin`,
  `cos`, `powf` (cross-machine golden stability, E5/E6).
- Broken contracts panic with a message naming the violated rule
  (house rule, dashpaint precedent).
- Commits: conventional, scope `dashcue`.

---

### Task 1: Vocabulary types + easing evaluation

**Files:**

- Create: `crates/dashcue/src/vocabulary.rs`
- Modify: `crates/dashcue/src/lib.rs` (crate docs + module + re-exports)
- Modify: `crates/dashcue/Cargo.toml` (stale description — still claims
  the staged-mutation API that SCOPE_DECISIONS.md §9 moved to core)
- Create: `crates/dashcue/tests/vocabulary.rs`

**Interfaces:**

- Produces: `dashcue::{PropKey, Easing, Keyframe, TransitionSpec,
  PropTransition, VariantTransition}` and
  `Easing::apply(self, t: f32) -> f32`. Tasks 2–4 consume all of these.

- [ ] **Step 1: Write the failing test**

`crates/dashcue/tests/vocabulary.rs`:

```rust
//! Vocabulary-type tests (issue #21): dashcue's public API only.

use dashcue::{Easing, Keyframe, PropKey, PropTransition, TransitionSpec, VariantTransition};

#[test]
fn easing_polynomials_hit_their_fixed_values() {
    // Linear: t. EaseIn: t^3. EaseOut: 1-(1-t)^3.
    // EaseInOut: 4t^3 below 1/2, 1-4(1-t)^3 above.
    for e in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
    }
    assert_eq!(Easing::Linear.apply(0.25), 0.25);
    assert_eq!(Easing::EaseIn.apply(0.25), 0.015625);
    assert_eq!(Easing::EaseIn.apply(0.5), 0.125);
    assert_eq!(Easing::EaseOut.apply(0.5), 0.875);
    assert_eq!(Easing::EaseOut.apply(0.75), 0.984375);
    assert_eq!(Easing::EaseInOut.apply(0.25), 0.0625);
    assert_eq!(Easing::EaseInOut.apply(0.5), 0.5);
    assert_eq!(Easing::EaseInOut.apply(0.75), 0.9375);
}

#[test]
fn vocabulary_types_are_plain_comparable_data() {
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: PropKey(7),
            spec: TransitionSpec::Keyframes {
                duration: 0.3,
                frames: vec![Keyframe { t: 0.5, value: 1.5 }],
            },
        }],
        stagger: 0.05,
    };
    assert_eq!(transition.clone(), transition);
    assert_ne!(PropKey(7), PropKey(8));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashcue --test vocabulary`
Expected: FAIL to compile — `unresolved import dashcue::{...}`.

- [ ] **Step 3: Write minimal implementation**

`crates/dashcue/Cargo.toml` — replace the `description` line:

```toml
description = "Descriptive animation vocabulary + its runtime scheduling (DESIGN_1.md §6.3)."
```

`crates/dashcue/src/vocabulary.rs`:

```rust
//! The descriptive animation vocabulary (DESIGN_1.md §6.3): data a
//! producer declares; the runtime advances it (P3). No resolved values
//! live here (P1) — endpoints bind at commit time.

/// Opaque per-track key. The caller encodes prop identity into it (the
/// engine packs node index and channel); `dashcue` only compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropKey(pub u64);

/// Fixed cubic easing curves. Exotic shapes are data — use
/// [`TransitionSpec::Keyframes`] (DESIGN_1.md §6.3 "keyframe track").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Maps linear progress `t` in [0, 1] to eased progress. Polynomial
    /// only — bit-stable across machines (E5/E6 golden determinism).
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = 1.0 - t;
                    1.0 - 4.0 * u * u * u
                }
            }
        }
    }
}

/// One declared point of a keyframes curve.
///
/// `t` is a fraction of the spec's duration, strictly inside (0, 1) and
/// strictly increasing across a frame list. `value` is a progress
/// fraction of the bound `from → to` span — it may leave [0, 1]
/// (overshoot is data). The endpoints (0, 0) and (1, 1) are implicit.
/// Values are fractions, not absolute prop values, because a document
/// never carries resolved values (P1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub t: f32,
    pub value: f32,
}

/// How one prop travels from its old to its new resolved value.
/// Durations are seconds; spring parameters follow Compose's
/// `SpringSpec` (stiffness + damping ratio) so Compose specs map onto
/// this as data (DESIGN_1.md §6.3 "Compose calibration").
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionSpec {
    Tween { duration: f32, easing: Easing },
    Spring { stiffness: f32, damping_ratio: f32 },
    Keyframes { duration: f32, frames: Vec<Keyframe> },
}

/// One prop's declared spec inside a variant transition.
#[derive(Debug, Clone, PartialEq)]
pub struct PropTransition {
    pub prop: PropKey,
    pub spec: TransitionSpec,
}

/// A variant transition (DESIGN_1.md §6.3): per-prop specs plus a
/// stagger. Track `i` starts `stagger * i` seconds after the commit.
/// The `set_variant` switch itself is `dashscene-core`'s; this is the
/// data describing how the switch animates (SCOPE_DECISIONS.md §9).
#[derive(Debug, Clone, PartialEq)]
pub struct VariantTransition {
    pub tracks: Vec<PropTransition>,
    pub stagger: f32,
}
```

`crates/dashcue/src/lib.rs` — replace the stub entirely:

```rust
//! Descriptive animation vocabulary + its runtime scheduling (DESIGN_1.md §6.3).
//!
//! Producers declare *how* a change animates as data (the vocabulary);
//! the runtime owns time and advances it (P3). v0.4 scope: variant
//! transitions (tween / spring / keyframes + stagger) and the
//! [`Scheduler`] that advances them. A multi-channel prop (a color, a
//! rect) animates as one `f32` track per channel.

mod scheduler;
mod vocabulary;

pub use scheduler::Scheduler;
pub use vocabulary::{Easing, Keyframe, PropKey, PropTransition, TransitionSpec, VariantTransition};
```

For this task only, keep `src/scheduler.rs` as a placeholder so the
crate compiles (Task 2 fills it):

```rust
//! Runtime scheduling of the vocabulary — filled in by Task 2.

/// Placeholder — Task 2 implements the scheduler.
#[derive(Debug, Default)]
pub struct Scheduler {}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashcue --test vocabulary`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/dashcue
git commit -m "feat(dashcue): add the descriptive animation vocabulary types"
```

(The `docs/wip/` design and plan are committed separately as
`docs(dashcue): record the story #21 design and plan` before Task 1.)

---

### Task 2: Scheduler with tween tracks

**Files:**

- Modify: `crates/dashcue/src/scheduler.rs` (replace placeholder)
- Create: `crates/dashcue/tests/scheduling.rs`

**Interfaces:**

- Consumes: `PropKey`, `Easing`, `TransitionSpec` from Task 1.
- Produces: `Scheduler::new() -> Scheduler`,
  `start(&mut self, key: PropKey, from: f32, to: f32, spec: TransitionSpec, delay: f32)`,
  `advance(&mut self, dt: f32)`, `sample(&self, key: PropKey) -> Option<f32>`,
  `samples(&self) -> impl Iterator<Item = (PropKey, f32)> + '_`,
  `len(&self) -> usize`, `is_empty(&self) -> bool`. Tasks 3–4 consume
  all of these.

- [ ] **Step 1: Write the failing test**

`crates/dashcue/tests/scheduling.rs`:

```rust
//! Scheduler tests (issue #21): fixed time steps, hand-computed
//! expectations — dashcue's public API only.

use dashcue::{Easing, PropKey, Scheduler, TransitionSpec};

const K: PropKey = PropKey(1);

fn linear_tween(duration: f32) -> TransitionSpec {
    TransitionSpec::Tween {
        duration,
        easing: Easing::Linear,
    }
}

#[test]
fn tween_advances_deterministically_with_a_fixed_step() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);

    assert_eq!(s.sample(K), Some(0.0)); // live before any advance
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(25.0));
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(50.0));
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(75.0));
}

#[test]
fn eased_tween_samples_the_easing_polynomial() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Tween {
            duration: 1.0,
            easing: Easing::EaseInOut,
        },
        0.0,
    );

    s.advance(0.25);
    assert_eq!(s.sample(K), Some(6.25)); // 4 * 0.25^3 * 100
}

#[test]
fn finished_track_samples_exactly_to_then_the_next_advance_drops_it() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);

    s.advance(1.5); // overshoots the duration
    assert_eq!(s.sample(K), Some(100.0)); // exact `to`, still sampleable
    assert_eq!(s.len(), 1);

    s.advance(0.0); // next frame: finished track is dropped first
    assert_eq!(s.sample(K), None);
    assert!(s.is_empty());
}

#[test]
fn delayed_track_holds_at_from_until_the_delay_elapses() {
    let mut s = Scheduler::new();
    s.start(K, 10.0, 20.0, linear_tween(1.0), 0.5);

    s.advance(0.25);
    assert_eq!(s.sample(K), Some(10.0)); // still inside the delay
    s.advance(0.5); // 0.25 left of delay, then 0.25 of track time
    assert_eq!(s.sample(K), Some(12.5));
}

#[test]
fn samples_iterates_live_tracks_in_start_order() {
    let mut s = Scheduler::new();
    s.start(PropKey(2), 0.0, 1.0, linear_tween(1.0), 0.0);
    s.start(PropKey(1), 5.0, 6.0, linear_tween(1.0), 0.0);

    s.advance(0.5);
    let got: Vec<(PropKey, f32)> = s.samples().collect();
    assert_eq!(got, vec![(PropKey(2), 0.5), (PropKey(1), 5.5)]);
}

#[test]
#[should_panic(expected = "dt")]
fn advance_panics_on_a_negative_dt() {
    let mut s = Scheduler::new();
    s.advance(-0.1);
}

#[test]
#[should_panic(expected = "duration")]
fn start_panics_on_a_non_positive_tween_duration() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 1.0, linear_tween(0.0), 0.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashcue --test scheduling`
Expected: FAIL to compile — `Scheduler` has none of these methods.

- [ ] **Step 3: Write minimal implementation**

Replace `crates/dashcue/src/scheduler.rs`:

```rust
//! Runtime scheduling of the vocabulary: the runtime calls
//! [`Scheduler::advance`] once per frame with its own clock's step —
//! the scheduler never reads a clock (P3), so a fixed step replays
//! bit-identically (E5 goldens at t = 0 / 0.5 / 1, E6).

use crate::vocabulary::{Keyframe, PropKey, TransitionSpec};

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
    /// Elapsed track time (past the delay). Meaningless for springs.
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
        assert!(delay.is_finite() && delay >= 0.0, "delay must be finite and >= 0");

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
        self.tracks.iter().find(|t| t.key == key).map(|t| t.position)
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
                let acceleration =
                    -stiffness * (self.position - self.to) - damping * self.velocity;
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashcue --test scheduling`
Expected: PASS (7 tests). `cargo test -p dashcue` all green.

- [ ] **Step 5: Commit**

```bash
git add crates/dashcue
git commit -m "feat(dashcue): add the caller-advanced scheduler with tween tracks"
```

---

### Task 3: Spring and keyframes tracks

**Files:**

- Modify: `crates/dashcue/tests/scheduling.rs` (append)
- Modify: `crates/dashcue/src/scheduler.rs` (only if a test exposes a
  defect — Task 2 already lands the spring/keyframes code paths)

**Interfaces:**

- Consumes: everything Task 2 produces, plus
  `TransitionSpec::{Spring, Keyframes}` and `Keyframe` from Task 1.
- Produces: verified spring convergence/determinism and keyframes
  interpolation — behavior Task 4 and story #22 rely on.

- [ ] **Step 1: Write the failing-or-passing tests**

Append to `crates/dashcue/tests/scheduling.rs` (add `Keyframe` to the
`use dashcue::{...}` list):

```rust
const STEP: f32 = 1.0 / 120.0;

fn critical_spring() -> TransitionSpec {
    TransitionSpec::Spring {
        stiffness: 100.0,
        damping_ratio: 1.0,
    }
}

#[test]
fn spring_converges_to_the_target_and_finishes() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);

    let mut steps = 0;
    while !s.is_empty() {
        s.advance(STEP);
        steps += 1;
        assert!(steps < 10_000, "spring never reached rest");
    }
    // The finishing frame sampled exactly `to` before the drop:
    // rerun and stop on the finishing frame.
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);
    for _ in 0..steps - 1 {
        s.advance(STEP);
    }
    assert_eq!(s.sample(K), Some(100.0));
}

#[test]
fn spring_advance_is_bit_deterministic_across_runs() {
    let run = || {
        let mut s = Scheduler::new();
        s.start(K, 0.0, 100.0, critical_spring(), 0.0);
        let mut samples = Vec::new();
        for _ in 0..240 {
            s.advance(STEP);
            samples.extend(s.sample(K).map(f32::to_bits));
        }
        samples
    };
    assert_eq!(run(), run());
}

#[test]
fn spring_moves_monotonically_toward_the_target_when_critically_damped() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);

    let mut previous = 0.0;
    for _ in 0..240 {
        s.advance(STEP);
        let Some(now) = s.sample(K) else { break };
        assert!(now >= previous, "critically damped spring moved away from the target");
        assert!(now <= 100.0 + 1.0, "critically damped spring exceeded the target");
        previous = now;
    }
}

#[test]
fn keyframes_interpolate_through_declared_frames_including_overshoot() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![Keyframe { t: 0.5, value: 1.5 }],
        },
        0.0,
    );

    s.advance(0.25); // between (0,0) and (0.5,1.5): progress 0.75
    assert_eq!(s.sample(K), Some(75.0));
    s.advance(0.25); // at the declared frame: progress 1.5 (overshoot)
    assert_eq!(s.sample(K), Some(150.0));
    s.advance(0.25); // between (0.5,1.5) and (1,1): progress 1.25
    assert_eq!(s.sample(K), Some(125.0));
    s.advance(0.25); // done: exactly `to`
    assert_eq!(s.sample(K), Some(100.0));
}

#[test]
fn keyframes_with_no_declared_frames_degrade_to_linear() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![],
        },
        0.0,
    );

    s.advance(0.5);
    assert_eq!(s.sample(K), Some(50.0));
}

#[test]
#[should_panic(expected = "strictly increasing")]
fn start_panics_on_unsorted_keyframes() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        1.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![Keyframe { t: 0.6, value: 0.5 }, Keyframe { t: 0.4, value: 0.9 }],
        },
        0.0,
    );
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p dashcue --test scheduling`
Expected: PASS (13 tests). These verify code paths Task 2 landed; a
failure here is a defect — fix `scheduler.rs` minimally until green
(the spring monotonicity test in particular guards the Euler step).

- [ ] **Step 3: Commit**

```bash
git add crates/dashcue
git commit -m "test(dashcue): pin spring convergence/determinism and keyframes interpolation"
```

---

### Task 4: Retarget semantics + variant transitions with stagger

**Files:**

- Modify: `crates/dashcue/src/scheduler.rs` (add `start_transition`)
- Create: `crates/dashcue/tests/retarget.rs`

**Interfaces:**

- Consumes: everything Tasks 1–2 produce, plus `VariantTransition` /
  `PropTransition`.
- Produces:
  `Scheduler::start_transition(&mut self, transition: &VariantTransition, bind: impl FnMut(PropKey) -> (f32, f32))`
  — the entry point story #22 calls at commit time.

- [ ] **Step 1: Write the failing test**

`crates/dashcue/tests/retarget.rs`:

```rust
//! Mid-flight retarget (R4) and variant-transition stagger tests
//! (issue #21): dashcue's public API only.

use dashcue::{Easing, PropKey, PropTransition, Scheduler, TransitionSpec, VariantTransition};

const K: PropKey = PropKey(1);
const STEP: f32 = 1.0 / 120.0;

fn linear_tween(duration: f32) -> TransitionSpec {
    TransitionSpec::Tween {
        duration,
        easing: Easing::Linear,
    }
}

fn spring() -> TransitionSpec {
    TransitionSpec::Spring {
        stiffness: 100.0,
        damping_ratio: 1.0,
    }
}

#[test]
fn tween_retarget_restarts_from_the_current_sample_and_ignores_from() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(50.0));

    s.start(K, 999.0, 0.0, linear_tween(1.0), 0.0); // `from` ignored
    assert_eq!(s.sample(K), Some(50.0)); // continuous at the retarget
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(25.0)); // halfway from 50 toward 0
}

#[test]
fn spring_retarget_keeps_position_and_velocity() {
    // A: launch toward 100, then retarget to 0 mid-flight.
    let mut a = Scheduler::new();
    a.start(K, 0.0, 100.0, spring(), 0.0);
    for _ in 0..60 {
        a.advance(STEP);
    }
    let mid = a.sample(K).unwrap();
    a.start(K, 999.0, 0.0, spring(), 0.0);
    assert_eq!(a.sample(K), Some(mid)); // position carried

    // B: a fresh spring at the same position with zero velocity.
    let mut b = Scheduler::new();
    b.start(K, mid, 0.0, spring(), 0.0);

    a.advance(STEP);
    b.advance(STEP);
    // Both accelerate toward 0, but A still carries its old upward
    // velocity, so after one step A sits above B — that difference is
    // exactly the carried velocity times the step.
    assert!(a.sample(K).unwrap() > b.sample(K).unwrap());
    assert!(b.sample(K).unwrap() < mid); // B starts from rest: straight down
}

#[test]
fn tween_to_spring_retarget_hands_off_zero_velocity() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);

    s.start(K, 999.0, 50.0, spring(), 0.0); // already at 50, no velocity
    s.advance(STEP);
    assert_eq!(s.sample(K), Some(50.0)); // at rest on the target: no motion
    s.advance(0.0);
    assert!(s.is_empty()); // and the rest thresholds finished it
}

#[test]
fn retarget_during_the_delay_rearms_from_the_held_sample() {
    let mut s = Scheduler::new();
    s.start(K, 10.0, 100.0, linear_tween(1.0), 1.0);
    s.advance(0.5); // still delayed, holding at `from`
    assert_eq!(s.sample(K), Some(10.0));

    s.start(K, 77.0, 200.0, linear_tween(1.0), 0.0); // `from` ignored
    assert_eq!(s.sample(K), Some(10.0));
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(105.0)); // halfway from 10 toward 200
}

#[test]
fn variant_transition_staggers_tracks_by_declaration_order() {
    let transition = VariantTransition {
        tracks: vec![
            PropTransition { prop: PropKey(1), spec: linear_tween(1.0) },
            PropTransition { prop: PropKey(2), spec: linear_tween(1.0) },
            PropTransition { prop: PropKey(3), spec: linear_tween(1.0) },
        ],
        stagger: 0.25,
    };
    let mut s = Scheduler::new();
    s.start_transition(&transition, |_| (0.0, 100.0));
    assert_eq!(s.len(), 3);

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(1)), Some(25.0)); // 0.25 into its tween
    assert_eq!(s.sample(PropKey(2)), Some(0.0)); // delay just consumed
    assert_eq!(s.sample(PropKey(3)), Some(0.0)); // 0.25 of delay left

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(1)), Some(50.0));
    assert_eq!(s.sample(PropKey(2)), Some(25.0));
    assert_eq!(s.sample(PropKey(3)), Some(0.0));

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(3)), Some(25.0));
}

#[test]
#[should_panic(expected = "stagger")]
fn start_transition_panics_on_a_negative_stagger() {
    let transition = VariantTransition {
        tracks: vec![],
        stagger: -0.1,
    };
    Scheduler::new().start_transition(&transition, |_| (0.0, 1.0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashcue --test retarget`
Expected: FAIL to compile — no method `start_transition`.

- [ ] **Step 3: Write minimal implementation**

In `crates/dashcue/src/scheduler.rs`, add `VariantTransition` to the
`use crate::vocabulary::{...}` line, and add to `impl Scheduler`:

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashcue --test retarget`
Expected: PASS (6 tests). Then `cargo test -p dashcue` — all green.

- [ ] **Step 5: Commit**

```bash
git add crates/dashcue
git commit -m "feat(dashcue): define mid-flight retarget and variant-transition stagger"
```

---

### Task 5: Full gate

- [ ] **Step 1: Run the workspace gate**

Run: `just build`
Expected: green (tests, clippy -D warnings, fmt, dprint, markdownlint).
Fix anything it flags, amend into the offending commit if trivial or
commit as `fix(dashcue): ...` otherwise.
