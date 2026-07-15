# The engine binds FLIP's resolved values; dashcue carries only the spec

    status   accepted (story #22, 2026-07-15). VariantFlip is built at v0.4.
    scope    dashscene-engine's VariantFlip; the dashcue VariantTransition
             seam; the v0.7 serialised binding table

## Context

A variant switch animates its layout delta with FLIP — First, Last, Invert,
Play: measure the geometry before the switch and after it, then play each moved
node from its old rect to its new one. `dashcue` is descriptive: it carries
intent, never results (P1), so a `VariantTransition` spec cannot name absolute
`x`/`y`/`w`/`h` targets. Something has to turn a declared transition into a
concrete animation with real `(from, to)` values and a per-node address.
(`docs/archive/2026-07-14-design-1-seed.md` §6.3; `docs/design/dashscene-engine.md`,
"FLIP".)

## Options

1. Put the resolved `(from, to)` into the transition spec — `dashcue` names the
   absolute geometry each track travels between.
2. The engine binds the resolved values at commit: it captures the before and
   after rects from the retained solve (First/Last), decomposes each rect into
   channels, packs a `(node, channel)` `PropKey`, and hands the scheduler
   concrete `(key, from, to, spec, delay)` tracks. `dashcue` stays descriptive;
   timing and retarget are the scheduler's.
3. A separate per-node animator outside `dashcue`'s scheduler.

## Choice

Option 2.

- `dashscene-engine`'s `VariantFlip` owns the `(node, channel)` `PropKey`
  packing (the node's arena slot in the high bits, the channel discriminant in
  the low two). A rect is a multi-channel prop, so it animates as one `dashcue`
  track per channel.
- At commit the engine knows each animated prop's old resolved value (the before
  solve) and its new one (the after solve) and binds them onto the declared
  transition through `Scheduler::start_transition` — the resolved values never
  enter the vocabulary (P1).
- An interrupting switch retargets through the scheduler's existing rule: a
  `start` on a live key resumes from the current sample and a spring keeps its
  velocity, so nothing snaps. Each frame costs `O(animated nodes)` with no
  per-frame allocation (R4).

## Why

- **P1** — a transition spec that named absolute targets would put resolved
  geometry in the document. Keeping `(from, to)` out of `dashcue` is what lets
  the same spec drive any switch.
- **P2** — geometry is the engine's. The engine already runs the solve, so
  binding `(from, to)` from the before/after rects keeps the one place that
  knows resolved geometry as the one place that reads it.
- **R4** — bounded cost and interruptibility come from reusing the scheduler's
  retarget instead of a bespoke animator (option 3), which would re-implement
  the track lifecycle `dashcue` already owns.

## Open consistency

The "engine owns the packing" intent is not yet uniform. The `dashlang` reactive
layer (story #166) packs the same opaque `PropKey` differently — `(dense << 32) |
code` versus the engine's `(index << 2) | channel`. The two use separate
`Scheduler` instances, so there is no runtime collision today, but v0.7's
serialised binding table needs one canonical packing that round-trips through the
document. Reconciling the two packings is tracked in debt #208; the related gap —
FLIP does not validate that a track's key is engine-packed — is #207.
