//! Incremental commit acceptance (issue #164): a retained `TaffySolver`
//! re-solves only what changed, a paint-only commit skips the solve
//! entirely, and an incremental solve lands on the same rects a fresh
//! build would.

use dashscene_core::{Arena, AxisSizing, Color, LayoutMode, NodeId, Prop};
use dashscene_engine::TaffySolver;

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

#[test]
fn a_paint_only_commit_performs_no_layout_solve() {
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let node = {
        let mut txn = arena.open();
        let node = txn.add_node(None, None);
        txn.set_prop(node, Prop::Width(20.0));
        txn.set_prop(node, Prop::Height(20.0));
        txn.set_prop(node, Prop::Fill(RED));
        txn.commit_with(&mut solver);
        node
    };
    let after_build = solver.solves();
    assert!(
        after_build >= 1,
        "the first commit builds and solves the tree"
    );

    // A paint-only change touches no layout intent, so the retained tree
    // is not recomputed at all.
    let mut txn = arena.open();
    txn.set_prop(node, Prop::Fill(BLUE));
    txn.commit_with(&mut solver);
    assert_eq!(
        solver.solves(),
        after_build,
        "a paint-only commit must not run a layout solve"
    );
    // The fill change still reaches the committed output.
    assert_eq!(
        arena.committed().dirty(),
        [0],
        "the recolored rect is still marked dirty"
    );

    // A geometry change does solve.
    let mut txn = arena.open();
    txn.set_prop(node, Prop::Width(30.0));
    txn.commit_with(&mut solver);
    assert!(
        solver.solves() > after_build,
        "a layout change runs a solve"
    );
    assert_eq!(arena.committed().rects()[0].w, 30.0);
}

#[test]
fn resizing_a_clipping_frame_dirties_its_children_incrementally() {
    // The incremental clip cascade: resizing a clipping frame changes its
    // children's resolved clip region even though the children themselves
    // do not move, and the retained solver reports only the frame. Commit
    // must still mark the children dirty (issue #164). The frame is
    // mode-None so the child keeps its authored offset — the solver
    // genuinely omits it, exercising the carry-forward path.
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let frame = {
        let mut txn = arena.open();
        let frame = txn.add_node(None, None);
        txn.set_prop(frame, Prop::Width(20.0));
        txn.set_prop(frame, Prop::Height(20.0));
        txn.set_prop(frame, Prop::Clip(true));
        let child = txn.add_node(Some(frame), None);
        txn.set_prop(child, Prop::Width(50.0));
        txn.set_prop(child, Prop::Height(50.0));
        txn.set_prop(child, Prop::Fill(RED));
        txn.commit_with(&mut solver);
        frame
    };

    let mut txn = arena.open();
    txn.set_prop(frame, Prop::Width(10.0));
    txn.commit_with(&mut solver);

    // DFS: frame=0, child=1. The frame resized and the child's clip
    // region moved with it.
    assert_eq!(
        arena.committed().dirty(),
        [0, 1],
        "the frame and the child whose clip region it changed"
    );
}

/// Build a 200-wide horizontal row of one fixed child and one fill child.
/// When `incremental` is set, the fixed child is first solved at 40 wide
/// and then changed to `fixed_width` through a second commit on the same
/// retained solver; otherwise it is built at `fixed_width` directly.
/// Returns every node's absolute rect.
fn row_rects(fixed_width: f32, incremental: bool) -> Vec<(f32, f32, f32, f32)> {
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let fixed: NodeId = {
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Width(200.0));
        txn.set_prop(row, Prop::Height(30.0));
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        let fixed = txn.add_node(Some(row), None);
        txn.set_prop(
            fixed,
            Prop::Width(if incremental { 40.0 } else { fixed_width }),
        );
        txn.set_prop(fixed, Prop::Height(30.0));
        let fill = txn.add_node(Some(row), None);
        txn.set_prop(fill, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(fill, Prop::Height(30.0));
        txn.commit_with(&mut solver);
        fixed
    };

    if incremental {
        let mut txn = arena.open();
        txn.set_prop(fixed, Prop::Width(fixed_width));
        txn.commit_with(&mut solver);
    }

    arena
        .committed()
        .rects()
        .iter()
        .map(|r| (r.x, r.y, r.w, r.h))
        .collect()
}

#[test]
fn an_incremental_reflow_matches_a_fresh_build() {
    // Growing the fixed child from 40 to 70 shrinks the fill sibling from
    // 160 to 130 and shifts it right. The pruned readback must report the
    // reflowed sibling even though only the fixed child's intent changed,
    // so the incremental result equals a fresh build at the final width.
    let incremental = row_rects(70.0, true);
    let fresh = row_rects(70.0, false);
    assert_eq!(incremental, fresh, "incremental reflow == fresh build");
    // Pin the expected geometry so the equality is not vacuous.
    assert_eq!(
        incremental,
        vec![
            (0.0, 0.0, 200.0, 30.0),
            (0.0, 0.0, 70.0, 30.0),
            (70.0, 0.0, 130.0, 30.0),
        ],
    );
}
