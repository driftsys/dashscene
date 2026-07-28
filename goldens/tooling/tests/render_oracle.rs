//! The design-source render oracle (story #284 tooling, story #301
//! productionization; exit criterion E7, guardrail G-11): a perceptual
//! diff of the reference painter's output against a design source (Figma's
//! REST `GET /images` export), with per-rule tolerance bands.
//!
//! Two kinds of test live here. The first validates the diff harness and
//! the pinned bands with controlled **synthetic** image pairs — no design
//! source is pretended. The second,
//! `the_reference_renders_match_their_design_source`, is the real oracle:
//! for every frame that has a committed design source it imports that
//! frame's committed Figma fixture, compiles it in-process through
//! `compile_figma` (`Profile::Core`), renders the committed scene with the
//! Skia reference painter, and diffs that fresh render against the committed
//! Figma export within the frame's band. The reference is our own render of
//! the imported fixture, not a pre-committed corpus golden — so the diff
//! measures the reference painter against Figma's own render of the same
//! scene at the same size. It runs in the ordinary `test` job: it is
//! hermetic (committed fixture + committed export + in-process compile, no
//! network) and fast. Frames without a committed design source stay pending
//! (`goldens/oracle/README.md`); no design source is fabricated (G-11).

use std::collections::BTreeMap;

use dashc_wasm::compile_figma;
use dashpaint::{AtlasIndex, GlyphQuad, GlyphRun, GlyphRunTable, Painter};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, FontFamily, TextShape, Typesetter, WeightedFont};
use dashscene_validator::Profile;
use goldens::oracle::{self, AA_EDGE, BLUR_FALLOFF, MSDF_TEXT, ToleranceBand};
use skia_safe::{Color, Color4f, EncodedImageFormat, Paint, Rect, surfaces};

mod common;
use common::manifest;
use common::{load_atlas, origin_of};

/// A `w`×`h` opaque PNG cleared to `base`, optionally with one axis-aligned
/// integer `patch` rect painted over it with anti-aliasing off — so every
/// pixel is exactly one of the two colors and the differing-pixel count is
/// exact, not jittered. Opaque colors round-trip through PNG into the
/// RGBA8888 comparison space without premultiplication loss.
fn png(w: i32, h: i32, base: u8, patch: Option<(Rect, u8)>) -> Vec<u8> {
    let gray = |v: u8| Color4f::from(Color::from_rgb(v, v, v));
    let mut surface = surfaces::raster_n32_premul((w, h)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(gray(base));
    if let Some((rect, value)) = patch {
        let mut paint = Paint::new(gray(value), None);
        paint.set_anti_alias(false);
        canvas.draw_rect(rect, &paint);
    }
    surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("PNG encode")
        .as_bytes()
        .to_vec()
}

/// A `w`×`h` PNG filled with one straight-alpha RGBA color, assembled by hand
/// (8-bit RGBA, one filter-0 scanline per row, a zlib "stored" IDAT) rather
/// than produced through skia. skia's PNG encoder normalizes every fully
/// transparent pixel to (0,0,0,0), which would erase the straight-alpha RGB the
/// #290 lock test authors under alpha 0. A hand-authored PNG carries that RGB,
/// and skia's *decoder* reads it back faithfully when the image is decoded as
/// unpremultiplied — the exact input needed to test what `oracle::diff` does
/// with a transparent pixel that has a non-zero color.
fn solid_rgba_png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    // One scanline: a filter-type byte (0 = None) then `w` RGBA pixels.
    let mut row = Vec::with_capacity(1 + (w * 4) as usize);
    row.push(0u8);
    for _ in 0..w {
        row.extend_from_slice(&rgba);
    }
    let mut raw = Vec::with_capacity(h as usize * row.len());
    for _ in 0..h {
        raw.extend_from_slice(&row);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, no filter/interlace

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    png_chunk(&mut png, b"IHDR", &ihdr);
    png_chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    png_chunk(&mut png, b"IEND", &[]);
    png
}

/// Appends one `length | type | data | CRC32` PNG chunk.
fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Wraps `data` in a zlib stream of DEFLATE "stored" (uncompressed) blocks —
/// no compression, so no compressor dependency is needed to author a PNG.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header (CMF, FLG), check value valid
    let mut offset = 0;
    while offset < data.len() {
        let end = (offset + 0xFFFF).min(data.len());
        let block = &data[offset..end];
        out.push(u8::from(end >= data.len())); // BFINAL on the last block, BTYPE 0
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
        offset = end;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

const BANDS: [&ToleranceBand; 3] = [&AA_EDGE, &BLUR_FALLOFF, &MSDF_TEXT];

#[test]
fn an_identical_design_source_passes_every_band() {
    let reference = png(100, 100, 120, None);
    let design_source = png(100, 100, 120, None);

    for band in BANDS {
        let d = oracle::diff(&reference, &design_source, band).expect("same size");
        assert_eq!(
            d.differing, 0,
            "identical images differ nowhere ({})",
            band.rule
        );
        assert_eq!(d.max_channel_delta, 0);
        assert!(d.passes(), "identical images pass {}", band.rule);
    }
}

#[test]
fn a_sub_threshold_difference_is_measured_but_passes() {
    // Every pixel differs by 20, below AA_EDGE's 40 per-pixel threshold, so
    // no pixel counts as differing — the sub-threshold noise a design-source
    // export carries against a CPU render is absorbed. The measured max delta
    // still reports the difference is real.
    let reference = png(100, 100, 120, None);
    let design_source = png(100, 100, 140, None);

    let d = oracle::diff(&reference, &design_source, &AA_EDGE).expect("same size");
    assert_eq!(d.max_channel_delta, 20, "the 20/255 difference is measured");
    assert_eq!(
        d.differing, 0,
        "no pixel exceeds the 40 per-pixel threshold"
    );
    assert!(d.passes());
}

#[test]
fn a_difference_above_the_band_fails() {
    // AA_EDGE allows 2% of pixels to differ; a 30×30 patch is 9% of the
    // 100×100 canvas, each patch pixel 60 above the base (over the 40
    // threshold), so it fails.
    let reference = png(100, 100, 120, None);
    let design_source = png(
        100,
        100,
        120,
        Some((Rect::from_xywh(0.0, 0.0, 30.0, 30.0), 180)),
    );

    let d = oracle::diff(&reference, &design_source, &AA_EDGE).expect("same size");
    assert_eq!(d.differing, 900, "the 30×30 patch differs");
    assert_eq!(d.total, 10_000);
    assert_eq!(d.max_channel_delta, 60);
    assert!(
        !d.passes(),
        "9% differing over the 2% AA_EDGE budget must fail (measured {:.3})",
        d.fraction()
    );
}

#[test]
fn a_sparse_above_threshold_difference_within_budget_passes() {
    // The same 60-delta patch, but only 10×10 = 100 px = 1% of the canvas,
    // under AA_EDGE's 2% budget — the fraction budget is what accepts it.
    let reference = png(100, 100, 120, None);
    let design_source = png(
        100,
        100,
        120,
        Some((Rect::from_xywh(0.0, 0.0, 10.0, 10.0), 180)),
    );

    let d = oracle::diff(&reference, &design_source, &AA_EDGE).expect("same size");
    assert_eq!(d.differing, 100);
    assert_eq!(
        d.max_channel_delta, 60,
        "the patch pixels are over the threshold"
    );
    assert!(d.passes(), "1% differing under the 2% budget passes");
}

#[test]
fn a_difference_confined_to_an_excluded_region_does_not_count() {
    // A per-frame excluded region removes its pixels from both the numerator
    // and the denominator (`oracle::diff_excluding`). A 20×20 patch that
    // differs by 80 (over AA_EDGE's threshold) is measured as 400 differing
    // pixels without an exclusion; excluding that exact rect drops all 400 from
    // the differing count AND drops them from the total, so the frame measures 0
    // differing over the pixels that remain. This is the masking a frame can use
    // for one genuine, disclosed structural divergence the area budget must not
    // silently absorb (`goldens/oracle/manifest.json`). No frame declares an
    // exclusion today — the text render path (#303) made `v08-grid-spans`'s
    // former text-cell exclusion unnecessary — but the mechanism stays available.
    let reference = png(100, 100, 120, None);
    let source = png(
        100,
        100,
        120,
        Some((Rect::from_xywh(10.0, 10.0, 20.0, 20.0), 200)),
    );

    // Baseline: without the exclusion the patch is counted.
    let plain = oracle::diff(&reference, &source, &AA_EDGE).expect("same size");
    assert_eq!(plain.differing, 400, "the 20×20 patch differs");
    assert_eq!(plain.total, 10_000);
    assert_eq!(plain.max_channel_delta, 80);

    // Excluding the patch's exact rect removes every differing pixel from both
    // the numerator and the denominator.
    let mask = [oracle::ExcludeRegion {
        x: 10,
        y: 10,
        w: 20,
        h: 20,
    }];
    let masked = oracle::diff_excluding(&reference, &source, &AA_EDGE, &mask).expect("same size");
    assert_eq!(
        masked.differing, 0,
        "every differing pixel lies inside the excluded rect"
    );
    assert_eq!(
        masked.total, 9_600,
        "the 400 excluded pixels leave the denominator too (10000 − 400)"
    );
    assert_eq!(
        masked.max_channel_delta, 0,
        "the excluded pixels do not move the reported max delta"
    );
    assert!(
        masked.passes(),
        "0 differing over the remaining pixels passes"
    );
}

#[test]
fn an_empty_exclusion_is_exactly_the_plain_diff() {
    // The masking must be inert when no region is declared: `diff_excluding`
    // with an empty slice measures identically to `diff`, so a frame with no
    // `excludeRegions` is unaffected.
    let reference = png(100, 100, 120, None);
    let source = png(
        100,
        100,
        120,
        Some((Rect::from_xywh(0.0, 0.0, 30.0, 30.0), 200)),
    );

    let plain = oracle::diff(&reference, &source, &AA_EDGE).expect("same size");
    let empty = oracle::diff_excluding(&reference, &source, &AA_EDGE, &[]).expect("same size");
    assert_eq!(
        (plain.differing, plain.total, plain.max_channel_delta),
        (empty.differing, empty.total, empty.max_channel_delta)
    );
}

#[test]
fn each_band_enforces_its_own_budget_above_fails_below_passes() {
    // Every band, not only AA_EDGE, must reject a difference above its own
    // budget and accept one below it — proving each band applies its own
    // `channel_delta` and `differing_fraction`. A full-width strip on the
    // 100-row canvas makes the differing fraction exactly rows/100, and each
    // strip pixel is 70 above the base — over every band's `channel_delta`
    // (the largest is MSDF_TEXT's 50) — so it counts as differing under any
    // band, isolating the fraction budget as the deciding factor.
    //
    // This asserts `within_residual`, not `passes`: the property under test is
    // that each band applies its own `channel_delta` and `differing_fraction`.
    // A gated band's `passes` is the conjunction of that budget and a second
    // one, and a 70 delta is over `blur-falloff`'s gate threshold too, so
    // grading through `passes` here would measure the gate rather than the
    // budget this test names. The gate has its own tests below.
    const W: i32 = 100;
    const H: i32 = 100;
    const BASE: u8 = 120;
    const PATCH: u8 = 190; // delta 70 > every band's channel_delta

    let reference = png(W, H, BASE, None);
    let strip = |rows: i32| {
        png(
            W,
            H,
            BASE,
            Some((Rect::from_xywh(0.0, 0.0, W as f32, rows as f32), PATCH)),
        )
    };

    for band in BANDS {
        // The band's budget expressed as a whole number of full-width rows.
        let budget_rows = (band.differing_fraction * H as f64).round() as i32;

        // One row above the budget: over the band → must fail.
        let above = oracle::diff(&reference, &strip(budget_rows + 1), band).expect("same size");
        assert_eq!(above.differing, ((budget_rows + 1) * W) as usize);
        assert_eq!(above.max_channel_delta, 70);
        assert!(
            !above.within_residual(),
            "{}: {} rows differ ({:.3}) over the {:.3} budget must fail",
            band.rule,
            budget_rows + 1,
            above.fraction(),
            band.differing_fraction,
        );

        // One row below the budget: within the band → must pass.
        let below = oracle::diff(&reference, &strip(budget_rows - 1), band).expect("same size");
        assert_eq!(below.differing, ((budget_rows - 1) * W) as usize);
        assert_eq!(below.max_channel_delta, 70);
        assert!(
            below.within_residual(),
            "{}: {} rows differ ({:.3}) under the {:.3} budget must pass",
            band.rule,
            budget_rows - 1,
            below.fraction(),
            band.differing_fraction,
        );
    }
}

#[test]
fn passes_grades_against_the_band_diff_was_computed_with() {
    // #291: `passes()` must grade against the band `diff` was called with, so a
    // caller cannot count differing pixels under one band's per-pixel threshold
    // and then grade that count against a different band's area budget. The band
    // is carried on the `OracleDiff`, and `passes()` takes no band argument, so
    // the mismatch is unrepresentable.
    //
    // A 5-row full-width strip on a 100-row canvas differs in exactly 5% of
    // pixels, each 70 over the base — above every band's `channel_delta`, so the
    // differing *count* is identical under any band. Only the area budget
    // decides the verdict: 5% is over AA_EDGE's 2% budget but under
    // BLUR_FALLOFF's 12%.
    let reference = png(100, 100, 120, None);
    let source = png(
        100,
        100,
        120,
        Some((Rect::from_xywh(0.0, 0.0, 100.0, 5.0), 190)),
    );

    let strict = oracle::diff(&reference, &source, &AA_EDGE).expect("same size");
    let lax = oracle::diff(&reference, &source, &BLUR_FALLOFF).expect("same size");

    // The same image yields the same differing count under either band.
    assert_eq!(strict.differing, 500);
    assert_eq!(lax.differing, 500);
    assert_eq!(strict.fraction(), 0.05);

    // Each diff carries the band it was computed against …
    assert_eq!(strict.band.rule, "aa-edge");
    assert_eq!(lax.band.rule, "blur-falloff");

    // … and the verdict follows that band's budget, not a re-supplied one.
    // Graded on the residual: `blur-falloff` also carries a gate, and a 70
    // delta is over the gate's threshold, so `passes` would report the gate's
    // verdict rather than the budget-selection property this test is about.
    assert!(
        !strict.within_residual(),
        "5% over AA_EDGE's 2% budget must fail"
    );
    assert!(
        lax.within_residual(),
        "5% under BLUR_FALLOFF's 12% budget must pass"
    );
}

#[test]
fn a_dimension_mismatch_is_an_error_not_a_pass() {
    let reference = png(100, 100, 120, None);
    let design_source = png(80, 80, 120, None);

    let err = oracle::diff(&reference, &design_source, &AA_EDGE)
        .expect_err("different sizes cannot be compared");
    assert!(
        err.contains("100x100") && err.contains("80x80"),
        "the error names both sizes: {err}"
    );
}

#[test]
fn the_three_rule_bands_are_pinned_and_distinct() {
    // The bands are pinned config (G-11: per-rule, not one global budget).
    // Asserting the exact values makes any retune a deliberate, reviewed
    // change rather than a silent drift — the same discipline as re-goldening.
    assert_eq!(AA_EDGE.rule, "aa-edge");
    assert_eq!(
        (AA_EDGE.channel_delta, AA_EDGE.differing_fraction),
        (40, 0.02)
    );

    assert_eq!(BLUR_FALLOFF.rule, "blur-falloff");
    assert_eq!(
        (BLUR_FALLOFF.channel_delta, BLUR_FALLOFF.differing_fraction),
        (24, 0.12)
    );

    assert_eq!(MSDF_TEXT.rule, "msdf-text");
    assert_eq!(
        (MSDF_TEXT.channel_delta, MSDF_TEXT.differing_fraction),
        (50, 0.03)
    );

    // A blurred falloff tolerates a wider area than a hard edge or glyph ink.
    // A compile-time invariant: the bands are const, so this is checked when
    // the crate builds, not only when the test runs.
    const _: () = assert!(
        BLUR_FALLOFF.differing_fraction > AA_EDGE.differing_fraction
            && BLUR_FALLOFF.differing_fraction > MSDF_TEXT.differing_fraction,
    );
}

#[test]
fn both_fully_transparent_pixels_never_count_as_differing_even_with_distinct_rgb() {
    // #290 asks that two fully transparent pixels (alpha 0) never count as
    // differing, however their RGB disagrees — nothing is drawn, so they are
    // visually identical. This is already guaranteed structurally: `oracle::diff`
    // decodes each PNG through skia's default (premultiplied) image, which
    // collapses every alpha-0 pixel to (0,0,0,0) on both sides before the
    // per-channel compare runs. This test *locks* that guarantee against a
    // future decode change: it feeds two hand-authored PNGs whose transparent
    // regions carry very different non-zero RGB, and asserts they still measure
    // as identical. If the decode ever preserved straight-alpha RGB, this test
    // would fail — that is exactly when the compare loop would need an explicit
    // alpha-0 guard.
    let a_rgb = [200u8, 50, 30];
    let b_rgb = [10u8, 180, 90];

    // Control: the same two RGBs, but opaque. This proves the hand-authored
    // PNGs really do carry distinct color and that `oracle::diff` would count
    // them when opaque — so the transparent case below is suppressing a real
    // RGB difference, not measuring one the encoder happened to erase.
    let opaque_a = solid_rgba_png(20, 20, [a_rgb[0], a_rgb[1], a_rgb[2], 255]);
    let opaque_b = solid_rgba_png(20, 20, [b_rgb[0], b_rgb[1], b_rgb[2], 255]);
    let control = oracle::diff(&opaque_a, &opaque_b, &AA_EDGE).expect("same size");
    assert_eq!(
        control.differing, 400,
        "control: the two colors differ everywhere when opaque"
    );
    assert!(
        control.max_channel_delta > AA_EDGE.channel_delta,
        "control: the opaque RGB delta ({}) is over the band threshold",
        control.max_channel_delta,
    );

    // Under test: the same two colors, both fully transparent.
    let transparent_a = solid_rgba_png(20, 20, [a_rgb[0], a_rgb[1], a_rgb[2], 0]);
    let transparent_b = solid_rgba_png(20, 20, [b_rgb[0], b_rgb[1], b_rgb[2], 0]);
    for band in BANDS {
        let d = oracle::diff(&transparent_a, &transparent_b, band).expect("same size");
        assert_eq!(
            d.differing, 0,
            "both fully transparent → no visible difference ({})",
            band.rule
        );
        assert_eq!(
            d.max_channel_delta, 0,
            "the meaningless RGB delta of transparent pixels is not reported ({})",
            band.rule
        );
        assert!(
            d.passes(),
            "both fully transparent passes every band ({})",
            band.rule
        );
    }
}

#[test]
fn transparent_versus_opaque_still_counts_as_differing() {
    // The alpha-0 guarantee is symmetric only when *both* sides are transparent.
    // A pixel fully transparent on one side but opaque on the other disagrees on
    // coverage — a real difference the oracle must keep counting. Same RGB on
    // both sides isolates the alpha disagreement as the sole cause.
    let rgb = [200u8, 50, 30];
    let transparent = solid_rgba_png(20, 20, [rgb[0], rgb[1], rgb[2], 0]);
    let opaque = solid_rgba_png(20, 20, [rgb[0], rgb[1], rgb[2], 255]);

    let d = oracle::diff(&transparent, &opaque, &AA_EDGE).expect("same size");
    assert_eq!(
        d.max_channel_delta, 255,
        "the alpha channel disagrees by the full range"
    );
    assert_eq!(
        d.differing, 400,
        "every pixel disagrees on coverage, so every pixel counts"
    );
    assert!(!d.passes(), "a full-coverage disagreement must fail");
}

// --- The corpus-frame ↔ design-source manifest (goldens/oracle/manifest.json) ---
//
// These tests check the manifest that carries each capture into the oracle:
// every frame names a known band and, when it declares a fixture, one that
// exists; and every frame either has a committed design source (status
// captured) or is explicitly pending. They run in the ordinary `test` job.

// --- The text render path (story #303) ---
//
// The oracle imports arbitrary Figma fixtures, some of which carry TEXT. A
// TEXT node measures to 0x0 unless the solver runs the typesetter measure seam
// (#29), and paints nothing unless a `GlyphRunTable` is staged for it. These
// helpers wire both, generalizing the single-node stagers the v0.5/v0.6/v0.7
// text goldens use (`v07_text_lowering.rs`, `v07_fallback.rs`) to every TEXT
// node in a lowered fixture.
//
// Font resolution is caller-side (there is no registry — `docs/design/
// typeset-latin.md`): the oracle shapes through one coverage cascade of the
// only committed, R7-reproducible atlases — Noto Sans (Latin) then Noto Sans
// Arabic. A Latin family the committed corpus does not provide (e.g. "Inter",
// the family every committed fixture authors) renders in Noto Sans, so its
// glyph *shapes* and advances differ from Figma's own render of that family.
// That substitution is a measured fidelity gap the diff surfaces, disclosed per
// frame in `goldens/oracle/manifest.json` — it is why a Latin-Inter frame is
// not wired as a passing design source (`goldens/oracle/README.md`).

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
const FONT_INTER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Regular.otf"
);
const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
const ATLAS_INTER_ASCII_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii"
);
const ATLAS_ARABIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// The one named cascade the oracle measures and stages every TEXT node
/// through: Noto Sans (font 0), Inter (font 1), Noto Sans Arabic (font 2).
/// The font index a shaped glyph carries indexes both this cascade and the
/// atlas list built in the same order (`[ascii, inter-ascii, arabic]`).
///
/// Inter joined at story #385, once #49 closed and the E7 freeze that had
/// kept this file's cascade private lifted.
/// `docs/decisions/corpus-ships-inter.md` deferred exactly this to
/// "whoever closes #49": the gate carried a disclosed Inter-to-Noto
/// substitution, and `v08-grid-spans` is where it showed.
///
/// One face per family, unlike the production walk's three Noto and four
/// Inter weights: every fixture this oracle measures is authored at weight
/// 400, so a wider cascade would add slots nothing selects. A heavier
/// fixture would resolve to the Regular face and say so as
/// `text.weight-substituted`.
fn oracle_typesetter() -> Typesetter {
    let load = |path: &str, what: &str| {
        Font::from_bytes(
            std::fs::read(path).unwrap_or_else(|e| panic!("corpus {what} font present: {e}")),
            0,
        )
        .unwrap_or_else(|e| panic!("{what} parses: {e}"))
    };
    Typesetter::with_named_font_families(vec![
        FontFamily::new(
            "Noto Sans",
            vec![WeightedFont::regular(load(FONT_LATIN, "Noto Sans"))],
        ),
        FontFamily::new(
            "Inter",
            vec![WeightedFont::regular(load(FONT_INTER, "Inter"))],
        ),
        FontFamily::new(
            "Noto Sans Arabic",
            vec![WeightedFont::regular(load(FONT_ARABIC, "Noto Sans Arabic"))],
        ),
    ])
}

/// Shapes `text` at `size` and places every glyph in absolute document space
/// (the painter moves nothing, P2: the node's box origin is added here),
/// splitting a new run wherever the cascade switched fonts so each run samples
/// the atlas of its own font. `atlases` is built in font-list order, so the
/// font index a glyph carries selects its atlas.
///
/// `family` and `weight` are the node's own (story #385). Both have to be
/// passed here and not only to the measure callback: the solve resolves a
/// family and a weight, so staging that ignored either would place one face's
/// glyph ids at another face's advances. This cascade declares one face per
/// family, so every request resolves to that face and neither argument can
/// change what renders today — they are passed so that widening the cascade
/// later cannot silently split staging from the measure. The other text axes
/// stay at their defaults, which is a separate, disclosed limitation of this
/// oracle (debt #306).
#[allow(clippy::too_many_arguments)]
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    origin: (f32, f32),
    text: &str,
    size: f32,
    color: dashpaint::Color,
    family: &str,
    weight: u16,
) -> Vec<GlyphRun> {
    let laid = ts.layout_styled(text, size, None, TextShape::default(), weight, family);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + g.y,
            };
            match runs.last_mut() {
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
                    atlas,
                    size,
                    color,
                    glyphs: vec![quad],
                    opacity: 1.0,
                }),
            }
        }
    }
    runs
}

/// Walks the committed arena and stages glyph runs for every TEXT node — one
/// or more runs per node, at the node's resolved box origin. A node is a text
/// leaf exactly when it carries both authored characters and a text style
/// (`dashscene_engine::text_context`); its style's `size` and `color` drive the
/// run. Non-text fixtures produce no runs.
fn stage_text(arena: &Arena, ts: &mut Typesetter, atlases: &[AtlasIndex]) -> Vec<GlyphRun> {
    fn walk(
        arena: &Arena,
        node: NodeId,
        ts: &mut Typesetter,
        atlases: &[AtlasIndex],
        out: &mut Vec<GlyphRun>,
    ) {
        if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
            let origin = origin_of(arena, node);
            out.extend(text_runs(
                ts,
                atlases,
                origin,
                text,
                style.size,
                style.color,
                &style.family,
                style.weight,
            ));
        }
        for &child in arena.children(node) {
            walk(arena, child, ts, atlases, out);
        }
    }
    let mut out = Vec::new();
    for &root in arena.roots() {
        walk(arena, root, ts, atlases, &mut out);
    }
    out
}

/// Imports a committed Figma fixture the way a real producer does — compile
/// through `dashc`'s `compile_figma` (`Profile::Core`), load the emitted
/// `.dsb`, re-solve through the one `TaffySolver` — then renders the committed
/// scene with the Skia reference painter and returns the PNG. This is the
/// reference half of the oracle: our own fresh render of the imported fixture,
/// sized to the root's solved rect, which the design source is diffed against.
///
/// The solver runs the typesetter measure seam (#29) so TEXT nodes size to
/// their shaped extent instead of collapsing to 0x0, and a `GlyphRunTable` is
/// staged for every TEXT node so text paints. Font resolution is the committed
/// Noto cascade — see the module note above for the Latin-family fidelity
/// caveat this carries.
fn render_fixture(name: &str, fixture_json: &str) -> Vec<u8> {
    let (bytes, report) = compile_figma(fixture_json, Profile::Core, &BTreeMap::new())
        .unwrap_or_else(|e| panic!("frame {name} fixture compiles: {e:?}"));
    // A clean fixture lowers with an empty report; a diagnostic would mean the
    // fixture is not renderable and must not be wired as a measured frame. This
    // guards only *lowering* diagnostics — an empty report does not certify the
    // render is faithful. Render-time fidelity (glyph shape and advance under
    // font substitution, atlas coverage) is caught by the diff against the
    // design source, and disclosed per frame in the manifest — not here.
    assert!(
        report.is_empty(),
        "frame {name} fixture lowers clean: {report}"
    );
    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit an empty transaction through a typesetter-backed solver
    // so a full solve runs the measure seam (the pattern the text goldens use).
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    // Stage glyph runs for every TEXT node. The atlases are pushed in the
    // cascade's font order (`[ascii, inter-ascii, arabic]`), so the font index
    // a shaped glyph carries selects its atlas.
    let mut glyphs = GlyphRunTable::new();
    let ascii = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    let inter = glyphs.push_atlas(load_atlas(ATLAS_INTER_ASCII_DIR));
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    for run in stage_text(&arena, &mut ts, &[ascii, inter, arabic]) {
        glyphs.push_run(run);
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &glyphs,
        None,
    );
    painter.png_bytes()
}

/// This oracle's manifest, walked through the shared harness (debt #338).
fn manifest() -> manifest::OracleManifest {
    manifest::OracleManifest::load(
        "oracle/manifest.json",
        "pending-265",
        "RENDER ORACLE (E7/G-11)",
    )
}

#[test]
fn every_frame_names_a_known_band_and_any_declared_fixture_exists() {
    manifest().assert_bands_and_fixtures();
}

#[test]
fn every_frame_declares_a_captured_source_or_is_pending_265() {
    // The #265 gate, asserted. The manifest spells its gate as `gate.issue`,
    // where the import oracle uses a top-level `issue`, so the field check
    // stays here while the per-frame accounting is shared.
    let m = manifest();
    assert_eq!(
        m.value()["gate"]["issue"].as_u64(),
        Some(265),
        "the manifest gate names issue #265"
    );
    m.assert_captured_or_pending();
}

/// The design-source assertion itself: for every frame that has a committed
/// design source, our fresh render of the frame's committed Figma fixture must
/// fall within the frame's band of Figma's REST `GET /images` export. Each
/// measured frame imports its fixture in-process ([`render_fixture`]) and diffs
/// the render against the export — no network, no pre-committed corpus golden.
///
/// This runs in the ordinary `test` job (no `#[ignore]`): it is hermetic and
/// fast (~0.05 s/frame). Nothing is fabricated (G-11), and the shared harness
/// asserts every frame is measured or pending, so a frame cannot be silently
/// dropped.
#[test]
fn the_reference_renders_match_their_design_source() {
    let repo = manifest::repo_root();
    manifest().measure(|frame| {
        let name = frame["frame"].as_str().expect("frame name");
        let fixture = frame["fixture"].as_str().unwrap_or_else(|| {
            panic!("frame {name} has a design source but names no fixture to render")
        });
        let fixture_json = std::fs::read_to_string(repo.join(fixture))
            .unwrap_or_else(|e| panic!("frame {name} fixture {fixture}: {e}"));
        render_fixture(name, &fixture_json)
    });
}

/// `blur-falloff` is the only band that declares a gate, and its numbers are
/// pinned like the bands' own (issue #422).
///
/// A gate is not a default. A band earns one when a measurement shows its
/// residual is wide enough to hide a defect the band exists to catch; nothing
/// measured on the other bands' frames does that, so writing a second number
/// for them would pin something no evidence chose — the same mistake in the
/// other direction.
#[test]
fn only_blur_falloff_declares_a_gate_and_its_numbers_are_pinned() {
    let gate = BLUR_FALLOFF
        .gate
        .as_ref()
        .expect("blur-falloff declares a gate");
    assert_eq!((gate.channel_delta, gate.differing_fraction), (40, 0.01));

    assert!(AA_EDGE.gate.is_none(), "aa-edge is deliberately ungated");
    assert!(
        MSDF_TEXT.gate.is_none(),
        "msdf-text is deliberately ungated"
    );

    // The gate is on a different axis from the residual, in both terms: a
    // higher per-pixel threshold and a narrower area budget. A gate that was
    // merely a tighter budget at the same threshold would replace the residual
    // rather than add a second job, which is the failure #422 named.
    const _: () = assert!(match &BLUR_FALLOFF.gate {
        Some(gate) =>
            gate.channel_delta > BLUR_FALLOFF.channel_delta
                && gate.differing_fraction < BLUR_FALLOFF.differing_fraction,
        None => false,
    });
}

/// The gate binds: a difference above it fails and one below it passes, with
/// the residual satisfied in both cases.
///
/// Both patches are far inside `blur-falloff`'s 12 % residual, so the residual
/// cannot be what decides either verdict. Only the gate can, which is what
/// makes this a test of the gate rather than of the band.
#[test]
fn the_blur_falloff_gate_binds_independently_of_the_residual() {
    const W: i32 = 100;
    const H: i32 = 100;
    const BASE: u8 = 120;
    // Delta 70: over the gate's 40 threshold, so these pixels count for it.
    const PATCH: u8 = 190;

    let reference = png(W, H, BASE, None);
    // 150 of 10000 pixels: 1.5 %, over the gate's 1 %.
    let over = png(
        W,
        H,
        BASE,
        Some((Rect::from_xywh(0.0, 0.0, 50.0, 3.0), PATCH)),
    );
    // 50 of 10000 pixels: 0.5 %, under it.
    let under = png(
        W,
        H,
        BASE,
        Some((Rect::from_xywh(0.0, 0.0, 50.0, 1.0), PATCH)),
    );

    let d = oracle::diff(&reference, &over, &BLUR_FALLOFF).expect("same size");
    assert_eq!(d.gate_differing, 150);
    assert_eq!(d.gate_fraction(), 0.015);
    assert!(
        d.within_residual(),
        "1.5% is far inside the 12% residual, so the residual is not what fails this"
    );
    assert!(!d.within_gate(), "1.5% is over the gate's 1% budget");
    assert!(!d.passes(), "a frame outside the gate does not pass");

    let d = oracle::diff(&reference, &under, &BLUR_FALLOFF).expect("same size");
    assert_eq!(d.gate_differing, 50);
    assert!(d.within_gate(), "0.5% is under the gate's 1% budget");
    assert!(d.passes());
}

/// Neither number is redundant: each catches a defect class the other passes.
///
/// This is the whole argument for there being two, and it is measurable rather
/// than rhetorical. The two synthetic defects below mirror the two real ones
/// recorded in `docs/technotes/2026-07-26-tolerance-band-coverage.md`:
///
/// - a **wide, low-amplitude** error is what moving the panel fill alpha from
///   0.20 to 0.35 produced — 23.559 % on this band's residual, 0.422 % at
///   threshold 40. The residual catches it; a gate never would.
/// - a **narrow, high-amplitude** error is what removing the effect produced —
///   the two shadow removals measured 4.351 % and 3.570 % on the residual,
///   nowhere near 12 %, while measuring 2.930 % and 2.018 % at threshold 40.
///   The gate catches those; the residual never would.
#[test]
fn the_residual_and_the_gate_each_catch_what_the_other_passes() {
    const W: i32 = 100;
    const H: i32 = 100;
    const BASE: u8 = 120;

    let reference = png(W, H, BASE, None);

    // Wide and low-amplitude: delta 30 is over the residual's 24 threshold and
    // under the gate's 40, across 20 % of the canvas.
    let wide = png(
        W,
        H,
        BASE,
        Some((Rect::from_xywh(0.0, 0.0, W as f32, 20.0), BASE + 30)),
    );
    let d = oracle::diff(&reference, &wide, &BLUR_FALLOFF).expect("same size");
    assert_eq!(d.differing, 2000, "20% of pixels exceed the 24 threshold");
    assert_eq!(d.gate_differing, 0, "none exceeds the gate's 40 threshold");
    assert!(!d.within_residual(), "20% is over the 12% residual");
    assert!(
        d.within_gate(),
        "the gate is blind to this defect — it is the residual's job"
    );
    assert!(!d.passes());

    // Narrow and high-amplitude: delta 70 is over both thresholds, across 2 %
    // of the canvas.
    let narrow = png(
        W,
        H,
        BASE,
        Some((Rect::from_xywh(0.0, 0.0, W as f32, 2.0), BASE + 70)),
    );
    let d = oracle::diff(&reference, &narrow, &BLUR_FALLOFF).expect("same size");
    assert_eq!(d.differing, 200);
    assert_eq!(d.gate_differing, 200);
    assert!(
        d.within_residual(),
        "2% is far inside the 12% residual — the residual is blind to this defect"
    );
    assert!(!d.within_gate(), "2% is over the gate's 1% budget");
    assert!(!d.passes());
}
