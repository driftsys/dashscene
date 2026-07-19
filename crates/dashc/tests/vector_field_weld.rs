//! The msdfgen weld (story B1, docs/wip/2026-07-19-B1-vector-msdf-design.md).
//!
//! The Figma import path runs `dashc.wasm`, so the C++ `msdfgen` /
//! `msdf-atlas-gen` (the offline glyph-atlas tool) can never ride it — the
//! field generator must be pure-Rust `fdsm`. This test welds fdsm to pinned
//! msdfgen: a reference MSDF is baked once by msdfgen for a canonical shape,
//! committed as a fixture, and this test bakes the same shape with fdsm and
//! asserts per-texel agreement. It mirrors the frozen-fixture discipline of
//! `schema_evolution.rs` and the glyph atlas's pinned-tool posture — CI needs
//! no C++ toolchain, and a fdsm bump must arrive as a deliberate re-weld.
//!
//! # What is compared
//!
//! Edge coloring is an implementation-defined heuristic: fdsm and msdfgen
//! assign the three MSDF channels differently, so a raw per-channel diff is
//! not the physically meaningful quantity. The painter reconstructs a signed
//! distance as the median of the three channels; that median is
//! coloring-invariant and is exactly what a glyph sample uses. The weld
//! asserts on the per-texel median-distance delta (in texels) and also
//! reports the raw per-channel deltas for the record.
//!
//! # Regenerating the reference
//!
//!     UPDATE_VECTOR_WELD_REFERENCE=1 cargo test -p dashc --test \
//!         vector_field_weld
//!
//! runs pinned msdfgen (found on PATH or via `MSDFGEN`) and rewrites the
//! committed reference. Not a routine step — only on a deliberate, reviewed
//! generator-pin bump.

use std::path::{Path, PathBuf};
use std::process::Command;

use dashc_wasm::figma::vector_field::{
    DEFAULT_DISTANCE_RANGE, DEFAULT_PX_PER_EM, VectorPath, WindingRule, bake_single, plan_field,
};

/// The pinned reference generator. Anything else is generator drift.
const REQUIRED_MSDFGEN_VERSION: &str = "1.13.0";
const MSDFGEN_ENV: &str = "MSDFGEN";
const UPDATE_ENV: &str = "UPDATE_VECTOR_WELD_REFERENCE";
const REFERENCE: &str = "tests/fixtures/weld_star_msdf.png";

/// The canonical weld shape: a five-point star (the fixture's
/// `star-5-point` kind — NONZERO, straight segments, sharp corners that
/// stress edge coloring). Vertices in shape space (y-down), outer radius 40
/// and inner radius 16 about (50, 50), starting at the top point.
fn star_vertices() -> Vec<(f64, f64)> {
    let cx = 50.0;
    let cy = 50.0;
    let outer = 40.0;
    let inner = 16.0;
    let mut v = Vec::with_capacity(10);
    for i in 0..10 {
        let r = if i % 2 == 0 { outer } else { inner };
        // Start at the top (-90 degrees) and step 36 degrees each vertex.
        let theta = -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::PI / 5.0;
        v.push((cx + r * theta.cos(), cy + r * theta.sin()));
    }
    v
}

fn star_path(vertices: &[(f64, f64)]) -> String {
    let mut s = format!("M {} {}", vertices[0].0, vertices[0].1);
    for (x, y) in &vertices[1..] {
        s.push_str(&format!(" L {x} {y}"));
    }
    s.push_str(" Z");
    s
}

/// The msdfgen shape description for the same vertices: a single contour of
/// straight edges closed with `#`.
fn star_shapedesc(vertices: &[(f64, f64)]) -> String {
    let mut s = String::from("{\n");
    for (x, y) in vertices {
        s.push_str(&format!("\t{x}, {y};\n"));
    }
    s.push_str("\t#\n}\n");
    s
}

fn reference_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(REFERENCE)
}

fn find_msdfgen() -> PathBuf {
    std::env::var_os(MSDFGEN_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("msdfgen"))
}

/// Runs pinned msdfgen to bake the reference field, checking the version
/// banner first (R7: a different generator is drift, not a pass).
fn generate_reference(vertices: &[(f64, f64)]) {
    let tool = find_msdfgen();

    let banner = Command::new(&tool)
        .arg("-version")
        .output()
        .expect("run msdfgen -version (install msdfgen or set MSDFGEN)");
    let banner = String::from_utf8_lossy(&banner.stdout);
    assert!(
        banner.contains(REQUIRED_MSDFGEN_VERSION),
        "msdfgen {REQUIRED_MSDFGEN_VERSION} required for the weld reference, banner was: {}",
        banner.trim()
    );

    let path = star_path(vertices);
    let plan = plan_field(
        &VectorPath {
            path: &path,
            winding: WindingRule::NonZero,
        },
        DEFAULT_PX_PER_EM,
        DEFAULT_DISTANCE_RANGE,
    )
    .expect("plan");

    let dir = std::env::temp_dir();
    let shapedesc = dir.join("weld_star.shape");
    std::fs::write(&shapedesc, star_shapedesc(vertices)).expect("write shapedesc");

    // msdfgen's -translate is in shape units and is applied before -scale;
    // the generator's translate is in texels, so divide it back out.
    let tx = plan.translate_x / plan.scale;
    let ty = plan.translate_y / plan.scale;
    let out = reference_path();
    std::fs::create_dir_all(out.parent().unwrap()).expect("mkdir fixtures");

    let status = Command::new(&tool)
        .args(["msdf", "-shapedesc"])
        .arg(&shapedesc)
        .args(["-scale", &plan.scale.to_string()])
        .args(["-translate", &tx.to_string(), &ty.to_string()])
        .args([
            "-dimensions",
            &plan.width.to_string(),
            &plan.height.to_string(),
        ])
        .args(["-pxrange", &DEFAULT_DISTANCE_RANGE.to_string()])
        .args(["-fillrule", "nonzero"])
        // fdsm writes the field y-down (shape +y maps to increasing image
        // rows); msdfgen defaults to y-up, so flip it to match.
        .arg("-yflip")
        .args(["-format", "png"])
        .arg("-o")
        .arg(&out)
        .status()
        .expect("run msdfgen");
    assert!(status.success(), "msdfgen exited {status}");
    eprintln!("{UPDATE_ENV}: wrote {}", out.display());
}

fn load_rgb(path: &Path) -> (u32, u32, Vec<u8>) {
    let img = image::open(path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .to_rgb8();
    (img.width(), img.height(), img.into_raw())
}

fn median(r: u8, g: u8, b: u8) -> u8 {
    r.max(g).min(b.max(r.min(g)))
}

#[test]
fn fdsm_welds_to_pinned_msdfgen() {
    let vertices = star_vertices();

    let regen = std::env::var_os(UPDATE_ENV).is_some_and(|v| v == "1");
    if regen || !reference_path().exists() {
        assert!(
            regen || std::env::var_os(UPDATE_ENV).is_none(),
            "reference missing"
        );
        generate_reference(&vertices);
    }

    let path = star_path(&vertices);
    let field = bake_single(
        &VectorPath {
            path: &path,
            winding: WindingRule::NonZero,
        },
        DEFAULT_PX_PER_EM,
        DEFAULT_DISTANCE_RANGE,
    )
    .expect("bake");

    let (rw, rh, reference) = load_rgb(&reference_path());
    assert_eq!(
        (rw, rh),
        (field.width, field.height),
        "reference and fdsm field dimensions must match"
    );

    let range = DEFAULT_DISTANCE_RANGE;
    let texel = |v: u8| (f64::from(v) / 255.0 - 0.5) * range;

    // One u8 quantization step is 1/255 of the range in texels.
    let quantum = range / 255.0;

    let n = (rw * rh) as usize;
    let mut max_channel = 0.0f64; // in texels
    let mut sum_channel = 0.0f64;
    let mut max_median = 0.0f64;
    let mut sum_median = 0.0f64;
    // Texels where the reconstructed channels disagree by more than a few
    // quantization steps — a gross coloring divergence the median would hide.
    let mut gross_channel = 0usize;
    let gross = 4.0 * quantum; // > 4 steps
    for i in 0..n {
        let mut worst = 0.0f64;
        for c in 0..3 {
            let d = (texel(field.rgb[i * 3 + c]) - texel(reference[i * 3 + c])).abs();
            worst = worst.max(d);
            sum_channel += d;
        }
        max_channel = max_channel.max(worst);
        if worst > gross {
            gross_channel += 1;
        }
        let fm = texel(median(
            field.rgb[i * 3],
            field.rgb[i * 3 + 1],
            field.rgb[i * 3 + 2],
        ));
        let rm = texel(median(
            reference[i * 3],
            reference[i * 3 + 1],
            reference[i * 3 + 2],
        ));
        let d = (fm - rm).abs();
        max_median = max_median.max(d);
        sum_median += d;
    }
    let mean_channel = sum_channel / (n * 3) as f64;
    let mean_median = sum_median / n as f64;
    let gross_fraction = gross_channel as f64 / n as f64;

    eprintln!(
        "weld deltas (texels): median max {max_median:.4} ({:.2} steps) mean {mean_median:.4}; \
         raw-channel max {max_channel:.4} mean {mean_channel:.4}; \
         gross-channel-divergence fraction {gross_fraction:.4} ({gross_channel}/{n}); \
         {rw}x{rh}",
        max_median / quantum
    );

    // Tolerances set empirically (see the story report). The median-distance
    // delta is the painter-relevant, coloring-invariant quantity: the measured
    // max is exactly one u8 quantization step (4/255 texels), mean ~0.002. The
    // committed reference is fixed, so the only cross-platform variability is
    // fdsm's own f64 math quantized to u8; the bounds leave a few quantization
    // steps of headroom for that while staying a real regression guard.
    const MEDIAN_MAX_TOL: f64 = 0.047; // ~3 quantization steps (3 * 4/255)
    const MEDIAN_MEAN_TOL: f64 = 0.010; // texels
    // The channel permutation at corners is confined to a tiny fraction of
    // texels; a coloring regression would inflate it.
    const GROSS_FRACTION_TOL: f64 = 0.02;
    assert!(
        max_median <= MEDIAN_MAX_TOL,
        "median-distance max delta {max_median:.4} texels exceeds {MEDIAN_MAX_TOL}"
    );
    assert!(
        mean_median <= MEDIAN_MEAN_TOL,
        "median-distance mean delta {mean_median:.4} texels exceeds {MEDIAN_MEAN_TOL}"
    );
    assert!(
        gross_fraction <= GROSS_FRACTION_TOL,
        "gross channel-divergence fraction {gross_fraction:.4} exceeds {GROSS_FRACTION_TOL}"
    );
}
