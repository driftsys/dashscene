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
    scales: &[f32],
    max_width: Option<f32>,
    letter_spacing: f32,
) -> Vec<Range<usize>> {
    let max_width = max_width.unwrap_or(f32::INFINITY);
    let glyphs = &shaped.glyphs;
    // Each glyph is measured at its own font's scale (story #219): a
    // fallback font's advances are in that font's units, so measuring them
    // at the primary's scale would mis-size a different-upem fallback. Letter
    // spacing (story #310) tracks each glyph — added here so the fit check
    // measures the same width the pen places (default 0.0 is a no-op).
    let advance = |g: &ShapedGlyph| g.x_advance as f32 * scales[g.font as usize] + letter_spacing;
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
                width += advance(g);
            }
        } else {
            let mut prospective = width;
            for g in &glyphs[run_start..i] {
                prospective += advance(g);
            }
            // The candidate line's own trailing letter-spacing step — after
            // its own last glyph, were nothing else appended — is excluded
            // from the width Figma wraps against (story #336), matching
            // what `position_line` will actually report for it. `prospective`
            // always covers at least one glyph here (the inner while loop
            // above advances `i` past `run_start` at least once), so this
            // cannot underflow past an empty run.
            if end == start || prospective - letter_spacing <= max_width {
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
                    width += advance(g);
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
    scales: &[f32],
    baseline_y: f32,
    letter_spacing: f32,
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
                pen_x = place(&mut glyphs, g, pen_x, scales, baseline_y, letter_spacing);
            }
        } else {
            for g in run_glyphs {
                pen_x = place(&mut glyphs, g, pen_x, scales, baseline_y, letter_spacing);
            }
        }
    }
    Line {
        glyphs,
        // The pen (`place`, below) advances past every glyph including the
        // line's last, so `pen_x` carries one letter-spacing step too many
        // for the reported width — Figma excludes the trailing step from the
        // box extent and alignment (story #336). Glyph positions above are
        // untouched by this: each glyph is pushed *before* its own trailing
        // step is folded into `pen_x`, so only this returned width shrinks,
        // never a glyph's `x`. `range` is non-empty here (the empty case
        // returns above), so at least one step was added and this cannot
        // underflow.
        width: pen_x - letter_spacing,
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
    scales: &[f32],
    baseline_y: f32,
    letter_spacing: f32,
) -> f32 {
    // Each glyph scales by its own font's upem (story #219): offsets and
    // the advance are in that glyph's font units, so a different-upem
    // fallback places correctly beside the primary. Letter spacing (story
    // #310) advances the pen an extra step after the glyph — the same amount
    // the measure fold in `break_lines` adds (default 0.0 is a no-op).
    let scale = scales[g.font as usize];
    out.push(PositionedGlyph {
        glyph_id: g.glyph_id,
        font: g.font,
        x: pen_x + g.x_offset as f32 * scale,
        y: baseline_y - g.y_offset as f32 * scale,
    });
    pen_x + g.x_advance as f32 * scale + letter_spacing
}

#[cfg(test)]
mod tests {
    use unicode_bidi::BidiInfo;

    use super::super::shape::{ShapedGlyph, ShapedText};
    use super::{break_lines, position_line};

    fn glyph(glyph_id: u16, cluster: u32, font: u16, x_advance: i32) -> ShapedGlyph {
        ShapedGlyph {
            glyph_id,
            cluster,
            font,
            x_advance,
            x_offset: 0,
            y_offset: 0,
        }
    }

    /// C2: each glyph scales by its own font's upem, not the primary's. Two
    /// glyphs with the same font-unit advance but from fonts of different
    /// upem must land at positions and contribute widths that reflect each
    /// glyph's own scale — otherwise a fallback font behind a different-upem
    /// primary mis-sizes and mis-places all its text.
    #[test]
    fn each_glyph_scales_by_its_own_fonts_upem() {
        let text = "ab"; // one LTR level run; clusters 0 and 1
        let bidi = BidiInfo::new(text, None);
        let para = &bidi.paragraphs[0];
        let shaped = ShapedText {
            glyphs: vec![glyph(1, 0, 0, 1000), glyph(2, 1, 1, 1000)],
        };
        // size 32: primary upem 1000 → scale 0.032; fallback upem 2000 →
        // scale 0.016.
        let scales = [32.0 / 1000.0, 32.0 / 2000.0];
        let line = position_line(&bidi, para, &shaped, 0..2, &scales, 0.0, 0.0);
        assert_eq!(line.glyphs.len(), 2);
        assert!((line.glyphs[0].x - 0.0).abs() < 1e-4);
        // Second glyph placed after the FIRST glyph's own-font advance
        // (0.032 * 1000 = 32).
        assert!(
            (line.glyphs[1].x - 32.0).abs() < 1e-4,
            "second glyph x {}",
            line.glyphs[1].x
        );
        // Line width adds the SECOND glyph's own-font advance
        // (0.016 * 1000 = 16), for 48 total.
        assert!(
            (line.width - 48.0).abs() < 1e-4,
            "line width {}",
            line.width
        );
    }

    /// Wrapping measures each glyph at its own scale too: a fallback word's
    /// advance must not be measured at the primary's scale. With correct
    /// per-glyph scale the half-upem word "b" fits under the width; measured
    /// at the primary's scale it would overflow and wrap.
    #[test]
    fn break_lines_measures_each_glyph_at_its_own_scale() {
        let text = "a b"; // 'a'=0, ' '=1, 'b'=2
        let shaped = ShapedText {
            glyphs: vec![
                glyph(1, 0, 0, 1000), // 'a', font 0 → 32
                glyph(2, 1, 0, 1000), // ' ', font 0 → 32
                glyph(3, 2, 1, 1000), // 'b', font 1 → 16 (correct) / 32 (wrong)
            ],
        };
        let scales = [32.0 / 1000.0, 32.0 / 2000.0];
        // 32 + 32 + 16 = 80 ≤ 85 fits on one line; at the primary's scale it
        // would be 32 + 32 + 32 = 96 > 85 and wrap.
        let lines = break_lines(text, &shaped, 0..3, &scales, Some(85.0), 0.0);
        assert_eq!(lines, vec![0..3]);
    }

    /// Figma excludes a line's trailing letter-spacing step — the one after
    /// its own last glyph — from the width it wraps against (story #336):
    /// the fit check must compare against that corrected width, not the raw
    /// per-glyph fold. "a b" at letter_spacing 4.0, scale 1.0: 'a' (always
    /// placed) plus the space accumulate 1000+4 + 1000+4 = 2008; adding 'b'
    /// raw (1000+4) reaches 3012, but the real rendered width of "a b" as
    /// one line — dropping only 'b's own trailing step — is 3008. A
    /// max_width of 3010 sits between the two: the old (uncorrected) fit
    /// check compared 3012 and would have wrapped; the corrected one must
    /// compare 3008 and keep both words on one line.
    #[test]
    fn break_lines_fit_check_excludes_the_candidate_lines_trailing_step() {
        let text = "a b"; // 'a'=0, ' '=1, 'b'=2
        let shaped = ShapedText {
            glyphs: vec![
                glyph(1, 0, 0, 1000),
                glyph(2, 1, 0, 1000),
                glyph(3, 2, 0, 1000),
            ],
        };
        let scales = [1.0];
        let lines = break_lines(text, &shaped, 0..3, &scales, Some(3010.0), 4.0);
        assert_eq!(lines, vec![0..3], "expected \"a b\" to stay on one line");
    }

    /// A run's reported width drops the one tracking step after its own
    /// last glyph (story #336); glyph positions are untouched — each glyph
    /// still sits after the *preceding* glyph's own advance-plus-spacing,
    /// exactly as before. Three glyphs, letter_spacing 4.0, scale 1.0: the
    /// pen crosses two internal steps (after glyph 0, after glyph 1) but not
    /// a third after glyph 2, since nothing follows it on this line.
    #[test]
    fn position_line_drops_the_lines_trailing_letter_spacing_step() {
        let text = "abc";
        let bidi = BidiInfo::new(text, None);
        let para = &bidi.paragraphs[0];
        let shaped = ShapedText {
            glyphs: vec![
                glyph(1, 0, 0, 1000),
                glyph(2, 1, 0, 1000),
                glyph(3, 2, 0, 1000),
            ],
        };
        let scales = [1.0];
        let line = position_line(&bidi, para, &shaped, 0..3, &scales, 0.0, 4.0);
        // Positions unchanged: glyph 0 at 0, glyph 1 after 1000+4, glyph 2
        // after 2*(1000+4).
        assert!(
            (line.glyphs[0].x - 0.0).abs() < 1e-4,
            "glyph 0 x {}",
            line.glyphs[0].x
        );
        assert!(
            (line.glyphs[1].x - 1004.0).abs() < 1e-4,
            "glyph 1 x {}",
            line.glyphs[1].x
        );
        assert!(
            (line.glyphs[2].x - 2008.0).abs() < 1e-4,
            "glyph 2 x {}",
            line.glyphs[2].x
        );
        // Width drops the third glyph's trailing step: 3*1000 + 2*4, not
        // + 3*4.
        assert!((line.width - 3008.0).abs() < 1e-4, "width {}", line.width);
    }

    /// A single-glyph run has no internal gap to space, so it carries zero
    /// tracking steps in its width — the pen's one trailing step (after the
    /// glyph, with nothing following it on the line) is excluded.
    #[test]
    fn position_line_single_glyph_run_has_zero_tracking_steps() {
        let text = "a";
        let bidi = BidiInfo::new(text, None);
        let para = &bidi.paragraphs[0];
        let shaped = ShapedText {
            glyphs: vec![glyph(1, 0, 0, 1000)],
        };
        let scales = [1.0];
        let line = position_line(&bidi, para, &shaped, 0..1, &scales, 0.0, 4.0);
        assert!((line.width - 1000.0).abs() < 1e-4, "width {}", line.width);
    }

    /// An empty glyph range must not underflow to a negative width when
    /// subtracting a trailing step that was never added.
    #[test]
    fn position_line_empty_range_has_zero_width_no_underflow() {
        let text = "a";
        let bidi = BidiInfo::new(text, None);
        let para = &bidi.paragraphs[0];
        let shaped = ShapedText {
            glyphs: vec![glyph(1, 0, 0, 1000)],
        };
        let scales = [1.0];
        let line = position_line(&bidi, para, &shaped, 0..0, &scales, 0.0, 4.0);
        assert_eq!(line.width, 0.0);
        assert!(line.glyphs.is_empty());
    }

    /// Zero letter-spacing is a no-op for the trailing-step correction too —
    /// the width is exactly the raw per-glyph advance sum, as before #336.
    #[test]
    fn position_line_zero_letter_spacing_is_unchanged() {
        let text = "abc";
        let bidi = BidiInfo::new(text, None);
        let para = &bidi.paragraphs[0];
        let shaped = ShapedText {
            glyphs: vec![
                glyph(1, 0, 0, 1000),
                glyph(2, 1, 0, 1000),
                glyph(3, 2, 0, 1000),
            ],
        };
        let scales = [1.0];
        let line = position_line(&bidi, para, &shaped, 0..3, &scales, 0.0, 0.0);
        assert!((line.width - 3000.0).abs() < 1e-4, "width {}", line.width);
    }
}
