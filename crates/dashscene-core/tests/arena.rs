//! Story #2 acceptance path: a scene built by hand through the staged
//! mutation API reads back as a resolved rect table + paint table
//! (issue #2; docs/design/architecture.md;
//! docs/decisions/staged-mutation-v01-scope.md).

use std::mem::{align_of, size_of};

use dashscene_core::{
    Arena, ClipBox, ClipIndex, Color, CornerRadii, LayoutMode, PaintEntry, PaintIndex, Prop,
    RectEntry, TextStyle,
};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[test]
fn committed_entries_are_blittable_plain_data() {
    // Boundary B pins the rect entry as blittable plain data:
    // x, y, w, h (f32) + paint index (u32) + clip index (u32), and the
    // solid-fill color as 4xf32 RGBA (dashbuf's Color shape).
    assert_eq!(size_of::<RectEntry>(), 24);
    assert_eq!(align_of::<RectEntry>(), 4);
    assert_eq!(size_of::<Color>(), 16);
    assert_eq!(align_of::<Color>(), 4);

    let entry = RectEntry {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        paint: PaintIndex(0),
        clip: ClipIndex::UNCLIPPED,
    };
    let copy = entry; // Copy, not a move
    assert_eq!(entry, copy);
}

#[test]
fn a_new_arena_commits_to_an_empty_scene() {
    let mut arena = Arena::new();
    assert_eq!(arena.committed().generation(), 0);
    assert!(arena.committed().rects().is_empty());

    let generation = arena.open().commit();

    assert_eq!(generation, 1);
    let scene = arena.committed();
    assert_eq!(scene.generation(), 1);
    assert!(scene.rects().is_empty());
    assert!(scene.paints().is_empty());
    assert!(scene.dirty().is_empty());
}

#[test]
fn a_single_filled_root_resolves_to_one_rect_and_one_paint() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::X(5.0));
    txn.set_prop(root, Prop::Y(7.0));
    txn.set_prop(root, Prop::Width(320.0));
    txn.set_prop(root, Prop::Height(240.0));
    txn.set_prop(root, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    assert_eq!(
        scene.rects(),
        &[RectEntry {
            x: 5.0,
            y: 7.0,
            w: 320.0,
            h: 240.0,
            paint: PaintIndex(0),
            clip: ClipIndex::UNCLIPPED,
        }]
    );
    assert_eq!(scene.paints().len(), 1);
    assert_eq!(
        scene.paints().resolve(scene.rects()[0].paint),
        &PaintEntry::solid(RED)
    );
    assert_eq!(arena.name(root), Some("bg"));
}

#[test]
fn dfs_order_and_absolute_positions_resolve_through_nesting() {
    // root(10,20) ── a(1,2) ── leaf(0.5,0.5)
    //            └── b(3,4)
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::X(10.0));
    txn.set_prop(root, Prop::Y(20.0));
    let a = txn.add_node(Some(root), Some("a"));
    txn.set_prop(a, Prop::X(1.0));
    txn.set_prop(a, Prop::Y(2.0));
    let leaf = txn.add_node(Some(a), Some("leaf"));
    txn.set_prop(leaf, Prop::X(0.5));
    txn.set_prop(leaf, Prop::Y(0.5));
    let b = txn.add_node(Some(root), Some("b"));
    txn.set_prop(b, Prop::X(3.0));
    txn.set_prop(b, Prop::Y(4.0));
    txn.commit();

    let scene = arena.committed();
    let positions: Vec<(f32, f32)> = scene.rects().iter().map(|r| (r.x, r.y)).collect();
    assert_eq!(
        positions,
        [(10.0, 20.0), (11.0, 22.0), (11.5, 22.5), (13.0, 24.0)],
        "DFS order root, a, leaf, b; absolutes sum ancestor offsets"
    );
}

#[test]
fn interleaved_creation_still_yields_dfs_document_order() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    let b = txn.add_node(Some(root), Some("b"));
    txn.add_node(Some(root), Some("a"));
    txn.add_node(Some(b), Some("leaf"));
    txn.commit();

    let scene = arena.committed();
    // Children in creation order under each parent, depth first:
    // root, b (first child), leaf (b's child), a.
    let names: Vec<&str> = (0..scene.rects().len())
        .map(|i| {
            arena
                .name(scene.node_of(u32::try_from(i).unwrap()))
                .unwrap()
        })
        .collect();
    assert_eq!(names, ["root", "b", "leaf", "a"]);
}

#[test]
fn node_ids_and_rect_indices_correspond() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    let b = txn.add_node(Some(root), None);
    let a = txn.add_node(Some(root), None);
    txn.commit();

    let scene = arena.committed();
    for id in [root, b, a] {
        let rect_index = scene
            .rect_index_of(id)
            .expect("committed node has an index");
        assert_eq!(scene.node_of(rect_index), id);
    }
}

#[test]
fn a_hidden_node_keeps_its_rect_table_index() {
    // Prop::Visible(false) still resolves to a rect (P4) and keeps its
    // rect-table index — no DFS index shifts for nodes committed after
    // it. This is the invariant the bounded-pool work depends on
    // (issue #166, issue #165).
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    let hidden = txn.add_node(Some(root), None);
    txn.set_prop(hidden, Prop::Visible(false));
    let after = txn.add_node(Some(root), None);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.rects().len(), 3, "hidden node still resolves");
    assert_eq!(scene.rect_index_of(hidden), Some(1));
    assert_eq!(scene.rect_index_of(after), Some(2));
    assert_eq!(scene.node_of(1), hidden);
    assert_eq!(scene.node_of(2), after);
}

#[test]
fn commits_fixed_resolution_ignores_visible_like_the_rest_of_the_flex_vocabulary() {
    // commit()'s FixedSolver ignores Visible, the same gap it already
    // leaves for the rest of the flex vocabulary (docs/design/dashscene-engine.md) —
    // only dashscene-engine's TaffySolver lowers it to Taffy Display::None.
    // Authoring real geometry on the hidden node (rather than leaving it
    // at its all-zero default) is what actually distinguishes "ignored"
    // from "coincidentally already degenerate".
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    let hidden = txn.add_node(Some(root), None);
    txn.set_prop(hidden, Prop::X(5.0));
    txn.set_prop(hidden, Prop::Y(6.0));
    txn.set_prop(hidden, Prop::Width(30.0));
    txn.set_prop(hidden, Prop::Height(20.0));
    txn.set_prop(hidden, Prop::Visible(false));
    txn.commit();

    let scene = arena.committed();
    assert_eq!(
        scene.rects()[1],
        RectEntry {
            x: 5.0,
            y: 6.0,
            w: 30.0,
            h: 20.0,
            paint: PaintIndex(0),
            clip: ClipIndex::UNCLIPPED,
        },
        "commit()'s fixed resolution resolves the hidden node's authored geometry, unhidden"
    );
}

#[test]
fn identical_fills_share_one_paint_entry_in_first_use_order() {
    const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let first = txn.add_node(None, None);
    txn.set_prop(first, Prop::Fill(RED));
    let second = txn.add_node(None, None);
    txn.set_prop(second, Prop::Fill(BLUE));
    let third = txn.add_node(None, None);
    txn.set_prop(third, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.paints().len(), 2);
    assert_eq!(
        scene.paints().resolve(PaintIndex(0)),
        &PaintEntry::solid(RED)
    );
    assert_eq!(
        scene.paints().resolve(PaintIndex(1)),
        &PaintEntry::solid(BLUE)
    );
    let indices: Vec<PaintIndex> = scene.rects().iter().map(|r| r.paint).collect();
    assert_eq!(indices, [PaintIndex(0), PaintIndex(1), PaintIndex(0)]);
}

#[test]
fn unfilled_nodes_resolve_to_one_shared_empty_paint_entry() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let container = txn.add_node(None, Some("container"));
    txn.set_prop(container, Prop::Width(100.0));
    txn.set_prop(container, Prop::Height(100.0));
    let second = txn.add_node(None, Some("spacer"));
    txn.set_prop(second, Prop::Width(10.0));
    txn.commit();

    // Every rect resolves — an unfilled node references the shared
    // draws-nothing entry instead of a sentinel index.
    let scene = arena.committed();
    assert_eq!(scene.rects()[0].paint, scene.rects()[1].paint);
    let entry = scene.paints().resolve(scene.rects()[0].paint);
    assert_eq!(entry, &PaintEntry::default());
    assert_eq!(scene.paints().len(), 1);
}

#[test]
fn staged_mutations_are_invisible_until_commit() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::X(1.0));
    txn.set_prop(node, Prop::Fill(RED));
    txn.commit();
    assert_eq!(arena.committed().rects()[0].x, 1.0);

    // Stage a move, then drop the txn without committing: the change
    // stays pending — the committed buffer still serves the old value.
    {
        let mut txn = arena.open();
        txn.set_prop(node, Prop::X(50.0));
    }
    assert_eq!(
        arena.committed().rects()[0].x,
        1.0,
        "uncommitted staging must not be visible"
    );

    // The pending change publishes with the next commit.
    arena.open().commit();
    assert_eq!(arena.committed().rects()[0].x, 50.0);
}

#[test]
fn every_commit_bumps_the_generation_even_without_changes() {
    let mut arena = Arena::new();
    assert_eq!(arena.open().commit(), 1);
    assert_eq!(arena.open().commit(), 2);
    assert_eq!(arena.open().commit(), 3);
    assert_eq!(arena.committed().generation(), 3);
}

#[test]
fn the_first_commit_marks_every_rect_dirty() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.add_node(Some(root), None);
    txn.add_node(None, None);
    txn.commit();

    assert_eq!(arena.committed().dirty(), [0, 1, 2]);
}

#[test]
fn moving_a_parent_dirties_it_and_its_descendants_only() {
    // root ── a ── leaf
    //     └── b
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    let a = txn.add_node(Some(root), None);
    let leaf = txn.add_node(Some(a), None);
    let b = txn.add_node(Some(root), None);
    txn.set_prop(leaf, Prop::X(5.0));
    txn.set_prop(b, Prop::X(9.0));
    txn.commit();

    let mut txn = arena.open();
    txn.set_prop(a, Prop::X(100.0));
    txn.commit();

    // DFS indices: root=0, a=1, leaf=2, b=3. Moving `a` changes the
    // resolved absolutes of `a` and `leaf`; root and b are untouched.
    assert_eq!(arena.committed().dirty(), [1, 2]);
}

#[test]
fn a_no_op_commit_has_an_empty_dirty_set() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::Fill(RED));
    txn.commit();

    arena.open().commit();
    assert!(arena.committed().dirty().is_empty());
    assert_eq!(arena.committed().generation(), 2);
}

#[test]
fn a_fill_change_earns_a_new_stable_paint_index_and_marks_the_rect_dirty() {
    const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::Fill(RED));
    txn.commit();
    assert_eq!(
        arena.committed().rects()[0].paint,
        PaintIndex(0),
        "RED at 0"
    );

    // The interner is retained across commits (issue #164), so RED keeps
    // index 0 and BLUE — a colour not seen before — earns the next index.
    // The rect's paint index therefore changes, and the dirty check is a
    // plain bit compare rather than a resolved-colour diff.
    let mut txn = arena.open();
    txn.set_prop(node, Prop::Fill(BLUE));
    txn.commit();

    assert_eq!(
        arena.committed().rects()[0].paint,
        PaintIndex(1),
        "BLUE at 1"
    );
    assert_eq!(arena.committed().dirty(), [0]);
}

#[test]
fn swapped_fills_mark_both_rects_dirty() {
    const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let a = txn.add_node(None, None);
    let b = txn.add_node(None, None);
    txn.set_prop(a, Prop::Fill(RED));
    txn.set_prop(b, Prop::Fill(BLUE));
    txn.commit();

    // Both colours already exist in the retained interner (RED at 0, BLUE
    // at 1), so swapping the fills swaps the two paint indices: `a` moves
    // 0→1 and `b` moves 1→0. Each rect's entry bits change, so the bit
    // compare marks both dirty (issue #164).
    let mut txn = arena.open();
    txn.set_prop(a, Prop::Fill(BLUE));
    txn.set_prop(b, Prop::Fill(RED));
    txn.commit();

    assert_eq!(arena.committed().dirty(), [0, 1]);
}

#[test]
fn a_node_added_in_a_dropped_txn_persists_and_publishes_with_the_next_commit() {
    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        txn.add_node(None, Some("pending"));
    } // dropped without commit: staged, not rolled back
    assert!(arena.committed().rects().is_empty());

    arena.open().commit();
    assert_eq!(arena.committed().rects().len(), 1);
}

#[test]
#[should_panic(expected = "is not a node of this arena")]
fn an_out_of_range_node_id_panics_with_a_clear_message() {
    let mut arena_a = Arena::new();
    let mut txn = arena_a.open();
    txn.add_node(None, None);
    let foreign = txn.add_node(None, None); // NodeId(1)
    txn.commit();

    // A second arena with fewer nodes: the foreign id is out of range.
    let mut arena_b = Arena::new();
    let mut txn = arena_b.open();
    txn.add_node(None, None);
    txn.set_prop(foreign, Prop::X(1.0));
}

#[test]
fn text_props_set_and_read_back_through_the_intent_accessors() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let label = txn.add_node(None, Some("label"));
    txn.set_prop(label, Prop::Text("Speed".to_string()));
    txn.set_prop(
        label,
        Prop::TextStyle(TextStyle {
            family: "Noto Sans".to_string(),
            size: 16.0,
            weight: 400,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        }),
    );
    txn.commit();
    assert_eq!(arena.text(label), Some("Speed"));
    let style = arena.text_style(label).expect("style set");
    assert_eq!(style.family, "Noto Sans");
    assert_eq!(style.weight, 400);
}

#[test]
fn text_accessors_read_staged_intent_immediately() {
    // Intent-side semantics (the #28/#29 seam): staged values are
    // visible before commit, unlike committed().
    let mut arena = Arena::new();
    let n = {
        let mut txn = arena.open();
        let n = txn.add_node(None, None);
        txn.set_prop(n, Prop::Text("pending".to_string()));
        n
    }; // txn dropped here — staged, never committed
    assert_eq!(arena.text(n), Some("pending"));
}

#[test]
fn text_props_replace_previous_values() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Text("old".to_string()));
    txn.set_prop(n, Prop::Text("new".to_string()));
    txn.commit();
    assert_eq!(arena.text(n), Some("new"));
}

#[test]
fn nodes_without_text_read_none() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.commit();
    assert_eq!(arena.text(n), None);
    assert!(arena.text_style(n).is_none());
}

#[test]
fn a_text_only_change_does_not_touch_the_rect_table() {
    // P1: text influences no v0.5 committed output; hug sizing is #29.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Width(10.0));
    txn.commit();
    let mut txn = arena.open();
    txn.set_prop(n, Prop::Text("hello".to_string()));
    txn.commit();
    assert!(arena.committed().dirty().is_empty());
}

#[test]
fn layout_defaults_match_the_schema_defaults() {
    use dashscene_core::{AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.commit();

    let layout = arena.layout(node);
    assert_eq!(layout.mode, LayoutMode::None);
    assert_eq!(layout.gap, 0.0);
    assert_eq!(
        (
            layout.padding.left,
            layout.padding.top,
            layout.padding.right,
            layout.padding.bottom
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert_eq!(layout.main_align, MainAxisAlign::Start);
    assert_eq!(layout.cross_align, CrossAxisAlign::Start);
    assert_eq!(layout.sizing_h, AxisSizing::Fixed);
    assert_eq!(layout.sizing_v, AxisSizing::Fixed);
    assert_eq!(layout.min_width, None);
    assert_eq!(layout.max_width, None);
    assert_eq!(layout.min_height, None);
    assert_eq!(layout.max_height, None);
}

#[test]
fn every_flex_prop_sets_its_layout_field() {
    use dashscene_core::{AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::X(1.0));
    txn.set_prop(node, Prop::Y(2.0));
    txn.set_prop(node, Prop::Width(30.0));
    txn.set_prop(node, Prop::Height(40.0));
    txn.set_prop(node, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(node, Prop::Gap(8.0));
    txn.set_prop(
        node,
        Prop::Padding {
            left: 1.0,
            top: 2.0,
            right: 3.0,
            bottom: 4.0,
        },
    );
    txn.set_prop(node, Prop::MainAlign(MainAxisAlign::SpaceBetween));
    txn.set_prop(node, Prop::CrossAlign(CrossAxisAlign::Center));
    txn.set_prop(node, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(node, Prop::SizingV(AxisSizing::Fill));
    txn.set_prop(node, Prop::MinWidth(10.0));
    txn.set_prop(node, Prop::MaxWidth(100.0));
    txn.set_prop(node, Prop::MinHeight(5.0));
    txn.set_prop(node, Prop::MaxHeight(50.0));
    txn.commit();

    let layout = arena.layout(node);
    assert_eq!((layout.x, layout.y), (1.0, 2.0));
    assert_eq!((layout.width, layout.height), (30.0, 40.0));
    assert_eq!(layout.mode, LayoutMode::Horizontal);
    assert_eq!(layout.gap, 8.0);
    assert_eq!(
        (
            layout.padding.left,
            layout.padding.top,
            layout.padding.right,
            layout.padding.bottom
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(layout.main_align, MainAxisAlign::SpaceBetween);
    assert_eq!(layout.cross_align, CrossAxisAlign::Center);
    assert_eq!(layout.sizing_h, AxisSizing::Hug);
    assert_eq!(layout.sizing_v, AxisSizing::Fill);
    assert_eq!(layout.min_width, Some(10.0));
    assert_eq!(layout.max_width, Some(100.0));
    assert_eq!(layout.min_height, Some(5.0));
    assert_eq!(layout.max_height, Some(50.0));
}

#[test]
fn flex_props_do_not_change_committed_output_yet() {
    use dashscene_core::{AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign};

    // Until story #9's Taffy solve, commit resolves fixed geometry
    // only: flex intent is stored, not solved. Every flex prop is set
    // here so a partial leak into the resolve step cannot pass.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::Width(100.0));
    txn.set_prop(root, Prop::Height(50.0));
    let child = txn.add_node(Some(root), None);
    txn.set_prop(child, Prop::X(10.0));
    txn.set_prop(child, Prop::Width(20.0));
    txn.commit();
    let before: Vec<RectEntry> = arena.committed().rects().to_vec();

    let mut txn = arena.open();
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Gap(4.0));
    txn.set_prop(
        root,
        Prop::Padding {
            left: 1.0,
            top: 2.0,
            right: 3.0,
            bottom: 4.0,
        },
    );
    txn.set_prop(root, Prop::MainAlign(MainAxisAlign::Center));
    txn.set_prop(root, Prop::CrossAlign(CrossAxisAlign::End));
    txn.set_prop(child, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(child, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(child, Prop::MinWidth(1.0));
    txn.set_prop(child, Prop::MaxWidth(90.0));
    txn.set_prop(child, Prop::MinHeight(2.0));
    txn.set_prop(child, Prop::MaxHeight(40.0));
    txn.commit();

    assert_eq!(arena.committed().rects(), before.as_slice());
    assert!(arena.committed().dirty().is_empty());
}

#[test]
fn roots_and_children_expose_the_intent_tree_in_creation_order() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let first_root = txn.add_node(None, None);
    let b = txn.add_node(Some(first_root), None);
    let a = txn.add_node(Some(first_root), None);
    let second_root = txn.add_node(None, None);
    let leaf = txn.add_node(Some(b), None);
    txn.commit();

    assert_eq!(arena.roots(), [first_root, second_root]);
    assert_eq!(arena.children(first_root), [b, a]);
    assert_eq!(arena.children(b), [leaf]);
    assert!(arena.children(leaf).is_empty());
}

#[test]
fn commit_with_uses_the_solver_geometry_verbatim() {
    use dashscene_core::{LayoutSolver, SolvedRect};

    // A solver that places every node at a fabricated position: the
    // committed table must carry exactly these rects, proving commit
    // takes geometry from the solver and computes none of its own.
    struct GridSolver;
    impl LayoutSolver for GridSolver {
        fn solve(&mut self, arena: &Arena) -> Vec<(dashscene_core::NodeId, SolvedRect)> {
            let mut out = Vec::new();
            let mut stack: Vec<_> = arena.roots().to_vec();
            let mut i = 0.0f32;
            while let Some(id) = stack.pop() {
                out.push((
                    id,
                    SolvedRect {
                        x: 100.0 * i,
                        y: 7.0,
                        w: 10.0 + i,
                        h: 20.0 + i,
                    },
                ));
                stack.extend(arena.children(id).iter().copied());
                i += 1.0;
            }
            out
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::X(999.0)); // must be ignored by commit_with
    txn.set_prop(root, Prop::Fill(RED));
    txn.add_node(Some(root), None);
    let generation = txn.commit_with(&mut GridSolver);

    assert_eq!(generation, 1);
    let rects = arena.committed().rects();
    assert_eq!(rects.len(), 2);
    assert_eq!((rects[0].x, rects[0].y), (0.0, 7.0), "root from solver");
    assert_eq!(
        (rects[0].w, rects[0].h),
        (10.0, 20.0),
        "root size from solver"
    );
    assert_eq!((rects[1].x, rects[1].y), (100.0, 7.0), "child from solver");
    // Paint interning still core's: the root's fill resolves to solid
    // RED regardless of where the geometry came from.
    assert_eq!(
        arena.committed().paints().resolve(rects[0].paint),
        &dashscene_core::PaintEntry::solid(RED)
    );
}

#[test]
#[should_panic(expected = "two rects for")]
fn commit_with_panics_on_a_duplicate_solver_rect() {
    use dashscene_core::{LayoutSolver, SolvedRect};

    struct StutteringSolver;
    impl LayoutSolver for StutteringSolver {
        fn solve(&mut self, arena: &Arena) -> Vec<(dashscene_core::NodeId, SolvedRect)> {
            let id = arena.roots()[0];
            let rect = SolvedRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            };
            vec![(id, rect), (id, rect)]
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    txn.add_node(None, None);
    txn.commit_with(&mut StutteringSolver);
}

#[test]
#[should_panic(expected = "not a node of this arena")]
fn commit_with_panics_on_a_foreign_solver_rect() {
    use dashscene_core::{LayoutSolver, SolvedRect};

    // A solver that replays an id from a bigger arena: out of range
    // here, and named as the contract breach it is.
    struct ReplaySolver(dashscene_core::NodeId);
    impl LayoutSolver for ReplaySolver {
        fn solve(&mut self, _arena: &Arena) -> Vec<(dashscene_core::NodeId, SolvedRect)> {
            vec![(
                self.0,
                SolvedRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            )]
        }
    }

    let mut big = Arena::new();
    let mut txn = big.open();
    txn.add_node(None, None);
    let foreign = txn.add_node(None, None);
    txn.commit();

    let mut small = Arena::new();
    let mut txn = small.open();
    txn.add_node(None, None);
    txn.commit_with(&mut ReplaySolver(foreign));
}

#[test]
#[should_panic(expected = "no rect for")]
fn commit_with_panics_when_a_node_has_no_rect_this_solve_or_last() {
    use dashscene_core::{LayoutSolver, SolvedRect};

    // The re-expressed invariant (issue #164): a solver may omit a node
    // whose rect is unchanged, but only when a previous commit resolved
    // it. On the first commit there is no previous rect, so an omitted
    // node has no rect at all — a broken contract, named loudly (P4).
    struct ForgetfulSolver;
    impl LayoutSolver for ForgetfulSolver {
        fn solve(&mut self, _arena: &Arena) -> Vec<(dashscene_core::NodeId, SolvedRect)> {
            Vec::new()
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    txn.add_node(None, None);
    txn.commit_with(&mut ForgetfulSolver);
}

#[test]
fn commit_with_carries_an_omitted_nodes_rect_forward_from_the_previous_commit() {
    use dashscene_core::{LayoutSolver, NodeId, SolvedRect};

    // The partial-solve happy path (issue #164): an incremental solver
    // reports only the nodes that changed and omits the rest; commit keeps
    // each omitted node's previous rect verbatim.
    struct OnlyNode(NodeId, SolvedRect);
    impl LayoutSolver for OnlyNode {
        fn solve(&mut self, _arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
            vec![(self.0, self.1)]
        }
    }
    // A solver that resolves every node, for the initial full commit.
    struct AllFixed;
    impl LayoutSolver for AllFixed {
        fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
            let mut out = Vec::new();
            let mut stack: Vec<_> = arena.roots().to_vec();
            let mut i = 0.0f32;
            while let Some(id) = stack.pop() {
                out.push((
                    id,
                    SolvedRect {
                        x: i,
                        y: 0.0,
                        w: 5.0,
                        h: 5.0,
                    },
                ));
                stack.extend(arena.children(id).iter().copied());
                i += 10.0;
            }
            out
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let a = txn.add_node(None, None);
    let b = txn.add_node(None, None);
    txn.commit_with(&mut AllFixed);
    let b_before = arena.committed().rects()[1];
    let _ = b;

    // Second commit: report only `a`'s new rect; `b` is omitted.
    let mut txn = arena.open();
    txn.set_prop(a, Prop::X(0.0));
    txn.commit_with(&mut OnlyNode(
        a,
        SolvedRect {
            x: 42.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        },
    ));

    let scene = arena.committed();
    assert_eq!(scene.rects()[0].x, 42.0, "reported node takes the new rect");
    assert_eq!(
        scene.rects()[1],
        b_before,
        "omitted node keeps its previous rect verbatim"
    );
    assert_eq!(scene.dirty(), [0], "only the node that actually changed");
}

#[test]
fn margin_prop_sets_and_reads_back() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    // Default is zero insets.
    txn.commit();
    let m = arena.layout(node).margin;
    assert_eq!((m.left, m.top, m.right, m.bottom), (0.0, 0.0, 0.0, 0.0));

    let mut txn = arena.open();
    txn.set_prop(
        node,
        Prop::Margin {
            left: -8.0,
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
        },
    );
    txn.commit();
    let m = arena.layout(node).margin;
    assert_eq!((m.left, m.top, m.right, m.bottom), (-8.0, 1.0, 2.0, 3.0));
}

#[test]
fn lower_negative_gaps_rewrites_a_horizontal_row_to_child_margins() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(-8.0));
    let a = txn.add_node(Some(row), None);
    let b = txn.add_node(Some(row), None);
    let c = txn.add_node(Some(row), None);
    txn.lower_negative_gaps();
    txn.commit();

    // The container's negative gap becomes zero...
    assert_eq!(arena.layout(row).gap, 0.0);
    // ...the first child is untouched...
    assert_eq!(arena.layout(a).margin.left, 0.0);
    // ...and every later child gains the negative gap as a leading
    // main-axis (left) margin.
    assert_eq!(arena.layout(b).margin.left, -8.0);
    assert_eq!(arena.layout(c).margin.left, -8.0);
    // The cross axis is untouched.
    assert_eq!(arena.layout(b).margin.top, 0.0);
}

#[test]
fn lower_negative_gaps_uses_the_top_margin_for_a_vertical_column() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(col, Prop::Gap(-5.0));
    let a = txn.add_node(Some(col), None);
    let b = txn.add_node(Some(col), None);
    txn.lower_negative_gaps();
    txn.commit();

    assert_eq!(arena.layout(col).gap, 0.0);
    assert_eq!(arena.layout(a).margin.top, 0.0);
    assert_eq!(arena.layout(b).margin.top, -5.0);
    assert_eq!(arena.layout(b).margin.left, 0.0);
}

#[test]
#[should_panic(expected = "negative gap on a Wrap container")]
fn lower_negative_gaps_refuses_a_wrap_container_by_name() {
    // Review finding R4 (story #43): the margin rewrite is only
    // gap-equivalent for a child that follows another child on the SAME
    // line, and wrap decides its line breaks after the lowering — a
    // lowered wrap scene pulls every later line's leading child into
    // the padding band and distorts the breaks. There is no margin
    // encoding of a negative wrap gap, so the construct is refused by
    // name (P4), never lowered wrong.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(row, Prop::Gap(-8.0));
    txn.add_node(Some(row), None);
    txn.add_node(Some(row), None);
    txn.lower_negative_gaps();
}

#[test]
fn lower_negative_gaps_leaves_a_wrap_container_with_a_positive_gap_untouched() {
    // Only the negative-gap wrap construct is refused; positive and
    // zero gaps are CSS-native vocabulary on a Wrap container too.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(row, Prop::Gap(8.0));
    let a = txn.add_node(Some(row), None);
    txn.lower_negative_gaps();
    txn.commit();

    assert_eq!(arena.layout(row).gap, 8.0);
    assert_eq!(arena.layout(a).margin.left, 0.0);
}

#[test]
fn lower_negative_gaps_leaves_positive_gaps_and_adds_to_existing_margins() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    // Positive gap: untouched (CSS-native).
    let kept = txn.add_node(None, None);
    txn.set_prop(kept, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(kept, Prop::Gap(6.0));
    txn.add_node(Some(kept), None);
    txn.add_node(Some(kept), None);
    // Negative gap over a child that already carries a margin: the
    // lowering adds to it, never replaces it.
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(-8.0));
    txn.add_node(Some(row), None);
    let b = txn.add_node(Some(row), None);
    txn.set_prop(
        b,
        Prop::Margin {
            left: 3.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
    txn.lower_negative_gaps();
    txn.commit();

    assert_eq!(arena.layout(kept).gap, 6.0, "positive gap untouched");
    assert_eq!(arena.layout(row).gap, 0.0);
    assert_eq!(
        arena.layout(b).margin.left,
        -5.0,
        "3 + (-8), added not replaced"
    );
}

#[test]
fn lower_negative_gaps_is_idempotent() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(-8.0));
    txn.add_node(Some(row), None);
    let b = txn.add_node(Some(row), None);
    txn.lower_negative_gaps();
    txn.lower_negative_gaps(); // second pass: no negative gaps remain
    txn.commit();

    assert_eq!(arena.layout(row).gap, 0.0);
    assert_eq!(arena.layout(b).margin.left, -8.0, "not doubled");
}

#[test]
fn lower_negative_gaps_reaches_containers_at_every_depth() {
    // The pass scans the whole arena, so a negative-gap container nested
    // under another negative-gap container is lowered too. An inner row
    // takes both roles at once: its own gap is zeroed (as a container),
    // and it gains a leading margin (as a non-first child of the outer
    // row). The two roles touch different fields, so neither clobbers
    // the other regardless of arena order.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let outer = txn.add_node(None, None);
    txn.set_prop(outer, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(outer, Prop::Gap(-10.0));

    let inner0 = txn.add_node(Some(outer), None);
    txn.set_prop(inner0, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(inner0, Prop::Gap(-4.0));
    let a0 = txn.add_node(Some(inner0), None);
    let b0 = txn.add_node(Some(inner0), None);

    let inner1 = txn.add_node(Some(outer), None);
    txn.set_prop(inner1, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(inner1, Prop::Gap(-4.0));
    let a1 = txn.add_node(Some(inner1), None);
    let b1 = txn.add_node(Some(inner1), None);

    txn.lower_negative_gaps();
    txn.commit();

    // Every container's gap is zeroed, at both depths.
    assert_eq!(arena.layout(outer).gap, 0.0);
    assert_eq!(arena.layout(inner0).gap, 0.0, "nested container lowered");
    assert_eq!(arena.layout(inner1).gap, 0.0, "nested container lowered");
    // The outer gap lands on the second inner row only.
    assert_eq!(arena.layout(inner0).margin.left, 0.0, "first child");
    assert_eq!(
        arena.layout(inner1).margin.left,
        -10.0,
        "container and child"
    );
    // Each inner row's own gap lands on that row's second child.
    assert_eq!(arena.layout(a0).margin.left, 0.0);
    assert_eq!(arena.layout(b0).margin.left, -4.0);
    assert_eq!(arena.layout(a1).margin.left, 0.0);
    assert_eq!(arena.layout(b1).margin.left, -4.0);
}

#[test]
fn lower_negative_gaps_leaves_a_nan_gap_untouched() {
    // A NaN gap is not genuinely negative; the lowering must not treat
    // it as such and spray NaN into child margins.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(f32::NAN));
    txn.add_node(Some(row), None);
    let b = txn.add_node(Some(row), None);
    txn.lower_negative_gaps();
    txn.commit();

    assert!(arena.layout(row).gap.is_nan(), "NaN gap left as-is");
    assert_eq!(arena.layout(b).margin.left, 0.0, "no NaN in child margin");
}

// ---------------------------------------------------------------------
// Subtree clip resolution (story #97): `Prop::Clip` is intent — commit
// resolves it into the per-rect clip regions boundary B carries, so no
// painter re-derives the tree (P2).
// ---------------------------------------------------------------------

const ROUND_4: CornerRadii = CornerRadii {
    top_left: 4.0,
    top_right: 4.0,
    bottom_right: 4.0,
    bottom_left: 4.0,
};

/// `size(w, h)` at `(x, y)`, filled red — the fixture body every clip
/// test below shares.
fn boxed(
    txn: &mut dashscene_core::Txn<'_>,
    parent: Option<dashscene_core::NodeId>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> dashscene_core::NodeId {
    let node = txn.add_node(parent, None);
    txn.set_prop(node, Prop::X(x));
    txn.set_prop(node, Prop::Y(y));
    txn.set_prop(node, Prop::Width(w));
    txn.set_prop(node, Prop::Height(h));
    node
}

#[test]
fn corner_radii_intent_reaches_the_committed_paint_entry() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    txn.set_prop(node, Prop::Fill(RED));
    txn.set_prop(
        node,
        Prop::Corners {
            top_left: 1.0,
            top_right: 2.0,
            bottom_right: 3.0,
            bottom_left: 4.0,
        },
    );
    txn.commit();

    let scene = arena.committed();
    assert_eq!(
        scene.paints().resolve(scene.rects()[0].paint).corners,
        CornerRadii {
            top_left: 1.0,
            top_right: 2.0,
            bottom_right: 3.0,
            bottom_left: 4.0,
        }
    );
}

#[test]
fn same_fill_different_corners_are_different_paint_entries() {
    // The paint interner keys on the whole entry, not the fill color
    // alone: two nodes that share a color but round differently must not
    // collapse into one pool entry.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let sharp = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    txn.set_prop(sharp, Prop::Fill(RED));
    let round = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    txn.set_prop(round, Prop::Fill(RED));
    txn.set_prop(
        round,
        Prop::Corners {
            top_left: 4.0,
            top_right: 4.0,
            bottom_right: 4.0,
            bottom_left: 4.0,
        },
    );
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.paints().len(), 2);
    assert_ne!(scene.rects()[0].paint, scene.rects()[1].paint);
}

#[test]
fn a_scene_without_clips_shares_the_reserved_unclipped_region() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    boxed(&mut txn, Some(root), 1.0, 1.0, 2.0, 2.0);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.clips().len(), 1, "only the reserved region");
    for rect in scene.rects() {
        assert_eq!(rect.clip, ClipIndex::UNCLIPPED);
        assert!(scene.clips().resolve(rect.clip).is_unclipped());
    }
}

#[test]
fn a_clipping_node_clips_its_descendants_but_not_itself() {
    // frame(clip, rounded) ── child ── grandchild
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 10.0, 20.0, 30.0, 40.0);
    txn.set_prop(frame, Prop::Clip(true));
    txn.set_prop(
        frame,
        Prop::Corners {
            top_left: 4.0,
            top_right: 4.0,
            bottom_right: 4.0,
            bottom_left: 4.0,
        },
    );
    let child = boxed(&mut txn, Some(frame), 0.0, 0.0, 100.0, 100.0);
    boxed(&mut txn, Some(child), 0.0, 0.0, 100.0, 100.0);
    txn.commit();

    let scene = arena.committed();
    let region = |i: usize| scene.clips().resolve(scene.rects()[i].clip);

    // The clipping node is not clipped by its own clip.
    assert!(region(0).is_unclipped());
    // Its descendants — child and grandchild alike — carry its box, in
    // absolute coordinates, with its corner radii.
    let expected = ClipBox {
        x: 10.0,
        y: 20.0,
        w: 30.0,
        h: 40.0,
        corners: ROUND_4,
    };
    assert_eq!(region(1).boxes(), &[expected]);
    assert_eq!(region(2).boxes(), &[expected]);
    // And they share one interned region entry.
    assert_eq!(scene.rects()[1].clip, scene.rects()[2].clip);
    assert_eq!(scene.clips().len(), 2, "unclipped + the frame's region");
}

#[test]
fn nested_clips_intersect_as_an_ancestor_chain_outermost_first() {
    // outer(clip) ── middle(clip, rounded) ── leaf
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let outer = boxed(&mut txn, None, 0.0, 0.0, 100.0, 100.0);
    txn.set_prop(outer, Prop::Clip(true));
    let middle = boxed(&mut txn, Some(outer), 10.0, 10.0, 50.0, 50.0);
    txn.set_prop(middle, Prop::Clip(true));
    txn.set_prop(
        middle,
        Prop::Corners {
            top_left: 4.0,
            top_right: 4.0,
            bottom_right: 4.0,
            bottom_left: 4.0,
        },
    );
    boxed(&mut txn, Some(middle), 0.0, 0.0, 80.0, 80.0);
    txn.commit();

    let scene = arena.committed();
    let region = |i: usize| scene.clips().resolve(scene.rects()[i].clip);

    assert!(region(0).is_unclipped());
    assert_eq!(
        region(1).boxes(),
        &[ClipBox {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            corners: CornerRadii::default(),
        }]
    );
    assert_eq!(
        region(2).boxes(),
        &[
            ClipBox {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
                corners: CornerRadii::default(),
            },
            ClipBox {
                x: 10.0,
                y: 10.0,
                w: 50.0,
                h: 50.0,
                corners: ROUND_4,
            },
        ],
        "outermost ancestor first"
    );
}

#[test]
fn a_non_clipping_node_passes_its_ancestors_region_through() {
    // frame(clip) ── pass ── leaf: `pass` clips nothing, so `leaf`
    // carries exactly the frame's region — the same interned entry.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 0.0, 0.0, 20.0, 20.0);
    txn.set_prop(frame, Prop::Clip(true));
    let pass = boxed(&mut txn, Some(frame), 0.0, 0.0, 5.0, 5.0);
    boxed(&mut txn, Some(pass), 0.0, 0.0, 5.0, 5.0);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.rects()[1].clip, scene.rects()[2].clip);
    assert_eq!(scene.clips().len(), 2);
}

#[test]
fn sibling_subtrees_under_one_clip_share_one_region_entry() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 0.0, 0.0, 20.0, 20.0);
    txn.set_prop(frame, Prop::Clip(true));
    boxed(&mut txn, Some(frame), 0.0, 0.0, 5.0, 5.0);
    boxed(&mut txn, Some(frame), 6.0, 0.0, 5.0, 5.0);
    boxed(&mut txn, Some(frame), 12.0, 0.0, 5.0, 5.0);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.clips().len(), 2, "unclipped + one shared region");
    assert_eq!(scene.rects()[1].clip, scene.rects()[2].clip);
    assert_eq!(scene.rects()[2].clip, scene.rects()[3].clip);
}

#[test]
fn resizing_a_clipping_ancestor_dirties_the_descendants_it_clips() {
    // The load-bearing dirty-set case: resizing the clipping frame changes
    // the child's resolved clip region without touching the child's own
    // geometry or fill. With retained interners (issue #164) the resized
    // frame mints a *new* clip region, so the child's clip index changes
    // and the plain bit compare catches the repaint — no resolved-clip
    // diff needed.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 0.0, 0.0, 20.0, 20.0);
    txn.set_prop(frame, Prop::Clip(true));
    let child = boxed(&mut txn, Some(frame), 0.0, 0.0, 50.0, 50.0);
    txn.set_prop(child, Prop::Fill(RED));
    txn.commit();

    let before = arena.committed().rects()[1];

    let mut txn = arena.open();
    txn.set_prop(frame, Prop::Width(10.0));
    txn.commit();

    let scene = arena.committed();
    let after = scene.rects()[1];
    assert_eq!(
        (after.x, after.y, after.w, after.h, after.paint),
        (before.x, before.y, before.w, before.h, before.paint),
        "the child's own geometry and fill did not change"
    );
    assert_ne!(
        after.clip, before.clip,
        "the child's clip index moved to the newly interned region"
    );
    assert_eq!(scene.dirty(), [0, 1], "the frame and the rect it clips");
}

#[test]
fn toggling_a_clip_off_dirties_the_descendants_it_clipped() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 0.0, 0.0, 20.0, 20.0);
    txn.set_prop(frame, Prop::Clip(true));
    let child = boxed(&mut txn, Some(frame), 0.0, 0.0, 50.0, 50.0);
    txn.set_prop(child, Prop::Fill(RED));
    txn.commit();

    let mut txn = arena.open();
    txn.set_prop(frame, Prop::Clip(false));
    txn.commit();

    let scene = arena.committed();
    assert!(scene.clips().resolve(scene.rects()[1].clip).is_unclipped());
    assert_eq!(
        scene.dirty(),
        [1],
        "only the rect that stopped being clipped"
    );
}

#[test]
fn a_no_op_commit_of_a_clipped_scene_stays_clean() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let frame = boxed(&mut txn, None, 0.0, 0.0, 20.0, 20.0);
    txn.set_prop(frame, Prop::Clip(true));
    let child = boxed(&mut txn, Some(frame), 0.0, 0.0, 50.0, 50.0);
    txn.set_prop(child, Prop::Fill(RED));
    txn.commit();

    arena.open().commit();

    assert!(arena.committed().dirty().is_empty());
}

// ---------------------------------------------------------------------
// Variant table + set_variant (story #20): a variant set's members carry
// sparse overrides against the arena's base node values
// (docs/decisions/variant-set-flat-index.md); set_variant switches which
// member is active, and commit resolves the active member's overrides
// into the rect/paint tables through the same resolve-then-diff pipeline
// every other prop uses.
// ---------------------------------------------------------------------

#[test]
fn switching_a_variant_changes_the_resolved_rect() {
    use dashscene_core::VariantValue;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: Some("Default".to_string()),
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: Some("Wide".to_string()),
            overrides: vec![(node, VariantValue::Width(100.0))],
        },
    ]);
    txn.commit();
    assert_eq!(arena.committed().rects()[0].w, 10.0, "default member");

    let mut txn = arena.open();
    txn.set_variant(set, 1);
    txn.commit();

    assert_eq!(arena.committed().rects()[0].w, 100.0);
}

#[test]
fn switching_a_variant_changes_the_resolved_paint() {
    use dashscene_core::VariantValue;

    const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    txn.set_prop(node, Prop::Fill(RED));
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(node, VariantValue::Fill(BLUE))],
        },
    ]);
    txn.commit();
    let scene = arena.committed();
    assert_eq!(
        scene.paints().resolve(scene.rects()[0].paint),
        &PaintEntry::solid(RED)
    );

    let mut txn = arena.open();
    txn.set_variant(set, 1);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(
        scene.paints().resolve(scene.rects()[0].paint),
        &PaintEntry::solid(BLUE)
    );
}

#[test]
fn a_variant_switch_dirties_only_the_overridden_rect() {
    use dashscene_core::VariantValue;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let untouched = boxed(&mut txn, None, 0.0, 0.0, 5.0, 5.0);
    txn.set_prop(untouched, Prop::Fill(RED));
    let node = boxed(&mut txn, None, 20.0, 20.0, 10.0, 10.0);
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(node, VariantValue::X(50.0))],
        },
    ]);
    txn.commit();

    let mut txn = arena.open();
    txn.set_variant(set, 1);
    txn.commit();

    assert_eq!(arena.committed().dirty(), [1]);
}

#[test]
fn a_variant_switch_moves_descendants_of_the_overridden_node() {
    use dashscene_core::VariantValue;

    // root ── child, child's absolute x tracks root's resolved x.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    let child = boxed(&mut txn, Some(root), 1.0, 1.0, 2.0, 2.0);
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(root, VariantValue::X(100.0))],
        },
    ]);
    txn.commit();
    let _ = child;

    let mut txn = arena.open();
    txn.set_variant(set, 1);
    txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.rects()[0].x, 100.0, "root moved by the override");
    assert_eq!(
        scene.rects()[1].x,
        101.0,
        "child's absolute tracks root's resolved x, even though no \
         override named the child directly"
    );
    assert_eq!(
        scene.dirty(),
        [0, 1],
        "both the overridden node and its descendant"
    );
}

#[test]
fn set_variant_is_staged_and_visible_before_commit() {
    use dashscene_core::VariantValue;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(node, VariantValue::Width(42.0))],
        },
    ]);
    txn.commit();

    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
    } // txn dropped here — staged, never committed

    // Staged, not yet committed: the intent-side accessor sees it
    // immediately (the same contract as `Arena::text`), the committed
    // buffer does not.
    assert_eq!(arena.layout(node).width, 42.0);
    assert_eq!(
        arena.committed().rects()[0].w,
        10.0,
        "dropping the txn without committing must not publish the switch"
    );
}

#[test]
fn a_second_variant_set_overriding_the_same_prop_wins_in_creation_order() {
    use dashscene_core::VariantValue;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    let first = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(node, VariantValue::Width(20.0))],
        },
    ]);
    let second = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![(node, VariantValue::Width(30.0))],
        },
    ]);
    txn.set_variant(first, 1);
    txn.set_variant(second, 1);
    txn.commit();

    assert_eq!(
        arena.committed().rects()[0].w,
        30.0,
        "the later-created set's override wins"
    );
}

#[test]
fn a_later_override_within_one_members_list_wins_over_an_earlier_one() {
    use dashscene_core::VariantValue;

    // Two overrides of the same (node, prop) inside ONE member's overrides
    // Vec — distinct from creation-order precedence across sets, which
    // the tests above cover.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
    let set = txn.add_variant_set(vec![
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![],
        },
        dashscene_core::VariantMember {
            name: None,
            overrides: vec![
                (node, VariantValue::Width(20.0)),
                (node, VariantValue::Width(30.0)),
            ],
        },
    ]);
    txn.set_variant(set, 1);
    txn.commit();

    assert_eq!(
        arena.committed().rects()[0].w,
        30.0,
        "the last override in the member's list wins"
    );
}

#[test]
fn active_variant_defaults_to_the_first_member() {
    let mut arena = Arena::new();
    let set = {
        let mut txn = arena.open();
        boxed(&mut txn, None, 0.0, 0.0, 10.0, 10.0);
        txn.add_variant_set(vec![
            dashscene_core::VariantMember {
                name: Some("Default".to_string()),
                overrides: vec![],
            },
            dashscene_core::VariantMember {
                name: Some("Alt".to_string()),
                overrides: vec![],
            },
        ])
    }; // txn dropped here — staged, never committed

    assert_eq!(arena.active_variant(set), 0);
}

#[test]
#[should_panic(expected = "a variant set needs at least one member")]
fn add_variant_set_panics_on_an_empty_member_list() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    txn.add_variant_set(vec![]);
}

#[test]
#[should_panic(expected = "is not a node of this arena")]
fn add_variant_set_panics_on_an_out_of_range_override_node() {
    use dashscene_core::VariantValue;

    let mut arena_a = Arena::new();
    let mut txn = arena_a.open();
    let foreign = txn.add_node(None, None);
    txn.commit();

    let mut arena_b = Arena::new();
    let mut txn = arena_b.open();
    txn.add_variant_set(vec![dashscene_core::VariantMember {
        name: None,
        overrides: vec![(foreign, VariantValue::Width(1.0))],
    }]);
}

#[test]
#[should_panic(expected = "is out of range")]
fn set_variant_panics_on_an_out_of_range_member() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let set = txn.add_variant_set(vec![dashscene_core::VariantMember {
        name: None,
        overrides: vec![],
    }]);
    txn.set_variant(set, 1);
}
