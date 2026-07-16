//! Greedy line breaking + baseline positioning (docs/design/architecture.md).
//! Break opportunities: after runs of ASCII space, and at `'\n'` (the
//! caller splits paragraphs). Breaking walks the logical order; each
//! line's level runs then reorder for display (UAX #9 L2) at
//! positioning time. A word wider than the maximum width overflows
//! its line — mid-word breaking and UAX #14 line breaking are out of
//! scope for v0.5/v0.6.

use std::ops::Range;

use unicode_bidi::{BidiInfo, ParagraphInfo};

use super::shape::{ShapedGlyph, ShapedText};
use super::{Line, PositionedGlyph};

/// Splits one bidi paragraph's glyph index range into lines of glyph
/// ranges (indices into the whole shaped chunk). An empty range
/// produces one empty line.
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
    para_glyphs: Range<usize>,
    scale: f32,
    max_width: Option<f32>,
) -> Vec<Range<usize>> {
    let max_width = max_width.unwrap_or(f32::INFINITY);
    let glyphs = &shaped.glyphs;
    // Clusters are byte indices of char starts, non-decreasing (the
    // shaping layer stores glyphs in logical order), so a glyph is a
    // space glyph exactly when its source byte is one. Breaking in
    // logical order is what UAX #9 prescribes; the display reorder is
    // [`position_line`]'s.
    let bytes = text.as_bytes();
    let is_space = |gi: usize| bytes[glyphs[gi].cluster as usize] == b' ';

    let mut lines: Vec<Range<usize>> = Vec::new();
    let mut start = para_glyphs.start; // current line's first glyph
    let mut end = para_glyphs.start; // current line's end, trailing spaces trimmed
    let mut width = 0f32; // fold over glyphs[start..cursor]
    let mut i = para_glyphs.start;
    while i < para_glyphs.end {
        let run_start = i;
        let space = is_space(i);
        while i < para_glyphs.end && is_space(i) == space {
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

/// Positions one line's glyphs on its baseline, in display order:
/// unicode-bidi reorders the line's level runs (UAX #9 L1+L2), an RTL
/// run's glyphs re-reverse from their stored logical order back to
/// visual order, and the pen advances left-to-right over the result.
/// The line lies within `para` by construction — the caller produces
/// lines per bidi paragraph.
pub(crate) fn position_line(
    bidi: &BidiInfo<'_>,
    para: &ParagraphInfo,
    shaped: &ShapedText,
    range: Range<usize>,
    scale: f32,
    baseline_y: f32,
) -> Line {
    if range.is_empty() {
        return Line {
            glyphs: Vec::new(),
            width: 0.0,
            baseline_y,
        };
    }
    // The line's byte range, wrap-trimmed spaces excluded: from the
    // first glyph's byte to the first glyph after the line (the first
    // trimmed space when the breaker trimmed), clamped to the
    // paragraph so a glyphless separator can never pull bytes of the
    // next paragraph in.
    let line_start = shaped.glyphs[range.start].cluster as usize;
    let line_end = shaped
        .glyphs
        .get(range.end)
        .map_or(bidi.text.len(), |g| g.cluster as usize)
        .min(para.range.end);
    let (levels, visual) = bidi.visual_runs(para, line_start..line_end);
    let line_glyphs = &shaped.glyphs[range];
    let mut glyphs = Vec::with_capacity(line_glyphs.len());
    let mut pen_x = 0f32;
    for run in visual {
        // Clusters are non-decreasing in the logical storage, so a
        // run's byte range maps to one contiguous glyph slice.
        let s = line_glyphs.partition_point(|g| (g.cluster as usize) < run.start);
        let e = line_glyphs.partition_point(|g| (g.cluster as usize) < run.end);
        let run_glyphs = &line_glyphs[s..e];
        if levels[run.start].is_rtl() {
            for g in run_glyphs.iter().rev() {
                pen_x = place(&mut glyphs, g, pen_x, scale, baseline_y);
            }
        } else {
            for g in run_glyphs {
                pen_x = place(&mut glyphs, g, pen_x, scale, baseline_y);
            }
        }
    }
    Line {
        glyphs,
        width: pen_x,
        baseline_y,
    }
}

/// Appends one glyph at the pen position and returns the advanced
/// pen. HarfBuzz offsets are y-up; document space is y-down, so the
/// y offset is subtracted.
fn place(
    out: &mut Vec<PositionedGlyph>,
    g: &ShapedGlyph,
    pen_x: f32,
    scale: f32,
    baseline_y: f32,
) -> f32 {
    out.push(PositionedGlyph {
        glyph_id: g.glyph_id,
        x: pen_x + g.x_offset as f32 * scale,
        y: baseline_y - g.y_offset as f32 * scale,
    });
    pen_x + g.x_advance as f32 * scale
}
