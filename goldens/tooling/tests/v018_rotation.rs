//! The v0.18 rotation golden (story #770): a node turns, through
//! `RectEntry::rotation` and `RectEntry::rotation_anchor`, hand-built at
//! boundary B.
//!
//! The fixture is shaped against the two traps story #770's acceptance
//! criteria name.
//!
//! **A rotation of zero must not be what makes the test pass.** So the bars
//! are rotated, and `rotating_is_what_the_golden_shows` re-renders the same
//! scene with every angle zeroed and asserts the pixels move. That check is
//! independent of the blessed PNG: it fails on a painter that ignores the
//! rotation term even if the golden were re-blessed against that painter.
//!
//! **A rotationally symmetric fixture changes no pixel.** So the bars are
//! 40 x 12 rather than square, and the third one shares the second one's
//! angle while differing only in its anchor — which is what makes
//! `the_anchor_is_where_the_node_turns` able to fail.

use dashpaint::{
    ClipIndex, ClipTable, Color, GlyphRunTable, ImageTable, PaintIndex, PaintTable, Painter,
    RectEntry, Vec2,
};
use dashscene_skia::SkiaPainter;

mod common;
use common::{decode_rgba, diff_vs};

/// 30 degrees. The angle is deliberately not a multiple of 90: a quarter turn
/// on a bar this size would land back on axis-aligned pixel boundaries, where
/// a sign error in the rotation term is much harder to see.
const ANGLE: f32 = std::f32::consts::FRAC_PI_6;

const W: i32 = 96;
const H: i32 = 96;

/// The bar's own box, repeated at three vertical positions.
const BAR_W: f32 = 40.0;
const BAR_H: f32 = 12.0;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn bar(x: f32, y: f32, paint: PaintIndex, rotation: f32, anchor: (f32, f32)) -> RectEntry {
    RectEntry {
        x,
        y,
        w: BAR_W,
        h: BAR_H,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation,
        rotation_anchor: Vec2 {
            x: anchor.0,
            y: anchor.1,
        },
    }
}

/// The scene: an unrotated control bar, one turned about its top-left, and
/// one turned by the same angle about its own centre.
fn scene() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let background = paints.push_solid(rgba(0.06, 0.07, 0.1));
    let control = paints.push_solid(rgba(0.35, 0.38, 0.45));
    let about_origin = paints.push_solid(rgba(0.85, 0.25, 0.2));
    let about_centre = paints.push_solid(rgba(0.15, 0.6, 0.85));

    let rects = vec![
        RectEntry {
            w: W as f32,
            h: H as f32,
            ..bar(0.0, 0.0, background, 0.0, (0.0, 0.0))
        },
        // Unrotated, so the golden shows what the other two turned away from.
        bar(8.0, 10.0, control, 0.0, (0.0, 0.0)),
        // The canonical anchor: the node's own top-left.
        bar(8.0, 46.0, about_origin, ANGLE, (0.0, 0.0)),
        // The same angle about the bar's centre. Same box, same angle, and it
        // lands somewhere else entirely — which is the whole reason the anchor
        // is carried rather than assumed.
        bar(28.0, 74.0, about_centre, ANGLE, (BAR_W / 2.0, BAR_H / 2.0)),
    ];
    (rects, paints)
}

fn render(rects: &[RectEntry], paints: &PaintTable) -> Vec<u8> {
    let mut painter = SkiaPainter::new(W, H);
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.png_bytes()
}

#[test]
fn the_v018_rotation_vocabulary_matches_its_golden() {
    let (rects, paints) = scene();
    // Rotated edges are anti-aliased and not bit-identical across CPU
    // architectures, the same reason the v0.3 paint golden carries a fraction
    // (`docs/decisions/golden-comparison-space.md`).
    goldens::assert_matches_golden_within("v018-rotation", &render(&rects, &paints), 0.01);
}

#[test]
fn rotating_is_what_the_golden_shows() {
    // The acceptance criterion, as a test rather than as a promise: zero the
    // rotation term and the picture must change. A painter that dropped
    // `RectEntry::rotation` would render these two identically and fail here,
    // without any golden image being consulted.
    let (rects, paints) = scene();
    let rotated = decode_rgba(&render(&rects, &paints));

    let upright: Vec<RectEntry> = rects
        .iter()
        .map(|r| RectEntry {
            rotation: 0.0,
            ..*r
        })
        .collect();
    let upright = decode_rgba(&render(&upright, &paints));

    let moved = diff_vs(&rotated, &upright);
    // Two 40 x 12 bars turned by 30 degrees each vacate and cover far more
    // than this; the floor is loose on purpose, so the test fails on "the
    // rotation did nothing" rather than on an anti-aliasing difference.
    assert!(
        moved > 500,
        "zeroing every rotation changed only {moved} pixels: the painter is \
         not drawing the rotation term",
    );
}

#[test]
fn the_anchor_is_where_the_node_turns() {
    // The third bar turns about its centre. Zero its anchor and it turns about
    // its top-left instead — same box, same angle, different pixels. A painter
    // that rotated about a hard-coded point, or ignored the anchor, fails here.
    let (rects, paints) = scene();
    let anchored = decode_rgba(&render(&rects, &paints));

    let at_origin: Vec<RectEntry> = rects
        .iter()
        .map(|r| RectEntry {
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
            ..*r
        })
        .collect();
    let at_origin = decode_rgba(&render(&at_origin, &paints));

    let moved = diff_vs(&anchored, &at_origin);
    assert!(
        moved > 200,
        "moving the anchor to the node's top-left changed only {moved} \
         pixels: the painter is not turning about `rotation_anchor`",
    );
}

#[test]
fn the_reference_painter_declares_that_it_rotates() {
    // The capability is the contract a lagging painter is measured against
    // (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
    // The reference painter draws rotation, so it says so.
    assert!(
        SkiaPainter::new(1, 1).rotates(),
        "the Skia painter draws the rotation term, so it must declare it",
    );
}
