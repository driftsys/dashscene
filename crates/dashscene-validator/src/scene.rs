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
    ClipTable, ImageTable, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry, StrokeAlign,
};

use crate::paint::{
    check_gradient_stops, check_image_bytes, check_image_index, check_stroke_width, error,
};
use crate::{Location, Report, rule};

/// Validates boundary-B input: the paint pool's vocabulary, the image
/// assets, the resolved clip regions, and the geometry budgets that need
/// each rect's solved box.
pub fn validate_scene(
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    clips: &ClipTable,
) -> Report {
    let mut report = Report::default();

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

fn check_paint_entry(report: &mut Report, entry: &PaintEntry, at: &Location, image_count: usize) {
    match &entry.fill {
        None | Some(PaintKind::Solid { .. }) => {}
        Some(PaintKind::Gradient(gradient)) => {
            let offsets: Vec<f32> = gradient.stops.iter().map(|s| s.offset).collect();
            check_gradient_stops(report, at, &offsets);
        }
        Some(PaintKind::Image { image, .. }) => {
            check_image_index(report, at, *image, image_count);
        }
    }

    if let Some(stroke) = &entry.stroke {
        check_stroke_width(report, at, stroke.width);
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
