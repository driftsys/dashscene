//! The subtree-clip golden (issue #97): unlike the other v0.3 paint
//! goldens, this scene is authored through `dashscene-core`'s producer
//! API — clipping is the one construct a painter cannot be handed
//! directly, because the ancestor relation only exists on the producer
//! side. The scene therefore exercises the whole path: `Prop::Clip` /
//! `Prop::Corners` intent → commit-time clip resolution → the reference
//! painter intersecting the resolved regions.
//!
//! Rounded clips are anti-aliased, so the image compares with the same
//! 2% differing-pixel tolerance as the other 64×64 family goldens
//! (`docs/decisions/golden-comparison-space.md`); the probes below sit
//! in flat interiors and are bit-stable.

use dashpaint::{Color, ImageTable, Painter};
use dashscene_core::{Arena, NodeId, Prop, Txn};
use dashscene_skia::SkiaPainter;

const SIZE: usize = 64;
const TOLERANCE: f64 = 0.02;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn quantized(c: Color) -> [u8; 4] {
    let q = |v: f32| (v * 255.0).round() as u8;
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

const NAVY: Color = Color {
    r: 0.06,
    g: 0.08,
    b: 0.16,
    a: 1.0,
};
const GRAY: Color = Color {
    r: 0.75,
    g: 0.75,
    b: 0.78,
    a: 1.0,
};

fn boxed(txn: &mut Txn<'_>, parent: Option<NodeId>, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    let node = txn.add_node(parent, None);
    txn.set_prop(node, Prop::X(x));
    txn.set_prop(node, Prop::Y(y));
    txn.set_prop(node, Prop::Width(w));
    txn.set_prop(node, Prop::Height(h));
    node
}

fn round(txn: &mut Txn<'_>, node: NodeId, radius: f32) {
    txn.set_prop(
        node,
        Prop::Corners {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        },
    );
}

/// Four panels on a 64×64 navy background. Every child is deliberately
/// oversized, so what shows is exactly what its clipping ancestors let
/// through.
///
///   A (4,4)  24×24  rounded r=8, clipping, gray fill
///            └── red 40×40 at (10,10) — overflows every side
///   B (36,4) 24×24  sharp, clipping, *unfilled* (layout-only)
///            └── green 40×40 at (28,12) — overflows left/right/bottom
///   C (4,36) 24×24  sharp, clipping, gray fill
///            └── (12,44) 24×24 rounded r=6, clipping, unfilled
///                └── yellow 40×40 at (12,44) — clipped by both boxes
///   D (36,36) 24×24 teal, no clip anywhere above it — paints in full
fn clip_scene(arena: &mut Arena) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    // A — a rounded clipping frame.
    let frame_a = boxed(&mut txn, Some(bg), 4.0, 4.0, 24.0, 24.0);
    round(&mut txn, frame_a, 8.0);
    txn.set_prop(frame_a, Prop::Clip(true));
    txn.set_prop(frame_a, Prop::Fill(GRAY));
    let child_a = boxed(&mut txn, Some(frame_a), 6.0, 6.0, 40.0, 40.0);
    txn.set_prop(child_a, Prop::Fill(rgba(0.85, 0.2, 0.2)));

    // B — a sharp clipping frame that draws nothing itself.
    let frame_b = boxed(&mut txn, Some(bg), 36.0, 4.0, 24.0, 24.0);
    txn.set_prop(frame_b, Prop::Clip(true));
    let child_b = boxed(&mut txn, Some(frame_b), -8.0, 8.0, 40.0, 40.0);
    txn.set_prop(child_b, Prop::Fill(rgba(0.2, 0.7, 0.35)));

    // C — a nested chain: sharp outer ∩ rounded inner.
    let frame_c = boxed(&mut txn, Some(bg), 4.0, 36.0, 24.0, 24.0);
    txn.set_prop(frame_c, Prop::Clip(true));
    txn.set_prop(frame_c, Prop::Fill(GRAY));
    let inner_c = boxed(&mut txn, Some(frame_c), 8.0, 8.0, 24.0, 24.0);
    round(&mut txn, inner_c, 6.0);
    txn.set_prop(inner_c, Prop::Clip(true));
    let child_c = boxed(&mut txn, Some(inner_c), 0.0, 0.0, 40.0, 40.0);
    txn.set_prop(child_c, Prop::Fill(rgba(0.95, 0.8, 0.25)));

    // D — untouched by any clip.
    let panel_d = boxed(&mut txn, Some(bg), 36.0, 36.0, 24.0, 24.0);
    txn.set_prop(panel_d, Prop::Fill(rgba(0.2, 0.55, 0.75)));

    txn.commit();
}

#[test]
fn the_clip_scene_matches_its_golden() {
    let mut arena = Arena::new();
    clip_scene(&mut arena);
    let scene = arena.committed();

    // The regions the commit resolved: unclipped, A's, B's, C's outer,
    // C's outer ∩ inner. Five, shared across the nine rects.
    assert_eq!(scene.clips().len(), 5);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        Some(scene.dirty()),
    );
    let bytes = painter.rgba_bytes();
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // A: the frame shows above its child; the child shows inside the
    // frame and nowhere outside it.
    assert_eq!(probe(14, 8), quantized(GRAY), "A: frame above its child");
    assert_eq!(
        probe(20, 20),
        quantized(rgba(0.85, 0.2, 0.2)),
        "A: the child inside the rounded frame"
    );
    assert_eq!(
        probe(31, 20),
        quantized(NAVY),
        "A: the child's overflow is clipped away"
    );

    // B: an unfilled clipping frame still clips.
    assert_eq!(
        probe(40, 20),
        quantized(rgba(0.2, 0.7, 0.35)),
        "B: the child inside the unfilled frame"
    );
    assert_eq!(
        probe(32, 20),
        quantized(NAVY),
        "B: overflow left of the frame is clipped away"
    );
    assert_eq!(
        probe(40, 8),
        quantized(NAVY),
        "B: the clipping frame itself draws nothing"
    );

    // C: the chain intersects — the outer box and the rounded inner box
    // both bite.
    assert_eq!(
        probe(20, 50),
        quantized(rgba(0.95, 0.8, 0.25)),
        "C: inside both clip boxes"
    );
    assert_eq!(
        probe(8, 50),
        quantized(GRAY),
        "C: left of the inner box — the outer frame shows through"
    );
    assert_eq!(
        probe(20, 62),
        quantized(NAVY),
        "C: below the outer box — clipped by the outermost ancestor"
    );

    // D: no ancestor clips, so nothing is taken away.
    assert_eq!(
        probe(48, 48),
        quantized(rgba(0.2, 0.55, 0.75)),
        "D: unclipped"
    );

    goldens::assert_matches_golden_within("v03-clips", &painter.png_bytes(), TOLERANCE);
}
