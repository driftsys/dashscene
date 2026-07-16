//! rustybuzz shaping into font-unit glyph runs (docs/design/architecture.md),
//! one run per UAX #9 level run — shaping a mixed-direction string as
//! a single run mis-orders embedded digits (spike #25,
//! `docs/technotes/msdf-arabic-atlas-spike.md`), so the bidi split
//! comes first.
//!
//! Ligatures (`liga`, `clig`) are disabled: the atlas closure is
//! cmap-only in v0.5 (`docs/decisions/atlas-closure-cmap-plus-extras.md`),
//! so a ligature glyph would shape to an id the atlas cannot cover.
//! They return together with GSUB charset closure (the v0.6 charset
//! story) as one coordinated change. Kerning stays on — it moves pen
//! positions and needs no atlas coverage.

use std::ops::Range;

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Feature, UnicodeBuffer};
use unicode_bidi::{BidiInfo, ParagraphInfo};

use super::font::Font;

/// One shaped glyph in font units, offsets preserved (GPOS positions
/// marks through offsets — spike #25 finding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// Byte index of the source character (cluster) in the shaped text.
    pub cluster: u32,
    pub x_advance: i32,
    pub x_offset: i32,
    /// HarfBuzz convention: y-up. Positioning negates it into document
    /// space (y-down).
    pub y_offset: i32,
}

/// A shaped, unpositioned run in font units — the cache value. Size
/// independence is what lets one entry serve every render size.
/// Glyphs are in logical order (clusters non-decreasing); the display
/// reorder is per line, at positioning time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
}

/// Shapes `text` as one run of the given direction — the caller
/// resolves the direction per UAX #9 level run ([`shape_paragraph`]),
/// never per whole mixed-direction string.
///
/// Glyphs come back in logical order: rustybuzz emits an RTL run in
/// visual (left-to-right) order, so it is reversed here. Positioning
/// re-reverses RTL runs for display (`layout.rs`).
pub(crate) fn shape(font: &Font, text: &str, direction: Direction) -> ShapedText {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    buffer.set_direction(direction);
    let features = [
        Feature::new(Tag::from_bytes(b"liga"), 0, ..),
        Feature::new(Tag::from_bytes(b"clig"), 0, ..),
    ];
    let glyphs = rustybuzz::shape(&font.face(), &features, buffer);
    let infos = glyphs.glyph_infos();
    let positions = glyphs.glyph_positions();
    let mut glyphs: Vec<ShapedGlyph> = infos
        .iter()
        .zip(positions)
        .map(|(i, p)| ShapedGlyph {
            // TrueType glyph ids are u16; rustybuzz widens to u32.
            glyph_id: i.glyph_id as u16,
            cluster: i.cluster,
            x_advance: p.x_advance,
            x_offset: p.x_offset,
            y_offset: p.y_offset,
        })
        .collect();
    if direction == Direction::RightToLeft {
        glyphs.reverse();
    }
    ShapedText { glyphs }
}

/// Shapes one paragraph: itemizes it into UAX #9 level runs, shapes
/// each run with its resolved direction, and rebases clusters from
/// run-relative to paragraph-relative bytes. Output stays in logical
/// order across the whole paragraph.
pub(crate) fn shape_paragraph(font: &Font, bidi: &BidiInfo<'_>) -> ShapedText {
    let mut glyphs = Vec::new();
    // One '\n'-split paragraph normally holds one bidi paragraph; the
    // other UAX #9 block separators (U+2029, U+0085, …) add more, and
    // `bidi.paragraphs` covers every byte either way.
    for para in &bidi.paragraphs {
        for run in level_runs(bidi, para) {
            let direction = if bidi.levels[run.start].is_rtl() {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            };
            let run_start = run.start as u32;
            let shaped = shape(font, &bidi.text[run], direction);
            glyphs.extend(shaped.glyphs.into_iter().map(|g| ShapedGlyph {
                cluster: g.cluster + run_start,
                ..g
            }));
        }
    }
    ShapedText { glyphs }
}

/// Maximal byte ranges sharing one resolved UAX #9 level, in logical
/// order. unicode-bidi resolves levels per byte and every byte of a
/// char carries its char's level, so boundaries fall on char
/// boundaries.
pub(crate) fn level_runs(bidi: &BidiInfo<'_>, para: &ParagraphInfo) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start = para.range.start;
    for i in start + 1..para.range.end {
        if bidi.levels[i] != bidi.levels[start] {
            runs.push(start..i);
            start = i;
        }
    }
    runs.push(start..para.range.end);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        let data = std::fs::read(crate::atlas::TEST_FONT).expect("fixture font present");
        Font::from_bytes(data, 0).expect("loads")
    }

    fn cmap(font_data: &[u8], c: char) -> u16 {
        ttf_parser::Face::parse(font_data, 0)
            .unwrap()
            .glyph_index(c)
            .unwrap()
            .0
    }

    fn hmtx(font_data: &[u8], c: char) -> u16 {
        let face = ttf_parser::Face::parse(font_data, 0).unwrap();
        let gid = face.glyph_index(c).unwrap();
        face.glyph_hor_advance(gid).unwrap()
    }

    #[test]
    fn shapes_av_with_kerning() {
        let data = std::fs::read(crate::atlas::TEST_FONT).unwrap();
        let shaped = shape(&font(), "AV", Direction::LeftToRight);
        assert_eq!(shaped.glyphs.len(), 2);
        assert_eq!(shaped.glyphs[0].glyph_id, cmap(&data, 'A'));
        assert_eq!(shaped.glyphs[1].glyph_id, cmap(&data, 'V'));
        let shaped_total: i32 = shaped.glyphs.iter().map(|g| g.x_advance).sum();
        let plain_total = i32::from(hmtx(&data, 'A')) + i32::from(hmtx(&data, 'V'));
        assert!(
            shaped_total < plain_total,
            "kerning must tighten AV: {shaped_total} vs {plain_total}"
        );
    }

    #[test]
    fn liga_disabled_keeps_fi_two_glyphs() {
        let data = std::fs::read(crate::atlas::TEST_FONT).unwrap();
        let shaped = shape(&font(), "fi", Direction::LeftToRight);
        assert_eq!(shaped.glyphs.len(), 2, "liga must be off");
        assert_eq!(shaped.glyphs[0].glyph_id, cmap(&data, 'f'));
        assert_eq!(shaped.glyphs[1].glyph_id, cmap(&data, 'i'));
    }

    #[test]
    fn clusters_are_byte_indices() {
        let shaped = shape(&font(), "ab", Direction::LeftToRight);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 1]);
    }

    #[test]
    fn empty_text_shapes_to_nothing() {
        assert!(shape(&font(), "", Direction::LeftToRight).glyphs.is_empty());
    }

    #[test]
    fn rtl_direction_restores_logical_cluster_order() {
        // rustybuzz emits an RTL run in visual (left-to-right) order,
        // clusters descending; shape() reverses it back to logical
        // order — the invariant the breaker and the cache rely on.
        let shaped = shape(&font(), "אב", Direction::RightToLeft);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 2]);
    }

    #[test]
    fn level_runs_match_uax9_references() {
        // Hand-derived UAX #9 reference cases. Byte offsets: a Hebrew
        // letter is 2 UTF-8 bytes, ASCII is 1.
        type Case = (&'static str, u8, &'static [(Range<usize>, u8)]);
        let cases: &[Case] = &[
            // Pure LTR: one even run.
            ("abc", 0, &[(0..3, 0)]),
            // Pure RTL: one odd run.
            ("אבג", 1, &[(0..6, 1)]),
            // Trailing RTL word in an LTR paragraph; the space takes
            // the base level (N2).
            ("abc אבג", 0, &[(0..4, 0), (4..10, 1)]),
            // Trailing LTR word in an RTL paragraph (I2 lifts L to 2).
            ("אבג abc", 1, &[(0..7, 1), (7..10, 2)]),
            // Digits embedded in RTL text: EN lifts to level 2 (I2);
            // the spaces resolve to the RTL level (N1 treats EN as R).
            ("אב 123 גד", 1, &[(0..5, 1), (5..8, 2), (8..13, 1)]),
            // Digits after strong L become L themselves (W7), so an
            // LTR paragraph with trailing digits stays one run.
            ("abc 123", 0, &[(0..7, 0)]),
        ];
        for (text, base, expected) in cases {
            let bidi = BidiInfo::new(text, None);
            let para = &bidi.paragraphs[0];
            assert_eq!(para.level.number(), *base, "base level of {text:?}");
            let runs = level_runs(&bidi, para);
            let got: Vec<(Range<usize>, u8)> = runs
                .into_iter()
                .map(|r| (r.clone(), bidi.levels[r.start].number()))
                .collect();
            assert_eq!(got, *expected, "level runs of {text:?}");
        }
    }

    #[test]
    fn visual_run_order_matches_uax9_references() {
        // The per-line display reorder (L2) the positioning code
        // consumes, pinned against hand-derived references: runs come
        // back as absolute byte ranges in visual order.
        let cases: &[(&str, &[Range<usize>])] = &[
            ("abc אבג", &[0..4, 4..10]),
            ("אבג abc", &[7..10, 0..7]),
            ("אב 123 גד", &[8..13, 5..8, 0..5]),
        ];
        for (text, expected) in cases {
            let bidi = BidiInfo::new(text, None);
            let para = &bidi.paragraphs[0];
            let (_, runs) = bidi.visual_runs(para, para.range.clone());
            assert_eq!(&runs, expected, "visual order of {text:?}");
        }
    }

    #[test]
    fn shape_paragraph_rebases_clusters_to_paragraph_bytes() {
        // Three level runs shaped separately; clusters come back
        // paragraph-relative and non-decreasing across run boundaries.
        let text = "אב 123 גד";
        let bidi = BidiInfo::new(text, None);
        let shaped = shape_paragraph(&font(), &bidi);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 2, 4, 5, 6, 7, 8, 9, 11]);
    }
}
