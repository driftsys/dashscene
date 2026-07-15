# Lottie: bake when possible, ThorVG only when you must (proposed)

    status   proposed — a direction, not yet ratified
    date     2026-07-13
    source   docs/technotes/runtime-content.md §4
    scope    dashc's asset pipeline; dashcue; the ThorVG escape hatch
             (docs/decisions/runtime-vector-via-thorvg-to-texture.md)

## Context

Lottie is not one thing; where a given animation sits decides whether it
bakes. `dashc` should triage each Lottie and emit a named diagnostic for
the path taken (P4/P5 — validated, never discovered).

## Leaning

- **Transform-only** (spinner, pulse, slide-in — most UI Lottie): bake
  the shapes into the SDF atlas and lower the keyframes into `dashcue`
  tracks driving their transforms. No runtime VG; stays resolution-
  independent, cheap, cross-backend, interruptible, and data-drivable (a
  live progress ring only works this way). Prefer this whenever it
  applies.
- **Canned full-frame, no runtime params** (small/short): bake a
  sprite-sheet — offline-render frames to a texture atlas, play as
  textured quads. No runtime VG. Cost is VRAM (frames × resolution), so
  small/short only; loses resolution independence and parameterization.
- **Path morphing / masks / mattes / runtime-dynamic**: no faithful bake,
  so fall back to ThorVG at runtime
  (`docs/decisions/runtime-vector-via-thorvg-to-texture.md`) — a
  budgeted escape hatch.

## Why this is a leaning, not a decision

The triage rule follows directly from the accepted bucket rule
(`docs/technotes/runtime-content.md` §1: resolve on "is it expressible in
what the runtime can already draw"), and preferring a bake whenever one
is faithful keeps the common case cheap and cross-backend. It is not yet
ratified because the mechanism that makes it real — `dashc`'s Lottie
triage logic, its VRAM-budget check for the sprite-sheet case, and its
reject-or-flag rule on `profile:core` for the VG case — is unbuilt and
unspiked.

## What ratifies this

- `dashc` Lottie triage + VRAM budget + `profile:core` reject rule,
  tracked as an open item in `docs/technotes/runtime-content.md` §8.

## Consequences

- The sprite-sheet path's VRAM cost and the runtime-VG path's per-frame
  render-target cost (Q-6) both need a number before either ships in a
  release build.
