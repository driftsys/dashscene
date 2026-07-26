//! The per-asset tolerance band — how far a derived payload may sit from the
//! canonical one before the packer refuses it.
//!
//! # The same vocabulary as the render oracle, on a different pair of images
//!
//! The render oracle (`goldens/tooling/src/oracle.rs`, exit criterion E7)
//! grades a reference render against its Figma export with a *per-pixel
//! threshold plus an area budget*: a pixel counts as differing when its largest
//! per-channel absolute delta exceeds the threshold, and the frame passes when
//! the differing fraction is at or below the budget. This module applies that
//! same two-knob rule to a different pair of images — a canonical payload's
//! texels against the texels a candidate encoding decodes back to.
//!
//! It is a second family of bands, not a second vocabulary. The knob names,
//! the differing predicate, and the pass predicate are the render oracle's, and
//! `goldens/tooling/tests/asset_band_weld.rs` holds the two implementations to
//! that by measuring one image pair through both and asserting the three
//! reported numbers are equal.
//!
//! # Why the numbers are not the render oracle's numbers
//!
//! The render bands are wide (per-pixel thresholds of 24 to 50) because the two
//! sides of that comparison are produced by different machines: a CPU
//! rasterizer against a server-side export, disagreeing on anti-aliasing,
//! resampling, hinting and gamma. None of that noise exists here. Both sides of
//! a pack diff are the same texel grid at the same size, and the only thing
//! that can differ is codec error. A pack band that inherited a render band's
//! width would be measuring nothing. The pinned values live in
//! [`crate::profile`], with the measured mutation that fails each one.
//!
//! For the same reason there is no counterpart to the render oracle's
//! `ExcludeRegion`. That mechanism exists so a frame carrying one genuine,
//! disclosed structural divergence — a real placement disagreement — can
//! measure the rest of itself honestly. A codec has no structural divergence to
//! disclose: it produces a texel for every canonical texel, at the same
//! coordinates. A region excluded here would only be a region the packer
//! declined to look at.

/// A per-asset-class perceptual tolerance band, in the render oracle's
/// vocabulary: a per-pixel threshold and an area budget.
///
/// A texel counts as differing only when its largest per-channel absolute delta
/// (0..=255) exceeds `channel_delta`; a candidate passes when the differing
/// fraction is at or below `differing_fraction`.
#[derive(Debug, PartialEq)]
pub struct ToleranceBand {
    /// The contract this band governs — a profile and an asset class, e.g.
    /// `hifi-image-fill`.
    pub rule: &'static str,
    /// The per-texel threshold: a texel whose max per-channel absolute delta
    /// exceeds this counts as differing.
    pub channel_delta: u8,
    /// The pass ceiling: the fraction of texels (0.0..=1.0) allowed to exceed
    /// `channel_delta`.
    pub differing_fraction: f64,
}

/// The measured outcome of one candidate-against-canonical diff.
///
/// Carries the numbers a refusal report needs — fidelity is a measured value,
/// not a bare pass/fail (guardrail G-11) — and the band it was measured
/// against, so a verdict cannot be graded against a different band than the one
/// that produced `differing`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandDiff {
    /// Texels whose max per-channel delta exceeded the band's `channel_delta`.
    pub differing: usize,
    /// Total texels compared.
    pub total: usize,
    /// The largest per-channel absolute delta seen at any texel — reports that
    /// a difference exists even when no texel crossed the threshold, which is
    /// how a lossless rung is told apart from a merely passing one.
    pub max_channel_delta: u8,
    /// The band [`diff`] applied to produce `differing`. [`BandDiff::passes`]
    /// grades against this band.
    pub band: &'static ToleranceBand,
}

impl BandDiff {
    /// The share of texels that exceeded the band's per-texel threshold.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.differing as f64 / self.total as f64
        }
    }

    /// Whether the measured difference is within the area budget of the band
    /// [`diff`] was called with.
    pub fn passes(&self) -> bool {
        self.fraction() <= self.band.differing_fraction
    }

    /// Whether the candidate reproduced the canonical texels exactly.
    ///
    /// Distinct from [`BandDiff::passes`]: a lossy rung can pass a band while
    /// still differing, and the terminal rung of every ladder must be lossless
    /// rather than merely passing.
    pub fn is_lossless(&self) -> bool {
        self.max_channel_delta == 0
    }
}

/// Why two texel buffers could not be diffed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandError {
    /// The two buffers describe different numbers of texels. Diffing them would
    /// mean choosing which texels to drop, so it is refused rather than
    /// truncated to the shorter one.
    TexelCount { canonical: usize, candidate: usize },
    /// A buffer's length is not a whole number of 8-bit RGBA texels.
    NotRgba { len: usize },
}

impl std::fmt::Display for BandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TexelCount {
                canonical,
                candidate,
            } => write!(
                f,
                "the canonical image holds {canonical} bytes and the candidate {candidate}; a \
                 candidate must decode back to exactly the canonical extent before it can be \
                 diffed"
            ),
            Self::NotRgba { len } => {
                write!(f, "{len} bytes is not a whole number of 8-bit RGBA texels")
            }
        }
    }
}

impl std::error::Error for BandError {}

/// Diffs a candidate encoding's decoded texels against the canonical texels,
/// both 8-bit RGBA with rows top to bottom and no padding.
///
/// Counts texels whose max per-channel absolute delta exceeds
/// `band.channel_delta` and reports the measured difference. A length mismatch
/// is an `Err` naming both lengths, never a silent pass.
///
/// The alpha channel is compared like any other: a codec that drops coverage is
/// a real difference, and on an image fill it is one of the more visible ones.
pub fn diff(
    canonical: &[u8],
    candidate: &[u8],
    band: &'static ToleranceBand,
) -> Result<BandDiff, BandError> {
    if canonical.len() != candidate.len() {
        return Err(BandError::TexelCount {
            canonical: canonical.len(),
            candidate: candidate.len(),
        });
    }
    if !canonical.len().is_multiple_of(4) {
        return Err(BandError::NotRgba {
            len: canonical.len(),
        });
    }

    let mut differing = 0usize;
    let mut max_channel_delta = 0u8;
    for (a, b) in canonical.chunks_exact(4).zip(candidate.chunks_exact(4)) {
        let texel_delta = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max_channel_delta = max_channel_delta.max(texel_delta);
        if texel_delta > band.channel_delta {
            differing += 1;
        }
    }

    Ok(BandDiff {
        differing,
        total: canonical.len() / 4,
        max_channel_delta,
        band,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BAND: ToleranceBand = ToleranceBand {
        rule: "test",
        channel_delta: 8,
        differing_fraction: 0.25,
    };

    /// Two texels, one of them off by more than the threshold.
    fn pair(deltas: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let canonical = vec![0u8; deltas.len() * 4];
        let candidate = deltas
            .iter()
            .flat_map(|&d| [d, 0, 0, 0])
            .collect::<Vec<u8>>();
        (canonical, candidate)
    }

    #[test]
    fn an_identical_pair_is_lossless_and_passes() {
        let (canonical, candidate) = pair(&[0, 0, 0, 0]);
        let measured = diff(&canonical, &candidate, &TEST_BAND).expect("equal lengths diff");
        assert_eq!(measured.differing, 0);
        assert_eq!(measured.total, 4);
        assert_eq!(measured.max_channel_delta, 0);
        assert!(measured.is_lossless());
        assert!(measured.passes());
    }

    #[test]
    fn a_delta_at_the_threshold_does_not_count_but_is_still_reported() {
        // The predicate is strictly greater than, matching the render oracle.
        let (canonical, candidate) = pair(&[8, 8, 8, 8]);
        let measured = diff(&canonical, &candidate, &TEST_BAND).expect("equal lengths diff");
        assert_eq!(measured.differing, 0);
        assert_eq!(measured.max_channel_delta, 8);
        // Reported as differing at all, so a passing band never reads as exact.
        assert!(!measured.is_lossless());
        assert!(measured.passes());
    }

    #[test]
    fn the_area_budget_is_what_fails_a_candidate() {
        // One texel of four is 25 %, exactly the budget: at the budget passes.
        let (canonical, candidate) = pair(&[9, 0, 0, 0]);
        let measured = diff(&canonical, &candidate, &TEST_BAND).expect("equal lengths diff");
        assert_eq!(measured.differing, 1);
        assert!((measured.fraction() - 0.25).abs() < f64::EPSILON);
        assert!(measured.passes());

        // Two of four is 50 %, over it.
        let (canonical, candidate) = pair(&[9, 9, 0, 0]);
        let measured = diff(&canonical, &candidate, &TEST_BAND).expect("equal lengths diff");
        assert_eq!(measured.differing, 2);
        assert!(!measured.passes());
    }

    #[test]
    fn every_channel_counts_including_alpha() {
        let canonical = vec![0u8; 4];
        for channel in 0..4 {
            let mut candidate = vec![0u8; 4];
            candidate[channel] = 40;
            let measured = diff(&canonical, &candidate, &TEST_BAND).expect("equal lengths diff");
            assert_eq!(
                measured.differing, 1,
                "channel {channel} must count toward the diff"
            );
            assert_eq!(measured.max_channel_delta, 40);
        }
    }

    #[test]
    fn a_length_mismatch_is_refused_rather_than_truncated() {
        let error = diff(&[0u8; 8], &[0u8; 4], &TEST_BAND).expect_err("a mismatch is an error");
        assert_eq!(
            error,
            BandError::TexelCount {
                canonical: 8,
                candidate: 4
            }
        );
    }

    #[test]
    fn a_buffer_that_is_not_whole_texels_is_refused() {
        let error =
            diff(&[0u8; 6], &[0u8; 6], &TEST_BAND).expect_err("a partial texel is an error");
        assert_eq!(error, BandError::NotRgba { len: 6 });
    }

    #[test]
    fn an_empty_pair_has_no_fraction_rather_than_a_division_by_zero() {
        let measured = diff(&[], &[], &TEST_BAND).expect("empty buffers diff");
        assert_eq!(measured.total, 0);
        assert_eq!(measured.fraction(), 0.0);
        assert!(measured.passes());
    }
}
