# dashbuf — the .dsb document schema

    crate    crates/dashbuf
    covers   v0.1 walking skeleton + v0.2 flex layout vocabulary
             (story #8) + v0.3 paint vocabulary (story #13)
             + v0.5 text vocabulary (story #26)

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

- `Document` is the `root_type`: `nodes: [Node]` plus the v0.3
  `paints: [Paint]` pool and `images: [Image]` assets.
- `Node`s are stored as a flattened DFS array — array index doubles as
  the rect-table index consumed at boundary B (`specs/DESIGN_1.md` §5).
  `Node.parent` is an index into that same array, or the `uint32::MAX`
  sentinel for a root node.
- `Node.layout: FixedSizeLayout` carries the authored x/y offset plus
  width/height — the datum `Fixed` sizing uses, and where the offset
  applies under a `mode = None` parent. Since v0.2 (story #8),
  `Node.flex: LayoutContainer` and `Node.constraints: LayoutConstraints`
  are two additional optional tables carrying the flex-layout
  vocabulary (mode NONE/H/V, per-axis hug/fill/fixed sizing, gap,
  padding, alignment, min/max) as stored intent — no Taffy yet, the
  solve consuming it is story #9
  (`docs/decisions/flex-vocabulary-shape.md`).
- Paint is split across two generations, both present at once by
  design: `Node.paint: SolidFill` is the v0.1 walking-skeleton inline
  shorthand; the v0.3 vocabulary lives in the document-level dedup pool
  `Document.paints: [Paint]` (DESIGN §5's "dedup style pool"),
  referenced by `Node.paint_entry: uint32` — `uint32::MAX` (NO_PAINT)
  means the node draws nothing, the same sentinel convention as
  `Node.parent`. The sentinel is document-level only: the committed
  runtime output has no sentinel since story #4 — every rect resolves
  to a pool entry (`docs/decisions/boundary-b-unification.md`). When
  `paint_entry` is set it supersedes `paint`. See
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md` for
  why both exist and the condition under which `paint` is removed.
- `Document.images: [Image]` holds embedded encoded image bytes;
  `ImageFill.image` indexes into it. Decoded pixel data crossing boundary
  B is out of scope here — see "Open for story #14" below.
- Text (v0.5, story #26) mirrors the same dedup-pool pattern as paint —
  DESIGN §5's document table lists a `text` row: "strings + style
  refs", never glyph positions. `Document.strings: [string]` is an
  interned string pool; `Document.text_styles: [TextStyle]` is a dedup
  style pool. `Node.text` and `Node.text_style: uint32 = uint32::MAX`
  are sentinel-indexed references into the two pools — the same
  `uint32::MAX` "absent" convention as `Node.parent` and
  `paint_entry`. Dedup is the producer's job (the pools make it
  representable; nothing in the schema forces it), the same posture as
  `Document.paints`. Two alternatives were rejected: an inline
  `Node.text: string` field works today, but retrofitting §5's
  interning later would leave a dead field — append-only evolution
  (R7) forbids repurposing or removing it, so the pool costs one
  indirection now instead of a second text field later. Text as a
  node-kind union was also rejected — it would restructure `Node` for
  no v0.5 gain, and §5 already models text as node content (strings +
  style refs), not a parallel node array.

## Public interface

All types are generated from `crates/dashbuf/schema/dashbuf.fbs`:

- `FixedSizeLayout` — `x`, `y`, `width`, `height: float32` (x/y are
  the authored offset relative to the parent; width/height double as
  the datum `Fixed` sizing uses under v0.2 flex).
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
  `handle_primary`, `handle_secondary: Vec2`, `stops: [GradientStop]`;
  handles and stops are `(required)`, so the flatbuffer verifier itself
  rejects a geometry-less gradient at the load gate (P4). One geometry
  model (three named normalized handle positions) serves all four
  gradient kinds — Figma's own `gradientHandlePositions` model; see
  `docs/decisions/paint-entry-composition.md`'s sub-decisions.
- `ImageFill` (table) — `image: uint32` (index into `Document.images`),
  `scale_mode: ScaleMode`, `transform: Mat23` (normalized image-space
  crop transform, Figma's imageTransform; identity when absent),
  `tile_scale: float32 = 1.0` (Figma's scalingFactor; story #14).
- `Mat23` (struct) — row-major 2×3 affine transform.
- `Fill` (union) — `SolidFill | Gradient | ImageFill`.
- `Stroke` (table) — `width: float32`, `align: StrokeAlign`,
  `color: Color (required)`; solid-only at v0.3.
- `Image` (table) — `format: ImageFormat`, `bytes: [ubyte]`.
- `Paint` (table) — one pool entry: `fill: Fill`, `stroke: Stroke`,
  `corners: CornerRadii`, `clip: bool = false`.
- `TextStyle` (table) — one text-style-pool entry: `family: string
  (required)` (font family name; the verifier rejects a family-less
  style at the load gate — P4, the same mechanism as `Gradient`'s
  required fields; a pool-indexed family was considered and rejected,
  since it would lose that verifier-enforced presence check and
  families repeat little once styles themselves are pooled),
  `size: float32` (em size in document units), `weight: ushort =
  400` (CSS-scale weight, 100 to 900 inclusive), `color: Color`.
- `LayoutMode` (`uint8` enum) — `None`, `Horizontal`, `Vertical`;
  Wrap and Grid append at v0.8.
- `AxisSizing` (`uint8` enum) — `Fixed`, `Hug`, `Fill`.
- `MainAxisAlign` (`uint8` enum) — `Start`, `Center`, `End`,
  `SpaceBetween`.
- `CrossAxisAlign` (`uint8` enum) — `Start`, `Center`, `End`;
  `Baseline` appends at v0.8 (Q-4).
- `EdgeInsets` — `left`, `top`, `right`, `bottom: float32`.
- `LayoutContainer` (table) — container-side v0.2 flex properties:
  `mode: LayoutMode`, `gap: float32`, `padding: EdgeInsets`,
  `main_align: MainAxisAlign`, `cross_align: CrossAxisAlign`.
- `LayoutConstraints` (table) — child-side v0.2 flex properties, valid
  on any node: `sizing_h`, `sizing_v: AxisSizing`, `min_width`,
  `max_width`, `min_height`, `max_height: float32 = null` (absent =
  unconstrained). Full rationale for the two-table split:
  `docs/decisions/flex-vocabulary-shape.md`.
- `Node` (table) — `name: string`, `parent: uint32` (`uint32::MAX`
  sentinel for roots), `layout: FixedSizeLayout`, `paint: SolidFill`
  (legacy), `paint_entry: uint32 = uint32::MAX` (the document-level
  NO_PAINT sentinel; index into `Document.paints`), `text: uint32 =
  uint32::MAX` (index into `Document.strings`, or the sentinel for a
  node without text), `text_style: uint32 = uint32::MAX` (index into
  `Document.text_styles`, or the sentinel for unstyled text — a
  diagnostic once text validation exists, never a silent default),
  `flex: LayoutContainer`, `constraints: LayoutConstraints` (both
  optional; absent = mode `None` / fully default constraints).
- `Document` (table, `root_type`) — `nodes: [Node]`, `images: [Image]`,
  `paints: [Paint]`, `strings: [string]`, `text_styles: [TextStyle]`.

## Testing

`crates/dashbuf/tests/roundtrip.rs` covers the v0.1 baseline (exit
criterion E6, `specs/DESIGN_1.md` §11): a document built in memory
survives a flatbuffer round trip byte-for-byte-equivalent in its decoded
fields, including the root-node parent sentinel. It also covers the
v0.2 flex vocabulary (story #8): a node carrying every `LayoutContainer`
and `LayoutConstraints` field round-trips field-for-field, and a node
without either table reads back absent.

`crates/dashbuf/tests/paint_roundtrip.rs` covers the v0.3 vocabulary
through the paint pool, one focused test per construct: every gradient
kind (iterating the generated `ENUM_VALUES`, so a future kind cannot be
silently missed), every image scale mode against a non-default asset
index, every stroke align, corner radii and clip, absent-field
defaults including the `NO_PAINT` sentinel, two nodes sharing one pool
entry, and the pooled fill coexisting with the legacy `paint` field.
The test file is the executable statement of the schema's v0.3
contract; this section deliberately does not restate its cases.

`crates/dashbuf/tests/text_roundtrip.rs` covers the v0.5 text
vocabulary (story #26), one focused test per construct, matching
`paint_roundtrip.rs`'s style: a text node reading back through both
pools, two nodes sharing one interned string index, the no-text/
no-style sentinels as defaults, and `weight`'s 400 default. A
family-less `TextStyle` cannot be constructed through the generated
safe API at all — `TextStyleArgs.family: None` panics in `create` for
a required field — so the verifier-rejection property is enforced at
build time for Rust producers and by the flatbuffer verifier for
foreign bytes; no test constructs invalid bytes by hand.

## Seams to later stories

- **#28** (Latin shaping) and **#29** (measure callback / hug sizing)
  read the string and style pools through `dashscene-core`'s
  intent-side accessors (`docs/design/dashscene-core-arena.md`); this
  schema defines what text a node contains, never how it is shaped or
  measured (P1).
- **#30** (glyph-run committed output and painting) is where boundary
  B gains positioned glyph runs; nothing in this schema anticipates
  that shape.
- **#34** (charset coverage) and the validator's future text
  diagnostics (an unstyled `text_style` sentinel on a node that has
  text, a family the atlas pipeline's charset does not cover) are out
  of scope for this schema change.

The asset path across boundary B is resolved since story #14: painters
receive the encoded, format-tagged assets as a `dashpaint::ImageTable`
(`docs/decisions/image-assets-cross-boundary-b.md`).

## Trace

- Satisfies: `specs/DESIGN_1.md` §5 document format (including the
  dedup style pool and the text row — strings + style refs), §11 v0.1,
  v0.2, v0.3, and v0.5 (text I) slices (v0.2 vocabulary is R2; v0.3
  vocabulary drawn from the §10.1 NOW list), R7 additive schema
  evolution; issue #8, issue #13, and issue #26 acceptance criteria.
- Blocks: `dashscene-core` lowering, `dashc`'s importer consumption
  (out of scope until later slices); #28's typeset consumption of the
  string and style pools. The story #9 Taffy solve consumes
  `dashscene-core`'s mirrored intent (`Arena::layout`), not these
  tables directly — nothing outside `dashbuf` links the generated
  code until a `.dsb` load path exists (v0.3+); see
  `docs/decisions/flex-vocabulary-shape.md`.
- Related decisions: `docs/decisions/flex-vocabulary-shape.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/text-track-early-start.md` (plan sequencing for
  issue #26).
