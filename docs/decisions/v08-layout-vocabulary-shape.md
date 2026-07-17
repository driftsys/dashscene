# The v0.8 layout vocabulary: wrap and grid as modes, tracks as tables, one cross gap

    status   accepted (story #43, 2026-07-17)
    scope    crates/dashbuf (schema), crates/dashscene-core (mirror),
             crates/dashscene-engine (mapping)
    binds    #264 (the dashc un-pin lowers Figma onto exactly these
             fields), #44/#45 (their schema appends land after this one),
             #46 (the stress corpus authors this vocabulary)

## Context

Story #43 teaches the solver wrap mode, grid with spans, and baseline
counter-axis alignment, and appends their vocabulary to the `.dsb`
schema (append-only, R7 — the frozen fixture regenerated in the same
change, the legitimate append case). The captured fixtures pin what the
vocabulary must express, because story #264 must lower them onto these
fields:

- `grid-basic.json`: `layoutMode: GRID` with **per-track sizing**
  (`gridColumnsSizing: "160px minmax(0,1fr) minmax(0,1fr)"`), separate
  `gridRowGap`/`gridColumnGap`, and per-child
  `gridRowAnchorIndex`/`gridColumnAnchorIndex` +
  `gridRowSpan`/`gridColumnSpan`.
- `lowering-wrap.json`: `layoutWrap: WRAP` on a horizontal auto-layout
  frame with `itemSpacing` (main) and `counterAxisSpacing` (cross) —
  two gaps.
- `lowering-baseline.json`: `counterAxisAlignItems: BASELINE` on a
  horizontal row.

## Decisions

### D1 — Wrap and Grid are `LayoutMode` members

`LayoutMode` appends `Wrap = 3` and `Grid = 4` — the placement the v0.2
schema comments reserved. `Wrap` is a horizontal wrapping row: Figma's
`layoutWrap` exists for horizontal auto-layout only, so a
wrap-direction field would encode a construct no producer can author. A
separate `wrap: bool` beside the mode was rejected for the same reason:
it would make `Vertical + wrap` representable.

### D2 — grid tracks are vectors of `GridTrack` tables

`LayoutContainer` appends `grid_rows: [GridTrack]` and
`grid_columns: [GridTrack]`; `GridTrack` is a **table**
(`sizing: GridTrackSizing`, `value: float32`) with
`GridTrackSizing : uint8 { Fixed, Fraction }`. `Fixed` is a
document-unit length; `Fraction` is a flexible weight the engine lowers
as `minmax(0, Nfr)` — Figma's own serialized track form, whose zero
minimum (unlike bare `fr`'s min-content one) divides exactly the free
space the captured grid divides.

Rejected alternatives: plain track **counts** (`grid_rows: ushort`)
with equal tracks cannot express the captured 160 px first column, so
story #264 could not lower `grid-basic.json` against it; a **struct**
track can never gain fields under R7, while a table lets a future
minmax bound or named line append instead of forcing a parallel
vocabulary.

### D3 — placement lives on `LayoutConstraints`

`grid_row`/`grid_column: ushort = null` (the 0-based anchor cell;
absent = auto-placed in document order) and
`grid_row_span`/`grid_column_span: ushort = 1` append to
`LayoutConstraints` — the child-side table, matching Figma's per-child
anchor/span fields. A separate placement table on `Node` was rejected:
a table for four scalars with no evolution gain.

### D4 — one appended `cross_gap`, absent means follows-`gap`

`LayoutContainer` appends `cross_gap: float32 = null`: the spacing
between wrap lines and between grid rows, while `gap` stays the
main-axis spacing (for `Wrap` and `Grid` the horizontal one — Figma's
`itemSpacing`/`gridColumnGap`). Absent = follows `gap`, which preserves
the v0.2 both-axes mapping for every existing document byte-for-byte. A
`row_gap`/`column_gap` pair was rejected: R7 forbids repurposing the
existing `gap` field id, so the pair would leave `gap` dead.

### D5 — the engine mapping

- `Wrap` maps as `Horizontal` plus `flex_wrap: Wrap` and
  `align_content: FlexStart` (Figma packs lines; taffy's default
  behaves as stretch and would move lines in a fixed-height container).
- `Grid` maps to `Display::Grid` with the track templates and gaps;
  `main_align`/`cross_align` are not mapped — placement is by cell.
- A grid child's in-cell alignment comes from its sizing intent:
  `Fill` → stretch over the cell area, `Fixed`/`Hug` → its own size at
  the cell origin (`justify_self`/`align_self`) — what the captured
  grid shows; taffy's default would stretch a hug child over its cell.
- `CrossAxisAlign` appends `Baseline = 3` → `AlignItems::Baseline`.
  Q-4's answer (from the taffy source, pinned by test): baselines exist
  for flex **rows**; a leaf's baseline is its bottom edge
  (`height + margin.top` — the measure seam carries no glyph baseline),
  a nested row propagates its first line's baseline, and in a
  `Vertical` container Baseline degrades to start alignment.
- In core, the track lists live **beside** `Layout` in the node data
  (`Arena::grid_tracks`, `Prop::GridRows`/`GridColumns`) — they are
  variable-length and `Layout` is `Copy`, the same split as `text`.
  The placement scalars and `cross_gap` live in `Layout`.
- The negative-gap lowering **refuses a `Wrap` container with a
  negative gap by named panic** (review finding R4): a margin is only
  gap-equivalent for a child that follows another child on the same
  line, and wrap decides its line breaks after the lowering — a lowered
  wrap scene pulls every later line's leading child into the padding
  band and distorts the break points, so there is no margin encoding of
  a negative wrap gap. Story #264 must refuse the Figma-side equivalent
  (`layoutWrap: WRAP` with a negative `itemSpacing`) by name for the
  same reason. The lowering skips `Grid`: grid gaps are track spacing,
  not flex-flow spacing — a leading margin would shift cell content,
  not overlap tracks.
- The engine's placement conversion saturates (review finding R5): a
  ushort anchor becomes a 1-based `i16` line via a checked conversion
  capped at `i16::MAX`, and a zero span floors at 1 — no document value
  can panic or wrap to an end-counted line. The honest diagnosis lives
  at the load gate (D6).

### D6 — the load gate ranges the grid vocabulary (findings R5-R7)

`dashscene-validator`'s load gate checks the new numeric domains in the
same posture as `weight` and stroke width:

- `grid.track-invalid-value` — a `Fixed` track value must be finite and
  non-negative; a `Fraction` weight finite and positive.
- `grid.span-zero` — a span of 0 spans no tracks and has no meaning.
- `grid.anchor-out-of-range` — an anchor must sit inside its parent's
  declared track list on that axis; with no declared list the bound is
  32766, the largest 0-based anchor whose 1-based line index fits the
  solver's `i16` lines.
- `grid.span-out-of-range` (story #264, D7) — an anchor plus its span
  must not run past the declared track count on that axis. The anchor
  alone can fit while `anchor + span` overruns; the engine would then
  grow implicit auto tracks and solve differently from the authored
  grid, so the overrun is diagnosed by name rather than solved silently.
- `grid.fraction-track-under-hug` — a `Fraction` track on an axis the
  grid container hugs is diagnosed by name (finding R7): a fraction
  divides free space, a hug axis has none, and the track (and
  everything anchored to it) would silently collapse to zero. No
  defensible defined behavior exists short of re-specifying hug-grid
  sizing, so the honest P4 move is the refusal.

Grid placement (anchor, span, track domains) is validated at the load
gate, **not** at the dashc Figma walk that gives every other refusal the
source Figma node path (story #264, D10). Two reasons: the check then
covers every producer that writes the schema — `dashlang` and a future
producer, not only the Figma lowering — and it sits in P4 parity with
the other numeric-domain ranges (weight, stroke width), which the engine
saturates rather than panics on, so the honest diagnosis lives at the
gate. The tradeoff is that a placement diagnostic names the `.dsb` node
index and path, not the source Figma layer; the track-token and
negative-gap refusals, which are Figma-vocabulary shapes with no schema
counterpart, stay at the walk where they carry the Figma path.

## Open question — a fixed child larger than its fraction cell (R8)

A `Fraction` track is `minmax(0, fr)`: it divides free space and never
grows for content, so a fixed child larger than its cell keeps its
authored size and **overflows into the neighbor cell** (pinned by
`crates/dashscene-engine/tests/solve.rs::a_fixed_child_larger_than_its_fraction_cell_overflows_it`).
Figma's reference behavior for this combination is uncaptured — none of
`grid-basic.json`'s fixed children exceed their cells. Revisit at
story #264 when real grid captures exist; if Figma grows the track
instead, the track lowering changes (for example to `minmax(auto,
fr)`), which is an engine-mapping change, not a schema change.

## Out of scope, appendable later

Wrap's `counterAxisAlignContent: SPACE_BETWEEN` (no capture pins it;
an `align_content` field appends when a real file needs it — until
then #264 refuses it by name, P4), per-track minmax bounds, and named
grid lines.

## Trace

- Satisfies: story #43 (engine + schema half of the v0.8 layout
  fidelity); R2 via `docs/decisions/flex-vocabulary-shape.md`; R7
  (append-only, fixture regenerated in the same change); resolves Q-4
  (`docs/technotes/open-questions.md`).
- Verified by: `crates/dashscene-engine/tests/solve.rs` — wrap and grid
  spans against the captured fixtures' boxes (`lowering-wrap.json`,
  `grid-basic.json`), baseline against a hand-built mixed-size scene
  (replaying the captured `lowering-baseline.json` is blocked on the
  text-baseline debt candidate: its children are text nodes, and the
  measure seam carries no glyph baseline); `crates/dashbuf/tests/roundtrip.rs`
  and `crates/dashbuf/tests/schema_evolution.rs` (the appended fields at
  non-default values); `crates/dashscene-core/tests/load.rs`
  (the load replay); `crates/dashscene-validator/tests/document.rs`
  (the D6 rules); `goldens/tooling/tests/v08_fidelity.rs`
  (one exact-compare golden per construct).
- Related: `docs/decisions/figma-flex-lowering.md` D5 (the refusals
  #264 un-pins into these fields),
  `docs/decisions/negative-margin-hug-rebate.md` (the #236 fix in the
  same style mapping),
  `docs/decisions/v02-flex-goldens-per-construct.md` (the golden rule
  these scenes extend).
