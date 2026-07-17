# Story #46 — E3 stress corpus green (working spec)

    status  working memory (docs/wip); garden into durable records before the PR
    story   #46 (epic #42, v0.8 — fidelity); closes E3

## Goal

Turn the six named E3 cases into `dashlang`-generated stress-corpus scenes, each
verified by an executable test that asserts exact, hand-computed rects
(integer-dimensioned, exact-compare — `docs/decisions/v02-flex-goldens-per-construct.md`).
The six cases: negative-gap, hug-in-fill, wrap, grid spans, baseline, variant
topology change (`docs/specification/05-qualification.md` E3, DESIGN_1 §6.2/§11).

Depends on #43 (engine wrap/grid/baseline + schema) and the #236 fix — both
landed. Folds debt #119 (assert rects by `NodeId`, not positional DFS index)
and debt #103 (sharing the checker-image fixture). Does not touch `dashbuf`,
`dashc`, or the engine's behavior.

## As-built facts that shape the design

- `dashlang` is the stress-corpus generator (crate map), lib-only. Its builder
  exposes the v0.2 flex vocabulary but no wrap-cross-gap or grid vocabulary yet.
  `LayoutMode::Wrap/Grid` and `CrossAxisAlign::Baseline` already exist in core
  (added by #43); `dashlang` re-exports the enums, so `mode(Wrap)` and
  `cross_align(Baseline)` already reach the arena. Missing builder vocabulary:
  `cross_gap`, `grid_rows`/`grid_columns`, `grid_row`/`grid_column`,
  `grid_row_span`/`grid_column_span`.
- Core variants are sparse `X/Y/Width/Height/Fill` overrides against existing
  nodes (`add_variant_set` + `set_variant`); they do NOT mutate the tree.
  `dashlang`'s builder has no variant vocabulary. A `Width` override feeds the
  solver (`Arena::layout`), so `set_variant` + `commit_with(TaffySolver)` reflows
  the layout — the same path E5/FLIP uses.
- `#236` (negative-margin-hug rebate) makes a Hug container over negative child
  margins sum correctly. The E3 negative-gap case must be a Hug container to make
  #236 a genuine prerequisite. `v08-layout-vocabulary-shape.md` D5: a Wrap
  container with a negative gap is a named refusal, so the negative-gap corpus
  case is a plain flex row, never wrap.
- The two already-proven cases keep their proofs: negative-gap in
  `crates/dashscene-engine/tests/solve.rs`, hug-in-fill in
  `goldens/tooling/tests/v02_flex.rs`. The generator complements them.

## Requirements

- R-46.1 The `dashlang` builder shall author wrap (cross gap), grid tracks, grid
  placement, grid spans, and baseline cross-alignment, with DSL output identical
  to the equivalent hand-built `Txn` output (builder acceptance test).
- R-46.2 A single executable corpus test shall generate all six E3 cases through
  the producer surface and assert each case's exact solved rects by `NodeId`.
- R-46.3 The negative-gap case shall be a Hug-width flex row whose hug size and
  child overlap are correct only under the #236 rebate, proven both via the DSL
  margin form and the core `gap` + `lower_negative_gaps` form (equivalence).
- R-46.4 The variant case shall switch an active member with `set_variant` and
  assert the reflowed rects before and after the switch.
- R-46.5 `corpus/dsl-generated/` shall document all six cases; the README status
  shall move from "generator not landed" to the six-case contract met.
- R-46.6 Debt #119 shall be folded: rect assertions by `NodeId` in the new corpus
  test and in the two files #119 names (`v02_flex.rs`, `solve.rs`); a shared
  `rect_of` helper in `goldens/tooling/tests/common/mod.rs` for the golden side.
- R-46.7 Debt #103 shall be folded: `checker_asset` shared from `common/mod.rs`,
  used by `v03.rs` and `v03_families.rs`, preserving both goldens byte-for-byte.
- R-46.8 `docs/specification/05-qualification.md` E3 shall move from partial to
  met, naming each case and its proof, distinguishing new from pre-existing.

## Alternatives considered

### Generator shape: build-time DSL authoring vs committed generated files

Chosen: build-time authoring. DESIGN_1 §6.2 defines the corpus as "generated, not
hand-built in Figma" — code-authored scenes, not serialized fixtures. The existing
cases already follow this (the DSL/`Txn` builds the scene at test time; no `.dsb`
is committed). A committed-scene-file corpus would add a second serialized IR to
maintain and would not exercise the producer surface the corpus exists to stress.
Rejected.

### Corpus test home: `crates/dashlang/tests/corpus.rs` vs `goldens/tooling/tests/`

Chosen: `crates/dashlang/tests/corpus.rs`. The crate map names `dashlang` the
stress-corpus generator; the corpus proof is a solving-correctness proof (exact
rects), needing only `dashlang` + core + the engine solver (a dev-dependency
already present), no Skia. The pixel goldens for wrap/grid/baseline already exist
hand-built in `goldens/tooling/tests/v08_fidelity.rs` (#43); the corpus adds the
DSL-generation + exact-rect proof, not a duplicate image. Placing it in
`goldens/tooling` would pull in Skia for no rendering and separate the generator's
proof from the generator crate. Rejected.

### Variant case authoring: extend the DSL with variant vocabulary vs core `Txn`

Chosen: author the variant case against core's `Txn` (`add_variant_set` /
`set_variant`). Variant overrides are a core staged-mutation concept, not builder
vocabulary; the task scopes the builder extension to wrap/grid/baseline only.
Adding a variant authoring surface to `dashlang` is a larger, separate design
(how a value-tree declares members and overrides) and out of this story's scope.
The corpus test drives the same producer surface either way. Rejected (deferred).

### Negative-gap case: DSL margin form vs core gap+lowering

Chosen: both. The DSL authors the lowered (negative-margin) form directly — the
`#236` scenario — and a core `gap(-8)` + `lower_negative_gaps` form cross-checks
it, reproducing story #10's equivalence criterion under a Hug container. A
rect-level test cannot witness that the lowering ran (Taffy applies a raw negative
gap identically — `docs/decisions/negative-gap-lowering.md`), so both sides are
pinned against the same hand-computed rects rather than against each other.

### #119 scope: local NodeId helpers vs one shared helper; solve.rs included

`solve.rs` and `v02_flex.rs` are named by #119; both migrate. The golden side
(`v02_flex.rs`) uses a shared `rect_of` added to `common/mod.rs` (the shared
golden-helper module, debt #120). `solve.rs` (a different crate) and the new
corpus test each get a tiny local `rect_of`, since an integration-test crate
cannot share code across crates without a dev-only crate (recorded in
`common/mod.rs`). Migration is value-preserving: `NodeId` creation order equals
DFS order for these append-only trees, so each assertion names the same box it
did by index.
