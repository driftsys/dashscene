//! v0.4 FLIP goldens (issue #23; docs/specification/05-qualification.md
//! E5, `DESIGN_1.md` §11): a variant switch sampled at t = 0 / 0.5 / 1
//! under a deterministic clock, each sample rendered through the Skia
//! reference painter and compared against a checked-in golden. These
//! three goldens are exit criterion E5.
//!
//! The path proven here is the one story #22 exposed for exactly this
//! sampling (`crates/dashscene-engine/tests/flip.rs`): the retained
//! `TaffySolver` (#164) solves the before and after layouts of a
//! `set_variant` switch; a `VariantFlip` binds the declared
//! `VariantTransition` onto `dashcue`'s scheduler; and a fixed-step
//! `advance` then `sample` reads the animated geometry deterministically.
//!
//! To turn a sample into a rendered frame the sampled rects are composed
//! into a full rect set — the after layout with each animating node's
//! rect replaced by its current sample — and committed through a
//! fixed-rect `LayoutSolver` (the `CachedSolver` pattern of
//! `crates/dashlang/src/reactive.rs`), so the painter renders the sampled
//! geometry without re-solving.
//!
//! A 1-second linear tween makes t = 0.5 land on the exact midpoint, and
//! every authored coordinate and every midpoint is an integer, so — like
//! the v0.2 flex goldens — the solid fills are integer-aligned and the
//! goldens compare exactly (`docs/decisions/golden-comparison-space.md`).
//!
//! Regeneration and diff workflow: goldens/README.md.

use dashcue::{Easing, PropTransition, TransitionSpec, VariantTransition};
use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{
    Arena, Color, LayoutMode, LayoutSolver, NodeId, Prop, SolvedRect, VariantMember, VariantValue,
};
use dashscene_engine::{Channel, TaffySolver, VariantFlip, prop_key};
use dashscene_skia::SkiaPainter;

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

const NAVY: Color = rgb(0.05, 0.1, 0.2);
const RED: Color = rgb(0.8, 0.1, 0.1);
const BLUE: Color = rgb(0.2, 0.4, 0.9);

/// A `LayoutSolver` that replays a fixed set of rects — the same
/// `CachedSolver` pattern `crates/dashlang/src/reactive.rs` uses to
/// publish geometry without invoking the real solver. Feeding the
/// composed sample to `commit_with` renders it without re-solving.
struct FixedRects(Vec<(NodeId, SolvedRect)>);

impl LayoutSolver for FixedRects {
    fn solve(&mut self, _arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        self.0.clone()
    }
}

/// Every committed node's rect, in DFS/rect-table order — the full set a
/// `FixedRects` commit needs so no node is left unresolved.
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

/// The rect of `node` in `rects`.
fn rect_of(rects: &[(NodeId, SolvedRect)], node: NodeId) -> SolvedRect {
    rects
        .iter()
        .find(|(n, _)| *n == node)
        .map(|(_, r)| *r)
        .expect("node is present in the rect set")
}

fn assert_rect(actual: SolvedRect, expected: (f32, f32, f32, f32), label: &str) {
    assert_eq!(
        (actual.x, actual.y, actual.w, actual.h),
        expected,
        "{label}"
    );
}

/// Compose the current FLIP sample into a full rect set (the after layout
/// with each animating node's rect overlaid), commit it through a
/// fixed-rect solver, render the committed scene on a canvas sized to the
/// root rect, and compare against the exact-match golden `name`.
fn render_sample(
    arena: &mut Arena,
    after: &[(NodeId, SolvedRect)],
    flip: &VariantFlip,
    name: &str,
) {
    let mut composed = after.to_vec();
    for (node, rect) in flip.sampled_rects() {
        let slot = composed
            .iter_mut()
            .find(|(n, _)| *n == node)
            .expect("an animating node is part of the after layout");
        slot.1 = rect;
    }

    {
        let txn = arena.open();
        txn.commit_with(&mut FixedRects(composed));
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    assert_eq!(root.w, root.w.round(), "root width is integral");
    assert_eq!(root.h, root.h.round(), "root height is integral");
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &GlyphRunTable::new(),
        None,
    );
    goldens::assert_matches_golden(name, &painter.png_bytes());
}

#[test]
fn variant_transition_goldens_at_t_0_half_and_1() {
    // A 220x120 navy backdrop (mode None, so children place by their
    // authored offsets). A static blue anchor bar sits along the bottom;
    // a red chip animates across the top. The variant switch moves the
    // chip (X 20 -> 140) and grows it (W 40 -> 60); Y and H do not
    // change. Every authored coordinate and every t = 0.5 midpoint
    // (X 80, W 50) is an integer, so the solid fills stay integer-aligned
    // and the goldens compare exactly.
    let mut arena = Arena::new();
    let mut solver = TaffySolver::new();

    let (chip, set) = {
        let mut txn = arena.open();
        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(220.0));
        txn.set_prop(root, Prop::Height(120.0));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));

        let anchor = txn.add_node(Some(root), Some("anchor"));
        txn.set_prop(anchor, Prop::X(20.0));
        txn.set_prop(anchor, Prop::Y(88.0));
        txn.set_prop(anchor, Prop::Width(180.0));
        txn.set_prop(anchor, Prop::Height(12.0));
        txn.set_prop(anchor, Prop::Fill(BLUE));

        let chip = txn.add_node(Some(root), Some("chip"));
        txn.set_prop(chip, Prop::X(20.0));
        txn.set_prop(chip, Prop::Y(24.0));
        txn.set_prop(chip, Prop::Width(40.0));
        txn.set_prop(chip, Prop::Height(50.0));
        txn.set_prop(chip, Prop::Fill(RED));

        // The chip keeps its RED fill in both members: only geometry
        // animates, so a single committed paint table is correct at
        // every sample.
        let set = txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("expanded".to_string()),
                overrides: vec![
                    (chip, VariantValue::X(140.0)),
                    (chip, VariantValue::Width(60.0)),
                ],
            },
        ]);
        txn.commit_with(&mut solver);
        (chip, set)
    };

    // First: the before layout (member 0 active).
    let before = full_rects(&arena);
    assert_rect(
        rect_of(&before, chip),
        (20.0, 24.0, 40.0, 50.0),
        "before chip",
    );

    // Last: switch to the expanded member and re-solve.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
        txn.commit_with(&mut solver);
    }
    let after = full_rects(&arena);
    assert_rect(
        rect_of(&after, chip),
        (140.0, 24.0, 60.0, 50.0),
        "after chip",
    );

    // Invert + Play: a 1-second linear tween on the two moved channels,
    // so t = 0.5 is the exact midpoint.
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: prop_key(chip, Channel::X),
                spec: TransitionSpec::Tween {
                    duration: 1.0,
                    easing: Easing::Linear,
                },
            },
            PropTransition {
                prop: prop_key(chip, Channel::W),
                spec: TransitionSpec::Tween {
                    duration: 1.0,
                    easing: Easing::Linear,
                },
            },
        ],
        stagger: 0.0,
    };

    let before_chip = [(chip, rect_of(&before, chip))];
    let after_chip = [(chip, rect_of(&after, chip))];
    let mut flip = VariantFlip::new();
    flip.start(&before_chip, &after_chip, &transition);

    // t = 0: the before layout. Y and H hold their (unchanged) values.
    assert_rect(
        flip.sample(chip).expect("chip is animating"),
        (20.0, 24.0, 40.0, 50.0),
        "sample t = 0",
    );
    render_sample(&mut arena, &after, &flip, "v04-flip-t000");

    // t = 0.5: the exact midpoint of both moved channels.
    flip.advance(0.5);
    assert_rect(
        flip.sample(chip).expect("chip is animating"),
        (80.0, 24.0, 50.0, 50.0),
        "sample t = 0.5",
    );
    render_sample(&mut arena, &after, &flip, "v04-flip-t050");

    // t = 1: the tween reaches its target on the finishing frame.
    flip.advance(0.5);
    assert_rect(
        flip.sample(chip).expect("chip is animating"),
        (140.0, 24.0, 60.0, 50.0),
        "sample t = 1",
    );
    render_sample(&mut arena, &after, &flip, "v04-flip-t100");
}
