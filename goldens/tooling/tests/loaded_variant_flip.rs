//! Issue #617, end to end: a committed `.dsb` carrying a variant table is
//! loaded, switched, re-solved, and animated.
//!
//! What #617 measured is that all ten `goldens/dsb` fixtures that preceded
//! `v018-variant-shelf.dsb` report zero variant sets, so loading one seeds a
//! single commit and then has nothing left to drive — no FLIP path is
//! reachable, because nothing in any committed document would trigger one.
//! The fixture is authored rather than imported: `dashc`'s Figma path
//! resolves an `INSTANCE` to its one active subtree at compile time, so a
//! static REST export names one concrete state and has no switchable set to
//! preserve.
//!
//! This file is the other half of `crates/dashc/tests/round_trip.rs`'s
//! coverage of the same bytes. That one asserts what the *load* path
//! produces — the active member's overrides resolved into the committed
//! tables — and stops there, because `dashc` does not depend on the solver.
//! This one runs the solver, so it can assert the thing the fixture was
//! shaped for: **`right` moves although it carries no override**, purely
//! because hiding `middle` reflows the row. Those before/after rects are
//! what a FLIP binds its tracks from, and they exist nowhere in the
//! document — which is the point (P1).
//!
//! The path itself is story #22's, the same one `v04_flip.rs` walks: the
//! retained `TaffySolver` solves both sides of the switch, and a
//! `VariantFlip` binds the declared `VariantTransition` onto `dashcue`'s
//! scheduler. The difference is only where the scene came from.

use dashcue::{Easing, PropTransition, TransitionSpec, VariantTransition};
use dashscene_core::{Arena, NodeId, SolvedRect, load_document};
use dashscene_engine::{Channel, TaffySolver, VariantFlip, prop_key};

/// The committed fixture, read as bytes rather than recompiled: what #617
/// measured is a property of the file on disk, and a test that built its own
/// copy would still pass if the file were never written.
const FIXTURE: &[u8] = include_bytes!("../../dsb/v018-variant-shelf.dsb");

/// Every committed rect paired with its node, the slice shape
/// [`VariantFlip::start`] takes.
fn full_rects(arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
    let scene = arena.committed();
    scene
        .rects()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                scene.node_of(i as u32),
                SolvedRect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: r.h,
                },
            )
        })
        .collect()
}

fn rect_of(rects: &[(NodeId, SolvedRect)], node: NodeId) -> SolvedRect {
    rects
        .iter()
        .find(|(n, _)| *n == node)
        .map(|(_, r)| *r)
        .expect("node is present in the rect set")
}

#[test]
fn a_loaded_documents_variant_switch_reflows_and_animates() {
    let (document, payloads) = dashbuf::open_verified(FIXTURE).expect("the fixture is a .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);

    // `load_document` commits the authored intent; it does not solve, because
    // `dashscene-core` has no solver. One empty transaction through the
    // retained solver turns that intent into the before layout.
    let mut solver = TaffySolver::new();
    {
        let txn = arena.open();
        txn.commit_with(&mut solver);
    }

    let shelf = arena.roots()[0];
    let chips = arena.children(shelf).to_vec();
    let (left, middle, right) = (chips[0], chips[1], chips[2]);

    let before = full_rects(&arena);
    assert_eq!(
        (rect_of(&before, left).x, rect_of(&before, left).w),
        (4.0, 40.0),
        "full: the left chip sits at the padding, at its authored width",
    );
    assert_eq!(
        (rect_of(&before, right).x, rect_of(&before, right).w),
        (100.0, 40.0),
        "full: padding 4 + left 40 + gap 8 + middle 40 + gap 8",
    );

    // The switch a loaded document could not make until `Arena::variant_sets`
    // existed: `VariantSetId` is otherwise minted only by
    // `Txn::add_variant_set`, which the loader calls internally and drops.
    let set = arena
        .variant_sets()
        .next()
        .expect("the fixture carries one variant set");
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
        txn.commit_with(&mut solver);
    }

    let after = full_rects(&arena);
    assert_eq!(
        (rect_of(&after, left).x, rect_of(&after, left).w),
        (4.0, 64.0),
        "collapsed: the left chip took its width override",
    );
    assert_eq!(
        (rect_of(&after, right).x, rect_of(&after, right).w),
        (76.0, 40.0),
        "collapsed: padding 4 + left 64 + gap 8, with middle gone from the row",
    );
    assert_ne!(
        rect_of(&before, right).x,
        rect_of(&after, right).x,
        "the right chip states no override and still moved — the reflow a FLIP \
         animates, and a value the document does not carry (P1)",
    );
    assert!(
        !arena.layout(middle).visible,
        "the collapsed member hid the middle chip, which is what freed the space",
    );

    // A 1-second linear tween on the reflowed chip, so t = 0.5 is the exact
    // midpoint of 100 -> 76.
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: prop_key(right, Channel::X),
            spec: TransitionSpec::Tween {
                duration: 1.0,
                easing: Easing::Linear,
            },
        }],
        stagger: 0.0,
    };
    let mut flip = VariantFlip::new();
    flip.start(&before, &after, &transition);

    assert_eq!(
        flip.sample(right).expect("the right chip is animating").x,
        100.0,
        "t = 0 holds the before layout",
    );
    flip.advance(0.5);
    assert_eq!(
        flip.sample(right).expect("the right chip is animating").x,
        88.0,
        "t = 0.5 is the midpoint of a reflow the document never stated",
    );
    flip.advance(0.5);
    assert_eq!(
        flip.sample(right).expect("the right chip is animating").x,
        76.0,
        "t = 1 reaches the after layout",
    );
}

#[test]
fn a_document_loaded_from_a_file_animates_through_the_frame_loop() {
    // The epic's definition-of-done line: "a `.dsb` carries a transition, and
    // a document loaded from a file animates without Rust written against
    // `dashlang`". Everything below the `set_variant` call is the ordinary
    // frame loop — `LiveScene::tick`, exactly as both hosts call it
    // (`dashscene-desktop`'s `frame`, `dashscene-web`'s tick) — and the
    // motion comes from the file, not from this test.
    //
    // The test above drives `VariantFlip` by hand, which is what a host had
    // to do before this. This one does not mention a flip, a scheduler or a
    // spec: it stages the switch an embedder stages and then runs frames.
    let (document, payloads) = dashbuf::open_verified(FIXTURE).expect("the fixture is a .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    let mut live = dashlang::attach_live(&mut arena, Box::new(TaffySolver::new()));

    let shelf = arena.roots()[0];
    let right = arena.children(shelf)[2];
    let x_of = |arena: &Arena| {
        let scene = arena.committed();
        let index = (0..scene.rects().len())
            .find(|i| scene.node_of(*i as u32) == right)
            .expect("the right chip is in the committed table");
        scene.rects()[index].x
    };
    assert_eq!(x_of(&arena), 100.0, "the loaded document's full layout");

    // What an embedder does through the host's scene seam: stage the switch
    // on the arena. It does not commit — the frame loop owns the commit.
    {
        let set = arena
            .variant_sets()
            .next()
            .expect("the fixture carries one variant set");
        let mut txn = arena.open();
        txn.set_variant(set, 1);
    }

    // Frames are `MAX_FRAME_DELTA` (0.1 s) each: `tick` clamps the interval
    // a host hands it, so ten of them are the one-second tween the file
    // declares. Asking for a larger step here would silently be the same
    // 0.1 and read as an animation running slow.
    //
    // One frame in. If the switch landed in a single frame — the behaviour
    // before this story — this would already be 76.
    live.tick(dashlang::MAX_FRAME_DELTA, &mut arena);
    assert_eq!(
        x_of(&arena),
        97.6,
        "the switch is easing, not landing: 100 + (76 - 100) * 0.1",
    );

    for _ in 0..4 {
        live.tick(dashlang::MAX_FRAME_DELTA, &mut arena);
    }
    assert_eq!(x_of(&arena), 88.0, "halfway through the declared duration");

    for _ in 0..5 {
        live.tick(dashlang::MAX_FRAME_DELTA, &mut arena);
    }
    assert_eq!(x_of(&arena), 76.0, "the tween reaches the after layout");

    // And settles: a further frame moves nothing and commits nothing, so the
    // generation holds steady and the idle skip is intact.
    let settled = live.tick(dashlang::MAX_FRAME_DELTA, &mut arena);
    assert_eq!(x_of(&arena), 76.0, "nothing moves after the tween finishes");
    assert_eq!(
        live.tick(dashlang::MAX_FRAME_DELTA, &mut arena),
        settled,
        "an idle frame after the animation advances no generation",
    );
}
