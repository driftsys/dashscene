//! The v0.1 exit gate (issue #6; DESIGN_1.md §11): a scene authored in
//! the Rust DSL, committed through dashscene-core, painted by the Skia
//! reference painter, compared against the checked-in golden.
//!
//! Regeneration and diff workflow: goldens/README.md.

use dashlang::{anon, node, rgba, scene};
use dashpaint::Painter;
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
    painter.paint(scene.rects(), scene.paints());

    // Key properties pinned independently of the image file: the
    // stacking order in the overlap region and the dedup'd gold fill.
    let rgba_bytes = painter.rgba_bytes();
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        rgba_bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
            .try_into()
            .unwrap()
    };
    let green_rgba = [26, 179, 51, 255];
    let gold_rgba = [230, 179, 26, 255];
    assert_eq!(pixel(32, 32), green_rgba, "overlap: later sibling wins");
    assert_eq!(pixel(13, 13), gold_rgba, "nested badge at 8+4, 8+4");
    assert_eq!(pixel(53, 5), gold_rgba, "dedup'd gold dot");

    goldens::assert_matches_golden("v01-walking-skeleton", &painter.png_bytes());
}
