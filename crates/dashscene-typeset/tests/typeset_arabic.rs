//! Arabic pipeline acceptance (issue #33): Arabic-context runs shape
//! over the #32 seam into contextual forms, required ligatures, and
//! GPOS-positioned harakat; digit shapes resolve from context; and
//! production-shaped output stays within the declared charset's atlas
//! coverage — the direction/feature coupling with the `atlas` module.
//! Glyph-id pins are the committed Noto Sans Arabic fixture's values,
//! cross-checked against spike #25
//! (`docs/technotes/msdf-arabic-atlas-spike.md`).

use std::collections::BTreeSet;

use dashscene_typeset::atlas::charset_closure;
use dashscene_typeset::text::{Line, TextShape, Typesetter};

mod common;

use common::FONT_ARABIC;

fn font_data() -> Vec<u8> {
    common::font_data(FONT_ARABIC)
}

fn typesetter() -> Typesetter {
    common::typesetter(FONT_ARABIC)
}

fn cmap(c: char) -> u16 {
    common::cmap(FONT_ARABIC, c)
}

/// The x position of the line's one glyph with the given id.
fn x_of(line: &Line, glyph_id: u16) -> f32 {
    let mut hits = line.glyphs.iter().filter(|g| g.glyph_id == glyph_id);
    let x = hits.next().expect("glyph present").x;
    assert!(hits.next().is_none(), "glyph id not unique on the line");
    x
}

fn line_gids(line: &Line) -> Vec<u16> {
    line.glyphs.iter().map(|g| g.glyph_id).collect()
}

#[test]
fn arabic_word_lays_out_with_contextual_forms() {
    // "كتاب" in display (left-to-right) order: beh dot + isolated beh
    // skeleton (alef joins to no following letter), final alef, teh
    // dots + medial teh skeleton, initial kaf. Every glyph is a
    // GSUB-composed form — Noto Sans Arabic builds letters from
    // dotless skeletons plus dot glyphs (spike #25), so not one
    // nominal cmap glyph of the four letters appears.
    let mut ts = typesetter();
    let l = ts.layout("كتاب", 16.0, None);
    assert_eq!(l.lines.len(), 1);
    let ids = line_gids(&l.lines[0]);
    assert_eq!(ids, vec![316, 14, 9, 287, 18, 61]);
    for c in "كتاب".chars() {
        assert!(!ids.contains(&cmap(c)), "nominal cmap glyph of {c:?}");
    }
}

#[test]
fn marks_position_through_gpos_offsets_in_document_space() {
    // The first real exercise of the y-offset negation (document
    // space is y-down, shaping offsets y-up — v0.5 Latin glyphs all
    // carry zero offsets). Lam + shadda + fatha stacks two marks
    // above the letter through GPOS mark-to-base and mark-to-mark:
    // the fixture's y offsets are 456 (fatha, gid 370) and 256
    // (shadda, gid 367) font units up, so in document space the fatha
    // sits highest. Beh's dot glyph (gid 316) carries a negative
    // y offset (-23) and lands below the baseline.
    let mut ts = typesetter();
    let size = 16.0;
    let scale = size / f32::from(ts.font().units_per_em());
    let l = ts.layout("\u{0644}\u{0651}\u{064E}", size, None); // lam shadda fatha
    let line = &l.lines[0];
    let lam = line.glyphs.iter().find(|g| g.glyph_id == 68).unwrap();
    let shadda = line.glyphs.iter().find(|g| g.glyph_id == 367).unwrap();
    let fatha = line.glyphs.iter().find(|g| g.glyph_id == 370).unwrap();
    assert_eq!(lam.y, line.baseline_y, "the letter sits on the baseline");
    assert!(
        fatha.y < shadda.y && shadda.y < lam.y,
        "marks must stack upward: fatha {} shadda {} lam {}",
        fatha.y,
        shadda.y,
        lam.y
    );
    assert!(
        (line.baseline_y - fatha.y - 456.0 * scale).abs() < 1e-3,
        "fatha height must be its GPOS offset, scaled"
    );

    let l = ts.layout("\u{0628}", size, None); // beh
    let line = &l.lines[0];
    let dot = line.glyphs.iter().find(|g| g.glyph_id == 316).unwrap();
    assert!(
        dot.y > line.baseline_y,
        "beh's dot must land below the baseline (negative y offset)"
    );
}

#[test]
fn arabic_indic_digits_keep_ltr_order_in_rtl_text() {
    // Logical "سرعة ١٢٣" displays as [١ ٢ ٣][␣ ة ع ر س]: the digits
    // keep ascending order left of the word.
    let mut ts = typesetter();
    let l = ts.layout("سرعة ١٢٣", 16.0, None);
    assert_eq!(l.lines.len(), 1);
    let line = &l.lines[0];
    let x1 = x_of(line, cmap('١'));
    let x2 = x_of(line, cmap('٢'));
    let x3 = x_of(line, cmap('٣'));
    assert!(x1 < x2 && x2 < x3, "digits keep logical order");
    let digits = [cmap('١'), cmap('٢'), cmap('٣')];
    for g in line.glyphs.iter().filter(|g| !digits.contains(&g.glyph_id)) {
        assert!(
            g.x > x3,
            "the word (and its space) displays right of the digits"
        );
    }
}

#[test]
fn european_digits_display_as_arabic_indic_in_arabic_text() {
    // Authored "123" after an Arabic word takes Arabic-Indic display
    // shapes — the context-derived digit selection. The authored
    // codepoints stay in the document; only the glyphs change.
    let mut ts = typesetter();
    let l = ts.layout("سرعة 123", 16.0, None);
    let line = &l.lines[0];
    let x1 = x_of(line, cmap('١'));
    let x3 = x_of(line, cmap('٣'));
    assert!(x1 < x3, "substituted digits keep logical order");
    for d in ['1', '2', '3'] {
        assert!(
            !line_gids(line).contains(&cmap(d)),
            "European digit glyph {d:?} must not appear"
        );
    }
}

#[test]
fn a_number_opening_an_arabic_paragraph_takes_arabic_indic_shapes() {
    // No strong character precedes the digits, so the following
    // Arabic word decides.
    let mut ts = typesetter();
    let l = ts.layout("123 سرعة", 16.0, None);
    let ids = line_gids(&l.lines[0]);
    for d in ['١', '٢', '٣'] {
        assert!(ids.contains(&cmap(d)), "{d:?} expected");
    }
    for d in ['1', '2', '3'] {
        assert!(!ids.contains(&cmap(d)), "{d:?} must not appear");
    }
}

#[test]
fn an_isolate_does_not_leak_arabic_context_to_digits() {
    // UAX #9 P2: an isolate seals its interior. The Arabic word
    // inside LRI..PDI must not decide the digits' display shapes —
    // the backward scan skips the isolate and finds nothing.
    let mut ts = typesetter();
    let l = ts.layout("\u{2066}كتاب\u{2069} 123", 16.0, None);
    let ids: Vec<u16> = l.lines[0].glyphs.iter().map(|g| g.glyph_id).collect();
    assert!(ids.contains(&cmap('1')), "digits must stay European");
    assert!(!ids.contains(&cmap('١')), "no Arabic-Indic substitution");
}

#[test]
fn the_forward_scan_skips_an_isolate_too() {
    // Mirror case: digits open the paragraph, and the nearest
    // reachable strong character is the Latin 'a' after the isolate,
    // not the Arabic letters sealed inside it.
    let mut ts = typesetter();
    let l = ts.layout("123\u{2067}كتاب\u{2069}abc", 16.0, None);
    let ids: Vec<u16> = l
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter().map(|g| g.glyph_id))
        .collect();
    assert!(ids.contains(&cmap('1')), "digits must stay European");
    assert!(!ids.contains(&cmap('١')), "no Arabic-Indic substitution");
}

#[test]
fn an_arabic_comma_does_not_arabize_a_latin_run() {
    // U+060C ARABIC COMMA is a neutral (bidi class CS): it merges
    // into the Latin run and must not flip the run's posture — the
    // digits keep their European shapes.
    let mut ts = typesetter();
    let l = ts.layout("Price 5\u{060C} Total 10", 16.0, None);
    let ids: Vec<u16> = l.lines[0].glyphs.iter().map(|g| g.glyph_id).collect();
    assert!(ids.contains(&cmap('5')), "digits must stay European");
    assert!(ids.contains(&cmap('0')), "digits must stay European");
    assert!(!ids.contains(&cmap('٥')), "no Arabic-Indic substitution");
    assert!(!ids.contains(&cmap('٠')), "no Arabic-Indic substitution");
}

#[test]
fn digits_without_an_arabic_anchor_stay_european() {
    // No strong character at all: authored shapes stay.
    let mut ts = typesetter();
    let l = ts.layout("123", 16.0, None);
    assert_eq!(
        line_gids(&l.lines[0]),
        vec![cmap('1'), cmap('2'), cmap('3')]
    );
    // Hebrew is strong non-Arabic: European shapes stay too (the
    // Hebrew letters themselves are .notdef in this fixture font —
    // the assertion reads the digit glyphs only).
    let l = ts.layout("אב 123", 16.0, None);
    let ids = line_gids(&l.lines[0]);
    assert!(ids.contains(&cmap('1')));
    assert!(!ids.contains(&cmap('١')));
}

/// The #33 join's coupling pin (docs/design/atlas-pipeline.md,
/// Charset closure): the closure shapes the declared charset with
/// natural direction and default features; production shapes per
/// UAX #9 level run with context-derived features and digit shapes.
/// Every glyph id production lays out for charset-composed text must
/// be inside the closure's coverage — a failure means the two modules
/// drifted on direction, feature set, or digit selection.
///
/// The charset here is the corpus's own characters, keeping the
/// closure's pairwise sweep small enough for every `cargo test` run;
/// the full-charset E2 pin runs in CI's atlas-repro job
/// (`tests/atlas_pipeline.rs`).
#[test]
fn production_shaped_output_stays_within_declared_charset_coverage() {
    // Words, harakat, lam-alef, both digit systems embedded in words,
    // a number opening a paragraph, extended Arabic (peh) anchoring
    // European digits, unanchored Arabic-Indic digits.
    let corpus = [
        "كتاب",
        "سلام",
        "لا",
        "سَلَامٌ",
        "سرعة ١٢٣",
        "سرعة 123",
        "123 سرعة",
        "پ 123",
        "٤٥",
    ];
    let charset: BTreeSet<char> = corpus.iter().flat_map(|s| s.chars()).collect();

    let data = font_data();
    let face = rustybuzz::Face::from_slice(&data, 0).expect("parses");
    let closure = charset_closure(&face, &charset, &BTreeSet::new());
    assert!(closure.missing_codepoints.is_empty());
    let covered: BTreeSet<u16> = closure.glyph_ids.iter().copied().collect();

    let mut ts = typesetter();
    for text in corpus {
        let l = ts.layout(text, 16.0, None);
        for line in &l.lines {
            for g in &line.glyphs {
                assert!(
                    covered.contains(&g.glyph_id),
                    "{text:?} lays out glyph id {} outside the declared \
                     charset's coverage",
                    g.glyph_id
                );
            }
        }
    }
}

/// Debt issue #353: the #33 coupling pin above never exercised
/// `ligatures_off=true` under an Arabic `RunContext`, through the real
/// bidi/font-fallback production path (`Typesetter::layout_with`), only
/// through the charset closure itself (which always shapes with
/// ligatures on) and through a direct, non-production `shape_with_face`
/// call in `text::shape`'s own unit tests. Two review passes on PR #350
/// argued this is safe by construction: `charset_closure` already shapes
/// every character and joined pair with the full default feature set
/// (`liga`/`clig` included), so the glyph ids a ligature produces are
/// already in the closure's coverage, and turning `liga`/`clig` off can
/// only make a run fall back to its already-covered, non-ligated
/// components — never reach an uncovered glyph id. This test exercises
/// that path directly rather than leaving it an argument.
#[test]
fn ligatures_off_arabic_output_stays_within_declared_charset_coverage() {
    // Same corpus as the coupling pin above, so the coverage set is
    // identical and only the shaping knob differs.
    let corpus = [
        "كتاب",
        "سلام",
        "لا",
        "سَلَامٌ",
        "سرعة ١٢٣",
        "سرعة 123",
        "123 سرعة",
        "پ 123",
        "٤٥",
    ];
    let charset: BTreeSet<char> = corpus.iter().flat_map(|s| s.chars()).collect();

    let data = font_data();
    let face = rustybuzz::Face::from_slice(&data, 0).expect("parses");
    let closure = charset_closure(&face, &charset, &BTreeSet::new());
    assert!(closure.missing_codepoints.is_empty());
    let covered: BTreeSet<u16> = closure.glyph_ids.iter().copied().collect();

    let shape = TextShape {
        ligatures_off: true,
        ..Default::default()
    };
    let mut ts = typesetter();
    for text in corpus {
        let l = ts.layout_with(text, 16.0, None, shape);
        for line in &l.lines {
            for g in &line.glyphs {
                assert!(
                    covered.contains(&g.glyph_id),
                    "{text:?} with ligatures_off lays out glyph id {} outside \
                     the declared charset's coverage",
                    g.glyph_id
                );
            }
        }
    }
}

// The RTL width-vs-bounds contract (issue #224,
// `docs/decisions/rtl-text-width-is-the-placed-extent.md`). `TextLayout::width`
// is the content advance — the widest line's pen advance — which is the
// hug-sizing datum the measure seam (#29) reads. The invariant is over
// ADVANCE boxes: pen positions fit `[0, width]`. Glyph INK (bearings, GPOS
// mark offsets) may overhang the box; the painter does not clip to it, and a
// consumer must not either. These run on the Arabic fixture font so real
// marks with GPOS offsets flow through the assertions.

fn glyph_x_extent(layout: &dashscene_typeset::text::TextLayout) -> (f32, f32) {
    let xs = layout
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .map(|g| g.x);
    xs.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
        (lo.min(x), hi.max(x))
    })
}

#[test]
fn hug_advance_box_holds_and_mark_ink_may_overhang() {
    // The hug case: the measure seam passes `max_width = None` and takes
    // `TextLayout::width` as the box. reh + kasra — the reh advances the whole
    // box (its pen position sits at the origin, inside the box), while the
    // kasra is a zero-advance mark whose GPOS offset places its ink just left
    // of the origin, at x ~= -0.08 (measured, size 16). So the advancing base
    // stays in `[0, width]` and the mark ink overhangs the left edge by a
    // sub-pixel amount — real, but never a gross escape. This is the ink
    // posture the decision records: a bounds field would not change it,
    // because the painter reads per-glyph positions, not `width`.
    let mut ts = typesetter();
    let laid = ts.layout("رِ", 16.0, None);
    assert_eq!(laid.lines.len(), 1);
    assert!(laid.width > 1.0, "the reh advances the box: {}", laid.width);

    let (min, max) = glyph_x_extent(&laid);
    assert!(
        min < 0.0,
        "the kasra's ink overhangs the box origin (the ink posture): min={min}",
    );
    assert!(
        min > -1.0,
        "the overhang is sub-pixel, not a gross escape: min={min}",
    );
    assert!(
        max <= laid.width + 1e-3,
        "advancing glyphs stay within the advance box [0, {}]: max={max}",
        laid.width,
    );
}

#[test]
fn a_fixed_width_rtl_box_is_bounded_by_its_authored_width_not_by_width() {
    // The fixed-width case: with a box `w` wider than the line, RTL glyphs
    // flush right in `(w - line, w]`, reaching past `TextLayout::width` — which
    // stays the line's own advance, unchanged by the wider box. A consumer
    // that fixes a box's width must bound it by that authored `w` (the value it
    // passed as `max_width`), never by `width`. A mark-free word keeps the
    // advance box clean.
    let mut ts = typesetter();
    let natural = ts.layout("كتاب", 16.0, None).width;
    let w = natural * 2.0;
    let laid = ts.layout("كتاب", 16.0, Some(w));
    assert!(
        (laid.width - natural).abs() < 1e-2,
        "width stays the content advance ({natural}), not the box ({w})",
    );
    let (min, max) = glyph_x_extent(&laid);
    assert!(
        max > laid.width,
        "flush-right glyphs reach past `width` ({}) — a [0,width] bound would clip them: max={max}",
        laid.width,
    );
    assert!(
        max <= w + 1e-2 && min >= -1e-2,
        "but every glyph stays within the authored box [0, {w}]: min={min} max={max}",
    );
}
