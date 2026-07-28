//! Two published perceptual scales, and PSNR for comparability — the
//! calibration behind `dashpack`'s tolerance bands (issue #544).
//!
//! # Why a second family of numbers
//!
//! `dashpack::band` grades a candidate encoding with a per-texel threshold and
//! an area budget. Those are gates, each with a measured mutation that fails
//! it, and they are internal to this project: nothing in them says where the
//! rung they choose lands on a scale a reader outside this repository would
//! recognise. These metrics answer that, and they gate nothing — a floor
//! asserted against one of them fails a test, it never changes which rung the
//! packer selects.
//!
//! - **SSIMULACRA2**, from the JPEG XL work. Published scale: roughly 90 and
//!   above visually lossless, 70 and above high quality, 50 medium, 30 low.
//! - **FLIP** (NVIDIA), built for comparing *rendered* images. Reported as the
//!   mean error over the image, where 0 is identical and 1 is maximally
//!   different.
//! - **PSNR** is recorded because everyone reports it, and decides nothing. It
//!   correlates poorly with perception.
//!
//! # What these cannot see
//!
//! **Alpha.** SSIMULACRA2 and FLIP are both defined over RGB and neither reads
//! an alpha channel, while `dashpack::band::diff` compares alpha like any other
//! channel — and on an image fill a codec that drops coverage is one of the
//! more visible failures. [`Scores::psnr_alpha`] is the column that can see it,
//! and `an_alpha_only_difference_is_invisible_to_both_perceptual_metrics`
//! measures the blind spot rather than describing it.
//!
//! **Small images.** SSIMULACRA2 is multi-scale: it rescales by half up to six
//! times and refuses anything below 8x8, so at 16x16 only two of its six scales
//! survive and the score stops meaning what it means elsewhere. So [`score`]
//! withholds the number below [`SSIMULACRA2_MIN_EXTENT`] rather than reporting
//! one that reads as comparable. FLIP and PSNR have no such floor and are
//! reported at every extent.
//!
//! The floor is set above the metric's own 8x8 because a score that is
//! *produced* is not the same as a score that is *comparable*. The probe behind
//! that judgement is recorded in
//! `docs/archive/2026-07-28-perceptual-band-calibration-design.md`: the same
//! 4-bit quantisation scored 12.59 on a 380 px corpus payload and 92.86 on a
//! 16 px one. Those two figures come from that probe and not from any test in
//! this repository — cited rather than restated as a measurement, because no
//! check here reproduces them.
//!
//! **A human.** These are models of perception, not observers. The triptych
//! (`just triptych`) is where a person is asked, under the viewing conditions
//! its recipe documents.

use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

/// The smallest extent at which an SSIMULACRA2 score is reported.
///
/// The metric's own floor is 8x8. This is higher, because a score that is
/// produced is not the same as a score that is comparable: see the module
/// documentation for the measurement behind the number.
pub const SSIMULACRA2_MIN_EXTENT: u32 = 64;

/// FLIP's published default viewing condition, 67 pixels per degree.
///
/// The library ships this as a rounded constant, not as a computed geometry: it
/// corresponds to roughly 0.7 m from a 3840 px, 0.7 m wide monitor, which
/// computes to 67.02. The constant is used rather than the computation because
/// it is the value every published FLIP figure is measured at, and
/// comparability is the only reason this column exists.
pub fn desk_ppd() -> f32 {
    nv_flip::DEFAULT_PIXELS_PER_DEGREE
}

/// An automotive centre display: 0.9 m viewing distance, 1920 px across a
/// 0.28 m wide panel.
///
/// **This geometry is not specified anywhere in this repository.**
/// `docs/specification/03-target-hardware-rules.md` pins GPU class, render-pass
/// rules and texture policy, and no display geometry at all, so this is a
/// stated assumption of the calibration rather than a value read from the
/// specification — issue #549 proposes pinning it there. It is reported beside
/// [`desk_ppd`] so the sensitivity to viewing distance is visible rather than
/// hidden inside a default.
///
/// That sensitivity turned out to matter. A pre-implementation probe using bit
/// quantisation as a stand-in for codec loss put the two conditions about 3 %
/// apart, and the design written from it expected this column to be nearly
/// redundant. Measured on real block-compression error in
/// `goldens/tooling/tests/perceptual_calibration.rs`, they disagree by 14 % to
/// 32 % — worst case 32.3 %, `block-stress` at astc-4x4. The stand-in was not
/// representative of the error the codec actually produces, which is why the
/// column is load-bearing and why #549 is worth closing.
pub fn panel_ppd() -> f32 {
    nv_flip::pixels_per_degree(0.9, 1920.0, 0.28)
}

/// What the three metrics measured for one reference-candidate pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scores {
    /// SSIMULACRA2, 0 to 100, higher is better. `None` below
    /// [`SSIMULACRA2_MIN_EXTENT`].
    pub ssimulacra2: Option<f64>,
    /// Mean FLIP error at [`desk_ppd`], 0 to 1, lower is better.
    pub flip_desk: f64,
    /// Mean FLIP error at [`panel_ppd`].
    pub flip_panel: f64,
    /// PSNR in dB over the three colour channels; infinite for an exact match.
    pub psnr_rgb: f64,
    /// PSNR in dB over the alpha channel alone — the channel neither perceptual
    /// metric reads.
    pub psnr_alpha: f64,
}

/// Why a pair could not be scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricError {
    /// The two buffers hold different numbers of bytes. Scoring them would mean
    /// choosing which texels to drop, so it is refused.
    TexelCount { reference: usize, candidate: usize },
    /// A buffer's length is not a whole number of 8-bit RGBA texels.
    NotRgba { len: usize },
    /// The buffers do not hold `width * height` texels.
    Extent { width: u32, height: u32, len: usize },
}

impl std::fmt::Display for MetricError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TexelCount {
                reference,
                candidate,
            } => write!(
                f,
                "the reference holds {reference} bytes and the candidate {candidate}; a pair must \
                 be the same extent before it can be scored"
            ),
            Self::NotRgba { len } => {
                write!(f, "{len} bytes is not a whole number of 8-bit RGBA texels")
            }
            Self::Extent { width, height, len } => write!(
                f,
                "{width}x{height} needs {} bytes, not {len}",
                *width as usize * *height as usize * 4
            ),
        }
    }
}

impl std::error::Error for MetricError {}

/// A score at a fixed number of decimal places, for recording and comparing as
/// a string.
///
/// String equality at a pinned precision is the tolerance: it is exact where
/// float equality is a judgement call, and it is the convention
/// `crates/dashpack/tests/band_contract.rs` and the profile-preview oracle both
/// already use for their fractions. An infinite PSNR — an exact match — records
/// as `lossless` rather than as `inf`, which reads as a missing value.
pub fn fixed(value: f64, decimals: usize) -> String {
    if value.is_infinite() {
        return "lossless".to_string();
    }
    format!("{value:.decimals$}")
}

/// Scores `candidate` against `reference`, both 8-bit RGBA with rows top to
/// bottom and no padding.
pub fn score(
    width: u32,
    height: u32,
    reference: &[u8],
    candidate: &[u8],
) -> Result<Scores, MetricError> {
    if reference.len() != candidate.len() {
        return Err(MetricError::TexelCount {
            reference: reference.len(),
            candidate: candidate.len(),
        });
    }
    if !reference.len().is_multiple_of(4) {
        return Err(MetricError::NotRgba {
            len: reference.len(),
        });
    }
    if reference.len() != width as usize * height as usize * 4 {
        return Err(MetricError::Extent {
            width,
            height,
            len: reference.len(),
        });
    }

    Ok(Scores {
        ssimulacra2: ssimulacra2_of(width, height, reference, candidate),
        flip_desk: flip_mean(width, height, reference, candidate, desk_ppd()),
        flip_panel: flip_mean(width, height, reference, candidate, panel_ppd()),
        psnr_rgb: psnr(reference, candidate, &[0, 1, 2]),
        psnr_alpha: psnr(reference, candidate, &[3]),
    })
}

/// SSIMULACRA2, or `None` below [`SSIMULACRA2_MIN_EXTENT`].
fn ssimulacra2_of(width: u32, height: u32, reference: &[u8], candidate: &[u8]) -> Option<f64> {
    if width < SSIMULACRA2_MIN_EXTENT || height < SSIMULACRA2_MIN_EXTENT {
        return None;
    }
    // The metric takes RGB with its transfer function named and linearises
    // internally, so the sRGB-encoded texels are handed over as sRGB rather
    // than linearised here, where a second implementation of the transfer
    // function could disagree with the metric's own.
    let as_rgb = |texels: &[u8]| {
        let data: Vec<[f32; 3]> = texels
            .chunks_exact(4)
            .map(|t| {
                [
                    t[0] as f32 / 255.0,
                    t[1] as f32 / 255.0,
                    t[2] as f32 / 255.0,
                ]
            })
            .collect();
        Rgb::new(
            data,
            width as usize,
            height as usize,
            TransferCharacteristic::SRGB,
            ColorPrimaries::BT709,
        )
        .expect("the texel count was checked against the extent above")
    };
    Some(
        compute_frame_ssimulacra2(as_rgb(reference), as_rgb(candidate))
            .expect("both frames are the same checked extent, above the metric's own 8x8 floor"),
    )
}

/// The mean FLIP error at `ppd`. FLIP reads RGB, so alpha is dropped here and
/// carried by [`Scores::psnr_alpha`] instead.
fn flip_mean(width: u32, height: u32, reference: &[u8], candidate: &[u8], ppd: f32) -> f64 {
    let rgb = |texels: &[u8]| -> Vec<u8> {
        texels
            .chunks_exact(4)
            .flat_map(|t| [t[0], t[1], t[2]])
            .collect()
    };
    let error = nv_flip::flip(
        nv_flip::FlipImageRgb8::with_data(width, height, &rgb(reference)),
        nv_flip::FlipImageRgb8::with_data(width, height, &rgb(candidate)),
        ppd,
    );
    nv_flip::FlipPool::from_image(&error).mean() as f64
}

/// PSNR in dB over the named channel offsets, infinite for an exact match.
fn psnr(reference: &[u8], candidate: &[u8], channels: &[usize]) -> f64 {
    let mut squared = 0.0f64;
    let mut count = 0usize;
    for (a, b) in reference.chunks_exact(4).zip(candidate.chunks_exact(4)) {
        for &channel in channels {
            let delta = a[channel] as f64 - b[channel] as f64;
            squared += delta * delta;
            count += 1;
        }
    }
    if count == 0 {
        return f64::INFINITY;
    }
    let mse = squared / count as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth gradient with an opaque alpha channel, large enough for
    /// SSIMULACRA2's six scales.
    fn gradient(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                out.push((x * 255 / width.max(1)) as u8);
                out.push((y * 255 / height.max(1)) as u8);
                out.push(((x + y) * 255 / (width + height).max(1)) as u8);
                out.push(255);
            }
        }
        out
    }

    #[test]
    fn an_identical_pair_scores_perfectly_on_every_metric() {
        let image = gradient(128, 128);
        let scores = score(128, 128, &image, &image).expect("equal buffers score");
        assert_eq!(
            fixed(scores.ssimulacra2.expect("128 is above the floor"), 2),
            "100.00"
        );
        assert_eq!(fixed(scores.flip_desk, 4), "0.0000");
        assert_eq!(fixed(scores.flip_panel, 4), "0.0000");
        assert!(
            scores.psnr_rgb.is_infinite(),
            "an exact match has no error to report"
        );
        assert!(scores.psnr_alpha.is_infinite());
    }

    /// The blind spot, measured rather than asserted in prose: SSIMULACRA2 and
    /// FLIP are both defined over RGB, so an alpha-only difference is invisible
    /// to both. The alpha PSNR column exists because of this test.
    #[test]
    fn an_alpha_only_difference_is_invisible_to_both_perceptual_metrics() {
        let reference = gradient(128, 128);
        let mut candidate = reference.clone();
        for texel in candidate.chunks_exact_mut(4) {
            texel[3] = 0;
        }
        let scores = score(128, 128, &reference, &candidate).expect("equal buffers score");
        assert_eq!(
            fixed(scores.ssimulacra2.expect("above the floor"), 2),
            "100.00"
        );
        assert_eq!(fixed(scores.flip_desk, 4), "0.0000");
        assert!(scores.psnr_rgb.is_infinite(), "no colour channel moved");
        assert!(
            scores.psnr_alpha.is_finite(),
            "the alpha column is the only one that can see this, so it must"
        );
    }

    /// Below the floor the score is withheld rather than reported, because
    /// SSIMULACRA2 reaches only two of its six scales at that extent and the
    /// number would not mean what the other rows mean.
    #[test]
    fn ssimulacra2_is_withheld_below_the_minimum_extent() {
        let small = gradient(16, 16);
        let mut candidate = small.clone();
        candidate[0] = candidate[0].wrapping_add(64);
        let scores = score(16, 16, &small, &candidate).expect("a small pair still scores");
        assert_eq!(
            scores.ssimulacra2, None,
            "16 is below {SSIMULACRA2_MIN_EXTENT}"
        );
        assert!(scores.flip_desk > 0.0, "FLIP has no minimum extent");
        assert!(scores.psnr_rgb.is_finite());
    }

    #[test]
    fn a_length_mismatch_is_refused_rather_than_truncated() {
        let error = score(2, 2, &[0u8; 16], &[0u8; 8]).expect_err("a mismatch is an error");
        assert_eq!(
            error,
            MetricError::TexelCount {
                reference: 16,
                candidate: 8
            }
        );
    }

    #[test]
    fn a_buffer_that_is_not_whole_texels_is_refused() {
        let error = score(1, 1, &[0u8; 6], &[0u8; 6]).expect_err("a partial texel is an error");
        assert_eq!(error, MetricError::NotRgba { len: 6 });
    }

    #[test]
    fn a_buffer_that_does_not_match_the_extent_is_refused() {
        let error = score(4, 4, &[0u8; 16], &[0u8; 16]).expect_err("4x4 needs 64 bytes");
        assert_eq!(
            error,
            MetricError::Extent {
                width: 4,
                height: 4,
                len: 16
            }
        );
    }

    /// The two viewing conditions, so a later reader can check the geometry
    /// behind each without rederiving it.
    ///
    /// The desk value is FLIP's shipped constant, a rounded 67 rather than the
    /// 67.02 its documented geometry computes — recorded as what the library
    /// gives, because that is the value every published FLIP figure carries.
    #[test]
    fn the_two_viewing_conditions_are_the_documented_ones() {
        assert_eq!(fixed(desk_ppd() as f64, 2), "67.00");
        assert_eq!(
            fixed(nv_flip::pixels_per_degree(0.7, 3840.0, 0.7) as f64, 2),
            "67.02",
            "the geometry the shipped constant rounds"
        );
        assert_eq!(fixed(panel_ppd() as f64, 2), "107.71");
    }
}
