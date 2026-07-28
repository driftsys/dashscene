# Perceptual band calibration — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record where each quality profile's chosen ASTC rung lands on SSIMULACRA2 and FLIP, for every rung of the ladder and for every triptych scene arm, so the band numbers can be read against a published scale.

**Architecture:** A new `goldens::metric` module wraps three metrics over plain RGBA8 buffers. A new integration test walks the ASTC ladder per corpus fixture and pins the scores; the existing profile-preview oracle gains the same scores for its scene arms, recorded in `goldens/oracle/profile-manifest.json`. Nothing reaches a shipped crate — `goldens` is `publish = false`.

**Tech Stack:** Rust 2024, `ssimulacra2` 0.5.1 (BSD-2, pure Rust), `nv-flip` 0.1.2 (MIT/Apache/Zlib, vendors NVIDIA's CPU FLIP), the vendored `astcenc` already in `dashpack`, `png` for canonical decode.

**Status:** archived 2026-07-28, all six tasks executed. The durable record is `docs/decisions/asset-quality-profile-bands.md` section 5. Kept verbatim; two steps did not survive contact with the measurement — Task 4's scene half moved into the existing profile-preview oracle rather than a new binary, and Task 1's recorded desk viewing condition was 67.02 where FLIP ships a rounded 67.

**Design:** `docs/archive/2026-07-28-perceptual-band-calibration-design.md`. **Issue:** #544.

## Global Constraints

- Every number in this work is **measured, then recorded** — never predicted. Write the assertion with a placeholder, run it, read the value off the failure, paste it in. `docs/technotes/2026-07-26-tolerance-band-coverage.md`: classify from the measured residual, never from expectation.
- **Floors are chosen after measurement.** If a band's accepted rung scores below the published rung it implies, that is the finding: it goes in the decision record and a new issue. Do not lower the floor to whatever passed, and do not change a fixture to fit a band.
- Recorded precision: SSIMULACRA2 and PSNR to **2 decimals**, FLIP mean to **4 decimals**, compared as strings. The rounding is the tolerance. This matches `percent()` in `profile_preview_oracle.rs` and the four-decimal fractions in `band_contract.rs`.
- Canonical payloads are decoded with the **`png` crate**, never Skia, so the texels match what `crates/dashpack/tests/band_contract.rs` feeds the encoder. The scene half uses Skia's decode, because both its arms are Skia renders.
- ASTC encodes use `dashpack::profile::PACK_QUALITY` and the class's own `AssetClass::color_space()`. Any other setting measures a codec we do not ship.
- Prose in code, commits and docs follows `~/.claude/rules/international-english.md`: literal phrasing, no idioms.
- Commits are conventional, scope from `.git-std.toml` (`goldens`, `docs`, `deps`), and end with the `Co-Authored-By` trailer.
- Run `just build` before declaring any task done. `cargo test` alone stops at the first failing binary — use `--no-fail-fast` when sweeping.

## File structure

| file                                              | responsibility                                                                                                                                                                               |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `goldens/tooling/src/metric.rs`                   | **new.** The three metrics over `(width, height, &[u8])` RGBA8 buffers, the two FLIP viewing conditions, and the SSIMULACRA2 minimum-extent rule. No knowledge of profiles, rungs or scenes. |
| `goldens/tooling/src/lib.rs`                      | **modify.** One `pub mod metric;` declaration.                                                                                                                                               |
| `goldens/tooling/Cargo.toml`                      | **modify.** Two dependencies.                                                                                                                                                                |
| `Cargo.toml` (workspace)                          | **modify.** Two `[workspace.dependencies]` entries with the house-style comment explaining why each is here.                                                                                 |
| `goldens/tooling/tests/common/stress.rs`          | **new.** The deterministic block-stress generator, moved out of the oracle binary so both test binaries share one generator.                                                                 |
| `goldens/tooling/tests/common/mod.rs`             | **modify.** One `pub mod stress;` declaration.                                                                                                                                               |
| `goldens/tooling/tests/profile_preview_oracle.rs` | **modify.** Uses the shared generator; scores each scene arm and asserts against the manifest.                                                                                               |
| `goldens/tooling/tests/perceptual_calibration.rs` | **new.** The per-asset ladder table: every rung of every fixture, scored and pinned, plus the floor claims.                                                                                  |
| `goldens/oracle/profile-manifest.json`            | **modify.** Metric fields per profile row, plus a description of what they are.                                                                                                              |
| `crates/dashpack/tests/band_contract.rs`          | **modify.** A pointer comment to the calibration. No behaviour change.                                                                                                                       |
| `docs/decisions/asset-quality-profile-bands.md`   | **modify.** A calibration section; "What this does not pin" updated.                                                                                                                         |
| `justfile`                                        | **modify.** Viewing conditions in the `triptych` recipe comment.                                                                                                                             |

---

### Task 1: `goldens::metric` — three metrics over RGBA8

**Files:**

- Create: `goldens/tooling/src/metric.rs`
- Modify: `goldens/tooling/src/lib.rs` (add `pub mod metric;` beside `pub mod oracle;`)
- Modify: `goldens/tooling/Cargo.toml`, `Cargo.toml`
- Test: unit tests inside `goldens/tooling/src/metric.rs`

**Interfaces:**

- Consumes: nothing from earlier tasks.
- Produces, for Tasks 3 and 4:
  - `pub struct Scores { pub ssimulacra2: Option<f64>, pub flip_desk: f64, pub flip_panel: f64, pub psnr_rgb: f64, pub psnr_alpha: f64 }`
  - `pub fn score(width: u32, height: u32, reference: &[u8], candidate: &[u8]) -> Result<Scores, MetricError>`
  - `pub fn desk_ppd() -> f32`, `pub fn panel_ppd() -> f32`
  - `pub const SSIMULACRA2_MIN_EXTENT: u32 = 64;`
  - `pub fn fixed(value: f64, decimals: usize) -> String`
  - `pub enum MetricError { TexelCount { reference: usize, candidate: usize }, NotRgba { len: usize }, Extent { width: u32, height: u32, len: usize } }`

- [ ] **Step 1: Add the dependencies**

In the workspace `Cargo.toml`, under `[workspace.dependencies]`, after the existing entries:

```toml
# The two published perceptual scales the quality profiles are calibrated
# against (issue #544). Test tooling only — `goldens` is `publish = false` and
# nothing in a shipped crate reaches either.
#
# SSIMULACRA2 is the JPEG XL project's still-image metric, on a published
# scale: roughly 90 and above visually lossless, 70 and above high quality.
# This is the rust-av port rather than a binding to libjxl, so it needs no
# C++ toolchain and no vendored tree.
ssimulacra2 = "0.5"
# FLIP is NVIDIA's metric for comparing *rendered* images — it models what a
# viewer notices alternating between two. Half the target content is 3D
# renders (docs/wip/2026-07-28-photorealistic-3d-content.md), which makes it
# the matched metric rather than a generic one. The crate vendors the CPU
# implementation inside its own `-sys` package, so this is an ordinary
# dependency and not a vendored tree NOTICE must cover.
nv-flip = "0.1"
```

In `goldens/tooling/Cargo.toml`, under `[dependencies]` (not dev-dependencies — `src/metric.rs` is a library module):

```toml
# The two published perceptual scales (issue #544). Real dependencies rather
# than dev-only because `src/metric.rs` is a library module both test binaries
# reach; `goldens` is `publish = false`, so nothing shipped links either.
ssimulacra2.workspace = true
nv-flip.workspace = true
```

- [ ] **Step 2: Write the failing tests**

Create `goldens/tooling/src/metric.rs` with only the module documentation and this test module, so it fails to compile against functions that do not exist yet.

```rust
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
        assert_eq!(fixed(scores.ssimulacra2.expect("128 is above the floor"), 2), "100.00");
        assert_eq!(fixed(scores.flip_desk, 4), "0.0000");
        assert_eq!(fixed(scores.flip_panel, 4), "0.0000");
        assert!(scores.psnr_rgb.is_infinite(), "an exact match has no error to report");
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
        assert_eq!(fixed(scores.ssimulacra2.expect("above the floor"), 2), "100.00");
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
        assert_eq!(scores.ssimulacra2, None, "16 is below {SSIMULACRA2_MIN_EXTENT}");
        assert!(scores.flip_desk > 0.0, "FLIP has no minimum extent");
        assert!(scores.psnr_rgb.is_finite());
    }

    #[test]
    fn a_length_mismatch_is_refused_rather_than_truncated() {
        let error = score(2, 2, &[0u8; 16], &[0u8; 8]).expect_err("a mismatch is an error");
        assert_eq!(error, MetricError::TexelCount { reference: 16, candidate: 8 });
    }

    #[test]
    fn a_buffer_that_is_not_whole_texels_is_refused() {
        let error = score(1, 1, &[0u8; 6], &[0u8; 6]).expect_err("a partial texel is an error");
        assert_eq!(error, MetricError::NotRgba { len: 6 });
    }

    #[test]
    fn a_buffer_that_does_not_match_the_extent_is_refused() {
        let error = score(4, 4, &[0u8; 16], &[0u8; 16]).expect_err("4x4 needs 64 bytes");
        assert_eq!(error, MetricError::Extent { width: 4, height: 4, len: 16 });
    }

    /// The two viewing conditions, so a later reader can check the geometry
    /// behind each without rederiving it.
    #[test]
    fn the_two_viewing_conditions_are_the_documented_ones() {
        assert_eq!(fixed(desk_ppd() as f64, 2), "67.02");
        assert_eq!(fixed(panel_ppd() as f64, 2), "107.71");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p goldens --lib metric 2>&1 | tail -20`
Expected: FAIL to compile — `cannot find function score in this scope`, and the same for `fixed`, `desk_ppd`, `panel_ppd`, `SSIMULACRA2_MIN_EXTENT`, `MetricError`.

- [ ] **Step 4: Write the implementation**

Prepend to `goldens/tooling/src/metric.rs`, above the test module:

```rust
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
//! channel — and on an image fill a codec that drops coverage is one of the more
//! visible failures. [`Scores::psnr_alpha`] is the column that can see it, and
//! `an_alpha_only_difference_is_invisible_to_both_perceptual_metrics` measures
//! the blind spot rather than describing it.
//!
//! **Small images.** SSIMULACRA2 is multi-scale: it rescales by half up to six
//! times and refuses anything below 8x8. At 16x16 only two scales survive, and
//! the score stops meaning what it means elsewhere — measured, not assumed: a
//! 4-bit quantisation scores 12.59 on a 380 px payload and 92.86 on a 16 px one.
//! So [`score`] withholds the number below [`SSIMULACRA2_MIN_EXTENT`] rather
//! than reporting one that reads as comparable. FLIP and PSNR have no such
//! floor and are reported at every extent.
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

/// FLIP's published default viewing condition: 0.7 m from a 3840 px, 0.7 m wide
/// monitor. Using the published default is what keeps our figures comparable to
/// every published FLIP number.
pub fn desk_ppd() -> f32 {
    nv_flip::DEFAULT_PIXELS_PER_DEGREE
}

/// An automotive centre display: 0.9 m viewing distance, 1920 px across a
/// 0.28 m wide panel.
///
/// **This geometry is not specified anywhere in this repository.**
/// `docs/specification/03-target-hardware-rules.md` pins GPU class, render-pass
/// rules and texture policy, and no display geometry at all, so this is a stated
/// assumption of the calibration rather than a value read from the specification
/// — issue #TBD-PANEL proposes pinning it there. It is reported beside
/// [`desk_ppd`] so the sensitivity to viewing distance is visible rather than
/// hidden inside a default; measured, the two agree to about 3 % of the value.
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
            Self::TexelCount { reference, candidate } => write!(
                f,
                "the reference holds {reference} bytes and the candidate {candidate}; a pair \
                 must be the same extent before it can be scored"
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
        return Err(MetricError::NotRgba { len: reference.len() });
    }
    if reference.len() != width as usize * height as usize * 4 {
        return Err(MetricError::Extent { width, height, len: reference.len() });
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
    // The metric takes linear-light RGB with its transfer function named, so the
    // sRGB-encoded texels are handed over as sRGB and converted inside rather
    // than linearised here, where a second implementation of the transfer
    // function could disagree with the metric's own.
    let as_rgb = |texels: &[u8]| {
        let data: Vec<[f32; 3]> = texels
            .chunks_exact(4)
            .map(|t| [t[0] as f32 / 255.0, t[1] as f32 / 255.0, t[2] as f32 / 255.0])
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
        texels.chunks_exact(4).flat_map(|t| [t[0], t[1], t[2]]).collect()
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
```

Then add `pub mod metric;` to `goldens/tooling/src/lib.rs`, in alphabetical position before `pub mod oracle;`, with a one-line doc comment in the style of the neighbouring declarations.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p goldens --lib metric 2>&1 | tail -20`
Expected: PASS, 7 tests.

If `the_two_viewing_conditions_are_the_documented_ones` fails, do not adjust the geometry to fit — read the reported value, confirm it against `nv_flip::pixels_per_degree`'s own definition, and correct the recorded string. If `an_identical_pair_scores_perfectly_on_every_metric` reports SSIMULACRA2 slightly below 100.00, that is a real finding about the port and belongs in the decision record, not in a widened assertion.

- [ ] **Step 6: Lint and commit**

```bash
just lint
git add Cargo.toml Cargo.lock goldens/tooling/Cargo.toml goldens/tooling/src/lib.rs goldens/tooling/src/metric.rs
git commit -m "test(goldens): add the two published perceptual scales as a metric module

SSIMULACRA2 and FLIP over plain RGBA8 buffers, with PSNR for comparability
and a second PSNR over alpha alone.

Both perceptual metrics are defined over RGB and neither reads alpha, while
dashpack::band::diff compares alpha like any other channel. The alpha PSNR
column exists for that gap, and a test measures the blind spot rather than
describing it.

SSIMULACRA2 is withheld below 64 px because it is multi-scale: at 16x16 only
two of its six scales survive, and the same degradation scores 12.59 on a
380 px payload against 92.86 on a 16 px one.

Refs #544.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Share the block-stress generator between both test binaries

**Files:**

- Create: `goldens/tooling/tests/common/stress.rs`
- Modify: `goldens/tooling/tests/common/mod.rs` (add `pub mod stress;`)
- Modify: `goldens/tooling/tests/profile_preview_oracle.rs:93-131` (delete the moved code, call the shared one)

**Interfaces:**

- Consumes: nothing.
- Produces, for Task 3: `common::stress::block_stress(width: u32, height: u32, amplitude: i32) -> Vec<u8>`, `common::stress::STRESS_EXTENT: u32`, `common::stress::STRESS_AMPLITUDE: i32`, `common::stress::STRESS_REF: &str`.

This is a pure move. `tests/common/` is the established home for helpers shared across this directory's binaries (debt #120), and it already carries `#![allow(dead_code)]` because each binary compiles its own copy.

- [ ] **Step 1: Record the current numbers**

Run: `cargo test -p goldens --test profile_preview_oracle 2>&1 | tail -5`
Expected: PASS. Note the test count — the move must not change it.

- [ ] **Step 2: Move the generator**

Create `goldens/tooling/tests/common/stress.rs` holding, moved verbatim from `profile_preview_oracle.rs`: the `STRESS_REF`, `STRESS_EXTENT` and `STRESS_AMPLITUDE` constants with their doc comments, and the `splitmix` and `block_stress` functions with theirs. Make all four `pub`; `splitmix` stays private to the module. Add a module header saying why it lives here:

```rust
//! The generated block-stress payload, shared by the two test binaries that
//! measure it: the profile-preview oracle's `profile-stress` scene and the
//! perceptual calibration's high-frequency fixture (issue #544).
//!
//! Shared rather than copied. `crates/dashpack/tests/band_contract.rs`
//! generates equivalent content independently, and that copy is deliberate —
//! it lives in a crate that must not link Skia. Within this directory there is
//! no such constraint, so a second copy would be two generators that have to
//! agree with nothing holding them to it.
```

Add `pub mod stress;` to `goldens/tooling/tests/common/mod.rs` beside `pub mod manifest;`.

In `profile_preview_oracle.rs`, delete the moved items and reference the shared ones (`use common::stress::{STRESS_AMPLITUDE, STRESS_EXTENT, STRESS_REF, block_stress};`).

- [ ] **Step 3: Verify no number moved**

Run: `cargo test -p goldens --test profile_preview_oracle 2>&1 | tail -5`
Expected: PASS with the same test count as Step 1. A moved generator that changed a byte would move `profile-stress`'s recorded fractions, so a green run here is the check.

- [ ] **Step 4: Commit**

```bash
just lint
git add goldens/tooling/tests/common/mod.rs goldens/tooling/tests/common/stress.rs goldens/tooling/tests/profile_preview_oracle.rs
git commit -m "refactor(goldens): share the block-stress generator across test binaries

Moves the deterministic generator into tests/common/ so the perceptual
calibration and the profile-preview oracle measure one payload rather than
two copies that have to agree. No number moves.

Refs #544.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: The per-asset ladder calibration

**Files:**

- Create: `goldens/tooling/tests/perceptual_calibration.rs`
- Modify: `crates/dashpack/tests/band_contract.rs` (pointer comment only)

**Interfaces:**

- Consumes: `goldens::metric::{Scores, score, fixed, SSIMULACRA2_MIN_EXTENT}` (Task 1), `common::stress::{block_stress, STRESS_EXTENT, STRESS_AMPLITUDE}` (Task 2).
- Produces: the recorded table the decision record quotes in Task 5.

Fixtures, extents confirmed on disk:

| name                | path                                                                                          | extent  | kind            |
| ------------------- | --------------------------------------------------------------------------------------------- | ------- | --------------- |
| `import-image-fill` | `corpus/figma-fixtures/import-image-fill.images/f856e637d6f6c2eb858e17a31d810f00542d2035.png` | 380x380 | `Image`         |
| `v03-paint`         | `corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png`         | 16x16   | `Image`         |
| `inter-ascii-atlas` | `corpus/atlas/inter-ascii/atlas.png`                                                          | 512x256 | `DistanceField` |
| `arabic-atlas`      | `corpus/atlas/arabic/atlas.png`                                                               | 512x256 | `DistanceField` |
| `block-stress`      | generated                                                                                     | 256x256 | `Image`         |

- [ ] **Step 1: Write the test with the table left empty**

Create `goldens/tooling/tests/perceptual_calibration.rs`. Structure it as `band_contract.rs` is structured: a `FIXTURES` array, a `TABLE` of recorded rows, and a test that walks and asserts. The ladder is `dashpack::profile`'s own — do not retype the footprints; read them from `AssetClass::ImageFill.lossy_rungs()`, then append `Rung::Uncompressed`, so a ladder change cannot leave this table describing a rung the packer no longer offers.

Key mechanics:

```rust
/// One recorded row: a fixture at one rung, scored against its canonical
/// texels.
struct Row {
    fixture: &'static str,
    rung: &'static str,
    /// SSIMULACRA2 to 2 decimals, or "withheld" below the minimum extent.
    ssimulacra2: &'static str,
    /// Mean FLIP at 67 ppd, then at 107.71 ppd, each to 4 decimals.
    flip_desk: &'static str,
    flip_panel: &'static str,
    /// PSNR in dB to 2 decimals, or "lossless".
    psnr_rgb: &'static str,
    psnr_alpha: &'static str,
    /// Which profiles, if any, selected this rung for this fixture.
    selected_by: &'static [Profile],
}
```

The walk, per fixture:

1. Decode the canonical payload with the `png` crate — copy `decode_png` from `goldens/tooling/tests/derived_bank.rs:113-130` verbatim, including its doc comment about why it is the `png` crate and not Skia. Note that `reader.output_buffer_size()` returns a `Result` in this version.
2. `let class = AssetClass::of(kind).expect("a known kind");`
3. For each `block` in `class_ladder()` (see below): `astc::encode(image, block, class.color_space(), PACK_QUALITY)` then `astc::decode(&payload, width, height, block, class.color_space())`, and `metric::score(width, height, &canonical, &decoded)`.
4. For the terminal rung, the candidate is the canonical texels themselves — no encode.
5. `selected_by` comes from `profile::pack(profile, kind, image)`: `Binding::Derived(d) => d.rung`, for `Profile::HiFi` and `Profile::LoFi`. `Binding::Canonical` only under RAW, which is not walked.

The ladder walked per fixture:

```rust
/// The rungs this calibration scores: the image-fill ladder, then the terminal
/// rung, read from the packer rather than retyped so a ladder change cannot
/// leave this table describing a footprint the packer no longer offers.
///
/// Every fixture is walked over this same list, including the two distance
/// fields — whose own `lossy_rungs()` is empty by rule. For them the walk is an
/// explicit **counterfactual**: what the packer refuses, never a rung it could
/// select. That is what puts a published-scale number behind
/// `docs/decisions/asset-quality-profile-bands.md`'s "no lossy rung could have
/// held either band". `AssetClass::DistanceField.lossy_rungs()` is deliberately
/// *not* consulted here, because reading it would yield an empty walk and
/// measure nothing.
fn scored_rungs() -> Vec<Rung> {
    AssetClass::ImageFill
        .lossy_rungs()
        .iter()
        .copied()
        .map(Rung::Astc)
        .chain([Rung::Uncompressed])
        .collect()
}
```

The `selected_by` column is what keeps the counterfactual honest: for a distance field it names the terminal rung and nothing else, because that is the only rung either profile can reach for that class.

Leave `TABLE` as an empty array for this step, and write the test so it prints every measured row when `UPDATE_GOLDENS` is set, following `updating()` in `profile_preview_oracle.rs:360-364`.

- [ ] **Step 2: Run it to read the numbers off**

Run: `UPDATE_GOLDENS=1 cargo test -p goldens --test perceptual_calibration -- --nocapture 2>&1 | tail -60`
Expected: the printed rows, one per fixture per rung, in the field order of `Row`.

**Do not predict these values.** Paste what is printed. This is the same discipline `band_contract.rs` records in its header: "Nothing below is predicted: the numbers were produced by running this code and reading them off."

- [ ] **Step 3: Record the table and assert it**

Paste the printed rows into `TABLE`. Add the assertion that every measured row equals its recorded row, string-compared at the pinned precision, and that the table's length equals fixtures times rungs — a row that is measured and not recorded is unpinned, and one that is recorded and not measured is a number nothing produces. This is the shape `every_scene_renders_within_its_profile_band` uses for its manifest, at `profile_preview_oracle.rs:432-441`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p goldens --test perceptual_calibration 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Add the floor claims, chosen from the measurement**

Read the recorded table. For each profile, find the rung `selected_by` names for each image-fill fixture and read its SSIMULACRA2 score. Then pin the floor at the published rung the measurement actually supports:

```rust
/// The published rung each profile's selected encoding reaches, asserted for
/// every image-fill fixture.
///
/// Chosen from the measured table above rather than from the band's intent. If
/// a later fixture drops a profile below its floor, that is evidence the band
/// is wrong and the direction of the fix is to retune the band against the
/// asset — not to lower this number and not to change the fixture.
const FLOORS: [(Profile, f64, &str); 2] = [ /* filled from the measurement */ ];
```

Write the test `every_profile_reaches_its_published_rung` asserting it, with a failure message that names the measured score, the floor, and the sentence above about which direction the fix goes.

**If the measurement does not support the floor the band implies** — HiFi below 90, LoFi below 70 — stop and report it. Record the floor at what is measured, add a paragraph to the decision record naming the gap, and file an issue against the band. Do not continue as though the number passed.

- [ ] **Step 6: Add the pointer comment to `band_contract.rs`**

In the module header of `crates/dashpack/tests/band_contract.rs`, after the "Why a digest and not only a length" section, add a short section: the bands here are gates internal to this project, the published-scale calibration of the rungs they choose is `goldens/tooling/tests/perceptual_calibration.rs`, and it walks the whole ladder rather than only the selected rung. No code changes.

- [ ] **Step 7: Verify the whole workspace and commit**

Run: `just build`
Expected: green.

```bash
git add goldens/tooling/tests/perceptual_calibration.rs crates/dashpack/tests/band_contract.rs
git commit -m "test(goldens): calibrate every ladder rung against SSIMULACRA2 and FLIP

Records where each rung of the ASTC ladder lands on two published perceptual
scales, for every corpus fixture, and which rungs HiFi and LoFi select. Every
number is measured and read off rather than predicted.

Walking the whole ladder rather than only the selected rung is what makes the
band's cut readable: it records the score of the rung each profile accepted
beside the score of the rung it rejected. For the two MSDF atlases the ladder
is walked as a counterfactual — what the packer refuses — which puts a
published-scale number behind the fields-never-lossy rule.

Refs #544.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Scene-arm metrics in the profile-preview oracle

**Files:**

- Modify: `goldens/tooling/tests/profile_preview_oracle.rs` (inside `every_scene_renders_within_its_profile_band`, after the existing `oracle::diff`)
- Modify: `goldens/oracle/profile-manifest.json`

**Interfaces:**

- Consumes: `goldens::metric::{score, fixed}` (Task 1).
- Produces: nothing later tasks build on; Task 5 quotes the recorded values.

**Deviation from the design, with the reason.** The design put both halves in one new test file. The scene half goes into the existing oracle instead, because that test already renders all three arms and derives each bank. A second binary would re-render every scene through Skia and re-run the packer to produce images it already has, and would give the same scene two sources of truth. The design's intent — one metric implementation, scene numbers recorded in the manifest beside the band numbers — is unchanged.

- [ ] **Step 1: Score the arms and print, with no assertion yet**

Inside the profile loop, after `let diff = oracle::diff(&png, &raw, band)...`, decode both arms into the comparison space and score them:

```rust
// The same texels the diff above measured, scored on the two published
// perceptual scales (issue #544). Skia's decode here, not the `png` crate's:
// both arms are Skia renders, so the comparison must start from the decode
// that produced them.
let ((width, height), profile_texels) = png_texels(&png);
let (_, raw_texels) = png_texels(&raw);
let scores = goldens::metric::score(width, height, &raw_texels, &profile_texels)
    .expect("both arms render at the scene's canvas extent");
```

Extend `report` to print the four figures beside the existing band numbers, so `just triptych` shows them.

- [ ] **Step 2: Read the numbers off**

Run: `cargo test -p goldens --test profile_preview_oracle -- --nocapture 2>&1 | grep 'PROFILE PREVIEW'`
Expected: one line per scene per profile, now carrying the scores.

- [ ] **Step 3: Record them in the manifest and assert**

Add to each profile row in `goldens/oracle/profile-manifest.json`, using the printed values:

```json
"ssimulacra2": "…",
"flipDesk": "…",
"flipPanel": "…",
"psnrRgb": "…",
"psnrAlpha": "…"
```

Extend the manifest's top-level `description` with a paragraph saying what these are: two published perceptual scales that calibrate the band numbers beside them, that they gate nothing, that FLIP is reported at two viewing conditions because its error depends on pixels per degree, and that the panel geometry behind the second is a stated assumption rather than a specified value.

Add the assertions to `assert_measurement`, string-compared at the pinned precision, skipped under `updating()` exactly as the existing fraction assertions are.

- [ ] **Step 4: Run to verify**

Run: `cargo test -p goldens --test profile_preview_oracle 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
just lint
git add goldens/tooling/tests/profile_preview_oracle.rs goldens/oracle/profile-manifest.json
git commit -m "test(goldens): score each triptych arm on the published perceptual scales

Every scene's HiFi and LoFi arm is now scored against its RAW arm on
SSIMULACRA2, FLIP at two viewing conditions, and PSNR, recorded beside the
band numbers the same rows already carry.

The scores live in the existing oracle rather than in a second binary: this
test already renders all three arms and derives each bank, so a separate one
would re-render what it has and give the same scene two sources of truth.

Refs #544.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: The records — decision record, triptych viewing conditions

**Files:**

- Modify: `docs/decisions/asset-quality-profile-bands.md`
- Modify: `justfile` (the `triptych` recipe comment, lines 315-331)
- Modify: `goldens/tooling/tests/profile_preview_oracle.rs` (module header, the "Artifacts" section)

- [ ] **Step 1: Add the calibration section to the decision record**

After section 4, "`AssetEntry.kind`, and who sets it", add a section carrying the measured table and what it says. It must state: which rung each profile selected and where that lands on each published scale; the score of the rung one coarser, which is what makes the band's cut readable; the counterfactual figures for the two MSDF atlases; that the metrics gate nothing; and that SSIMULACRA2 is withheld for `v03-paint` with the measurement that justifies it.

Edit the record in place rather than adding a new one beside it — the working-memory lifecycle rule: when new work changes a recorded decision, edit the existing record.

- [ ] **Step 2: Update "What this does not pin"**

The entry "Nothing measures a photograph" stands and gains a sentence: the calibration figures are measured on a gradient, two MSDF atlases and generated stress content, so they are a baseline that #455's fixtures will move. Add an entry: cross-architecture agreement of the scores is unmeasured, and the pinned precision is what would make a disagreement visible.

- [ ] **Step 3: Write the viewing conditions into the triptych recipe**

Extend the `triptych` recipe's comment block in the `justfile` with the review conditions, and echo the short form in the recipe body so a person running it sees them:

- Native pixels. No browser zoom, no display scaling, no window that resizes the image. Smooth scaling averages block artifacts away, so a viewer who rescales is reporting on the resampler rather than on the codec.
- Integer nearest-neighbour if zoom is needed at all.
- Blind and randomised order if the opinion is to mean anything — a reviewer who knows which arm is LoFi is not answering the question.
- The full ladder rather than three points: the useful question is where loss becomes visible, not whether three named arms differ.
- ITU-R BT.500 and ITU-T P.910 are the standard protocols for doing this properly.

Mirror the same list in the "Artifacts" section of `profile_preview_oracle.rs`'s module header, where a reader of the code arrives.

- [ ] **Step 4: Verify and commit**

Run: `just lint && just triptych`
Expected: lint green; the triptych prints its rows, now with the perceptual scores and the viewing conditions.

```bash
git add docs/decisions/asset-quality-profile-bands.md justfile goldens/tooling/tests/profile_preview_oracle.rs
git commit -m "docs(docs): record the perceptual calibration of the profile bands

Adds the measured table to the band decision record: where each profile's
selected rung lands on SSIMULACRA2 and FLIP, what the rung one coarser scores,
and the counterfactual figures for the two distance-field atlases.

Also documents the viewing conditions for human review of the triptych, in the
recipe a reviewer runs and in the oracle module a reader arrives at. Smooth
scaling averages block artifacts away, so a viewer who rescales the image is
reporting on the resampler rather than on the codec.

Refs #544.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Follow-up issue, gardening, and the pull request

**Files:**

- Modify: `goldens/tooling/src/metric.rs` (replace the `#TBD-PANEL` placeholder with the real issue number)
- Move: `docs/wip/2026-07-28-perceptual-band-calibration-design.md` and `-plan.md` to `docs/archive/`

- [ ] **Step 1: File the panel-geometry issue**

```bash
gh issue create --label debt --milestone "v0.13 — pre-v1 hardening" \
  --title "docs(specification): no display geometry is pinned, so FLIP's viewing condition is an assumption" \
  --body "…"
```

The body states: `docs/specification/03-target-hardware-rules.md` pins GPU class, render-pass rules and texture policy and no display geometry at all; `goldens::metric::panel_ppd` therefore assumes 0.9 m viewing distance, 1920 px across a 0.28 m panel; the measurement shows FLIP's answer moves about 3 % between that and the desk default, so nothing currently depends on the exact value; it should be pinned in the hardware rules if any later work is to depend on it. `Refs #544.`

- [ ] **Step 2: Replace the placeholder**

Replace `#TBD-PANEL` in `goldens/tooling/src/metric.rs` with the issue number returned above. Verify no placeholder survives: `grep -rn 'TBD' goldens/ crates/ docs/decisions/` returns nothing from this branch.

- [ ] **Step 3: Garden the working memory**

Invoke the `sdd-gardening` skill. The design and plan are gardened — the design's decisions are now in `docs/decisions/asset-quality-profile-bands.md` — so both files move to `docs/archive/` and `docs/wip/` returns to the nine files its README explains. Confirm with `.claude/rules/sdd-working-memory-lifecycle/wip-gate.sh` if it is installed.

- [ ] **Step 4: Open the draft pull request**

```bash
just verify
gh pr create --draft --base main \
  --title "test(goldens): calibrate the profile bands against SSIMULACRA2 and FLIP" \
  --body "…"
```

The body carries what was measured, the floors chosen and whether the measurement supported the rung each band implies, the deviation in Task 4 and its reason, and the follow-up issue. Write `Refs #544` — never a closing keyword in prose (AGENTS.md), and reserve the closing keyword for the one issue this completes.

- [ ] **Step 5: Review the pull request**

Run `/code-review` on it. Capture every finding as a checklist in the description. Fix all critical findings; file one `debt`-labelled issue per minor finding. Mark ready for review only once CI is green and the critical findings are resolved.

## Self-review

**Spec coverage.** Design §1 → Tasks 1 and 3 (with the Task 4 deviation stated). §2 the ladder → Task 3 Step 1. §3 fixtures and the `v03-paint` exclusion → Task 1 (`SSIMULACRA2_MIN_EXTENT`, its test) and Task 3. §4 alpha → Task 1 (`psnr_alpha`, its test). §5 scene half → Task 4. §6 assertions → Task 1 (`fixed`), Tasks 3 and 4. §7 floors after measurement → Task 3 Step 5 and the global constraints. §8 FLIP viewing conditions → Task 1 (`desk_ppd`, `panel_ppd`) and Task 6 Step 1. §9 documents → Task 5. §10 dependencies → Task 1 Step 1.

**Placeholders.** One deliberate: `#TBD-PANEL` in Task 1's implementation, which Task 6 Step 2 replaces and greps for. `TABLE` and `FLOORS` are deliberately empty until measured — the plan says how to fill each and forbids predicting the values.

**Type consistency.** `Scores`, `score`, `fixed`, `desk_ppd`, `panel_ppd`, `SSIMULACRA2_MIN_EXTENT` and `MetricError` are declared in Task 1 and used with those names in Tasks 3 and 4. `flip_desk` and `flip_panel` are `f64` on `Scores` and cast from `nv-flip`'s `f32` inside `flip_mean`. The manifest keys `flipDesk` and `flipPanel` are camelCase to match the existing manifest's `measuredDiffering` and `maxChannelDelta`.
