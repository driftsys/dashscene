# Story #46 — implementation plan (TDD)

1. Extend the `dashlang` builder (`crates/dashlang/src/lib.rs`).
   - Add `Node` fields `grid_rows`/`grid_columns: Vec<GridTrack>`; setters
     `cross_gap`, `grid_rows`, `grid_columns`, `grid_row`, `grid_column`,
     `grid_row_span`, `grid_column_span`; re-export `GridTrack`.
   - Emit the new props in `set_base_props` only when set (cross_gap Some;
     grid_row/column Some; spans != 1; track lists non-empty) — non-grid nodes
     stage unchanged.
   - verify: a new `crates/dashlang/tests/builder.rs` case (grid + wrap-cross-gap
     - baseline scene) asserts DSL output == hand-built `Txn` output. Red first
       (setters absent → does not compile / mismatched props), then green.

2. Write `crates/dashlang/tests/corpus.rs` — the six E3 cases.
   - Local `rect_of(&Arena, NodeId) -> (f32,f32,f32,f32)` via `rect_index_of`.
   - negative-gap: DSL Hug row, margin form; core gap+lower form; assert both to
     the same rects incl. hug width 74.
   - hug-in-fill: DSL, assert rects (120x60 root; hug 30; two Fill 45).
   - wrap: DSL, assert the v08_fidelity wrap rects (root 200x100; four chips).
   - grid spans: DSL, assert the v08_fidelity grid rects (six boxes).
   - baseline: DSL, assert the v08_fidelity baseline rects (three boxes).
   - variant: core `add_variant_set`/`set_variant`; assert before/after rects.
   - verify: `cargo test -p dashlang --test corpus`. Author expected rects by
     hand, run, confirm the solver agrees; if a case disagrees, it is an engine
     bug → STOP that case and report (out of scope), continue the others.

3. `corpus/dsl-generated/`: add `wrap.md`, `grid-spans.md`, `baseline.md`,
   `variant-topology.md`; update `negative-gap.md`/`hug-in-fill.md` to name the
   new DSL proof; update `README.md` Cases + status.

4. Fold #119: replace the positional `rect(&arena, i)` helper with a `NodeId`
   `rect_of` in `goldens/tooling/tests/v02_flex.rs` (via `common::rect_of`) and
   `crates/dashscene-engine/tests/solve.rs` (local). Capture the `NodeId`s each
   test asserts. verify: `cargo test -p goldens --test v02_flex`,
   `cargo test -p dashscene-engine --test solve` stay green (same values).

5. Fold #103: add `checker_asset(dark: Color)` to `common/mod.rs`; `v03.rs` and
   `v03_families.rs` call it with their own dark (light is shared). verify: both
   v03 goldens pass unchanged (`cargo test -p goldens --test v03 --test v03_families`).

6. `docs/specification/05-qualification.md`: E3 partial → met; summary table row.

7. Garden: fold the builder-vocabulary addition into `docs/design/dashlang.md`
   (as-built); record the corpus in the durable records; move `docs/wip/` originals
   to `docs/archive/`. `docs/wip/` ends empty.

8. Gates: `just build`, `just verify`. Squash to one conventional commit. Push.
   Open a DRAFT PR; run `/code-review`; capture findings as a checklist; STOP.
