# Decision: MSDF-only text rendering in v0; no per-size bitmap atlases yet

    status   accepted
    date     2026-07-12
    scope    resolves docs/technotes/open-questions.md's Q-1 for v0;
             binds #27 (atlas pipeline), #28/#30 (typeset + Skia glyph
             quads), and the validator's text checks (#373)
    evidence docs/technotes/arabic-atlas-coverage.md (spike #25)

## Context

`docs/technotes/open-questions.md`'s Q-1 left open whether small fixed
sizes (below ~14 px per em) need per-size baked bitmap atlases instead
of MSDF, pending a visual check.

## Options

1. MSDF only; treat text below a size floor as out-of-profile
   vocabulary (named validator diagnostic) until real hardware data
   exists.
2. Build per-size bitmap atlas pages alongside MSDF now.
3. Raise MSDF atlas resolution for small sizes.

## Choice

Option 1. For all of v0:

- The atlas pipeline (#27) produces MSDF only, keyed by glyph id,
  default `-size 32 -pxrange 4`, pinned generator version and seed.
- Text styled below 14 px per em is a warning-severity diagnostic
  (P4/R6: named diagnostic, no silent degrade). The floor is a constant
  the validator owns, so it can be revised from target-hardware
  measurements in v1 without a format change.
- Per-size bitmap atlas pages remain the designated fallback design
  if v1 hardware evaluation shows real screens need sub-14 px text.

The diagnostic is `text.style-below-msdf-floor`, raised by the load gate
against `dashscene_validator::MSDF_MIN_PX_PER_EM` (#373). It is checked
against the smallest em size the document can **reach** at runtime, not
the authored size: a construct that shrinks the text under its authored
size would otherwise pass the floor and smear anyway. The two answers
coincide for every document the v0 format can express, and that is a
classification rather than an assumption — `min_text_scale` walks the
document's binding rows and variant overrides, and no channel and no
override arm names `TextStyle.size`. Appending one fails the two registry
tests in `crates/dashscene-validator/src/document.rs`, which is where the
scale it reaches gets supplied.

## Why

- The spike's visual check answers Q-1 directly: MSDF matches direct
  rasterization closely at 14 px/em and above, is acceptably legible
  at 12 px/em, and degrades below that (dots and harakat smear).
- Option 3 is ruled out by measurement: a 48 px/em atlas with a wider
  pixel range does not materially improve rendering below 14 px/em.
  The bottleneck is screen-pixel sampling and missing hinting, not
  atlas resolution.
- Option 2 is premature: cockpit UI text on the actual targets is
  high-DPI and rarely below 14 px/em physical. Building a second
  atlas flavor now adds a format fork, a second runtime path, and a
  second golden family before any real screen demands it.
- A validator warning keeps the constraint visible to designers at
  import time instead of shipping quietly blurry text — the same
  posture `docs/specification/04-figma-vocabulary-profile.md` takes
  for every deferred vocabulary item.

## Alternatives considered and rejected

- Supersampled MSDF rendering at small sizes: improves smoothness,
  not stem alignment; costs per-pixel work on the weakest targets.
- Hinted bitmap fallback inside the MSDF atlas (mixed pages): revisit
  only together with option 2 in v1; adds the same second path.
