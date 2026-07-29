//! The paint gate: does this solved scene stay inside painter budgets?
//!
//! Boundary B's input, checked just before a painter reads it. Two things
//! live only here.
//!
//! **Geometry budgets.** Issue #100's over-wide inside stroke needs the
//! node's *resolved* box, which by P1 exists nowhere in a document — a
//! `Hug` or `Fill` node has no authored size at all. So this rule is
//! unreachable at the load gate and belongs on the solved scene.
//!
//! **Index resolution against the runtime tables.** `PaintTable::resolve`,
//! `ImageTable::resolve` and `ClipTable::resolve` all panic on a miss, each
//! documented as "validated upstream (P4)". This gate is that upstream: it
//! turns the panic into a named diagnostic for a producer that builds
//! boundary-B input by hand (which is how the painter tests and, until the
//! document→arena loader lands, every non-`.dsb` producer builds a scene).
//!
//! The resolved clip regions (issue #97) exist *only* here. A document
//! carries clip **intent** — `Paint.clip`, a bool — while the region a
//! painter consumes is the ancestor-intersected result `dashscene-core`
//! computes at commit. By P1 a result never appears in a document, so the
//! load gate has nothing to check and this gate has to.

use dashpaint::{
    ClipTable, GlyphRunTable, GroupComposite, ImageTable, PaintEntry, PaintIndex, PaintKind,
    PaintTable, RectEntry, StrokeAlign,
};

use crate::paint::{
    check_corners, check_gradient_stops, check_image_bytes, check_image_index, check_shadow,
    check_stroke_width, error, warning,
};
use crate::{Location, NodePath, RENDER_TARGET_BUDGET_PLACEHOLDER, Report, rule};

/// Validates boundary-B input: the paint pool's vocabulary, the image
/// assets, the resolved clip regions, the render-target group-opacity
/// budget, and the geometry budgets that need each rect's solved box.
///
/// `glyphs` carries no rule today. It had one — `paint.text-outside-group`,
/// which named the combination of runs and render-target groups as a
/// limitation — and that was retired when the painter began compositing
/// runs into group layers (issue #274). The parameter stays because this
/// gate's contract is "validate a boundary-B scene", which is all five
/// tables, not the subset a rule happens to read this slice.
pub fn validate_scene(
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    clips: &ClipTable,
    groups: &[GroupComposite],
    glyphs: &GlyphRunTable,
) -> Report {
    let _ = glyphs;
    let mut report = Report::default();

    // The render-target group-opacity budget (story #44). Each overlapping
    // group opacity is one offscreen composite; too many strain the
    // mid-frame render-target switch R-T1. The budget value is the Q-6
    // placeholder, so this warns rather than errors — the count is a real
    // contract even while the ceiling is unmeasured.
    if groups.len() > RENDER_TARGET_BUDGET_PLACEHOLDER {
        report.push(warning(
            rule::RENDER_TARGET_BUDGET,
            &Location::Node(NodePath::unnamed(0)),
            format!(
                "scene uses {} render-target group composites, over the placeholder budget of {} \
                 (Q-6, unmeasured)",
                groups.len(),
                RENDER_TARGET_BUDGET_PLACEHOLDER
            ),
        ));
    }

    // A pool entry and an image asset are each shared by every rect that
    // references them, so each is checked once, at its own index. Reporting
    // per referencing rect would repeat one authoring mistake N times.
    for i in 0..paints.len() {
        let index = i as u32;
        let entry = paints
            .get(PaintIndex(index))
            .expect("an index below len() resolves");
        check_paint_entry(
            &mut report,
            entry,
            &Location::PaintEntry(index),
            images.len(),
        );
    }

    for i in 0..images.len() {
        let index = i as u32;
        let asset = images.get(index).expect("an index below len() resolves");
        check_image_bytes(&mut report, &Location::ImageAsset(index), asset.bytes.len());
    }

    // The geometry rules, and the two per-rect table links the painter would
    // panic on.
    for (i, rect) in rects.iter().enumerate() {
        let at = Location::Node(crate::NodePath::unnamed(i as u32));

        check_rect_extent(&mut report, rect, &at);

        if clips.get(rect.clip).is_none() {
            report.push(error(
                rule::CLIP_INDEX_OUT_OF_RANGE,
                &at,
                format!(
                    "rect references clip region {}, but the clip table holds {} regions",
                    rect.clip.0,
                    clips.len()
                ),
            ));
        }

        let Some(entry) = paints.get(rect.paint) else {
            report.push(error(
                rule::PAINT_ENTRY_OUT_OF_RANGE,
                &at,
                format!(
                    "rect references paint entry {}, but the paint table holds {} entries",
                    rect.paint.0,
                    paints.len()
                ),
            ));
            continue;
        };
        check_stroke_fits_box(&mut report, entry, rect, &at);
    }

    report
}

/// One resolved fill kind's vocabulary rules: a gradient's stops, an image
/// fill's asset index. Shared by the primary `PaintEntry.fill` and every
/// stacked layer in `PaintEntry.extra_fills` (story C1, debt #146) — a
/// layer is not exempt from the same rules just because it sits in a stack.
fn check_fill_kind(report: &mut Report, at: &Location, kind: &PaintKind, image_count: usize) {
    match kind {
        PaintKind::Solid { .. } => {}
        PaintKind::Gradient(gradient) => {
            let offsets: Vec<f32> = gradient.stops.iter().map(|s| s.offset).collect();
            check_gradient_stops(report, at, &offsets);
        }
        PaintKind::Image { image, .. } => {
            check_image_index(report, at, *image, image_count);
        }
    }
}

fn check_paint_entry(report: &mut Report, entry: &PaintEntry, at: &Location, image_count: usize) {
    if let Some(kind) = &entry.fill {
        check_fill_kind(report, at, kind, image_count);
    }
    // Stacked fills (story C1, debt #146): each layer's own vocabulary
    // rules, the same posture as the shadows loop below — one check per
    // layer, `at` naming the paint entry rather than the individual layer.
    for kind in &entry.extra_fills {
        check_fill_kind(report, at, kind, image_count);
    }

    if let Some(stroke) = &entry.stroke {
        check_stroke_width(report, at, stroke.width);
    }

    let c = entry.corners;
    check_corners(
        report,
        at,
        [c.top_left, c.top_right, c.bottom_right, c.bottom_left],
    );

    // v0.8 shadows (story #45): the same numeric domain as the load gate, on
    // the resolved paint entry.
    for (i, shadow) in entry.shadows.iter().enumerate() {
        check_shadow(
            report,
            at,
            i,
            [shadow.offset.x, shadow.offset.y],
            shadow.blur,
            shadow.spread,
            [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            ],
        );
    }
}

/// A rect's extents must be finite and non-negative (issue #128). Rects come
/// from the solver, so a non-finite or negative extent is a broken
/// inter-crate contract rather than authoring — but the paint gate is the
/// last checkpoint before a painter rasterizes NaN or inverted geometry.
///
/// This names what `check_stroke_fits_box` only declines to judge: that
/// function returns without a verdict on a non-finite box (`f32::min` would
/// otherwise silently compare against the other axis), leaving the real
/// fault — the non-finite extent itself — unnamed until now.
fn check_rect_extent(report: &mut Report, rect: &RectEntry, at: &Location) {
    for (extent, axis) in [(rect.w, "width"), (rect.h, "height")] {
        if !extent.is_finite() || extent < 0.0 {
            report.push(error(
                rule::RECT_INVALID_EXTENT,
                at,
                format!("rect {axis} is {extent}; it must be finite and non-negative"),
            ));
        }
    }
}

/// Issue #100: an `Inside` stroke insets the box by half the width per
/// side, so the stroked geometry is `w - width` by `h - width`. Above
/// `min(w, h)` that inverts and the stroke silently collapses instead of
/// drawing.
///
/// The threshold is strict. At exactly `min(w, h)` the inset extent is
/// zero and the stroke covers the box completely, which is what a stroke
/// that wide should look like — only *above* it does the geometry invert.
fn check_stroke_fits_box(report: &mut Report, entry: &PaintEntry, rect: &RectEntry, at: &Location) {
    let Some(stroke) = &entry.stroke else {
        return;
    };
    if stroke.align != StrokeAlign::Inside {
        return;
    }
    // `f32::min` returns the non-NaN operand, so a NaN extent would silently
    // compare against the other axis and a fully-NaN box would pass every
    // comparison. Rects come from the solver, so a non-finite extent is a
    // broken contract rather than authoring — but this gate is the last
    // checkpoint before a painter rasterizes it.
    if !rect.w.is_finite() || !rect.h.is_finite() {
        return;
    }
    let smaller_extent = rect.w.min(rect.h);
    if stroke.width > smaller_extent {
        report.push(error(
            rule::STROKE_EXCEEDS_BOX,
            at,
            format!(
                "inside stroke is {} wide on a {}x{} box; above the box's smaller extent ({}) \
                 the inset inverts and the stroke collapses instead of drawing",
                stroke.width, rect.w, rect.h, smaller_extent
            ),
        ));
    }
}
