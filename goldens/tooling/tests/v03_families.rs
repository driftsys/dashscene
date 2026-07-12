//! Per-family v0.3 paint goldens (issue #18): one focused scene per
//! construct family, so a visual regression implicates one family
//! (DESIGN_1.md §8 bisect-by-construction) rather than the combined
//! `v03-paint.png`. Hand-built at boundary B (no producer stages this
//! vocabulary); exact per-kind bytes live in the painter's unit tests.
//!
//! Anti-aliased gradients and curves are not bit-identical across CPU
//! architectures (`docs/decisions/golden-comparison-space.md`), so each
//! golden compares with a 2% differing-pixel tolerance; a few
//! interior-probe asserts pin the key property bit-stably.

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat, ImageTable,
    Mat23, PaintEntry, PaintKind, PaintTable, Painter, RectEntry, ScaleMode, Stroke, StrokeAlign,
    Vec2,
};
use dashscene_skia::SkiaPainter;

/// Small-canvas tolerance: cross-machine AA edge jitter is a larger
/// fraction of a 64×64 image than of the combined golden's 96×96, but
/// still far below any real change (a construct fills ≥1/4 of a strip).
const TOLERANCE: f64 = 0.02;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn probe(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
    goldens::pixel(rgba, size, x, y)
}

fn quantized(c: Color) -> [u8; 4] {
    let q = |v: f32| (v * 255.0).round() as u8;
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

fn full_box(paint: dashpaint::PaintIndex, w: f32, h: f32) -> RectEntry {
    RectEntry {
        x: 0.0,
        y: 0.0,
        w,
        h,
        paint,
    }
}

fn gradient_fill(kind: GradientKind, stops: Vec<GradientStop>) -> PaintKind {
    PaintKind::Gradient(Gradient {
        kind,
        handle_origin: Vec2 { x: 0.5, y: 0.5 },
        handle_primary: Vec2 { x: 1.0, y: 0.5 },
        handle_secondary: Vec2 { x: 0.5, y: 1.0 },
        stops,
    })
}

fn two_stops(a: Color, b: Color) -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: a,
        },
        GradientStop {
            offset: 1.0,
            color: b,
        },
    ]
}

/// A 4×4 checker asset, rendered through the painter itself.
fn checker_asset() -> ImageAsset {
    let mut painter = SkiaPainter::new(4, 4);
    let mut paints = PaintTable::new();
    let dark = paints.push(PaintEntry::solid(rgba(0.15, 0.15, 0.2)));
    let light = paints.push(PaintEntry::solid(rgba(0.9, 0.85, 0.7)));
    let mut rects = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            rects.push(RectEntry {
                x: x as f32,
                y: y as f32,
                w: 1.0,
                h: 1.0,
                paint: if (x + y) % 2 == 0 { dark } else { light },
            });
        }
    }
    painter.paint(&rects, &paints, &ImageTable::new());
    ImageAsset {
        format: ImageFormat::Png,
        bytes: painter.png_bytes(),
    }
}

#[test]
fn the_gradient_family_matches_its_golden() {
    let red = rgba(0.85, 0.15, 0.1);
    let blue = rgba(0.1, 0.2, 0.85);
    let gold = rgba(0.95, 0.75, 0.1);
    let teal = rgba(0.1, 0.6, 0.55);
    let green = rgba(0.15, 0.7, 0.2);
    let amber = rgba(0.95, 0.6, 0.05);

    // 2×2 strip of 32×32 cells: linear, radial, angular gauge, diamond.
    let mut paints = PaintTable::new();
    let linear = paints.push(PaintEntry {
        fill: Some(gradient_fill(GradientKind::Linear, two_stops(red, blue))),
        ..PaintEntry::default()
    });
    let radial = paints.push(PaintEntry {
        fill: Some(gradient_fill(GradientKind::Radial, two_stops(gold, teal))),
        ..PaintEntry::default()
    });
    // Gauge-style angular sweep: a green→amber→red dial arc.
    let gauge = paints.push(PaintEntry {
        fill: Some(gradient_fill(
            GradientKind::Angular,
            vec![
                GradientStop {
                    offset: 0.0,
                    color: green,
                },
                GradientStop {
                    offset: 0.5,
                    color: amber,
                },
                GradientStop {
                    offset: 1.0,
                    color: red,
                },
            ],
        )),
        ..PaintEntry::default()
    });
    let diamond = paints.push(PaintEntry {
        fill: Some(gradient_fill(GradientKind::Diamond, two_stops(teal, red))),
        ..PaintEntry::default()
    });

    let cell = |x: f32, y: f32, p| RectEntry {
        x,
        y,
        w: 32.0,
        h: 32.0,
        paint: p,
    };
    let rects = [
        cell(0.0, 0.0, linear),
        cell(32.0, 0.0, radial),
        cell(0.0, 32.0, gauge),
        cell(32.0, 32.0, diamond),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(&rects, &paints, &ImageTable::new());
    let bytes = painter.rgba_bytes();

    // Interior probes in clamped regions, where the color is exactly a
    // stop and bit-stable across machines (smooth-gradient interiors are
    // not, so the golden covers those).
    assert_eq!(
        probe(&bytes, 64, 2, 16),
        quantized(red),
        "linear left half clamps to stop 0"
    );
    assert_eq!(
        probe(&bytes, 64, 33, 1),
        quantized(teal),
        "radial outside the disk clamps to stop 1"
    );
    // (Diamond is a half-precision SkSL shader; its bytes can differ by
    // one code from exact quantization, so the golden covers it rather
    // than an exact probe.)

    goldens::assert_matches_golden_within("v03-gradients", &painter.png_bytes(), TOLERANCE);
}

#[test]
fn the_stroke_family_matches_its_golden() {
    let navy = rgba(0.06, 0.07, 0.1);
    let gold = rgba(0.95, 0.75, 0.1);
    let red = rgba(0.85, 0.15, 0.1);

    let mut paints = PaintTable::new();
    let background = paints.push(PaintEntry::solid(navy));
    let stroke_entry = |align, corners| PaintEntry {
        fill: Some(PaintKind::Solid { color: gold }),
        stroke: Some(Stroke {
            width: 4.0,
            align,
            color: red,
        }),
        corners,
        ..PaintEntry::default()
    };
    let inside = paints.push(stroke_entry(StrokeAlign::Inside, CornerRadii::default()));
    let center = paints.push(stroke_entry(StrokeAlign::Center, CornerRadii::default()));
    let outside = paints.push(stroke_entry(StrokeAlign::Outside, CornerRadii::default()));
    let rounded = paints.push(stroke_entry(
        StrokeAlign::Inside,
        CornerRadii {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    ));

    let cell = |x: f32, y: f32, p| RectEntry {
        x,
        y,
        w: 20.0,
        h: 20.0,
        paint: p,
    };
    let rects = [
        full_box(background, 64.0, 64.0),
        cell(6.0, 6.0, inside),
        cell(38.0, 6.0, center),
        cell(6.0, 38.0, outside),
        cell(38.0, 38.0, rounded),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(&rects, &paints, &ImageTable::new());
    let bytes = painter.rgba_bytes();

    // Each cell's centre is the gold fill; a rounded-cell square corner
    // shows the navy background (own-content rounded clip).
    assert_eq!(probe(&bytes, 64, 16, 16), quantized(gold), "inside fill");
    assert_eq!(probe(&bytes, 64, 48, 48), quantized(gold), "rounded fill");
    assert_eq!(
        probe(&bytes, 64, 38, 38),
        quantized(navy),
        "rounded square corner clipped to background"
    );

    goldens::assert_matches_golden_within("v03-strokes", &painter.png_bytes(), TOLERANCE);
}

#[test]
fn the_image_family_matches_its_golden() {
    let navy = rgba(0.06, 0.07, 0.1);
    let mut images = ImageTable::new();
    let checker = images.push(checker_asset());

    let mut paints = PaintTable::new();
    let background = paints.push(PaintEntry::solid(navy));
    let image_entry = |scale_mode, transform, corners| PaintEntry {
        fill: Some(PaintKind::Image {
            image: checker,
            scale_mode,
            transform,
            tile_scale: 1.0,
        }),
        corners,
        ..PaintEntry::default()
    };
    let fill = paints.push(image_entry(ScaleMode::Fill, None, CornerRadii::default()));
    let fit = paints.push(image_entry(ScaleMode::Fit, None, CornerRadii::default()));
    let crop = paints.push(image_entry(
        ScaleMode::Crop,
        // box uv [0,1] maps to image uv [0.25,0.5]×[0,0.25] = texel
        // (1,0), the single light texel, so the crop cell is uniform
        // and clearly distinct from the background.
        Some(Mat23 {
            a: 0.25,
            b: 0.0,
            c: 0.0,
            d: 0.25,
            tx: 0.25,
            ty: 0.0,
        }),
        CornerRadii::default(),
    ));
    // Tile in a rounded box: exercises tiling and the own-content
    // rounded clip together.
    let tile = paints.push(image_entry(
        ScaleMode::Tile,
        None,
        CornerRadii {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    ));

    // Non-square cells so Fill/Fit differ visibly.
    let cell = |x: f32, y: f32, p| RectEntry {
        x,
        y,
        w: 28.0,
        h: 20.0,
        paint: p,
    };
    let rects = [
        full_box(background, 64.0, 64.0),
        cell(4.0, 6.0, fill),
        cell(32.0, 6.0, fit),
        cell(4.0, 38.0, crop),
        cell(32.0, 38.0, tile),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(&rects, &paints, &images);
    let bytes = painter.rgba_bytes();

    // The crop transform selects texel (1,0), the light checker square,
    // uniformly across the whole cell.
    let light = quantized(rgba(0.9, 0.85, 0.7));
    assert_eq!(
        probe(&bytes, 64, 8, 42),
        light,
        "crop shows the light texel"
    );
    assert_eq!(probe(&bytes, 64, 24, 54), light, "crop is uniform");
    // Fit letterboxes a wide cell over the navy background.
    assert_eq!(
        probe(&bytes, 64, 33, 16),
        quantized(navy),
        "fit letterbox stays background"
    );

    goldens::assert_matches_golden_within("v03-images", &painter.png_bytes(), TOLERANCE);
}
