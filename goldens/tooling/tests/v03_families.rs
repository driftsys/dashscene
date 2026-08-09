//! Per-family v0.3 paint goldens (issue #18): one focused scene per
//! construct family, so a visual regression implicates one family
//! (docs/design/architecture.md, bisect-by-construction) rather than the combined
//! `v03-paint.png`. Hand-built at boundary B (no producer stages this
//! vocabulary); exact per-kind bytes live in the painter's unit tests.
//!
//! Anti-aliased gradients and curves are not bit-identical across CPU
//! architectures (`docs/decisions/golden-comparison-space.md`), so each
//! golden compares with a 2% differing-pixel tolerance; a few
//! interior-probe asserts pin the key property bit-stably.

use dashpaint::{
    ClipIndex, ClipTable, Color, CornerRadii, EntryParts, FillSpec, GlyphRunTable, Gradient,
    GradientKind, GradientStop, ImageFill, ImageTable, Mat23, PaintEntry, PaintKind, PaintTable,
    Painter, RectEntry, ScaleMode, StopRange, Stroke, StrokeAlign, Vec2,
};
use dashscene_skia::SkiaPainter;

mod common;
use common::checker_asset;

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
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }
}

fn gradient_fill(
    paints: &mut PaintTable,
    kind: GradientKind,
    stops: Vec<GradientStop>,
) -> PaintKind {
    paints.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind,
            handle_origin: Vec2 { x: 0.5, y: 0.5 },
            handle_primary: Vec2 { x: 1.0, y: 0.5 },
            handle_secondary: Vec2 { x: 0.5, y: 1.0 },
            stops: StopRange::NONE,
        },
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
    let linear_fill = gradient_fill(&mut paints, GradientKind::Linear, two_stops(red, blue));
    let linear = paints.push(PaintEntry {
        fill: linear_fill,
        ..PaintEntry::default()
    });
    let radial_fill = gradient_fill(&mut paints, GradientKind::Radial, two_stops(gold, teal));
    let radial = paints.push(PaintEntry {
        fill: radial_fill,
        ..PaintEntry::default()
    });
    // Gauge-style angular sweep: a green→amber→red dial arc.
    let gauge_fill = gradient_fill(
        &mut paints,
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
    );
    let gauge = paints.push(PaintEntry {
        fill: gauge_fill,
        ..PaintEntry::default()
    });
    let diamond_fill = gradient_fill(&mut paints, GradientKind::Diamond, two_stops(teal, red));
    let diamond = paints.push(PaintEntry {
        fill: diamond_fill,
        ..PaintEntry::default()
    });

    let cell = |x: f32, y: f32, p| RectEntry {
        x,
        y,
        w: 32.0,
        h: 32.0,
        paint: p,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    let rects = [
        cell(0.0, 0.0, linear),
        cell(32.0, 0.0, radial),
        cell(0.0, 32.0, gauge),
        cell(32.0, 32.0, diamond),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
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
    let background = paints.push_solid(navy);
    let gold_fill = paints.intern_fill(&FillSpec::Solid { color: gold });
    let stroke_entry = |paints: &mut PaintTable, align, corners| {
        paints.push_with(
            PaintEntry {
                fill: gold_fill,
                corners,
                ..PaintEntry::default()
            },
            EntryParts {
                stroke: Some(Stroke {
                    width: 4.0,
                    align,
                    color: red,
                }),
                ..EntryParts::default()
            },
        )
    };
    let inside = stroke_entry(&mut paints, StrokeAlign::Inside, CornerRadii::default());
    let center = stroke_entry(&mut paints, StrokeAlign::Center, CornerRadii::default());
    let outside = stroke_entry(&mut paints, StrokeAlign::Outside, CornerRadii::default());
    let rounded = stroke_entry(
        &mut paints,
        StrokeAlign::Inside,
        CornerRadii {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    );

    let cell = |x: f32, y: f32, p| RectEntry {
        x,
        y,
        w: 20.0,
        h: 20.0,
        paint: p,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    let rects = [
        full_box(background, 64.0, 64.0),
        cell(6.0, 6.0, inside),
        cell(38.0, 6.0, center),
        cell(6.0, 38.0, outside),
        cell(38.0, 38.0, rounded),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
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
    let checker = images.push(checker_asset(rgba(0.15, 0.15, 0.2)));

    let mut paints = PaintTable::new();
    let background = paints.push_solid(navy);
    let image_fill = |paints: &mut PaintTable, scale_mode, transform| {
        paints.intern_fill(&FillSpec::Image(ImageFill {
            image: checker,
            scale_mode,
            transform,
            tile_scale: 1.0,
        }))
    };
    let image_entry = |fill, corners| PaintEntry {
        fill,
        corners,
        ..PaintEntry::default()
    };
    let fill_fill = image_fill(&mut paints, ScaleMode::Fill, Mat23::IDENTITY);
    let fill = paints.push(image_entry(fill_fill, CornerRadii::default()));
    let fit_fill = image_fill(&mut paints, ScaleMode::Fit, Mat23::IDENTITY);
    let fit = paints.push(image_entry(fit_fill, CornerRadii::default()));
    // box uv [0,1] maps to image uv [0.25,0.5]×[0,0.25] = texel
    // (1,0), the single light texel, so the crop cell is uniform
    // and clearly distinct from the background.
    let crop_fill = image_fill(
        &mut paints,
        ScaleMode::Crop,
        Mat23 {
            a: 0.25,
            b: 0.0,
            c: 0.0,
            d: 0.25,
            tx: 0.25,
            ty: 0.0,
        },
    );
    let crop = paints.push(image_entry(crop_fill, CornerRadii::default()));
    // Tile in a rounded box: exercises tiling and the own-content
    // rounded clip together.
    let tile_fill = image_fill(&mut paints, ScaleMode::Tile, Mat23::IDENTITY);
    let tile = paints.push(image_entry(
        tile_fill,
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
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    let rects = [
        full_box(background, 64.0, 64.0),
        cell(4.0, 6.0, fill),
        cell(32.0, 6.0, fit),
        cell(4.0, 38.0, crop),
        cell(32.0, 38.0, tile),
    ];

    let mut painter = SkiaPainter::new(64, 64);
    painter.paint(
        &rects,
        &paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
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
