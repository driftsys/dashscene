//! The staged binding tables (story #167): signal declarations and
//! binding rows as intent metadata on the arena.

use dashscene_core::{Arena, Channel, Prop, ScalarTransform};

#[test]
fn declared_signals_and_bindings_read_back_in_order() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("card"));
    let chip = txn.add_node(Some(root), Some("chip"));

    let gap = txn.declare_signal(Some("size/gap"), 16.0);
    let accent_r = txn.declare_signal(Some("color/accent.r"), 0.13);
    let anonymous = txn.declare_signal(None, 1.0);

    txn.bind(root, Channel::Gap, gap, ScalarTransform::Identity);
    txn.bind(chip, Channel::FillR, accent_r, ScalarTransform::Identity);
    txn.bind(chip, Channel::Width, anonymous, ScalarTransform::Scale(2.0));
    txn.commit();

    let signals = arena.signals();
    assert_eq!(signals.len(), 3);
    assert_eq!(signals[0].name.as_deref(), Some("size/gap"));
    assert_eq!(signals[0].initial, 16.0);
    assert_eq!(signals[2].name, None);

    let bindings = arena.bindings();
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].node, root);
    assert_eq!(bindings[0].channel, Channel::Gap);
    assert_eq!(bindings[0].signal, gap);
    assert_eq!(bindings[0].transform, ScalarTransform::Identity);
    assert_eq!(bindings[2].transform, ScalarTransform::Scale(2.0));
}

#[test]
fn binding_tables_do_not_change_committed_output() {
    // The tables are intent metadata: commit never reads them, so a
    // document with bindings commits the same output as one without (P3 —
    // flushing values is a producer-side runtime's job).
    let mut plain = Arena::new();
    let mut txn = plain.open();
    let node = txn.add_node(None, Some("bar"));
    txn.set_prop(node, Prop::Width(10.0));
    txn.commit();

    let mut bound = Arena::new();
    let mut txn = bound.open();
    let node = txn.add_node(None, Some("bar"));
    txn.set_prop(node, Prop::Width(10.0));
    let signal = txn.declare_signal(Some("w"), 10.0);
    txn.bind(node, Channel::Width, signal, ScalarTransform::Identity);
    txn.commit();

    assert_eq!(
        plain.committed().rects().len(),
        bound.committed().rects().len()
    );
    let (a, b) = (&plain.committed().rects()[0], &bound.committed().rects()[0]);
    assert_eq!((a.x, a.y, a.w, a.h), (b.x, b.y, b.w, b.h));
}

#[test]
fn intent_accessors_expose_parent_and_fill() {
    use dashscene_core::{Color, PaintKind};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("card"));
    let chip = txn.add_node(Some(root), Some("chip"));
    txn.set_prop(
        chip,
        Prop::Fill(Color {
            r: 0.5,
            g: 0.25,
            b: 0.125,
            a: 1.0,
        }),
    );
    txn.commit();

    assert_eq!(arena.parent(root), None);
    assert_eq!(arena.parent(chip), Some(root));
    assert_eq!(arena.fill(root), None);
    match arena.fill(chip) {
        Some(PaintKind::Solid { color }) => assert_eq!(color.r, 0.5),
        other => panic!("expected a solid fill, got {other:?}"),
    }
}

#[test]
#[should_panic(expected = "is not a signal declaration of this arena")]
fn binding_a_foreign_signal_panics() {
    // A SignalId is only meaningful for the arena that produced it. This
    // arena declares no signals, so a donor arena's id is out of range.
    let mut donor = Arena::new();
    let mut txn = donor.open();
    let foreign = txn.declare_signal(None, 0.0);
    drop(txn);

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.bind(node, Channel::X, foreign, ScalarTransform::Identity);
}

#[test]
#[should_panic(expected = "is not a node of this arena")]
fn binding_a_foreign_node_panics() {
    let mut donor = Arena::new();
    let mut txn = donor.open();
    let a = txn.add_node(None, None);
    let foreign = txn.add_node(Some(a), None);
    drop(txn);

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let _node = txn.add_node(None, None);
    let signal = txn.declare_signal(None, 0.0);
    txn.bind(foreign, Channel::X, signal, ScalarTransform::Identity);
}
