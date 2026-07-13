//! Paint-vocabulary rules shared by the load gate and the paint gate.
//!
//! A document and a solved scene hold the same paint vocabulary in two
//! different types (`dashbuf`'s generated tables; `dashpaint`'s
//! `PaintEntry`), so these rules take the decomposed values rather than
//! one of the two types. The extraction differs per surface; the rule does
//! not.
//!
//! Each rule here stands in front of a panic or a misrender that
//! `dashscene-skia` documents as "validated upstream (P4)" — see issue
//! #100.

use crate::{Diagnostic, Location, MAX_GRADIENT_STOPS, Report, Severity, rule};

pub(crate) fn error(rule: &'static str, at: &Location, message: String) -> Diagnostic {
    Diagnostic {
        rule,
        severity: Severity::Error,
        at: at.clone(),
        message,
    }
}

/// Gradient stop rules. `offsets` is the stop offsets in declaration order.
///
/// The painter takes `stops.first().expect(..)`, asserts the count against
/// [`MAX_GRADIENT_STOPS`], and hands the offsets to Skia as a `positions`
/// array. `(required)` in the schema mandates presence, not non-emptiness,
/// so an empty vector is reachable from a well-formed buffer — the false
/// assurance issue #100 names.
pub(crate) fn check_gradient_stops(report: &mut Report, at: &Location, offsets: &[f32]) {
    if offsets.is_empty() {
        report.push(error(
            rule::GRADIENT_NO_STOPS,
            at,
            "gradient carries no color stops; the schema's (required) annotation mandates a \
             present stops vector, not a non-empty one"
                .to_owned(),
        ));
        return;
    }

    if offsets.len() > MAX_GRADIENT_STOPS {
        report.push(error(
            rule::GRADIENT_STOP_BUDGET,
            at,
            format!(
                "gradient carries {} color stops, over the budget of {MAX_GRADIENT_STOPS}",
                offsets.len()
            ),
        ));
    }

    for (i, &offset) in offsets.iter().enumerate() {
        if !offset.is_finite() || !(0.0..=1.0).contains(&offset) {
            report.push(error(
                rule::GRADIENT_STOP_OFFSET_INVALID,
                at,
                format!(
                    "gradient stop {i} has offset {offset}; offsets are normalized to 0..=1 \
                     along the gradient's primary axis"
                ),
            ));
        }
    }

    // Skia's gradient shaders take the offsets as a `positions` array, which
    // is specified to be monotonically increasing. Unordered stops are each
    // individually in range, so no rule above catches them, and the result
    // is backend-dependent rasterization — a silent misrender, and exactly
    // the painter-swap divergence the validator exists to prevent.
    if let Some(i) = offsets.windows(2).position(|w| w[0] > w[1]) {
        report.push(error(
            rule::GRADIENT_STOP_ORDER,
            at,
            format!(
                "gradient stop {} has offset {}, behind stop {}'s offset {}; stops are ordered \
                 along the gradient's primary axis, and painters take them as a monotonically \
                 increasing ramp",
                i + 1,
                offsets[i + 1],
                i,
                offsets[i]
            ),
        ));
    }
}

/// Stroke width must be a finite, non-negative number. Geometry-free, so it
/// holds on a document as much as on a solved scene.
pub(crate) fn check_stroke_width(report: &mut Report, at: &Location, width: f32) {
    if !width.is_finite() || width < 0.0 {
        report.push(error(
            rule::STROKE_INVALID_WIDTH,
            at,
            format!("stroke width is {width}; it must be finite and non-negative"),
        ));
    }
}

/// An image fill's asset index must resolve.
pub(crate) fn check_image_index(
    report: &mut Report,
    at: &Location,
    image: u32,
    image_count: usize,
) {
    if image as usize >= image_count {
        report.push(error(
            rule::IMAGE_OUT_OF_RANGE,
            at,
            format!(
                "image fill references asset {image}, but {image_count} image assets are \
                 available"
            ),
        ));
    }
}

/// An image asset must carry bytes to decode.
///
/// The painter calls `deferred_from_encoded_data(..).expect("image asset
/// decodes (validated upstream, P4)")`. A present-but-empty byte vector
/// reaches it and panics.
pub(crate) fn check_image_bytes(report: &mut Report, at: &Location, byte_count: usize) {
    if byte_count == 0 {
        report.push(error(
            rule::IMAGE_NO_BYTES,
            at,
            "image asset carries no bytes; a painter cannot decode it".to_owned(),
        ));
    }
}
