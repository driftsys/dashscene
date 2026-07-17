//! The design-source render oracle (story #284, exit criterion E7,
//! guardrail G-11): a perceptual diff of the reference painter's output
//! against a design source (Figma's REST `GET /images` export), with
//! per-rule tolerance bands.
//!
//! This file validates the diff harness and the pinned bands with
//! controlled **synthetic** image pairs — no design source is pretended.
//! The real design-source captures are authored manually and tracked by
//! issue #265 (parked), so the assertion that a frame's render matches its
//! real Figma export is `#[ignore]`-gated with a named #265 reason (see
//! `the_reference_renders_match_their_design_source` below) and is not run
//! by the ordinary `test` job. What runs here proves the math the gated
//! assertion depends on, honestly and without a real source.

use goldens::oracle::{self, AA_EDGE, BLUR_FALLOFF, MSDF_TEXT, ToleranceBand};
use skia_safe::{Color, Color4f, EncodedImageFormat, Paint, Rect, surfaces};

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
fn each_band_enforces_its_own_budget_above_fails_below_passes() {
    // Every band, not only AA_EDGE, must reject a difference above its own
    // budget and accept one below it — proving each band applies its own
    // `channel_delta` and `differing_fraction`. A full-width strip on the
    // 100-row canvas makes the differing fraction exactly rows/100, and each
    // strip pixel is 70 above the base — over every band's `channel_delta`
    // (the largest is MSDF_TEXT's 50) — so it counts as differing under any
    // band, isolating the fraction budget as the deciding factor.
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
            !above.passes(),
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
            below.passes(),
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
    assert!(!strict.passes(), "5% over AA_EDGE's 2% budget must fail");
    assert!(lax.passes(), "5% under BLUR_FALLOFF's 12% budget must pass");
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
// These tests prove the plumbing that carries a real capture into the oracle,
// without a real capture: the manifest names known bands and existing
// reference goldens, and every frame either has a committed design source or
// is explicitly pending #265. They run in the ordinary `test` job.

use serde_json::Value;

/// The `goldens/` root — one level up from this crate (`goldens/tooling`).
fn goldens_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn load_manifest() -> Value {
    let path = goldens_root().join("oracle/manifest.json");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("oracle manifest {} present: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("oracle manifest {} parses: {e}", path.display()))
}

fn frames(manifest: &Value) -> &Vec<Value> {
    manifest["frames"]
        .as_array()
        .expect("the manifest has a frames array")
}

#[test]
fn every_frame_names_a_known_band_and_an_existing_reference_image() {
    let manifest = load_manifest();
    let root = goldens_root();
    assert!(!frames(&manifest).is_empty(), "the manifest lists frames");

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name");
        let band = frame["band"].as_str().expect("band name");
        let resolved = oracle::band_for(band).unwrap_or_else(|| {
            panic!("frame {name} names band {band}, which is not one of the pinned rules")
        });
        assert_eq!(
            resolved.rule, band,
            "band_for({band}) must return the band whose rule matches the name, \
             not a mis-mapped band"
        );
        let reference = frame["referenceImage"].as_str().expect("referenceImage");
        let path = root.join(reference);
        assert!(
            path.exists(),
            "frame {name}'s reference golden {} is not committed",
            path.display()
        );
    }
}

#[test]
fn every_frame_declares_a_captured_source_or_is_pending_265() {
    // The #265 gate, asserted: a frame with no committed design source must
    // say so (status pending-265), and one that has a source must actually
    // ship the file. This stays valid after #265 lands — it checks each
    // frame's own state, never "all frames are pending".
    let manifest = load_manifest();
    let root = goldens_root();
    assert_eq!(
        manifest["gate"]["issue"].as_u64(),
        Some(265),
        "the manifest gate names issue #265"
    );

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name");
        match frame["designSource"].as_str() {
            None => assert_eq!(
                frame["status"].as_str(),
                Some("pending-265"),
                "frame {name} has no design source, so it must be marked pending-265"
            ),
            Some(source) => {
                let path = root.join(source);
                assert!(
                    path.exists(),
                    "frame {name} declares design source {} but the file is not committed",
                    path.display()
                );
                assert_eq!(
                    frame["status"].as_str(),
                    Some("captured"),
                    "frame {name} has a design source, so its status must be captured, \
                     not a stale pending-265"
                );
            }
        }
    }
}

/// The design-source assertion itself: each reference render must fall within
/// its band of the real Figma REST export. `#[ignore]`-gated because the real
/// exports are authored manually and tracked by the parked issue #265 — this
/// story delivers the tooling, not the assertion. The `render-oracle` CI job
/// runs it with `--ignored`. With no committed design source it measures
/// nothing and prints a pending summary naming #265; it never fabricates a
/// source, and E7 stays open in `docs/specification/05-qualification.md`.
#[test]
#[ignore = "design-source Figma REST image exports are authored manually and tracked by #265 (parked)"]
fn the_reference_renders_match_their_design_source() {
    let manifest = load_manifest();
    let root = goldens_root();

    let mut measured = 0usize;
    let mut pending: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name").to_string();
        let band_name = frame["band"].as_str().expect("band name");
        let band = oracle::band_for(band_name)
            .unwrap_or_else(|| panic!("frame {name} names unknown band {band_name}"));

        match frame["designSource"].as_str() {
            None => pending.push(name),
            Some(source) => {
                let source_bytes = std::fs::read(root.join(source))
                    .unwrap_or_else(|e| panic!("frame {name} design source {source}: {e}"));
                let reference = frame["referenceImage"].as_str().expect("referenceImage");
                let reference_bytes = std::fs::read(root.join(reference))
                    .unwrap_or_else(|e| panic!("frame {name} reference {reference}: {e}"));
                let d = oracle::diff(&reference_bytes, &source_bytes, band)
                    .unwrap_or_else(|e| panic!("frame {name}: {e}"));
                measured += 1;
                if !d.passes() {
                    failures.push(format!(
                        "{name}: {}/{} px differ ({:.3}%, max Δ {}) over the {} band's {:.1}% budget",
                        d.differing,
                        d.total,
                        d.fraction() * 100.0,
                        d.max_channel_delta,
                        band.rule,
                        band.differing_fraction * 100.0,
                    ));
                }
            }
        }
    }

    eprintln!(
        "RENDER ORACLE (E7/G-11): {measured} frame(s) measured against a design source, \
         {} pending #265{}",
        pending.len(),
        if pending.is_empty() {
            String::new()
        } else {
            format!(" ({})", pending.join(", "))
        }
    );

    // Test-lock the report's honesty: `assert!(failures.is_empty())` alone
    // passes even when nothing was measured, so the accounting must be
    // asserted too. Every frame is either measured against a real source or
    // pending #265 — nothing is silently dropped — and `pending` names exactly
    // the frames whose `designSource` is null. This is NOT
    // `assert!(pending.is_empty())`: that is the v0.9 exit gate's job (#49) and
    // would make this job red today. The current posture is green-with-loud-log
    // until #265 lands; only the accounting is enforced here.
    let expected_pending: Vec<String> = frames(&manifest)
        .iter()
        .filter(|frame| frame["designSource"].as_str().is_none())
        .map(|frame| frame["frame"].as_str().expect("frame name").to_string())
        .collect();
    assert_eq!(
        measured + pending.len(),
        frames(&manifest).len(),
        "every manifest frame must be measured or pending — none silently dropped"
    );
    assert_eq!(
        pending, expected_pending,
        "pending must be exactly the frames whose designSource is null"
    );

    assert!(
        failures.is_empty(),
        "design-source fidelity failures:\n{}",
        failures.join("\n")
    );
}
