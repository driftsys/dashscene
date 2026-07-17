# Story #43 — engine + schema layout fidelity: spec

Status: working memory (Superpowers session, 2026-07-17). Gardened into
`docs/design/dashscene-engine.md`, `docs/design/dashbuf.md`, and
`docs/decisions/` before the PR; then this file moves to `docs/archive/`.

## Goal

Teach the sole Taffy solver wrap mode, grid with spans, and baseline
counter-axis alignment; append the schema fields those constructs need
(`crates/dashbuf/schema/dashbuf.fbs`, append-only, frozen r7 fixture
regenerated in the same commit); add the core `Prop`/layout plumbing that
carries the fields to the engine. Fold debts #236 (fixed early — hard
prerequisite of E3's negative-gap case), #177, #115, #189. #105 stays
open, gated on the parked `real-file` fixture (#265).

Out of scope: dashc lowering changes (#264 un-pins the D5 refusals into
this story's schema fields), dashcue, the stress-corpus generator (#46),
masks/opacity (#44).

## Facts the design rests on

- **#236 root cause, located.** taffy 0.12.1/0.12.2,
  `src/compute/flexbox.rs`, `determine_container_main_size`, the
  `MinContent | MaxContent` branch. The first pass computes
  `content_flex_fraction = diff / f32_max(1.0, flex_shrink * inner_flex_basis)`
  (divisor 1.0 for a shrink-0 item); the second pass reconstructs the
  item size with `f32_max(1.0, flex_shrink) * inner_flex_basis *
  flex_fraction` (multiplier = the item's basis for a shrink-0 item). A
  negative main-axis margin makes `diff = margin sum < 0`, and the
  reconstruction amplifies it by the flex basis: a 56-wide child with
  margin-left −16 contributes 56 − 896 instead of 40. This reproduces
  issue #236's table exactly (−1 → 56, −16 → 0). Positive margins take
  the `diff > 0` path, whose two formulas agree — which is why only
  negative margins mis-sum.
- **The captured fixtures pin the needed vocabulary.**
  `corpus/figma-fixtures/grid-basic.json`: `layoutMode: GRID`,
  `gridColumnCount`/`gridRowCount`, `gridColumnGap`/`gridRowGap`,
  per-track sizing (`gridColumnsSizing: "160px minmax(0,1fr)
  minmax(0,1fr)"`), and per-child `gridRowAnchorIndex`/
  `gridColumnAnchorIndex`/`gridRowSpan`/`gridColumnSpan`.
  `lowering-wrap.json`: `layoutWrap: WRAP` with `itemSpacing` (main) and
  `counterAxisSpacing` (cross) — two gaps. `lowering-baseline.json`:
  `counterAxisAlignItems: BASELINE` on a horizontal row of mixed-size
  text plus a nested box. Since #264 must un-pin the D5 refusals into
  this story's schema fields, per-track sizing and the second gap are
  required vocabulary, not speculation.
- **Q-4 answer (from the taffy source).** Baselines are computed for
  flex rows only. A leaf (including a measured text leaf — the measure
  closure returns a `Size`, no baseline channel) synthesizes its
  baseline as `height + margin.top`, i.e. its bottom edge; a nested flex
  row propagates its first line's real baseline
  (`offset_vertical + child.baseline`). In a `Vertical` container,
  Baseline degrades to start-alignment. Glyph-true text baselines would
  need a baseline channel through the measure seam — reported as a debt
  candidate, not built here.

## Decisions (alternatives considered inline)

### D1 — #236 workaround: the negative-margin basis rebate

For a flex-flow child whose **main-axis sizing is `Fixed`** and whose
**main-axis margin sum is negative**, `style_for` maps
`flex_basis = size + margin_sum` (the rebate) and floors the main-axis
`min_size` at `size` (capped by an authored max, maxed with an authored
min). Effect: `content_contribution` (clamped preferred + margins)
equals the rebated basis, `diff = 0`, and the broken reconstruction is
never entered; the definite pass clamps the hypothetical size back to
`size` via the min floor, so positions and sizes are unchanged
everywhere else. Verified by hand against the taffy source for the #236
table, the captured fixture (root width 264), and the definite pass.

Alternatives considered:

1. **Fork/patch taffy** (make the two shrink-factor formulas agree).
   Correct at the source and covers every sizing, but carries a fork of
   a core dependency for one arithmetic line; the upstream defect is
   reported instead (debt candidate below) and the workaround is
   removable when a fixed taffy releases.
2. **Engine pre-pass computing hug main sizes.** Re-implements
   intrinsic sizing outside taffy — a second solver in all but name
   (against the one-solver posture of P2) and wrong the day taffy's
   evaluation changes.
3. **Map `Fixed` to `flex_shrink: 1` plus a min floor** (shrink-1 makes
   the two formulas agree). Touches every `Fixed` child everywhere, and
   leans on the formulas agreeing only at shrink = 1 — a wider blast
   radius for the same effect.

Residual: a **`Hug`-sized child** with a negative margin still mis-sums
(its basis is content-derived, so no static rebate exists). Named as a
debt candidate; the captured negative-gap scene and E3's case use fixed
children.

### D2 — schema shape for wrap/grid/baseline (append-only)

- `LayoutMode` appends `Wrap = 3`, `Grid = 4` (the placement the schema
  comments reserved at v0.2). `Wrap` is a horizontal wrapping row —
  Figma's `layoutWrap` exists for horizontal auto-layout only.
- `CrossAxisAlign` appends `Baseline = 3` (Q-4).
- `LayoutContainer` appends `cross_gap: float32 = null` — the cross-axis
  gap (wrap's line spacing, grid's row gap). Absent = follows `gap`,
  which preserves the v0.2 both-axes mapping for every existing
  document. Alternative — a `row_gap`/`column_gap` rename — was
  rejected: it cannot reuse the existing `gap` field id (R7 forbids
  repurposing) and would leave `gap` dead.
- `LayoutContainer` appends `grid_rows: [GridTrack]`,
  `grid_columns: [GridTrack]` — the per-track sizing the captured grid
  needs (`160px minmax(0,1fr) …`). `GridTrack` is a **table**
  (`sizing: GridTrackSizing`, `value: float32`) so a future minmax
  bound or named line can append as fields; a struct can never gain
  fields. `GridTrackSizing : uint8 { Fixed = 0, Fraction = 1 }` —
  `Fixed` is a document-unit length, `Fraction` is a flexible weight
  lowered as `minmax(0, Nfr)`, matching Figma's serialized tracks.
  Alternative — plain `grid_rows: ushort` counts with equal tracks —
  was rejected: it cannot express the captured 160 px first column, so
  #264 could not lower `grid-basic.json` against it.
- `LayoutConstraints` appends `grid_row: ushort = null`,
  `grid_column: ushort = null` (0-based anchor cell; absent =
  auto-placed), `grid_row_span: ushort = 1`,
  `grid_column_span: ushort = 1`. Alternative — a separate placement
  table on `Node` — adds a table for four scalars with no evolution
  gain; the container comment already reserves `LayoutContainer` for
  track fields and `LayoutConstraints` is the child-side home.

### D3 — core plumbing shape

- `LayoutMode::{Wrap, Grid}` and `CrossAxisAlign::Baseline` append to
  the core enums.
- `Layout` gains `cross_gap: Option<f32>`, `grid_row: Option<u16>`,
  `grid_column: Option<u16>`, `grid_row_span: u16` (default 1),
  `grid_column_span: u16` (default 1) — all `Copy`, so `Layout` stays
  `Copy`.
- Track lists are variable-length, so they live **beside** `Layout` in
  `NodeData` (the same split as `text`): `Txn::set_prop` takes
  `Prop::GridRows(Vec<GridTrack>)` / `Prop::GridColumns(Vec<GridTrack>)`
  and `Arena::grid_tracks(node) -> (&[GridTrack], &[GridTrack])` is the
  read seam. Alternative — `Vec` inside `Layout` — costs `Layout: Copy`
  and turns every `arena.layout()` read into a clone; rejected.
- New props classify as `PropClass::Layout` (dirty tracking works
  unchanged); `commit()`'s `FixedSolver` ignores them like the rest of
  the flex vocabulary.

### D4 — engine mapping

Container side:

- `Wrap` maps as `Horizontal` plus `flex_wrap: Wrap` and
  `align_content: FlexStart` (Figma packs lines; taffy's default
  `None` behaves as stretch and would move lines in a fixed-height
  container).
- The gap mapping becomes axis-aware everywhere:
  main axis ← `gap`, cross axis ← `cross_gap` or `gap` when unset
  (for `Grid`, columns-gap ← `gap`, rows-gap ← `cross_gap` or `gap` —
  the horizontal/vertical split, same as `Wrap`).
- `Grid` maps to `Display::Grid` with
  `grid_template_rows`/`grid_template_columns` from the track lists
  (`Fixed(v)` → `length(v)`, `Fraction(w)` → `minmax(length(0.0),
  fr(w))`), plus gap and padding. `main_align`/`cross_align` are not
  mapped in grid mode — placement is by cell.
- `CrossAxisAlign::Baseline` → `AlignItems::Baseline` (rows only, per
  Q-4; in a `Vertical` container it degrades to start, pinned by test).

Child side (parent-mode match):

- Under a `Wrap` parent: identical to `Horizontal` (main axis =
  horizontal).
- Under a `Grid` parent: margin maps as in flex flow; placement maps
  `grid_row`/`grid_column` anchors to 1-based taffy lines with
  `span(grid_*_span)` ends (absent anchor = auto placement);
  per-axis alignment comes from sizing — `Fill` → `Stretch`,
  `Fixed`/`Hug` → `Start` (`justify_self`/`align_self`), which is what
  the captured grid shows (a hug chip sits at its cell origin, fill
  cells stretch).
- The `taffy` dependency gains its `grid` feature.

### D5 — #177 fix

`measure_text` gains the `AvailableSpace::MinContent => Some(0.0)` arm:
a min-content probe measures at wrap width 0, which the greedy breaker
turns into one word per line — width = widest word, the true
min-content box. Pinned by a shrinkable text node in a
width-constrained row whose automatic minimum keeps it at widest-word
width, not full-line width.

## Acceptance (all in the construct-owning crate)

- `crates/dashscene-engine/tests/solve.rs`: the #236 table (0/+16/−1/−16)
  solved to 112/128/111/96; wrap reproducing `lowering-wrap.json`'s
  hand-computed line breaks and the captured boxes; grid reproducing
  `grid-basic.json`'s captured boxes (fixed + fraction tracks, row/col
  spans, fill/hug/fixed children); the Q-4 mixed-size baseline row
  (leaf bottoms + a nested row's real baseline) against hand-computed
  rects; baseline-in-column degrades to start.
- `crates/dashc/tests/flex_lowering.rs`: the pinned collapsed root
  width flips to the Figma-captured 264 — closing #236 loudly.
- `crates/dashscene-engine/tests/measure.rs`: the #177 min-content case.
- `crates/dashlang/tests/builder.rs`: the #189 defaults test.
- `crates/dashbuf/tests/roundtrip.rs` + `schema_evolution.rs`: new
  fields round-trip and are frozen at non-default values (fixture
  regenerated in the same commit — the legitimate append case).
- `goldens/`: one exact-compare, integer-dimensioned golden per new
  construct (wrap, grid spans, baseline), the v0.2 per-construct
  pattern.
