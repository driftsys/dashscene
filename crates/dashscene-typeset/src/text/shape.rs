//! rustybuzz shaping into font-unit glyph runs (docs/design/architecture.md),
//! one run per UAX #9 level run — shaping a mixed-direction string as
//! a single run mis-orders embedded digits (spike #25,
//! `docs/technotes/msdf-arabic-atlas-spike.md`), so the bidi split
//! comes first.
//!
//! Each run shapes under a [`RunContext`] derived from its paragraph
//! (never authored — P1): Arabic-context runs take rustybuzz's full
//! default feature set — the exact configuration the atlas closure
//! shapes with (`atlas::charset_closure`), so production output and
//! atlas coverage move together — and display European digits with
//! their Arabic-Indic counterparts' glyphs. All other runs keep
//! `liga`/`clig` disabled: the closure's ligature sweep is pairwise,
//! so a three-character Latin ligature (`ffi`) would shape to a glyph
//! id the atlas cannot cover
//! (`docs/decisions/liga-clig-off-until-gsub-closure.md`). Kerning
//! stays on everywhere — it moves pen positions and needs no atlas
//! coverage.

use std::ops::Range;

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Feature, UnicodeBuffer};
use unicode_bidi::{BidiClass, BidiInfo, ParagraphInfo, bidi_class};

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

/// Shaping posture of one level run, derived from its paragraph
/// context by [`run_context`] — never authored (P1: the document
/// carries the authored codepoints; digit shapes and feature sets are
/// resolved results that live only in the layout output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunContext {
    /// Full default OpenType feature set (`liga`/`clig` included) —
    /// the exact configuration `atlas::charset_closure` shapes with —
    /// and Arabic-Indic display shapes for European digits.
    Arabic,
    /// `liga`/`clig` disabled (the pairwise-closure boundary), digits
    /// keep their authored shapes.
    Plain,
}

/// The Arabic-Indic counterpart (U+0660..=U+0669) of a European
/// digit; any other character passes through unchanged. The charset
/// closure derives atlas coverage with this same function
/// (`atlas/closure.rs`), so the display rule and the coverage rule
/// cannot drift.
pub(crate) fn arabic_indic_digit(c: char) -> char {
    if c.is_ascii_digit() {
        char::from_u32(0x0660 + (c as u32 - '0' as u32)).expect("valid Arabic-Indic digit")
    } else {
        c
    }
}

/// Whether a character is strong Arabic — UAX #9 bidi class AL. The
/// one trigger for the Arabic run posture, shared with the charset
/// closure's digit-coverage derivation (`atlas/closure.rs`) so the
/// production rule and the coverage rule cannot drift. Class AL
/// covers the Arabic letters of every block (standard, supplement,
/// extended, presentation forms) but not Arabic-block neutrals such
/// as U+060C ARABIC COMMA (class CS), which travel with whatever run
/// surrounds them and must not change its posture.
pub(crate) fn is_arabic_strong(c: char) -> bool {
    matches!(bidi_class(c), BidiClass::AL)
}

/// Shapes `text` as one run of the given direction and context — the
/// caller resolves both per UAX #9 level run ([`shape_paragraph`]),
/// never per whole mixed-direction string.
///
/// Glyphs come back in logical order: rustybuzz emits an RTL run in
/// visual (left-to-right) order, so it is reversed here. Positioning
/// re-reverses RTL runs for display (`layout.rs`).
pub(crate) fn shape(
    font: &Font,
    text: &str,
    direction: Direction,
    context: RunContext,
) -> ShapedText {
    let mut buffer = UnicodeBuffer::new();
    // Pushed per char so a substituted digit keeps its authored byte
    // index as the cluster (`push_str` is this same loop without the
    // substitution) — breaking and positioning index the authored
    // text, never the display string.
    for (i, c) in text.char_indices() {
        let c = match context {
            RunContext::Arabic => arabic_indic_digit(c),
            RunContext::Plain => c,
        };
        buffer.add(c, i as u32);
    }
    buffer.guess_segment_properties();
    buffer.set_direction(direction);
    let liga_clig_off = [
        Feature::new(Tag::from_bytes(b"liga"), 0, ..),
        Feature::new(Tag::from_bytes(b"clig"), 0, ..),
    ];
    let features: &[Feature] = match context {
        RunContext::Arabic => &[],
        RunContext::Plain => &liga_clig_off,
    };
    let glyphs = rustybuzz::shape(&font.face(), features, buffer);
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
            let context = run_context(bidi, para, &run);
            let run_start = run.start as u32;
            let shaped = shape(font, &bidi.text[run], direction, context);
            glyphs.extend(shaped.glyphs.into_iter().map(|g| ShapedGlyph {
                cluster: g.cluster + run_start,
                ..g
            }));
        }
    }
    ShapedText { glyphs }
}

/// Resolves one level run's [`RunContext`] from its paragraph.
///
/// Strong characters inside the run decide directly: any strong
/// Arabic character (class AL, [`is_arabic_strong`]) makes the run
/// Arabic; otherwise any L or R makes it Plain — UAX #9 W7 folds
/// Latin-anchored digits into their L run, so those digits resolve
/// here. A run left with no strong character (digits, separators,
/// marks) matters only when it carries digits: European digits pick
/// a display shape and Arabic-Indic digits the closure-parity
/// feature set, while anything else shapes identically under either
/// posture. For those runs the nearest strong character before the
/// run's first digit decides, the nearest one after it when none
/// precedes (a number opening an Arabic sentence), and Plain when
/// the paragraph has no reachable strong character at all. Both
/// scans are isolate-aware (UAX #9 P2): an isolate's interior is
/// sealed, so the scans jump over initiator..PDI pairs and stop at
/// the enclosing isolate's boundary. Unlike the resolved classes,
/// the raw per-char classes keep EN distinct from AN (W2 rewrites
/// EN to AN after Arabic — exactly the distinction scanned for), so
/// the scans classify chars directly.
fn run_context(bidi: &BidiInfo<'_>, para: &ParagraphInfo, run: &Range<usize>) -> RunContext {
    let text = &bidi.text[run.clone()];
    if text.chars().any(is_arabic_strong) {
        return RunContext::Arabic;
    }
    if text
        .chars()
        .any(|c| matches!(bidi_class(c), BidiClass::L | BidiClass::R))
    {
        return RunContext::Plain;
    }
    let Some(first_digit) = text
        .char_indices()
        .find(|&(_, c)| c.is_ascii_digit() || bidi_class(c) == BidiClass::AN)
        .map(|(i, _)| i)
    else {
        return RunContext::Plain;
    };
    let first_digit = run.start + first_digit;
    let decisive = strong_before(&bidi.text[para.range.start..first_digit])
        .or_else(|| strong_after(&bidi.text[first_digit..para.range.end]));
    match decisive {
        Some(BidiClass::AL) => RunContext::Arabic,
        _ => RunContext::Plain,
    }
}

/// The nearest strong character (L, R, or AL) at `text`'s end,
/// walking backward and skipping isolate interiors: a PDI jumps the
/// scan over its isolate, and an unmatched initiator is the enclosing
/// isolate's start — nothing before it is visible from inside.
fn strong_before(text: &str) -> Option<BidiClass> {
    let mut depth = 0usize;
    for c in text.chars().rev() {
        match bidi_class(c) {
            BidiClass::PDI => depth += 1,
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            class @ (BidiClass::L | BidiClass::R | BidiClass::AL) if depth == 0 => {
                return Some(class);
            }
            _ => {}
        }
    }
    None
}

/// [`strong_before`]'s forward mirror: an initiator jumps the scan
/// over its isolate, and an unmatched PDI ends the enclosing isolate
/// — nothing after it is visible from inside.
fn strong_after(text: &str) -> Option<BidiClass> {
    let mut depth = 0usize;
    for c in text.chars() {
        match bidi_class(c) {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => depth += 1,
            BidiClass::PDI => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            class @ (BidiClass::L | BidiClass::R | BidiClass::AL) if depth == 0 => {
                return Some(class);
            }
            _ => {}
        }
    }
    None
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

    fn arabic_font() -> Font {
        let data = std::fs::read(crate::atlas::TEST_FONT_ARABIC).expect("arabic fixture present");
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
        let shaped = shape(&font(), "AV", Direction::LeftToRight, RunContext::Plain);
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
        let shaped = shape(&font(), "fi", Direction::LeftToRight, RunContext::Plain);
        assert_eq!(shaped.glyphs.len(), 2, "liga must be off");
        assert_eq!(shaped.glyphs[0].glyph_id, cmap(&data, 'f'));
        assert_eq!(shaped.glyphs[1].glyph_id, cmap(&data, 'i'));
    }

    #[test]
    fn clusters_are_byte_indices() {
        let shaped = shape(&font(), "ab", Direction::LeftToRight, RunContext::Plain);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 1]);
    }

    #[test]
    fn empty_text_shapes_to_nothing() {
        assert!(
            shape(&font(), "", Direction::LeftToRight, RunContext::Plain)
                .glyphs
                .is_empty()
        );
    }

    #[test]
    fn rtl_direction_restores_logical_cluster_order() {
        // rustybuzz emits an RTL run in visual (left-to-right) order,
        // clusters descending; shape() reverses it back to logical
        // order — the invariant the breaker and the cache rely on.
        let shaped = shape(&font(), "אב", Direction::RightToLeft, RunContext::Plain);
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

    fn gids(shaped: &ShapedText) -> Vec<u16> {
        shaped.glyphs.iter().map(|g| g.glyph_id).collect()
    }

    #[test]
    fn arabic_letters_take_their_joining_forms() {
        // Noto Sans Arabic composes each letter from a dotless
        // skeleton plus a dot glyph (spike #25), so beh's nominal cmap
        // glyph (606) never appears in shaped output, and the skeleton
        // gid differs per joining context: isolated 14, final 15,
        // medial 16, initial 19; the dot below is 316.
        let data = std::fs::read(crate::atlas::TEST_FONT_ARABIC).unwrap();
        let font = arabic_font();
        let isolated = shape(&font, "ب", Direction::RightToLeft, RunContext::Arabic);
        assert_eq!(gids(&isolated), vec![14, 316]);
        let word = shape(&font, "ببب", Direction::RightToLeft, RunContext::Arabic);
        // Logical order: initial, medial, final — each skeleton with
        // its dot, both glyphs of a letter sharing the letter's
        // cluster (2 bytes per char).
        assert_eq!(gids(&word), vec![19, 316, 16, 316, 15, 316]);
        let clusters: Vec<u32> = word.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 0, 2, 2, 4, 4]);
        assert!(!gids(&word).contains(&cmap(&data, 'ب')));
    }

    #[test]
    fn lam_alef_ligates_through_rlig() {
        // Lam+alef is a required ligature (rlig, default-on): both
        // output glyphs are contextual lam-alef forms (73 and 10, the
        // spike-verified gids), not the nominal lam (68) and alef (8).
        let data = std::fs::read(crate::atlas::TEST_FONT_ARABIC).unwrap();
        let shaped = shape(
            &arabic_font(),
            "لا",
            Direction::RightToLeft,
            RunContext::Arabic,
        );
        assert_eq!(gids(&shaped), vec![73, 10]);
        assert!(!gids(&shaped).contains(&cmap(&data, 'ل')));
        assert!(!gids(&shaped).contains(&cmap(&data, 'ا')));
    }

    #[test]
    fn harakat_carry_gpos_offsets() {
        // Beh + fatha: the mark is its own glyph (cmap gid 370) with
        // zero advance, positioned by GPOS through x/y offsets — the
        // spike #25 finding the ShapedGlyph fields exist for. The
        // offsets are the fixture font's exact values.
        let shaped = shape(
            &arabic_font(),
            "بَ",
            Direction::RightToLeft,
            RunContext::Arabic,
        );
        let fatha = shaped
            .glyphs
            .iter()
            .find(|g| g.glyph_id == 370)
            .expect("fatha glyph present");
        assert_eq!(fatha.x_advance, 0);
        assert_eq!((fatha.x_offset, fatha.y_offset), (324, -160));
        // The mark clusters with its base (both index the beh bytes).
        assert_eq!(fatha.cluster, 0);
    }

    #[test]
    fn arabic_context_displays_european_digits_as_arabic_indic() {
        // The substitution changes the display glyphs only: clusters
        // keep indexing the authored ASCII bytes.
        let data = std::fs::read(crate::atlas::TEST_FONT_ARABIC).unwrap();
        let shaped = shape(
            &arabic_font(),
            "123",
            Direction::LeftToRight,
            RunContext::Arabic,
        );
        let expected: Vec<u16> = "١٢٣".chars().map(|c| cmap(&data, c)).collect();
        assert_eq!(gids(&shaped), expected);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 1, 2]);
    }

    #[test]
    fn plain_context_keeps_european_digits() {
        let data = std::fs::read(crate::atlas::TEST_FONT_ARABIC).unwrap();
        let shaped = shape(
            &arabic_font(),
            "123",
            Direction::LeftToRight,
            RunContext::Plain,
        );
        let expected: Vec<u16> = "123".chars().map(|c| cmap(&data, c)).collect();
        assert_eq!(gids(&shaped), expected);
    }

    #[test]
    fn digit_runs_resolve_their_context_from_the_nearest_strong_character() {
        use RunContext::{Arabic, Plain};
        // Expected context per level run, in logical order.
        let cases: &[(&str, &[RunContext])] = &[
            // Digits after Arabic form their own level run; the
            // preceding AL decides.
            ("كتاب 123", &[Arabic, Arabic]),
            // Digits after Hebrew stay European.
            ("אב 123", &[Plain, Plain]),
            // Digits after Latin share its level run (W7) and stay
            // European through the backward scan.
            ("abc 123", &[Plain]),
            // Latin-embedded digits in an RTL paragraph: the nearest
            // strong character is the Latin 'c', not the Arabic word.
            ("كتاب abc 123", &[Arabic, Plain]),
            // A number opening an Arabic sentence: the following
            // strong character decides.
            ("123 كتاب", &[Arabic, Arabic]),
            // No strong character at all: keep the authored shapes.
            ("123", &[Plain]),
            // Authored Arabic-Indic digits with no strong anchor: the
            // scan finds nothing, and either posture shapes them to
            // the same cmap glyphs.
            ("١٢٣", &[Plain]),
            // Anchored Arabic-Indic digits take the Arabic posture —
            // the closure-parity feature set.
            ("سرعة ١٢٣", &[Arabic, Arabic]),
            // Extended Arabic (peh, U+067E) is strong Arabic by class
            // AL, exactly like the standard letters.
            ("پ 123", &[Arabic, Arabic]),
            // An isolate seals its interior (UAX #9 P2): the Arabic
            // word inside it must not decide the digits' shapes. The
            // initiator takes the outer level, so it rides with an
            // outer run.
            ("\u{2066}كتاب\u{2069} 123", &[Plain, Arabic, Plain]),
            // The forward fallback skips an isolate the same way: the
            // decisive strong character is the Latin 'a' after the
            // matching PDI, not the Arabic letters inside.
            ("123\u{2067}كتاب\u{2069}abc", &[Plain, Arabic, Plain]),
            // An Arabic-block neutral (U+060C ARABIC COMMA, class CS)
            // merges into a Latin run and must not arabize it.
            ("Price 5\u{060C} Total 10", &[Plain]),
        ];
        for (text, expected) in cases {
            let bidi = BidiInfo::new(text, None);
            let para = &bidi.paragraphs[0];
            let got: Vec<RunContext> = level_runs(&bidi, para)
                .iter()
                .map(|r| run_context(&bidi, para, r))
                .collect();
            assert_eq!(&got, expected, "run contexts of {text:?}");
        }
    }
}
