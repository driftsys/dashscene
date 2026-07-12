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
  a drawable kind.
- Contract note: the pinned v0.1 cross-session contract fixes
  `RectEntry`, `Color`, `PaintKind::Solid`'s shape, and the type names —
  all untouched. `PaintTable`'s entry composition is not frozen, and
  `dashscene-core` does not depend on `dashpaint` until story #4 unifies
  the types; that wiring is this same session's next story, so the shape
  lands with full knowledge on both sides.

## Relation to debt #55 (paint-less nodes) and core's sentinel

Two complementary mechanisms now exist, at different levels:

- `dashscene-core`'s committed output marks an unfilled node at the
  rect level: `RectEntry.paint = NO_PAINT (u32::MAX)`, no table entry
  at all (`docs/decisions/core-committed-output-shape.md`, merged with
  story #2). That record flags the conflict with `PaintTable::resolve`
  (which panics on any unresolvable index) for story #4 to resolve.
- This record adds the entry-level representation:
  `PaintEntry { fill: None, .. }` — needed regardless of the sentinel,
  because a stroke-only entry has no fill either, and the document
  schema's pool entry (`dashbuf`'s `Paint` table) has the same optional
  fill.

Resolved by story #4 (`docs/decisions/boundary-b-unification.md`):
the committed output interns the shared empty entry
(`PaintEntry::default()`) for unfilled nodes — every rect resolves, and
the `NO_PAINT` sentinel is gone from core's public API. `dashbuf`'s
document-level `Node.paint_entry` sentinel remains (a format concern,
not a boundary-B one). Debt #55 closed with story #4.

## Sub-decisions recorded with this choice

- **Strokes are solid-only at v0.3.** Figma allows gradient/image
  strokes; the v0 corpus does not need them, the v0.3 slice
  (`DESIGN_1.md` §11) scopes "rrect + stroke align" — not stroke
  fills — and the field widens additively later. Until then the
  importer diagnoses them by name (R6) rather than dropping them.
- **Gradient geometry is three named normalized handle positions**
  (`handle_origin`, `handle_primary`, `handle_secondary` — Figma's
  gradientHandlePositions) for all four kinds, rather than per-kind
  parameter sets (center/radius, angle, …) or a positional array.
  Handles are the authored intent (P1); resolved geometry is
  per-painter math; named fields keep the schema and the mirror types
  self-describing so a lowering cannot silently swap axes.
- **Corner radii and clip live in the paint entry, not `RectEntry`.**
  `RectEntry`'s layout is pinned and blittable (§7.3); corners/clip are
  paint-side shape parameters and share the paint table's dedup pool.
  Dedup itself is the producer's job — the table is append-only by
  design (core already interns by color bits, see its record above).
- **Image assets stay out of boundary B for now.** Resolved at
  story #14: encoded assets cross as a `dashpaint::ImageTable` parameter
  on `Painter::paint` (`docs/decisions/image-assets-cross-boundary-b.md`).
