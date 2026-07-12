//! Story #5 acceptance: a scene built via the DSL matches the same
//! scene built by hand through `dashscene-core` directly (issue #5;
//! DESIGN_1.md §6.2).

// Arena and Color come through dashlang's re-export: a DSL consumer
// needs no direct dashscene-core dependency. Prop/PaintEntry are the
// raw core surface, imported directly for the hand-built comparison
// side.
use dashlang::{Arena, Color, anon, node, rgba, scene};
use dashscene_core::{PaintEntry, Prop};

/// The two arenas must have committed to identical painter input, and
/// the DSL's names must have reached the arena (observable through the
/// NodeId↔rect-index correspondence).
fn assert_same_output(dsl: &Arena, hand: &Arena) {
    assert_eq!(dsl.committed().rects(), hand.committed().rects());
    assert_eq!(dsl.committed().paints(), hand.committed().paints());
    let names = |arena: &Arena| -> Vec<Option<String>> {
        (0..arena.committed().rects().len())
            .map(|i| {
                arena
                    .name(arena.committed().node_of(u32::try_from(i).unwrap()))
                    .map(String::from)
            })
            .collect()
    };
    assert_eq!(names(dsl), names(hand));
}

// The re-exported Color is nameable in consumer signatures.
#[allow(dead_code)]
fn takes_a_dashlang_color(_: Color) {}

#[test]
fn the_dsl_scene_matches_the_hand_built_scene() {
    let bg = rgba(0.1, 0.2, 0.3, 1.0);
    let red = rgba(1.0, 0.0, 0.0, 1.0);

    let mut dsl = Arena::new();
    scene([node("bg")
        .size(320.0, 240.0)
        .fill(bg)
        .child(node("badge").at(10.0, 10.0).size(24.0, 24.0).fill(red))])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::X(0.0));
    txn.set_prop(root, Prop::Y(0.0));
    txn.set_prop(root, Prop::Width(320.0));
    txn.set_prop(root, Prop::Height(240.0));
    txn.set_prop(root, Prop::Fill(bg));
    let badge = txn.add_node(Some(root), Some("badge"));
    txn.set_prop(badge, Prop::X(10.0));
    txn.set_prop(badge, Prop::Y(10.0));
    txn.set_prop(badge, Prop::Width(24.0));
    txn.set_prop(badge, Prop::Height(24.0));
    txn.set_prop(badge, Prop::Fill(red));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn repeater_children_come_from_an_iterator_in_order() {
    let red = rgba(1.0, 0.0, 0.0, 1.0);

    let mut dsl = Arena::new();
    scene([node("row")
        .children((0..3).map(|i| anon().at(30.0 * i as f32, 0.0).size(24.0, 24.0).fill(red)))])
    .build(&mut dsl);

    let rects = dsl.committed().rects();
    assert_eq!(rects.len(), 4);
    let child_xs: Vec<f32> = rects[1..].iter().map(|r| r.x).collect();
    assert_eq!(child_xs, [0.0, 30.0, 60.0]);
}

#[test]
fn multiple_roots_keep_declaration_order() {
    let mut dsl = Arena::new();
    scene([
        node("first").size(1.0, 1.0),
        node("second").at(5.0, 0.0).size(1.0, 1.0),
    ])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let first = txn.add_node(None, Some("first"));
    txn.set_prop(first, Prop::Width(1.0));
    txn.set_prop(first, Prop::Height(1.0));
    let second = txn.add_node(None, Some("second"));
    txn.set_prop(second, Prop::X(5.0));
    txn.set_prop(second, Prop::Width(1.0));
    txn.set_prop(second, Prop::Height(1.0));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn build_appends_to_a_non_empty_arena_and_commits_exactly_once() {
    let mut arena = Arena::new();
    scene([node("first").size(1.0, 1.0)]).build(&mut arena);
    assert_eq!(arena.committed().generation(), 1);

    let generation = scene([node("second").size(2.0, 2.0)]).build(&mut arena);

    assert_eq!(generation, 2);
    assert_eq!(arena.committed().generation(), 2);
    assert_eq!(arena.committed().rects().len(), 2);
    assert_eq!(arena.committed().rects()[1].w, 2.0);
}

#[test]
fn unset_fill_and_geometry_keep_core_defaults() {
    let mut dsl = Arena::new();
    scene([anon()]).build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    txn.add_node(None, None);
    txn.commit();

    assert_same_output(&dsl, &hand);
    // An unfilled node resolves to the shared draws-nothing entry
    // (story #4 boundary-B unification).
    let scene = dsl.committed();
    assert_eq!(
        scene.paints().resolve(scene.rects()[0].paint),
        &PaintEntry::default()
    );
    assert_eq!(scene.rects()[0].w, 0.0);
}
