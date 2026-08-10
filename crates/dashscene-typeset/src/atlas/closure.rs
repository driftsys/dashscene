//! Charset → glyph-id closure (docs/design/architecture.md).
//!
//! An atlas must cover every glyph a shaped run can produce from the
//! declared charset. Two sources feed the closure:
//!
//! - **cmap** — the nominal glyph id of each charset codepoint.
//! - **GSUB** — the contextual forms, mark forms, and ligatures that
//!   shaping produces. rustybuzz exposes shaping but no standalone
//!   glyph-closure operation, so the closure shapes the charset in the
//!   contexts that trigger those substitutions and unions the output
//!   glyph ids — spike #25's proven method
//!   (docs/technotes/arabic-atlas-coverage.md). Each character is
//!   shaped in the four Arabic joining contexts
//!   (isolated/initial/medial/final), each haraka on a base letter,
//!   and every ordered character pair (for ligatures such as lam-alef
//!   and the Latin `fi`).
//!
//! The closure shapes with the default OpenType feature set, so
//! ligatures are on — the same configuration production shaping uses
//! for Arabic-context runs, while other runs keep `liga`/`clig`
//! disabled (docs/decisions/liga-clig-off-until-gsub-closure.md).
//! Ligatures longer than two characters (for example `ffi`, or the
//! Allah ligature) are outside the pairwise sweep; a shaped run that
//! reaches one is the painter's named missing-glyph diagnostic (#30),
//! never a silent drop.
//!
//! Direction coupling: the closure shapes each run in its natural
//! direction (guessed from the text), so Arabic shapes right-to-left.
//! Coverage therefore assumes the production shaper also shapes Arabic
//! in its natural direction, which holds as-built: #32's seam shapes
//! each UAX #9 level run with its resolved direction, and #33 shapes
//! Arabic-context runs with the same default feature set this closure
//! uses. The #33 acceptance test pins that coupling — production-shaped
//! output is a subset of this coverage for the declared charset
//! (tests/typeset_arabic.rs).
//!
//! Codepoints the font's cmap cannot represent are reported in
//! `missing_codepoints` (a named diagnostic, R6), never dropped.

use std::collections::BTreeSet;

use rustybuzz::{Face, UnicodeBuffer};

/// The glyph-id set an atlas must cover, plus the charset entries the
/// font cannot represent (a named diagnostic surface, R6 — the caller
/// decides severity, nothing is dropped silently).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    /// Sorted, deduplicated; always contains glyph id 0 (`.notdef`) so
    /// painters can draw a visible fallback for unmapped input.
    pub glyph_ids: Vec<u16>,
    /// Charset codepoints without a cmap entry, ascending.
    pub missing_codepoints: Vec<u32>,
}

/// Beh (U+0628), a dual-joining Arabic letter. Placing a target letter
/// next to it forces the joining context whose contextual form the
/// atlas must carry — the connector spike #25's sweep used.
const JOINING_CONNECTOR: char = '\u{0628}';

/// Standard Arabic letters (hamza … yeh), which get the connector sweep
/// through the four joining contexts; matches spike #25's sweep range.
/// An extended-Arabic codepoint (Persian/Urdu letters, presentation
/// forms) is out of v0.6 scope: it still receives its isolated form, its
/// ligature forms, and — when it is the second character of a pair whose
/// first character is in this range — the contextual forms that pair
/// sweep reaches incidentally, but it gets no joining-context sweep of
/// its own.
pub const ARABIC_LETTERS: std::ops::RangeInclusive<u32> = 0x0621..=0x064A;

/// Standard Arabic harakat (fathatan … sukun) placed on a base letter;
/// matches spike #25.
pub const ARABIC_HARAKAT: std::ops::RangeInclusive<u32> = 0x064B..=0x0652;

/// Resolves `charset` through the font's cmap, adds the GSUB forms
/// shaping can produce from it, and merges `extra_glyph_ids`.
pub fn charset_closure(
    face: &Face<'_>,
    charset: &BTreeSet<char>,
    extra_glyph_ids: &BTreeSet<u16>,
) -> Closure {
    // Production shaping displays a European digit with its
    // Arabic-Indic counterpart when the digit sits in Arabic context
    // (the `text` module's digit-shape rule, story #33), so a charset
    // that declares strong Arabic characters next to European digits
    // must cover the Arabic-Indic digit glyphs too. Trigger and
    // mapping are the text module's own functions — one definition,
    // so this derivation cannot drift from the production rule. The
    // derived digits join the charset for both cmap and GSUB, so a
    // font without them reports the gap in `missing_codepoints` like
    // any declared codepoint.
    let mut charset = charset.clone();
    if charset.iter().copied().any(crate::text::is_arabic_strong) {
        let derived: Vec<char> = charset
            .iter()
            .filter(|c| c.is_ascii_digit())
            .map(|&c| crate::text::arabic_indic_digit(c))
            .collect();
        charset.extend(derived);
    }
    let charset = &charset;

    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0);
    let mut missing = Vec::new();

    // cmap: nominal glyph per codepoint. `rustybuzz::Face` derefs to
    // `ttf_parser::Face`, so the lookup is unchanged from v0.5.
    for &c in charset {
        match face.glyph_index(c) {
            Some(gid) => {
                gids.insert(gid.0);
            }
            None => missing.push(c as u32),
        }
    }

    // GSUB: the forms shaping produces from the charset.
    add_gsub_forms(face, charset, &mut gids);

    gids.extend(extra_glyph_ids.iter().copied());
    Closure {
        glyph_ids: gids.into_iter().collect(),
        missing_codepoints: missing,
    }
}

/// Shapes the charset in the contexts that trigger GSUB substitution
/// and unions every output glyph id into `gids`.
fn add_gsub_forms(face: &Face<'_>, charset: &BTreeSet<char>, gids: &mut BTreeSet<u16>) {
    let cc = JOINING_CONNECTOR;
    for &c in charset {
        let cp = c as u32;
        if ARABIC_HARAKAT.contains(&cp) {
            // A haraka on an isolated base and on a joined (medial)
            // base; a lone mark would only shape to a dotted-circle
            // placeholder that no charset codepoint declares.
            shape_into(face, &format!("{cc}{c}"), gids);
            shape_into(face, &format!("{cc}{cc}{c}"), gids);
        } else {
            // The isolated form of every other character. For Arabic
            // this is a real GSUB form (Noto Sans Arabic composes
            // dotless skeletons with separate dots even in isolation —
            // spike #25); for Latin it equals the cmap glyph.
            shape_into(face, &c.to_string(), gids);
            if ARABIC_LETTERS.contains(&cp) {
                sweep_joining_contexts(face, &c.to_string(), gids);
            }
        }
    }

    // Ligatures (lam-alef, Latin `fi`, ...) are reachable only from
    // their own character sequence, never from the fixed connector, so
    // shape every ordered pair of the charset. An Arabic ligature is a
    // letter-like unit with its own joining behaviour — a lam-alef
    // preceded by a joining letter takes a different glyph than the
    // isolated one — so a pair that starts with an Arabic letter is
    // also swept through the joining contexts.
    let mut pair = String::new();
    for &a in charset {
        for &b in charset {
            pair.clear();
            pair.push(a);
            pair.push(b);
            shape_into(face, &pair, gids);
            if ARABIC_LETTERS.contains(&(a as u32)) {
                sweep_joining_contexts(face, &pair, gids);
            }
        }
    }
}

/// Shapes `unit` (a letter or a ligature-forming sequence) with a
/// dual-joining connector before it, after it, and on both sides, so it
/// receives its final, initial, and medial contextual forms.
fn sweep_joining_contexts(face: &Face<'_>, unit: &str, gids: &mut BTreeSet<u16>) {
    let cc = JOINING_CONNECTOR;
    shape_into(face, &format!("{cc}{unit}"), gids);
    shape_into(face, &format!("{unit}{cc}"), gids);
    shape_into(face, &format!("{cc}{unit}{cc}"), gids);
}

/// Shapes `text` with the default OpenType feature set and inserts
/// every output glyph id.
fn shape_into(face: &Face<'_>, text: &str, gids: &mut BTreeSet<u16>) {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let output = rustybuzz::shape(face, &[], buffer);
    for info in output.glyph_infos() {
        // TrueType glyph ids are u16; rustybuzz widens to u32.
        gids.insert(info.glyph_id as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const FONT: &str = crate::atlas::TEST_FONT;
    const FONT_ARABIC: &str = crate::atlas::TEST_FONT_ARABIC;

    fn face(data: &[u8]) -> Face<'_> {
        Face::from_slice(data, 0).expect("fixture font parses")
    }

    /// Shapes `text` with default features and returns its output glyph
    /// ids — the independent oracle the coverage tests check against.
    fn shaped_gids(face: &Face<'_>, text: &str) -> Vec<u16> {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let output = rustybuzz::shape(face, &[], buffer);
        output
            .glyph_infos()
            .iter()
            .map(|i| i.glyph_id as u16)
            .collect()
    }

    #[test]
    fn resolves_covered_codepoints_to_sorted_unique_gids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['B', 'A', 'A', 'a'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert!(c.missing_codepoints.is_empty());
        // Latin letters have no contextual forms and A/B/a form no
        // ligature, so the set is exactly .notdef plus one gid per char.
        assert_eq!(c.glyph_ids.len(), 4);
        assert_eq!(c.glyph_ids[0], 0);
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, c.glyph_ids, "sorted and deduplicated");
        assert!(c.glyph_ids[1..].iter().all(|&g| g != 0));
    }

    #[test]
    fn reports_uncovered_codepoints_sorted() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        // Syriac letters — absent from a Latin/Greek/Cyrillic font.
        let charset: BTreeSet<char> = ['\u{0712}', '\u{0710}', 'A'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert_eq!(c.missing_codepoints, vec![0x0710, 0x0712]);
        // Uncovered codepoints shape to .notdef, adding no glyph; the
        // set is .notdef plus 'A'.
        assert_eq!(c.glyph_ids.len(), 2);
    }

    #[test]
    fn merges_extra_glyph_ids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['A'].into_iter().collect();
        let extras: BTreeSet<u16> = [700u16, 3].into_iter().collect();
        let c = charset_closure(&face, &charset, &extras);
        assert!(c.glyph_ids.contains(&700));
        assert!(c.glyph_ids.contains(&3));
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, c.glyph_ids);
    }

    #[test]
    fn latin_ligature_fi_is_covered() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['f', 'i'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        // With default features "fi" shapes to a single ligature glyph
        // that is neither cmap('f') nor cmap('i').
        let fi = shaped_gids(&face, "fi");
        assert_eq!(fi.len(), 1, "fi must ligate with default features");
        let ligature = fi[0];
        let f = face.glyph_index('f').unwrap().0;
        let i = face.glyph_index('i').unwrap().0;
        assert!(ligature != f && ligature != i, "fi is a GSUB-only glyph");
        assert!(
            c.glyph_ids.contains(&ligature),
            "closure must cover the fi ligature ({ligature})"
        );
    }

    /// The core Arabic claim: an atlas built from a declared Arabic
    /// charset covers the glyphs that real words composed from that
    /// charset shape to — including the GSUB-only contextual forms and
    /// dots that carry no cmap entry.
    #[test]
    fn gsub_closure_covers_shaped_arabic_words() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        // Words with no three-or-more character ligature.
        let words = ["مرحبا", "بيت", "كتاب", "سلام", "درجة", "مدرسة", "١٢٣"];
        // The charset is exactly the words' characters, so the O(n^2)
        // pairwise sweep stays small while the assertion still exercises
        // the contextual forms and the lam-alef ligature.
        let charset: BTreeSet<char> = words.iter().flat_map(|w| w.chars()).collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        let covered: BTreeSet<u16> = c.glyph_ids.iter().copied().collect();

        for word in words {
            for gid in shaped_gids(&face, word) {
                assert!(
                    covered.contains(&gid),
                    "word {word:?} shapes to gid {gid}, absent from the closure"
                );
            }
        }
    }

    /// The closure must add glyphs cmap alone cannot reach — otherwise
    /// it would not carry Arabic's contextual forms at all.
    #[test]
    fn gsub_closure_adds_forms_beyond_cmap() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = "بتنيسملاه".chars().collect();
        let cmap_only: BTreeSet<u16> = charset
            .iter()
            .filter_map(|&ch| face.glyph_index(ch).map(|g| g.0))
            .chain(std::iter::once(0))
            .collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        let gsub_only: Vec<u16> = c
            .glyph_ids
            .iter()
            .copied()
            .filter(|g| !cmap_only.contains(g))
            .collect();
        assert!(
            !gsub_only.is_empty(),
            "GSUB closure added no glyph beyond cmap"
        );
    }

    /// Lam-alef is a mandatory Arabic ligature reachable only from the
    /// lam+alef sequence; the pairwise sweep must catch it.
    #[test]
    fn gsub_closure_covers_lam_alef_ligature() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['\u{0644}', '\u{0627}'].into_iter().collect(); // lam, alef
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        let covered: BTreeSet<u16> = c.glyph_ids.iter().copied().collect();
        let lamalef = shaped_gids(&face, "\u{0644}\u{0627}");
        let lam = face.glyph_index('\u{0644}').unwrap().0;
        let alef = face.glyph_index('\u{0627}').unwrap().0;
        assert!(
            lamalef.iter().any(|g| *g != lam && *g != alef),
            "lam-alef must produce a GSUB-only form"
        );
        for gid in lamalef {
            assert!(
                covered.contains(&gid),
                "lam-alef shapes to gid {gid}, absent from the closure"
            );
        }
    }

    /// Production shaping displays European digits with Arabic-Indic
    /// glyphs in Arabic context, so an Arabic charset declaring
    /// European digits must cover the counterpart glyphs.
    #[test]
    fn arabic_charset_with_european_digits_covers_arabic_indic_glyphs() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['\u{0628}', '1', '2'].into_iter().collect(); // beh, 1, 2
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        for d in ['\u{0661}', '\u{0662}'] {
            let gid = face.glyph_index(d).unwrap().0;
            assert!(
                c.glyph_ids.contains(&gid),
                "derived Arabic-Indic digit {d:?} (gid {gid}) must be covered"
            );
        }
    }

    /// The derivation trigger is production's own predicate — any
    /// strong Arabic (AL-classed) character — not the standard-letter
    /// sweep range: extended Arabic (peh, U+067E) anchors a digit run
    /// exactly like a standard letter, so the coverage must follow.
    #[test]
    fn extended_arabic_with_european_digits_covers_arabic_indic_glyphs() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['\u{067E}', '1', '2', '3', ' '].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        for d in ['\u{0661}', '\u{0662}', '\u{0663}'] {
            let gid = face.glyph_index(d).unwrap().0;
            assert!(
                c.glyph_ids.contains(&gid),
                "derived Arabic-Indic digit {d:?} (gid {gid}) must be covered"
            );
        }
    }

    /// Without an Arabic letter in the charset, no digit context can
    /// resolve Arabic, so no counterpart glyphs are derived.
    #[test]
    fn digits_without_arabic_letters_derive_no_arabic_indic_glyphs() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['1', '2'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        for d in ['\u{0661}', '\u{0662}'] {
            let gid = face.glyph_index(d).unwrap().0;
            assert!(!c.glyph_ids.contains(&gid), "{d:?} must not be derived");
        }
    }

    /// A haraka on a base letter (its GPOS-positioned mark glyph) is
    /// covered by the base+mark sweep.
    #[test]
    fn gsub_closure_covers_haraka_on_base() {
        let data = std::fs::read(FONT_ARABIC).expect("arabic fixture present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['\u{0628}', '\u{064E}'].into_iter().collect(); // beh, fatha
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        let covered: BTreeSet<u16> = c.glyph_ids.iter().copied().collect();
        for gid in shaped_gids(&face, "\u{0628}\u{064E}") {
            assert!(
                covered.contains(&gid),
                "beh+fatha shapes to gid {gid}, absent from the closure"
            );
        }
    }
}
