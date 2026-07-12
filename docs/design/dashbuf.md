# dashbuf — the .dsb document schema

    crate    crates/dashbuf
    covers   v0.1 walking skeleton + v0.3 paint vocabulary (story #13)

## Purpose

`dashbuf` owns the `.dsb` document format: the FlatBuffers schema that is
the one intermediate representation between producers (Figma import,
`dashlang`) and the runtime (`specs/DESIGN_1.md` §5). It is boundary A —
the `.dsb` load gate (`CONTRIBUTING.md` "Crate ownership and scope").
Principle P1 (`AGENTS.md`) holds throughout: the document carries intent,
never results — no resolved x/y/w/h, no rasterized pixels, no glyph
positions.

The schema lives in `crates/dashbuf/schema/dashbuf.fbs`; `build.rs` shells
out to flatc at build time and generates the Rust bindings, which
`src/lib.rs` re-exports from a clippy-exempted generated submodule.

Evolution is append-only: existing field ids keep their positions, and new
fields are appended (R7). Every extension — v0.2's layout modes, v0.3's
paint vocabulary below — is additive, not a rewrite; a reader built
against an older schema version keeps working unchanged.

## Schema shape

- `Document` is the `root_type`: `nodes: [Node]` plus `images: [Image]`
  (v0.3).
- `Node`s are stored as a flattened DFS array — array index doubles as
  the rect-table index consumed at boundary B (`specs/DESIGN_1.md` §5).
  `Node.parent` is an index into that same array, or the `uint32::MAX`
  sentinel for a root node.
- `Node.layout: FixedSizeLayout` is the v0.1 layout mode (width/height
  only; no Taffy yet — `dashscene-engine`'s solve lands at v0.2).
- Paint is split across two generations on `Node`, both present at once
  by design: `paint: SolidFill` is the v0.1 walking-skeleton shorthand;
  `fill: Fill` (the v0.3 union), `stroke: Stroke`, `corners: CornerRadii`,
  and `clip: bool` are the v0.3 vocabulary. When `fill` is present it
  supersedes `paint`. See
  `docs/decisions/fill-union-keeps-legacy-paint-field.md` for why both
  fields exist and the condition under which `paint` is removed.
- `Document.images: [Image]` holds embedded encoded image bytes;
  `ImageFill.image` indexes into it. Decoded pixel data crossing boundary
  B is out of scope here — see "Open for story #14" below.

## Public interface

All types are generated from `crates/dashbuf/schema/dashbuf.fbs`:

- `FixedSizeLayout` — `width`, `height: float32` (v0.1 layout mode).
- `Color` — `r`, `g`, `b`, `a: float32`; the same shape `dashpaint`
  reproduces as a plain Rust type (`docs/decisions/dashpaint-owns-boundary-b-types.md`).
- `SolidFill` — `color: Color`; the legacy `Node.paint` shorthand.
- `Vec2` — `x`, `y: float32`; gradient handle positions.
- `GradientStop` — `offset: float32` (normalized 0..1), `color: Color`.
- `CornerRadii` — `top_left`, `top_right`, `bottom_right`,
  `bottom_left: float32`; all zero = sharp corners.
- `GradientKind` (`uint8` enum) — `Linear`, `Radial`, `Angular`,
  `Diamond`.
- `StrokeAlign` (`uint8` enum) — `Inside`, `Center`, `Outside`.
- `ScaleMode` (`uint8` enum) — `Fill`, `Fit`, `Crop`, `Tile` (Figma
  image-fill scale modes).
- `ImageFormat` (`uint8` enum) — `Png` (only format for now).
- `Gradient` (table) — `kind: GradientKind`, `handle_origin`,
  `handle_primary`, `handle_secondary: Vec2`, `stops: [GradientStop]`.
  One geometry model (three normalized handle positions) serves all four
  gradient kinds — Figma's own `gradientHandlePositions` model; see
  `docs/decisions/paint-entry-composition.md`'s sub-decisions.
- `ImageFill` (table) — `image: uint32` (index into `Document.images`),
  `scale_mode: ScaleMode`.
- `Fill` (union) — `SolidFill | Gradient | ImageFill`.
- `Stroke` (table) — `width: float32`, `align: StrokeAlign`,
  `color: Color`; solid-only at v0.3.
- `Image` (table) — `format: ImageFormat`, `bytes: [ubyte]`.
- `Node` (table) — `name: string`, `parent: uint32` (`uint32::MAX`
  sentinel for roots), `layout: FixedSizeLayout`, `paint: SolidFill`
  (legacy), `fill: Fill`, `stroke: Stroke`, `corners: CornerRadii`,
  `clip: bool = false`.
- `Document` (table, `root_type`) — `nodes: [Node]`, `images: [Image]`.

## Testing

`crates/dashbuf/tests/roundtrip.rs` covers the v0.1 baseline (exit
criterion E6, `specs/DESIGN_1.md` §11): a document built in memory
survives a flatbuffer round trip byte-for-byte-equivalent in its decoded
fields, including the root-node parent sentinel.

`crates/dashbuf/tests/paint_roundtrip.rs` covers the v0.3 vocabulary, one
focused test per construct: every gradient kind's discriminant, handles,
and stops; every image scale mode plus `Document.images`; every stroke
align; corner radii and clip (present and absent-defaults); and the
`Fill` union's discrimination alongside the legacy `paint` field reading
back unchanged. The test file is the executable statement of the
schema's v0.3 contract; this section deliberately does not restate its
cases.

## Open for story #14

`ImageFill.image` names an asset by index into `Document.images`, which
stores embedded encoded bytes. How decoded pixel data reaches a painter —
the asset store crossing boundary B — is deliberately unresolved here and
lands with the painter work in #14 (see also
`docs/design/dashpaint.md`'s matching note).

## Trace

- Satisfies: `specs/DESIGN_1.md` §5 document format, §10.1 NOW paint
  vocabulary, §11 v0.1 and v0.3 slices, R7 additive schema evolution;
  issue #13 acceptance criteria.
- Blocks: `dashscene-core` lowering, `dashc`'s importer consumption
  (out of scope until later slices).
- Related decisions: `docs/decisions/fill-union-keeps-legacy-paint-field.md`,
  `docs/decisions/paint-entry-composition.md`.
