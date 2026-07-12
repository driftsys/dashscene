# dashpaint — boundary B

    crate    crates/dashpaint
    covers   v0.1 walking skeleton (story #3) + v0.3 paint vocabulary
             (story #13)

## Purpose

`dashpaint` defines boundary B (`specs/DESIGN_1.md` §4, §7.3, §8): the
complete input a painter consumes, and the trait every painter implements.
Principle P2 (`AGENTS.md`) holds throughout — a painter only colors; it
never measures, wraps, kerns, or moves anything.

Boundary B is a rect table plus a paint table. The paint vocabulary is
the v0.3 slice's set (`specs/DESIGN_1.md` §11 v0.3, drawn from the
§10.1 NOW list): solid fills, the four
gradient kinds, image fills with scale modes, stroke with align,
per-corner radii, and clip. The crate has no dependencies, including no
`dashscene-core` and no `dashbuf` — see
`docs/decisions/dashpaint-owns-boundary-b-types.md` for why.

## The boundary-B contract

- The rect-table index is the document DFS node index
  (`specs/DESIGN_1.md` §5); a `RectEntry` carries no id field of its own.
- `RectEntry.paint` is an index into the `PaintTable`.
- Solid-fill color is 4×f32 RGBA — the same shape as `dashbuf`'s `Color`
  struct (`crates/dashbuf/schema/dashbuf.fbs`), reproduced here as a
  plain type rather than shared by dependency.
- The generation stamp (`specs/DESIGN_1.md` §7.3) belongs to the double
  buffer `dashscene-core` owns, not to individual rect entries; it is out
  of scope for this crate.

## Public interface

All types and the trait live in `crates/dashpaint/src/lib.rs`:

- `Color` — `#[repr(C)]` RGBA, 4×f32 fields `r`, `g`, `b`, `a`.
- `RectEntry` — `#[repr(C)]`, fields `x`, `y`, `w`, `h: f32`,
  `paint: u32`.
- `PaintKind` — the fill vocabulary: `Solid { color }` (the pinned
  v0.1 shape), `Gradient(Gradient)`, `Image { image, scale_mode }`.
- Vocabulary value types — `Vec2`, `GradientStop`, `GradientKind`
  (Linear/Radial/Angular/Diamond), `Gradient` (kind + three normalized
  handle positions + stops), `ScaleMode` (Fill/Fit/Crop/Tile),
  `StrokeAlign` (Inside/Center/Outside), `Stroke` (width + align +
  solid color), `CornerRadii` (per-corner, `Default` = sharp).
- `PaintEntry` — the paint-table entry: `fill: Option<PaintKind>`
  (`None` = a paint-less, layout-only node), `stroke: Option<Stroke>`,
  `corners: CornerRadii`, `clip: bool`; `PaintEntry::solid(Color)` is
  the v0.1 shorthand. See `docs/decisions/paint-entry-composition.md`.
- `PaintTable` — a dense entry list behind a private field, indexed by
  `RectEntry.paint`: `new`, `push(&mut self, PaintEntry) -> u32`
  (returns the sequential index just assigned), `get(&self, u32) ->
  Option<&PaintEntry>`, `resolve(&self, u32) -> &PaintEntry` (the
  lookup painters use — panics on an out-of-range index), `len`,
  `is_empty`.
- `Painter` — the one trait every paint backend implements:
  `fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable)`.

`Color` and `RectEntry` are `#[repr(C)]` because `specs/DESIGN_1.md`
§7.3 calls rect entries blittable and R-T4 plans dirty-range
instance-buffer uploads of per-frame painter input; fixing the layout
now costs nothing.

`Painter::paint` is infallible and the trait is object-safe (`Box<dyn
Painter>` must work — backend selection is whole-scene, R3). Slice order
defines stacking — a later entry composites over an earlier one, since
DFS order encodes document stacking. The composited result is the
contract; iteration order is the implementation's choice (the lean
painter draws opaque cores front-to-back, `specs/DESIGN_1.md` §9 R-T2).
An out-of-range paint index is a broken contract between crates;
`PaintTable::resolve` centralizes the panic for that case, so no painter
invents its own failure path (a silent skip would be the silent drop P4
forbids). See `docs/decisions/painter-trait-infallible-slice-input.md`
for the alternatives considered on the trait's signature.

## Testing

`crates/dashpaint/tests/boundary_b.rs` exercises the public API only,
against hand-built fixtures, with no `dashscene-core` dependency. It
covers the `PaintTable` indexing contract (including the `resolve`
panic), the `PaintEntry` composition (solid shorthand, paint-less
entry, full gradient+stroke+corners+clip entry, image fill), the
recorded output of a `RecordingPainter` test double over a two-rect
fixture, and dyn-dispatch through `&mut dyn Painter`. The test file is
the executable statement of the boundary-B contract; this section
deliberately does not restate its cases.

## Open for story #14

An image fill references its asset by index (`image: u32`), matching
`dashbuf`'s `Document.images`. How decoded pixel data reaches a painter
— the asset store crossing boundary B — is deliberately unresolved here
and lands with the painter work in #14.

## Trace

- Satisfies: `specs/DESIGN_1.md` §8 painter trait (boundary B), §7.3
  output shape, §11 v0.3 paint vocabulary (from the §10.1 NOW list);
  issue #3 and #13 acceptance
  criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness), #14 (v0.3 painting).
- Related decisions: `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`.
