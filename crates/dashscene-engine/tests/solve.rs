//! Story #9 acceptance: representative H/V, hug-in-fill, fill-weight,
//! and min/max cases against hand-computed rects (issue #9;
//! docs/design/architecture.md).

use dashscene_core::{Arena, AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop};
use dashscene_engine::TaffySolver;

/// Shorthand: rect (x, y, w, h) of the DFS index `i`.
fn rect(arena: &Arena, i: usize) -> (f32, f32, f32, f32) {
    let r = arena.committed().rects()[i];
    (r.x, r.y, r.w, r.h)
}

fn fixed(txn: &mut dashscene_core::Txn<'_>, parent: NodeId, w: f32, h: f32) -> NodeId {
    let id = txn.add_node(Some(parent), None);
    txn.set_prop(id, Prop::Width(w));
    txn.set_prop(id, Prop::Height(h));
    id
}

#[test]
fn a_horizontal_row_places_fixed_children_with_gap_and_padding() {
    // row 200×40, padding (10,10,10,10), gap 5:
    //   a 30×20 fixed at (10,10); b 50×20 fixed at (45,10).
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, Some("row"));
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(40.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(5.0));
    txn.set_prop(
        row,
        Prop::Padding {
            left: 10.0,
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
        },
    );
    fixed(&mut txn, row, 30.0, 20.0);
    fixed(&mut txn, row, 50.0, 20.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 200.0, 40.0));
    assert_eq!(rect(&arena, 1), (10.0, 10.0, 30.0, 20.0));
    assert_eq!(rect(&arena, 2), (45.0, 10.0, 50.0, 20.0));
}

#[test]
fn mode_none_trees_match_the_fixed_commit_exactly() {
    // The passthrough guarantee: a nested fixed-geometry tree solved
    // by Taffy equals commit()'s own fixed resolution.
    let build = |arena: &mut Arena, taffy: bool| {
        let mut txn = arena.open();
        let root = txn.add_node(None, None);
        txn.set_prop(root, Prop::X(10.0));
        txn.set_prop(root, Prop::Y(20.0));
        txn.set_prop(root, Prop::Width(300.0));
        txn.set_prop(root, Prop::Height(200.0));
        let a = txn.add_node(Some(root), None);
        txn.set_prop(a, Prop::X(5.0));
        txn.set_prop(a, Prop::Y(6.0));
        txn.set_prop(a, Prop::Width(50.0));
        txn.set_prop(a, Prop::Height(40.0));
        let leaf = txn.add_node(Some(a), None);
        txn.set_prop(leaf, Prop::X(1.0));
        txn.set_prop(leaf, Prop::Y(2.0));
        txn.set_prop(leaf, Prop::Width(10.0));
        txn.set_prop(leaf, Prop::Height(10.0));
        if taffy {
            txn.commit_with(&mut TaffySolver::new());
        } else {
            txn.commit();
        }
    };
    let mut via_taffy = Arena::new();
    build(&mut via_taffy, true);
    let mut via_fixed = Arena::new();
    build(&mut via_fixed, false);

    assert_eq!(via_taffy.committed().rects(), via_fixed.committed().rects());
}

#[test]
fn fill_children_split_free_space_and_a_fixed_sibling_keeps_its_size() {
    // row 200×30, no gap/padding: fixed 40 + two Fill children ->
    // each Fill gets (200-40)/2 = 80.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(30.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    let a = fixed(&mut txn, row, 40.0, 30.0);
    let b = txn.add_node(Some(row), None);
    txn.set_prop(b, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(b, Prop::Height(30.0));
    let c = txn.add_node(Some(row), None);
    txn.set_prop(c, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(c, Prop::Height(30.0));
    txn.commit_with(&mut TaffySolver::new());

    let _ = (a, b, c);
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 40.0, 30.0));
    assert_eq!(rect(&arena, 2), (40.0, 0.0, 80.0, 30.0));
    assert_eq!(rect(&arena, 3), (120.0, 0.0, 80.0, 30.0));
}

#[test]
fn a_hug_container_sizes_to_its_children_not_the_free_space() {
    // row 300×50: [hug column [fixed 40×10, fixed 25×10], fill].
    // The hug column takes max(40,25)=40 wide; the fill child takes
    // the remaining 260.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(300.0));
    txn.set_prop(row, Prop::Height(50.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    let hug = txn.add_node(Some(row), None);
    txn.set_prop(hug, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::SizingV(AxisSizing::Hug));
    fixed(&mut txn, hug, 40.0, 10.0);
    fixed(&mut txn, hug, 25.0, 10.0);
    let fill = txn.add_node(Some(row), None);
    txn.set_prop(fill, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(fill, Prop::Height(50.0));
    txn.commit_with(&mut TaffySolver::new());

    let _ = fill;
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 40.0, 20.0), "hug column");
    assert_eq!(rect(&arena, 2), (0.0, 0.0, 40.0, 10.0), "first fixed");
    assert_eq!(rect(&arena, 3), (0.0, 10.0, 25.0, 10.0), "second fixed");
    assert_eq!(rect(&arena, 4), (40.0, 0.0, 260.0, 50.0), "fill sibling");
}

#[test]
fn a_column_spreads_with_space_between_and_centers_cross_axis() {
    // column 100×90, SpaceBetween/Center: two 20×20 children at
    // y=0 and y=70; centered x=40.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Width(100.0));
    txn.set_prop(col, Prop::Height(90.0));
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(col, Prop::MainAlign(MainAxisAlign::SpaceBetween));
    txn.set_prop(col, Prop::CrossAlign(CrossAxisAlign::Center));
    fixed(&mut txn, col, 20.0, 20.0);
    fixed(&mut txn, col, 20.0, 20.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1), (40.0, 0.0, 20.0, 20.0));
    assert_eq!(rect(&arena, 2), (40.0, 70.0, 20.0, 20.0));
}

#[test]
fn min_and_max_constraints_clamp_fill_and_hug() {
    // row 200×30: a Fill child capped at max_width 50 and a second
    // Fill child taking the rest (150).
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(30.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    let capped = txn.add_node(Some(row), None);
    txn.set_prop(capped, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(capped, Prop::MaxWidth(50.0));
    txn.set_prop(capped, Prop::Height(30.0));
    let rest = txn.add_node(Some(row), None);
    txn.set_prop(rest, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(rest, Prop::Height(30.0));
    txn.commit_with(&mut TaffySolver::new());

    let _ = (capped, rest);
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 50.0, 30.0), "capped fill");
    assert_eq!(rect(&arena, 2), (50.0, 0.0, 150.0, 30.0), "remaining fill");

    // A Hug column floored by min_height: one 10-high child, min 25.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Width(50.0));
    txn.set_prop(col, Prop::Height(50.0));
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    let hug = txn.add_node(Some(col), None);
    txn.set_prop(hug, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(hug, Prop::MinHeight(25.0));
    fixed(&mut txn, hug, 10.0, 10.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1).3, 25.0, "hug floored by min_height");
}

#[test]
fn fractional_geometry_passes_through_unrounded() {
    // R7: the solver is an f32 passthrough — Taffy's default
    // whole-pixel rounding must be off, or fractional authored
    // geometry (and fractional Fill splits) shifts by up to a pixel.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::X(10.5));
    txn.set_prop(root, Prop::Y(20.25));
    txn.set_prop(root, Prop::Width(300.5));
    txn.set_prop(root, Prop::Height(200.25));
    let child = txn.add_node(Some(root), None);
    txn.set_prop(child, Prop::X(5.5));
    txn.set_prop(child, Prop::Y(6.25));
    txn.set_prop(child, Prop::Width(50.5));
    txn.set_prop(child, Prop::Height(40.25));
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (10.5, 20.25, 300.5, 200.25));
    assert_eq!(rect(&arena, 1), (16.0, 26.5, 50.5, 40.25));

    // Three Fill children split 100 into exact thirds, not 33/34/33.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(100.0));
    txn.set_prop(row, Prop::Height(10.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    for _ in 0..3 {
        let c = txn.add_node(Some(row), None);
        txn.set_prop(c, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(c, Prop::Height(10.0));
    }
    txn.commit_with(&mut TaffySolver::new());
    // The exact bits depend on Taffy's evaluation order; the contract
    // is that the split is unrounded (not 33/34/33) and lossless.
    let widths: Vec<f32> = (1..4).map(|i| rect(&arena, i).2).collect();
    for w in &widths {
        assert!((w - 100.0 / 3.0).abs() < 1e-3, "unrounded third, got {w}");
        assert_ne!(w.fract(), 0.0, "must not round to whole pixels");
    }
    let total: f32 = widths.iter().sum();
    assert!((total - 100.0).abs() < 1e-3, "split sums back, got {total}");
}

#[test]
fn hug_under_a_none_parent_wraps_content_not_the_authored_size() {
    // Under a mode-None parent, Fixed sizing reproduces the fixed
    // resolve; Hug keeps its content-wrapping meaning (a hug group
    // inside a plain frame). A childless Hug node therefore sizes to
    // zero — authored width/height only feed Fixed sizing. Pinned so
    // the behavior is chosen, not accidental; the fixed-commit
    // equivalence guarantee applies to fixed-sized trees.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::Width(300.0));
    txn.set_prop(root, Prop::Height(200.0));
    let hug = txn.add_node(Some(root), None);
    txn.set_prop(hug, Prop::X(5.0));
    txn.set_prop(hug, Prop::Y(6.0));
    txn.set_prop(hug, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::SizingV(AxisSizing::Hug));
    fixed(&mut txn, hug, 40.0, 10.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1), (5.0, 6.0, 40.0, 10.0), "hug wraps content");
}

#[test]
fn hiding_a_child_collapses_a_hug_container_and_reflows_its_siblings() {
    // Hug row: three fixed 30x20 children, no gap. Hiding the middle
    // child (issue #165) lowers to Taffy Display::None: the row's hug
    // width drops by 30, the hidden child resolves to a degenerate
    // rect, and its sibling closes into its place.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::Height(20.0));
    let a = fixed(&mut txn, row, 30.0, 20.0);
    let b = fixed(&mut txn, row, 30.0, 20.0);
    let c = fixed(&mut txn, row, 30.0, 20.0);
    txn.set_prop(b, Prop::Visible(false));
    txn.commit_with(&mut TaffySolver::new());

    let _ = (a, b, c);
    assert_eq!(
        rect(&arena, 0).2,
        60.0,
        "container collapses by the hidden child's width"
    );
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 30.0, 20.0), "a unaffected");
    assert_eq!(
        rect(&arena, 2),
        (0.0, 0.0, 0.0, 0.0),
        "hidden child resolves to a degenerate rect"
    );
    assert_eq!(
        rect(&arena, 3),
        (30.0, 0.0, 30.0, 20.0),
        "c reflows into b's place"
    );
}

#[test]
fn hiding_a_container_hides_its_whole_subtree_regardless_of_a_descendants_own_visible() {
    // Taffy's Display::None hides descendants during layout regardless
    // of their own style (issue #165) — a grandchild with no Visible
    // prop of its own (default true) still resolves degenerate under a
    // hidden ancestor.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::Height(20.0));
    let a = fixed(&mut txn, row, 30.0, 20.0);
    let hidden = txn.add_node(Some(row), None);
    txn.set_prop(hidden, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(hidden, Prop::Width(30.0));
    txn.set_prop(hidden, Prop::Height(20.0));
    txn.set_prop(hidden, Prop::Visible(false));
    let grandchild = fixed(&mut txn, hidden, 10.0, 10.0);
    txn.commit_with(&mut TaffySolver::new());

    let _ = (a, grandchild);
    assert_eq!(
        rect(&arena, 0).2,
        30.0,
        "container collapses; the hidden subtree contributes nothing"
    );
    assert_eq!(
        rect(&arena, 2),
        (0.0, 0.0, 0.0, 0.0),
        "hidden container resolves to a degenerate rect"
    );
    assert_eq!(
        rect(&arena, 3),
        (0.0, 0.0, 0.0, 0.0),
        "grandchild is hidden by its ancestor despite its own Visible defaulting to true"
    );
}

#[test]
fn multiple_roots_keep_their_authored_origins() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let first = txn.add_node(None, None);
    txn.set_prop(first, Prop::Width(50.0));
    txn.set_prop(first, Prop::Height(50.0));
    let second = txn.add_node(None, None);
    txn.set_prop(second, Prop::X(400.0));
    txn.set_prop(second, Prop::Y(300.0));
    txn.set_prop(second, Prop::Width(60.0));
    txn.set_prop(second, Prop::Height(60.0));
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 50.0, 50.0));
    assert_eq!(rect(&arena, 1), (400.0, 300.0, 60.0, 60.0));
}

/// Build a horizontal row of three fixed 30x20 children, letting
/// `configure` set the container gap / margins. Returns the solved
/// child x-positions.
fn row_child_xs(
    configure: impl FnOnce(&mut dashscene_core::Txn<'_>, NodeId, [NodeId; 3]),
) -> Vec<f32> {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(20.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    let a = fixed(&mut txn, row, 30.0, 20.0);
    let b = fixed(&mut txn, row, 30.0, 20.0);
    let c = fixed(&mut txn, row, 30.0, 20.0);
    configure(&mut txn, row, [a, b, c]);
    txn.commit_with(&mut TaffySolver::new());
    (1..4).map(|i| rect(&arena, i).0).collect()
}

#[test]
fn a_negative_gap_scene_lowers_to_the_same_rects_as_the_margin_scene() {
    // Scene A: negative gap, then lowered.
    let scene_a = row_child_xs(|txn, row, _| {
        txn.set_prop(row, Prop::Gap(-8.0));
        txn.lower_negative_gaps();
    });
    // Scene B: the equivalent margin-based scene, authored directly.
    let scene_b = row_child_xs(|txn, _row, [_a, b, c]| {
        for child in [b, c] {
            txn.set_prop(
                child,
                Prop::Margin {
                    left: -8.0,
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                },
            );
        }
    });

    // Each child overlaps its predecessor by 8: 0, 30-8=22, 52-8=44.
    //
    // Pin both sides independently instead of comparing scene_a to
    // scene_b: lower_negative_gaps() sets exactly the margin the
    // closure above sets by hand, so by the time either scene reaches
    // the solver both hold bit-identical margins. `scene_a == scene_b`
    // would then hold no matter what the solver did with a negative
    // margin — it only proves the solver is deterministic, not correct
    // (issue #114). Pinning each side against the hand-computed
    // expectation makes a wrong lowering or a wrong solve fail here.
    let expected = [0.0, 22.0, 44.0];
    assert_eq!(scene_a, expected, "negative-gap scene, lowered");
    assert_eq!(scene_b, expected, "equivalent hand-authored margins");
}

#[test]
fn a_vertical_negative_gap_column_overlaps_on_the_main_axis() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Width(30.0));
    txn.set_prop(col, Prop::Height(200.0));
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(col, Prop::Gap(-5.0));
    fixed(&mut txn, col, 30.0, 20.0);
    fixed(&mut txn, col, 30.0, 20.0);
    txn.lower_negative_gaps();
    txn.commit_with(&mut TaffySolver::new());

    // y: 0, then 20-5=15.
    assert_eq!(rect(&arena, 1).1, 0.0);
    assert_eq!(rect(&arena, 2).1, 15.0);
}

#[test]
fn authored_margins_solve_without_any_lowering() {
    // Margin is real standalone vocabulary, not only a lowering
    // artifact: a positive margin pushes a child along the main axis.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(20.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    fixed(&mut txn, row, 30.0, 20.0);
    let b = fixed(&mut txn, row, 30.0, 20.0);
    txn.set_prop(
        b,
        Prop::Margin {
            left: 10.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
    txn.commit_with(&mut TaffySolver::new());

    // a at 0..30; b pushed by margin-left 10 to x=40.
    assert_eq!(rect(&arena, 1).0, 0.0);
    assert_eq!(rect(&arena, 2).0, 40.0);
}

#[test]
fn a_margin_under_a_passthrough_parent_does_not_shift_the_child() {
    // Margin is flex-flow vocabulary; under a mode-None (passthrough)
    // parent, placement is the authored offset and margin is inert —
    // so TaffySolver must agree with commit()'s FixedSolver, which
    // ignores margin. (The lowering never margins passthrough
    // children, but a producer may author one directly.)
    let build = |arena: &mut Arena, taffy: bool| {
        let mut txn = arena.open();
        let root = txn.add_node(None, None);
        txn.set_prop(root, Prop::Width(300.0));
        txn.set_prop(root, Prop::Height(200.0));
        // root mode defaults to None (passthrough).
        let child = txn.add_node(Some(root), None);
        txn.set_prop(child, Prop::X(10.0));
        txn.set_prop(child, Prop::Y(10.0));
        txn.set_prop(child, Prop::Width(40.0));
        txn.set_prop(child, Prop::Height(30.0));
        txn.set_prop(
            child,
            Prop::Margin {
                left: 5.0,
                top: 6.0,
                right: 0.0,
                bottom: 0.0,
            },
        );
        if taffy {
            txn.commit_with(&mut TaffySolver::new());
        } else {
            txn.commit();
        }
    };
    let mut via_taffy = Arena::new();
    build(&mut via_taffy, true);
    let mut via_fixed = Arena::new();
    build(&mut via_fixed, false);

    assert_eq!(via_taffy.committed().rects(), via_fixed.committed().rects());
    // The child sits at its authored offset, margin ignored.
    assert_eq!(rect(&via_taffy, 1), (10.0, 10.0, 40.0, 30.0));
}

/// Build the nested scene: an outer row of two inner rows, each inner
/// row holding two fixed children. `configure` receives the outer row
/// and, per inner row, the row and its two children. Returns the solved
/// absolute x of every node, in DFS order.
fn nested_row_xs(
    configure: impl FnOnce(&mut dashscene_core::Txn<'_>, NodeId, [(NodeId, [NodeId; 2]); 2]),
) -> Vec<f32> {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let outer = txn.add_node(None, None);
    txn.set_prop(outer, Prop::Width(400.0));
    txn.set_prop(outer, Prop::Height(40.0));
    txn.set_prop(outer, Prop::Mode(LayoutMode::Horizontal));

    let inners = [(); 2].map(|()| {
        let inner = txn.add_node(Some(outer), None);
        txn.set_prop(inner, Prop::Width(100.0));
        txn.set_prop(inner, Prop::Height(20.0));
        txn.set_prop(inner, Prop::Mode(LayoutMode::Horizontal));
        let a = fixed(&mut txn, inner, 30.0, 20.0);
        let b = fixed(&mut txn, inner, 30.0, 20.0);
        (inner, [a, b])
    });
    configure(&mut txn, outer, inners);
    txn.commit_with(&mut TaffySolver::new());
    (0..arena.committed().rects().len())
        .map(|i| rect(&arena, i).0)
        .collect()
}

#[test]
fn lowered_margins_compose_through_nesting() {
    // Story #10's acceptance criterion (a negative-gap scene solves to
    // the same rects as the equivalent margin scene) extended to depth:
    // a negative-gap row nested inside a negative-gap row.
    //
    // Note what this does and does not pin. Taffy takes `gap` as a raw
    // length and applies a negative one arithmetically, so the rects
    // alone cannot witness that the lowering ran — an un-lowered scene
    // solves identically. That the lowering rewrites containers at every
    // depth is pinned at the intent level, in dashscene-core's
    // `lower_negative_gaps_reaches_containers_at_every_depth`. What this
    // test pins is the engine half: nested negative margins compose
    // correctly through the Taffy style mapping, so the lowering's
    // output is faithfully solved.
    //
    // `lowered` and `authored` are pinned independently below rather
    // than compared to each other: lower_negative_gaps() sets exactly
    // the margins the closure below sets by hand, so both scenes reach
    // the solver holding bit-identical margins. `lowered == authored`
    // would then hold no matter what the solver did with those margins
    // — it only proves the solver is deterministic, not correct (issue
    // #114).
    let lowered = nested_row_xs(|txn, outer, inners| {
        txn.set_prop(outer, Prop::Gap(-10.0));
        for (inner, _) in inners {
            txn.set_prop(inner, Prop::Gap(-4.0));
        }
        txn.lower_negative_gaps();
    });
    // The same scene with every lowered margin authored by hand.
    let authored = nested_row_xs(|txn, _outer, inners| {
        let pull = |txn: &mut dashscene_core::Txn<'_>, node: NodeId, left: f32| {
            txn.set_prop(
                node,
                Prop::Margin {
                    left,
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                },
            );
        };
        // Second inner row pulled back by the outer gap; second child of
        // each inner row pulled back by that row's own gap.
        pull(txn, inners[1].0, -10.0);
        for (_, [_, second]) in inners {
            pull(txn, second, -4.0);
        }
    });

    // DFS: outer, inner0, inner0.a, inner0.b, inner1, inner1.a, inner1.b.
    // inner0 at 0, its children at 0 and 30-4=26.
    // inner1 at 100-10=90, its children at 90 and 90+30-4=116.
    let expected = [0.0, 0.0, 0.0, 26.0, 90.0, 90.0, 116.0];
    assert_eq!(lowered, expected, "nested negative gap, lowered");
    assert_eq!(authored, expected, "equivalent hand-authored margins");
}
