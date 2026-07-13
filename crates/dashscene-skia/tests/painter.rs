//! Story #4 acceptance path: a scene committed by dashscene-core paints
//! through the Skia CPU raster painter with exact, deterministic pixels
//! (issue #4; DESIGN_1.md §8.1) — the first end-to-end crossing of
//! boundary B.

use dashpaint::{
    Color, Gradient, GradientKind, ImageTable, MAX_GRADIENT_STOPS, PaintEntry, PaintKind, Painter,
    RectEntry, Vec2,
};
use dashscene_core::{Arena, Prop};
use dashscene_skia::SkiaPainter;

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
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

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
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

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
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

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
    CornerRadii, GradientStop, ImageAsset, ImageFormat, Mat23, PaintTable, ScaleMode, Stroke,
    StrokeAlign,
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
    painter.paint(rects, paints, images);
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
    }];
    let mut painter = SkiaPainter::new(16, 16);
    painter.paint(&rects, &paints, &ImageTable::new());
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
    })
    .collect();
    painter.paint(&rects, &paints, &ImageTable::new());
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
    painter.paint(&rects, &paints, &images);
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

#[test]
#[should_panic(expected = "issue #97")]
fn subtree_clip_panics_naming_the_follow_up_issue() {
    let mut entry = PaintEntry::solid(RED);
    entry.clip = true;
    let (rects, paints) = single_entry_scene(entry, 4.0, 4.0);

    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(&rects, &paints, &ImageTable::new());
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
    painter.paint(&rects, &paints, &ImageTable::new());
}
