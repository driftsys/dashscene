//! Story #368: font weight. A `Typesetter` built with
//! `with_font_families` groups the cascade into families, each family an
//! ordered set of weighted faces, and flattens the grid back into the one
//! positional slot list `PositionedGlyph::font` already indexes. Selection
//! runs coverage first (which family), then weight within that family (the
//! CSS Fonts 4 rule).
//!
//! The pre-#368 path is unchanged: `with_fonts` declares one weight-400
//! face per family and `layout`/`layout_with` request weight 400, so every
//! existing call site — the E7 oracle and the goldens included — shapes,
//! measures and tags exactly as before.

use dashscene_typeset::text::{Font, TextShape, Typesetter, WeightedFont};

mod common;

use common::{FONT, FONT_ARABIC, FONT_BOLD, FONT_SEMIBOLD, font_data};

fn face(path: &str) -> Font {
    Font::from_bytes(font_data(path), 0).expect("fixture font loads")
}

/// The committed Latin family after this story: Regular, SemiBold, Bold.
fn latin_family() -> Vec<WeightedFont> {
    vec![
        WeightedFont::new(face(FONT), 400),
        WeightedFont::new(face(FONT_SEMIBOLD), 600),
        WeightedFont::new(face(FONT_BOLD), 700),
    ]
}

/// A one-family cascade over the three committed Latin weights.
fn latin() -> Typesetter {
    Typesetter::with_font_families(vec![latin_family()])
}

/// The widest line's measured width — what a box is sized to.
fn width(ts: &mut Typesetter, text: &str, weight: u16) -> f32 {
    ts.layout_weighted(text, 32.0, None, TextShape::default(), weight)
        .width
}

/// Every glyph a weighted layout produces, as (slot, glyph id, x).
fn glyphs(ts: &mut Typesetter, text: &str, weight: u16) -> Vec<(u16, u16, f32)> {
    ts.layout_weighted(text, 32.0, None, TextShape::default(), weight)
        .lines
        .iter()
        .flat_map(|l| l.glyphs.iter().map(|g| (g.font, g.glyph_id, g.x)))
        .collect()
}

/// **The cache trap.** The shaped-run cache is keyed by paragraph text, and
/// weight is a shaping input — a heavier face has its own advances and
/// kerning. Shaping a string at Regular *first* and then asking for Bold
/// must not hand back the Regular entry. This is the exact failure story
/// #341 hit with `ligatures_off`, so the order here matters: Regular first,
/// deliberately, to fill the cache before the Bold request.
#[test]
fn the_same_string_at_two_weights_shapes_differently() {
    let mut ts = latin();
    let text = "Sphinx of quartz 123";
    let regular = width(&mut ts, text, 400);
    let bold = width(&mut ts, text, 700);
    let semibold = width(&mut ts, text, 600);
    assert!(
        regular < semibold && semibold < bold,
        "each heavier weight must measure wider: 400 {regular}, 600 {semibold}, 700 {bold}"
    );
    // Not merely different totals — the per-glyph advances differ, which is
    // what a stale cache entry would hide.
    let at_400 = glyphs(&mut ts, text, 400);
    let at_700 = glyphs(&mut ts, text, 700);
    assert_eq!(
        at_400.len(),
        at_700.len(),
        "same characters, same glyph count"
    );
    assert!(
        at_400.iter().zip(&at_700).any(|(a, b)| a.2 != b.2),
        "bold pen positions must differ from regular's"
    );
}

/// A cached entry is reused within one posture and not across postures: the
/// second Regular layout hits, the first Bold layout misses.
#[test]
fn each_weight_gets_its_own_cache_entry() {
    let mut ts = latin();
    let text = "Sphinx of quartz 123";
    width(&mut ts, text, 400);
    let after_first = ts.cache_stats();
    assert_eq!((after_first.hits, after_first.misses), (0, 1));

    width(&mut ts, text, 400);
    let repeat = ts.cache_stats();
    assert_eq!(
        (repeat.hits, repeat.misses),
        (1, 1),
        "the same text at the same weight must hit"
    );

    width(&mut ts, text, 700);
    let bold = ts.cache_stats();
    assert_eq!(
        (bold.hits, bold.misses),
        (1, 2),
        "the same text at a different weight must miss, not reuse the entry"
    );

    width(&mut ts, text, 700);
    let bold_repeat = ts.cache_stats();
    assert_eq!(
        (bold_repeat.hits, bold_repeat.misses),
        (2, 2),
        "the bold posture caches too"
    );
}

/// A weight-resolved glyph carries the *slot* of the face that shaped it,
/// so the stager's parallel atlas list is indexed correctly. The families
/// flatten family-major: Latin 400/600/700 are slots 0/1/2.
#[test]
fn glyphs_carry_the_resolved_slot() {
    let mut ts = latin();
    assert_eq!(ts.weights(), [400, 600, 700]);
    for (weight, slot) in [(400u16, 0u16), (600, 1), (700, 2)] {
        let slots: Vec<u16> = glyphs(&mut ts, "Hi", weight).iter().map(|g| g.0).collect();
        assert!(
            slots.iter().all(|&s| s == slot),
            "weight {weight} must tag slot {slot}, got {slots:?}"
        );
    }
}

/// Coverage picks the family before weight picks the face. A bold Arabic
/// run in a cascade whose Arabic family has only a Regular face must
/// resolve to Arabic Regular — never to Latin Bold, which would render the
/// text as `.notdef` boxes. Correctness (the reader can read it) outranks
/// fidelity (it is the right weight).
#[test]
fn coverage_outranks_weight() {
    let mut ts = Typesetter::with_font_families(vec![
        latin_family(),
        vec![WeightedFont::regular(face(FONT_ARABIC))],
    ]);
    // Slots: Latin 400/600/700 = 0/1/2, Arabic 400 = 3.
    assert_eq!(ts.weights(), [400, 600, 700, 400]);
    let slots: Vec<u16> = glyphs(&mut ts, "مرحبا", 700).iter().map(|g| g.0).collect();
    assert!(
        slots.iter().all(|&s| s == 3),
        "a bold Arabic run must stay in the Arabic family, got slots {slots:?}"
    );
    // And the substitution is reported rather than made silently (P4).
    let reports = ts.weight_substitutions();
    assert_eq!(reports.len(), 1, "one report: {reports:?}");
    assert_eq!(
        (reports[0].family, reports[0].requested, reports[0].resolved),
        (1, 700, 400)
    );
}

/// A family the run never touched must not be reported. Resolution runs over
/// the whole cascade, because the resolved faces are what the coverage split
/// probes — but only the families coverage actually selected substituted
/// anything. A pure-Latin bold run against the production cascade
/// (`goldens/tooling/src/render.rs`: Latin 400/600/700 plus Arabic 400)
/// resolves the Arabic family to Arabic Regular and then never uses it, so
/// reporting it would name a substitution that did not happen and make the
/// true reports indistinguishable from noise (P4). `coverage_outranks_weight`
/// cannot catch this: its Latin family has an exact 700 face, so the Latin
/// side reports nothing either way.
#[test]
fn an_untouched_family_reports_no_substitution() {
    let production_cascade = || {
        Typesetter::with_font_families(vec![
            latin_family(),
            vec![WeightedFont::regular(face(FONT_ARABIC))],
        ])
    };

    // Pure Latin at 700: the Latin family has an exact Bold face, and the
    // Arabic family was resolved but never shaped a glyph.
    let mut latin_only = production_cascade();
    let slots: Vec<u16> = glyphs(&mut latin_only, "Hello", 700)
        .iter()
        .map(|g| g.0)
        .collect();
    assert!(
        slots.iter().all(|&s| s == 2),
        "a pure-Latin bold run must shape entirely in Latin Bold (slot 2), got {slots:?}"
    );
    assert_eq!(
        latin_only.weight_substitutions(),
        [],
        "no glyph came from the Arabic family, so it substituted nothing"
    );

    // The same cascade, the same requested weight, but Arabic text: now the
    // Arabic family *is* selected, has no Bold face, and must be reported.
    let mut with_arabic = production_cascade();
    glyphs(&mut with_arabic, "مرحبا", 700);
    let reports = with_arabic.weight_substitutions();
    assert_eq!(
        reports.len(),
        1,
        "the real substitution is reported: {reports:?}"
    );
    assert_eq!(
        (reports[0].family, reports[0].requested, reports[0].resolved),
        (1, 700, 400),
        "family 1 is Arabic: asked 700, got 400"
    );
}

/// A family used at weight 500 against the committed corpus is reported even
/// though a *different* family in the same cascade rendered at an exact
/// weight — reporting follows the output family by family, not all-or-nothing.
#[test]
fn a_used_family_reports_even_beside_an_exact_one() {
    let mut ts = Typesetter::with_font_families(vec![
        latin_family(),
        vec![WeightedFont::regular(face(FONT_ARABIC))],
    ]);
    // Mixed run at 600: Latin has an exact SemiBold, Arabic does not.
    glyphs(&mut ts, "Hi مرحبا", 600);
    let reports = ts.weight_substitutions();
    assert_eq!(
        reports.len(),
        1,
        "only the Arabic family substituted: {reports:?}"
    );
    assert_eq!(
        (reports[0].family, reports[0].requested, reports[0].resolved),
        (1, 600, 400)
    );
}

/// P4: the substitution is named, deduplicated per (family, requested,
/// resolved) triple, and non-fatal.
#[test]
fn a_substitution_is_reported_once_per_triple() {
    let mut ts = Typesetter::with_font_families(vec![vec![WeightedFont::regular(face(FONT))]]);
    assert!(
        ts.weight_substitutions().is_empty(),
        "an exact match reports nothing"
    );
    width(&mut ts, "one", 400);
    assert!(ts.weight_substitutions().is_empty());

    // Weight 700 against a Regular-only family: resolves to 400 and reports.
    width(&mut ts, "two", 700);
    assert_eq!(ts.weight_substitutions().len(), 1);
    // The same triple again, on different text — still one report.
    width(&mut ts, "three", 700);
    width(&mut ts, "four", 700);
    assert_eq!(
        ts.weight_substitutions().len(),
        1,
        "the triple must deduplicate across layouts"
    );
    // A different requested weight is a different triple.
    width(&mut ts, "five", 900);
    assert_eq!(ts.weight_substitutions().len(), 2);

    let text = ts.weight_substitutions()[0].to_string();
    assert!(
        text.starts_with("text.weight-substituted:"),
        "the diagnostic is named: {text}"
    );
}

/// The E7 guard, at the unit level: `with_fonts` is `with_font_families`
/// with one weight-400 face each, and `layout_with` is `layout_weighted` at
/// 400 — so the two produce identical layouts over the same fonts, and no
/// pre-#368 call site changed behavior.
#[test]
fn with_fonts_is_the_all_regular_cascade() {
    let text = "Sphinx of quartz — مرحبا 123";
    let mut old = Typesetter::with_fonts(vec![face(FONT), face(FONT_ARABIC)]);
    let mut new = Typesetter::with_font_families(vec![
        vec![WeightedFont::regular(face(FONT))],
        vec![WeightedFont::regular(face(FONT_ARABIC))],
    ]);
    assert_eq!(old.weights(), [400, 400]);
    let from_old = old.layout_with(text, 32.0, Some(200.0), TextShape::default());
    let from_new = new.layout_weighted(text, 32.0, Some(200.0), TextShape::default(), 400);
    assert_eq!(from_old, from_new);
    assert!(old.weight_substitutions().is_empty());
    assert!(new.weight_substitutions().is_empty());
}

/// Requesting weight 400 against the three-weight family reproduces the
/// Regular-only cascade exactly — the property the E7 frames rely on, since
/// every E7 fixture carries weight 400.
#[test]
fn weight_400_against_a_multi_weight_family_matches_regular_alone() {
    let text = "Hello dashscene\n88 mph";
    let mut regular_only = Typesetter::with_fonts(vec![face(FONT)]);
    let mut all_weights = latin();
    let baseline = regular_only.layout_with(text, 28.0, Some(432.0), TextShape::default());
    let weighted = all_weights.layout_weighted(text, 28.0, Some(432.0), TextShape::default(), 400);
    assert_eq!(baseline, weighted);
    assert!(all_weights.weight_substitutions().is_empty());
}

/// Weight 500 resolves to Regular, not SemiBold — the CSS Fonts 4 rule's
/// specified answer against the committed {400, 600, 700} corpus, and the
/// reason story #368 commits no Medium face. It reports a substitution, so
/// the gap is visible rather than silent.
#[test]
fn weight_500_resolves_to_regular_and_reports() {
    let mut ts = latin();
    let text = "Get the App";
    let at_500 = width(&mut ts, text, 500);
    let at_400 = width(&mut ts, text, 400);
    assert_eq!(at_500, at_400, "500 must render at Regular's metrics");
    let reports = ts.weight_substitutions();
    assert_eq!(reports.len(), 1);
    assert_eq!((reports[0].requested, reports[0].resolved), (500, 400));
}

/// The measured extent — what the engine sizes a box from — follows the
/// resolved face. A bold run measured at Regular's advances would overflow
/// its box, which is the fidelity bug this story fixes.
#[test]
fn a_bold_run_measures_at_bold_advances() {
    let mut ts = latin();
    let text = "Sphinx of quartz 123";
    // Wrap at the Regular width: Regular fits on one line, Bold cannot.
    let regular_width = width(&mut ts, text, 400);
    let bold = ts.layout_weighted(text, 32.0, Some(regular_width), TextShape::default(), 700);
    assert!(
        bold.lines.len() > 1,
        "bold text must wrap inside a Regular-width box, got {} line(s)",
        bold.lines.len()
    );
}
