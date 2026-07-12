//! Story #2 acceptance path: a scene built by hand through the staged
//! mutation API reads back as a resolved rect table + paint table
//! (issue #2; DESIGN_1.md §5, §7.3; SCOPE_DECISIONS.md §9).

use std::mem::{align_of, size_of};

use dashscene_core::{Arena, Color, PaintEntry, PaintIndex, Prop, RectEntry, TextStyle};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[test]
fn committed_entries_are_blittable_plain_data() {
    // Boundary B pins the rect entry as blittable plain data:
    // x, y, w, h (f32) + paint index (u32), and the solid-fill color
    // as 4xf32 RGBA (dashbuf's Color shape).
    assert_eq!(size_of::<RectEntry>(), 20);
    assert_eq!(align_of::<RectEntry>(), 4);
    assert_eq!(size_of::<Color>(), 16);
    assert_eq!(align_of::<Color>(), 4);

    let entry = RectEntry {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        paint: PaintIndex(0),
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
fn a_fill_change_marks_the_rect_dirty_even_when_its_paint_index_is_stable() {
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

    // The re-interned paint table assigns BLUE index 0 again, so the
    // rect entry's bits are unchanged — but its resolved color is not.
    let mut txn = arena.open();
    txn.set_prop(node, Prop::Fill(BLUE));
    txn.commit();

    assert_eq!(arena.committed().rects()[0].paint, PaintIndex(0));
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

    // Swapping the fills keeps both paint indices bit-identical
    // (first-use interning assigns 0 and 1 again) while both resolved
    // colors change.
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
            size_px: 16.0,
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
    assert_eq!(arena.text_of(label), Some("Speed"));
    let style = arena.text_style_of(label).expect("style set");
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
    assert_eq!(arena.text_of(n), Some("pending"));
}

#[test]
fn text_props_replace_previous_values() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Text("old".to_string()));
    txn.set_prop(n, Prop::Text("new".to_string()));
    txn.commit();
    assert_eq!(arena.text_of(n), Some("new"));
}

#[test]
fn nodes_without_text_read_none() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.commit();
    assert_eq!(arena.text_of(n), None);
    assert!(arena.text_style_of(n).is_none());
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
