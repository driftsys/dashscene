//! The paint gate (story #15): the budgets that need a *solved* box, and
//! the boundary-B index resolution the painter would otherwise panic on.
//!
//! These rules are unreachable at the load gate: P1 says the document
//! carries intent, never results, so a `Hug`/`Fill` node has no box for a
//! stroke width to be measured against (issue #100).

use dashpaint::{
    Color, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat, ImageTable, PaintEntry,
    PaintIndex, PaintKind, PaintTable, RectEntry, ScaleMode, Stroke, StrokeAlign, Vec2,
};
use dashscene_validator::{Location, Report, rule, validate_scene};

fn red() -> Color {
    Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

fn gradient(offsets: &[f32]) -> PaintKind {
    PaintKind::Gradient(Gradient {
        kind: GradientKind::Linear,
        handle_origin: Vec2 { x: 0.0, y: 0.0 },
        handle_primary: Vec2 { x: 1.0, y: 0.0 },
        handle_secondary: Vec2 { x: 0.0, y: 1.0 },
        stops: offsets
            .iter()
            .map(|&offset| GradientStop {
                offset,
                color: red(),
            })
            .collect(),
    })
}

fn rect(w: f32, h: f32, paint: u32) -> RectEntry {
    RectEntry {
        x: 0.0,
        y: 0.0,
        w,
        h,
        paint: PaintIndex(paint),
    }
}

/// One rect, sized `w`x`h`, painted by `entry`.
fn check_one(w: f32, h: f32, entry: PaintEntry) -> Report {
    let mut paints = PaintTable::new();
    let index = paints.push(entry);
    validate_scene(&[rect(w, h, index.0)], &paints, &ImageTable::new())
}

#[test]
fn a_well_formed_scene_produces_no_diagnostics() {
    let report = check_one(100.0, 50.0, PaintEntry::solid(red()));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn an_inside_stroke_wider_than_the_box_is_named() {
    // Issue #100: `draw_stroke` insets by half the width per side, so the
    // stroked box is `w - width` by `h - width`. Above `min(w, h)` that
    // inverts and the stroke silently collapses rather than drawing.
    let report = check_one(
        100.0,
        20.0,
        PaintEntry {
            stroke: Some(Stroke {
                width: 24.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..PaintEntry::solid(red())
        },
    );
    assert!(report.has(rule::STROKE_EXCEEDS_BOX), "{report}");
    assert!(report.has_errors());
}

#[test]
fn an_inside_stroke_exactly_filling_the_box_is_allowed() {
    // The threshold is strict. At exactly `min(w, h)` the inset extent is
    // zero and the stroke covers the box completely — which is what a
    // stroke that wide should look like. Only *above* it does the geometry
    // invert, so this must not be a diagnostic.
    let report = check_one(
        100.0,
        20.0,
        PaintEntry {
            stroke: Some(Stroke {
                width: 20.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..PaintEntry::solid(red())
        },
    );
    assert!(!report.has(rule::STROKE_EXCEEDS_BOX), "{report}");
}

#[test]
fn a_wide_stroke_that_is_not_inside_aligned_is_allowed() {
    // Center and outside strokes expand outward; they never invert the
    // box, so a wide one is legal (it just overhangs).
    for align in [StrokeAlign::Center, StrokeAlign::Outside] {
        let report = check_one(
            100.0,
            20.0,
            PaintEntry {
                stroke: Some(Stroke {
                    width: 60.0,
                    align,
                    color: red(),
                }),
                ..PaintEntry::solid(red())
            },
        );
        assert!(
            !report.has(rule::STROKE_EXCEEDS_BOX),
            "{align:?} stroke must not trip the inside-stroke rule:\n{report}"
        );
    }
}

#[test]
fn a_gradient_with_no_stops_is_named() {
    let report = check_one(
        100.0,
        50.0,
        PaintEntry {
            fill: Some(gradient(&[])),
            ..PaintEntry::default()
        },
    );
    assert!(report.has(rule::GRADIENT_NO_STOPS), "{report}");
}

#[test]
fn a_gradient_over_the_stop_budget_is_named() {
    let offsets: Vec<f32> = (0..=dashscene_validator::MAX_GRADIENT_STOPS)
        .map(|i| i as f32 / dashscene_validator::MAX_GRADIENT_STOPS as f32)
        .collect();
    let report = check_one(
        100.0,
        50.0,
        PaintEntry {
            fill: Some(gradient(&offsets)),
            ..PaintEntry::default()
        },
    );
    assert!(report.has(rule::GRADIENT_STOP_BUDGET), "{report}");
}

#[test]
fn a_negative_stroke_width_is_named() {
    let report = check_one(
        100.0,
        50.0,
        PaintEntry {
            stroke: Some(Stroke {
                width: -1.0,
                align: StrokeAlign::Center,
                color: red(),
            }),
            ..PaintEntry::default()
        },
    );
    assert!(report.has(rule::STROKE_INVALID_WIDTH), "{report}");
}

#[test]
fn an_image_fill_past_the_image_table_is_named() {
    // `ImageTable::resolve` panics on a miss, documented as "validated
    // upstream (P4)". This gate is that upstream.
    let mut paints = PaintTable::new();
    let index = paints.push(PaintEntry {
        fill: Some(PaintKind::Image {
            image: 3,
            scale_mode: ScaleMode::Fill,
            transform: None,
            tile_scale: 1.0,
        }),
        ..PaintEntry::default()
    });
    let mut images = ImageTable::new();
    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: vec![0],
    });

    let report = validate_scene(&[rect(10.0, 10.0, index.0)], &paints, &images);
    assert!(report.has(rule::IMAGE_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_rect_pointing_past_the_paint_table_is_named() {
    // `PaintTable::resolve` panics on a miss, also documented as
    // "validated upstream (P4)".
    let mut paints = PaintTable::new();
    paints.push(PaintEntry::solid(red()));

    let report = validate_scene(&[rect(10.0, 10.0, 5)], &paints, &ImageTable::new());
    assert!(report.has(rule::PAINT_ENTRY_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_shared_paint_entry_is_reported_once_not_once_per_rect() {
    // A pool entry is shared by every rect that references it. Reporting
    // it per referencing rect would repeat one authoring mistake N times
    // and bury the rest of the report.
    let mut paints = PaintTable::new();
    let index = paints.push(PaintEntry {
        fill: Some(gradient(&[])),
        ..PaintEntry::default()
    });
    let rects: Vec<RectEntry> = (0..5).map(|_| rect(10.0, 10.0, index.0)).collect();

    let report = validate_scene(&rects, &paints, &ImageTable::new());
    let count = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule::GRADIENT_NO_STOPS)
        .count();
    assert_eq!(count, 1, "one broken pool entry, one diagnostic:\n{report}");
}

#[test]
fn a_pool_diagnostic_points_at_the_pool_entry_not_a_rect() {
    // The pooled entry's index is a POOL index, not a rect index. Both are
    // small integers, so reporting one as the other resolves to an unrelated
    // rect without any type error to catch it.
    let mut paints = PaintTable::new();
    paints.push(PaintEntry::solid(red()));
    let broken = paints.push(PaintEntry {
        fill: Some(gradient(&[])),
        ..PaintEntry::default()
    });

    let report = validate_scene(&[rect(10.0, 10.0, broken.0)], &paints, &ImageTable::new());
    let diagnostic = report
        .find(rule::GRADIENT_NO_STOPS)
        .expect("the empty gradient is reported");
    assert_eq!(diagnostic.at, Location::PaintEntry(1));
}

#[test]
fn an_image_asset_with_no_bytes_is_named() {
    // ImageTable::resolve hands the asset to the painter, which decodes it
    // behind `.expect("image asset decodes (validated upstream, P4)")`.
    let mut paints = PaintTable::new();
    let index = paints.push(PaintEntry {
        fill: Some(PaintKind::Image {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: None,
            tile_scale: 1.0,
        }),
        ..PaintEntry::default()
    });
    let mut images = ImageTable::new();
    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: Vec::new(),
    });

    let report = validate_scene(&[rect(10.0, 10.0, index.0)], &paints, &images);
    assert!(report.has(rule::IMAGE_NO_BYTES), "{report}");
    assert_eq!(
        report.find(rule::IMAGE_NO_BYTES).unwrap().at,
        Location::ImageAsset(0)
    );
}

#[test]
fn gradient_stops_that_run_backwards_are_named() {
    let report = check_one(
        100.0,
        50.0,
        PaintEntry {
            fill: Some(gradient(&[0.0, 0.8, 0.3, 1.0])),
            ..PaintEntry::default()
        },
    );
    assert!(report.has(rule::GRADIENT_STOP_ORDER), "{report}");
}

#[test]
fn a_non_finite_box_does_not_silently_pass_the_stroke_rule() {
    // `f32::min` returns the non-NaN operand, so a NaN extent would compare
    // against the other axis; a fully-NaN box would pass every comparison.
    // The rule must not claim a NaN box is fine — it declines to judge it.
    let report = check_one(
        f32::NAN,
        f32::NAN,
        PaintEntry {
            stroke: Some(Stroke {
                width: 24.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..PaintEntry::solid(red())
        },
    );
    assert!(!report.has(rule::STROKE_EXCEEDS_BOX), "{report}");
}
