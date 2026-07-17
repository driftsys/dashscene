//! Minimal FLIP acceptance (issue #22): a `set_variant` switch animates
//! between two solved layouts, deterministically under a fixed time step
//! (the way #23's E5 goldens sample at t = 0 / 0.5 / 1); a second switch
//! mid-flight retargets the running animation without snapping; and a spring
//! FLIP replays bit-identically.
//!
//! The primary test drives the whole path: the retained `TaffySolver` (#164)
//! produces the before and after rects, and `VariantFlip` binds the declared
//! `VariantTransition` onto `dashcue`'s scheduler and samples the animated
//! geometry.

use dashcue::{Easing, PropTransition, TransitionSpec, VariantTransition};
use dashscene_core::{Arena, NodeId, Prop, SolvedRect, VariantMember, VariantValue};
use dashscene_engine::{Channel, TaffySolver, VariantFlip, prop_key};

/// The child's resolved rect in the last committed scene.
fn committed_rect(arena: &Arena, node: NodeId) -> SolvedRect {
    let index = arena
        .committed()
        .rect_index_of(node)
        .expect("the node has a committed rect");
    let entry = &arena.committed().rects()[index as usize];
    SolvedRect {
        x: entry.x,
        y: entry.y,
        w: entry.w,
        h: entry.h,
    }
}

fn assert_rect(actual: SolvedRect, expected: (f32, f32, f32, f32)) {
    assert_eq!(
        (actual.x, actual.y, actual.w, actual.h),
        expected,
        "sampled rect mismatch"
    );
}

#[test]
fn variant_switch_animates_between_two_solved_layouts() {
    // A mode-None root with one absolutely-placed child. A variant set moves
    // the child (X 10 -> 110) and grows it (W 50 -> 80); Y and H do not
    // change. The retained solver gives the before and after rects.
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let (child, set) = {
        let mut txn = arena.open();
        let root = txn.add_node(None, None);
        txn.set_prop(root, Prop::Width(200.0));
        txn.set_prop(root, Prop::Height(100.0));
        let child = txn.add_node(Some(root), None);
        txn.set_prop(child, Prop::X(10.0));
        txn.set_prop(child, Prop::Y(10.0));
        txn.set_prop(child, Prop::Width(50.0));
        txn.set_prop(child, Prop::Height(50.0));
        let set = txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("moved".to_string()),
                overrides: vec![
                    (child, VariantValue::X(110.0)),
                    (child, VariantValue::Width(80.0)),
                ],
            },
        ]);
        txn.commit_with(&mut solver);
        (child, set)
    };

    // First: capture the before rect from the solved, committed scene.
    let before = [(child, committed_rect(&arena, child))];
    assert_rect(before[0].1, (10.0, 10.0, 50.0, 50.0));

    // Switch the variant and re-solve. Last: the after rect is the new solve.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
        txn.commit_with(&mut solver);
    }
    let after = [(child, committed_rect(&arena, child))];
    assert_rect(after[0].1, (110.0, 10.0, 80.0, 50.0));

    // The declared transition animates exactly the two moved channels with a
    // 1-second linear tween (so t = 0.5 lands on the midpoint exactly).
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: prop_key(child, Channel::X),
                spec: TransitionSpec::Tween {
                    duration: 1.0,
                    easing: Easing::Linear,
                },
            },
            PropTransition {
                prop: prop_key(child, Channel::Width),
                spec: TransitionSpec::Tween {
                    duration: 1.0,
                    easing: Easing::Linear,
                },
            },
        ],
        stagger: 0.0,
    };

    let mut flip = VariantFlip::new();
    flip.start(&before, &after, &transition);

    // t = 0: the before layout. Y and H hold their (unchanged) after values.
    assert_rect(
        flip.sample(child).expect("child is animating"),
        (10.0, 10.0, 50.0, 50.0),
    );

    // t = 0.5: the exact midpoint of both moved channels.
    flip.advance(0.5);
    assert_rect(
        flip.sample(child).expect("child is animating"),
        (60.0, 10.0, 65.0, 50.0),
    );
    // The whole-frame iterator #23 reads yields the one animating node, same
    // rect the point sample gives.
    let frame: Vec<_> = flip.sampled_rects().collect();
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].0, child);
    assert_rect(frame[0].1, (60.0, 10.0, 65.0, 50.0));

    // t = 1.0: the tween reaches its target on the finishing frame.
    flip.advance(0.5);
    assert_rect(
        flip.sample(child).expect("child is animating"),
        (110.0, 10.0, 80.0, 50.0),
    );

    // The next advance drops the finished tracks; the node stops animating.
    flip.advance(0.5);
    assert!(flip.sample(child).is_none(), "finished tracks are dropped");
    assert!(flip.is_empty());
}

#[test]
fn a_second_switch_mid_flight_retargets_without_snapping() {
    // A single node whose X animates 10 -> 110 over a 1-second linear tween.
    // Halfway through (X = 60) a second switch retargets X back toward 10.
    // The scheduler's retarget resumes from the current sample, so the sample
    // stays 60 across the switch — it does not snap to a fresh `from`.
    let mut arena = Arena::new();
    let node = {
        let mut txn = arena.open();
        let node = txn.add_node(None, None);
        txn.set_prop(node, Prop::Width(50.0));
        txn.set_prop(node, Prop::Height(50.0));
        txn.commit_with(&mut TaffySolver::new());
        node
    };

    let transition = || VariantTransition {
        tracks: vec![PropTransition {
            prop: prop_key(node, Channel::X),
            spec: TransitionSpec::Tween {
                duration: 1.0,
                easing: Easing::Linear,
            },
        }],
        stagger: 0.0,
    };

    let mut flip = VariantFlip::new();
    // Switch A: X 10 -> 110.
    flip.start(
        &[(node, rect_x(10.0))],
        &[(node, rect_x(110.0))],
        &transition(),
    );
    flip.advance(0.5);
    assert_eq!(flip.sample(node).unwrap().x, 60.0, "midpoint of switch A");

    // Switch B: retarget X toward 10. The `before` X here is a sentinel that
    // must be ignored — the scheduler resumes from the current sample (60).
    flip.start(
        &[(node, rect_x(-999.0))],
        &[(node, rect_x(10.0))],
        &transition(),
    );
    assert_eq!(
        flip.sample(node).unwrap().x,
        60.0,
        "retarget resumes from the current sample, not the passed `before`"
    );

    // Halfway through switch B: X = 60 + 0.5 * (10 - 60) = 35.
    flip.advance(0.5);
    assert_eq!(
        flip.sample(node).unwrap().x,
        35.0,
        "midpoint of the retarget"
    );
}

#[test]
fn a_spring_flip_replays_bit_identically() {
    // A spring FLIP fed the same fixed steps twice produces bit-identical
    // samples — the determinism the E5 goldens rely on.
    let mut arena = Arena::new();
    let node = {
        let mut txn = arena.open();
        let node = txn.add_node(None, None);
        txn.set_prop(node, Prop::Width(50.0));
        txn.set_prop(node, Prop::Height(50.0));
        txn.commit_with(&mut TaffySolver::new());
        node
    };

    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: prop_key(node, Channel::X),
            spec: TransitionSpec::Spring {
                stiffness: 120.0,
                damping_ratio: 0.7,
            },
        }],
        stagger: 0.0,
    };

    let run = || {
        let mut flip = VariantFlip::new();
        flip.start(
            &[(node, rect_x(0.0))],
            &[(node, rect_x(100.0))],
            &transition,
        );
        let mut samples = Vec::new();
        for _ in 0..30 {
            flip.advance(1.0 / 60.0);
            samples.push(flip.sample(node).map(|r| r.x.to_bits()));
        }
        samples
    };

    assert_eq!(
        run(),
        run(),
        "a fixed-step spring FLIP replays bit-identically"
    );
}

/// A 50x50 rect whose X is `x` — the geometry the retarget and spring tests
/// vary along one channel.
fn rect_x(x: f32) -> SolvedRect {
    SolvedRect {
        x,
        y: 0.0,
        w: 50.0,
        h: 50.0,
    }
}

// -- The one prop-key packing (debts #207/#208): core's math, typed here ------

#[test]
fn prop_key_round_trips_through_the_canonical_decoder() {
    use dashscene_engine::decode_prop_key;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    let child = txn.add_node(Some(root), None);
    txn.commit();

    for channel in [
        Channel::X,
        Channel::Y,
        Channel::Width,
        Channel::Height,
        Channel::Gap,
        Channel::FillR,
        Channel::FillG,
        Channel::FillB,
        Channel::FillA,
    ] {
        let key = prop_key(child, channel);
        assert_eq!(
            decode_prop_key(key),
            Some((child.index() as u32, channel)),
            "{channel:?} round-trips"
        );
    }

    // A key whose low byte is no channel code was not built by prop_key.
    assert_eq!(decode_prop_key(dashcue::PropKey(0xFF)), None);
}

#[test]
#[should_panic(expected = "is not an engine-packed prop key")]
fn a_foreign_prop_key_is_refused_by_name() {
    // A raw key a producer packed itself (debt #207): the low byte (0xAB)
    // is no channel code, so the track cannot be an engine-packed key and
    // the FLIP refuses it by name instead of silently mis-binding it.
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: dashcue::PropKey(0xABCD_00AB),
            spec: TransitionSpec::Tween {
                duration: 1.0,
                easing: Easing::Linear,
            },
        }],
        stagger: 0.0,
    };
    VariantFlip::new().start(&[], &[], &transition);
}

#[test]
#[should_panic(expected = "is not a rect channel")]
fn a_non_rect_channel_track_is_refused_by_name() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.commit();

    // Gap is a legitimate binding channel but not a rect channel; FLIP
    // animates rects only, so the track is refused by name.
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: prop_key(node, Channel::Gap),
            spec: TransitionSpec::Tween {
                duration: 1.0,
                easing: Easing::Linear,
            },
        }],
        stagger: 0.0,
    };
    VariantFlip::new().start(&[], &[], &transition);
}
