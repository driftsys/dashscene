//! The v0.3 paint-vocabulary golden (issue #14): every paint kind on
//! one canvas, hand-built at boundary B (no producer stages this
//! vocabulary yet). Per-kind pixel semantics live in
//! `crates/dashscene-skia/tests/painter.rs`; this golden pins the full
//! rendering.

use dashpaint::{
    ClipIndex, ClipTable, Color, CornerRadii, GlyphRunTable, Gradient, GradientKind, GradientStop,
    ImageTable, PaintEntry, PaintKind, PaintTable, Painter, RectEntry, ScaleMode, Stroke,
    StrokeAlign, Vec2,
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

fn gradient(kind: GradientKind, colors: [Color; 2]) -> PaintKind {
    PaintKind::Gradient(Gradient {
        kind,
        handle_origin: Vec2 { x: 0.5, y: 0.5 },
        handle_primary: Vec2 { x: 1.0, y: 0.5 },
        handle_secondary: Vec2 { x: 0.5, y: 1.0 },
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
    let background = paints.push(PaintEntry::solid(rgba(0.06, 0.07, 0.1)));
    let linear = paints.push(PaintEntry {
        fill: Some(gradient(GradientKind::Linear, [red, blue])),
        ..PaintEntry::default()
    });
    let radial = paints.push(PaintEntry {
        fill: Some(gradient(GradientKind::Radial, [gold, teal])),
        ..PaintEntry::default()
    });
    let angular = paints.push(PaintEntry {
        fill: Some(gradient(GradientKind::Angular, [blue, gold])),
        ..PaintEntry::default()
    });
    let diamond = paints.push(PaintEntry {
        fill: Some(gradient(GradientKind::Diamond, [teal, red])),
        ..PaintEntry::default()
    });
    let rounded_stroked = paints.push(PaintEntry {
        fill: Some(PaintKind::Solid { color: gold }),
        stroke: Some(Stroke {
            width: 3.0,
            align: StrokeAlign::Inside,
            color: red,
        }),
        corners: CornerRadii {
            top_left: 6.0,
            top_right: 6.0,
            bottom_right: 6.0,
            bottom_left: 6.0,
        },
        ..PaintEntry::default()
    });
    let outside_stroke_only = paints.push(PaintEntry {
        stroke: Some(Stroke {
            width: 2.0,
            align: StrokeAlign::Outside,
            color: teal,
        }),
        ..PaintEntry::default()
    });
    let rounded_image = paints.push(PaintEntry {
        fill: Some(PaintKind::Image {
            image: checker,
            scale_mode: ScaleMode::Fill,
            transform: None,
            tile_scale: 1.0,
        }),
        corners: CornerRadii {
            top_left: 10.0,
            top_right: 10.0,
            bottom_right: 10.0,
            bottom_left: 10.0,
        },
        ..PaintEntry::default()
    });
    let tiled_image = paints.push(PaintEntry {
        fill: Some(PaintKind::Image {
            image: checker,
            scale_mode: ScaleMode::Tile,
            transform: None,
            tile_scale: 2.0,
        }),
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
