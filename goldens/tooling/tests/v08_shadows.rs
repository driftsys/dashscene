//! The v0.8 drop- and inner-shadow goldens (story #45). Each scene is
//! authored through `dashscene-core`'s producer API (`Prop::Shadows`),
//! committed, and rendered by the reference painter — the whole path from
//! shadow intent to pixels.
//!
//! `docs/decisions/effects-vocabulary-shadows.md`. A blurred shadow is
//! anti-aliased, so the golden compares with the same 2% differing-pixel
//! tolerance as the other 64×64 goldens
//! (`docs/decisions/golden-comparison-space.md`). That coarse tolerance
//! cannot, on its own, prove the golden pins the shadow — a 2% budget on a
//! 64×64 canvas is ~82 pixels. So each test also renders a broken variant
//! (the same scene with the shadow removed) and asserts it differs from the
//! good render by far more than that budget: the shadow inks hundreds of
//! pixels, so a regression that drops it cannot pass. This is the
//! demonstrated-sensitivity discipline the goldens lesson requires; the
//! probes below add exact, machine-independent relationships on top.

use dashpaint::{Color, GlyphRunTable, ImageTable, Painter, Shadow, ShadowKind, Vec2};
use dashscene_core::{Arena, NodeId, Prop, Txn};
use dashscene_skia::SkiaPainter;

const SIZE: usize = 64;
const TOLERANCE: f64 = 0.02;

/// The 2% tolerance is ~82 pixels on this canvas. The sensitivity guard
/// requires the shadow to move the render well past that — a floor of
/// 250 px. Measured: dropping the shadow changes 1159 px (drop) / 748 px
/// (inner), so the floor sits ≈3× above the tolerance budget and well below
/// the shadow's actual ink. A regression that drops or badly misplaces the
/// shadow therefore fails the golden rather than hiding under the tolerance.
const SENSITIVITY_FLOOR: usize = 250;

const NAVY: Color = Color {
    r: 0.06,
    g: 0.08,
    b: 0.16,
    a: 1.0,
};
const AMBER: Color = Color {
    r: 0.98,
    g: 0.78,
    b: 0.20,
    a: 1.0,
};
const NEAR_WHITE: Color = Color {
    r: 0.92,
    g: 0.94,
    b: 0.98,
    a: 1.0,
};
const SHADOW_INK: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.55,
};
const SHADOW_RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 0.6,
};
const SHADOW_BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 0.6,
};

fn boxed(txn: &mut Txn<'_>, parent: Option<NodeId>, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    let node = txn.add_node(parent, None);
    txn.set_prop(node, Prop::X(x));
    txn.set_prop(node, Prop::Y(y));
    txn.set_prop(node, Prop::Width(w));
    txn.set_prop(node, Prop::Height(h));
    node
}

fn rounded(txn: &mut Txn<'_>, node: NodeId, r: f32) {
    txn.set_prop(
        node,
        Prop::Corners {
            top_left: r,
            top_right: r,
            bottom_right: r,
            bottom_left: r,
        },
    );
}

fn render(arena: &Arena, painter: &mut SkiaPainter) -> Vec<u8> {
    let scene = arena.committed();
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
    painter.rgba_bytes()
}

fn quantized(c: Color) -> [u8; 4] {
    let q = |v: f32| (v * 255.0).round() as u8;
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

/// The count of differing pixels between two RGBA8888 buffers — the
/// sensitivity guard's measure. Inlined so this golden stays self-contained
/// (story #46 owns the shared tooling helpers).
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

/// A rounded amber card on a navy field, casting a drop shadow down and
/// behind it.
///
///   bg (navy 64×64)
///     └── card amber (16,16) 32×32 rounded r=6, drop shadow offset (0,4)
///         blur 6, black α=0.55
fn drop_scene(arena: &mut Arena, with_shadow: bool) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let card = boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
    rounded(&mut txn, card, 6.0);
    txn.set_prop(card, Prop::Fill(AMBER));
    if with_shadow {
        txn.set_prop(
            card,
            Prop::Shadows(vec![Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: 0.0, y: 4.0 },
                blur: 6.0,
                spread: 0.0,
                color: SHADOW_INK,
            }]),
        );
    }
    txn.commit();
}

#[test]
fn the_drop_shadow_scene_matches_its_golden() {
    let mut arena = Arena::new();
    drop_scene(&mut arena, true);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // The opaque card fill is unchanged (the drop shadow is behind it).
    assert_eq!(
        probe(32, 32),
        quantized(AMBER),
        "the card fill is unchanged"
    );
    // A far corner, clear of the shadow, keeps the background.
    assert_eq!(probe(2, 2), quantized(NAVY), "the background is untouched");
    // Just below the card, in the offset direction, the shadow darkens the
    // navy background — the shadow ink is darker than the navy on every
    // channel.
    let shadow_px = probe(32, 50);
    let navy = quantized(NAVY);
    assert!(
        shadow_px[0] < navy[0] && shadow_px[2] < navy[2],
        "the drop shadow darkens the background below the card: {shadow_px:?} vs navy {navy:?}"
    );

    // Sensitivity: the same scene with no shadow differs from the good
    // render by far more than the 2% tolerance budget (~82 px), so a
    // regression that drops the shadow cannot pass the golden.
    let mut broken_arena = Arena::new();
    drop_scene(&mut broken_arena, false);
    let mut broken_painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let broken = render(&broken_arena, &mut broken_painter);
    let diff = differing(&bytes, &broken);
    assert!(
        diff > SENSITIVITY_FLOOR,
        "the drop-shadow golden must pin the shadow: a no-shadow render differs by only {diff} px \
         (floor {SENSITIVITY_FLOOR})"
    );

    goldens::assert_matches_golden_within("v08-drop-shadow", &painter.png_bytes(), TOLERANCE);
}

/// A rounded near-white panel on a navy field, with an inner shadow ringing
/// its inside edges.
///
///   bg (navy 64×64)
///     └── panel near-white (16,16) 32×32 rounded r=6, inner shadow
///         offset (0,0) blur 6, black α=0.55
fn inner_scene(arena: &mut Arena, with_shadow: bool) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let panel = boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
    rounded(&mut txn, panel, 6.0);
    txn.set_prop(panel, Prop::Fill(NEAR_WHITE));
    if with_shadow {
        txn.set_prop(
            panel,
            Prop::Shadows(vec![Shadow {
                kind: ShadowKind::Inner,
                offset: Vec2 { x: 0.0, y: 0.0 },
                blur: 6.0,
                spread: 0.0,
                color: SHADOW_INK,
            }]),
        );
    }
    txn.commit();
}

#[test]
fn the_inner_shadow_scene_matches_its_golden() {
    let mut arena = Arena::new();
    inner_scene(&mut arena, true);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // The background outside the panel is untouched — an inner shadow does
    // not leak past the shape.
    assert_eq!(probe(2, 2), quantized(NAVY), "the background is untouched");
    // The inner shadow (black) rings the inside edge and fades toward the
    // center, so the center stays near the white fill while a pixel near the
    // top edge is darker.
    let center = probe(32, 32);
    let edge = probe(32, 19);
    assert!(
        center[0] > edge[0],
        "the inner shadow darkens the edge ({}) more than the center ({})",
        edge[0],
        center[0]
    );
    assert!(
        center[0] > 200,
        "the panel center stays near white: {center:?}"
    );

    // Sensitivity: the panel with no inner shadow differs from the good
    // render by far more than the tolerance budget.
    let mut broken_arena = Arena::new();
    inner_scene(&mut broken_arena, false);
    let mut broken_painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let broken = render(&broken_arena, &mut broken_painter);
    let diff = differing(&bytes, &broken);
    assert!(
        diff > SENSITIVITY_FLOOR,
        "the inner-shadow golden must pin the shadow: a no-shadow render differs by only {diff} px \
         (floor {SENSITIVITY_FLOOR})"
    );

    goldens::assert_matches_golden_within("v08-inner-shadow", &painter.png_bytes(), TOLERANCE);
}

/// Two same-kind drop shadows on one card, pinning the stacking order. The
/// list is Figma's `effects` array order, which is back-to-front, so
/// `shadows[0]` (blue) is the backmost and `shadows[1]` (red) renders on top
/// — the painter draws the list forward, so the later element composites
/// over the earlier one. Both use blur 0, so the overlap is a flat region
/// whose color depends entirely on the order: red-over-blue if correct,
/// blue-over-red if the draw loop were reversed.
///
///   bg (navy 64×64)
///     └── card amber (24,24) 16×16
///           shadows[0] blue  α=0.6 offset (0,12)  — backmost
///           shadows[1] red   α=0.6 offset (0,6)   — on top
fn stacked_scene(arena: &mut Arena) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let card = boxed(&mut txn, Some(bg), 24.0, 24.0, 16.0, 16.0);
    txn.set_prop(card, Prop::Fill(AMBER));
    txn.set_prop(
        card,
        Prop::Shadows(vec![
            Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: 0.0, y: 12.0 },
                blur: 0.0,
                spread: 0.0,
                color: SHADOW_BLUE,
            },
            Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: 0.0, y: 6.0 },
                blur: 0.0,
                spread: 0.0,
                color: SHADOW_RED,
            },
        ]),
    );
    txn.commit();
}

#[test]
fn the_stacked_shadows_scene_matches_its_golden_and_pins_the_order() {
    let mut arena = Arena::new();
    stacked_scene(&mut arena);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // Below the card, both shadows cover x in [24,40): the red box is
    // y [30,46), the blue box y [36,52). At y=42 both overlap, and red is on
    // top, so red dominates blue. If the draw loop were reversed, blue would
    // be on top here and this assertion would fail.
    let overlap = probe(30, 42);
    assert!(
        overlap[0] > overlap[2],
        "shadows[1] (red) composites over shadows[0] (blue): overlap {overlap:?} must be \
         red-dominant, which reverses if the stacking order is wrong"
    );
    // Further down, only the blue box reaches (y [46,52)), so blue shows —
    // confirming the backmost shadow is present, not swallowed.
    let blue_only = probe(30, 48);
    assert!(
        blue_only[2] > blue_only[0],
        "the backmost blue shadow still paints where the red box does not reach: {blue_only:?}"
    );
    // The card fill and the untouched background.
    assert_eq!(
        probe(32, 32),
        quantized(AMBER),
        "the card fill is unchanged"
    );
    assert_eq!(probe(2, 2), quantized(NAVY), "the background is untouched");

    goldens::assert_matches_golden_within("v08-stacked-shadows", &painter.png_bytes(), TOLERANCE);
}
