# dashcue v0.4 — animation vocabulary + scheduler — design

    story    #21 (epic #19, slice v0.4)
    branch   story/dashcue-vocabulary
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

`dashcue` is the descriptive animation vocabulary of `DESIGN_1.md` §6.3
and the runtime scheduling that advances it. Producers declare _how_ a
change animates as data; the runtime owns time and advances the
animation (P3 — nothing producer-side executes inside the frame loop).

This story implements the variant-transition part of the vocabulary and
the scheduling skeleton:

- vocabulary data types: a variant transition is per-prop specs
  (tween / spring / keyframes) plus a stagger
- defined, tested mid-flight retarget semantics (R4)
- a scheduler that advances active tracks from a caller-supplied time
  step and exposes the current sampled values

Out of scope for this story (later vocabulary rows of §6.3): per-prop
smoothing, loop tracks, standalone keyframe tracks, enter/exit specs.
FLIP capture and wiring is story #22 (`dashscene-engine`).

## The seam (SCOPE_DECISIONS.md §9, issue #20/#21/#22)

- `set_variant` — the structural switch — is `dashscene-core`'s.
- The transition spec describing how that switch animates is `dashcue`
  data referenced by the commit.
- At commit time the engine (#22) knows each affected prop's old and
  new resolved values. It binds them onto the declared transition and
  hands the scheduler concrete tracks: `(key, from, to, spec, delay)`.

Two consequences shape the API:

- **The vocabulary carries no resolved values.** A document must not
  contain resolved positions (P1), so a transition spec cannot name
  absolute targets. Keyframe values are therefore _progress fractions_
  of the bound `from → to` span, not absolute prop values (a deliberate
  deviation from Compose's absolute-valued `keyframes {}`; the shape of
  the curve is data, the endpoints bind at commit).
- **`dashcue` has no dependencies.** Props are identified by an opaque
  `PropKey(u64)` the caller encodes (the engine packs node index and
  channel into it). This keeps the §9 dependency direction: consumers
  depend on `dashcue`, never the reverse.

Animated values are `f32` scalars. A multi-channel prop (a color, a
rect) animates as one track per channel.

## Public API

All types live in `dashcue` with no dependencies.

    pub struct PropKey(pub u64);   // opaque, caller-encoded

    pub enum Easing { Linear, EaseIn, EaseOut, EaseInOut }

    pub struct Keyframe { pub t: f32, pub value: f32 }
    // t: 0..1 fraction of the duration, strictly increasing;
    // value: progress fraction of from→to (may leave 0..1 — overshoot
    // is data). Implicit endpoints (0, 0) and (1, 1).

    pub enum TransitionSpec {
        Tween { duration: f32, easing: Easing },
        Spring { stiffness: f32, damping_ratio: f32 },
        Keyframes { duration: f32, frames: Vec<Keyframe> },
    }

    pub struct PropTransition { pub prop: PropKey, pub spec: TransitionSpec }

    pub struct VariantTransition {
        pub tracks: Vec<PropTransition>,
        pub stagger: f32,   // seconds between successive tracks' starts
    }

    pub struct Scheduler { /* active tracks */ }

    impl Scheduler {
        pub fn new() -> Self;
        /// Starts or retargets one track.
        pub fn start(&mut self, key: PropKey, from: f32, to: f32,
                     spec: TransitionSpec, delay: f32);
        /// Binds a declared transition: one `start` per track, in
        /// declaration order, with `delay = stagger * index`.
        /// `bind` supplies `(from, to)` per prop key.
        pub fn start_transition(&mut self, transition: &VariantTransition,
                                bind: impl FnMut(PropKey) -> (f32, f32));
        /// Advances every track by `dt` seconds (the runtime clock's
        /// step). Drops tracks that finished before this call.
        pub fn advance(&mut self, dt: f32);
        /// Current sampled value of a live track.
        pub fn sample(&self, key: PropKey) -> Option<f32>;
        /// Live tracks, in start order: `(key, current value)`.
        pub fn samples(&self) -> impl Iterator<Item = (PropKey, f32)>;
        pub fn len(&self) -> usize;
        pub fn is_empty(&self) -> bool;
    }

## Semantics

**Advancing.** The scheduler never reads a clock; the runtime calls
`advance(dt)` once per frame with its own step (P3). Tracks whose
stagger delay has not elapsed hold at `from`. A track that reaches its
end during an `advance` samples at exactly `to` for the rest of that
frame; the next `advance` removes it before advancing the rest. The
frame contract is: `advance(dt)`, then read `sample`/`samples`.

**Finishing.** A tween or keyframes track finishes when its elapsed
time reaches `duration`. A spring finishes when both `|value − to|`
and `|velocity|` are below rest thresholds; it then snaps to exactly
`to`. The thresholds are crate constants at v0.4 (track values are
pixel-scaled there — FLIP rects); promoting them to spec data is
deferred until non-pixel props animate.

**Retarget (R4).** Calling `start` for a key that already has a live
track retargets it. One uniform rule: the new track's `from` is the old
track's current sample (the caller-supplied `from` argument is ignored),
the new spec's clock starts at zero, and the new `delay` applies as
given. On top of that, keyed by the _new_ spec kind:

- **Spring:** additionally inherits the old track's velocity when the
  old track was a spring (the natural spring behavior §6.3 names);
  tween and keyframes tracks hand off zero velocity.
- **Tween / keyframes:** position-only restart from the current sample
  toward the new target.
- A retarget during the stagger delay follows the same rule; the
  current sample is still `from`, so the track simply re-arms from it.

**Determinism.** `advance` uses only IEEE 754 basic operations
(add/mul/div and `sqrt` for the spring's damping coefficient — all
correctly rounded, bit-stable across machines). Springs integrate with
semi-implicit Euler at the caller's step; easing curves are fixed cubic
polynomials; keyframes interpolate linearly. No transcendental libm
calls, no wall clock, no hashing — the same `advance` sequence produces
bit-identical samples everywhere (needed for E5 goldens at
t = 0 / 0.5 / 1 and E6).

**Bounded cost (R4).** Each track advances in O(1) with no allocation;
one frame costs O(live tracks). Track storage is a `Vec` scanned
linearly by key — fine at v0.4 scale, revisit with the v0.8 stress
corpus.

**Broken contracts panic (house rule, dashpaint precedent).** Specs are
validated upstream eventually (P4); `start` centralizes the panic for a
spec no valid document can contain: non-finite or non-positive
`duration`/`stiffness`, negative `damping_ratio`/`delay`/`stagger`,
non-finite `from`/`to`, keyframe `t` outside (0, 1) or not strictly
increasing, non-finite keyframe values. `advance` panics on a negative
or non-finite `dt`.

## Alternatives considered

- **Generic value type (`V: Lerp`) vs `f32` scalars** — chose `f32`.
  Multi-channel props decompose into per-channel tracks; the boundary-B
  tables are `f32` throughout; a lerp trait adds surface no v0 consumer
  needs.
- **Closed-form spring (exp/cos) vs semi-implicit Euler** — chose
  Euler. Closed form needs `exp`/`cos`, whose results vary across libm
  implementations, breaking cross-machine golden stability (E5, E6).
  Euler's state is `(position, velocity)`, which is exactly what makes
  retarget natural.
- **Scheduler owns the clock (`Instant`) vs caller-advanced `dt`** —
  chose caller-advanced. The runtime owns time (P3); fixed-step calls
  make tests and goldens deterministic.
- **Absolute keyframe values (Compose shape) vs progress fractions** —
  chose fractions. The document cannot carry resolved values (P1), and
  fractions keep retarget defined (rebind `from`/`to`, reuse the
  curve).
- **Depend on `dashscene-core` for prop identity vs opaque key** —
  chose the opaque `PropKey(u64)`. Keeps `dashcue` standalone (§9
  dependency rationale) and lets #20/#21 merge in either order.

## Testing

Integration tests against the public API only (house style, dashpaint
precedent), fixed time steps throughout:

- tween advances deterministically: sampled values at fixed steps match
  hand-computed expectations; finishes at exactly `to`
- easing variants hit their polynomial values at t = 0.25 / 0.5 / 0.75
- spring converges to the target and finishes (rest thresholds); two
  identical runs produce identical sample sequences
- keyframes interpolate through declared frames, including overshoot
- stagger: later tracks hold at `from` for `stagger * index` seconds
- retarget mid-flight: spring keeps velocity (no discontinuity in the
  sample sequence); tween restarts from the current sample toward the
  new target; retarget during delay replaces the track
- finished-track lifecycle: exact `to` on the finishing frame, removed
  by the next `advance`
- panics: one representative invalid spec, negative `dt`

Story #22 consumes this API for FLIP; nothing here renders, so no
goldens in this story (E5 goldens are #23).
