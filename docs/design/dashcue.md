# dashcue — animation vocabulary + scheduler

    crate    crates/dashcue
    covers   v0.4 slice: variant-transition vocabulary + scheduler (story #21)

## Purpose

`dashcue` is the descriptive animation vocabulary of `DESIGN_1.md` §6.3
and the runtime scheduling that advances it. Producers declare _how_ a
change animates as data; the runtime owns time and advances the
animation (P3 — nothing producer-side executes inside the frame loop).

This slice implements the variant-transition part of the vocabulary and
the scheduling skeleton: vocabulary data types (a variant transition is
per-prop specs — tween / spring / keyframes — plus a stagger), defined
and tested mid-flight retarget semantics (R4), and a scheduler that
advances active tracks from a caller-supplied time step and exposes the
current sampled values.

Out of scope for this slice (later vocabulary rows of §6.3): per-prop
smoothing, loop tracks, standalone keyframe tracks, enter/exit specs.
FLIP capture and wiring is story #22 (`dashscene-engine`).

## The seam (SCOPE_DECISIONS.md §9)

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
  of the bound `from → to` span, not absolute prop values — see
  `docs/decisions/dashcue-keyframe-values-are-progress-fractions.md`.
- **`dashcue` has no dependencies.** Props are identified by an opaque
  `PropKey(u64)` the caller encodes (the engine packs node index and
  channel into it). This keeps the §9 dependency direction: consumers
  depend on `dashcue`, never the reverse.

Animated values are `f32` scalars. A multi-channel prop (a color, a
rect) animates as one track per channel.

## Public API

All types live in `crates/dashcue/src/vocabulary.rs`, re-exported flat
from `crates/dashcue/src/lib.rs`:

- `PropKey(pub u64)` — opaque, caller-encoded prop identity; `dashcue`
  only compares it.
- `Easing` — `Linear` / `EaseIn` / `EaseOut` / `EaseInOut`, fixed cubic
  polynomials via `apply(self, t: f32) -> f32`. Exotic curve shapes are
  data (`TransitionSpec::Keyframes`), not more `Easing` variants.
- `Keyframe { t, value }` — `t` is a fraction of the spec's duration,
  strictly inside `(0, 1)` and strictly increasing across a frame list;
  `value` is a progress fraction of the bound `from → to` span (may
  leave `[0, 1]` — overshoot is data). Implicit endpoints `(0, 0)` and
  `(1, 1)`.
- `TransitionSpec` — `Tween { duration, easing }`, `Spring { stiffness,
  damping_ratio }` (Compose's `SpringSpec` shape, so Compose specs map
  onto this as data), or `Keyframes { duration, frames }`.
- `PropTransition { prop, spec }` — one prop's declared spec.
- `VariantTransition { tracks, stagger }` — `stagger` is the delay in
  seconds between successive tracks' starts, applied in declaration
  order.

The scheduler lives in `crates/dashcue/src/scheduler.rs`:

- `Scheduler::new() -> Self`.
- `start(&mut self, key, from, to, spec, delay)` — starts or retargets
  one track.
- `start_transition(&mut self, transition: &VariantTransition, bind: impl
  FnMut(PropKey) -> (f32, f32))` — binds a declared transition: one
  `start` per track, in declaration order, with `delay = stagger *
  index`; `bind` supplies `(from, to)` per prop key. This is the entry
  point story #22 calls at commit time.
- `advance(&mut self, dt: f32)` — advances every live track by `dt`
  seconds (the runtime clock's step); drops tracks that finished before
  this call.
- `sample(&self, key: PropKey) -> Option<f32>` — current sampled value
  of a live track.
- `samples(&self) -> impl Iterator<Item = (PropKey, f32)>` — live
  tracks, in start order (a retarget re-enters at the back).
- `len(&self) -> usize`, `is_empty(&self) -> bool` — count live tracks,
  including one that finished this frame and will be dropped by the
  next `advance`.

## Semantics

**Advancing.** The scheduler never reads a clock; the runtime calls
`advance(dt)` once per frame with its own step (P3). Tracks whose
stagger delay has not elapsed hold at `from`. A track that reaches its
end during an `advance` samples at exactly `to` for the rest of that
frame; the next `advance` removes it before advancing the rest. The
frame contract is: `advance(dt)`, then read `sample`/`samples`.

**Finishing.** A tween or keyframes track finishes when its elapsed
time reaches `duration`. A spring finishes when both `|value − to|`
and `|velocity|` are below rest thresholds (`REST_DELTA`,
`REST_VELOCITY`); it then snaps to exactly `to`. The thresholds are
crate constants at v0.4 (track values are pixel-scaled there — FLIP
rects); promoting them to spec data is deferred until non-pixel props
animate.

**Retarget (R4).** Calling `start` for a key that already has a live
track retargets it. One uniform rule: the new track's `from` is the old
track's current sample (the caller-supplied `from` argument is
ignored), the new spec's clock starts at zero, and the new `delay`
applies as given. On top of that, keyed by the _new_ spec kind:

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
semi-implicit Euler; the caller's step is split into equal substeps
below the stability bound `h < 1 / ((2ζ + 1)·ω)`, so one large `dt` (a
frame hitch) cannot make the integration diverge, and a frame-scale
step within the bound is a single plain Euler step (see
`docs/decisions/dashcue-spring-uses-semi-implicit-euler.md`). Easing
curves are fixed cubic polynomials; keyframes interpolate linearly. No
transcendental libm calls, no wall clock, no hashing — the same
`advance` sequence produces bit-identical samples everywhere (needed
for E5 goldens at t = 0 / 0.5 / 1 and E6).

**Bounded cost (R4).** A tween or keyframes track advances in O(1); a
spring track advances in O(substeps), proportional to `dt` over the
spec's stability bound and cut short as soon as the spring reaches
rest. No track allocates during `advance`; one frame costs O(live
tracks). Track storage is a `Vec` scanned
linearly by key — fine at v0.4 scale, revisit with the v0.8 stress
corpus.

**Broken contracts panic (house rule, dashpaint precedent).** Specs are
validated upstream eventually (P4); `start` centralizes the panic for a
spec no valid document can contain: non-finite or non-positive
`duration`/`stiffness`, negative `damping_ratio`/`delay`/`stagger`,
non-finite `from`/`to`, keyframe `t` outside `(0, 1)` or not strictly
increasing, non-finite keyframe values. `advance` panics on a negative
or non-finite `dt`.

## Alternatives considered

- **Generic value type (`V: Lerp`) vs `f32` scalars** — chose `f32`.
  Multi-channel props decompose into per-channel tracks; the
  boundary-B tables are `f32` throughout; a lerp trait adds surface no
  v0 consumer needs.
- **Scheduler owns the clock (`Instant`) vs caller-advanced `dt`** —
  chose caller-advanced. The runtime owns time (P3); fixed-step calls
  make tests and goldens deterministic.
- **Depend on `dashscene-core` for prop identity vs opaque key** —
  chose the opaque `PropKey(u64)`. Keeps `dashcue` standalone (§9
  dependency rationale) and lets #20/#21 merge in either order.

Two further deviations bind downstream work closely enough to warrant
their own decision records rather than a bullet here: the spring
integration scheme
(`docs/decisions/dashcue-spring-uses-semi-implicit-euler.md`) and the
keyframe value representation
(`docs/decisions/dashcue-keyframe-values-are-progress-fractions.md`).

## Testing

Integration tests against the public API only (house style, dashpaint
precedent), fixed time steps throughout, across three files:

- `tests/vocabulary.rs` — easing polynomial values; vocabulary types
  are plain, comparable data.
- `tests/scheduling.rs` — tween advances deterministically to hand-
  computed values and finishes at exactly `to`; easing variants hit
  their polynomial values; stagger holds a delayed track at `from`;
  spring converges to the target and finishes (rest thresholds), is
  bit-deterministic across repeated runs, and moves monotonically
  toward the target when critically damped; keyframes interpolate
  through declared frames including overshoot and degrade to linear
  with no declared frames; the finished-track lifecycle (exact `to` on
  the finishing frame, removed by the next `advance`); panics on
  invalid specs and on a negative `dt`.
- `tests/retarget.rs` — mid-flight retarget: tween restarts from the
  current sample toward the new target and ignores the caller's
  `from`; spring keeps position and velocity (no discontinuity in the
  sample sequence, verified against a fresh spring at the same
  position); a tween-to-spring retarget hands off zero velocity;
  retarget during the stagger delay re-arms from the held sample;
  `start_transition` staggers tracks by declaration order and panics
  on a negative stagger.

Story #22 consumes this API for FLIP; nothing here renders, so no
goldens in this story (E5 goldens are #23).

## Trace

- Satisfies: `DESIGN_1.md` §6.3 (descriptive animation vocabulary,
  variant-transition row); R4 (interruptibility, bounded cost); issue
  #21 acceptance criteria.
- Blocks: #22 (`dashscene-engine`, FLIP capture/wiring and commit-time
  binding of `(from, to)`); #23 (E5 goldens); future `dashbuf` schema
  work for persisting transition specs.
- Related decisions:
  `docs/decisions/dashcue-spring-uses-semi-implicit-euler.md`,
  `docs/decisions/dashcue-keyframe-values-are-progress-fractions.md`.
