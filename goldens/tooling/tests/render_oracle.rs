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
        assert!(d.passes(band), "identical images pass {}", band.rule);
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
    assert!(d.passes(&AA_EDGE));
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
        !d.passes(&AA_EDGE),
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
    assert!(
        d.passes(&AA_EDGE),
        "1% differing under the 2% budget passes"
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
            !above.passes(band),
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
            below.passes(band),
            "{}: {} rows differ ({:.3}) under the {:.3} budget must pass",
            band.rule,
            budget_rows - 1,
            below.fraction(),
            band.differing_fraction,
        );
    }
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
                if !d.passes(band) {
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
