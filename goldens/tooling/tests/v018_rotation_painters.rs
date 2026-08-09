//! Story #832: the lean painter's rotation against the reference painter's.
//!
//! # What this establishes, and what it does not
//!
//! `docs/design/dashscene-gpu.md` puts a per-pixel comparison against the Skia
//! oracle at **layer 4** — a measurement on real hardware, not a gate — because
//! a software rasteriser's own antialiasing is not the one a driver runs. So
//! this does not assert byte equality. It asserts the thing a wrong rotation
//! breaks and a different AA resolve does not: **the two painters put the bar
//! in the same place**, measured as the overlap of their covered areas.
//!
//! A sign error, a pivot in the wrong space, or a missing rotation each move
//! the silhouette wholesale — tens of percent of the covered area — while the
//! two painters' edge treatments differ by a pixel-wide rim. One band separates
//! those two magnitudes comfortably, which is what makes this gateable where a
//! byte comparison would not be.
//!
//! Layer 1 already pins the packed rows bit-exact
//! (`crates/dashscene-gpu/tests/layer1_instances.rs`) and layer 3 pins that the
//! vertex stage turns the quad at all. Neither can see that it turns the *same
//! way* the reference painter does — that is this file.

use dashpaint::{
    ClipIndex, ClipTable, Color, GlyphRunTable, ImageTable, PaintTable, Painter, RectEntry, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};
use dashscene_skia::SkiaPainter;

const W: u32 = 96;
const H: u32 = 96;

/// A 40 x 10 bar, centred, turning about its own centre. Not square: a
/// rotationally symmetric fixture changes no pixel and would agree with a
/// painter that dropped the term.
fn scene(rotation: f32) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let solid = paints.push_solid(Color {
        r: 0.9,
        g: 0.2,
        b: 0.1,
        a: 1.0,
    });
    let bar = RectEntry {
        x: 28.0,
        y: 43.0,
        w: 40.0,
        h: 10.0,
        paint: solid,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation,
        rotation_anchor: Vec2 { x: 20.0, y: 5.0 },
    };
    (vec![bar], paints)
}

fn skia_pixels(rects: &[RectEntry], paints: &PaintTable) -> Vec<u8> {
    let mut painter = SkiaPainter::new(W as i32, H as i32);
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

fn gpu_pixels(rects: &[RectEntry], paints: &PaintTable) -> Vec<u8> {
    let mut painter = GpuPainter::new();
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    Renderer::new()
        .expect("this comparison needs a device")
        .render(
            painter.instances(),
            paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum")
}

/// Whether each pixel is substantially covered, which is what the two painters
/// must agree on. The threshold is deliberately far from both ends so a
/// one-pixel difference in how an edge ramps moves nothing.
fn covered(pixels: &[u8]) -> Vec<bool> {
    pixels.chunks_exact(4).map(|p| p[3] > 128).collect()
}

/// How much of the two covered areas coincide, as a fraction of their union.
fn agreement(a: &[bool], b: &[bool]) -> f64 {
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    assert!(either > 0, "neither painter drew anything");
    both as f64 / either as f64
}

#[test]
fn both_painters_turn_the_bar_the_same_way() {
    let (rects, paints) = scene(std::f32::consts::FRAC_PI_4);
    let skia = covered(&skia_pixels(&rects, &paints));
    let gpu = covered(&gpu_pixels(&rects, &paints));

    let overlap = agreement(&skia, &gpu);
    assert!(
        overlap > 0.97,
        "the two painters' rotated bars overlap on only {:.1} % of their \
         combined area: they are not turning the same way",
        overlap * 100.0,
    );
}

/// The sign, isolated. Turning the other way must *disagree*, so the assertion
/// above cannot be passing because both painters ignore the term.
#[test]
fn the_two_painters_disagree_when_one_turns_the_other_way() {
    let (rects, paints) = scene(std::f32::consts::FRAC_PI_4);
    let (mirrored, _) = scene(-std::f32::consts::FRAC_PI_4);

    let skia = covered(&skia_pixels(&rects, &paints));
    let gpu_mirrored = covered(&gpu_pixels(&mirrored, &paints));

    let overlap = agreement(&skia, &gpu_mirrored);
    assert!(
        overlap < 0.60,
        "a bar turned +45 degrees and one turned -45 degrees overlap on \
         {:.1} % of their combined area, so this fixture cannot tell the two \
         signs apart and the agreement test above proves nothing",
        overlap * 100.0,
    );
}

/// An unrotated node is unaffected: whatever the two painters agreed on before
/// story #832, they still agree on.
#[test]
fn an_unrotated_bar_still_agrees() {
    let (rects, paints) = scene(0.0);
    let skia = covered(&skia_pixels(&rects, &paints));
    let gpu = covered(&gpu_pixels(&rects, &paints));
    let overlap = agreement(&skia, &gpu);
    assert!(
        overlap > 0.97,
        "an unrotated bar now differs between the painters ({:.1} % overlap): \
         the rotation path changed what an unrotated instance draws",
        overlap * 100.0,
    );
}
