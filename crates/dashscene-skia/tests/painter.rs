//! Story #4 acceptance path: a scene committed by dashscene-core paints
//! through the Skia CPU raster painter with exact, deterministic pixels
//! (issue #4; docs/design/architecture.md) — the first end-to-end crossing of
//! boundary B.

use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, ClipIndex, ClipTable, Color, GlyphQuad, GlyphRun, GlyphRunTable,
    Gradient, GradientKind, ImageTable, MAX_GRADIENT_STOPS, PaintEntry, PaintIndex, PaintKind,
    Painter, RectEntry, Vec2,
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
    Blur, BlurKind, ClipBox, CornerRadii, GradientStop, ImageAsset, ImageFormat, Mat23, PaintTable,
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

// Stacked fills (story C1, debt #146): `PaintEntry.extra_fills` composites
// over `fill`, bottom to top, on the same box `fill` alone always painted.

#[test]
fn a_single_fill_entry_renders_byte_identically_with_no_extra_fills() {
    // The guard: an entry that never touches `extra_fills` (the
    // `PaintEntry::default()` empty vec) paints exactly the single fill it
    // always has — no compositing loop runs for it.
    let entry = PaintEntry::solid(RED);
    let (rects, paints) = single_entry_scene(entry, 4.0, 4.0);
    let rgba = render(&rects, &paints, &ImageTable::new(), 4);

    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(px(&rgba, 4, x, y), RED_RGBA, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn stacked_opaque_fills_composite_bottom_to_top_last_on_top() {
    // Three fully opaque layers stacked on one node: `fill` (bottom, red),
    // then `extra_fills` blue then green (top). Each fully occludes the one
    // below it, so the visible color is the *last* array element — proving
    // the painter draws the list in array order, not reversed.
    let entry = PaintEntry {
        fill: Some(PaintKind::Solid { color: RED }),
        extra_fills: vec![
            PaintKind::Solid { color: BLUE },
            PaintKind::Solid { color: GREEN },
        ],
        ..PaintEntry::default()
    };
    let (rects, paints) = single_entry_scene(entry, 4.0, 4.0);
    let rgba = render(&rects, &paints, &ImageTable::new(), 4);

    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(px(&rgba, 4, x, y), GREEN_RGBA, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn a_semi_transparent_stacked_fill_blends_over_the_bottom_fill() {
    // The stacked-fills fixture's own shape: an opaque bottom fill, a
    // semi-transparent layer on top. Real alpha compositing, not a bare
    // overwrite — the result carries both colors, and stays fully opaque
    // (compositing anything over an opaque base cannot lower its alpha).
    let entry = PaintEntry {
        fill: Some(PaintKind::Solid { color: RED }),
        extra_fills: vec![PaintKind::Solid {
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 0.5,
            },
        }],
        ..PaintEntry::default()
    };
    let (rects, paints) = single_entry_scene(entry, 4.0, 4.0);
    let rgba = render(&rects, &paints, &ImageTable::new(), 4);
    let center = px(&rgba, 4, 2, 2);

    assert_eq!(
        center[3], 255,
        "an opaque bottom fill keeps the result opaque"
    );
    assert!(
        (100..155).contains(&center[0]),
        "red channel carries the bottom fill's contribution: {center:?}"
    );
    assert_eq!(center[1], 0, "no fill in the stack has a green component");
    assert!(
        (100..155).contains(&center[2]),
        "blue channel carries the top fill's contribution: {center:?}"
    );
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
fn clipped_square(boxes: &[ClipBox]) -> Vec<u8> {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry::solid(RED));
    let mut clips = ClipTable::new();
    let clip = clips.push(boxes);
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
    let rgba = clipped_square(&[ClipBox {
        x: 4.0,
        y: 4.0,
        w: 8.0,
        h: 8.0,
        corners: CornerRadii::default(),
    }]);

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
    let rgba = clipped_square(&[ClipBox {
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
    }]);

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
    let rgba = clipped_square(&[
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
    ]);

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
    let corner = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        corners: CornerRadii::default(),
    }]);
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

/// The retained-mode tests' square surface. Named because
/// `assert_modes_agree_after_every_frame` turns a flat pixel index back into
/// a row and column with it, and would report the wrong coordinates if the two
/// drifted apart.
const RETAINED_SURFACE: i32 = 16;

/// Renders a sequence of (rects, paints, dirty) frames through one painter
/// in `mode`, returning the final surface. Named `render_frames` to avoid
/// the single-frame `render` helper above.
fn render_frames(mode: DirtyMode, frames: &[Frame]) -> Vec<u8> {
    let mut painter = SkiaPainter::with_mode(RETAINED_SURFACE, RETAINED_SURFACE, mode);
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

/// An index the rect table does not have is advisory noise, not a crash
/// (debt #181). The trait calls `dirty` advisory and says ignoring it is
/// always correct, so a caller honoring that contract may hand over a stale
/// index — a product painter carrying last frame's set across a shrink, for
/// one. The refresh used to index `rects[i]` directly and panic on it.
///
/// The in-range indices in the same set must still refresh, so the surface
/// stays equal to a full redraw: skipping the surplus index must not degrade
/// into skipping the whole set.
#[test]
fn retained_mode_ignores_an_out_of_range_dirty_index() {
    let (r0, p0) = two_rects(8.0);
    let (r1, p1) = two_rects(4.0); // rect 0's width changed: its bits differ

    let frames = vec![
        (r0, p0, None),
        // Both real indices, plus one the two-entry table does not have.
        (r1, p1, Some(vec![0, 1, 7])),
    ];

    let full = render_frames(DirtyMode::Full, &frames);
    let retained = render_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        full, retained,
        "an out-of-range dirty index must be skipped, and the valid ones still applied"
    );
}

/// The same two side-by-side rects plus a third drawn over their seam, so a
/// stale third entry is visible in the middle of the surface and a missing one
/// is visible as the seam showing through.
fn three_rects(left_w: f32) -> (Vec<RectEntry>, PaintTable) {
    let (mut rects, mut paints) = two_rects(left_w);
    let b = paints.push(PaintEntry::solid(BLUE));
    rects.push(RectEntry {
        x: 4.0,
        y: 4.0,
        w: 8.0,
        h: 8.0,
        paint: b,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    });
    (rects, paints)
}

/// Renders every prefix of `frames` in both modes and asserts the two agree
/// after each one.
///
/// Comparing only the final surface would let a divergence in the middle of the
/// sequence be papered over by a later full refresh, which is precisely the
/// path under test: every frame here changes the node count, so every frame
/// takes the full-refresh arm and a later one would repair an earlier one's
/// damage.
fn assert_modes_agree_after_every_frame(frames: &[Frame], what: &str) {
    for n in 1..=frames.len() {
        let prefix = &frames[..n];
        let full = render_frames(DirtyMode::Full, prefix);
        let retained = render_frames(DirtyMode::Retained, prefix);
        if full == retained {
            continue;
        }
        // Reported as a count plus the first disagreeing pixel rather than by
        // `assert_eq!` on the buffers: the surface is 16x16, so the derived
        // message would be two 1024-byte dumps and the reader would have to
        // find the difference by eye.
        let differing = full
            .chunks_exact(4)
            .zip(retained.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        let (at, a, b) = full
            .chunks_exact(4)
            .zip(retained.chunks_exact(4))
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, a.to_vec(), b.to_vec()))
            .expect("the buffers differ, so some pixel differs");
        panic!(
            "{what}: the retained buffer diverged from a full redraw after frame {}. \
             {differing} of {} pixels differ; the first is ({}, {}), full {a:?} retained {b:?}",
            n - 1,
            full.len() / 4,
            at % RETAINED_SURFACE as usize,
            at / RETAINED_SURFACE as usize,
        );
    }
}

/// A node count that changes **mid-sequence** forces a full refresh of the
/// retained buffer, in both directions.
///
/// `paint` takes the incremental arm only when `self.retained.len() ==
/// rects.len()`; any other case clears the buffer and re-uploads. Until this
/// test the `_` arm was reached only by the first frame, where the buffer is
/// empty — so the guard's job on a *structural* change was never exercised, and
/// narrowing it would have gone uncaught (debt #182).
///
/// The two directions fail differently, which is why both are here:
///
/// - **growing** the count with a guard that admits a shorter buffer indexes
///   past the buffer's end, because `dirty` names an index it does not have
///   yet;
/// - **shrinking** it with a guard that admits a longer buffer is silent. The
///   entries `dirty` names are refreshed, the surplus tail is not dropped, and
///   the painter draws from a buffer longer than the caller's table — a node
///   that no longer exists keeps rendering.
///
/// The shrinking frame keeps the three-entry paint table on purpose, holding
/// only two rects. Core pools paint entries and reclaims them only when most of
/// the table is unreachable, so a node going away does not necessarily shrink
/// the table, and this is an ordinary scene. It also decides what the failure
/// looks like: with the removed node's paint entry still resolvable, a surplus
/// tail renders a stale rect rather than panicking on an out-of-range paint
/// index, so the mutation is caught as the wrong picture — which is how it
/// would reach a product.
///
/// That frame carries an **empty** dirty set, also on purpose. Nothing in it
/// names the change, so the count itself is the only thing that can trigger the
/// refresh, and the assertion cannot pass by way of the incremental arm
/// happening to copy the right entries.
#[test]
fn retained_mode_refreshes_fully_when_the_node_count_changes() {
    let (two, two_paints) = two_rects(8.0);
    let (three, three_paints) = three_rects(8.0);
    let (shrunk, shrunk_paints) = {
        let (mut rects, paints) = three_rects(8.0);
        rects.pop();
        (rects, paints)
    };

    let frames = vec![
        // First frame: no dirty information, buffer empty — the pre-existing
        // route into the full-refresh arm.
        (two, two_paints, None),
        // The count grows. `dirty` names only the new index, which the
        // two-entry buffer does not have.
        (three, three_paints, Some(vec![2])),
        // The count shrinks, and nothing says so.
        (shrunk, shrunk_paints, Some(vec![])),
    ];

    assert_modes_agree_after_every_frame(&frames, "a node count changing mid-sequence");
}

/// The control for `retained_mode_refreshes_fully_when_the_node_count_changes`:
/// the three frames it renders are not all the same picture.
///
/// Without this, a painter that drew nothing at all would satisfy every
/// equality in that test. Here the two-rect and three-rect surfaces are
/// required to differ, so agreement between the modes is agreement about
/// something.
#[test]
fn the_node_count_change_frames_render_different_pictures() {
    let (two, two_paints) = two_rects(8.0);
    let (three, three_paints) = three_rects(8.0);

    let two_only = render_frames(DirtyMode::Full, &[(two, two_paints, None)]);
    let three_only = render_frames(DirtyMode::Full, &[(three, three_paints, None)]);
    assert_ne!(
        two_only, three_only,
        "the third rect must change the surface, or the node-count-change test compares two \
         identical pictures"
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

/// The anchor rect a hand-staged glyph run needs: one draws-nothing entry at
/// index 0, so `GlyphRun::rect` resolves.
///
/// A run is read against the rect table of the commit it came from, and the
/// painter now reaches through the anchor for the run's clip (issue #275). A
/// table with runs but no rects is not a scene commit can produce, so these
/// tests supply the rect rather than the painter tolerating its absence — a
/// run with no rect is a run with no clip and no group, which is the state
/// issue #505 exists to end.
fn anchor_rect() -> [RectEntry; 1] {
    [RectEntry {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
        paint: PaintIndex(0),
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    }]
}

/// The paint table [`anchor_rect`] indexes: one draws-nothing entry, the
/// same shared entry commit resolves an unfilled node to.
fn anchor_paints() -> PaintTable {
    let mut paints = PaintTable::new();
    paints.push(PaintEntry::default());
    paints
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
        rect: 0,
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
        &anchor_rect(),
        &anchor_paints(),
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
        rect: 0,
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
        &anchor_rect(),
        &anchor_paints(),
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
            rect: 0,
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
            &anchor_rect(),
            &anchor_paints(),
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

/// Issue #275: a run draws only inside the region its anchor rect carries.
///
/// The atlas is all-"inside", so the glyph fills its whole box — (8, 8) to
/// (24, 24) at a 16 px em — with no anti-aliased edge to blur the reading.
/// The anchor's clip stops at x = 16, so the glyph's left half must ink and
/// its right half must not. Before this, `draw_glyph_runs` ignored the clip
/// table entirely and the whole box inked.
#[test]
fn a_glyph_run_is_clipped_to_the_region_its_anchor_rect_carries() {
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        rect: 0,
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

    let mut clips = ClipTable::new();
    let clip = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 16.0,
        h: 32.0,
        corners: CornerRadii::default(),
    }]);
    let rects = [RectEntry {
        clip,
        ..anchor_rect()[0]
    }];

    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        &rects,
        &anchor_paints(),
        &ImageTable::new(),
        &clips,
        &[],
        &glyphs,
        None,
    );
    let rgba = painter.rgba_bytes();

    assert_eq!(
        pixel(&rgba, 32, 12, 16),
        RED_RGBA,
        "inside the clip: the glyph's left half inks"
    );
    assert_eq!(
        pixel(&rgba, 32, 20, 16),
        TRANSPARENT_RGBA,
        "outside the clip: the glyph's right half is cut, not drawn"
    );
}

/// The other half of issue #275: an unclipped anchor leaves the run
/// untouched, so clipping is applied where a region exists rather than
/// shrinking every run.
#[test]
fn a_run_whose_anchor_is_unclipped_draws_in_full() {
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        rect: 0,
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
        &anchor_rect(),
        &anchor_paints(),
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &glyphs,
        None,
    );
    let rgba = painter.rgba_bytes();

    assert_eq!(pixel(&rgba, 32, 12, 16), RED_RGBA, "left half inks");
    assert_eq!(pixel(&rgba, 32, 20, 16), RED_RGBA, "right half inks too");
}

/// One run's clip must not leak onto the next. Found by mutation testing
/// issue #275: dropping the `canvas.restore()` after a clipped run left
/// every test green, because no fixture drew a second run behind a clipped
/// one — the fixture could not express the difference, so it was the
/// fixture that was wrong.
///
/// Two runs, both inking the same right-hand pixel. The first is anchored
/// to a rect clipped to the left half; the second is unclipped and must
/// draw in full.
#[test]
fn a_clipped_runs_region_does_not_leak_onto_the_next_run() {
    let (mut glyphs, atlas) = inside_atlas();
    let quad = |x: f32| GlyphQuad {
        glyph_id: 1,
        x,
        y: 24.0,
    };
    let run = |rect: u32, x: f32| GlyphRun {
        rect,
        atlas,
        size: 16.0,
        color: RED,
        glyphs: vec![quad(x)],
        opacity: 1.0,
    };
    // Rect 0 is clipped to x < 16; rect 1 is unclipped.
    glyphs.push_run(run(0, 8.0));
    glyphs.push_run(run(1, 8.0));

    let mut clips = ClipTable::new();
    let clip = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 16.0,
        h: 32.0,
        corners: CornerRadii::default(),
    }]);
    let rects = [
        RectEntry {
            clip,
            ..anchor_rect()[0]
        },
        anchor_rect()[0],
    ];

    let mut painter = SkiaPainter::new(32, 32);
    painter.paint(
        &rects,
        &anchor_paints(),
        &ImageTable::new(),
        &clips,
        &[],
        &glyphs,
        None,
    );
    let rgba = painter.rgba_bytes();

    assert_eq!(
        pixel(&rgba, 32, 20, 16),
        RED_RGBA,
        "the second run is unclipped: the first run's clip was restored"
    );
}

/// A run whose anchor names no rect is a broken contract between crates,
/// not a scene to draw unclipped: a run and the rect table it is read
/// against come from one commit (P4). Drawing it as foreground would be the
/// silent-wrong-pixel outcome issue #505 exists to end.
#[test]
#[should_panic(expected = "out of range")]
fn a_run_anchored_past_the_rect_table_is_named_rather_than_drawn_unclipped() {
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        rect: 7,
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
        &anchor_rect(),
        &anchor_paints(),
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &glyphs,
        None,
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

// --------------------------------------------------------------------------
// Free-path folded opacity: a fill and its own stroke (debt #277).
//
// On the free path `RectEntry::opacity` is folded into each draw separately
// (`docs/decisions/masks-and-group-opacity.md`). Where an Inside- or
// Center-aligned stroke lies over its own fill, that means the fill is dimmed,
// and then the dimmed stroke composites *over* the already-dimmed fill —
// alpha over alpha. The composite path flattens the node first and dims once,
// so the two paths disagree in the overlap band.
//
// The scene is chosen so the arithmetic is exact and readable: an opaque fill
// and an opaque stroke of the same colour, at opacity 0.5. Flattening first
// gives a half-transparent node, alpha 128. Compositing twice gives
// 128 + 128*(128/255) = 192.

fn fill_and_stroke_at_half_opacity() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry {
        fill: Some(PaintKind::Solid { color: RED }),
        stroke: Some(Stroke {
            width: 8.0,
            align: StrokeAlign::Inside,
            color: RED,
        }),
        ..PaintEntry::default()
    });
    (
        vec![RectEntry {
            x: 16.0,
            y: 16.0,
            w: 32.0,
            h: 32.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 0.5,
        }],
        paints,
    )
}

#[test]
fn a_stroke_over_its_own_fill_is_dimmed_once_not_twice() {
    let (rects, paints) = fill_and_stroke_at_half_opacity();
    let bytes = render(&rects, &paints, &ImageTable::new(), 64);

    // (20, 32) sits inside the 8px Inside stroke band, which covers x in
    // [16, 24), and therefore over the fill as well.
    let i = ((32 * 64) + 20) * 4;
    let alpha = bytes[i + 3];

    // The node is opaque before its group alpha, so the whole node — stroke
    // band included — must read the group alpha exactly once.
    assert_eq!(
        alpha, 128,
        "the stroke band read alpha {alpha}, so the fill and its own stroke \
         were each dimmed and then composited (alpha over alpha) instead of \
         the node being flattened and dimmed once",
    );

    // The fill-only interior is the control: it has always been correct, so a
    // regression here would mean the fix broke the ordinary path.
    let interior = ((32 * 64) + 32) * 4;
    assert_eq!(
        bytes[interior + 3],
        128,
        "the fill-only interior must be unchanged"
    );
}

#[test]
fn a_stroke_with_no_fill_is_correct_at_partial_opacity() {
    // The companion to the test above, and the reason its `has_fill` guard is
    // an optimisation rather than a correctness gate: with no fill underneath,
    // there is only one draw in the band, so folding the group alpha into it
    // and flattening the node both give the same answer. Removing the guard
    // does not change any rendered result — it only opens an offscreen layer
    // that cannot matter.
    //
    // Pinned so that stays true. If a later change makes the two paths differ
    // for a stroke-only node, this fails and the guard becomes a real gate
    // that needs its own reasoning.
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry {
        stroke: Some(Stroke {
            width: 8.0,
            align: StrokeAlign::Inside,
            color: RED,
        }),
        ..PaintEntry::default()
    });
    let rects = vec![RectEntry {
        x: 16.0,
        y: 16.0,
        w: 32.0,
        h: 32.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 0.5,
    }];
    let bytes = render(&rects, &paints, &ImageTable::new(), 64);

    let band = ((32 * 64) + 20) * 4;
    assert_eq!(
        bytes[band + 3],
        128,
        "a stroke with no fill under it must read the group alpha exactly once",
    );
}

// --------------------------------------------------------------------------
// A backdrop blur replaces its region (debt #405).
//
// `draw_backdrop_blur_box` opens a `save_layer` carrying the blurred backdrop
// and restores it, so the layer composited SrcOver over the *unmodified*
// backdrop. A real backdrop filter replaces the region. When the backdrop is
// opaque the two are indistinguishable — `alpha = 1` replaces — which is every
// case measured before this test. When the backdrop is partially transparent
// the blurred copy also has `alpha < 1`, so the sharp original showed through
// underneath and the blur's alpha falloff was lost: the alpha edge stayed
// hard.
//
// The scene is the one issue #405 measured: a 64x64 canvas cleared to
// transparent, an opaque red band covering x < 32, and a fill-less panel at
// (16,16)-(48,48) carrying a backdrop blur of radius 12. Row y = 32 crosses
// the band's edge inside the panel, so it is where the falloff must appear.

fn transparent_backdrop_blur_scene() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let band = paints.push(PaintEntry {
        fill: Some(PaintKind::Solid { color: RED }),
        ..PaintEntry::default()
    });
    let panel = paints.push(PaintEntry {
        blurs: vec![Blur {
            kind: BlurKind::Backdrop,
            radius: 12.0,
        }],
        ..PaintEntry::default()
    });
    (
        vec![
            RectEntry {
                x: 0.0,
                y: 0.0,
                w: 32.0,
                h: 64.0,
                paint: band,
                clip: ClipIndex::UNCLIPPED,
                opacity: 1.0,
            },
            RectEntry {
                x: 16.0,
                y: 16.0,
                w: 32.0,
                h: 32.0,
                paint: panel,
                clip: ClipIndex::UNCLIPPED,
                opacity: 1.0,
            },
        ],
        paints,
    )
}

#[test]
fn a_backdrop_blur_over_a_transparent_backdrop_softens_its_alpha_edge() {
    let (rects, paints) = transparent_backdrop_blur_scene();
    let bytes = render(&rects, &paints, &ImageTable::new(), 64);
    let alpha_at = |x: usize| bytes[(((32 * 64) + x) * 4) + 3];

    // Inside the panel and inside the opaque band: still essentially opaque,
    // because the blur only starts pulling in transparency near the edge.
    assert!(
        alpha_at(18) >= 250,
        "deep inside the band the blur should stay near-opaque, got {}",
        alpha_at(18),
    );

    // Approaching the band's edge at x = 32, the blurred copy must carry
    // progressively less alpha. Compositing over the sharp original instead
    // pins every one of these at 255.
    let (a24, a28, a31) = (alpha_at(24), alpha_at(28), alpha_at(31));
    assert!(
        a24 < 250 && a28 < a24 && a31 < a28,
        "the alpha edge must soften across the blur: got {a24} at x=24, \
         {a28} at x=28, {a31} at x=31 — a flat 255 means the layer \
         composited over the sharp backdrop instead of replacing it",
    );
}

// ---------------------------------------------------------------------
// Retained group composition (issue #278): in `DirtyMode::Retained` a
// group whose rect range no dirty index touches blends the previous
// frame's composite again instead of redrawing its subtree.
//
// Two things have to hold at once, and a test of only one of them would
// pass on an obviously broken painter. A cache that never invalidates
// composites once and renders stale pixels forever; a cache that always
// invalidates renders correctly and saves nothing. So the counter is
// asserted in both directions — stable groups build once, a touched group
// builds again — and the pixels are compared against `DirtyMode::Full`
// after every frame, which is the boundary-B rule that a painter honoring
// the dirty set must render what one ignoring it renders.
// ---------------------------------------------------------------------

/// The retention tests' square surface.
const GROUP_SURFACE: i32 = 24;

/// One frame of a retention sequence: the rect table, the render-target
/// groups, and the advisory dirty set for that commit.
struct GroupFrame {
    rects: Vec<RectEntry>,
    groups: Vec<dashpaint::GroupComposite>,
    dirty: Option<Vec<u32>>,
}

/// Two nested render-target groups over overlapping rounded rects, with a
/// glyph run anchored inside the inner one.
///
/// Every ingredient is deliberate. **Overlap** is what makes a render
/// target necessary at all — without it the alpha would ride on each rect.
/// **Rounded corners** put anti-aliased edges inside the layer, where a
/// composite blended even slightly differently from `save_layer_alpha`
/// shows up first. **Nesting** exercises a layer blended into another
/// layer rather than onto the base surface. The **glyph run** is there
/// because runs draw inside the group layers enclosing their anchor
/// (issues #274 and #275), so a reused composite has to carry them; a
/// painter that skipped the range but drew its runs anyway would show the
/// text twice over.
///
/// `inner_y` moves the innermost rect, which is the mutation the sequences
/// below apply.
fn nested_group_scene(
    inner_y: f32,
) -> (Vec<RectEntry>, PaintTable, Vec<dashpaint::GroupComposite>) {
    let mut paints = PaintTable::new();
    let rounded = |color| PaintEntry {
        corners: CornerRadii {
            top_left: 3.0,
            top_right: 3.0,
            bottom_right: 3.0,
            bottom_left: 3.0,
        },
        ..PaintEntry::solid(color)
    };
    let backdrop = paints.push(PaintEntry::solid(BLUE));
    let outer = paints.push(rounded(RED));
    let middle = paints.push(rounded(GREEN));
    let inner = paints.push(rounded(BLUE));

    let rect = |x: f32, y: f32, w: f32, h: f32, paint| RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    };
    let rects = vec![
        // 0: outside every group — the surface the composites blend onto.
        rect(0.0, 0.0, 24.0, 24.0, backdrop),
        // 1: the outer group's own node.
        rect(2.0, 2.0, 14.0, 14.0, outer),
        // 2: the inner group's own node, overlapping rect 1.
        rect(8.0, 6.0, 12.0, 12.0, middle),
        // 3: inside the inner group, overlapping rect 2.
        rect(10.0, inner_y, 10.0, 8.0, inner),
    ];
    let groups = vec![
        dashpaint::GroupComposite {
            start: 1,
            end: 4,
            alpha: 0.5,
        },
        dashpaint::GroupComposite {
            start: 2,
            end: 4,
            alpha: 0.75,
        },
    ];
    (rects, paints, groups)
}

/// A glyph run anchored at rect 2 — inside both groups.
fn group_glyphs() -> GlyphRunTable {
    let (mut glyphs, atlas) = inside_atlas();
    glyphs.push_run(GlyphRun {
        rect: 2,
        atlas,
        size: 6.0,
        color: RED,
        glyphs: vec![GlyphQuad {
            glyph_id: 1,
            x: 9.0,
            y: 12.0,
        }],
        opacity: 1.0,
    });
    glyphs
}

/// Renders a sequence of frames through one painter in `mode`, returning
/// the surface after **every** frame and the painter's composite-build
/// count at the end.
///
/// Every frame's surface is kept rather than only the last, because a
/// stale composite that a later frame happens to rebuild would otherwise
/// be invisible — the same reason `assert_modes_agree_after_every_frame`
/// renders prefixes.
fn render_group_frames(mode: DirtyMode, frames: &[GroupFrame]) -> (Vec<Vec<u8>>, u64) {
    let mut painter = SkiaPainter::with_mode(GROUP_SURFACE, GROUP_SURFACE, mode);
    let glyphs = group_glyphs();
    // The paint table does not depend on `inner_y` — only rect 3's position
    // does — so one table serves every frame, and the `PaintIndex` values in
    // each frame's rects resolve against it.
    let (_, paints, _) = nested_group_scene(0.0);
    let mut surfaces = Vec::with_capacity(frames.len());
    for frame in frames {
        painter.paint(
            &frame.rects,
            &paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &frame.groups,
            &glyphs,
            frame.dirty.as_deref(),
        );
        surfaces.push(painter.rgba_bytes());
    }
    (surfaces, painter.group_composites_built())
}

/// A frame whose rect table and groups come from [`nested_group_scene`].
fn group_frame(inner_y: f32, dirty: Option<Vec<u32>>) -> GroupFrame {
    let (rects, _, groups) = nested_group_scene(inner_y);
    GroupFrame {
        rects,
        groups,
        dirty,
    }
}

/// Asserts the two modes rendered the same pixels after every frame, and
/// reports the first disagreement by coordinate rather than by dumping two
/// buffers.
fn assert_group_modes_agree(full: &[Vec<u8>], retained: &[Vec<u8>], what: &str) {
    for (frame, (a, b)) in full.iter().zip(retained).enumerate() {
        if a == b {
            continue;
        }
        let differing = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(x, y)| x != y)
            .count();
        let (at, x, y) = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .position(|(x, y)| x != y)
            .map(|at| (at, &a[at * 4..at * 4 + 4], &b[at * 4..at * 4 + 4]))
            .expect("the buffers differ, so some pixel does");
        panic!(
            "{what}: frame {frame} diverged from a full redraw at \
             ({}, {}) — full {x:?}, retained {y:?}, {differing} pixels differ \
             in all. A retained group composite must render what a full \
             redraw renders.",
            at % GROUP_SURFACE as usize,
            at / GROUP_SURFACE as usize,
        );
    }
}

/// Direction one: across frames whose groups and rects are unchanged, each
/// composite is built **once**, not once per frame.
///
/// Falsifiable: making `GroupCache::reuse` always return `None` (the
/// pre-#278 behavior, which rebuilt every composite every commit) makes
/// this read 10 instead of 2.
#[test]
fn a_stable_group_builds_its_composite_once_across_frames() {
    let frames: Vec<GroupFrame> = std::iter::once(group_frame(10.0, None))
        .chain((1..5).map(|_| group_frame(10.0, Some(Vec::new()))))
        .collect();

    let (retained, builds) = render_group_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        builds, 2,
        "two groups over five frames must build two composites — one each \
         on the first frame, which has no dirty information — not two per \
         frame"
    );

    let (full, full_builds) = render_group_frames(DirtyMode::Full, &frames);
    assert_eq!(
        full_builds, 0,
        "DirtyMode::Full composites through save_layer and retains nothing"
    );
    assert_group_modes_agree(&full, &retained, "a stable group");
}

/// Direction two: a dirty rect inside a group's range rebuilds that
/// group's composite, and the pixels follow.
///
/// Both halves matter. The count proves the cache noticed; the pixel
/// comparison proves it noticed for the right reason — a cache that
/// rebuilt on a timer would pass the count and still be wrong.
///
/// Falsifiable: making `GroupCache::begin_frame` ignore `dirty` entirely
/// (keeping every entry whose range still exists) makes this read 2
/// instead of 4, and the pixel comparison then fails on frame 3 as well.
#[test]
fn a_dirty_rect_inside_a_group_rebuilds_its_composite() {
    // Frame 3 moves rect 3, which sits inside both group ranges, so both
    // composites are rebuilt: two on the first frame plus two more.
    let frames = vec![
        group_frame(10.0, None),
        group_frame(10.0, Some(Vec::new())),
        group_frame(10.0, Some(Vec::new())),
        group_frame(14.0, Some(vec![3])),
        group_frame(14.0, Some(Vec::new())),
    ];

    let (retained, builds) = render_group_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        builds, 4,
        "a rect inside both ranges must rebuild both composites: two on the \
         first frame, two on the frame that moved it, and none on the \
         clean frames around them"
    );

    let (full, _) = render_group_frames(DirtyMode::Full, &frames);
    assert_group_modes_agree(&full, &retained, "a dirty rect inside a group");

    // The mutation has to be visible at all, or the comparison above holds
    // trivially and the count assertion is measuring nothing.
    assert_ne!(
        full[2], full[3],
        "moving rect 3 must change the picture, or this test proves nothing"
    );
}

/// A rect that changed **outside** every group range leaves both
/// composites alone: this is the case the retention exists for, and the
/// one a cache keyed on "anything changed" would get wrong.
#[test]
fn a_dirty_rect_outside_every_group_keeps_both_composites() {
    let mut moved = group_frame(10.0, Some(vec![0]));
    moved.rects[0].w = 20.0;

    let frames = vec![group_frame(10.0, None), moved];

    let (retained, builds) = render_group_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        builds, 2,
        "rect 0 is outside [1, 4) and [2, 4), so neither composite is rebuilt"
    );
    let (full, _) = render_group_frames(DirtyMode::Full, &frames);
    assert_group_modes_agree(&full, &retained, "a dirty rect outside every group");
}

/// A group that dissolves and one that re-forms both go through the cache
/// without leaving a composite behind that no longer describes the scene.
///
/// `dashscene-core` dirties every rect a group covers when the group is
/// present on only one side of a commit, so the sequence below hands over
/// the dirty sets that commit would produce.
#[test]
fn a_group_that_dissolves_and_re_forms_rebuilds_its_composite() {
    let base = group_frame(10.0, None);
    let mut dissolved = group_frame(10.0, Some(vec![1, 2, 3]));
    // Only the inner group survives; the outer one is gone, and core would
    // have folded its alpha into the rects. The rect bits are left as they
    // are: what is under test is the group set, not the alpha arithmetic.
    dissolved.groups.remove(0);
    let reformed = group_frame(10.0, Some(vec![1, 2, 3]));

    let frames = vec![base, dissolved, reformed];
    let (retained, builds) = render_group_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        builds, 5,
        "frame 1 builds both, frame 2 rebuilds the surviving inner one, and \
         frame 3 rebuilds both"
    );
    let (full, _) = render_group_frames(DirtyMode::Full, &frames);
    assert_group_modes_agree(&full, &retained, "a group that dissolves and re-forms");
}

/// A changed rect count takes the painter's full-refresh arm, on which the
/// dirty set does not describe the difference between the two tables — so
/// no composite may be reused either.
#[test]
fn a_changed_rect_count_rebuilds_every_composite() {
    let mut shorter = group_frame(10.0, Some(Vec::new()));
    shorter.rects.pop();
    shorter.groups[0].end = 3;
    shorter.groups[1].end = 3;

    let frames = vec![group_frame(10.0, None), shorter];
    let (retained, builds) = render_group_frames(DirtyMode::Retained, &frames);
    assert_eq!(
        builds, 4,
        "the second frame's rect table is a different length, so its empty \
         dirty set says nothing and both composites are rebuilt"
    );
    let (full, _) = render_group_frames(DirtyMode::Full, &frames);
    assert_group_modes_agree(&full, &retained, "a changed rect count");
}
