//! The bidi hot path, cached (issues #225 and #226).
//!
//! The engine's measure callback calls `layout()` several times per text
//! node per Taffy solve. Issue #225: the full UAX #9 resolution ran in
//! front of the shaped-run cache lookup, so every one of those calls
//! repaid it. Issue #226: `unicode_bidi`'s `visual_runs` copies the whole
//! paragraph's level vector per line, so a paragraph wrapped into N lines
//! paid that copy N times per call.
//!
//! Both fixes are caches, so the assertions come in pairs: a **count**
//! that the work happens once, and a **correctness** check that what the
//! cache serves is what a cold typesetter would have computed. A cache
//! that returns the wrong levels is a silent bug — no golden covers a
//! text the corpus does not carry, and the direction of a paragraph is
//! not visible in a width.

use dashscene_typeset::text::{TextLayout, TextShape, Typesetter};

mod common;

use common::{FONT, FONT_ARABIC};

fn latin() -> Typesetter {
    common::typesetter(FONT)
}

fn arabic() -> Typesetter {
    common::typesetter(FONT_ARABIC)
}

/// A multi-line Arabic paragraph — the showcase's own shape, and the
/// worst case for both issues: an RTL base direction with embedded
/// European digits, long enough to wrap.
const ARABIC_PARAGRAPH: &str = "العربية 2026 نص طويل يلتف على عدة أسطر مع أرقام \
                                123 و 456 داخل الفقرة لقياس إعادة الترتيب";

/// A pair of paragraphs with the **same byte length**, the same
/// characters, and opposite base directions — the first strong character
/// is Latin in one and Hebrew in the other. Any cache that keyed on
/// something weaker than the text, or any buffer that leaked one
/// paragraph's state into the other, serves one where the other was
/// asked for, and this pair is what catches it.
///
/// The committed Latin fixture has no Hebrew glyphs, so the Hebrew
/// shapes to `.notdef` at a uniform advance. The base direction is
/// therefore visible in the *placement*: an RTL-base line flushes right
/// within `max_width`, an LTR-base line stays at x = 0. That placement is
/// read straight off the cached paragraph level, so it is exactly the
/// value a wrong cache entry would corrupt — but it only shows against a
/// container, hence [`FLUSH_WIDTH`] rather than an unconstrained layout.
const LTR_BASE: &str = "ab 12 אב";
const RTL_BASE: &str = "אב 12 ab";

/// A container wider than either paragraph of the direction pair, so the
/// RTL-base one has somewhere to flush to.
const FLUSH_WIDTH: Option<f32> = Some(200.0);

/// Lays `text` out on a typesetter that has never seen it — the value the
/// cache must reproduce.
fn cold(text: &str, max_width: Option<f32>) -> TextLayout {
    latin().layout(text, 16.0, max_width)
}

/// [`cold`] over the Arabic cascade, for the texts that have Arabic
/// glyphs to shape.
fn cold_arabic(text: &str, max_width: Option<f32>) -> TextLayout {
    arabic().layout(text, 16.0, max_width)
}

// ---------------------------------------------------------------------
// Issue #225 — the resolution runs once per paragraph text
// ---------------------------------------------------------------------

/// The count for issue #225. Six `layout()` calls on one paragraph — the
/// measure callback's own pattern, probing a node at several widths
/// within one solve — must run the UAX #9 resolution once, not six
/// times.
///
/// Falsifiable: `bidi_resolutions` counts calls to the resolution itself,
/// not cache lookups, so it reads 6 when the resolution sits in front of
/// the cache and 1 when it sits behind it. The number is the call count,
/// so it cannot pass by accident on a machine that is merely fast.
#[test]
fn uax9_resolution_runs_once_however_often_the_paragraph_is_laid_out() {
    let mut ts = arabic();
    for width in [None, Some(600.0), Some(400.0), Some(300.0), Some(220.0)] {
        let _ = ts.layout(ARABIC_PARAGRAPH, 16.0, width);
    }
    let _ = ts.layout(ARABIC_PARAGRAPH, 24.0, Some(400.0));
    assert_eq!(
        ts.cache_stats().bidi_resolutions,
        1,
        "six layouts of one paragraph must resolve UAX #9 once"
    );
}

/// The resolution has no posture: neither the requested weight nor the
/// ligature setting reaches `BidiInfo::new`, so the same paragraph shaped
/// under two postures still resolves its levels once — unlike the shaped
/// runs, which must be cached per posture.
#[test]
fn two_postures_of_one_paragraph_share_one_resolution() {
    let mut ts = arabic();
    let ligatures_off = TextShape {
        ligatures_off: true,
        ..TextShape::default()
    };
    let _ = ts.layout(ARABIC_PARAGRAPH, 16.0, Some(400.0));
    let _ = ts.layout_with(ARABIC_PARAGRAPH, 16.0, Some(400.0), ligatures_off);
    let stats = ts.cache_stats();
    assert_eq!(stats.bidi_resolutions, 1, "one text, one resolution");
    assert_eq!(stats.misses, 2, "but two postures, two shaped entries");
}

/// One resolution per **distinct** paragraph text: a '\n'-separated block
/// resolves each of its chunks, and re-laying the block adds nothing.
#[test]
fn each_distinct_paragraph_resolves_once() {
    let mut ts = latin();
    let block = "first line\nsecond line\nfirst line";
    let _ = ts.layout(block, 16.0, None);
    assert_eq!(
        ts.cache_stats().bidi_resolutions,
        2,
        "three chunks, two distinct texts"
    );
    let _ = ts.layout(block, 16.0, None);
    assert_eq!(ts.cache_stats().bidi_resolutions, 2, "no new text, no work");
}

// ---------------------------------------------------------------------
// Issue #226 — the per-line reorder copies the paragraph once, not N times
// ---------------------------------------------------------------------

/// The count for issue #226. One `layout()` of a paragraph that wraps
/// into five or more lines must copy at most the paragraph's own byte
/// count of embedding levels in total — one line's levels per line —
/// rather than the whole paragraph's per line.
///
/// Falsifiable: the lines of a paragraph are disjoint byte ranges, so
/// their lengths sum to at most the paragraph's length whatever the wrap
/// produces, while `visual_runs`'s per-line clone of the paragraph's
/// level vector sums to `lines × paragraph length` — five times the bound
/// here at five lines. The assertion is over counted levels, not elapsed
/// time.
#[test]
fn a_wrapped_paragraph_copies_its_levels_once_not_once_per_line() {
    let mut ts = arabic();
    let before = ts.cache_stats().reorder_levels_copied;
    let laid = ts.layout(ARABIC_PARAGRAPH, 16.0, Some(120.0));
    let copied = ts.cache_stats().reorder_levels_copied - before;
    let lines = laid.lines.len() as u64;
    let bytes = ARABIC_PARAGRAPH.len() as u64;
    assert!(lines >= 5, "fixture must wrap: {lines} lines");
    assert!(
        copied <= bytes,
        "copied {copied} levels for a {bytes}-byte paragraph; \
         the per-line clone would have copied {}",
        lines * bytes
    );
}

/// The reorder buffer is reused across calls and across paragraphs, so
/// the per-call cost must stay flat: laying the same wrapped paragraph
/// out ten times copies ten times one paragraph's levels, not more.
#[test]
fn the_reorder_cost_per_call_stays_flat() {
    let mut ts = arabic();
    let before = ts.cache_stats().reorder_levels_copied;
    for _ in 0..10 {
        let _ = ts.layout(ARABIC_PARAGRAPH, 16.0, Some(120.0));
    }
    let copied = ts.cache_stats().reorder_levels_copied - before;
    let bytes = ARABIC_PARAGRAPH.len() as u64;
    assert!(
        copied <= 10 * bytes,
        "ten layouts copied {copied} levels; bound is {}",
        10 * bytes
    );
}

// ---------------------------------------------------------------------
// Correctness — the caches never serve stale or wrong levels
// ---------------------------------------------------------------------

/// A battery covering every direction shape the pipeline resolves: pure
/// LTR, pure RTL, either direction embedded in the other, digits inside
/// RTL (which lift to their own level), an isolate, a multi-paragraph
/// block, and the same-length direction-flip pair.
const BATTERY: &[&str] = &[
    "",
    "the quick brown fox jumps over the lazy dog",
    "אבג דהו זחט",
    LTR_BASE,
    RTL_BASE,
    "אב 123 גד",
    "abc 123 def",
    "abc\u{2067}אבג\u{2069}def",
    "first\nשני\nthird",
    "trailing spaces   ",
    "   leading spaces",
];

/// The whole point of the cache: what it serves must equal what a cold
/// typesetter computes, for every text, in any order, however many times.
/// A wrong cached level moves glyphs — it does not fail loudly — so this
/// compares the full `TextLayout`, glyph positions included.
#[test]
fn a_warm_typesetter_lays_out_exactly_as_a_cold_one() {
    let references: Vec<TextLayout> = BATTERY.iter().map(|t| cold(t, Some(180.0))).collect();
    let mut ts = latin();
    // Three passes over the battery, so every text is laid out cold,
    // warm, and warm again behind every other text in the battery.
    for pass in 0..3 {
        for (text, reference) in BATTERY.iter().zip(&references) {
            let got = ts.layout(text, 16.0, Some(180.0));
            assert_eq!(&got, reference, "pass {pass}, text {text:?}");
        }
    }
}

/// The direction flip, on one typesetter, alternating: two paragraphs of
/// the same byte length whose base directions differ must each keep their
/// own resolution. This is the assertion a text-keyed cache has to earn —
/// the base level is auto-detected from the text, so the key is complete
/// only because the text is the sole input to the resolution.
#[test]
fn alternating_base_directions_each_keep_their_own_levels() {
    assert_eq!(
        LTR_BASE.len(),
        RTL_BASE.len(),
        "the pair must not be separable by length alone"
    );
    let ltr = cold(LTR_BASE, FLUSH_WIDTH);
    let rtl = cold(RTL_BASE, FLUSH_WIDTH);
    assert_ne!(
        ltr, rtl,
        "the pair must genuinely lay out differently, or this proves nothing"
    );
    let mut ts = latin();
    for _ in 0..4 {
        assert_eq!(
            ts.layout(LTR_BASE, 16.0, FLUSH_WIDTH),
            ltr,
            "LTR-base paragraph"
        );
        assert_eq!(
            ts.layout(RTL_BASE, 16.0, FLUSH_WIDTH),
            rtl,
            "RTL-base paragraph"
        );
    }
}

/// Signal-driven text: the showcase changes a text node's content every
/// frame, so the typesetter sees a new paragraph each time and must
/// resolve each one afresh — while still reusing the entries for content
/// it has already seen.
#[test]
fn changing_content_resolves_afresh_and_still_reuses_what_repeats() {
    let mut ts = arabic();
    let frames: Vec<String> = (0..8).map(|i| format!("السرعة {i} كم/س")).collect();
    let references: Vec<TextLayout> = frames
        .iter()
        .map(|t| arabic().layout(t, 16.0, Some(200.0)))
        .collect();
    for (frame, reference) in frames.iter().zip(&references) {
        assert_eq!(&ts.layout(frame, 16.0, Some(200.0)), reference, "{frame:?}");
    }
    assert_eq!(
        ts.cache_stats().bidi_resolutions,
        frames.len() as u64,
        "eight distinct texts, eight resolutions"
    );
    // The counter reads 0 for the second sweep: every text is now cached.
    let before = ts.cache_stats().bidi_resolutions;
    for (frame, reference) in frames.iter().zip(&references) {
        assert_eq!(&ts.layout(frame, 16.0, Some(200.0)), reference, "{frame:?}");
    }
    assert_eq!(ts.cache_stats().bidi_resolutions, before, "no new work");
}

/// The reorder buffer is shared, so a long paragraph must leave nothing
/// behind that a short one could read. Interleaving the longest and
/// shortest texts in the battery, each still equals its cold layout.
#[test]
fn a_long_paragraph_leaves_nothing_for_a_short_one_to_read() {
    let long = cold_arabic(ARABIC_PARAGRAPH, Some(120.0));
    let short = cold_arabic("مم", Some(120.0));
    let mut ts = arabic();
    for _ in 0..4 {
        assert_eq!(ts.layout(ARABIC_PARAGRAPH, 16.0, Some(120.0)), long);
        assert_eq!(ts.layout("مم", 16.0, Some(120.0)), short);
    }
}

/// Wrapping is a per-call axis and the resolution is not, so the same
/// cached levels must serve every width — including the measure pass's
/// unconstrained probe, which is the call the engine makes first.
#[test]
fn one_resolution_serves_every_width() {
    let widths = [None, Some(600.0), Some(400.0), Some(240.0), Some(120.0)];
    let references: Vec<TextLayout> = widths
        .iter()
        .map(|&w| arabic().layout(ARABIC_PARAGRAPH, 16.0, w))
        .collect();
    let mut ts = arabic();
    for pass in 0..3 {
        for (&width, reference) in widths.iter().zip(&references) {
            assert_eq!(
                &ts.layout(ARABIC_PARAGRAPH, 16.0, width),
                reference,
                "pass {pass}, width {width:?}"
            );
        }
    }
    assert_eq!(ts.cache_stats().bidi_resolutions, 1);
}
