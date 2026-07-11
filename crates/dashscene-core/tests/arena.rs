//! Story #2 acceptance path: a scene built by hand through the staged
//! mutation API reads back as a resolved rect table + paint table
//! (issue #2; DESIGN_1.md §5, §7.3; SCOPE_DECISIONS.md §9).

use std::mem::{align_of, size_of};

use dashscene_core::{Arena, Color, NO_PAINT, Paint, Prop, RectEntry};

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
    assert_eq!(NO_PAINT, u32::MAX);

    let entry = RectEntry {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        paint: 0,
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
            paint: 0,
        }]
    );
    assert_eq!(scene.paints(), &[Paint { color: RED }]);
    assert_eq!(arena.name(root), Some("bg"));
}
