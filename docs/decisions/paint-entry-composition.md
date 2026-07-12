# dashpaint: the paint-table entry is a composition (fill, stroke, corners, clip)

    status   accepted (story #13, 2026-07-12)
    scope    crates/dashpaint

## Context

v0.1's paint-table entry was a bare `PaintKind` with one variant
(`Solid`). The v0.3 vocabulary adds constructs that are not fills:
stroke with align, per-corner radii, and clip. A node can carry a fill
AND a stroke AND corners at once, and debt #55 recorded that boundary B
had no representation for a paint-less (layout-only) node at all.

## Options

1. `PaintEntry { fill: Option<PaintKind>, stroke: Option<Stroke>,
   corners: CornerRadii, clip: bool }` as the table entry; `PaintKind`
   stays the fill vocabulary.
2. Keep the entry a bare `PaintKind` and encode stroke/corners/clip
   inside every fill variant.
3. Option 1 but with a `PaintKind::None` variant instead of
   `fill: Option<PaintKind>`.

## Choice

Option 1.

## Why

- Option 2 duplicates stroke/corner/clip fields across every fill
  variant and still cannot express "stroke but no fill" or "no paint at
  all". `DESIGN_1.md` §5's paint-table row lists "fill/stroke/effect
  params" as siblings of the kind enum — a composition, not a variant
  payload.
- Option 3 makes "no fill" a member of the fill vocabulary; an `Option`
  keeps `PaintKind` meaning "a way to fill" and cannot be confused with
  a drawable kind. `fill: None` resolves #55's gap directly.
- Contract note: the pinned v0.1 cross-session contract fixes
  `RectEntry`, `Color`, `PaintKind::Solid`'s shape, and the type names —
  all untouched. `PaintTable`'s entry composition is not frozen, and
  `dashscene-core` does not depend on `dashpaint` until story #4 unifies
  the types; that wiring is this same session's next story, so the shape
  lands with full knowledge on both sides.

## Sub-decisions recorded with this choice

- **Strokes are solid-only at v0.3.** Figma allows gradient/image
  strokes; the v0 corpus does not need them, `DESIGN_1.md` §10.1's NOW
  list names stroke _align_ only, and the field widens additively later.
  Until then the importer diagnoses them by name (R6) rather than
  dropping them.
- **Gradient geometry is three normalized handle positions** (origin,
  primary-axis end, secondary-axis end — Figma's
  gradientHandlePositions) for all four kinds, rather than per-kind
  parameter sets (center/radius, angle, …). Handles are the authored
  intent (P1); resolved geometry is per-painter math, and one model
  round-trips the importer losslessly.
- **Corner radii and clip live in the paint entry, not `RectEntry`.**
  `RectEntry`'s layout is pinned and blittable (§7.3); corners/clip are
  paint-side shape parameters and share the paint table's dedup pool.
- **Image assets stay out of boundary B for now.** The schema stores
  embedded encoded bytes (`Document.images`); how decoded pixels reach a
  painter is story #14's plumbing and is recorded there as an open item.
