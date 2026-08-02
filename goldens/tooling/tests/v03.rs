//! The v0.3 paint-vocabulary golden (issue #14): every paint kind on
//! one canvas, hand-built at boundary B (no producer stages this
//! vocabulary yet). Per-kind pixel semantics live in
//! `crates/dashscene-skia/tests/painter.rs`; this golden pins the full
//! rendering.

use dashpaint::{
    ClipIndex, ClipTable, Color, CornerRadii, EntryParts, FillSpec, GlyphRunTable, Gradient,
    GradientKind, GradientStop, ImageFill, ImageTable, Mat23, PaintEntry, PaintKind, PaintTable,
    Painter, RectEntry, ScaleMode, StopRange, Stroke, StrokeAlign, Vec2,
};
use dashscene_skia::SkiaPainter;

mod common;
use common::checker_asset;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn stops(colors: [Color; 2]) -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: colors[0],
        },
        GradientStop {
            offset: 1.0,
            color: colors[1],
        },
    ]
}

fn gradient(paints: &mut PaintTable, kind: GradientKind, colors: [Color; 2]) -> PaintKind {
    paints.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind,
            handle_origin: Vec2 { x: 0.5, y: 0.5 },
            handle_primary: Vec2 { x: 1.0, y: 0.5 },
            handle_secondary: Vec2 { x: 0.5, y: 1.0 },
            stops: StopRange::NONE,
        },
        stops: stops(colors),
    })
}

#[test]
fn the_v03_paint_vocabulary_matches_its_golden() {
    let red = rgba(0.8, 0.15, 0.1);
    let blue = rgba(0.1, 0.2, 0.8);
    let gold = rgba(0.9, 0.7, 0.1);
    let teal = rgba(0.1, 0.6, 0.55);

    let mut images = ImageTable::new();
    let checker = images.push(checker_asset(rgba(0.2, 0.2, 0.25)));

    let mut paints = PaintTable::new();
    let background = paints.push_solid(rgba(0.06, 0.07, 0.1));
    let linear_fill = gradient(&mut paints, GradientKind::Linear, [red, blue]);
    let linear = paints.push(PaintEntry {
        fill: linear_fill,
        ..PaintEntry::default()
    });
    let radial_fill = gradient(&mut paints, GradientKind::Radial, [gold, teal]);
    let radial = paints.push(PaintEntry {
        fill: radial_fill,
        ..PaintEntry::default()
    });
    let angular_fill = gradient(&mut paints, GradientKind::Angular, [blue, gold]);
    let angular = paints.push(PaintEntry {
        fill: angular_fill,
        ..PaintEntry::default()
    });
    let diamond_fill = gradient(&mut paints, GradientKind::Diamond, [teal, red]);
    let diamond = paints.push(PaintEntry {
        fill: diamond_fill,
        ..PaintEntry::default()
    });
    let rounded_stroked_fill = paints.intern_fill(&FillSpec::Solid { color: gold });
    let rounded_stroked = paints.push_with(
        PaintEntry {
            fill: rounded_stroked_fill,
            corners: CornerRadii {
                top_left: 6.0,
                top_right: 6.0,
                bottom_right: 6.0,
                bottom_left: 6.0,
            },
            ..PaintEntry::default()
        },
        EntryParts {
            stroke: Some(Stroke {
                width: 3.0,
                align: StrokeAlign::Inside,
                color: red,
            }),
            ..EntryParts::default()
        },
    );
    let outside_stroke_only = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: 2.0,
                align: StrokeAlign::Outside,
                color: teal,
            }),
            ..EntryParts::default()
        },
    );
    let rounded_image_fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: checker,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let rounded_image = paints.push(PaintEntry {
        fill: rounded_image_fill,
        corners: CornerRadii {
            top_left: 10.0,
            top_right: 10.0,
            bottom_right: 10.0,
            bottom_left: 10.0,
        },
        ..PaintEntry::default()
    });
    let tiled_image_fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: checker,
        scale_mode: ScaleMode::Tile,
        transform: Mat23::IDENTITY,
        tile_scale: 2.0,
    }));
    let tiled_image = paints.push(PaintEntry {
        fill: tiled_image_fill,
        ..PaintEntry::default()
    });

    let entry = |x: f32, y: f32, w: f32, h: f32, paint| RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    };
    let rects = [
        entry(0.0, 0.0, 96.0, 96.0, background),
        entry(8.0, 8.0, 24.0, 24.0, linear),
        entry(40.0, 8.0, 24.0, 24.0, radial),
        entry(72.0, 8.0, 16.0, 24.0, angular),
        entry(8.0, 40.0, 24.0, 24.0, diamond),
        entry(40.0, 40.0, 24.0, 24.0, rounded_stroked),
        entry(74.0, 42.0, 12.0, 20.0, outside_stroke_only),
        entry(8.0, 72.0, 24.0, 16.0, rounded_image),
        entry(40.0, 72.0, 48.0, 16.0, tiled_image),
    ];

    let mut painter = SkiaPainter::new(96, 96);
    painter.paint(
        &rects,
        &paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );

    // Anti-aliased gradients and curves are not bit-identical across CPU
    // architectures; a small fraction absorbs cross-machine edge jitter
    // (docs/decisions/golden-comparison-space.md). The v0.1 golden stays
    // exact (integer-aligned solids, AA is a no-op there).
    goldens::assert_matches_golden_within("v03-paint", &painter.png_bytes(), 0.01);
}
