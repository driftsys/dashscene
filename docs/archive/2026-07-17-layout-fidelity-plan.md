# Story #43 — implementation plan

TDD throughout: red test first per behavior, then green.
`cargo test -p dashscene-engine` (and the touched crate) at every step.

1. **#236 first** (hard prerequisite of E3).
   - Red: `solve.rs` test with the issue's margin table
     (0/+16/−1/−16 → 112/128/111/96) on a hug row of two fixed-56
     children.
   - Green: the basis-rebate mapping in `style_for` (spec D1).
   - Flip `crates/dashc/tests/flex_lowering.rs`'s pinned root width to
     264 (red before the engine fix, green after).
2. **Schema append** (spec D2): `dashbuf.fbs` fields; extend
   `roundtrip.rs` (red against old schema) and `schema_evolution.rs`
   (new node 4 carrying every new field at non-default values);
   regenerate the frozen fixture with `UPDATE_DSB_FIXTURE=1`; mechanical
   `LayoutContainerArgs`/`LayoutConstraintsArgs` compile fixes in
   `dashc/src/emit.rs` (absent values — emitted bytes unchanged).
3. **Core plumbing** (spec D3): enums, `Layout` fields, `GridTrack`,
   props, `grid_tracks` accessor, prop classification, `load.rs`
   decode, validator `check_enum` for `GridTrack.sizing`.
4. **Engine wrap** (spec D4): red acceptance test from
   `lowering-wrap.json`'s numbers; green mapping (`flex_wrap`,
   `align_content`, axis-aware gap).
5. **Engine grid**: red acceptance test from `grid-basic.json`'s
   numbers; green mapping (templates, placement, per-axis self
   alignment, taffy `grid` feature).
6. **Engine baseline (Q-4)**: red mixed-size baseline-row test
   (hand-computed from the synthesis rule) + baseline-in-column
   degrade; green `AlignItems::Baseline` arm.
7. **#177**: red min-content measure test; green `MinContent =>
   Some(0.0)` arm.
8. **#115**: factor the margin helper in `solve.rs` (tests stay green).
9. **#189**: `unset_flex_fields_keep_core_defaults` in dashlang
   builder tests.
10. **Goldens**: `goldens/tooling/tests/v08_fidelity.rs`, one
    integer-dimensioned exact-compare golden per construct
    (wrap/grid/baseline), rects asserted before pixels;
    `UPDATE_GOLDENS=1` to author the images.
11. **Garden**: as-built updates to `docs/design/dashscene-engine.md`
    and `docs/design/dashbuf.md`; decision records for D1 and D2;
    Q-4 resolved in `docs/technotes/open-questions.md`; decisions
    README index; move this spec+plan to `docs/archive/`.
12. **Gates**: `just build`, `just verify`, `just wasm`. Squash to one
    conventional commit, push, open the draft PR.

Verification per step: the named test file runs red before the change
and green after; the full workspace suite stays green at every commit
point.
