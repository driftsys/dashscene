//! Story #219: multi-font fallback. A `Typesetter` built with an ordered
//! font list splits each UAX #9 level run into font sub-runs by coverage
//! before shaping — a codepoint goes to the first font in the list that
//! covers the glyph it will actually shape to; a codepoint no font covers
//! keeps the P4 posture (shapes to `.notdef` in the primary font). Every
//! `PositionedGlyph` carries the index of the font it was shaped with, so
//! the boundary-B stager groups a mixed-script layout into one glyph run
//! per atlas.
//!
//! The single-font path (`Typesetter::new`) is unchanged: one font, every
//! glyph tagged font 0, exactly the pre-#219 output — the E2 Arabic golden
//! depends on it.

use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{FONT, FONT_ARABIC, cmap, font_data};

/// A typesetter over an ordered font list (primary first).
fn multi(paths: &[&str]) -> Typesetter {
    let fonts = paths
        .iter()
        .map(|p| Font::from_bytes(font_data(p), 0).expect("fixture font loads"))
        .collect();
    Typesetter::with_fonts(fonts)
}

/// Every glyph a layout produces, flattened, with its font index.
fn glyphs(ts: &mut Typesetter, text: &str, size: f32) -> Vec<(u16, u16, f32)> {
    ts.layout(text, size, None)
        .lines
        .iter()
        .flat_map(|l| l.glyphs.iter().map(|g| (g.font, g.glyph_id, g.x)))
        .collect()
}

/// "sur'a km/h" (speed km/h): the Arabic word shapes in the primary Arabic
/// font; the Latin unit "km/h" — letters and the solidus Noto Sans Arabic
/// carries neither — cascades to the Latin fallback. Both fonts' glyphs
/// appear in one layout, tagged by font index and positioned as one
/// visual line.
#[test]
fn mixed_script_cascade_tags_and_places_both_fonts() {
    // Primary Arabic, fallback Latin.
    let mut ts = multi(&[FONT_ARABIC, FONT]);
    let laid = glyphs(&mut ts, "سرعة km/h", 32.0);

    // Both fonts contribute glyphs.
    assert!(
        laid.iter().any(|&(f, _, _)| f == 0),
        "the Arabic word must shape in the primary font (0)"
    );
    assert!(
        laid.iter().any(|&(f, _, _)| f == 1),
        "the Latin unit must cascade to the fallback font (1)"
    );

    // The fallback glyphs are exactly the Latin unit's glyphs, from the
    // Latin font's cmap (liga/clig are off for the Plain Latin run, so
    // "km/h" stays four glyphs). Kerning shifts positions, not glyph ids.
    let fallback: std::collections::BTreeSet<u16> = laid
        .iter()
        .filter(|&&(f, _, _)| f == 1)
        .map(|&(_, g, _)| g)
        .collect();
    let expected: std::collections::BTreeSet<u16> = ['k', 'm', '/', 'h']
        .into_iter()
        .map(|c| cmap(FONT, c))
        .collect();
    assert_eq!(
        fallback, expected,
        "the fallback run must be exactly the Latin unit's cmap glyphs"
    );

    // Positioned as one line: the LTR Latin unit sits to the left of the
    // right-to-left Arabic word (the Arabic paragraph is RTL-based).
    let latin_max_x = laid
        .iter()
        .filter(|&&(f, _, _)| f == 1)
        .map(|&(_, _, x)| x)
        .fold(f32::MIN, f32::max);
    let primary_min_x = laid
        .iter()
        .filter(|&&(f, _, _)| f == 0)
        .map(|&(_, _, x)| x)
        .fold(f32::MAX, f32::min);
    assert!(
        latin_max_x < primary_min_x,
        "the LTR Latin unit ({latin_max_x}) must sit left of the RTL Arabic word ({primary_min_x})"
    );
}

/// A codepoint no font in the list covers keeps the P4 posture: it shapes
/// to `.notdef` (glyph id 0) in the primary font, never a silent drop, and
/// is tagged font 0 so the painter's missing-glyph diagnostic (#30) fires.
#[test]
fn uncovered_codepoint_is_notdef_in_the_primary_font() {
    // Neither Noto Sans Arabic nor Noto Sans carries a CJK ideograph.
    let mut ts = multi(&[FONT_ARABIC, FONT]);
    let laid = glyphs(&mut ts, "一", 32.0);
    assert_eq!(laid.len(), 1);
    assert_eq!(
        laid[0].0, 0,
        "an uncovered codepoint stays in the primary font"
    );
    assert_eq!(laid[0].1, 0, "an uncovered codepoint shapes to .notdef");
}

/// The single-font constructor is the pre-#219 path: one font, every glyph
/// tagged font 0. This is what keeps the E2 golden byte-identical.
#[test]
fn single_font_tags_every_glyph_font_zero() {
    let font = Font::from_bytes(font_data(FONT_ARABIC), 0).expect("loads");
    let mut ts = Typesetter::new(font);
    let laid = glyphs(&mut ts, "سرعة 120", 32.0);
    assert!(!laid.is_empty());
    assert!(
        laid.iter().all(|&(f, _, _)| f == 0),
        "a single-font typesetter tags every glyph font 0"
    );
}

/// A line's height comes from the fonts that actually shaped its glyphs,
/// not from the cascade's primary font (story #219 applied per line, the
/// same per-font principle the per-glyph scale follows). A pure-Arabic
/// word is shaped entirely by the Arabic font whether the Arabic font is
/// the primary or the fallback, so its laid-out height must be identical
/// under either cascade order. Noto Sans Arabic's line box is taller than
/// Noto Sans's (its ascender and descender reach further), so taking the
/// height from a Latin primary would measure the Arabic text too short.
#[test]
fn line_height_comes_from_the_shaping_font_not_the_primary() {
    // Four Arabic letters, no spaces: one line, every glyph shaped by the
    // Arabic font under either cascade order.
    let text = "سرعة";
    let size = 32.0;

    let mut latin_primary = multi(&[FONT, FONT_ARABIC]);
    let mut arabic_primary = multi(&[FONT_ARABIC, FONT]);
    let laid_latin_primary = latin_primary.layout(text, size, None);
    let laid_arabic_primary = arabic_primary.layout(text, size, None);

    // Premise: the cascade routed every glyph to the Arabic font in each
    // order — the fallback (index 1) under a Latin primary, the primary
    // (index 0) under an Arabic primary. The two layouts therefore shaped
    // the identical Arabic glyphs, so their heights must match.
    assert!(
        glyphs(&mut latin_primary, text, size)
            .iter()
            .all(|&(f, _, _)| f == 1),
        "every glyph must cascade to the Arabic fallback under a Latin primary"
    );
    assert!(
        glyphs(&mut arabic_primary, text, size)
            .iter()
            .all(|&(f, _, _)| f == 0),
        "every glyph must shape in the Arabic primary under an Arabic primary"
    );

    assert_eq!(
        laid_latin_primary.height, laid_arabic_primary.height,
        "a pure-Arabic line's height must come from the Arabic font that shaped it, \
         not the cascade's primary (Latin-primary {} vs Arabic-primary {})",
        laid_latin_primary.height, laid_arabic_primary.height,
    );
}

/// The shaped-run cache key stays the paragraph text alone: the font list
/// is fixed per typesetter (runtime configuration, not a per-call axis),
/// so the cascade is a pure function of the text and one cache entry serves
/// every render size — as in the single-font case.
#[test]
fn cache_key_is_text_across_sizes_for_a_multi_font_typesetter() {
    let mut ts = multi(&[FONT_ARABIC, FONT]);
    let text = "سرعة km/h";
    let _ = ts.layout(text, 20.0, None);
    let _ = ts.layout(text, 40.0, None);
    let _ = ts.layout(text, 33.0, Some(200.0));
    let stats = ts.cache_stats();
    assert_eq!(stats.misses, 1, "one shaping miss for the paragraph");
    assert_eq!(stats.hits, 2, "the two re-layouts reuse the cached cascade");
}

/// The digit-shape context scan is not confused by a font-split boundary:
/// in "sur'a 120 km/h" the authored European "120" resolves Arabic context
/// (the preceding Arabic word) and shapes to Arabic-Indic glyphs in the
/// primary Arabic font, while "km/h" still cascades to the Latin fallback.
#[test]
fn digit_context_survives_the_font_split() {
    let mut ts = multi(&[FONT_ARABIC, FONT]);
    let laid = glyphs(&mut ts, "سرعة 120 km/h", 32.0);

    // The three authored European digits render as the Arabic-Indic glyphs
    // of the primary font, exactly as they would with no fallback at all.
    let arabic_indic: std::collections::BTreeSet<u16> = ['١', '٢', '٠']
        .into_iter()
        .map(|c| cmap(FONT_ARABIC, c))
        .collect();
    let digit_glyphs: std::collections::BTreeSet<u16> = laid
        .iter()
        .filter(|&&(f, g, _)| f == 0 && arabic_indic.contains(&g))
        .map(|&(_, g, _)| g)
        .collect();
    assert_eq!(
        digit_glyphs, arabic_indic,
        "European digits in Arabic context render Arabic-Indic in the primary font"
    );
    // None of the European digit glyphs leaked through.
    let european: std::collections::BTreeSet<u16> = ['1', '2', '0']
        .into_iter()
        .map(|c| cmap(FONT_ARABIC, c))
        .collect();
    assert!(
        laid.iter().all(|&(_, g, _)| !european.contains(&g)),
        "no authored European digit glyph survives the Arabic-context substitution"
    );
    // The Latin unit still cascaded to the fallback.
    assert!(
        laid.iter().any(|&(f, _, _)| f == 1),
        "km/h stays in the fallback font"
    );
}

/// The cascade routes a character to the font that renders the glyph it
/// will actually shape to, not merely its authored codepoint. With a Latin
/// primary and an Arabic fallback, a European digit in Arabic context
/// resolves to its Arabic-Indic display shape, which only the Arabic font
/// carries — so the digit cascades to the fallback and renders correctly,
/// rather than substituting to a glyph the primary lacks.
#[test]
fn european_digit_in_arabic_context_cascades_to_the_font_that_renders_it() {
    // Primary Latin, fallback Arabic — the reverse configuration.
    let mut ts = multi(&[FONT, FONT_ARABIC]);
    let laid = glyphs(&mut ts, "كتاب 5", 32.0);

    let five_arabic_indic = cmap(FONT_ARABIC, '٥');
    let hit = laid
        .iter()
        .find(|&&(_, g, _)| g == five_arabic_indic)
        .expect("the digit must render as the Arabic-Indic five");
    assert_eq!(
        hit.0, 1,
        "the digit cascades to the Arabic fallback (1), the only font with the shape"
    );
    // The digit did not stay European in the Latin primary.
    let five_european = cmap(FONT, '5');
    assert!(
        laid.iter().all(|&(_, g, _)| g != five_european),
        "the European five glyph must not appear"
    );
    // The Arabic word letters cannot render in the Latin primary either, so
    // they too cascade to the Arabic fallback — nothing shapes to .notdef.
    assert!(
        laid.iter().all(|&(_, g, _)| g != 0),
        "no glyph is a dropped .notdef"
    );
}

/// The glyph ids a single-font typesetter lays `text` out to — the correct,
/// undivided shaping the cascade must match when a whole shaping unit routes
/// to one font.
fn single_font_gids(path: &str, text: &str) -> Vec<u16> {
    let mut ts = Typesetter::new(Font::from_bytes(font_data(path), 0).expect("loads"));
    ts.layout(text, 32.0, None)
        .lines
        .iter()
        .flat_map(|l| l.glyphs.iter().map(|g| g.glyph_id))
        .collect()
}

/// C1(a): a joining control (ZWJ) is a format character, not a routable
/// glyph — it must shape in the same rustybuzz call as the letters it joins.
/// ZWJ is in the Latin cmap too, so routing it on its own coverage would
/// strand it in the Latin primary and split the Arabic word into separate
/// shaping calls, so the letters come out isolated instead of joined. It
/// inherits the preceding base's font instead.
#[test]
fn a_joining_control_routes_with_the_arabic_it_joins() {
    // Latin primary, Arabic fallback.
    let mut ts = multi(&[FONT, FONT_ARABIC]);
    let laid = glyphs(&mut ts, "ل\u{200D}م", 32.0);
    assert!(
        laid.iter().all(|&(f, _, _)| f == 1),
        "lam, ZWJ, and meem all route to the Arabic fallback: {laid:?}"
    );
    // The letters shaped joined, identical to the undivided Arabic shaping —
    // not the isolated forms a mid-word font split would produce.
    let cascaded: Vec<u16> = laid.iter().map(|&(_, g, _)| g).collect();
    assert_eq!(
        cascaded,
        single_font_gids(FONT_ARABIC, "ل\u{200D}م"),
        "the cascaded word must shape identically to the single-font joined word"
    );
}

/// C1(b): a combining mark is not a routable glyph either — it must shape
/// with its base so GPOS mark-to-base can position it. A fatha whose base
/// 'a' is only in the Latin primary must not split off to the Arabic
/// fallback and shape alone (where, with no base in the buffer, it renders
/// as a floating glyph on the baseline). It inherits its base's font.
#[test]
fn a_combining_mark_routes_with_its_base() {
    // Latin primary (has 'a', not the fatha), Arabic fallback (has the fatha).
    let mut ts = multi(&[FONT, FONT_ARABIC]);
    let laid = glyphs(&mut ts, "a\u{064E}", 32.0);
    assert!(
        laid.iter().all(|&(f, _, _)| f == 0),
        "the fatha inherits its base's font (0), never splitting to the Arabic fallback: {laid:?}"
    );
}

/// The non-joiner (ZWNJ, Persian/Urdu) is a format control like the joiner:
/// it must route with the Arabic letters it separates so its joining-break
/// takes effect within one shaping call, rather than stranding in the Latin
/// primary and splitting the word.
#[test]
fn a_non_joiner_routes_with_the_arabic_around_it() {
    let mut ts = multi(&[FONT, FONT_ARABIC]);
    let laid = glyphs(&mut ts, "ب\u{200C}ب", 32.0);
    assert!(
        laid.iter().all(|&(f, _, _)| f == 1),
        "the ZWNJ and both behs route to the Arabic fallback: {laid:?}"
    );
    let cascaded: Vec<u16> = laid.iter().map(|&(_, g, _)| g).collect();
    assert_eq!(
        cascaded,
        single_font_gids(FONT_ARABIC, "ب\u{200C}ب"),
        "the ZWNJ break must be honored in one shaping call, matching single-font output"
    );
}

/// T1: a font split strictly INSIDE one bidi level run. "sur'a & 'arabi" is
/// one right-to-left level run (the ampersand and spaces are neutrals that
/// take the surrounding RTL level), and the ampersand — which Noto Sans
/// Arabic does not carry — cascades to the Latin fallback. The split must
/// not disturb the run's visual order: the ampersand sits between the two
/// Arabic words, not at either end.
#[test]
fn a_font_split_inside_one_level_run_keeps_visual_order() {
    let mut ts = multi(&[FONT_ARABIC, FONT]);
    let text = "سرعة & عربي";
    // Precondition: the whole string is one RTL level run.
    // (Assert routing and order on the laid-out result.)
    let laid = glyphs(&mut ts, text, 32.0);

    // The ampersand is the only fallback (font 1) glyph; it is the Latin
    // cmap ampersand.
    let amp = cmap(FONT, '&');
    let fallback: Vec<(u16, u16, f32)> = laid.iter().copied().filter(|&(f, _, _)| f == 1).collect();
    assert_eq!(fallback.len(), 1, "only the ampersand cascades: {laid:?}");
    assert_eq!(
        fallback[0].1, amp,
        "the fallback glyph is the Latin ampersand"
    );

    // Visual order in this RTL run: the second Arabic word 'arabi is
    // leftmost, then the ampersand, then sur'a rightmost. So the ampersand's
    // x lies strictly between the two Arabic segments — it did not reorder to
    // an end of the run.
    let amp_x = fallback[0].2;
    let left_word: Vec<f32> = laid
        .iter()
        .filter(|&&(f, _, x)| f == 0 && x < amp_x)
        .map(|&(_, _, x)| x)
        .collect();
    let right_word: Vec<f32> = laid
        .iter()
        .filter(|&&(f, _, x)| f == 0 && x > amp_x)
        .map(|&(_, _, x)| x)
        .collect();
    assert!(
        !left_word.is_empty() && !right_word.is_empty(),
        "Arabic glyphs sit on both sides of the ampersand (visual order preserved): {laid:?}"
    );
}
