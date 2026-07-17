//! Story #46: the DSL-generated E3 stress corpus (`corpus/dsl-generated/`,
//! `docs/specification/05-qualification.md` E3, DESIGN_1 §6.2/§11). The named
//! edge cases — negative gap, hug-in-fill, wrap, grid spans, baseline, variant
//! topology change — plus the R2 vocabulary the six do not otherwise reach
//! (a `Vertical` case and a min/max case), authored through the producer
//! surface `dashlang` is the skin over, each solved by `dashscene-engine`'s
//! `TaffySolver` and pinned against hand-computed rects.
//!
//! Every scene is integer-dimensioned, so each solved rect lands on an integer
//! and the comparison is exact — the discipline
//! `docs/decisions/v02-flex-goldens-per-construct.md` sets. Rects are read back
//! by `NodeId` (looked up by the node's authored name), never by a hand-counted
//! DFS index, so inserting a node cannot silently renumber an assertion
//! (debt #119).
//!
//! Most cases author through `dashlang`'s value-tree builder. Two use core's
//! `Txn` directly, because the construct is not builder vocabulary:
//!
//! - the variant case declares an `add_variant_set` and switches its active
//!   member with `set_variant` to hide a child (`VariantValue::Visible(false)`
//!   → Taffy `Display::None`); core variants are sparse scalar overrides — the
//!   slice X/Y/Width/Height/Fill/Visible
//!   (`docs/decisions/variant-set-flat-index.md`, story #283);
//! - the negative-gap case cross-checks the DSL margin form against a
//!   `gap` + `lower_negative_gaps` form, the shared core lowering pass
//!   (`docs/decisions/negative-gap-lowering.md`).
//!
//! The variant case proves the "different child counts" reading of E3's sixth
//! stress case: a `set_variant` switch that hides a child removes it from the
//! laid-out set, its sibling reflows, and the Hug container collapses — the
//! topology change the five-prop slice could not express until `Visible` joined
//! the variant override vocabulary (story #283).
//!
//! Pre-existing complementary proofs are kept, not replaced: `negative-gap` in
//! `crates/dashscene-engine/tests/solve.rs`, `hug-in-fill` in
//! `goldens/tooling/tests/v02_flex.rs`. The hand-built pixel goldens for wrap,
//! grid, and baseline live in `goldens/tooling/tests/v08_fidelity.rs` (#43);
//! this corpus is the DSL-generated, exact-rect companion.

use dashlang::{Arena, AxisSizing, Color, CrossAxisAlign, GridTrack, LayoutMode, node, scene};
use dashscene_core::{NodeId, Prop, VariantMember, VariantValue};
use dashscene_engine::TaffySolver;

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

const NAVY: Color = rgb(0.05, 0.1, 0.2);
const RED: Color = rgb(0.8, 0.1, 0.1);
const GREEN: Color = rgb(0.1, 0.7, 0.2);
const GOLD: Color = rgb(0.9, 0.7, 0.1);
const BLUE: Color = rgb(0.2, 0.4, 0.9);

/// Rect `(x, y, w, h)` of a committed node, addressed by identity rather than
/// by a positional DFS index (debt #119).
fn rect_of(arena: &Arena, node: NodeId) -> (f32, f32, f32, f32) {
    let scene = arena.committed();
    let index = scene
        .rect_index_of(node)
        .expect("the node is committed in this generation");
    let r = scene.rects()[index as usize];
    (r.x, r.y, r.w, r.h)
}

/// The committed node carrying the authored `name`. The DSL builder forwards a
/// node's name to the arena, so a scene names the boxes it asserts and this
/// resolves each name to its `NodeId` — no hand-counted indices.
fn named(arena: &Arena, want: &str) -> NodeId {
    let scene = arena.committed();
    (0..scene.rects().len())
        .map(|i| scene.node_of(u32::try_from(i).unwrap()))
        .find(|&n| arena.name(n) == Some(want))
        .unwrap_or_else(|| panic!("no committed node is named {want:?}"))
}

#[test]
fn negative_gap_overlaps_children_and_hugs_to_the_reduced_width() {
    // A Hug-width horizontal row of three fixed 30x20 boxes with an item
    // spacing of -8: each box after the first overlaps its predecessor by 8.
    // The row hugs to 30 + 22 + 22 = 74, correct only under the #236 rebate
    // (taffy 0.12 alone collapses the hug sum over negative child margins).
    // A plain flex row, never wrap: a negative wrap gap is a named refusal
    // (`docs/decisions/v08-layout-vocabulary-shape.md` D5).
    //
    // DSL side: the lowered (negative child margin) form, authored directly.
    let mut dsl = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Horizontal)
        .sizing_h(AxisSizing::Hug)
        .size(0.0, 20.0)
        .fill(NAVY)
        .child(node("a").size(30.0, 20.0).fill(RED))
        .child(
            node("b")
                .size(30.0, 20.0)
                .margin(-8.0, 0.0, 0.0, 0.0)
                .fill(GOLD),
        )
        .child(
            node("c")
                .size(30.0, 20.0)
                .margin(-8.0, 0.0, 0.0, 0.0)
                .fill(GREEN),
        )])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(
        rect_of(&dsl, named(&dsl, "row")),
        (0.0, 0.0, 74.0, 20.0),
        "hug width"
    );
    assert_eq!(rect_of(&dsl, named(&dsl, "a")), (0.0, 0.0, 30.0, 20.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "b")),
        (22.0, 0.0, 30.0, 20.0),
        "30 - 8 overlap"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "c")),
        (44.0, 0.0, 30.0, 20.0),
        "52 - 8 overlap"
    );

    // Core side: the negative-gap form as a producer authors it — a negative
    // `gap` lowered by the shared core pass `lower_negative_gaps`, exercising
    // the real lowering path the DSL margin form above skips. Its own
    // explicit rects are the guard: a DSL-vs-core equivalence assertion would
    // be tautological, since `lower_negative_gaps` sets exactly the margins
    // the DSL form authors by hand and taffy applies a raw negative gap
    // identically (`docs/decisions/negative-gap-lowering.md`); it would only
    // prove the solver is deterministic. Pinning the lowered form's rects
    // against the hand-computed table instead witnesses the lowering's output.
    let mut core = Arena::new();
    let mut txn = core.open();
    let row = txn.add_node(None, Some("row"));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::Height(20.0));
    txn.set_prop(row, Prop::Gap(-8.0));
    let mut children = Vec::new();
    for _ in 0..3 {
        let child = txn.add_node(Some(row), None);
        txn.set_prop(child, Prop::Width(30.0));
        txn.set_prop(child, Prop::Height(20.0));
        children.push(child);
    }
    txn.lower_negative_gaps();
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(
        rect_of(&core, row),
        (0.0, 0.0, 74.0, 20.0),
        "hug width, lowered form"
    );
    assert_eq!(rect_of(&core, children[0]), (0.0, 0.0, 30.0, 20.0));
    assert_eq!(rect_of(&core, children[1]), (22.0, 0.0, 30.0, 20.0));
    assert_eq!(rect_of(&core, children[2]), (44.0, 0.0, 30.0, 20.0));
}

#[test]
fn hug_in_fill_sizes_content_first_then_splits_the_rest() {
    // A 120x60 row: a Hug box (its width is its 30-wide child's) followed by
    // two Fill boxes that split the remaining (120 - 30) / 2 = 45 each. The
    // two sizing modes resolve against each other in one pass.
    let mut dsl = Arena::new();
    scene([node("root")
        .mode(LayoutMode::Horizontal)
        .size(120.0, 60.0)
        .fill(NAVY)
        .child(
            node("hug")
                .mode(LayoutMode::Horizontal)
                .sizing_h(AxisSizing::Hug)
                .size(0.0, 60.0)
                .fill(RED)
                .child(node("inner").size(30.0, 40.0).fill(GOLD)),
        )
        .child(
            node("fill-a")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 60.0)
                .fill(GREEN),
        )
        .child(
            node("fill-b")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 60.0)
                .fill(BLUE),
        )])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "root")), (0.0, 0.0, 120.0, 60.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "hug")),
        (0.0, 0.0, 30.0, 60.0),
        "content width"
    );
    assert_eq!(rect_of(&dsl, named(&dsl, "inner")), (0.0, 0.0, 30.0, 40.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "fill-a")),
        (30.0, 0.0, 45.0, 60.0)
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "fill-b")),
        (75.0, 0.0, 45.0, 60.0)
    );
}

#[test]
fn wrap_breaks_lines_and_hugs_to_them() {
    // A 200-wide, Hug-height wrap row: padding 10, main gap 10, cross gap 20.
    // The inner width is 180, so 80 + 10 + 60 = 150 fits and + 10 + 70 does
    // not: the row breaks into [80, 60] then [70, 50]. The distinct cross gap
    // (20) sets the line spacing, and the hug height is 10 + 30 + 20 + 30 + 10
    // = 100.
    let mut dsl = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Wrap)
        .size(200.0, 0.0)
        .sizing_v(AxisSizing::Hug)
        .gap(10.0)
        .cross_gap(20.0)
        .padding(10.0, 10.0, 10.0, 10.0)
        .fill(NAVY)
        .child(node("chip0").size(80.0, 30.0).fill(RED))
        .child(node("chip1").size(60.0, 30.0).fill(GOLD))
        .child(node("chip2").size(70.0, 30.0).fill(GREEN))
        .child(node("chip3").size(50.0, 30.0).fill(BLUE))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(
        rect_of(&dsl, named(&dsl, "row")),
        (0.0, 0.0, 200.0, 100.0),
        "hug height"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "chip0")),
        (10.0, 10.0, 80.0, 30.0),
        "line 1"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "chip1")),
        (100.0, 10.0, 60.0, 30.0),
        "80 + 10 gap"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "chip2")),
        (10.0, 60.0, 70.0, 30.0),
        "wrapped line 2"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "chip3")),
        (90.0, 60.0, 50.0, 30.0),
        "70 + 10 gap"
    );
}

#[test]
fn grid_spans_place_children_across_tracks() {
    // A fixed 200x160 grid, padding 10, both gaps 10, columns [60px, 1fr, 1fr]
    // and rows [40px, 1fr, 1fr]: the fraction columns take
    // (200 - 20 - 20 - 60) / 2 = 50 and the fraction rows
    // (160 - 20 - 20 - 40) / 2 = 40. A header spans all three columns, a tall
    // cell spans two rows, a footer spans two columns, and a fixed 30x20 box
    // sits at its cell origin instead of stretching.
    let mut dsl = Arena::new();
    scene([node("grid")
        .mode(LayoutMode::Grid)
        .size(200.0, 160.0)
        .gap(10.0)
        .cross_gap(10.0)
        .padding(10.0, 10.0, 10.0, 10.0)
        .fill(NAVY)
        .grid_columns([
            GridTrack::Fixed(60.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ])
        .grid_rows([
            GridTrack::Fixed(40.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ])
        .child(
            node("header")
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .grid_row(0)
                .grid_column(0)
                .grid_column_span(3)
                .fill(RED),
        )
        .child(
            node("tall")
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .grid_row(1)
                .grid_column(0)
                .grid_row_span(2)
                .fill(GOLD),
        )
        .child(
            node("plain")
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .grid_row(1)
                .grid_column(1)
                .fill(GREEN),
        )
        .child(
            node("footer")
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .grid_row(2)
                .grid_column(1)
                .grid_column_span(2)
                .fill(BLUE),
        )
        .child(
            node("fixed")
                .size(30.0, 20.0)
                .grid_row(1)
                .grid_column(2)
                .fill(GREEN),
        )])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "grid")), (0.0, 0.0, 200.0, 160.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "header")),
        (10.0, 10.0, 180.0, 40.0),
        "spans 3 columns"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "tall")),
        (10.0, 60.0, 60.0, 90.0),
        "spans 2 rows"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "plain")),
        (80.0, 60.0, 50.0, 40.0),
        "plain fill cell"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "footer")),
        (80.0, 110.0, 110.0, 40.0),
        "spans 2 columns"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "fixed")),
        (140.0, 60.0, 30.0, 20.0),
        "at its cell origin"
    );
}

#[test]
fn baseline_aligns_mixed_height_boxes_on_their_bottoms() {
    // A fixed 140x60 row, gap 10, baseline-aligned. A leaf's baseline is its
    // bottom edge, so the three mixed-height boxes align their bottoms at the
    // tallest child's (48): y = 48 - height, i.e. 28, 0, 16.
    let mut dsl = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Horizontal)
        .size(140.0, 60.0)
        .gap(10.0)
        .cross_align(CrossAxisAlign::Baseline)
        .fill(NAVY)
        .child(node("short").size(30.0, 20.0).fill(RED))
        .child(node("tall").size(40.0, 48.0).fill(GOLD))
        .child(node("middle").size(30.0, 32.0).fill(GREEN))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "row")), (0.0, 0.0, 140.0, 60.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "short")),
        (0.0, 28.0, 30.0, 20.0),
        "48 - 20"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "tall")),
        (40.0, 0.0, 40.0, 48.0),
        "the tallest, baseline 48"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "middle")),
        (90.0, 16.0, 30.0, 32.0),
        "48 - 32"
    );
}

#[test]
fn baseline_propagates_from_a_nested_row() {
    // The nested-row half of the baseline construct (Q-4,
    // `docs/decisions/v08-layout-vocabulary-shape.md` D5): a row nested under
    // a baseline-aligned container contributes its FIRST line's baseline, not
    // its own bottom edge. A 200x80 row, gap 10, baseline-aligned, holding two
    // leaves and a nested row (padding-top 4 around a 20x10 leaf). The nested
    // row's baseline is 4 + 10 = 14, so it aligns 14 below the line's baseline
    // (the tall leaf's 48): y = 48 - 14 = 34, and its inner leaf sits at
    // 34 + 4 = 38.
    let mut dsl = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Horizontal)
        .size(200.0, 80.0)
        .gap(10.0)
        .cross_align(CrossAxisAlign::Baseline)
        .fill(NAVY)
        .child(node("short").size(30.0, 20.0).fill(RED))
        .child(node("tall").size(40.0, 48.0).fill(GOLD))
        .child(
            node("nested")
                .mode(LayoutMode::Horizontal)
                .size(60.0, 40.0)
                .padding(0.0, 4.0, 0.0, 0.0)
                .fill(GREEN)
                .child(node("inner").size(20.0, 10.0).fill(BLUE)),
        )])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "row")), (0.0, 0.0, 200.0, 80.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "short")),
        (0.0, 28.0, 30.0, 20.0),
        "leaf baseline 20: 48 - 20"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "tall")),
        (40.0, 0.0, 40.0, 48.0),
        "the tallest, baseline 48"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "nested")),
        (90.0, 34.0, 60.0, 40.0),
        "nested row propagates its first line's baseline 14: 48 - 14"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "inner")),
        (90.0, 38.0, 20.0, 10.0),
        "the nested leaf, at the nested row's padding-top"
    );
}

#[test]
fn a_vertical_column_stacks_and_fills_the_main_axis() {
    // LayoutMode::Vertical (R2's second mode): a 40x120 column, gap 10, with a
    // Fill child between two fixed 30-high boxes. The Fill takes the remaining
    // 120 - 30 - 30 - 2*10 = 40 on the main (vertical) axis, so the boxes
    // stack at y = 0, 40, 90.
    let mut dsl = Arena::new();
    scene([node("col")
        .mode(LayoutMode::Vertical)
        .size(40.0, 120.0)
        .gap(10.0)
        .fill(NAVY)
        .child(node("top").size(40.0, 30.0).fill(RED))
        .child(
            node("fill")
                .sizing_v(AxisSizing::Fill)
                .size(40.0, 0.0)
                .fill(GOLD),
        )
        .child(node("bot").size(40.0, 30.0).fill(GREEN))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "col")), (0.0, 0.0, 40.0, 120.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "top")),
        (0.0, 0.0, 40.0, 30.0),
        "at the top"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "fill")),
        (0.0, 40.0, 40.0, 40.0),
        "fills the remaining main-axis space"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "bot")),
        (0.0, 90.0, 40.0, 30.0),
        "40 + 40 + 10 gap"
    );
}

#[test]
fn min_and_max_clamps_bound_a_fill_split() {
    // R2's min/max clamps. Two Fill siblings that would split 100/100 in a
    // 200-wide row; a clamp on the first moves the split and the freed (or
    // taken) space goes to the sibling.
    let mut max = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Horizontal)
        .size(200.0, 30.0)
        .fill(NAVY)
        .child(
            node("capped")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .max_width(50.0)
                .fill(RED),
        )
        .child(
            node("rest")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .fill(GREEN),
        )])
    .build_with(&mut max, &mut TaffySolver::new());
    assert_eq!(
        rect_of(&max, named(&max, "capped")),
        (0.0, 0.0, 50.0, 30.0),
        "capped at max_width 50"
    );
    assert_eq!(
        rect_of(&max, named(&max, "rest")),
        (50.0, 0.0, 150.0, 30.0),
        "the sibling takes the rest"
    );

    let mut min = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Horizontal)
        .size(200.0, 30.0)
        .fill(NAVY)
        .child(
            node("floored")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .min_width(150.0)
                .fill(GOLD),
        )
        .child(
            node("rest")
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .fill(BLUE),
        )])
    .build_with(&mut min, &mut TaffySolver::new());
    assert_eq!(
        rect_of(&min, named(&min, "floored")),
        (0.0, 0.0, 150.0, 30.0),
        "floored at min_width 150"
    );
    assert_eq!(
        rect_of(&min, named(&min, "rest")),
        (150.0, 0.0, 50.0, 30.0),
        "the sibling keeps only 50"
    );
}

#[test]
fn a_fixed_height_wrap_packs_its_lines_at_the_cross_start() {
    // The Wrap->flex mapping sets align_content = FlexStart (D5): wrap lines
    // pack at the cross start, they do not spread. That mapping is inert in a
    // Hug-height container (the lines define the height), so this case fixes
    // the height at 200 to make it load-bearing. Two 60x30 boxes in a 100-wide
    // row wrap (60 + 60 > 100), and the second line packs at y = 40 (30 + 10
    // cross gap), not at the container's far edge as stretch would place it.
    let mut dsl = Arena::new();
    scene([node("row")
        .mode(LayoutMode::Wrap)
        .size(100.0, 200.0)
        .cross_gap(10.0)
        .fill(NAVY)
        .child(node("first").size(60.0, 30.0).fill(RED))
        .child(node("second").size(60.0, 30.0).fill(GOLD))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_eq!(rect_of(&dsl, named(&dsl, "row")), (0.0, 0.0, 100.0, 200.0));
    assert_eq!(
        rect_of(&dsl, named(&dsl, "first")),
        (0.0, 0.0, 60.0, 30.0),
        "line 1"
    );
    assert_eq!(
        rect_of(&dsl, named(&dsl, "second")),
        (0.0, 40.0, 60.0, 30.0),
        "packed at the cross start, not spread over the 200 height"
    );
}

#[test]
fn a_variant_switch_hides_a_child_and_reflows_the_laid_out_set() {
    // The variant case, in E3's true "different child counts" form
    // (story #283): a `set_variant` switch sets a child's `Visible(false)`,
    // which lowers to Taffy `Display::None` — the child leaves the laid-out
    // set, its sibling closes into its place, and the Hug row collapses by the
    // child's width. Switching back re-adds it. `Visible` reaching the
    // laid-out set through a variant override is exactly the topology change
    // the five-prop slice could not express before
    // (`docs/decisions/variant-set-flat-index.md`).
    //
    // A Hug-width horizontal row of three fixed 30x20 chips, no gap: the row
    // hugs to 90 with all shown, 60 with the middle chip hidden.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, Some("row"));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::Height(20.0));
    let a = txn.add_node(Some(row), Some("a"));
    txn.set_prop(a, Prop::Width(30.0));
    txn.set_prop(a, Prop::Height(20.0));
    let b = txn.add_node(Some(row), Some("b"));
    txn.set_prop(b, Prop::Width(30.0));
    txn.set_prop(b, Prop::Height(20.0));
    let c = txn.add_node(Some(row), Some("c"));
    txn.set_prop(c, Prop::Width(30.0));
    txn.set_prop(c, Prop::Height(20.0));
    let set = txn.add_variant_set(vec![
        VariantMember {
            name: Some("all".to_string()),
            overrides: vec![],
        },
        VariantMember {
            name: Some("hide-middle".to_string()),
            overrides: vec![(b, VariantValue::Visible(false))],
        },
    ]);
    txn.commit_with(&mut TaffySolver::new());

    // Member 0 "all": three chips packed left to right, the row hugs to 90.
    assert_eq!(rect_of(&arena, row), (0.0, 0.0, 90.0, 20.0), "all shown");
    assert_eq!(rect_of(&arena, a), (0.0, 0.0, 30.0, 20.0), "a first");
    assert_eq!(
        rect_of(&arena, b),
        (30.0, 0.0, 30.0, 20.0),
        "b in the middle"
    );
    assert_eq!(rect_of(&arena, c), (60.0, 0.0, 30.0, 20.0), "c last");

    // Switch to member 1 "hide-middle": b leaves the laid-out set (a
    // degenerate rect), c reflows into b's place, and the row collapses to 60.
    let mut txn = arena.open();
    txn.set_variant(set, 1);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(
        rect_of(&arena, row),
        (0.0, 0.0, 60.0, 20.0),
        "the row collapses by the hidden child's width"
    );
    assert_eq!(rect_of(&arena, a), (0.0, 0.0, 30.0, 20.0), "a unaffected");
    assert_eq!(
        rect_of(&arena, b),
        (0.0, 0.0, 0.0, 0.0),
        "the hidden child leaves the laid-out set (degenerate rect)"
    );
    assert_eq!(
        rect_of(&arena, c),
        (30.0, 0.0, 30.0, 20.0),
        "c reflows into b's place"
    );

    // Switch back to member 0: b re-enters the laid-out set and the row grows
    // back — the reverse topology change.
    let mut txn = arena.open();
    txn.set_variant(set, 0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect_of(&arena, row), (0.0, 0.0, 90.0, 20.0), "row restored");
    assert_eq!(
        rect_of(&arena, b),
        (30.0, 0.0, 30.0, 20.0),
        "b re-enters the set"
    );
    assert_eq!(
        rect_of(&arena, c),
        (60.0, 0.0, 30.0, 20.0),
        "c back to last"
    );
}
