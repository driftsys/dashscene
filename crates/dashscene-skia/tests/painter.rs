//! Story #4 acceptance path: a scene committed by dashscene-core paints
//! through the Skia CPU raster painter with exact, deterministic pixels
//! (issue #4; docs/design/architecture.md) — the first end-to-end crossing of
//! boundary B.

use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, ClipIndex, ClipTable, Color, GlyphQuad, GlyphRun, GlyphRunTable,
    Gradient, GradientKind, ImageTable, MAX_GRADIENT_STOPS, PaintEntry, PaintKind, Painter,
    RectEntry, Vec2,
};
use dashscene_core::{Arena, Prop};
use dashscene_skia::{DirtyMode, SkiaPainter};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

const RED_RGBA: [u8; 4] = [255, 0, 0, 255];
const BLUE_RGBA: [u8; 4] = [0, 0, 255, 255];
const TRANSPARENT_RGBA: [u8; 4] = [0, 0, 0, 0];

fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

#[test]
fn paints_a_core_committed_scene_with_exact_pixels() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::Width(4.0));
    txn.set_prop(root, Prop::Height(4.0));
    txn.set_prop(root, Prop::Fill(RED));
    let child = txn.add_node(Some(root), Some("badge"));
    txn.set_prop(child, Prop::X(1.0));
    txn.set_prop(child, Prop::Y(1.0));
    txn.set_prop(child, Prop::Width(2.0));
    txn.set_prop(child, Prop::Height(2.0));
    txn.set_prop(child, Prop::Fill(BLUE));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );

    let rgba = painter.rgba_bytes();
    assert_eq!(rgba.len(), 4 * 4 * 4);
    for y in 0..4 {
        for x in 0..4 {
            let expected = if (1..3).contains(&x) && (1..3).contains(&y) {
                BLUE_RGBA
            } else {
                RED_RGBA
            };
            assert_eq!(pixel(&rgba, 4, x, y), expected, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn an_unfilled_node_draws_nothing() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    // A layout-only container: occupies the full surface, draws nothing.
    let container = txn.add_node(None, Some("container"));
    txn.set_prop(container, Prop::Width(4.0));
    txn.set_prop(container, Prop::Height(4.0));
    let child = txn.add_node(Some(container), Some("dot"));
    txn.set_prop(child, Prop::Width(1.0));
    txn.set_prop(child, Prop::Height(1.0));
    txn.set_prop(child, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );

    let rgba = painter.rgba_bytes();
    assert_eq!(pixel(&rgba, 4, 0, 0), RED_RGBA, "the filled child paints");
    assert_eq!(
        pixel(&rgba, 4, 3, 3),
        TRANSPARENT_RGBA,
        "the unfilled container leaves the surface untouched"
    );
}

#[test]
fn encodes_png() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::Width(2.0));
    txn.set_prop(root, Prop::Height(2.0));
    txn.set_prop(root, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(2, 2);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );

    let png = painter.png_bytes();
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG signature"
    );
    assert!(png.len() > 8);
}

// ---- v0.3 vocabulary (issue #14) -------------------------------------
//
// Hand-built boundary-B input throughout: no producer can stage this
// vocabulary yet, and the painter contract needs no producer. Probe
// pixels sit away from geometry edges, and gradients use coincident
// ("hard") stops, so every expectation is an exact byte value even
// with anti-aliasing on.

use dashpaint::{
    ClipBox, ClipRegion, CornerRadii, GradientStop, ImageAsset, ImageFormat, Mat23, PaintTable,
    ScaleMode, Stroke, StrokeAlign,
};

const GREEN: Color = Color {
    r: 0.0,
    g: 1.0,
    b: 0.0,
    a: 1.0,
};
const WHITE: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

const GREEN_RGBA: [u8; 4] = [0, 255, 0, 255];
const WHITE_RGBA: [u8; 4] = [255, 255, 255, 255];

fn hard_red_blue_stops() -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: RED,
        },
        GradientStop {
            offset: 0.5,
            color: RED,
        },
        GradientStop {
            offset: 0.5,
            color: BLUE,
        },
        GradientStop {
            offset: 1.0,
            color: BLUE,
        },
    ]
}

fn single_entry_scene(entry: PaintEntry, w: f32, h: f32) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let paint = paints.push(entry);
    (
        vec![RectEntry {
            x: 0.0,
            y: 0.0,
            w,
            h,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        }],
        paints,
    )
}

fn gradient_entry(kind: GradientKind, stops: Vec<GradientStop>) -> PaintEntry {
    PaintEntry {
        fill: Some(PaintKind::Gradient(Gradient {
            kind,
            handle_origin: Vec2 { x: 0.5, y: 0.5 },
            handle_primary: Vec2 { x: 1.0, y: 0.5 },
            handle_secondary: Vec2 { x: 0.5, y: 1.0 },
            stops,
        })),
        ..PaintEntry::default()
    }
}

fn render(rects: &[RectEntry], paints: &PaintTable, images: &ImageTable, size: i32) -> Vec<u8> {
    let mut painter = SkiaPainter::new(size, size);
    painter.paint(
        rects,
        paints,
        images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

// Local copy of the 4-line indexing helper (the goldens crate is not a
// dependency of this crate; see goldens::pixel for the harness one).
fn px(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

#[test]
fn linear_gradient_splits_at_a_hard_stop() {
    // Origin at the box center, primary axis +x: t=0 at x=4, t=1 at
    // x=8, clamped left of center. The hard stop at 0.5 lands at x=6.
    let (rects, paints) = single_entry_scene(
        gradient_entry(GradientKind::Linear, hard_red_blue_stops()),
        8.0,
        8.0,
    );
    let rgba = render(&rects, &paints, &ImageTable::new(), 8);

    assert_eq!(px(&rgba, 8, 1, 4), RED_RGBA, "clamped region before t=0");
    assert_eq!(px(&rgba, 8, 4, 4), RED_RGBA, "first half");
    assert_eq!(px(&rgba, 8, 7, 4), BLUE_RGBA, "second half");
}

#[test]
fn radial_gradient_fills_a_disk_inside_the_hard_stop() {
    // Unit radius = half the box; the hard stop at 0.5 is a disk of
    // radius 2px around the center of the 8px box.
    let (rects, paints) = single_entry_scene(
        gradient_entry(GradientKind::Radial, hard_red_blue_stops()),
        8.0,
        8.0,
    );
    let rgba = render(&rects, &paints, &ImageTable::new(), 8);

    assert_eq!(px(&rgba, 8, 4, 4), RED_RGBA, "center is inside the disk");
    assert_eq!(px(&rgba, 8, 1, 1), BLUE_RGBA, "corner is far outside");
    assert_eq!(px(&rgba, 8, 7, 4), BLUE_RGBA, "past the disk on the axis");
}

#[test]
fn angular_gradient_splits_half_turns_at_the_hard_stop() {
    let (rects, paints) = single_entry_scene(
        gradient_entry(GradientKind::Angular, hard_red_blue_stops()),
        8.0,
        8.0,
    );
    let rgba = render(&rects, &paints, &ImageTable::new(), 8);

    // The sweep starts along +x (the primary handle); y grows downward,
    // so the first half turn covers the lower half plane.
    assert_eq!(px(&rgba, 8, 7, 5), RED_RGBA, "just past the start angle");
    assert_eq!(px(&rgba, 8, 7, 3), BLUE_RGBA, "just before the full turn");
    assert_eq!(px(&rgba, 8, 4, 7), RED_RGBA, "quarter turn");
    assert_eq!(px(&rgba, 8, 4, 1), BLUE_RGBA, "three quarter turn");
}

#[test]
fn diamond_gradient_fills_a_diamond_inside_the_hard_stop() {
    let (rects, paints) = single_entry_scene(
        gradient_entry(GradientKind::Diamond, hard_red_blue_stops()),
        8.0,
        8.0,
    );
    let rgba = render(&rects, &paints, &ImageTable::new(), 8);

    assert_eq!(px(&rgba, 8, 4, 4), RED_RGBA, "center");
    assert_eq!(px(&rgba, 8, 1, 1), BLUE_RGBA, "corner: |dx|+|dy| > 1");
    assert_eq!(px(&rgba, 8, 7, 4), BLUE_RGBA, "axis extreme: t = 1");
    // Manhattan distance 1px from the center: t = 0.25.
    assert_eq!(px(&rgba, 8, 4, 3), RED_RGBA, "inside the diamond");
    // Manhattan distance 3px: t = 0.75.
    assert_eq!(px(&rgba, 8, 5, 5), BLUE_RGBA, "outside the diamond");
}

#[test]
fn a_gradient_with_a_degenerate_frame_falls_back_to_the_first_stop() {
    let mut entry = gradient_entry(GradientKind::Linear, hard_red_blue_stops());
    if let Some(PaintKind::Gradient(gradient)) = &mut entry.fill {
        // All three handles coincide: the frame has no area.
        gradient.handle_primary = Vec2 { x: 0.5, y: 0.5 };
        gradient.handle_secondary = Vec2 { x: 0.5, y: 0.5 };
    }
    let (rects, paints) = single_entry_scene(entry, 4.0, 4.0);
    let rgba = render(&rects, &paints, &ImageTable::new(), 4);

    assert_eq!(px(&rgba, 4, 2, 2), RED_RGBA);
}

fn stroked_square(align: StrokeAlign) -> Vec<u8> {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry {
        stroke: Some(Stroke {
            width: 2.0,
            align,
            color: RED,
        }),
        ..PaintEntry::default()
    });
    let rects = [RectEntry {
        x: 4.0,
        y: 4.0,
        w: 8.0,
        h: 8.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    }];
    let mut painter = SkiaPainter::new(16, 16);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

#[test]
fn stroke_align_places_the_band_relative_to_the_outline() {
    // Left outline at x=4, width 2: center covers x in [3,5),
    // inside [4,6), outside [2,4).
    let center = stroked_square(StrokeAlign::Center);
    let inside = stroked_square(StrokeAlign::Inside);
    let outside = stroked_square(StrokeAlign::Outside);

    assert_eq!(px(&center, 16, 3, 8), RED_RGBA);
    assert_eq!(px(&center, 16, 4, 8), RED_RGBA);
    assert_eq!(px(&center, 16, 5, 8), TRANSPARENT_RGBA);
    assert_eq!(px(&center, 16, 2, 8), TRANSPARENT_RGBA);

    assert_eq!(px(&inside, 16, 4, 8), RED_RGBA);
    assert_eq!(px(&inside, 16, 5, 8), RED_RGBA);
    assert_eq!(px(&inside, 16, 3, 8), TRANSPARENT_RGBA);

    assert_eq!(px(&outside, 16, 2, 8), RED_RGBA);
    assert_eq!(px(&outside, 16, 3, 8), RED_RGBA);
    assert_eq!(px(&outside, 16, 4, 8), TRANSPARENT_RGBA);
}

#[test]
fn rounded_corners_shape_the_fill() {
    let mut entry = PaintEntry::solid(RED);
    entry.corners = CornerRadii {
        top_left: 8.0,
        top_right: 8.0,
        bottom_right: 8.0,
        bottom_left: 8.0,
    };
    let (rects, paints) = single_entry_scene(entry, 16.0, 16.0);
    let rgba = render(&rects, &paints, &ImageTable::new(), 16);

    assert_eq!(px(&rgba, 16, 8, 8), RED_RGBA, "center");
    assert_eq!(
        px(&rgba, 16, 0, 0),
        TRANSPARENT_RGBA,
        "square corner is outside the round"
    );
    assert_eq!(px(&rgba, 16, 1, 1), TRANSPARENT_RGBA, "still outside");
}

/// A 2×2 asset: TL red, TR green, BL blue, BR white.
fn quadrant_asset() -> ImageAsset {
    let mut painter = SkiaPainter::new(2, 2);
    let mut paints = PaintTable::new();
    let rects: Vec<RectEntry> = [
        (0.0, 0.0, RED),
        (1.0, 0.0, GREEN),
        (0.0, 1.0, BLUE),
        (1.0, 1.0, WHITE),
    ]
    .into_iter()
    .map(|(x, y, color)| RectEntry {
        x,
        y,
        w: 1.0,
        h: 1.0,
        paint: paints.push(PaintEntry::solid(color)),
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    })
    .collect();
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    ImageAsset {
        format: ImageFormat::Png,
        bytes: painter.png_bytes(),
    }
}

fn image_entry(scale_mode: ScaleMode, transform: Option<Mat23>, tile_scale: f32) -> PaintEntry {
    PaintEntry {
        fill: Some(PaintKind::Image {
            image: 0,
            scale_mode,
            transform,
            tile_scale,
        }),
        ..PaintEntry::default()
    }
}

fn image_scene(entry: PaintEntry, w: f32, h: f32, size: i32) -> Vec<u8> {
    let mut images = ImageTable::new();
    images.push(quadrant_asset());
    let (rects, paints) = single_entry_scene(entry, w, h);
    let mut painter = SkiaPainter::new(size, size);
    painter.paint(
        &rects,
        &paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

#[test]
fn image_fill_covers_the_box_with_quadrants_in_place() {
    let rgba = image_scene(image_entry(ScaleMode::Fill, None, 1.0), 8.0, 8.0, 8);

    assert_eq!(px(&rgba, 8, 2, 2), RED_RGBA);
    assert_eq!(px(&rgba, 8, 6, 2), GREEN_RGBA);
    assert_eq!(px(&rgba, 8, 2, 6), BLUE_RGBA);
    assert_eq!(px(&rgba, 8, 6, 6), WHITE_RGBA);
}

#[test]
fn image_fit_letterboxes_a_wide_box() {
    // 8×4 box, square image: contain gives a 4×4 image centered at
    // x in [2, 6); the letterbox stays transparent.
    let rgba = image_scene(image_entry(ScaleMode::Fit, None, 1.0), 8.0, 4.0, 8);

    assert_eq!(px(&rgba, 8, 0, 1), TRANSPARENT_RGBA, "letterbox left");
    assert_eq!(px(&rgba, 8, 7, 1), TRANSPARENT_RGBA, "letterbox right");
    assert_eq!(px(&rgba, 8, 3, 1), RED_RGBA, "image top-left quadrant");
    assert_eq!(
        px(&rgba, 8, 5, 2),
        WHITE_RGBA,
        "image bottom-right quadrant"
    );
}

#[test]
fn image_tile_repeats_at_native_scale() {
    let rgba = image_scene(image_entry(ScaleMode::Tile, None, 1.0), 8.0, 8.0, 8);

    // The 2×2 pattern repeats every 2 pixels.
    assert_eq!(px(&rgba, 8, 0, 0), RED_RGBA);
    assert_eq!(px(&rgba, 8, 1, 0), GREEN_RGBA);
    assert_eq!(px(&rgba, 8, 0, 1), BLUE_RGBA);
    assert_eq!(px(&rgba, 8, 1, 1), WHITE_RGBA);
    assert_eq!(px(&rgba, 8, 2, 0), RED_RGBA, "next tile starts over");
    assert_eq!(px(&rgba, 8, 5, 5), WHITE_RGBA);
}

#[test]
fn image_tile_scale_magnifies_the_pattern() {
    let rgba = image_scene(image_entry(ScaleMode::Tile, None, 2.0), 8.0, 8.0, 8);

    // Each texel now covers 2×2 pixels: one 4×4 tile.
    assert_eq!(px(&rgba, 8, 1, 1), RED_RGBA);
    assert_eq!(px(&rgba, 8, 3, 1), GREEN_RGBA);
    assert_eq!(px(&rgba, 8, 1, 3), BLUE_RGBA);
    assert_eq!(px(&rgba, 8, 3, 3), WHITE_RGBA);
    assert_eq!(px(&rgba, 8, 5, 1), RED_RGBA, "second tile");
}

#[test]
fn image_crop_shows_the_transformed_region() {
    // uv_image = T · uv_box with T = scale(0.5): the box shows only the
    // image's top-left quarter — the red texel.
    let transform = Mat23 {
        a: 0.5,
        b: 0.0,
        c: 0.0,
        d: 0.5,
        tx: 0.0,
        ty: 0.0,
    };
    let rgba = image_scene(
        image_entry(ScaleMode::Crop, Some(transform), 1.0),
        8.0,
        8.0,
        8,
    );

    assert_eq!(px(&rgba, 8, 1, 1), RED_RGBA);
    assert_eq!(px(&rgba, 8, 6, 6), RED_RGBA);
}

#[test]
fn image_overflow_clips_to_rounded_corners() {
    let mut entry = image_entry(ScaleMode::Fill, None, 1.0);
    entry.corners = CornerRadii {
        top_left: 8.0,
        top_right: 8.0,
        bottom_right: 8.0,
        bottom_left: 8.0,
    };
    let rgba = image_scene(entry, 16.0, 16.0, 16);

    // (7,7) sits in the asset's top-left (red) quadrant, inside the
    // rounded box.
    assert_eq!(px(&rgba, 16, 7, 7), RED_RGBA, "inside still paints");
    assert_eq!(
        px(&rgba, 16, 0, 0),
        TRANSPARENT_RGBA,
        "the rounded corner clips the image"
    );
}

// ---- resolved subtree clips (issue #97) ------------------------------
//
// The painter consumes the clip regions `dashscene-core` resolves at
// commit; it never asks which node a box came from (P2). These fixtures
// are hand-built at boundary B, like the rest of the v0.3 vocabulary.

/// One filled 16x16 rect, clipped by `region`, rendered on a 16x16
/// surface.
fn clipped_square(region: ClipRegion) -> Vec<u8> {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry::solid(RED));
    let mut clips = ClipTable::new();
    let clip = clips.push(region);
    let rects = [RectEntry {
        x: 0.0,
        y: 0.0,
        w: 16.0,
        h: 16.0,
        paint,
        clip,
        opacity: 1.0,
    }];

    let mut painter = SkiaPainter::new(16, 16);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

#[test]
fn a_clip_region_confines_a_rect_to_its_ancestors_box() {
    let rgba = clipped_square(ClipRegion::new(vec![ClipBox {
        x: 4.0,
        y: 4.0,
        w: 8.0,
        h: 8.0,
        corners: CornerRadii::default(),
    }]));

    assert_eq!(px(&rgba, 16, 8, 8), RED_RGBA, "inside the clip box");
    assert_eq!(
        px(&rgba, 16, 2, 8),
        TRANSPARENT_RGBA,
        "left of the clip box: the rect covers it, the clip removes it"
    );
    assert_eq!(px(&rgba, 16, 8, 14), TRANSPARENT_RGBA, "below the clip box");
}

#[test]
fn a_rounded_clip_region_rounds_the_clipped_rect() {
    let rgba = clipped_square(ClipRegion::new(vec![ClipBox {
        x: 0.0,
        y: 0.0,
        w: 16.0,
        h: 16.0,
        corners: CornerRadii {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    }]));

    assert_eq!(px(&rgba, 16, 8, 8), RED_RGBA, "the middle still paints");
    assert_eq!(
        px(&rgba, 16, 0, 0),
        TRANSPARENT_RGBA,
        "the rounded corner clips the fill — a rounding no rect-intersection can express"
    );
}

#[test]
fn nested_clip_boxes_intersect() {
    // Two ancestor boxes overlapping in x in [8,12), y in [4,12).
    let rgba = clipped_square(ClipRegion::new(vec![
        ClipBox {
            x: 0.0,
            y: 4.0,
            w: 12.0,
            h: 8.0,
            corners: CornerRadii::default(),
        },
        ClipBox {
            x: 8.0,
            y: 0.0,
            w: 8.0,
            h: 16.0,
            corners: CornerRadii::default(),
        },
    ]));

    assert_eq!(px(&rgba, 16, 10, 8), RED_RGBA, "inside both boxes");
    assert_eq!(
        px(&rgba, 16, 4, 8),
        TRANSPARENT_RGBA,
        "inside the first box only"
    );
    assert_eq!(
        px(&rgba, 16, 10, 1),
        TRANSPARENT_RGBA,
        "inside the second box only"
    );
}

#[test]
fn a_clip_region_does_not_leak_into_the_next_rect() {
    // Slice order is stacking order; a clipped rect must not clip the
    // rects painted after it.
    let mut paints = PaintTable::new();
    let red = paints.push(PaintEntry::solid(RED));
    let blue = paints.push(PaintEntry::solid(BLUE));
    let mut clips = ClipTable::new();
    let corner = clips.push(ClipRegion::new(vec![ClipBox {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        corners: CornerRadii::default(),
    }]));
    let rects = [
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 16.0,
            h: 16.0,
            paint: red,
            clip: corner,
            opacity: 1.0,
        },
        RectEntry {
            x: 8.0,
            y: 8.0,
            w: 8.0,
            h: 8.0,
            paint: blue,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        },
    ];

    let mut painter = SkiaPainter::new(16, 16);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let rgba = painter.rgba_bytes();

    assert_eq!(px(&rgba, 16, 2, 2), RED_RGBA, "the clipped red rect");
    assert_eq!(px(&rgba, 16, 6, 6), TRANSPARENT_RGBA, "clipped away");
    assert_eq!(
        px(&rgba, 16, 12, 12),
        BLUE_RGBA,
        "the unclipped rect after it paints in full"
    );
}

#[test]
#[should_panic(expected = "gradient stop budget")]
fn a_diamond_gradient_with_too_many_stops_panics_by_name() {
    // One over the shared budget. Reading `MAX_GRADIENT_STOPS` rather than
    // spelling `9` keeps this test, the painter's assertion, and the
    // validator's rule (which rejects exactly this input upstream) pinned
    // to one number.
    let stops = (0..=MAX_GRADIENT_STOPS)
        .map(|i| GradientStop {
            offset: i as f32 / MAX_GRADIENT_STOPS as f32,
            color: RED,
        })
        .collect();
    let (rects, paints) =
        single_entry_scene(gradient_entry(GradientKind::Diamond, stops), 4.0, 4.0);

    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
}

/// Two side-by-side rects, so an incomplete dirty set can starve one.
fn two_rects(left_w: f32) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let l = paints.push(PaintEntry::solid(RED));
    let r = paints.push(PaintEntry::solid(GREEN));
    let rects = vec![
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: left_w,
            h: 16.0,
            paint: l,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        },
        RectEntry {
            x: 8.0,
            y: 0.0,
            w: 8.0,
            h: 16.0,
            paint: r,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        },
    ];
    (rects, paints)
}

/// One recorded frame for the retained-mode tests: the rect table, its
/// paint table, and the advisory dirty set for that commit.
type Frame = (Vec<RectEntry>, PaintTable, Option<Vec<u32>>);

/// Renders a sequence of (rects, paints, dirty) frames through one painter
/// in `mode`, returning the final surface. Named `render_frames` to avoid
/// the single-frame `render` helper above.
fn render_frames(mode: DirtyMode, frames: &[Frame]) -> Vec<u8> {
    let mut painter = SkiaPainter::with_mode(16, 16, mode);
    for (rects, paints, dirty) in frames {
        painter.paint(
            rects,
            paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &[],
            &GlyphRunTable::new(),
            dirty.as_deref(),
        );
    }
    painter.rgba_bytes()
}

/// With a complete dirty set, the retained buffer always equals the input
/// table, so the retained mode is pixel-identical to a full redraw. This is
/// the advisory contract.
#[test]
fn retained_mode_with_a_complete_dirty_set_matches_a_full_redraw() {
    let (r0, p0) = two_rects(8.0);
    let (r1, p1) = two_rects(4.0); // rect 0's width changed: its bits differ

    let frames = vec![
        (r0, p0, None),          // first frame: no dirty information
        (r1, p1, Some(vec![0])), // rect 0 is dirty, rect 1 is not
    ];

    let full = render_frames(DirtyMode::Full, &frames);
    let retained = render_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        full, retained,
        "a complete dirty set must not change the pixels"
    );
}

/// The mode must actually read `dirty`. If it does, withholding a changed
/// index leaves a stale entry in the retained buffer and the pixels
/// diverge. If this test passes trivially, `Retained` is not honoring the
/// set and the oracle in `goldens/tooling/tests/dirty_oracle.rs` would
/// prove nothing.
#[test]
fn retained_mode_starves_on_an_incomplete_dirty_set() {
    let (r0, p0) = two_rects(8.0);
    let (r1, p1) = two_rects(4.0); // rect 0 shrank...

    let frames = vec![
        (r0, p0, None),
        (r1, p1, Some(vec![])), // ...but the dirty set does not say so
    ];

    let full = render_frames(DirtyMode::Full, &frames);
    let retained = render_frames(DirtyMode::Retained, &frames);
    assert_ne!(
        full, retained,
        "an incomplete dirty set must leave the retained buffer stale"
    );
}

/// A solid-white N×N atlas image (PNG bytes): every texel decodes to
/// distance 1.0, so the MSDF median is 1.0 (fully inside the glyph)
/// everywhere. A synthetic field that isolates the atlas-quad plumbing
/// from real MSDF anti-aliasing — the golden covers the real field.
fn solid_atlas_png(n: i32) -> Vec<u8> {
    let mut painter = SkiaPainter::new(n, n);
    let mut paints = PaintTable::new();
    let white = paints.push(PaintEntry::solid(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }));
    let rects = [RectEntry {
        x: 0.0,
        y: 0.0,
        w: n as f32,
        h: n as f32,
        paint: white,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    }];
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.png_bytes()
}

/// A one-glyph atlas placing the whole image over one em, all "inside".
fn inside_atlas() -> (GlyphRunTable, AtlasIndex) {
    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: solid_atlas_png(4),
        },
        4,
        4,
        4,
        4.0,
        vec![AtlasGlyph {
            glyph_id: 1,
            plane_em: [0.0, 0.0, 1.0, 1.0],
            atlas_px: [0.0, 0.0, 4.0, 4.0],
        }],
    ));
    (glyphs, atlas)
}

#[test]
fn a_glyph_quad_fills_its_box_with_the_text_color() {
    // The whole atlas reads "inside", so the resolved coverage is 1 across
    // the quad and it renders at full text color — the quad placement,
    // atlas sampling, and colour modulation, without AA in the way.
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        atlas,
        size: 16.0,
        color: RED,
        glyphs: vec![GlyphQuad {
            glyph_id: 1,
            x: 8.0,
            y: 24.0,
        }],
        opacity: 1.0,
    });

    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        &[],
        &PaintTable::new(),
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &glyphs,
        None,
    );
    let rgba = painter.rgba_bytes();
    // plane_em over a 16 px em maps the glyph to the box (8, 8)-(24, 24).
    assert_eq!(
        pixel(&rgba, 32, 16, 16),
        RED_RGBA,
        "the glyph interior is the text colour"
    );
    assert_eq!(
        pixel(&rgba, 32, 2, 2),
        TRANSPARENT_RGBA,
        "outside the quad draws nothing"
    );
}

#[test]
fn a_glyph_absent_from_the_atlas_draws_nothing() {
    // glyph id 2 has no atlas quad (an empty outline such as a space, or a
    // glyph outside the charset): it paints nothing rather than panicking.
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        atlas,
        size: 16.0,
        color: RED,
        glyphs: vec![GlyphQuad {
            glyph_id: 2,
            x: 8.0,
            y: 24.0,
        }],
        opacity: 1.0,
    });

    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        &[],
        &PaintTable::new(),
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &glyphs,
        None,
    );
    let rgba = painter.rgba_bytes();
    assert!(
        rgba.chunks_exact(4).all(|p| p == TRANSPARENT_RGBA),
        "an absent glyph leaves the surface clear"
    );
}

// ---------------------------------------------------------------------
// Group opacity (story #44): the free-path per-rect alpha and the
// render-target group composite (`docs/decisions/masks-and-group-opacity.md`).
// ---------------------------------------------------------------------

fn render_with_groups(
    rects: &[RectEntry],
    paints: &PaintTable,
    groups: &[dashpaint::GroupComposite],
    width: i32,
    height: i32,
) -> Vec<u8> {
    let mut painter = SkiaPainter::new(width, height);
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        &ClipTable::new(),
        groups,
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

#[test]
fn free_path_opacity_modulates_a_solid_fills_alpha() {
    let (mut rects, paints) = single_entry_scene(PaintEntry::solid(RED), 8.0, 8.0);
    rects[0].opacity = 0.5;
    let rgba = render_with_groups(&rects, &paints, &[], 8, 8);

    let p = px(&rgba, 8, 4, 4);
    assert_eq!([p[0], p[1], p[2]], [255, 0, 0], "still red");
    assert!(
        (127..=128).contains(&p[3]),
        "alpha halved to ~128, got {}",
        p[3]
    );
}

#[test]
fn a_render_target_group_flattens_before_applying_alpha() {
    // Two opaque red rects overlap in x = [4, 8); a render-target group
    // over both composites at 0.5. The union is opaque red inside the
    // layer, so every covered pixel — overlap or not — reads the same
    // half alpha. The free path would instead double-blend the overlap
    // (0.5 over 0.5 = 0.75), so equal alphas is what proves the composite.
    let mut paints = PaintTable::new();
    let red = paints.push(PaintEntry::solid(RED));
    let rects = [
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        },
        RectEntry {
            x: 4.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
        },
    ];
    let groups = [dashpaint::GroupComposite {
        start: 0,
        end: 2,
        alpha: 0.5,
    }];
    let rgba = render_with_groups(&rects, &paints, &groups, 12, 8);

    let only_first = px(&rgba, 12, 2, 4); // rect 0 only
    let overlap = px(&rgba, 12, 6, 4); // rect 0 and rect 1
    let only_second = px(&rgba, 12, 10, 4); // rect 1 only

    assert_eq!([overlap[0], overlap[1], overlap[2]], [255, 0, 0]);
    assert_eq!(
        overlap[3], only_first[3],
        "the overlap is not darker than a single rect — the group flattened first"
    );
    assert_eq!(overlap[3], only_second[3]);
    assert!(
        (127..=128).contains(&overlap[3]),
        "group alpha 0.5 applied once, got {}",
        overlap[3]
    );
}

#[test]
fn a_glyph_runs_free_path_opacity_dims_the_text() {
    // Story #44 M4: a glyph run's free-path group alpha (`GlyphRun.opacity`)
    // dims the whole run, mirroring RectEntry.opacity for rects. A run at
    // 0.5 must paint fewer fully-inked pixels than the same run at 1.0.
    use dashpaint::GlyphRun;

    fn inked(opacity: f32) -> usize {
        let (mut glyphs, atlas) = inside_atlas();
        glyphs.push_run(GlyphRun {
            atlas,
            size: 16.0,
            color: RED,
            glyphs: vec![GlyphQuad {
                glyph_id: 1,
                x: 8.0,
                y: 24.0,
            }],
            opacity,
        });
        let mut painter = SkiaPainter::new(32, 32);
        painter.paint(
            &[],
            &PaintTable::new(),
            &ImageTable::new(),
            &ClipTable::new(),
            &[],
            &glyphs,
            None,
        );
        // Count near-fully-opaque inked pixels. The unpremultiplied red
        // channel stays ~255 regardless of alpha, so the alpha channel is
        // what the free-path opacity dims.
        painter
            .rgba_bytes()
            .chunks_exact(4)
            .filter(|p| p[3] > 180)
            .count()
    }

    let full = inked(1.0);
    let half = inked(0.5);
    assert!(full > 0, "the opaque run inks near-opaque pixels");
    assert!(
        half < full,
        "the 0.5 run inks fewer near-opaque pixels ({half} vs {full})",
    );
}

/// A drop shadow inks pixels outside the node's own box, in the offset
/// direction, and leaves the far side untouched (story #45).
#[test]
fn a_drop_shadow_inks_behind_and_outside_the_node() {
    use dashscene_core::{Shadow, ShadowKind};

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, Some("card"));
    txn.set_prop(node, Prop::X(8.0));
    txn.set_prop(node, Prop::Y(8.0));
    txn.set_prop(node, Prop::Width(16.0));
    txn.set_prop(node, Prop::Height(16.0));
    txn.set_prop(node, Prop::Fill(RED));
    txn.set_prop(
        node,
        Prop::Shadows(vec![Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 4.0, y: 4.0 },
            blur: 4.0,
            spread: 0.0,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        }]),
    );
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let rgba = painter.rgba_bytes();

    // The fill still covers the box interior.
    assert_eq!(pixel(&rgba, 32, 16, 16), RED_RGBA, "the fill is unchanged");
    // Just past the box's bottom-right (box ends at 24), in the offset
    // direction: the shadow inks it.
    assert!(
        pixel(&rgba, 32, 26, 26)[3] > 0,
        "the drop shadow inks outside the box, toward the offset"
    );
    // The opposite corner, away from the offset, stays clear.
    assert_eq!(
        pixel(&rgba, 32, 2, 2),
        TRANSPARENT_RGBA,
        "no shadow on the far side of the node"
    );
}

/// An inner shadow rings the inside edge and fades toward the center, so
/// the center is lighter than a pixel near the edge (story #45).
#[test]
fn an_inner_shadow_darkens_the_edge_not_the_center() {
    use dashscene_core::{Shadow, ShadowKind};

    const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, Some("well"));
    txn.set_prop(node, Prop::X(8.0));
    txn.set_prop(node, Prop::Y(8.0));
    txn.set_prop(node, Prop::Width(16.0));
    txn.set_prop(node, Prop::Height(16.0));
    txn.set_prop(node, Prop::Fill(WHITE));
    txn.set_prop(
        node,
        Prop::Shadows(vec![Shadow {
            kind: ShadowKind::Inner,
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 4.0,
            spread: 0.0,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        }]),
    );
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let rgba = painter.rgba_bytes();

    let center = pixel(&rgba, 32, 16, 16);
    let edge = pixel(&rgba, 32, 9, 9);
    // The inner shadow (black) darkens the edge; the center stays near the
    // white fill.
    assert!(
        center[0] > edge[0],
        "the inner shadow darkens the edge ({}) more than the center ({})",
        edge[0],
        center[0]
    );
    // And it stays inside the shape: a pixel outside the node is untouched.
    assert_eq!(
        pixel(&rgba, 32, 2, 2),
        TRANSPARENT_RGBA,
        "an inner shadow does not leak outside the node"
    );
}

/// Renders a single node (a `size`×`size` box at (bx,by,bw,bh)) with one
/// drop shadow, onto a `dim`×`dim` transparent surface, and returns the
/// RGBA readback. `stroke` optionally adds an outside stroke.
fn one_shadow(
    dim: i32,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
    shadow: dashscene_core::Shadow,
    stroke: Option<dashscene_core::Stroke>,
) -> Vec<u8> {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, Some("card"));
    txn.set_prop(node, Prop::X(bx));
    txn.set_prop(node, Prop::Y(by));
    txn.set_prop(node, Prop::Width(bw));
    txn.set_prop(node, Prop::Height(bh));
    txn.set_prop(node, Prop::Fill(RED));
    if let Some(s) = stroke {
        txn.set_prop(node, Prop::Stroke(s));
    }
    txn.set_prop(node, Prop::Shadows(vec![shadow]));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(dim, dim);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

/// P1: a drop shadow casts from the stroked silhouette, so an outside
/// stroke widens the shadow past the fill box.
#[test]
fn a_drop_shadow_casts_from_the_outside_stroke_silhouette() {
    use dashscene_core::{Shadow, ShadowKind, Stroke, StrokeAlign};

    let shadow = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 8.0 },
        blur: 0.0,
        spread: 0.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    };
    let stroke = Stroke {
        width: 4.0,
        align: StrokeAlign::Outside,
        color: Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
    };
    // node fill box (16,16)-(32,32); outside stroke widens the silhouette to
    // (12,12)-(36,36); the shadow offsets that down by 8 -> (12,20)-(36,44).
    let rgba = one_shadow(48, 16.0, 16.0, 16.0, 16.0, shadow, Some(stroke));

    // x in [12,16) at y=42 is left of the fill box, below the stroke, and
    // inked ONLY because the shadow casts from the stroked silhouette. With
    // the bare fill box the shadow's left edge would be x=16 and this pixel
    // would be transparent.
    assert!(
        pixel(&rgba, 48, 13, 42)[3] > 0,
        "the drop shadow reaches the stroke-widened left edge"
    );
    // Far outside even the widened shadow: clear.
    assert_eq!(
        pixel(&rgba, 48, 2, 2),
        TRANSPARENT_RGBA,
        "the shadow does not ink the far corner"
    );
}

/// T4: a zero-blur shadow has a hard edge — a pixel one step outside the
/// shadow box is fully transparent (a blurred shadow would bleed there).
#[test]
fn a_zero_blur_shadow_has_a_hard_edge() {
    use dashscene_core::{Shadow, ShadowKind};

    let shadow = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 0.0 },
        blur: 0.0,
        spread: 4.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    };
    // node (16,16)-(32,32); spread 4 -> shadow box (12,12)-(36,36).
    let rgba = one_shadow(48, 16.0, 16.0, 16.0, 16.0, shadow, None);

    // The spread band left of the fill: pixel 12 is inside the box, pixel 11
    // is exactly outside. Zero blur -> pixel 11 is fully transparent.
    assert!(
        pixel(&rgba, 48, 13, 24)[3] > 0,
        "inside the zero-blur shadow"
    );
    assert_eq!(
        pixel(&rgba, 48, 11, 24),
        TRANSPARENT_RGBA,
        "a zero-blur shadow does not bleed past its hard edge"
    );
}

/// T5: a negative spread shrinks the shadow — it inks fewer pixels than the
/// same shadow with a positive spread, which catches a sign error in the
/// spread math that the validator (which allows a negative spread) cannot.
#[test]
fn a_negative_spread_shrinks_the_shadow() {
    use dashscene_core::{Shadow, ShadowKind};

    let base = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 20.0 },
        blur: 0.0,
        spread: 0.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    };
    let ink = |spread: f32| {
        let s = Shadow { spread, ..base };
        one_shadow(64, 16.0, 16.0, 24.0, 24.0, s, None)
            .chunks_exact(4)
            .filter(|p| p[3] > 0)
            .count()
    };
    let shrunk = ink(-4.0);
    let grown = ink(4.0);
    assert!(shrunk > 0, "a negative-spread shadow still renders");
    assert!(
        shrunk < grown,
        "a negative spread inks fewer pixels ({shrunk}) than a positive one ({grown})"
    );
}

/// T6: a shadow whose color alpha is zero renders invisible.
#[test]
fn a_zero_alpha_shadow_is_invisible() {
    use dashscene_core::{Shadow, ShadowKind};

    let shadow = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 12.0 },
        blur: 4.0,
        spread: 0.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    };
    let rgba = one_shadow(48, 16.0, 16.0, 16.0, 16.0, shadow, None);

    // Where the shadow would ink if it had alpha, the surface stays clear.
    assert_eq!(
        pixel(&rgba, 48, 24, 34),
        TRANSPARENT_RGBA,
        "a fully transparent shadow leaves no ink"
    );
    // The fill is untouched.
    assert_eq!(pixel(&rgba, 48, 24, 24), RED_RGBA, "the fill still draws");
}

/// T7: a shadow pushed entirely off the surface paints nothing, without a
/// panic — the painter clips to the surface and the fill is intact.
#[test]
fn an_off_canvas_shadow_paints_nothing_without_error() {
    use dashscene_core::{Shadow, ShadowKind};

    let shadow = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 {
            x: 1000.0,
            y: 1000.0,
        },
        blur: 4.0,
        spread: 0.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    };
    // A 32×32 surface; the shadow lands ~1000 px off it.
    let rgba = one_shadow(32, 8.0, 8.0, 16.0, 16.0, shadow, None);

    // The fill draws; nothing else does.
    assert_eq!(pixel(&rgba, 32, 16, 16), RED_RGBA, "the fill draws");
    assert_eq!(
        pixel(&rgba, 32, 2, 2),
        TRANSPARENT_RGBA,
        "the off-canvas shadow leaves the surface clear"
    );
}
