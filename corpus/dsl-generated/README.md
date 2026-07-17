# DSL-generated stress corpus

Edge-case scenes the runtime must handle, authored as code against the
producer surface rather than hand-built in Figma (docs/design/architecture.md,
docs/specification/05-qualification.md E3): wrap, hug-in-fill, grid spans, baseline, variant topology
change, negative gap.

**Status:** landed (story #46, v0.8). The generator is `dashlang`-driven:
each case is authored through the producer surface `dashlang` is the skin
over and solved by `dashscene-engine`'s `TaffySolver`, then pinned against
hand-computed rects (integer-dimensioned, exact-compare —
`docs/decisions/v02-flex-goldens-per-construct.md`). Rects are read back by
`NodeId`, never by a positional DFS index (debt #119). The proof is
`crates/dashlang/tests/corpus.rs`; wrap, grid, and baseline also have
hand-built pixel goldens in `goldens/tooling/tests/v08_fidelity.rs` (#43).

Exit criterion E3 stays **partial** after this story: five of the six named
cases are proven exactly, and the variant case is proven as a layout-topology
change (a wrap line appearing) but not the Figma child-count form, which needs
the variant `Visible` widening (issue #283, the path to met — see
`docs/specification/05-qualification.md` E3).

Most cases author through the `dashlang` builder. Two use core's `Txn`
directly, because the construct is not builder vocabulary: the variant case
(`add_variant_set`/`set_variant` — the sparse five-prop slice, not a
value-tree construct) and the negative-gap cross-check (`gap` +
`lower_negative_gaps`, the shared core lowering).

## Cases

- [negative-gap.md](negative-gap.md) — negative flex gap lowered to child
  margins (story #10), plus the DSL-generated Hug case that exercises the
  #236 rebate (#46). Proofs: `crates/dashscene-engine/tests/solve.rs`,
  `crates/dashlang/tests/corpus.rs`.
- [hug-in-fill.md](hug-in-fill.md) — a Hug-sized node among Fill-sized
  siblings (story #11). Proofs: `goldens/tooling/tests/v02_flex.rs`,
  `crates/dashlang/tests/corpus.rs`.
- [wrap.md](wrap.md) — a wrapping row with a distinct cross gap, plus a
  fixed-height variant that exercises `align_content = FlexStart` line
  packing (#43/#46). Proof: `crates/dashlang/tests/corpus.rs`.
- [grid-spans.md](grid-spans.md) — grid with per-track sizing, cell anchors,
  and row/column spans (#43/#46). Proof: `crates/dashlang/tests/corpus.rs`.
- [baseline.md](baseline.md) — baseline cross-axis alignment, leaf boxes and
  a nested row propagating its first line (#43/#46). Proof:
  `crates/dashlang/tests/corpus.rs`.
- [variant-topology.md](variant-topology.md) — a `set_variant` switch that
  makes a wrap line appear, restructuring the resolved flow (#46). A child
  leaving the laid-out set is a reported blocker (variant vocabulary lacks
  `Visible`). Proof: `crates/dashlang/tests/corpus.rs`.

R2 coverage the six named cases do not otherwise reach, added to the corpus
(#46): a `Vertical` column with a Fill child, and a min/max-clamp split.
Proof: `crates/dashlang/tests/corpus.rs`
(`a_vertical_column_stacks_and_fills_the_main_axis`,
`min_and_max_clamps_bound_a_fill_split`).
