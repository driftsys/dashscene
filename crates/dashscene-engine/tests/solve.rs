//! Story #9 acceptance: representative H/V, hug-in-fill, fill-weight,
//! and min/max cases against hand-computed rects (issue #9;
//! docs/design/architecture.md). Story #43 (v0.8) adds the wrap, grid-span,
//! baseline, and negative-margin-hug cases, verified against the
//! Figma-captured fixtures' boxes where a capture exists.

use dashscene_core::{
    Arena, AxisSizing, CrossAxisAlign, GridTrack, LayoutMode, MainAxisAlign, NodeId, Prop,
};
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

/// Shorthand for the margin shape most scenes here author: a leading
/// left margin only — the negative-gap lowering's output (debt #115).
fn margin_left(txn: &mut dashscene_core::Txn<'_>, node: NodeId, left: f32) {
    txn.set_prop(
        node,
        Prop::Margin {
            left,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
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
fn a_hidden_mask_does_not_mask_under_the_taffy_solver() {
    // Story #44 M2 (Taffy path): a hidden mask lowers to Display::None and
    // resolves 0x0. Without honoring its visibility, that 0x0 box would clip
    // every following sibling to nothing; honoring it, the sibling is
    // unclipped. The fixed-solver half is in dashscene-core's arena tests.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    // A passthrough parent so the sibling keeps its authored position.
    let parent = txn.add_node(None, None);
    txn.set_prop(parent, Prop::Width(100.0));
    txn.set_prop(parent, Prop::Height(100.0));
    let mask = txn.add_node(Some(parent), None);
    txn.set_prop(mask, Prop::X(10.0));
    txn.set_prop(mask, Prop::Y(10.0));
    txn.set_prop(mask, Prop::Width(30.0));
    txn.set_prop(mask, Prop::Height(30.0));
    txn.set_prop(mask, Prop::Mask(true));
    txn.set_prop(mask, Prop::Visible(false));
    let after = txn.add_node(Some(parent), None);
    txn.set_prop(after, Prop::Width(80.0));
    txn.set_prop(after, Prop::Height(80.0));
    txn.set_prop(
        after,
        Prop::Fill(dashscene_core::Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    txn.commit_with(&mut TaffySolver::new());

    let scene = arena.committed();
    assert!(
        scene.clips().resolve(scene.rects()[2].clip).is_unclipped(),
        "M2: a hidden mask does not stencil its siblings under the Taffy solver",
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

#[test]
fn a_hug_row_over_negative_child_margins_sums_like_positive_ones() {
    // Debt #236: taffy 0.12's intrinsic (max-content) pass reconstructs
    // a shrink-0 item's contribution with a different scaled-shrink
    // factor than the one it divided by, so a negative main-axis margin
    // is amplified by the item's flex basis and the hug sum collapses.
    // The engine rebates the negative margin into the flex basis (and
    // floors the min size back at the authored size), so the sum is
    // plain arithmetic again. This is the issue's reproduction table:
    // two fixed 56-wide children, the second carrying a left margin.
    for (left, expected_width) in [(0.0, 112.0), (16.0, 128.0), (-1.0, 111.0), (-16.0, 96.0)] {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(56.0));
        fixed(&mut txn, row, 56.0, 56.0);
        let b = fixed(&mut txn, row, 56.0, 56.0);
        margin_left(&mut txn, b, left);
        txn.commit_with(&mut TaffySolver::new());

        assert_eq!(
            rect(&arena, 0).2,
            expected_width,
            "hug width with margin-left {left}"
        );
        // The children land where the margin puts them in both cases.
        assert_eq!(rect(&arena, 1), (0.0, 0.0, 56.0, 56.0));
        assert_eq!(rect(&arena, 2), (56.0 + left, 0.0, 56.0, 56.0));
    }
}

#[test]
fn the_rebate_respects_an_authored_max_alongside_a_negative_margin() {
    // Review finding R1: the rebate's min-size floor is the authored
    // size clamped by the authored max (spec D1). Without the cap, the
    // floor (56) beats the authored max (40) — taffy clamps min-wins —
    // and the negative margin GROWS the child from 40 to 56.
    let solve = |left: f32| {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::Width(200.0));
        txn.set_prop(row, Prop::Height(56.0));
        fixed(&mut txn, row, 56.0, 56.0);
        let b = fixed(&mut txn, row, 56.0, 56.0);
        txn.set_prop(b, Prop::MaxWidth(40.0));
        margin_left(&mut txn, b, left);
        txn.commit_with(&mut TaffySolver::new());
        rect(&arena, 2).2
    };

    assert_eq!(solve(0.0), 40.0, "the max clamps without a margin");
    assert_eq!(solve(-16.0), 40.0, "the max still clamps under the rebate");
}

#[test]
fn the_rebate_survives_a_padded_childs_basis_floor() {
    // Review finding R2: taffy floors a flex basis at the item's own
    // padding sum, which used to re-enter the broken shrink-0 branch
    // for a padded child (an overlapped padded card — the exact shape
    // the rebate exists for). The mapping anchors a floored basis at
    // padding + 1, where the branch's two scaled-shrink formulas agree,
    // so the sum stays exact on both sides of the old floor.
    let solve = |left: f32| {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(56.0));
        fixed(&mut txn, row, 56.0, 56.0);
        // The padded card: a fixed 30-wide container whose own
        // horizontal padding sums to 24.
        let card = txn.add_node(Some(row), None);
        txn.set_prop(card, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(card, Prop::Width(30.0));
        txn.set_prop(card, Prop::Height(56.0));
        txn.set_prop(
            card,
            Prop::Padding {
                left: 12.0,
                top: 0.0,
                right: 12.0,
                bottom: 0.0,
            },
        );
        margin_left(&mut txn, card, left);
        txn.commit_with(&mut TaffySolver::new());
        (rect(&arena, 0).2, rect(&arena, 2))
    };

    // At the old floor boundary (margin −6: 30 − 6 = 24 = the padding
    // sum) and beyond it (margin −10: 20 < 24), the hug width is
    // 56 + 30 + margin and the card keeps its authored size.
    assert_eq!(solve(-6.0), (80.0, (50.0, 0.0, 30.0, 56.0)));
    assert_eq!(solve(-10.0), (76.0, (46.0, 0.0, 30.0, 56.0)));
}

#[test]
fn a_deep_overlap_beyond_the_childs_own_width_still_sums_exactly() {
    // Review finding R3: a margin more negative than the child's own
    // width used to floor the rebated basis at zero and contribute
    // nothing. The padding+1 anchor keeps the contribution exact even
    // when it goes negative — the child pulls the hug sum below its
    // predecessor's edge, which is what the overlap means.
    let solve = |left: f32| {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(56.0));
        fixed(&mut txn, row, 56.0, 56.0);
        let b = fixed(&mut txn, row, 10.0, 56.0);
        margin_left(&mut txn, b, left);
        txn.commit_with(&mut TaffySolver::new());
        (rect(&arena, 0).2, rect(&arena, 2))
    };

    // 56 + 10 − 12 = 54; the child sits at 56 − 12 = 44.
    assert_eq!(solve(-12.0), (54.0, (44.0, 0.0, 10.0, 56.0)));
    // A contribution that goes negative (10 − 20) still sums: 46.
    assert_eq!(solve(-20.0), (46.0, (36.0, 0.0, 10.0, 56.0)));
}

#[test]
fn the_rebate_respects_an_authored_min_alongside_a_negative_margin() {
    // The #236 rebate floors the child's main-axis min size at its
    // authored size; an authored min above the size must still win
    // exactly as it does without the margin.
    let solve = |left: f32| {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(56.0));
        fixed(&mut txn, row, 56.0, 56.0);
        let b = fixed(&mut txn, row, 56.0, 56.0);
        txn.set_prop(b, Prop::MinWidth(70.0));
        margin_left(&mut txn, b, left);
        txn.commit_with(&mut TaffySolver::new());
        (rect(&arena, 0).2, rect(&arena, 2).2)
    };

    // The min widens the child to 70, margin or no: 56 + 70 = 126 flat,
    // minus the 16 overlap = 110.
    assert_eq!(solve(0.0), (126.0, 70.0));
    assert_eq!(solve(-16.0), (110.0, 70.0));
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
            margin_left(txn, child, -8.0);
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
    margin_left(&mut txn, b, 10.0);
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
        // Second inner row pulled back by the outer gap; second child of
        // each inner row pulled back by that row's own gap.
        margin_left(txn, inners[1].0, -10.0);
        for (_, [_, second]) in inners {
            margin_left(txn, second, -4.0);
        }
    });

    // DFS: outer, inner0, inner0.a, inner0.b, inner1, inner1.a, inner1.b.
    // inner0 at 0, its children at 0 and 30-4=26.
    // inner1 at 100-10=90, its children at 90 and 90+30-4=116.
    let expected = [0.0, 0.0, 0.0, 26.0, 90.0, 90.0, 116.0];
    assert_eq!(lowered, expected, "nested negative gap, lowered");
    assert_eq!(authored, expected, "equivalent hand-authored margins");
}

// ---------------------------------------------------------------------------
// v0.8 layout fidelity (story #43): wrap, grid with spans, baseline.
// ---------------------------------------------------------------------------

/// Build the wrap scene of `corpus/figma-fixtures/lowering-wrap.json` in
/// core vocabulary: a fixed-width, hug-height wrapping row of seven
/// fixed-size chips. `cross_gap` is the authored line spacing; `None`
/// leaves it following `gap`.
fn wrap_scene(cross_gap: Option<f32>) -> Arena {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(row, Prop::Width(420.0));
    txn.set_prop(row, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(row, Prop::Gap(12.0));
    if let Some(v) = cross_gap {
        txn.set_prop(row, Prop::CrossGap(v));
    }
    txn.set_prop(
        row,
        Prop::Padding {
            left: 16.0,
            top: 16.0,
            right: 16.0,
            bottom: 16.0,
        },
    );
    for w in [120.0, 80.0, 160.0, 100.0, 140.0, 90.0, 110.0] {
        fixed(&mut txn, row, w, 40.0);
    }
    txn.commit_with(&mut TaffySolver::new());
    arena
}

#[test]
fn a_wrap_row_breaks_lines_and_hugs_to_them_like_figmas_capture() {
    // The captured `lowering-wrap` scene, hand-computed: the inner width
    // is 420 − 2×16 = 388; chips greedily fill 120+12+80+12+160 = 384,
    // then 100+12+140+12+90 = 354, then 110. Lines sit 40 high with the
    // authored cross gap 16 between them, so the hug height is
    // 16+40+16+40+16+40+16 = 184. Every box equals the capture's
    // absoluteBoundingBox.
    let arena = wrap_scene(Some(16.0));
    assert_eq!(rect(&arena, 0), (0.0, 0.0, 420.0, 184.0), "hug root");
    let chips: [(f32, f32, f32); 7] = [
        (16.0, 16.0, 120.0),
        (148.0, 16.0, 80.0),
        (240.0, 16.0, 160.0),
        (16.0, 72.0, 100.0),
        (128.0, 72.0, 140.0),
        (280.0, 72.0, 90.0),
        (16.0, 128.0, 110.0),
    ];
    for (i, (x, y, w)) in chips.into_iter().enumerate() {
        assert_eq!(rect(&arena, i + 1), (x, y, w, 40.0), "chip {i}");
    }
}

#[test]
fn an_unset_cross_gap_follows_the_main_gap() {
    // Without an authored cross gap the line spacing is the main gap
    // (the v0.2 both-axes mapping, kept for wrap): lines at y = 16,
    // 16+40+12 = 68, 120; hug height 176.
    let arena = wrap_scene(None);
    assert_eq!(rect(&arena, 0).3, 176.0, "hug height at gap-spaced lines");
    assert_eq!(rect(&arena, 4).1, 68.0, "second line");
    assert_eq!(rect(&arena, 7).1, 120.0, "third line");
}

#[test]
fn a_grid_with_spans_solves_to_figmas_captured_rects() {
    // The captured `grid-basic` scene in core vocabulary: a fixed
    // 720×480 grid, padding 16, both gaps 12, tracks
    // rows [96px, 1fr, 1fr] and columns [160px, 1fr, 1fr] — so the
    // fraction columns take (720−32−24−160)/2 = 252 and the fraction
    // rows (480−32−24−96)/2 = 164. Children anchor to cells and span
    // tracks; every box equals the capture's absoluteBoundingBox.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let grid = txn.add_node(None, None);
    txn.set_prop(grid, Prop::Mode(LayoutMode::Grid));
    txn.set_prop(grid, Prop::Width(720.0));
    txn.set_prop(grid, Prop::Height(480.0));
    txn.set_prop(grid, Prop::Gap(12.0));
    txn.set_prop(grid, Prop::CrossGap(12.0));
    txn.set_prop(
        grid,
        Prop::Padding {
            left: 16.0,
            top: 16.0,
            right: 16.0,
            bottom: 16.0,
        },
    );
    txn.set_prop(
        grid,
        Prop::GridRows(vec![
            GridTrack::Fixed(96.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ]),
    );
    txn.set_prop(
        grid,
        Prop::GridColumns(vec![
            GridTrack::Fixed(160.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ]),
    );

    let place = |txn: &mut dashscene_core::Txn<'_>, node: NodeId, row: u16, column: u16| {
        txn.set_prop(node, Prop::GridRow(row));
        txn.set_prop(node, Prop::GridColumn(column));
    };
    let fill = |txn: &mut dashscene_core::Txn<'_>| {
        let node = txn.add_node(Some(grid), None);
        txn.set_prop(node, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(node, Prop::SizingV(AxisSizing::Fill));
        node
    };

    // span-3-cols: the full first row.
    let span_cols = fill(&mut txn);
    place(&mut txn, span_cols, 0, 0);
    txn.set_prop(span_cols, Prop::GridColumnSpan(3));
    // span-2-rows: the first column of the two fraction rows.
    let span_rows = fill(&mut txn);
    place(&mut txn, span_rows, 1, 0);
    txn.set_prop(span_rows, Prop::GridRowSpan(2));
    // fill-plain cells.
    let fill_a = fill(&mut txn);
    place(&mut txn, fill_a, 1, 1);
    let fill_b = fill(&mut txn);
    place(&mut txn, fill_b, 2, 2);
    // hug-content: a hug×hug row (padding 12/8) around fixed 50×17
    // content — it sits at its cell origin instead of stretching.
    let hug = txn.add_node(Some(grid), None);
    txn.set_prop(hug, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(
        hug,
        Prop::Padding {
            left: 12.0,
            top: 8.0,
            right: 12.0,
            bottom: 8.0,
        },
    );
    place(&mut txn, hug, 1, 2);
    fixed(&mut txn, hug, 50.0, 17.0);
    // fixed-size: keeps its authored size at its cell origin.
    let fixed_child = fixed(&mut txn, grid, 140.0, 60.0);
    place(&mut txn, fixed_child, 2, 1);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 720.0, 480.0), "grid root");
    assert_eq!(rect(&arena, 1), (16.0, 16.0, 688.0, 96.0), "span-3-cols");
    assert_eq!(rect(&arena, 2), (16.0, 124.0, 160.0, 340.0), "span-2-rows");
    assert_eq!(rect(&arena, 3), (188.0, 124.0, 252.0, 164.0), "fill-minmax");
    assert_eq!(rect(&arena, 4), (452.0, 300.0, 252.0, 164.0), "fill-plain");
    assert_eq!(rect(&arena, 5), (452.0, 124.0, 74.0, 33.0), "hug-content");
    assert_eq!(rect(&arena, 6), (464.0, 132.0, 50.0, 17.0), "hug me");
    assert_eq!(rect(&arena, 7), (188.0, 300.0, 140.0, 60.0), "fixed-size");
}

#[test]
fn a_mixed_size_baseline_row_aligns_on_flex_baselines() {
    // Q-4 (docs/technotes/open-questions.md): the mixed-size baseline
    // acceptance case, hand-computed from Taffy's baseline rules — a
    // leaf's baseline is its bottom edge (height), a nested row
    // propagates its first line's baseline (its child's offset plus
    // that child's baseline).
    //
    //   a 30×20 leaf            baseline 20
    //   b 40×48 leaf            baseline 48   (the row's max)
    //   c 60×40 nested row,     baseline 4 + 10 = 14
    //     padding-top 4, one 20×10 leaf
    //
    // Offsets from the line top: a at 48−20 = 28, b at 0, c at
    // 48−14 = 34 (its inner leaf at 38); x runs 0, 40, 90 with gap 10.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Width(200.0));
    txn.set_prop(row, Prop::Height(80.0));
    txn.set_prop(row, Prop::Gap(10.0));
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
    fixed(&mut txn, row, 30.0, 20.0);
    fixed(&mut txn, row, 40.0, 48.0);
    let nested = txn.add_node(Some(row), None);
    txn.set_prop(nested, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(nested, Prop::Width(60.0));
    txn.set_prop(nested, Prop::Height(40.0));
    txn.set_prop(
        nested,
        Prop::Padding {
            left: 0.0,
            top: 4.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
    fixed(&mut txn, nested, 20.0, 10.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1), (0.0, 28.0, 30.0, 20.0), "short leaf");
    assert_eq!(rect(&arena, 2), (40.0, 0.0, 40.0, 48.0), "tall leaf");
    assert_eq!(rect(&arena, 3), (90.0, 34.0, 60.0, 40.0), "nested row");
    assert_eq!(rect(&arena, 4), (90.0, 38.0, 20.0, 10.0), "nested leaf");
}

#[test]
fn baseline_in_a_vertical_container_degrades_to_start() {
    // Taffy computes baselines for rows only; in a column the Baseline
    // keyword falls back to flex-start. Pinned so the degradation is
    // chosen, not accidental (Q-4).
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(col, Prop::Width(40.0));
    txn.set_prop(col, Prop::Height(100.0));
    txn.set_prop(col, Prop::CrossAlign(CrossAxisAlign::Baseline));
    fixed(&mut txn, col, 20.0, 20.0);
    fixed(&mut txn, col, 30.0, 20.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1).0, 0.0, "start-aligned");
    assert_eq!(rect(&arena, 2).0, 0.0, "start-aligned");
}

#[test]
fn an_out_of_range_grid_anchor_does_not_panic() {
    // Review finding R5: the schema's anchors are ushort, and taffy's
    // line indices are i16 — the conversion must saturate, never
    // overflow (a debug-build panic at 32767, a wrapped end-counted
    // line above it). The load gate bounds anchors for documents; this
    // pins the engine's own hardening for direct producers.
    // 32767 is the debug-overflow boundary (`i16::MAX`), u16::MAX the
    // extreme; both saturate to the same line. (The solve pays for the
    // implicit tracks taffy materializes, so the list stays short.)
    for anchor in [32767u16, u16::MAX] {
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let grid = txn.add_node(None, None);
        txn.set_prop(grid, Prop::Mode(LayoutMode::Grid));
        txn.set_prop(grid, Prop::Width(100.0));
        txn.set_prop(grid, Prop::Height(100.0));
        txn.set_prop(
            grid,
            Prop::GridColumns(vec![GridTrack::Fraction(1.0), GridTrack::Fraction(1.0)]),
        );
        let child = fixed(&mut txn, grid, 10.0, 10.0);
        txn.set_prop(child, Prop::GridRow(anchor));
        txn.set_prop(child, Prop::GridColumn(anchor));
        // The contract under test is only "no panic, finite output":
        // the solved placement of a saturated anchor is degenerate by
        // construction, and the load gate refuses it for documents.
        txn.commit_with(&mut TaffySolver::new());
        let r = arena.committed().rects()[1];
        assert!(r.x.is_finite() && r.y.is_finite(), "finite at {anchor}");
    }
}

#[test]
fn a_zero_span_is_saturated_to_one() {
    // Review finding R6's engine half: a span of 0 (refused at the load
    // gate) must not reach taffy as Span(0) from a direct producer —
    // the engine floors it at 1.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let grid = txn.add_node(None, None);
    txn.set_prop(grid, Prop::Mode(LayoutMode::Grid));
    txn.set_prop(grid, Prop::Width(100.0));
    txn.set_prop(grid, Prop::Height(40.0));
    txn.set_prop(
        grid,
        Prop::GridColumns(vec![GridTrack::Fraction(1.0), GridTrack::Fraction(1.0)]),
    );
    let child = txn.add_node(Some(grid), None);
    txn.set_prop(child, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(child, Prop::SizingV(AxisSizing::Fill));
    txn.set_prop(child, Prop::GridRow(0));
    txn.set_prop(child, Prop::GridColumn(0));
    txn.set_prop(child, Prop::GridRowSpan(0));
    txn.set_prop(child, Prop::GridColumnSpan(0));
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1), (0.0, 0.0, 50.0, 40.0), "spans one cell");
}

#[test]
fn a_fixed_child_larger_than_its_fraction_cell_overflows_it() {
    // Review finding R8, pinned as chosen behavior with a named open
    // question (docs/decisions/v08-layout-vocabulary-shape.md): a
    // Fraction track is minmax(0, fr) — it divides free space and never
    // grows for content — so a fixed child larger than its cell keeps
    // its authored size and overflows into the neighbor cell. Figma's
    // reference behavior for this combination is uncaptured; revisit at
    // story #264 when real grid captures exist.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let grid = txn.add_node(None, None);
    txn.set_prop(grid, Prop::Mode(LayoutMode::Grid));
    txn.set_prop(grid, Prop::Width(100.0));
    txn.set_prop(grid, Prop::Height(100.0));
    txn.set_prop(
        grid,
        Prop::GridColumns(vec![GridTrack::Fraction(1.0), GridTrack::Fraction(1.0)]),
    );
    let big = fixed(&mut txn, grid, 80.0, 40.0);
    txn.set_prop(big, Prop::GridRow(0));
    txn.set_prop(big, Prop::GridColumn(0));
    txn.commit_with(&mut TaffySolver::new());

    // Each fraction column is 50 wide; the 80-wide child overflows 30
    // into the second column rather than growing its track.
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 80.0, 40.0));
}

#[test]
fn distinct_gaps_and_asymmetric_sizing_map_to_their_own_grid_axes() {
    // Review findings R9/R10 (regression armor): the column gap (8) and
    // row gap (20) differ, so an axis swap in the gap mapping moves
    // every second-row/second-column cell; and each child sizes Fill on
    // one axis only, so a justify_self/align_self swap changes its box.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let grid = txn.add_node(None, None);
    txn.set_prop(grid, Prop::Mode(LayoutMode::Grid));
    txn.set_prop(grid, Prop::Width(148.0));
    txn.set_prop(grid, Prop::Height(110.0));
    txn.set_prop(grid, Prop::Gap(8.0));
    txn.set_prop(grid, Prop::CrossGap(20.0));
    txn.set_prop(
        grid,
        Prop::GridColumns(vec![GridTrack::Fixed(30.0), GridTrack::Fixed(40.0)]),
    );
    txn.set_prop(
        grid,
        Prop::GridRows(vec![GridTrack::Fixed(20.0), GridTrack::Fixed(25.0)]),
    );

    // Fill across, fixed 10 high: stretches to the 30-wide cell only.
    let wide = txn.add_node(Some(grid), None);
    txn.set_prop(wide, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(wide, Prop::Height(10.0));
    txn.set_prop(wide, Prop::GridRow(0));
    txn.set_prop(wide, Prop::GridColumn(0));
    // Fixed 15 wide, Fill down: stretches to the 20-high cell only.
    let tall = txn.add_node(Some(grid), None);
    txn.set_prop(tall, Prop::Width(15.0));
    txn.set_prop(tall, Prop::SizingV(AxisSizing::Fill));
    txn.set_prop(tall, Prop::GridRow(0));
    txn.set_prop(tall, Prop::GridColumn(1));
    // Fill on both axes in the second row and column.
    let both = txn.add_node(Some(grid), None);
    txn.set_prop(both, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(both, Prop::SizingV(AxisSizing::Fill));
    txn.set_prop(both, Prop::GridRow(1));
    txn.set_prop(both, Prop::GridColumn(1));
    txn.commit_with(&mut TaffySolver::new());

    // Column x runs 0, 30+8 = 38; row y runs 0, 20+20 = 40.
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 30.0, 10.0), "fill across");
    assert_eq!(rect(&arena, 2), (38.0, 0.0, 15.0, 20.0), "fill down");
    assert_eq!(rect(&arena, 3), (38.0, 40.0, 40.0, 25.0), "fill both");
}

#[test]
fn a_fixed_height_wrap_container_packs_its_lines_at_the_cross_start() {
    // Review finding R11: the align_content FlexStart mapping is inert
    // in a hug-height wrap container (the lines define the height), so
    // this scene fixes the height. Packed lines sit at y = 0 and
    // y = 30 + 10; taffy's default (stretch) would spread them over the
    // 200-high container instead.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(row, Prop::Width(100.0));
    txn.set_prop(row, Prop::Height(200.0));
    txn.set_prop(row, Prop::CrossGap(10.0));
    fixed(&mut txn, row, 60.0, 30.0);
    fixed(&mut txn, row, 60.0, 30.0);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 1), (0.0, 0.0, 60.0, 30.0), "first line");
    assert_eq!(
        rect(&arena, 2),
        (0.0, 40.0, 60.0, 30.0),
        "packed, not spread"
    );
}
