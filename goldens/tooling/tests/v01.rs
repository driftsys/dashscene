//! The v0.1 exit gate (issue #6; docs/specification/05-qualification.md): a scene authored in
//! the Rust DSL, committed through dashscene-core, painted by the Skia
//! reference painter, compared against the checked-in golden.
//!
//! Regeneration and diff workflow: goldens/README.md.

use dashlang::{anon, node, rgba, scene};
use dashpaint::{ImageTable, Painter};
use dashscene_core::Arena;
use dashscene_skia::SkiaPainter;

/// One 64×64 scene exercising the whole v0.1 vocabulary: a paint-less
/// container, a background fill, overlapping squares (stacking order),
/// a nested child with an authored offset, and two nodes sharing one
/// fill color (paint-table dedup).
fn walking_skeleton(arena: &mut Arena) {
    let navy = rgba(0.05, 0.1, 0.2, 1.0);
    let red = rgba(0.8, 0.1, 0.1, 1.0);
    let green = rgba(0.1, 0.7, 0.2, 1.0);
    let gold = rgba(0.9, 0.7, 0.1, 1.0);

    scene([anon() // layout-only root: draws nothing
        .size(64.0, 64.0)
        .child(node("bg").size(64.0, 64.0).fill(navy))
        .child(
            node("left")
                .at(8.0, 8.0)
                .size(32.0, 32.0)
                .fill(red)
                // Nested: absolute position sums the ancestor offsets.
                .child(node("badge").at(4.0, 4.0).size(8.0, 8.0).fill(gold)),
        )
        // Overlaps "left"; painted later, so it stacks on top.
        .child(node("right").at(24.0, 24.0).size(32.0, 32.0).fill(green))
        // Shares the gold fill with "badge": one deduplicated entry.
        .child(node("dot").at(52.0, 4.0).size(8.0, 8.0).fill(gold))])
    .build(arena);
}

#[test]
fn the_walking_skeleton_scene_matches_its_golden() {
    let mut arena = Arena::new();
    walking_skeleton(&mut arena);
    let scene = arena.committed();

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        None,
    );

    // Key properties pinned independently of the image file: the
    // stacking order in the overlap region and the dedup'd gold fill.
    // Expected bytes are derived from the fixture colors with the same
    // 8-bit quantization the painter applies, so an intended fixture
    // change updates them together with the UPDATE_GOLDENS run.
    let rgba_bytes = painter.rgba_bytes();
    let q = |c: f32| -> u8 { (c * 255.0).round() as u8 };
    let quantized = |r: f32, g: f32, b: f32| -> [u8; 4] { [q(r), q(g), q(b), 255] };
    let green_rgba = quantized(0.1, 0.7, 0.2);
    let gold_rgba = quantized(0.9, 0.7, 0.1);
    let pixel = |x, y| goldens::pixel(&rgba_bytes, 64, x, y);
    assert_eq!(pixel(32, 32), green_rgba, "overlap: later sibling wins");
    assert_eq!(pixel(13, 13), gold_rgba, "nested badge at 8+4, 8+4");
    assert_eq!(pixel(53, 5), gold_rgba, "dedup'd gold dot");

    goldens::assert_matches_golden("v01-walking-skeleton", &painter.png_bytes());
}
