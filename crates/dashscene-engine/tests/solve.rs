//! Story #9 acceptance: representative H/V, hug-in-fill, fill-weight,
//! and min/max cases against hand-computed rects (issue #9;
//! DESIGN_1.md §7.1).

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
