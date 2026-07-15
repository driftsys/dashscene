//! Greedy line breaking + baseline positioning (docs/design/architecture.md,
//! Latin subset). Break opportunities: after runs of ASCII space, and
//! at `'\n'` (the caller splits paragraphs). A word wider than the
//! maximum width overflows its line — mid-word breaking and UAX #14
//! line breaking are out of scope for v0.5.

use std::ops::Range;

use super::shape::ShapedText;
use super::{Line, PositionedGlyph};

/// Splits one shaped paragraph into lines of glyph ranges.
///
/// Space handling: leading authored spaces stay on their line;
/// mid-line spaces separate words and keep their glyphs; trailing
/// spaces (before a wrap or at the paragraph end) are on no line and
/// count toward no width — identically with and without a maximum
/// width, so a measure pass (`None`) and a constrained layout pass
/// agree on the glyph set.
///
/// Width bookkeeping extends one left-to-right fold, one multiply-add
/// per glyph — the same operation order [`position_line`] uses — so a
/// line accepted by the fit check can never measure wider than the
/// maximum through float re-association.
pub(crate) fn break_lines(
    text: &str,
    shaped: &ShapedText,
    scale: f32,
    max_width: Option<f32>,
) -> Vec<Range<usize>> {
    let max_width = max_width.unwrap_or(f32::INFINITY);
    let glyphs = &shaped.glyphs;
    // Clusters are byte indices of char starts (LTR, forced in
    // shaping), so a glyph is a space glyph exactly when its source
    // byte is one.
    let bytes = text.as_bytes();
    let is_space = |gi: usize| bytes[glyphs[gi].cluster as usize] == b' ';

    let mut lines: Vec<Range<usize>> = Vec::new();
    let mut start = 0usize; // current line's first glyph
    let mut end = 0usize; // current line's end, trailing spaces trimmed
    let mut width = 0f32; // fold over glyphs[start..cursor]
    let mut i = 0usize;
    while i < glyphs.len() {
        let run_start = i;
        let space = is_space(i);
        while i < glyphs.len() && is_space(i) == space {
            i += 1;
        }
        if space {
            // Spaces extend the width fold but never `end`: a run that
            // turns out to be trailing is dropped by construction.
            for g in &glyphs[run_start..i] {
                width += g.x_advance as f32 * scale;
            }
        } else {
            let mut prospective = width;
            for g in &glyphs[run_start..i] {
                prospective += g.x_advance as f32 * scale;
            }
            if end == start || prospective <= max_width {
                // The first word of a line is always placed, overflow
                // included; leading authored spaces are part of that
                // first placement since `start` never moves past them.
                end = i;
                width = prospective;
            } else {
                lines.push(start..end);
                start = run_start;
                end = i;
                width = 0.0;
                for g in &glyphs[run_start..i] {
                    width += g.x_advance as f32 * scale;
                }
            }
        }
    }
    lines.push(start..end);
    lines
}

/// Positions one line's glyphs on its baseline. HarfBuzz offsets are
/// y-up; document space is y-down, so the y offset is subtracted.
pub(crate) fn position_line(
    shaped: &ShapedText,
    range: Range<usize>,
    scale: f32,
    baseline_y: f32,
) -> Line {
    let mut glyphs = Vec::with_capacity(range.len());
    let mut pen_x = 0f32;
    for g in &shaped.glyphs[range] {
        glyphs.push(PositionedGlyph {
            glyph_id: g.glyph_id,
            x: pen_x + g.x_offset as f32 * scale,
            y: baseline_y - g.y_offset as f32 * scale,
        });
        pen_x += g.x_advance as f32 * scale;
    }
    Line {
        glyphs,
        width: pen_x,
        baseline_y,
    }
}
