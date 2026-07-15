# dashpaint — boundary B

    crate    crates/dashpaint
    covers   v0.1 walking skeleton (story #3) + v0.3 paint vocabulary
             (story #13) + resolved subtree clips (story #97)

## Purpose

`dashpaint` defines boundary B (`docs/design/architecture.md`): the
complete input a painter consumes, and the trait every painter implements.
Principle P2 (`AGENTS.md`) holds throughout — a painter only colors; it
never measures, wraps, kerns, or moves anything.

Boundary B is a rect table plus a paint table plus a clip table. The paint vocabulary is
the v0.3 slice's set (`docs/roadmap.md`'s v0.3, drawn from
`docs/specification/04-figma-vocabulary-profile.md`'s NOW list): solid fills, the four
gradient kinds, image fills with scale modes, stroke with align,
per-corner radii, and clip. The crate has no dependencies, including no
`dashscene-core` and no `dashbuf` — see
`docs/decisions/dashpaint-owns-boundary-b-types.md` for why.

## The boundary-B contract

- The rect-table index is the document DFS node index
  (`docs/design/dashbuf.md`); a `RectEntry` carries no id field of its own.
- `RectEntry.paint` is an index into the `PaintTable`.
- `RectEntry.clip` is an index into the `ClipTable`. Clipping crosses
  this boundary already resolved: `dashscene-core` walks the clipping
  ancestors at commit, because a flat rect table carries none for a
  painter to walk (P2, story #97).
- Solid-fill color is 4×f32 RGBA — the same shape as `dashbuf`'s `Color`
  struct (`crates/dashbuf/schema/dashbuf.fbs`), reproduced here as a
  plain type rather than shared by dependency.
- The generation stamp (`docs/design/architecture.md`) belongs to the double
  buffer `dashscene-core` owns, not to individual rect entries; it is out
  of scope for this crate.

## Public interface

All types and the trait live in `crates/dashpaint/src/lib.rs`:

- `Color` — `#[repr(C)]` RGBA, 4×f32 fields `r`, `g`, `b`, `a`.
- `RectEntry` — `#[repr(C)]`, fields `x`, `y`, `w`, `h: f32`,
  `paint: PaintIndex` (20 bytes total, pinned by test).
- `PaintIndex` — `#[repr(transparent)]` newtype over `u32` (story #4,
  debt #54): a node index or other bare `u32` cannot cross into a
  paint index without an explicit wrap; layout unchanged.
- `PaintKind` — the fill vocabulary: `Solid { color }` (the pinned
  v0.1 shape), `Gradient(Gradient)`, `Image { image, scale_mode,
  transform, tile_scale }` (crop transform and tile magnification,
  story #14).
- Vocabulary value types — `Vec2`, `GradientStop`, `GradientKind`
  (Linear/Radial/Angular/Diamond), `Gradient` (kind + three normalized
  handle positions + stops), `ScaleMode` (Fill/Fit/Crop/Tile),
  `StrokeAlign` (Inside/Center/Outside), `Stroke` (width + align +
  solid color), `CornerRadii` (per-corner, `Default` = sharp).
- `MAX_GRADIENT_STOPS` — the gradient stop budget (story #15). It lives
  here, on boundary B, because it is a property of the paint vocabulary
  rather than of one backend: `dashscene-skia` asserts against it and
  `dashscene-validator` rejects it upstream (P4), and two hard-coded
  copies that drifted would make the validator's guarantee false.
- `PaintEntry` — the paint-table entry: `fill: Option<PaintKind>`
  (`None` = a paint-less, layout-only node), `stroke: Option<Stroke>`,
  `corners: CornerRadii`; `PaintEntry::solid(Color)` is the v0.1
  shorthand. See `docs/decisions/paint-entry-composition.md`. It carries
  no clip flag — whether a node clips its children is intent, and lives
  in the document and the arena, not in resolved painter input
  (`docs/decisions/resolved-clip-regions-at-commit.md`).
- `PaintTable` — a dense entry list behind a private field, indexed by
  `RectEntry.paint`: `new`, `push(&mut self, PaintEntry) -> PaintIndex`
  (returns the sequential index just assigned), `get(&self, PaintIndex)
  -> Option<&PaintEntry>`, `resolve(&self, PaintIndex) -> &PaintEntry`
  (the lookup painters use — panics on an out-of-range index), `len`,
  `is_empty`.
- `ImageTable` / `ImageAsset` / `ImageFormat` — encoded, format-tagged
  image assets (mirrors `dashbuf`'s `Document.images`), indexed by
  `PaintKind::Image`'s `image` field; same push/get/resolve contract as
  `PaintTable`. See
  `docs/decisions/image-assets-cross-boundary-b.md` (story #14).
- `Mat23` — row-major 2×3 affine; the image crop transform's shape.
- `ClipBox` — `#[repr(C)]`, one clipping ancestor's resolved box:
  `x`, `y`, `w`, `h: f32` plus `corners: CornerRadii` (all-zero radii =
  a sharp box).
- `ClipRegion` — the clip that applies to one rect: the boxes to
  **intersect**, outermost ancestor first (`boxes()`), behind a private
  field. No boxes = unclipped (`unclipped()`, `is_unclipped()`). The
  list is not pre-intersected into one box because the intersection of
  two rounded rects is not a rounded rect.
- `ClipTable` / `ClipIndex` — the region pool, same push/get/resolve
  contract as `PaintTable`, with one addition: `ClipTable::new()`
  reserves index 0 (`ClipIndex::UNCLIPPED`) for the unclipped region, so
  every rect resolves without a sentinel. `len()` counts it; a clip
  table is never empty, so there is no `is_empty`.
- `Painter` — the one trait every paint backend implements:
  `fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
  images: &ImageTable, clips: &ClipTable)` (an empty image table is
  valid input for image-less scenes; a fresh `ClipTable` is valid input
  for a scene that clips nothing).

`Color`, `RectEntry` and `ClipBox` are `#[repr(C)]` because
`docs/design/architecture.md` calls rect entries blittable and R-T4 plans
dirty-range instance-buffer uploads of per-frame painter input; fixing
the layout now costs nothing. A `RectEntry` is 24 bytes — four
coordinates plus the paint and clip indices — pinned by test.

`Painter::paint` is infallible and the trait is object-safe (`Box<dyn
Painter>` must work — backend selection is whole-scene, R3). Slice order
defines stacking — a later entry composites over an earlier one, since
DFS order encodes document stacking. The composited result is the
contract; iteration order is the implementation's choice (the lean
painter draws opaque cores front-to-back,
`docs/specification/03-target-hardware-rules.md`'s R-T2).
An out-of-range paint index is a broken contract between crates;
`PaintTable::resolve` centralizes the panic for that case, so no painter
invents its own failure path (a silent skip would be the silent drop P4
forbids). See `docs/decisions/painter-trait-infallible-slice-input.md`
for the alternatives considered on the trait's signature.

## Testing

`crates/dashpaint/tests/boundary_b.rs` exercises the public API only,
against hand-built fixtures, with no `dashscene-core` dependency. It
covers the `PaintTable` and `ClipTable` indexing contracts (including
both `resolve` panics and the reserved unclipped region), the
`PaintEntry` composition (solid shorthand, paint-less entry, full
gradient+stroke+corners entry, image fill), the recorded output of a
`RecordingPainter` test double over a two-rect fixture — including the
clip region it resolves per rect — and dyn-dispatch through
`&mut dyn Painter`. The test file is
the executable statement of the boundary-B contract; this section
deliberately does not restate its cases.

## Subtree clipping

`Paint.clip` ("clips its children to its box", `docs/design/architecture.md`)
is a relation between a node and its descendants — the one construct a
painter cannot be handed directly, since the flat rect table has no
ancestors and P2 forbids re-deriving them. `dashscene-core` resolves it
at commit (issue #97): every rect carries the `ClipRegion` its clipping
ancestors add up to, and a painter intersects the boxes it is given
without asking which node each came from. A clipping node does not clip
itself — only its descendants; its own corner radii still shape its own
fill and stroke. The full contract and the rejected alternatives are
`docs/decisions/resolved-clip-regions-at-commit.md`.

## Trace

- Satisfies: `docs/design/architecture.md` painter trait (boundary B)
  and output shape, `docs/roadmap.md`'s v0.3 paint vocabulary (from
  `docs/specification/04-figma-vocabulary-profile.md`'s NOW list);
  issue #3, #13 and #97 acceptance
  criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness), #14 (v0.3 painting).
- Related decisions: `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`,
  `docs/decisions/resolved-clip-regions-at-commit.md`.
