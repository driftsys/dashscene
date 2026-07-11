# dashpaint — boundary B

    crate    crates/dashpaint
    covers   v0.1 walking skeleton (story #3)

## Purpose

`dashpaint` defines boundary B (`specs/DESIGN_1.md` §4, §7.3, §8): the
complete input a painter consumes, and the trait every painter implements.
Principle P2 (`AGENTS.md`) holds throughout — a painter only colors; it
never measures, wraps, kerns, or moves anything.

For v0.1, boundary B is a rect table plus a paint table with one paint
kind (solid fill). The crate has no dependencies, including no
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
- `PaintKind` — enum, one variant for v0.1: `Solid { color: Color }`.
- `PaintTable` — a dense paint list behind a private field, indexed by
  `RectEntry.paint`: `new`, `push(&mut self, PaintKind) -> u32`
  (returns the sequential index just assigned), `get(&self, u32) ->
  Option<&PaintKind>`, `len`, `is_empty`.
- `Painter` — the one trait every paint backend implements:
  `fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable)`.

`Color` and `RectEntry` are `#[repr(C)]` because rect/paint data is
blittable by design (`specs/DESIGN_1.md` §7.3) and R-T4 plans
dirty-range instance-buffer uploads straight from the rect table; fixing
the layout now costs nothing.

`Painter::paint` is infallible and the trait is object-safe (`Box<dyn
Painter>` must work — backend selection is whole-scene, R3). Slice order
is paint order, back-to-front: DFS order already encodes document
stacking for v0.1's fixed-rect scenes. See
`docs/decisions/painter-trait-infallible-slice-input.md` for the
alternatives considered on the trait's signature.

## Testing

`crates/dashpaint/tests/boundary_b.rs` exercises the public API only,
against hand-built fixtures, with no `dashscene-core` dependency:

- `PaintTable` push/get: sequential indices starting at 0, resolution
  by index, and past-the-end lookups returning `None`.
- A `RecordingPainter` test double implements `Painter`, resolves each
  rect's paint index, and records `(RectEntry, Color)` pairs; a
  two-rect, two-paint fixture asserts the recorded output — order,
  geometry, and resolved colors.
- The same fixture driven through `Box<dyn Painter>` proves object
  safety.

## Trace

- Satisfies: `specs/DESIGN_1.md` §8 painter trait (boundary B), §7.3
  output shape; issue #3 acceptance criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness).
- Related decisions: `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`.
