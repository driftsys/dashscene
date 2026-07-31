//! Cacheable UAX #9 resolution, and per-line reordering that does not
//! copy the whole paragraph once per line (issues #225, #226).
//!
//! [`unicode_bidi::BidiInfo`] borrows the text it resolved, so it cannot
//! itself be stored in a cache keyed by that text. [`Resolved`] is its
//! three owned fields, which is everything the pipeline reads besides the
//! text; [`Bidi`] pairs a `Resolved` back with a `&str` and derefs to it,
//! so every reader keeps the field access it had against `BidiInfo`.
//!
//! [`Reorder`] replaces [`unicode_bidi::BidiInfo::visual_runs`], which
//! runs Rule L1 by cloning the paragraph's whole level vector and then
//! adjusting only the line's slice of it. A paragraph wrapped into N
//! lines paid that clone N times per `layout()` call. `Reorder` keeps one
//! buffer and copies only each line's own levels into it, so a paragraph
//! copies its own length once however many lines it wraps into.

use std::ops::{Deref, Range};

use unicode_bidi::{BidiClass, BidiInfo, Level, LevelRun, ParagraphInfo};

/// The base paragraph level this crate resolves under. `None` means UAX
/// #9 P2/P3 auto-detection: each paragraph takes its direction from its
/// own first strong character, so the resolved levels are a pure function
/// of the paragraph text.
///
/// **This constant is why the cache key is the text alone.**
/// [`BidiInfo::new`] takes exactly two inputs — the text and this base
/// level — and nothing else in the pipeline reaches the resolution. If a
/// base direction ever becomes a per-call axis (an authored
/// `direction: rtl`, say), it stops being derivable from the text and
/// must join the key of `Typesetter::bidi_cache`, or a paragraph resolved
/// under one direction would be served to a layout asking for the other.
/// `base_level_pins_the_cache_key` guards the constant against a silent
/// change.
pub(crate) const BASE_LEVEL: Option<Level> = None;

/// One paragraph chunk's resolved UAX #9 state, owned so it can outlive
/// the `&str` it was resolved from and live in the typesetter's cache.
/// These are [`BidiInfo`]'s fields other than its borrowed `text`.
#[derive(Debug)]
pub(crate) struct Resolved {
    pub(crate) original_classes: Vec<BidiClass>,
    pub(crate) levels: Vec<Level>,
    pub(crate) paragraphs: Vec<ParagraphInfo>,
}

impl Resolved {
    /// Runs the full UAX #9 resolution over `text` at [`BASE_LEVEL`].
    /// This is the work issue #225 was repaying on every `layout()` call.
    pub(crate) fn new(text: &str) -> Resolved {
        let BidiInfo {
            original_classes,
            levels,
            paragraphs,
            ..
        } = BidiInfo::new(text, BASE_LEVEL);
        Resolved {
            original_classes,
            levels,
            paragraphs,
        }
    }
}

/// A [`Resolved`] paired with the text it was resolved from — the
/// borrowing view [`BidiInfo`] is, over state the cache owns. Derefs to
/// [`Resolved`], and carries `text` as its own field, so `bidi.text`,
/// `bidi.levels`, `bidi.original_classes` and `bidi.paragraphs` all read
/// exactly as they did against `BidiInfo`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bidi<'a> {
    pub(crate) text: &'a str,
    resolved: &'a Resolved,
}

impl<'a> Bidi<'a> {
    /// # Panics
    ///
    /// Panics if `resolved` was not resolved from `text` — pairing a
    /// resolution with different text is exactly the stale-cache bug this
    /// type exists to make impossible, so it is checked rather than
    /// assumed. The check is a length comparison: `original_classes` and
    /// `levels` hold one entry per *byte* of the resolved text.
    pub(crate) fn new(text: &'a str, resolved: &'a Resolved) -> Bidi<'a> {
        assert_eq!(
            resolved.levels.len(),
            text.len(),
            "bidi resolution paired with text it was not resolved from"
        );
        Bidi { text, resolved }
    }
}

impl Deref for Bidi<'_> {
    type Target = Resolved;

    fn deref(&self) -> &Resolved {
        self.resolved
    }
}

/// The per-line reorder buffer (issue #226), held by the typesetter and
/// reused across every line of every paragraph of every `layout()` call.
///
/// [`unicode_bidi::BidiInfo::visual_runs`] allocates and copies a
/// paragraph-length `Vec<Level>` per call, because its Rule L1 pass wants
/// to hand back levels indexed by paragraph byte. Rule L1 only ever
/// *writes* inside the line, so this keeps one buffer of paragraph length
/// and refreshes just the line's slice of it before each line's pass —
/// the same indexing, without the per-line allocation.
#[derive(Debug, Default)]
pub(crate) struct Reorder {
    /// Levels indexed by paragraph byte, valid only inside the line most
    /// recently passed to [`line`](Self::line).
    levels: Vec<Level>,
    /// Total [`Level`] slots copied into `levels`. Instrumentation for the
    /// issue #226 assertion, not used by the algorithm: with the buffer it
    /// counts each line's own length, and without it counted the whole
    /// paragraph's length per line.
    pub(crate) copied: u64,
}

impl Reorder {
    /// Rules L1 and L2 for one line of `para`, returning the reordered
    /// levels (indexed by paragraph byte, valid within `line`) and the
    /// line's level runs in visual order — the same pair
    /// [`unicode_bidi::BidiInfo::visual_runs`] returns, for the same
    /// arguments.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not within `bidi`, or if `line` is empty at the
    /// very end of the text, matching `visual_runs`'s own bounds.
    pub(crate) fn line(
        &mut self,
        bidi: Bidi<'_>,
        para: &ParagraphInfo,
        line: Range<usize>,
    ) -> (&[Level], Vec<LevelRun>) {
        assert!(line.end <= bidi.levels.len(), "line beyond the paragraph");
        if self.levels.len() < bidi.levels.len() {
            self.levels.resize(bidi.levels.len(), Level::ltr());
        }
        // Only the line's own slice is refreshed and only it is read
        // below, so whatever a previous line left elsewhere in the buffer
        // is never observed.
        self.levels[line.clone()].copy_from_slice(&bidi.levels[line.clone()]);
        self.copied += line.len() as u64;
        reset_levels(
            &bidi.original_classes[line.clone()],
            &mut self.levels[line.clone()],
            &bidi.text[line.clone()],
            para.level,
        );
        let runs = visual_runs_for_line(&self.levels, &line);
        (&self.levels, runs)
    }
}

/// UAX #9 [Rule L1] over one line: whitespace and isolate formatting
/// before a segment or paragraph separator, and at the end of the line,
/// reset to the paragraph level; the retained explicit formatting
/// characters take the preceding level.
///
/// All three slices are line-relative — `classes` and `levels` are the
/// line's slices of the paragraph's vectors, and `text` the line's text.
/// This is `unicode_bidi`'s own `reorder_levels`, which is private, over
/// `str` rather than its `TextSource` trait: for `str` its
/// `indices_lengths` and `char_len` are both `char::len_utf8`, so the two
/// walks it zips are one walk here. `reorder_matches_unicode_bidi` pins
/// the equivalence against the upstream implementation.
///
/// [Rule L1]: https://www.unicode.org/reports/tr9/#L1
fn reset_levels(classes: &[BidiClass], levels: &mut [Level], text: &str, para_level: Level) {
    let mut reset_from: Option<usize> = Some(0);
    let mut reset_to: Option<usize> = None;
    let mut prev_level = para_level;
    for (i, c) in text.char_indices() {
        let length = c.len_utf8();
        match classes[i] {
            // Segment separator, paragraph separator.
            BidiClass::B | BidiClass::S => {
                debug_assert_eq!(reset_to, None);
                reset_to = Some(i + length);
                if reset_from.is_none() {
                    reset_from = Some(i);
                }
            }
            // Whitespace, isolate formatting.
            BidiClass::WS | BidiClass::FSI | BidiClass::LRI | BidiClass::RLI | BidiClass::PDI => {
                if reset_from.is_none() {
                    reset_from = Some(i);
                }
            }
            // Retained explicit formatting characters: as above, and the
            // level itself takes the preceding one.
            // <https://www.unicode.org/reports/tr9/#Retaining_Explicit_Formatting_Characters>
            BidiClass::RLE
            | BidiClass::LRE
            | BidiClass::RLO
            | BidiClass::LRO
            | BidiClass::PDF
            | BidiClass::BN => {
                if reset_from.is_none() {
                    reset_from = Some(i);
                }
                for level in &mut levels[i..i + length] {
                    *level = prev_level;
                }
            }
            _ => reset_from = None,
        }
        if let (Some(from), Some(to)) = (reset_from, reset_to) {
            for level in &mut levels[from..to] {
                *level = para_level;
            }
            reset_from = None;
            reset_to = None;
        }
        prev_level = levels[i];
    }
    if let Some(from) = reset_from {
        for level in &mut levels[from..] {
            *level = para_level;
        }
    }
}

/// UAX #9 [Rule L2] over one line: the line's maximal same-level runs,
/// returned in visual order. `levels` is indexed by paragraph byte and
/// only `line` is read.
///
/// This is `unicode_bidi`'s own `visual_runs_for_line`, which is private,
/// taking the levels by reference instead of by value — the whole point
/// of issue #226 being that the caller no longer owns a fresh copy of
/// them.
///
/// [Rule L2]: https://www.unicode.org/reports/tr9/#L2
fn visual_runs_for_line(levels: &[Level], line: &Range<usize>) -> Vec<LevelRun> {
    let mut runs = Vec::new();
    let mut start = line.start;
    let mut run_level = levels[start];
    let mut min_level = run_level;
    let mut max_level = run_level;

    for (i, &new_level) in levels.iter().enumerate().take(line.end).skip(start + 1) {
        if new_level != run_level {
            runs.push(start..i);
            start = i;
            run_level = new_level;
            min_level = min_level.min(run_level);
            max_level = max_level.max(run_level);
        }
    }
    runs.push(start..line.end);

    let run_count = runs.len();
    // Reverse contiguous stretches of runs at or above `max_level`,
    // lowering `max_level` by one each pass, down to the lowest odd level.
    min_level = min_level.new_lowest_ge_rtl().expect("Level error");
    while max_level >= min_level {
        let mut seq_start = 0;
        while seq_start < run_count {
            if levels[runs[seq_start].start] < max_level {
                seq_start += 1;
                continue;
            }
            let mut seq_end = seq_start + 1;
            while seq_end < run_count {
                if levels[runs[seq_end].start] < max_level {
                    break;
                }
                seq_end += 1;
            }
            runs[seq_start..seq_end].reverse();
            seq_start = seq_end;
        }
        max_level
            .lower(1)
            .expect("Lowering embedding level below zero");
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strings that exercise the reordering rules this module reimplements:
    /// pure LTR, pure RTL, both directions mixed, digits inside RTL,
    /// trailing and interior whitespace, an explicit isolate, an explicit
    /// override, and a segment separator.
    const CORPUS: &[&str] = &[
        "",
        "abc",
        "abc def ghi",
        "\u{5d0}\u{5d1}\u{5d2}",
        "\u{5d0} 12 \u{5d2}\u{5d3}\u{5d4}",
        "abc \u{5d0}\u{5d1} def",
        "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} 2026 abc",
        "abc   ",
        "   abc",
        "\u{5d0}\u{5d1}   ",
        "a\tb\tc",
        "abc\u{2066}\u{5d0}\u{5d1}\u{2069}def",
        "abc\u{202e}def\u{202c}ghi",
        "\u{5d0}\u{200b}\u{5d1} abc 42",
    ];

    /// The cache key is the paragraph text **because** the only other input
    /// to the resolution is pinned here. A change to this constant makes a
    /// text-keyed cache unsound, so it is asserted rather than assumed
    /// (issue #225).
    #[test]
    fn base_level_pins_the_cache_key() {
        assert_eq!(
            BASE_LEVEL, None,
            "the bidi cache in Typesetter is keyed by paragraph text alone; \
             a base level that is not auto-detected from the text must join \
             that key"
        );
    }

    /// The reimplemented Rules L1 and L2 must agree with `unicode_bidi`'s
    /// own, byte for byte, for every line split of every corpus string —
    /// that equivalence is what lets `Reorder` replace `visual_runs`
    /// without moving a pixel.
    #[test]
    fn reorder_matches_unicode_bidi() {
        let mut reorder = Reorder::default();
        for &text in CORPUS {
            let info = BidiInfo::new(text, BASE_LEVEL);
            let resolved = Resolved::new(text);
            let bidi = Bidi::new(text, &resolved);
            for para in &info.paragraphs {
                // Every char-boundary split of the paragraph, both halves.
                let bounds: Vec<usize> = (para.range.start..=para.range.end)
                    .filter(|&i| text.is_char_boundary(i))
                    .collect();
                for &start in &bounds {
                    for &end in bounds.iter().filter(|&&e| e >= start) {
                        if start == end && start == text.len() {
                            // `visual_runs` itself indexes `levels[start]`
                            // here and would panic; not a reachable line.
                            continue;
                        }
                        let (want_levels, want_runs) = info.visual_runs(para, start..end);
                        let (got_levels, got_runs) = reorder.line(bidi, para, start..end);
                        assert_eq!(got_runs, want_runs, "{text:?} line {start}..{end}");
                        assert_eq!(
                            &got_levels[start..end],
                            &want_levels[start..end],
                            "{text:?} line {start}..{end}"
                        );
                    }
                }
            }
        }
    }

    /// `Resolved` must carry exactly what `BidiInfo` resolved — it is the
    /// cached stand-in for it, so a divergence would be served forever.
    #[test]
    fn resolved_matches_bidi_info() {
        for &text in CORPUS {
            let info = BidiInfo::new(text, BASE_LEVEL);
            let resolved = Resolved::new(text);
            assert_eq!(resolved.levels, info.levels, "{text:?}");
            assert_eq!(resolved.original_classes, info.original_classes, "{text:?}");
            assert_eq!(resolved.paragraphs, info.paragraphs, "{text:?}");
        }
    }

    /// Pairing a resolution with text it was not resolved from is the
    /// stale-cache failure, and must not be silent.
    #[test]
    #[should_panic(expected = "bidi resolution paired with text it was not resolved from")]
    fn pairing_a_resolution_with_other_text_panics() {
        let resolved = Resolved::new("abc");
        let _ = Bidi::new("abcd", &resolved);
    }

    /// The buffer is reused, so a long paragraph followed by a short one
    /// must not let the long one's leftover levels reach the short one's
    /// runs.
    #[test]
    fn a_reused_buffer_carries_nothing_between_paragraphs() {
        let mut reorder = Reorder::default();
        let long = "\u{5d0}\u{5d1}\u{5d2} abc \u{5d3}\u{5d4}\u{5d5} def";
        let short = "ab";
        for text in [long, short, long, short] {
            let info = BidiInfo::new(text, BASE_LEVEL);
            let resolved = Resolved::new(text);
            let bidi = Bidi::new(text, &resolved);
            let para = &info.paragraphs[0];
            let (want_levels, want_runs) = info.visual_runs(para, para.range.clone());
            let (got_levels, got_runs) = reorder.line(bidi, para, para.range.clone());
            assert_eq!(got_runs, want_runs, "{text:?}");
            assert_eq!(
                &got_levels[para.range.clone()],
                &want_levels[para.range.clone()],
                "{text:?}"
            );
        }
    }

    /// The issue #226 count, at the unit level: reordering the lines of one
    /// paragraph copies each line's own levels, so the total is the sum of
    /// the line lengths — not the line count times the paragraph length.
    #[test]
    fn reordering_copies_each_line_once_not_the_paragraph_per_line() {
        let text = "aaaa bbbb cccc dddd eeee ffff";
        let info = BidiInfo::new(text, BASE_LEVEL);
        let resolved = Resolved::new(text);
        let bidi = Bidi::new(text, &resolved);
        let para = &info.paragraphs[0];
        let lines = [0..4, 5..9, 10..14, 15..19, 20..24, 25..29];
        let mut reorder = Reorder::default();
        for line in &lines {
            let _ = reorder.line(bidi, para, line.clone());
        }
        let expected: u64 = lines.iter().map(|l| l.len() as u64).sum();
        assert_eq!(reorder.copied, expected);
        // The pre-fix cost, for scale: six clones of a 29-byte paragraph.
        assert!(reorder.copied < (lines.len() * text.len()) as u64);
    }
}
