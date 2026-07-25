//! Story #385: font family. A `Typesetter` built with
//! `with_named_font_families` carries a name per family, and
//! `layout_styled` probes the family a document asked for ahead of the
//! rest of the cascade — step 2 of `docs/decisions/font-resolution-order.md`,
//! which makes `TextStyle::family` a field that affects the render rather
//! than one the runtime carries and ignores.
//!
//! Selection is family, then coverage, then weight. Family matching only
//! reorders which family is probed first; coverage still decides which
//! family shapes each codepoint, so naming a family never costs a reader
//! their text.
//!
//! The pre-#385 path is unchanged: the unnamed constructors declare empty
//! names and `layout`/`layout_with`/`layout_weighted` request no family,
//! so every existing call site shapes, measures and tags exactly as before.

use dashscene_typeset::text::{Font, FontFamily, TextShape, Typesetter, WeightedFont};

mod common;

use common::{FONT, FONT_ARABIC, FONT_BOLD, FONT_INTER, FONT_INTER_BOLD, font_data};

fn face(path: &str) -> Font {
    Font::from_bytes(font_data(path), 0).expect("fixture font loads")
}

/// The production shape of the cascade after this story: two Latin
/// families that both cover ASCII, plus an Arabic family that neither
/// Latin family covers.
fn cascade() -> Typesetter {
    Typesetter::with_named_font_families(vec![
        FontFamily::new(
            "Noto Sans",
            vec![
                WeightedFont::new(face(FONT), 400),
                WeightedFont::new(face(FONT_BOLD), 700),
            ],
        ),
        FontFamily::new(
            "Inter",
            vec![
                WeightedFont::new(face(FONT_INTER), 400),
                WeightedFont::new(face(FONT_INTER_BOLD), 700),
            ],
        ),
        FontFamily::new(
            "Noto Sans Arabic",
            vec![WeightedFont::regular(face(FONT_ARABIC))],
        ),
    ])
}

/// The distinct flat slots a layout's glyphs were shaped by, in first-seen
/// order — which face actually rendered, which is the only question these
/// tests ask.
fn slots_used(ts: &mut Typesetter, text: &str, weight: u16, family: &str) -> Vec<u16> {
    let laid = ts.layout_styled(text, 32.0, None, TextShape::default(), weight, family);
    let mut out: Vec<u16> = Vec::new();
    for glyph in laid.lines.iter().flat_map(|line| &line.glyphs) {
        if !out.contains(&glyph.font) {
            out.push(glyph.font);
        }
    }
    out
}

// Flat slots, family-major: Noto 400/700 are 0/1, Inter 400/700 are 2/3,
// Arabic 400 is 4.
const NOTO_400: u16 = 0;
const NOTO_700: u16 = 1;
const INTER_400: u16 = 2;
const INTER_700: u16 = 3;
const ARABIC_400: u16 = 4;

#[test]
fn a_named_family_shapes_the_run_even_though_an_earlier_family_covers_it() {
    // The whole point of the story: Noto Sans covers ASCII and comes first,
    // so before family matching every Latin run resolved there. Asking for
    // Inter must now reach Inter.
    let mut ts = cascade();
    assert_eq!(slots_used(&mut ts, "Aa", 400, "Inter"), vec![INTER_400]);
    assert_eq!(slots_used(&mut ts, "Aa", 400, "Noto Sans"), vec![NOTO_400]);
}

#[test]
fn family_selects_before_weight_selects_the_face() {
    // Family picks the family; the CSS Fonts 4 rule then picks the face
    // inside it. A bold Inter run must be Inter's bold face, not Noto's.
    let mut ts = cascade();
    assert_eq!(slots_used(&mut ts, "Aa", 700, "Inter"), vec![INTER_700]);
    assert_eq!(slots_used(&mut ts, "Aa", 700, "Noto Sans"), vec![NOTO_700]);
}

#[test]
fn naming_a_family_never_costs_the_reader_an_uncovered_codepoint() {
    // Coverage still outranks the family preference: Inter has no Arabic,
    // so an Arabic run under a request for Inter falls through to the
    // family that covers it rather than rendering .notdef.
    let mut ts = cascade();
    assert_eq!(
        slots_used(&mut ts, "\u{0645}\u{0631}", 400, "Inter"),
        vec![ARABIC_400]
    );
}

#[test]
fn a_mixed_run_splits_between_the_named_family_and_the_covering_one() {
    let mut ts = cascade();
    let used = slots_used(&mut ts, "A\u{0645}", 400, "Inter");
    assert!(
        used.contains(&INTER_400) && used.contains(&ARABIC_400),
        "the Latin half shapes in the named family and the Arabic half in \
         the one that covers it, got {used:?}"
    );
    assert!(!used.contains(&NOTO_400), "got {used:?}");
}

#[test]
fn an_unknown_family_resolves_by_coverage_and_is_reported_not_refused() {
    // P4: a gap the renderer's asset set cannot fill is a named
    // diagnostic, never a silent drop and never a hard error — committed
    // fixtures name families the corpus does not carry.
    let mut ts = cascade();
    assert_eq!(slots_used(&mut ts, "Aa", 400, "Helvetica"), vec![NOTO_400]);
    let reports = ts.family_substitutions();
    assert_eq!(reports.len(), 1, "got {reports:?}");
    assert_eq!(reports[0].requested, "Helvetica");
    assert_eq!(reports[0].resolved, "Noto Sans");
    assert!(
        reports[0]
            .to_string()
            .starts_with("text.family-substituted:"),
        "got {}",
        reports[0]
    );
}

#[test]
fn a_family_found_but_not_covering_still_reports_the_family_that_stood_in() {
    // Inter is in the cascade, so the Latin half is not substituted; the
    // Arabic half shaped elsewhere, and that is a real substitution the
    // renderer names.
    let mut ts = cascade();
    slots_used(&mut ts, "A\u{0645}", 400, "Inter");
    let reports = ts.family_substitutions();
    assert_eq!(reports.len(), 1, "got {reports:?}");
    assert_eq!(reports[0].requested, "Inter");
    assert_eq!(reports[0].resolved, "Noto Sans Arabic");
}

#[test]
fn a_family_that_shaped_nothing_is_never_reported() {
    // Reporting is driven by the output, not the resolution: a pure-Latin
    // run under a request Inter answers resolves every family in the
    // cascade, but only Inter shaped anything.
    let mut ts = cascade();
    slots_used(&mut ts, "Aa", 400, "Inter");
    assert!(ts.family_substitutions().is_empty());
}

#[test]
fn reports_are_deduplicated_per_requested_resolved_pair() {
    let mut ts = cascade();
    for _ in 0..3 {
        slots_used(&mut ts, "Aa", 400, "Helvetica");
        slots_used(&mut ts, "Bb", 400, "Helvetica");
    }
    assert_eq!(ts.family_substitutions().len(), 1);
}

#[test]
fn requesting_no_family_is_exactly_the_pre_385_path() {
    // The condition for not disturbing a single golden: with no family
    // requested, the cascade order is untouched, the same faces shape, and
    // nothing is reported.
    let mut ts = cascade();
    let styled = slots_used(&mut ts, "Aa", 400, "");
    assert_eq!(styled, vec![NOTO_400]);
    assert!(ts.family_substitutions().is_empty());

    let mut plain = cascade();
    let weighted: Vec<u16> = plain
        .layout_weighted("Aa", 32.0, None, TextShape::default(), 400)
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .map(|glyph| glyph.font)
        .collect();
    assert_eq!(weighted.first().copied(), Some(NOTO_400));
    assert!(plain.family_substitutions().is_empty());
}

#[test]
fn an_unnamed_cascade_never_reports_a_family_substitution() {
    // `with_font_families` declares empty names, so a document naming any
    // family finds none — and that must stay silent, or every pre-#385
    // cascade would start emitting diagnostics it cannot act on.
    let mut ts = Typesetter::with_font_families(vec![vec![WeightedFont::regular(face(FONT))]]);
    ts.layout_styled("Aa", 32.0, None, TextShape::default(), 400, "Inter");
    assert!(ts.family_substitutions().is_empty());
}

#[test]
fn a_weight_report_names_the_cascade_family_not_the_probe_position() {
    // Family matching reorders the probe order, so the position a family
    // is probed at is no longer its position in the cascade. A weight
    // report must still name the cascade family, or it points at the wrong
    // one whenever a later family was preferred.
    let mut ts = cascade();
    // Inter is cascade family 1 and has no face at 500; asking for Inter
    // probes it first, at position 0.
    ts.layout_styled("Aa", 32.0, None, TextShape::default(), 500, "Inter");
    let reports = ts.weight_substitutions();
    assert_eq!(reports.len(), 1, "got {reports:?}");
    assert_eq!(reports[0].family, 1, "got {reports:?}");
    assert_eq!(reports[0].requested, 500);
}

#[test]
fn family_names_are_matched_case_insensitively_and_trimmed() {
    assert!(FontFamily::name_matches("Inter", "inter"));
    assert!(FontFamily::name_matches("Inter", "  INTER  "));
    assert!(FontFamily::name_matches(" Noto Sans ", "Noto Sans"));
    assert!(!FontFamily::name_matches("Inter", "Inter Tight"));
    // An empty name on either side expresses no preference.
    assert!(!FontFamily::name_matches("", "Inter"));
    assert!(!FontFamily::name_matches("Inter", ""));
    assert!(!FontFamily::name_matches("", ""));
}

#[test]
fn the_declared_family_names_are_readable_in_cascade_order() {
    let ts = cascade();
    assert_eq!(
        ts.family_names(),
        ["Noto Sans", "Inter", "Noto Sans Arabic"]
    );
}
