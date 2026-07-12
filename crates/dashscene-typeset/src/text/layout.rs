//! Greedy line breaking + baseline positioning (DESIGN_1.md §7.2,
//! Latin subset). Break opportunities: after runs of ASCII space, and
//! at `'\n'` (the caller splits paragraphs). A word wider than the
//! maximum width overflows its line — mid-word breaking and UAX #14
//! line breaking are not v0.5 problems.

use std::ops::Range;

use super::font::Font;
use super::shape::ShapedText;
use super::{Line, PositionedGlyph};

/// Splits one shaped paragraph into lines of glyph ranges. Spaces
/// that a wrap consumes appear on neither line and count toward
/// neither width; spaces inside a line keep their glyphs and advance.
pub(crate) fn break_lines(
    text: &str,
    shaped: &ShapedText,
    scale: f32,
    max_width: Option<f32>,
) -> Vec<Range<usize>> {
    let glyph_count = shaped.glyphs.len();
    let Some(max_width) = max_width else {
        // One line spanning every glyph (the lint-suggested
        // `(0..n).collect()` would build n ranges, not one).
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..glyph_count];
    };

    // Tokenize into alternating space/word glyph runs. Clusters are
    // byte indices into `text` (LTR, one run), so a glyph is a space
    // glyph exactly when its source byte is one.
    let bytes = text.as_bytes();
    let is_space = |gi: usize| bytes[shaped.glyphs[gi].cluster as usize] == b' ';
    let mut tokens: Vec<(bool, Range<usize>, f32)> = Vec::new();
    let mut i = 0;
    while i < glyph_count {
        let space = is_space(i);
        let start = i;
        let mut advance = 0f32;
        while i < glyph_count && is_space(i) == space {
            advance += shaped.glyphs[i].x_advance as f32 * scale;
            i += 1;
        }
        tokens.push((space, start..i, advance));
    }

    let mut lines: Vec<Range<usize>> = Vec::new();
    let mut start = 0usize; // current line's first glyph
    let mut end = 0usize; // current line's end, trailing spaces trimmed
    let mut full_width = 0f32; // width including any trailing spaces

    for (space, range, advance) in tokens {
        if space {
            // Space runs extend the width (they separate words on the
            // line) but never the trimmed end — a run that turns out
            // to be trailing (before a wrap or at paragraph end) is
            // dropped with zero width by construction.
            full_width += advance;
            continue;
        }
        if end == start || full_width + advance <= max_width {
            // First word on a line always lands (overflow allowed —
            // leading authored spaces are part of that first landing
            // since `start` never moves past them).
            end = range.end;
            full_width += advance;
        } else {
            lines.push(start..end);
            start = range.start;
            end = range.end;
            full_width = advance;
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

/// Baseline-to-baseline distance in font units (hhea; descender is
/// negative).
pub(crate) fn line_advance(font: &Font) -> i32 {
    i32::from(font.ascender()) - i32::from(font.descender()) + i32::from(font.line_gap())
}
