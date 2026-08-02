//! The paint gate (story #15): the budgets that need a *solved* box, and
//! the boundary-B index resolution the painter would otherwise panic on.
//!
//! These rules are unreachable at the load gate: P1 says the document
//! carries intent, never results, so a `Hug`/`Fill` node has no box for a
//! stroke width to be measured against (issue #100).

use dashpaint::{
    ClipBox, ClipIndex, ClipTable, Color, CornerRadii, EntryParts, FillSpec, GlyphRunTable,
    Gradient, GradientKind, GradientStop, GroupComposite, ImageAsset, ImageFill, ImageFormat,
    ImageTable, Mat23, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry, ScaleMode, Shadow,
    ShadowKind, StopRange, Stroke, StrokeAlign, Vec2,
};
use dashscene_validator::{
    Location, RENDER_TARGET_BUDGET_PLACEHOLDER, Report, Severity, rule, validate_scene,
};

fn red() -> Color {
    Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

/// A solid-red [`PaintEntry`], interned into `paints` — the test suite's
/// replacement for the `PaintEntry::solid` shorthand story #578 removed: a
/// solid fill is now a [`PaintTable`] row, so building one takes the table
/// it will live in.
fn solid_entry(paints: &mut PaintTable, color: Color) -> PaintEntry {
    PaintEntry {
        fill: paints.intern_fill(&FillSpec::Solid { color }),
        ..PaintEntry::default()
    }
}

/// Interns a linear gradient carrying `offsets` as its stops (all red) and
/// returns the [`PaintKind`] naming it. `offsets` may be empty or over
/// [`MAX_GRADIENT_STOPS`](dashscene_validator::MAX_GRADIENT_STOPS) —
/// [`PaintTable::intern_fill`] does not check the count (that is
/// `dashscene-validator`'s rule, per P4), so a bad `offsets` interns just as
/// cleanly as a good one and the scene gate is what names it.
fn gradient(paints: &mut PaintTable, offsets: &[f32]) -> PaintKind {
    paints.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Linear,
            handle_origin: Vec2 { x: 0.0, y: 0.0 },
            handle_primary: Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: Vec2 { x: 0.0, y: 1.0 },
            stops: StopRange::NONE,
        },
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
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
    }
}

/// One rect, sized `w`x`h`, painted by the entry `build` constructs against
/// its own fresh table.
fn check_one(w: f32, h: f32, build: impl FnOnce(&mut PaintTable) -> PaintEntry) -> Report {
    let mut paints = PaintTable::new();
    let entry = build(&mut paints);
    let index = paints.push(entry);
    validate_scene(
        &[rect(w, h, index.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    )
}

/// [`check_one`]'s counterpart for an entry that also carries parts living in
/// the table's flat arrays — a stroke, stacked fill layers, effects, or a
/// coverage mask (story #578 moved these off `PaintEntry` and into
/// [`PaintTable::push_with`]). `paints` and `entry` come in already built
/// (typically `entry` from [`solid_entry`]) because assembling them can
/// itself need to intern a fill against the same table.
fn check_one_with(
    w: f32,
    h: f32,
    mut paints: PaintTable,
    entry: PaintEntry,
    parts: EntryParts<'_>,
) -> Report {
    let index = paints.push_with(entry, parts);
    validate_scene(
        &[rect(w, h, index.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    )
}

#[test]
fn a_well_formed_scene_produces_no_diagnostics() {
    let report = check_one(100.0, 50.0, |paints| solid_entry(paints, red()));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn an_inside_stroke_wider_than_the_box_is_named() {
    // Issue #100: `draw_stroke` insets by half the width per side, so the
    // stroked box is `w - width` by `h - width`. Above `min(w, h)` that
    // inverts and the stroke silently collapses rather than drawing.
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let report = check_one_with(
        100.0,
        20.0,
        paints,
        entry,
        EntryParts {
            stroke: Some(Stroke {
                width: 24.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..EntryParts::default()
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
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let report = check_one_with(
        100.0,
        20.0,
        paints,
        entry,
        EntryParts {
            stroke: Some(Stroke {
                width: 20.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..EntryParts::default()
        },
    );
    assert!(!report.has(rule::STROKE_EXCEEDS_BOX), "{report}");
}

#[test]
fn a_wide_stroke_that_is_not_inside_aligned_is_allowed() {
    // Center and outside strokes expand outward; they never invert the
    // box, so a wide one is legal (it just overhangs).
    for align in [StrokeAlign::Center, StrokeAlign::Outside] {
        let mut paints = PaintTable::new();
        let entry = solid_entry(&mut paints, red());
        let report = check_one_with(
            100.0,
            20.0,
            paints,
            entry,
            EntryParts {
                stroke: Some(Stroke {
                    width: 60.0,
                    align,
                    color: red(),
                }),
                ..EntryParts::default()
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
    let report = check_one(100.0, 50.0, |paints| {
        let fill = gradient(paints, &[]);
        PaintEntry {
            fill,
            ..PaintEntry::default()
        }
    });
    assert!(report.has(rule::GRADIENT_NO_STOPS), "{report}");
}

#[test]
fn a_gradient_with_no_stops_inside_a_stacked_fill_is_named() {
    // Story C1 (debt #146): a stacked layer's own vocabulary rules apply
    // exactly as the primary fill's on the resolved paint entry, same as at
    // the load gate.
    let mut paints = PaintTable::new();
    let extra = gradient(&mut paints, &[]);
    let entry = PaintEntry {
        fill: paints.intern_fill(&FillSpec::Solid { color: red() }),
        ..PaintEntry::default()
    };
    let report = check_one_with(
        100.0,
        50.0,
        paints,
        entry,
        EntryParts {
            extra_fills: &[extra],
            ..EntryParts::default()
        },
    );
    assert!(report.has(rule::GRADIENT_NO_STOPS), "{report}");
}

#[test]
fn a_gradient_over_the_stop_budget_is_named() {
    let offsets: Vec<f32> = (0..=dashscene_validator::MAX_GRADIENT_STOPS)
        .map(|i| i as f32 / dashscene_validator::MAX_GRADIENT_STOPS as f32)
        .collect();
    let report = check_one(100.0, 50.0, |paints| {
        let fill = gradient(paints, &offsets);
        PaintEntry {
            fill,
            ..PaintEntry::default()
        }
    });
    assert!(report.has(rule::GRADIENT_STOP_BUDGET), "{report}");
}

#[test]
fn a_negative_stroke_width_is_named() {
    let report = check_one_with(
        100.0,
        50.0,
        PaintTable::new(),
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: -1.0,
                align: StrokeAlign::Center,
                color: red(),
            }),
            ..EntryParts::default()
        },
    );
    assert!(report.has(rule::STROKE_INVALID_WIDTH), "{report}");
}

#[test]
fn an_image_fill_past_the_image_table_is_named() {
    // `ImageTable::resolve` panics on a miss, documented as "validated
    // upstream (P4)". This gate is that upstream.
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: 3,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let index = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let mut images = ImageTable::new();
    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: vec![0],
    });

    let report = validate_scene(
        &[rect(10.0, 10.0, index.0)],
        &paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
    assert!(report.has(rule::IMAGE_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_rect_pointing_past_the_paint_table_is_named() {
    // `PaintTable::resolve` panics on a miss, also documented as
    // "validated upstream (P4)".
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    paints.push(entry);

    let report = validate_scene(
        &[rect(10.0, 10.0, 5)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
    assert!(report.has(rule::PAINT_ENTRY_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_shared_paint_entry_is_reported_once_not_once_per_rect() {
    // A pool entry is shared by every rect that references it. Reporting
    // it per referencing rect would repeat one authoring mistake N times
    // and bury the rest of the report.
    let mut paints = PaintTable::new();
    let fill = gradient(&mut paints, &[]);
    let index = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let rects: Vec<RectEntry> = (0..5).map(|_| rect(10.0, 10.0, index.0)).collect();

    let report = validate_scene(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
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
    let first = solid_entry(&mut paints, red());
    paints.push(first);
    let fill = gradient(&mut paints, &[]);
    let broken = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });

    let report = validate_scene(
        &[rect(10.0, 10.0, broken.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
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
    let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: 0,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let index = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let mut images = ImageTable::new();
    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: Vec::new(),
    });

    let report = validate_scene(
        &[rect(10.0, 10.0, index.0)],
        &paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
    assert!(report.has(rule::IMAGE_NO_BYTES), "{report}");
    assert_eq!(
        report.find(rule::IMAGE_NO_BYTES).unwrap().at,
        Location::ImageAsset(0)
    );
}

#[test]
fn gradient_stops_that_run_backwards_are_named() {
    let report = check_one(100.0, 50.0, |paints| {
        let fill = gradient(paints, &[0.0, 0.8, 0.3, 1.0]);
        PaintEntry {
            fill,
            ..PaintEntry::default()
        }
    });
    assert!(report.has(rule::GRADIENT_STOP_ORDER), "{report}");
}

#[test]
fn a_non_finite_box_does_not_silently_pass_the_stroke_rule() {
    // `f32::min` returns the non-NaN operand, so a NaN extent would compare
    // against the other axis; a fully-NaN box would pass every comparison.
    // The rule must not claim a NaN box is fine — it declines to judge it.
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let report = check_one_with(
        f32::NAN,
        f32::NAN,
        paints,
        entry,
        EntryParts {
            stroke: Some(Stroke {
                width: 24.0,
                align: StrokeAlign::Inside,
                color: red(),
            }),
            ..EntryParts::default()
        },
    );
    assert!(!report.has(rule::STROKE_EXCEEDS_BOX), "{report}");
    // The stroke rule declines to judge the NaN box; #128's extent rule is
    // what names the real fault so it is not discovered downstream (P4).
    assert!(report.has(rule::RECT_INVALID_EXTENT), "{report}");
}

#[test]
fn a_non_finite_rect_extent_is_named() {
    // Issue #128: rects come from the solver, so a NaN/infinite extent is a
    // broken inter-crate contract — but the paint gate is the last checkpoint
    // before a painter rasterizes NaN geometry, so it names it.
    for (w, h) in [(f32::NAN, 50.0), (f32::INFINITY, 50.0), (100.0, f32::NAN)] {
        let report = check_one(w, h, |paints| solid_entry(paints, red()));
        assert!(
            report.has(rule::RECT_INVALID_EXTENT),
            "{w}x{h} extent must be named:\n{report}"
        );
        assert!(report.has_errors());
    }
}

#[test]
fn a_negative_rect_extent_is_named() {
    let report = check_one(-10.0, 50.0, |paints| solid_entry(paints, red()));
    assert!(report.has(rule::RECT_INVALID_EXTENT), "{report}");
}

#[test]
fn a_negative_scene_corner_radius_is_named() {
    // A PaintEntry's corners feed both its own rounding and — when the node
    // clips — every ClipBox of its subtree (issue #128). A negative radius
    // is not clamped by the painter, so it is caught at its own pool entry.
    let report = check_one(100.0, 50.0, |paints| PaintEntry {
        corners: CornerRadii {
            top_left: -1.0,
            ..CornerRadii::default()
        },
        ..solid_entry(paints, red())
    });
    assert!(report.has(rule::CORNER_RADIUS_INVALID), "{report}");
    assert_eq!(
        report.find(rule::CORNER_RADIUS_INVALID).unwrap().at,
        Location::PaintEntry(0),
    );
}

#[test]
fn a_rect_pointing_past_the_clip_table_is_named() {
    // Story #97 gave every rect a resolved clip region, and
    // `ClipTable::resolve` panics on a miss with the same "validated
    // upstream (P4)" note as the paint and image tables. This gate is that
    // upstream — without it, #97 would ship a panic with nothing standing in
    // front of it.
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let paint = paints.push(entry);

    let mut broken = rect(10.0, 10.0, paint.0);
    broken.clip = ClipIndex(7);

    let report = validate_scene(
        &[broken],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
    assert!(report.has(rule::CLIP_INDEX_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_rect_carrying_a_real_clip_region_is_clean() {
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let paint = paints.push(entry);

    let mut clips = ClipTable::new();
    let region = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
        corners: CornerRadii::default(),
    }]);

    let mut clipped = rect(10.0, 10.0, paint.0);
    clipped.clip = region;

    let report = validate_scene(
        &[clipped],
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
    );
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn the_reserved_unclipped_region_always_resolves() {
    // ClipTable::new() reserves index 0 as the unclipped region, so a rect
    // that clips nothing needs no sentinel and must never trip the rule.
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let paint = paints.push(entry);

    let report = validate_scene(
        &[rect(10.0, 10.0, paint.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
    );
    assert!(!report.has(rule::CLIP_INDEX_OUT_OF_RANGE), "{report}");
}

// ---------------------------------------------------------------------
// The render-target group-opacity budget (story #44): a scene-wide
// warning against the Q-6 placeholder, never an error.
// (`docs/decisions/masks-and-group-opacity.md`.)
// ---------------------------------------------------------------------

fn group(start: u32) -> GroupComposite {
    GroupComposite {
        start,
        end: start + 1,
        alpha: 0.5,
    }
}

#[test]
fn render_target_groups_within_the_budget_do_not_warn() {
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let paint = paints.push(entry);
    let groups: Vec<GroupComposite> = (0..RENDER_TARGET_BUDGET_PLACEHOLDER as u32)
        .map(group)
        .collect();

    let report = validate_scene(
        &[rect(10.0, 10.0, paint.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &groups,
        &GlyphRunTable::new(),
    );
    assert!(!report.has(rule::RENDER_TARGET_BUDGET), "{report}");
}

#[test]
fn too_many_render_target_groups_warn_but_never_error() {
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    let paint = paints.push(entry);
    let groups: Vec<GroupComposite> = (0..RENDER_TARGET_BUDGET_PLACEHOLDER as u32 + 1)
        .map(group)
        .collect();

    let report = validate_scene(
        &[rect(10.0, 10.0, paint.0)],
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &groups,
        &GlyphRunTable::new(),
    );
    assert!(report.has(rule::RENDER_TARGET_BUDGET), "{report}");
    // A placeholder budget must not hard-fail a build.
    assert!(
        !report.has_errors(),
        "the budget is a warning, not an error"
    );
    assert!(
        report
            .diagnostics()
            .iter()
            .filter(|d| d.rule == rule::RENDER_TARGET_BUDGET)
            .all(|d| d.severity == Severity::Warning)
    );
}

/// [`check_one`] over a plain red fill carrying one shadow.
///
/// A helper rather than a `PaintEntry` builder because `PaintEntry::shadows`
/// is a range into a `PaintTable`'s flat array (story #578): the entry and
/// its shadow have to reach the table together, through
/// `PaintTable::push_with`, and only the table can pair them.
fn check_shadowed(w: f32, h: f32, shadow: Shadow) -> Report {
    let mut paints = PaintTable::new();
    let entry = solid_entry(&mut paints, red());
    check_one_with(
        w,
        h,
        paints,
        entry,
        EntryParts {
            shadows: &[shadow],
            ..EntryParts::default()
        },
    )
}

fn drop_shadow() -> Shadow {
    Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 4.0 },
        blur: 8.0,
        spread: 1.0,
        color: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.25,
        },
    }
}

#[test]
fn a_well_formed_shadow_produces_no_diagnostics() {
    let report = check_shadowed(100.0, 50.0, drop_shadow());
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_negative_blur_radius_is_named() {
    let report = check_shadowed(
        100.0,
        50.0,
        Shadow {
            blur: -4.0,
            ..drop_shadow()
        },
    );
    assert!(report.has(rule::SHADOW_INVALID_GEOMETRY), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_non_finite_offset_is_named() {
    let report = check_shadowed(
        100.0,
        50.0,
        Shadow {
            offset: Vec2 {
                x: f32::NAN,
                y: 0.0,
            },
            ..drop_shadow()
        },
    );
    assert!(report.has(rule::SHADOW_INVALID_GEOMETRY), "{report}");
}

#[test]
fn a_shadow_color_channel_out_of_range_is_named() {
    let report = check_shadowed(
        100.0,
        50.0,
        Shadow {
            color: Color {
                r: 1.5,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            ..drop_shadow()
        },
    );
    assert!(report.has(rule::SHADOW_COLOR_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_negative_spread_is_allowed() {
    // CSS/Figma spread may be negative (it shrinks the shadow); only the
    // blur radius is required non-negative.
    let report = check_shadowed(
        100.0,
        50.0,
        Shadow {
            spread: -3.0,
            ..drop_shadow()
        },
    );
    assert!(
        !report.has(rule::SHADOW_INVALID_GEOMETRY),
        "a negative spread is legal:\n{report}"
    );
}
