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
use dashscene_core::{
    Arena, AxisSizing, LayoutMode, NodeId, Prop, SolvedRect, VariantMember, VariantValue,
};
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
fn an_appearing_disappearing_node_pops_while_a_reflowing_sibling_tweens() {
    // Issue #293: story #283 named, but did not fix, a FLIP limit. FLIP
    // (`crates/dashscene-engine/src/flip.rs` module docs) animates rect
    // channels only (X/Y/Width/Height); there is no visibility or opacity
    // channel. A `set_variant` toggle that hides a child gives that child a
    // degenerate rect (`docs/decisions/variant-set-flat-index.md`,
    // story #283), but nothing about "degenerate" is itself a rect-channel
    // value a transition can fade through — so a real transition (like the
    // ones this file hand-builds elsewhere) only ever declares tracks for
    // the nodes that move (the reflowing siblings), never for the node that
    // is appearing or disappearing. The result: the appearing/disappearing
    // node has no live FLIP track at all — it "pops" straight from its old
    // committed rect to its new one with no interpolated frame — while a
    // sibling with a declared track genuinely tweens.
    //
    // Same three-chip Hug row as the E3 corpus case
    // (`crates/dashlang/tests/corpus.rs`) and the incremental-path test
    // (`crates/dashscene-engine/tests/incremental.rs`): hugs to 90 with all
    // shown, 60 with the middle chip (`b`) hidden — `c` reflows left by 30.
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let (b, c, set) = {
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(20.0));
        let a = txn.add_node(Some(row), None);
        txn.set_prop(a, Prop::Width(30.0));
        txn.set_prop(a, Prop::Height(20.0));
        let b = txn.add_node(Some(row), None);
        txn.set_prop(b, Prop::Width(30.0));
        txn.set_prop(b, Prop::Height(20.0));
        let c = txn.add_node(Some(row), None);
        txn.set_prop(c, Prop::Width(30.0));
        txn.set_prop(c, Prop::Height(20.0));
        let set = txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("hide-middle".to_string()),
                overrides: vec![(b, VariantValue::Visible(false))],
            },
        ]);
        txn.commit_with(&mut solver);
        (b, c, set)
    };

    // First: capture both children's before rects from the solved,
    // committed scene.
    let b_before = committed_rect(&arena, b);
    let c_before = committed_rect(&arena, c);
    // b shown, in the middle; c last.
    assert_rect(b_before, (30.0, 0.0, 30.0, 20.0));
    assert_rect(c_before, (60.0, 0.0, 30.0, 20.0));

    // Switch to hide-middle and re-solve. Last: b's rect is degenerate; c
    // reflowed into b's old place.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
        txn.commit_with(&mut solver);
    }
    let b_after = committed_rect(&arena, b);
    let c_after = committed_rect(&arena, c);
    // b leaves the laid-out set (degenerate rect); c reflows into its place.
    assert_rect(b_after, (0.0, 0.0, 0.0, 0.0));
    assert_rect(c_after, (30.0, 0.0, 30.0, 20.0));

    // The declared transition names only c's X channel — the reflow — a
    // 1-second linear tween. It names no channel of b's: nothing about
    // "disappearing" is expressible as a rect-channel track (no visibility
    // or opacity channel exists), so a real transition simply has nothing
    // to say about b, the same as `dashcue`'s only consumer of this path
    // does today (no auto-generated track for an appearing/disappearing
    // node exists anywhere in the tree, "no new FLIP machinery" per the
    // story #283 module docs).
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: prop_key(c, Channel::X),
            spec: TransitionSpec::Tween {
                duration: 1.0,
                easing: Easing::Linear,
            },
        }],
        stagger: 0.0,
    };

    let mut flip = VariantFlip::new();
    flip.start(
        &[(b, b_before), (c, c_before)],
        &[(b, b_after), (c, c_after)],
        &transition,
    );

    // b has no live track at any point in the animation: it never appears
    // in the FLIP's animated set, so it is never interpolated — it pops
    // directly from `b_before` to `b_after` with no in-between frame. The
    // whole-frame iterator (#23's sampling surface) never yields it either.
    assert!(
        flip.sample(b).is_none(),
        "b has no declared track and is not tweened at t = 0"
    );
    let frame_t0: Vec<_> = flip.sampled_rects().map(|(n, _)| n).collect();
    assert!(
        !frame_t0.contains(&b),
        "b is absent from the animated frame at t = 0"
    );

    // c, which does have a declared track, genuinely tweens: at t = 0 it
    // holds the before value.
    assert_rect(
        flip.sample(c).expect("c is animating"),
        (60.0, 0.0, 30.0, 20.0),
    );

    // Midway: c is at the exact midpoint; b is still untracked, so its rect
    // never took an in-between value through FLIP.
    flip.advance(0.5);
    assert_rect(
        flip.sample(c).expect("c is animating"),
        (45.0, 0.0, 30.0, 20.0),
    );
    assert!(
        flip.sample(b).is_none(),
        "b is still untracked halfway through c's tween -- it pops, it does not tween"
    );

    // Finish: c reaches its after value; b was never part of the animation.
    flip.advance(0.5);
    assert_rect(
        flip.sample(c).expect("c is animating"),
        (30.0, 0.0, 30.0, 20.0),
    );
    assert!(flip.sample(b).is_none(), "b was never tweened");
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

/// A one-second linear tween — the spec every #487 test below declares, so
/// t = 0.5 lands on a midpoint exactly.
fn linear_second() -> TransitionSpec {
    TransitionSpec::Tween {
        duration: 1.0,
        easing: Easing::Linear,
    }
}

/// A node with one committed rect, for a FLIP that needs no real solve.
fn lone_node() -> (Arena, NodeId) {
    let mut arena = Arena::new();
    let node = {
        let mut txn = arena.open();
        let node = txn.add_node(None, None);
        txn.commit();
        node
    };
    (arena, node)
}

#[test]
fn a_switch_where_no_declared_channel_moved_animates_nothing() {
    // Debt #487. Every rect channel is declared, and the switch moves none
    // of them. Each track would sample the value the node already holds for
    // a whole second. The engine declines all four, so nothing is scheduled
    // and the node never enters the animated set — which is the same place
    // a node with no declared track already sits.
    let (_arena, node) = lone_node();
    let rect = SolvedRect {
        x: 10.0,
        y: 20.0,
        w: 30.0,
        h: 40.0,
    };
    let transition = VariantTransition {
        tracks: [Channel::X, Channel::Y, Channel::Width, Channel::Height]
            .into_iter()
            .map(|channel| PropTransition {
                prop: prop_key(node, channel),
                spec: linear_second(),
            })
            .collect(),
        stagger: 0.0,
    };

    let mut flip = VariantFlip::new();
    flip.start(&[(node, rect)], &[(node, rect)], &transition);

    assert!(
        flip.is_empty(),
        "a switch that moved nothing animates nothing"
    );
    assert!(flip.sample(node).is_none(), "the node is not animating");
    assert_eq!(flip.sampled_rects().count(), 0, "and yields no frame rect");
}

#[test]
fn an_unmoved_channel_of_a_moving_node_still_samples_its_after_value() {
    // The other half of #487's contract: declining a channel must not change
    // what a consumer reads. The node's X moves and its width does not, so
    // the node stays in the animated set and the declined width channel
    // samples the after value at every step — exactly what a live track
    // pinned at from == to would have produced.
    let (_arena, node) = lone_node();
    let before = SolvedRect {
        x: 10.0,
        y: 20.0,
        w: 30.0,
        h: 40.0,
    };
    let after = SolvedRect { x: 110.0, ..before };
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: prop_key(node, Channel::X),
                spec: linear_second(),
            },
            PropTransition {
                prop: prop_key(node, Channel::Width),
                spec: linear_second(),
            },
        ],
        stagger: 0.0,
    };

    let mut flip = VariantFlip::new();
    flip.start(&[(node, before)], &[(node, after)], &transition);
    assert_rect(
        flip.sample(node).expect("x is animating"),
        (10.0, 20.0, 30.0, 40.0),
    );
    flip.advance(0.5);
    assert_rect(
        flip.sample(node).expect("x is animating"),
        (60.0, 20.0, 30.0, 40.0),
    );
    flip.advance(0.5);
    assert_rect(
        flip.sample(node).expect("x is animating"),
        (110.0, 20.0, 30.0, 40.0),
    );
}

#[test]
fn declining_an_unmoved_channel_leaves_the_later_tracks_stagger_alone() {
    // #487 rests on `dashcue` computing a track's delay from its DECLARED
    // index (#74), so a declined track leaves the rest of the switch on the
    // schedule the author wrote. Width is declared first and does not move;
    // X is declared second and does. X must still wait one stagger step, as
    // it would if the width track had started.
    let (_arena, node) = lone_node();
    let before = SolvedRect {
        x: 10.0,
        y: 20.0,
        w: 30.0,
        h: 40.0,
    };
    let after = SolvedRect { x: 110.0, ..before };
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: prop_key(node, Channel::Width),
                spec: linear_second(),
            },
            PropTransition {
                prop: prop_key(node, Channel::X),
                spec: linear_second(),
            },
        ],
        stagger: 0.5,
    };

    let mut flip = VariantFlip::new();
    flip.start(&[(node, before)], &[(node, after)], &transition);

    // Still inside X's 0.5-second delay: it holds its `from`.
    flip.advance(0.25);
    assert_eq!(flip.sample(node).expect("x is animating").x, 10.0);
    // The delay is spent; a further half second is half of X's tween.
    flip.advance(0.75);
    assert_eq!(flip.sample(node).expect("x is animating").x, 60.0);
}
