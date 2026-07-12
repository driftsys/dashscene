# dashcue springs integrate with semi-implicit Euler, not a closed form

    status   accepted (story #21, 2026-07-12)
    scope    crates/dashcue; binds story #22 (dashscene-engine) and any
             future dashbuf schema field for spring parameters

## Context

`dashcue`'s spring transition (`TransitionSpec::Spring`) must produce
the same sample sequence on every machine: E5 pins golden images at
t = 0 / 0.5 / 1 and E6 the same, so `Scheduler::advance` cannot use any
operation whose result varies by platform or libm implementation.
Springs also need mid-flight retarget (R4) to be natural: an old
track's motion state must carry forward into a new target without a
visible jump.

## Options

1. Closed-form spring position: evaluate the standard damped harmonic
   oscillator equation (involves `exp`/`cos`) at the track's elapsed
   time.
2. Semi-implicit (symplectic) Euler integration of the spring's
   equation of motion, stepped inside each `advance(dt)` call in equal
   substeps below the stability bound `h < 1 / ((2ζ + 1)·ω)` (a
   frame-scale `dt` within the bound is a single step; a frame hitch
   splits into several, so the integration cannot diverge).

## Choice

Option 2.

## Why

- `exp`/`cos` are transcendental libm calls; their last-bit results are
  not guaranteed identical across platforms or libm implementations,
  which breaks the cross-machine golden stability E5/E6 require.
  Euler's per-step math is IEEE 754 basic operations (add, multiply)
  plus `sqrt` for the damping coefficient — all correctly rounded, so
  the same `advance` sequence is bit-identical everywhere.
- Euler's state is exactly `(position, velocity)`. That is what makes
  retarget (R4) natural: a mid-flight retarget carries the old track's
  velocity into the new one directly; a closed-form solution would
  have to re-derive an equivalent state from elapsed time, which does
  not read simply.
- The tradeoff is real but small at v0.4 scale: Euler only approaches
  the target, never reaches it exactly, so the scheduler needs rest
  thresholds (`REST_DELTA`, `REST_VELOCITY`) to decide when a spring
  has finished. Those thresholds are crate constants tuned for the
  pixel-scaled values v0.4 animates (FLIP rects); a non-pixel prop (for
  example, a 0..1 opacity channel) would need different thresholds —
  promoting them to spec data is deferred until such a prop exists
  (see `docs/design/dashcue.md`).
