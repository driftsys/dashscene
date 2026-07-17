# dashbuf — the .dsb document schema

    crate    crates/dashbuf
    covers   v0.1 walking skeleton + v0.2 flex layout vocabulary
             (story #8) + v0.3 paint vocabulary (story #13)
             + v0.5 text vocabulary (story #26) + v0.4 variant table
             (story #20) + v0.7 binding tables (story #167)
             + v0.8 layout fidelity — wrap/grid modes, grid tracks and
             placement, baseline, cross gap (story #43)

## Purpose

`dashbuf` owns the `.dsb` document format: the FlatBuffers schema that is
the one intermediate representation between producers (Figma import,
`dashlang`) and the runtime (`docs/archive/2026-07-14-design-1-seed.md`
§5). It is boundary A —
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
against an older schema version keeps working unchanged. That discipline
is enforced by a frozen byte fixture, not by convention alone — see
"Testing" below and
`docs/decisions/dsb-frozen-fixture-r7-guard.md`.

## Schema shape

- `Document` is the `root_type`: `nodes: [Node]` plus the v0.3
  `paints: [Paint]` pool and `images: [Image]` assets.
- `Node`s are stored as a flattened DFS array — array index doubles as
  the rect-table index consumed at boundary B
  (`docs/archive/2026-07-14-design-1-seed.md` §5).
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
  `Document.paints: [Paint]` (`docs/archive/2026-07-14-design-1-seed.md`
  §5's "dedup style pool"),
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
  `docs/archive/2026-07-14-design-1-seed.md` §5's document table lists
  a `text` row: "strings + style
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
- The variant table (v0.4, story #20) mirrors §5's row for it verbatim:
  "sparse per-variant overrides, never duplicate trees." `Document.variant_sets:
  [VariantSet]` is a flat pool of independently-switchable groups —
  Figma's "component SET" (`docs/technotes/glossary.md`) — each holding
  a `members: [VariantMember]` list and an `active_member: uint32 = 0`
  selecting which one applies before any runtime `set_variant` call. A
  member carries `name: string` plus `overrides: [VariantOverride]`,
  sparse by construction: only the props that differ from the
  document's base `Node` values need an entry. One override is `node:
  uint32` (an index into `Document.nodes`, the same convention as
  `Node.parent`) plus a `VariantPropValue` union naming which prop and
  its value — `VariantX`/`VariantY`/`VariantWidth`/`VariantHeight`
  (each one `{ value: float32 }`) or `VariantFill` (`{ color: Color
  (required) }`, required for the same reason `Stroke.color` is). This
  is the narrowest slice of `dashscene-core`'s `Prop` vocabulary that
  proves resolved rect/paint correctness, not the full vocabulary —
  widening it is additive future work
  (`docs/decisions/variant-set-flat-index.md`, which also records why
  selection is a flat member index rather than axis-keyed).

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
- `LayoutMode` (`uint8` enum) — `None`, `Horizontal`, `Vertical`, and
  since v0.8 (story #43) `Wrap` (a horizontal wrapping row — Figma's
  `layoutWrap` exists for horizontal auto-layout only) and `Grid`.
- `AxisSizing` (`uint8` enum) — `Fixed`, `Hug`, `Fill`.
- `MainAxisAlign` (`uint8` enum) — `Start`, `Center`, `End`,
  `SpaceBetween`.
- `CrossAxisAlign` (`uint8` enum) — `Start`, `Center`, `End`, and
  since v0.8 `Baseline` (Q-4 — resolved,
  `docs/technotes/open-questions.md`).
- `EdgeInsets` — `left`, `top`, `right`, `bottom: float32`.
- `GridTrackSizing` (`uint8` enum, v0.8) — `Fixed` (a document-unit
  length), `Fraction` (a flexible weight, Figma's `minmax(0, Nfr)`).
- `GridTrack` (table, v0.8) — one grid row or column track:
  `sizing: GridTrackSizing`, `value: float32`. A table, not a struct,
  so a future bound appends as a field
  (`docs/decisions/v08-layout-vocabulary-shape.md` D2).
- `LayoutContainer` (table) — container-side flex properties:
  `mode: LayoutMode`, `gap: float32`, `padding: EdgeInsets`,
  `main_align: MainAxisAlign`, `cross_align: CrossAxisAlign`, plus the
  v0.8 appends `cross_gap: float32 = null` (wrap-line / grid-row
  spacing; absent = follows `gap`, preserving the v0.2 both-axes
  mapping) and `grid_rows`/`grid_columns: [GridTrack]` (absent under
  mode `Grid` = implicit auto tracks).
- `LayoutConstraints` (table) — child-side v0.2 flex properties, valid
  on any node: `sizing_h`, `sizing_v: AxisSizing`, `min_width`,
  `max_width`, `min_height`, `max_height: float32 = null` (absent =
  unconstrained), and `margin: EdgeInsets` (story #10; absent = zero
  insets, negative values legal — the negative-gap lowering target,
  `docs/decisions/negative-gap-lowering.md`), plus the v0.8 grid
  placement appends `grid_row`/`grid_column: ushort = null` (0-based
  anchor cell; absent = auto-placed) and
  `grid_row_span`/`grid_column_span: ushort = 1`. Full rationale for
  the two-table split: `docs/decisions/flex-vocabulary-shape.md`.
- `Node` (table) — `name: string`, `parent: uint32` (`uint32::MAX`
  sentinel for roots), `layout: FixedSizeLayout`, `paint: SolidFill`
  (legacy), `paint_entry: uint32 = uint32::MAX` (the document-level
  NO_PAINT sentinel; index into `Document.paints`), `text: uint32 =
  uint32::MAX` (index into `Document.strings`, or the sentinel for a
  node without text), `text_style: uint32 = uint32::MAX` (index into
  `Document.text_styles`, or the sentinel for unstyled text — a
  diagnostic once text validation exists, never a silent default),
  `flex: LayoutContainer`, `constraints: LayoutConstraints` (both
  optional; absent = mode `None` / fully default constraints),
  `opacity: float32 = 1.0`, `mask: bool = false`, `visible: bool = true`
  (v0.8, story #44: group opacity, mask membership, and Figma
  visibility — each default omits from the buffer, so a pre-v0.8 document
  emits unchanged; `docs/decisions/masks-and-group-opacity.md`).
- `VariantX`, `VariantY`, `VariantWidth`, `VariantHeight` (tables) —
  each `{ value: float32 }`; `VariantFill` (table) — `{ color: Color
  (required) }`. The five `VariantPropValue` union members (v0.4).
- `VariantOverride` (table) — `node: uint32` (index into
  `Document.nodes`), `value: VariantPropValue`.
- `VariantMember` (table) — `name: string`, `overrides:
  [VariantOverride]` (sparse: absent means no change from the base
  `Node` values).
- `VariantSet` (table) — `members: [VariantMember]`, `active_member:
  uint32 = 0` (flat index into `members`;
  `docs/decisions/variant-set-flat-index.md`).
- The binding tables (v0.7, story #167,
  `docs/decisions/binding-table-in-the-document.md`): `SignalDecl`
  (table) — `name: string` (the runtime lookup name; absent for an
  anonymous producer signal), `initial: float32` (the value every
  binding of it seeds from — authored intent, never a runtime value,
  P1). `BindingChannel` (`uint8` enum) — the §23 channel set `X`, `Y`,
  `Width`, `Height`, `Gap`, `FillR`, `FillG`, `FillB`, `FillA`,
  `Opacity` (v0.8, story #44, debt #253), mirroring `dashscene-core`'s
  `Channel` wire codes. `TransformScale` /
  `TransformMapRange` / `TransformClamp` (tables) — the three
  `BindingTransform` union members; union `NONE` means the identity
  transform, so the common Figma-authored row costs no transform table.
  `Binding` (table) — `signal: uint32` (index into `Document.signals`),
  `node: uint32` (index into `Document.nodes`), `channel:
  BindingChannel`, `transform: BindingTransform`.
- `Document` (table, `root_type`) — `nodes: [Node]`, `images: [Image]`,
  `paints: [Paint]`, `strings: [string]`, `text_styles: [TextStyle]`,
  `variant_sets: [VariantSet]`, `signals: [SignalDecl]`,
  `bindings: [Binding]`.

## Testing

`crates/dashbuf/tests/roundtrip.rs` covers the v0.1 baseline (exit
criterion E6, `docs/specification/05-qualification.md`): a document built in memory
survives a flatbuffer round trip byte-for-byte-equivalent in its decoded
fields, including the root-node parent sentinel. It also covers the
v0.2 flex vocabulary (story #8): a node carrying every `LayoutContainer`
and `LayoutConstraints` field round-trips field-for-field — since v0.8
(story #43) including the cross gap, both grid track lists, and the
grid placement at non-default values — and a node without either table
reads back absent. A v0.8 test additionally round-trips the appended
enum tail members (`Wrap`, `Grid`, `Baseline`) and pins that the
unwritten v0.8 fields read back absent (spans at their default of 1).

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

`crates/dashbuf/tests/variant_roundtrip.rs` covers the v0.4 variant
table (story #20), matching `paint_roundtrip.rs`'s style: every
`VariantPropValue` union member round-tripping through one override
each targeting a distinct node index, a member's `overrides` reading
back empty when absent, `active_member` reading back both its default
and a set value, and independent round-tripping across multiple
`VariantSet`s in one document.

`crates/dashbuf/tests/bindings_roundtrip.rs` covers the v0.7 binding
tables (story #167), one focused test per construct, matching
`paint_roundtrip.rs`'s style: named and anonymous declarations, every
`BindingChannel` via the generated `ENUM_VALUES`, every
`BindingTransform` union member plus the union-NONE identity default,
two rows sharing one declaration, and absent tables reading back absent.

`crates/dashbuf/tests/schema_evolution.rs` is the R7 guard (debt #64).
The suites above build and decode with the same freshly generated
bindings, so a schema edit that shifts a field id or a union
discriminant — which breaks every `.dsb` already written to disk —
leaves them green. This suite instead decodes
`crates/dashbuf/tests/fixtures/v0_5_document.dsb`, a binary document
checked into the repo and frozen: one document exercising the four
sentinel-defaulted `Node` fields, all three `Fill` union members,
`Paint.clip`, the legacy inline `Node.paint`, both flex tables, both
text pools, (v0.4) one `VariantSet` with a non-default
`active_member` and one override of each of two `VariantPropValue`
kinds, (v0.7) both binding tables — a named and an anonymous
declaration, an identity row on a non-default channel, and a
`TransformScale` row — and (v0.8, story #43) a grid node carrying the
appended layout fields: mode `Grid`, `Baseline` cross alignment, a
cross gap, a `Fixed` and a `Fraction` track per axis at
per-axis-distinct values, and a non-default anchor and span per axis —
every field written to a value distinguishable from its default. The assertions are on those values — a shifted field id
usually still decodes, and quietly returns another field's value or a
default, which is the failure worth catching.

The fixture is rewritten only under `UPDATE_DSB_FIXTURE=1 cargo test -p
dashbuf --test schema_evolution` (the same environment-gate posture as
`goldens/`' `UPDATE_GOLDENS=1`), and only on a deliberate, reviewed
format-generation bump — never to make the suite go green. A slice that
adds schema fields adds them to the fixture in the same commit, which is
an append and therefore legitimate.
`docs/decisions/dsb-frozen-fixture-r7-guard.md` has the rationale.

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

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §5 document
  format (including the dedup style pool, the text row — strings +
  style refs, and the variant row — sparse per-variant overrides, never
  duplicate trees), `docs/roadmap.md`'s v0.1, v0.2, v0.3, v0.4, v0.5
  (text I), and v0.8 (layout fidelity) slices (v0.2 vocabulary is R2;
  v0.3 vocabulary drawn from
  `docs/specification/04-figma-vocabulary-profile.md`'s NOW list), R7
  additive schema evolution; issue #8, issue #13, issue #20, issue #26,
  and issue #43 acceptance criteria.
- Blocks: `dashscene-core` lowering, `dashc`'s importer consumption
  (out of scope until later slices); #28's typeset consumption of the
  string and style pools. The story #9 Taffy solve consumes
  `dashscene-core`'s mirrored intent (`Arena::layout`), not these
  tables directly — nothing outside `dashbuf` links the generated
  code until a `.dsb` load path exists (v0.3+); see
  `docs/decisions/flex-vocabulary-shape.md`.
- Related decisions: `docs/decisions/dsb-frozen-fixture-r7-guard.md`,
  `docs/decisions/v08-layout-vocabulary-shape.md` (the v0.8 appends),
  `docs/decisions/flex-vocabulary-shape.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/text-track-early-start.md` (plan sequencing for
  issue #26), `docs/decisions/variant-set-flat-index.md` (issue #20's
  selection shape and overridable-prop vocabulary).
