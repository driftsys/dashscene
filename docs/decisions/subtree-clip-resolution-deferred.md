# Subtree clipsContent resolves in dashscene-core, not the painter (issue #97)

    status   accepted (story #14, 2026-07-12); resolved by story #97,
             2026-07-13 — see docs/decisions/resolved-clip-regions-at-commit.md
    scope    dashpaint (PaintEntry.clip), dashscene-skia; dashscene-core

## Context

`PaintEntry.clip` marks a node whose box clips its descendants
(`Paint.clip`, `specs/DESIGN_1.md` §8.1). Boundary B is a flat rect
table (`dashpaint::RectEntry` slice): a painter has no parent/child
structure to walk, and P2 forbids a painter re-deriving the tree from
it. Painting `entry.clip` correctly therefore needs the ancestor-clip
region computed before boundary B, not inside `paint()`.

## Options

1. Leave `entry.clip` unpaintable at story #14 (a named `unimplemented!`),
   and file a `dashscene-core` follow-up story to resolve ancestor clips
   into painter-consumable data at commit — a contract extension (a
   resolved clip region per rect) left for that story to define.
2. Approximate subtree clips now by intersecting geometry in core's
   committed rects: clip each descendant rect's committed bounds
   against the ancestor's box.
3. Have the painter itself walk stacking order and infer clip ancestry
   from slice-order conventions.

## Choice

Option 1.

## Why

- Option 2 is correct only for axis-aligned clips on solid fills: it
  distorts gradient frames (a gradient's frame is built from handle
  positions normalized to the node's own box, not a clipped sub-rect)
  and cannot express rounded clips (rounding a rect's bounds is not the
  same operation as rounding a paint's rrect).
- Option 3 asks the painter to re-derive tree structure from a flat
  rect table, which P2 forbids outright — a painter only colors.
- Rounded corners and image-content clipping are legitimately the
  painter's job because they shape one entry's own fill and stroke
  against its own box. Subtree clipping is a different operation —
  clipping other entries' geometry against this entry's box — which
  needs ancestor relationships boundary B does not carry.
- A named panic (`entry.clip` → `unimplemented!`, naming issue #97)
  keeps the failure visible per P4 rather than a silent misrender; the
  v0.3 corpus that exercises rounded corners and image clipping does
  not need subtree clip to ship.

## Consequences

- `dashscene-core` owns a follow-up story (issue #97): resolve ancestor
  clips into painter-consumable data at commit. This decision
  deliberately does not fix that contract's shape.
- Until issue #97 lands, every painter that implements the v0.3
  vocabulary must panic by name on `entry.clip`, never skip it silently.
- Rounded corners (shaping an entry's own fill and stroke) and
  image-content clipping to the entry's box are unaffected — both are
  the entry's own geometry, not a subtree relationship, and both ship
  with story #14 (`docs/design/dashscene-skia.md`).

## Resolution (story #97, 2026-07-13)

The deferred contract now exists:
`docs/decisions/resolved-clip-regions-at-commit.md`. Commit resolves
each rect's clipping ancestors into a `dashpaint::ClipRegion` — the
(rounded) boxes to intersect — and `RectEntry.clip` indexes the
`ClipTable` that holds them. `PaintEntry::clip: bool` is gone from
boundary B (the intent stays in `dashbuf`'s `Paint.clip` and the arena's
`Prop::Clip`), and with it the `unimplemented!` this record put in the
reference painter. Option 2 above stayed rejected on exactly the grounds
recorded here; the shape chosen expresses rounded and nested clips,
which option 2 cannot.

## Trace

- Satisfies: `specs/DESIGN_1.md` §8.1 (`Paint.clip`); issue #14
  acceptance criteria.
- Files: issue #97 (`dashscene-core`: resolve clipsContent into
  painter-consumable clips at commit) — closed by
  `docs/decisions/resolved-clip-regions-at-commit.md`.
- Related: `docs/design/dashpaint.md`; `docs/design/dashscene-skia.md`;
  `docs/decisions/paint-entry-composition.md`.
