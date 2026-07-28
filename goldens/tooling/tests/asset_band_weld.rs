//! The weld between the two families of tolerance bands.
//!
//! The render oracle (`goldens::oracle`, exit criterion E7) grades a reference
//! render against its Figma export. The packer's band oracle
//! (`dashpack::band`, story #432) grades a candidate encoding against the
//! canonical texels. They are two families of bands over two different pairs of
//! images, and the v0.12 breakdown asked for the second to *reuse* the first's
//! vocabulary rather than invent a parallel one.
//!
//! Reuse is a claim, and a claim in a doc comment drifts. This file is the
//! check: one image pair, run through both implementations, with the three
//! reported numbers asserted equal. If either side changes its differing
//! predicate, its pass predicate, or what it does with the alpha channel, this
//! goes red.
//!
//! The two cannot share one implementation. `goldens::oracle` takes PNG bytes
//! and decodes them through skia, which the packer must not link; `dashpack`
//! takes decoded texels, which is what it already holds at the point it
//! measures. So the arithmetic is written twice on purpose and held together
//! here — the same pattern the asset-pipeline plan records for the encoder and
//! decoder pair.

use dashpack::band as pack_band;
use goldens::oracle;
use skia_safe::{AlphaType, ColorType, EncodedImageFormat, ImageInfo, images};

/// The extent of the weld fixture. 16x16 is 256 texels, enough to make a
/// differing *fraction* meaningful rather than a handful of counts.
const EXTENT: i32 = 16;

/// Encodes straight-alpha RGBA8888 as a PNG through skia — the form
/// `goldens::oracle::diff` takes.
///
/// Every texel below is fully opaque, which avoids skia's PNG encoder
/// normalising a fully transparent texel's colour to zero. The weld is about
/// the diff arithmetic, and it does not need a transparent texel to exercise
/// it; `render_oracle.rs` already covers that case for the render side.
fn png(rgba: &[u8]) -> Vec<u8> {
    let info = ImageInfo::new(
        (EXTENT, EXTENT),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let data = skia_safe::Data::new_copy(rgba);
    let image = images::raster_from_data(&info, data, EXTENT as usize * 4)
        .expect("the fixture is a valid raster image");
    image
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("skia encodes PNG")
        .as_bytes()
        .to_vec()
}

/// The canonical side: a deterministic ramp, fully opaque.
fn canonical() -> Vec<u8> {
    let mut out = Vec::with_capacity((EXTENT * EXTENT * 4) as usize);
    for i in 0..(EXTENT * EXTENT) {
        let v = (i % 200) as u8;
        out.extend_from_slice(&[v, v.wrapping_add(7), v.wrapping_add(13), 255]);
    }
    out
}

/// The candidate side: the canonical ramp with a per-texel offset that sweeps
/// 0..=63, so every threshold under test has texels on both sides of it.
///
/// The offset lands on a different channel every fourth texel, including the
/// alpha channel, so a diff that skipped a channel would report a lower
/// differing count than the other and the weld would fail.
fn candidate(canonical: &[u8]) -> Vec<u8> {
    let mut out = canonical.to_vec();
    for (i, texel) in out.chunks_exact_mut(4).enumerate() {
        let offset = (i % 64) as u8;
        let channel = i % 4;
        texel[channel] = texel[channel].saturating_sub(offset);
    }
    out
}

/// The thresholds the weld is measured at: the three pinned render bands and
/// the two pinned pack bands, so the check covers the values actually in use.
const THRESHOLDS: [u8; 5] = [
    2,  // dashpack HIFI_IMAGE_FILL
    8,  // dashpack LOFI_IMAGE_FILL
    24, // goldens BLUR_FALLOFF
    40, // goldens AA_EDGE
    50, // goldens MSDF_TEXT
];

#[test]
fn the_two_band_implementations_report_the_same_numbers() {
    let canonical = canonical();
    let candidate = candidate(&canonical);
    let canonical_png = png(&canonical);
    let candidate_png = png(&candidate);

    for threshold in THRESHOLDS {
        // The same two knobs on both sides. `differing_fraction` is deliberately
        // set so neither side can pass, because `passes` is graded separately
        // below and the counts are what this test welds.
        // No gate: this test welds the two crates' *counting* against each
        // other, and the packer's band has no gate to weld one against. A gate
        // here would add a term only one side can compute.
        let render: &'static oracle::ToleranceBand = Box::leak(Box::new(oracle::ToleranceBand {
            rule: "weld",
            channel_delta: threshold,
            differing_fraction: 0.5,
            gate: None,
        }));
        let packer: &'static pack_band::ToleranceBand =
            Box::leak(Box::new(pack_band::ToleranceBand {
                rule: "weld",
                channel_delta: threshold,
                differing_fraction: 0.5,
            }));

        let by_render = oracle::diff(&canonical_png, &candidate_png, render)
            .expect("the render oracle diffs two same-sized PNGs");
        let by_packer = pack_band::diff(&canonical, &candidate, packer)
            .expect("the packer diffs two same-sized texel buffers");

        assert_eq!(
            by_render.differing, by_packer.differing,
            "at threshold {threshold} the two implementations counted different texels"
        );
        assert_eq!(
            by_render.total, by_packer.total,
            "at threshold {threshold} the two implementations compared different totals"
        );
        assert_eq!(
            by_render.max_channel_delta, by_packer.max_channel_delta,
            "at threshold {threshold} the two implementations saw different peak deltas"
        );
        assert!(
            (by_render.fraction() - by_packer.fraction()).abs() < f64::EPSILON,
            "at threshold {threshold} the two implementations report different fractions"
        );
        assert_eq!(
            by_render.passes(),
            by_packer.passes(),
            "at threshold {threshold} the two implementations graded differently"
        );
    }
}

/// The fixture has to actually exercise the thresholds, or the weld above
/// could pass by measuring zero on both sides.
#[test]
fn the_weld_fixture_straddles_every_threshold_under_test() {
    let canonical = canonical();
    let candidate = candidate(&canonical);

    let mut previous = usize::MAX;
    for threshold in THRESHOLDS {
        let band: &'static pack_band::ToleranceBand =
            Box::leak(Box::new(pack_band::ToleranceBand {
                rule: "coverage",
                channel_delta: threshold,
                differing_fraction: 1.0,
            }));
        let measured = pack_band::diff(&canonical, &candidate, band).expect("same size");
        assert!(
            measured.differing > 0,
            "no texel exceeds threshold {threshold}, so the weld would be measuring nothing"
        );
        assert!(
            measured.differing < measured.total,
            "every texel exceeds threshold {threshold}, so the threshold is doing nothing"
        );
        assert!(
            measured.differing < previous,
            "a higher threshold must count fewer texels; {threshold} did not"
        );
        previous = measured.differing;
    }
}
