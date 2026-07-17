# Story #264 — dashc: un-pin the GRID/WRAP/BASELINE refusals

Working memory (spec + plan). Gardened into `docs/design/dashc.md`,
`docs/specification/06-dashc-figma-lowering.md`, and
`docs/decisions/figma-flex-lowering.md` (D5 edited in place) before the PR;
this file then moves to `docs/archive/`.

## Goal

`docs/decisions/figma-flex-lowering.md` D5 refuses `GRID`,
`layoutWrap: WRAP`, and `counterAxisAlignItems: BASELINE` by name in the
dashc Figma lowering "until v0.8". Story #43 taught the engine those
constructs and appended their vocabulary to the `.dsb` schema
(`docs/decisions/v08-layout-vocabulary-shape.md`). This story lowers the
three Figma constructs onto #43's schema fields and removes the three
refusals, without weakening any other named refusal (P4).

## Success criteria (verifiable)

1. `grid-basic` lowers `GRID` onto `LayoutMode::Grid` with track lists
   (from `gridColumnsSizing`/`gridRowsSizing`), gaps
   (`gap = gridColumnGap`, `cross_gap = gridRowGap`), and per-child
   placement (`grid_row`/`grid_column` from the anchor indices,
   `grid_row_span`/`grid_column_span` from the spans). A derivation with
   its one `TEXT` leaf retyped to a fixed `FRAME` (the font-free
   solve-fidelity pattern already used for `lowering-hug-in-fill`) solves
   through the engine to the captured `absoluteBoundingBox` of every node.
2. `lowering-wrap` lowers `layoutWrap: WRAP` onto `LayoutMode::Wrap` with
   `gap = itemSpacing`, `cross_gap = counterAxisSpacing`. The raw capture
   (fixed-size chips, no text) solves to the captured rects.
3. `lowering-baseline` lowers `counterAxisAlignItems: BASELINE` onto
   `CrossAxisAlign::Baseline` and compiles end to end. Its solved rects
   diverge from the capture (debt #273: the engine's leaf baseline is the
   box bottom, not the glyph baseline, and this test binary wires no
   typesetter), so the test pins the lowered intent — not a false rect
   match — and names the divergence.
4. Two new named refusals (P4): `counterAxisAlignContent: SPACE_BETWEEN`
   on a wrap frame, and `layoutWrap: WRAP` combined with a negative
   `itemSpacing` (mirrors the engine's `Txn::lower_negative_gaps` refusal,
   `docs/decisions/v08-layout-vocabulary-shape.md` D5). One test each.
5. Every construct still outside the widened vocabulary keeps its named
   refusal — the refusal tests are extended, never weakened.
6. `just build` and `just wasm` green.

## Design

- **REST (`figma/rest.rs`)** — add the grid/wrap fields the captures
  expose: `grid_row_gap`, `grid_column_gap`, `grid_row_count`,
  `grid_column_count`, `grid_columns_sizing`, `grid_rows_sizing`,
  `grid_row_anchor_index`, `grid_column_anchor_index`, `grid_row_span`,
  `grid_column_span`, `counter_axis_spacing`, `counter_axis_align_content`.
  <!-- As-shipped correction (review D13): `grid_row_count` /
  `grid_column_count` were NOT added — the sizing strings carry one entry
  per track, so the counts are redundant, and the REST subset stays
  partial (only what the lowering needs). -->
  As-built `rest.rs` omits the two count fields.
- **Document model (`document.rs`)** — mirror #43's schema appends:
  `LayoutMode` gains `Wrap`/`Grid`; `CrossAxisAlign` gains `Baseline`; a
  `GridTrack` enum (`Fixed(f32)`/`Fraction(f32)`, mirroring core);
  `LayoutContainer` gains `cross_gap: Option<f32>`,
  `grid_rows`/`grid_columns: Vec<GridTrack>` (so it is `Clone`, not
  `Copy`); `LayoutConstraints` gains `grid_row`/`grid_column: Option<u16>`
  and `grid_row_span`/`grid_column_span: u16` (a hand-written `Default`
  with spans = 1, matching the schema default, so a non-grid child still
  collapses to the absent table).
- **Emit (`emit.rs`)** — write the appended fields instead of the
  hardcoded absent/default placeholders story #43 left; build the
  `GridTrack` table vectors. `node.container` is read by reference now
  that `LayoutContainer` is not `Copy`.
- **Lowering (`figma/mod.rs`, `container_of`)** — `GRID` -> `Grid` mode
  with tracks/gaps/placement; `HORIZONTAL` + `layoutWrap: WRAP` -> `Wrap`
  mode with `cross_gap = counterAxisSpacing`; `BASELINE` -> `Baseline`
  cross-align. `constraints_of` reads the per-child anchors/spans. The
  negative-gap-to-margin rewrite is restricted to `Horizontal`/`Vertical`
  (grid gaps are track spacing, not flow spacing —
  `docs/decisions/v08-layout-vocabulary-shape.md` D5). Two refusals:
  `WRAP` + negative `itemSpacing`, and non-`AUTO`
  `counterAxisAlignContent`.
- **Grid track parsing** — split `gridColumnsSizing`/`gridRowsSizing` on
  whitespace: `Npx` -> `Fixed(N)`, `minmax(0,Nfr)` -> `Fraction(N)`
  (Figma's own serialized form, D2). Any other token is a named refusal
  (P4) — no silent drop.

## #272 (a Fixed child larger than its Fraction cell)

`grid-basic`'s only fixed child (`fixed-size`, 140x60) sits inside a
252x164 fraction cell, so it is smaller than its cell. The capture does
not exercise the overflow case, so it does not answer #272. Report that;
do not change the engine.

## Alternatives considered

- **Keep `LayoutContainer` `Copy` by hanging the track lists off `Node`
  (like `text`/`text_style`).** Rejected: the schema puts the tracks
  inside `LayoutContainer`, and `emit` builds the container table from
  `node.container`; a parallel `Node` field would split one schema table
  across two model fields. Dropping `Copy` costs two `as_ref()` edits in
  the walk and emit, which is cheaper than the split.
- **Lower grid tracks from `gridColumnCount`/`gridRowCount` as equal
  tracks.** Rejected for the same reason #43 rejected plain counts in the
  schema: equal tracks cannot express `grid-basic`'s 160px first column.
  The sizing strings are the authored track sizes.
- **Refuse a grid `Fraction` track under a hug axis in dashc.** Rejected:
  that verdict is the validator's load gate
  (`grid.fraction-track-under-hug`, `v08-layout-vocabulary-shape.md` D6),
  which `compile_figma` already runs. Duplicating it in the producer
  violates P5 (the producer maps, the validator decides). `grid-basic` is
  a fixed-size root, so no hug-grid case arises here anyway.
- **Force `lowering-baseline` to match the captured rects.** Rejected:
  the divergence is real (debt #273). Pinning a false match would hide
  it. Pin the exact lowered intent and name the divergence instead.
