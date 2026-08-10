//! The design-source render oracle (exit criterion E7, guardrail G-11).
//!
//! The rest of this crate diffs the reference painter against the project's
//! own committed golden — a self-oracle, which by construction cannot see
//! the painter's drift away from what a design actually looks like
//! (`docs/technotes/engineering-guardrails.md` G-23). This module adds the
//! missing half of R6: a perceptual diff of the reference render against its
//! **design source** — Figma's REST `GET /images` export — with per-rule
//! tolerance bands.
//!
//! Per-rule, not one global budget (G-11): a hard rect edge, a blurred
//! shadow's soft falloff, and an MSDF glyph edge each disagree with the
//! design source differently, so each rule pins its own band. See the band
//! constants below and `docs/design/goldens.md` for the rationale of each
//! pinned value.
//!
//! The corpus-frame ↔ design-source wiring is `goldens/oracle/manifest.json`;
//! the real design-source images it points at are authored manually and
//! tracked by issue #265 (parked). Until they land, the assertion that a
//! frame matches its export is gated (`goldens/tooling/tests/render_oracle.rs`,
//! `the_reference_renders_match_their_design_source`, `#[ignore]`d with a
//! named #265 reason). This module — the diff math and the bands — is proven
//! now with synthetic image pairs, without a real source.

use skia_safe::{AlphaType, ColorType, Data, ImageInfo, images};

/// A second, tighter measurement a band's frames must also pass, beside the
/// band's own residual.
///
/// A single (threshold, budget) pair does two jobs at once: it sizes an
/// acceptable **residual**, and it acts as a pass/fail **gate**. Those want
/// different values, and when one number tries to be both, one of the two jobs
/// is done badly. Issue #422 recorded exactly that against `blur-falloff`,
/// where a budget sized for a wide falloff could not fail on any bounded-area
/// defect of the frames it governs.
///
/// The two measurements are deliberately on **different axes**, not the same
/// axis twice. The residual is a low threshold with a wide budget — many pixels
/// off by a little, which is what a soft falloff legitimately looks like. The
/// gate is a high threshold with a narrow budget — almost no pixel may be
/// grossly wrong, which is what removing or misplacing the effect looks like.
/// A tighter budget at the *same* threshold would not add a second job; it
/// would replace the first one and leave the residual dead.
#[derive(Debug, PartialEq)]
pub struct BandGate {
    /// The gate's per-pixel threshold, above the band's own.
    pub channel_delta: u8,
    /// The fraction of pixels allowed to exceed [`Self::channel_delta`].
    pub differing_fraction: f64,
}

/// A per-rule perceptual tolerance band. A pixel counts as differing only
/// when its largest per-channel absolute delta (0..=255) exceeds
/// `channel_delta`; a frame passes when the differing fraction is at or
/// below `differing_fraction` **and** it is within [`Self::gate`], if the band
/// declares one.
#[derive(Debug, PartialEq)]
pub struct ToleranceBand {
    /// The construct this band governs, matching a manifest frame's `band`.
    pub rule: &'static str,
    /// The per-pixel threshold: a pixel whose max per-channel absolute delta
    /// exceeds this counts as differing. Absorbs the sub-threshold
    /// resampling noise between a CPU render and a server-side export.
    pub channel_delta: u8,
    /// The pass ceiling: the fraction of pixels (0.0..=1.0) allowed to
    /// exceed `channel_delta`.
    pub differing_fraction: f64,
    /// A second measurement the frame must also pass, or `None` when the
    /// band's own budget is the whole verdict.
    ///
    /// Only `blur-falloff` declares one. A band earns a gate when its residual
    /// is wide enough that a defect it exists to catch can hide under the
    /// budget — which is a property to measure, not to assume, so the other
    /// bands stay single-number until a measurement says otherwise.
    pub gate: Option<BandGate>,
}

/// An axis-aligned rectangle of pixels excluded from a frame's diff, in the
/// render's pixel coordinates (origin top-left, `x` right, `y` down). A pixel
/// at `(px, py)` is inside when `x <= px < x + w` and `y <= py < y + h`
/// (half-open). Excluded pixels count toward neither `differing` nor `total`
/// — they are removed from both the numerator and the denominator — so the
/// measured fraction reflects only the pixels outside every excluded region.
///
/// This exists for a frame that carries one genuine, disclosed structural
/// divergence the area budget must not silently absorb — a real placement or
/// size disagreement, not missing glyph ink — so the frame measures the rest
/// rather than hiding the divergence inside the band. No frame declares an
/// exclusion today: `v08-grid-spans` used one for its `hug me` TEXT cell while
/// text measurement was unwired, but the text render path (story #303) sizes
/// that cell correctly, so the exclusion was removed and the whole frame is
/// measured (`goldens/oracle/manifest.json`, `goldens/oracle/README.md`). The
/// mechanism stays available for a future divergence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExcludeRegion {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl ExcludeRegion {
    /// Whether the pixel at `(px, py)` lies inside this region (half-open on
    /// the right and bottom edges).
    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// The measured outcome of one design-source diff. Carries the numbers a
/// failure report needs — fidelity is a measured value, not a bare
/// pass/fail (G-11) — and the band it was measured against, so the verdict
/// cannot be graded against a different band than the one that produced
/// `differing` (#291).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OracleDiff {
    /// Pixels whose max per-channel delta exceeded the band's `channel_delta`.
    pub differing: usize,
    /// Pixels whose max per-channel delta exceeded the band's
    /// [`BandGate::channel_delta`], or 0 when the band declares no gate.
    ///
    /// Measured in the same pass as `differing` — one walk of the pixels
    /// counts both — so the two figures can never come from different renders.
    pub gate_differing: usize,
    /// Total pixels compared.
    pub total: usize,
    /// The largest per-channel absolute delta seen at any pixel — reports
    /// that a difference exists even when no pixel crossed the threshold.
    pub max_channel_delta: u8,
    /// The band [`diff`] applied to produce `differing`. [`OracleDiff::passes`]
    /// grades against this band, so the count and the budget always come from
    /// the same rule.
    pub band: &'static ToleranceBand,
}

impl OracleDiff {
    /// The share of pixels that exceeded the band's per-pixel threshold.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.differing as f64 / self.total as f64
        }
    }

    /// The share of pixels that exceeded the band gate's threshold, or 0.0
    /// when the band declares no gate.
    pub fn gate_fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.gate_differing as f64 / self.total as f64
        }
    }

    /// Whether the frame is within its band's **residual** budget — the band's
    /// own `differing_fraction` at its own threshold.
    ///
    /// This is not the whole verdict when the band declares a gate; see
    /// [`Self::passes`].
    pub fn within_residual(&self) -> bool {
        self.fraction() <= self.band.differing_fraction
    }

    /// Whether the frame is within its band's gate, or `true` when the band
    /// declares none.
    pub fn within_gate(&self) -> bool {
        match &self.band.gate {
            Some(gate) => self.gate_fraction() <= gate.differing_fraction,
            None => true,
        }
    }

    /// Whether the measured difference is within **both** the area budget of
    /// the band [`diff`] was called with and that band's gate, if it declares
    /// one. The band is carried on the diff (`self.band`), so the differing
    /// counts and the budgets they are graded against always come from the
    /// same rule (#291).
    ///
    /// Both terms bind, and they bind on different defect classes — a wide,
    /// low-amplitude error trips the residual while passing the gate, and a
    /// narrow, high-amplitude one trips the gate while passing the residual.
    /// Neither is redundant, which is the whole point of there being two
    /// (issue #422).
    pub fn passes(&self) -> bool {
        self.within_residual() && self.within_gate()
    }
}

/// Perceptually diffs a reference-painter render against a design-source
/// image, both PNG, in the unpremultiplied RGBA8888 comparison space
/// (`docs/decisions/golden-comparison-space.md`). Counts pixels whose max
/// per-channel absolute delta exceeds `band.channel_delta` and reports the
/// measured difference. A dimension mismatch is an `Err` naming both sizes,
/// never a silent pass.
///
/// This is [`diff_excluding`] with no excluded regions — every pixel counts.
pub fn diff(
    reference_png: &[u8],
    design_source_png: &[u8],
    band: &'static ToleranceBand,
) -> Result<OracleDiff, String> {
    diff_excluding(reference_png, design_source_png, band, &[])
}

/// Like [`diff`], but pixels inside any [`ExcludeRegion`] are removed from
/// both the differing count and the total (numerator and denominator), and
/// from `max_channel_delta` — so the measured fidelity reflects only the
/// pixels outside every excluded region. An empty `exclude` slice is exactly
/// [`diff`].
///
/// A frame may declare its regions in `goldens/oracle/manifest.json` for one
/// genuine, disclosed structural divergence (a real placement or size
/// disagreement the area budget must not absorb); excluding it keeps the frame
/// a clean measurement of the rest. No frame declares one today — the text
/// render path (#303) removed `v08-grid-spans`'s former text-cell exclusion —
/// so `exclude` is the empty slice in practice, and the mechanism stays for a
/// future divergence.
pub fn diff_excluding(
    reference_png: &[u8],
    design_source_png: &[u8],
    band: &'static ToleranceBand,
    exclude: &[ExcludeRegion],
) -> Result<OracleDiff, String> {
    let (reference_size, reference) = decode_rgba(reference_png, "the reference render")?;
    let (source_size, source) = decode_rgba(design_source_png, "the design source")?;

    if reference_size != source_size {
        return Err(format!(
            "the reference render is {}x{} but the design source is {}x{} — a \
             design-source export must match the rendered canvas before it can \
             be diffed",
            reference_size.0, reference_size.1, source_size.0, source_size.1
        ));
    }

    let width = reference_size.0;
    let mut differing = 0usize;
    let mut gate_differing = 0usize;
    let mut total = 0usize;
    let mut max_channel_delta = 0u8;
    for (i, (a, b)) in reference
        .chunks_exact(4)
        .zip(source.chunks_exact(4))
        .enumerate()
    {
        let x = i as i32 % width;
        let y = i as i32 / width;
        // An excluded pixel counts toward neither numerator nor denominator,
        // and does not move max_channel_delta — it is as if it were not there.
        if exclude.iter().any(|region| region.contains(x, y)) {
            continue;
        }
        total += 1;
        // The alpha channel is compared like any other: a design source and
        // a render disagreeing on coverage is a real difference.
        let pixel_delta = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max_channel_delta = max_channel_delta.max(pixel_delta);
        if pixel_delta > band.channel_delta {
            differing += 1;
        }
        // The gate is counted in this same walk rather than in a second pass:
        // two walks could only ever agree, and one walk makes it impossible for
        // the two figures to describe different renders.
        if band
            .gate
            .as_ref()
            .is_some_and(|g| pixel_delta > g.channel_delta)
        {
            gate_differing += 1;
        }
    }

    Ok(OracleDiff {
        differing,
        gate_differing,
        total,
        max_channel_delta,
        band,
    })
}

/// Decodes an encoded image to `((width, height), unpremultiplied RGBA8888
/// rows)`. `label` names the image in the error (reference vs design source).
///
/// Crate-visible rather than private only so that [`crate::profile`] can decode
/// a canonical asset payload through this exact function (story #435). The two
/// arms of a profile diff have to start from one decode of the canonical bytes,
/// and this is the reference painter's own codec — the one the RAW arm paints
/// through. No behaviour changed when the visibility widened.
pub(crate) fn decode_rgba(png_bytes: &[u8], label: &str) -> Result<((i32, i32), Vec<u8>), String> {
    let data = Data::new_copy(png_bytes);
    let image = images::deferred_from_encoded_data(data, None)
        .ok_or_else(|| format!("{label} is not a decodable PNG"))?;
    let (width, height) = (image.width(), image.height());
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * height as usize];
    let read = image.read_pixels(
        &info,
        &mut pixels,
        row_bytes,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    );
    if !read {
        return Err(format!(
            "{label} has a readable header but its pixel data does not decode"
        ));
    }
    Ok(((width, height), pixels))
}

/// Hard rect edges (the E3 exact-layout frames: wrap, grid).
///
/// A hard edge anti-aliased against the design source disagrees on a thin
/// 1–2 px band, where the reference painter's coverage rounding and Figma's
/// server-side export resampling can swing far apart. The fraction budget is
/// the primary tolerance — an edge is a small share of the canvas — and the
/// per-pixel threshold filters sub-threshold interior noise.
///
/// The E3 baseline frame (v08-baseline) is also an exact-layout case, but its
/// content is all text: once the baseline alignment is correct (#272), its
/// residual is glyph edges and font-metric difference, not rect edges, so it
/// is measured in the `msdf-text` band with the other text frames. Its layout
/// correctness — the mixed-size runs meeting one glyph baseline — is proven
/// exactly by the engine unit test, not by this pixel diff.
pub const AA_EDGE: ToleranceBand = ToleranceBand {
    rule: "aa-edge",
    channel_delta: 40,
    differing_fraction: 0.02,
    // No gate: nothing measured on this band's frames hides under its
    // budget, so a second number would pin something no evidence chose.
    gate: None,
};

/// A blurred shadow's soft falloff (the sigma mapping, story #45 and issue
/// #412 — `docs/decisions/blur-sigma-is-figmas-mapping.md`).
///
/// A blur spreads a small per-pixel disagreement across a wide falloff
/// region — many pixels off by a little. The sigma mapping is an
/// approximation of Figma's blur, so the whole falloff can be systematically
/// off by a small amount; a wider area budget with a moderate per-pixel
/// threshold pins "the falloff shape is close" without demanding pixel
/// identity.
///
/// **This band did not pin the sigma mapping, and cannot.** It was written
/// expecting to, once #265 landed a real capture. The captures landed, the
/// mapping was re-fitted at issue #412 — from `blur/2` to `0.4375 * blur` —
/// and this band saw almost none of it: the inner-shadow count stayed at
/// 0/9216 across the whole sweep and the drop-shadow count went *up*, from 2
/// to 4 px, while the mean delta over each frame's falloff improved by 5.3x
/// and 7.1x. A per-pixel threshold cannot see a wide low-amplitude falloff
/// difference, which is the same reason this band needs a separate gate
/// (below). The refit was decided on the fit against Figma's export
/// (`docs/decisions/blur-sigma-is-figmas-mapping.md`), and no band or gate
/// number was retuned to suit it.
///
/// # Why this band has a gate and the others do not (issue #422)
///
/// The 12 % budget above is sound as a **residual** and cannot work as a
/// **gate**. A blur defect is a bounded-area error: it moves the region the
/// effect covers and leaves the rest alone, and on the frames this band governs
/// that region is a few percent of the canvas. So destroying the effect
/// entirely still cannot reach 12 %, measured:
///
/// | mutation                       | frame              | at 24 | at 40 |
/// | ------------------------------ | ------------------ | ----- | ----- |
/// | healthy                        | `v08-drop-shadow`  | 0.043 % | 0.000 % |
/// | healthy                        | `v08-inner-shadow` | 0.000 % | 0.000 % |
/// | the drop shadow removed        | `v08-drop-shadow`  | 4.351 % | 2.930 % |
/// | the inner shadow removed       | `v08-inner-shadow` | 3.570 % | 2.018 % |
///
/// Both removals sit far under 12 % and would have passed. The gate is the
/// second number the owner's ruling on #422 added, and it is on a different
/// axis: a **high threshold with a narrow budget**, because removing an effect
/// leaves few pixels but grossly wrong ones, while a falloff approximation
/// leaves many pixels slightly wrong.
///
/// **Why 40 and 1 %.** At threshold 40 both healthy frames measure exactly
/// 0.000 %, so the gate has the whole budget as headroom rather than a share of
/// it. The budget is set by the smallest defect it must catch: the layer-clip
/// removal recorded at 1.585 % in
/// `docs/technotes/tolerance-band-coverage.md`. The gate must sit
/// below that, so 1 % — the round number under it — and the two shadow
/// removals then fail at 2.9x and 2.0x. The number binds: at 2 % the
/// layer-clip figure passes, and at 3 % the inner-shadow removal passes too.
///
/// **Both numbers bind, on different defects.** The residual is not left
/// dead by the gate. The one mutation that exceeds 12 % — the panel fill alpha
/// moved from 0.20 to 0.35, measured at 23.559 % — is an *amplitude* error
/// across the whole blurred area, and it measures only 0.422 % at threshold 40,
/// so the gate passes it and the residual is the only term that catches it.
/// Removal and confinement defects are the mirror image. Neither number is
/// redundant.
pub const BLUR_FALLOFF: ToleranceBand = ToleranceBand {
    rule: "blur-falloff",
    channel_delta: 24,
    differing_fraction: 0.12,
    gate: Some(BandGate {
        channel_delta: 40,
        differing_fraction: 0.01,
    }),
};

/// MSDF glyph edges (the text frames: v05-text-latin, v06-text-arabic, and
/// the mixed-size baseline row v08-baseline).
///
/// MSDF glyph edges are sharp high-contrast transitions; the reference
/// painter's MSDF resolve and Figma's font rasterizer disagree at glyph
/// boundaries (hinting, gamma), and a whole run shifts by the small
/// difference between the reference and Figma first-line ascent metrics. Text
/// ink is sparse, so a small area budget with a higher per-pixel threshold
/// pins the glyph shapes without over-tolerating.
pub const MSDF_TEXT: ToleranceBand = ToleranceBand {
    rule: "msdf-text",
    channel_delta: 50,
    differing_fraction: 0.03,
    // No gate: nothing measured on this band's frames hides under its
    // budget, so a second number would pin something no evidence chose.
    gate: None,
};

/// The pinned bands, keyed by their manifest `rule` name.
pub const BANDS: [&ToleranceBand; 3] = [&AA_EDGE, &BLUR_FALLOFF, &MSDF_TEXT];

/// The band a manifest frame's `band` name selects, or `None` if the name is
/// not one of the three pinned rules.
///
/// The profile-preview bands below are deliberately **not** reachable from
/// here. They measure a different kind of residual against a different
/// reference, and a design-source frame that named one would be graded against
/// a number chosen for something else.
pub fn band_for(rule: &str) -> Option<&'static ToleranceBand> {
    BANDS.into_iter().find(|band| band.rule == rule)
}

// ------------------------------------------------- the profile-preview bands

/// HiFi, measured over a whole rendered scene against the same scene under RAW
/// (story #435).
///
/// # Why these are not the render bands' numbers
///
/// The three bands above are 24 to 50 because they compare a CPU rasterizer
/// against Figma's server-side export and must absorb anti-aliasing, resampling
/// and gamma disagreement. Here **both arms are the same painter, the same
/// solver, the same typesetter and the same canvas**; the only variable is
/// which bytes the asset entries resolve to. Nothing disagrees except the
/// codec, so a threshold sized for a rasterizer disagreement would be blind to
/// everything this oracle exists to see. Measured: on `profile-photo`, HiFi's
/// whole-scene residual has a maximum per-channel delta of 3, so every render
/// band's threshold reports it as a perfect match.
///
/// # Why they are the packer's numbers exactly
///
/// `dashpack::profile::HIFI_IMAGE_FILL` is 2 and 1 %, and this band is 2 and
/// 1 %. The profile's promise is a per-asset band; this oracle asks whether the
/// profile keeps that promise once the asset is composited into a scene, so the
/// number to hold it to is the promise itself. `the_scene_bands_are_the_packers_
/// bands` in the oracle test asserts the equality, so the two cannot drift
/// apart silently — if they ever need to differ, that is a decision to record
/// rather than a constant to edit.
///
/// The scene measurement is not merely the asset measurement repeated. Pixels
/// covered by opaque ink — glyphs, strokes — contribute no difference and still
/// count toward the total, so a scene dilutes; and a scaled image fill
/// resamples, which can amplify. Measured on `profile-photo`: 0.2043 % at scene
/// level against 0.2133 % for the same asset alone.
///
/// **The mutation that fails it**, measured, because a budget nothing can
/// exceed is not a gate (issue #422): on `profile-photo`, an escalation that
/// stopped one rung early at 8x8 instead of reaching 6x6 measures 2.6627 %,
/// against this 1 % budget. On `profile-stress`, where the escalation must run
/// all the way to the lossless rung, stopping at the finest lossy rung 4x4
/// measures 51.8097 %. Both are recorded in `goldens/oracle/profile-manifest.json`
/// and re-measured by the oracle on every run.
pub const PROFILE_HIFI_SCENE: ToleranceBand = ToleranceBand {
    rule: "profile-hifi-scene",
    channel_delta: 2,
    differing_fraction: 0.01,
    // No gate: nothing measured on this band's frames hides under its
    // budget, so a second number would pin something no evidence chose.
    gate: None,
};

/// LoFi, measured over a whole rendered scene against the same scene under RAW
/// (story #435).
///
/// `dashpack::profile::LOFI_IMAGE_FILL`'s numbers — 8 and 5 % — for the reason
/// [`PROFILE_HIFI_SCENE`] gives.
///
/// **The mutation that fails it**, measured: on `profile-stress`, LoFi's
/// escalation settles at 6x6 and the whole scene measures 4.5166 %; an
/// escalation that stopped one rung early at 8x8 measures 9.7733 %, against
/// this 5 % budget.
///
/// `profile-photo` does **not** exercise this budget and does not pretend to.
/// That scene's gradient survives the cheapest rung on the ladder, so LoFi
/// settles at 12x12 with a whole-scene residual of 0.0000 % and there is no
/// coarser rung for a mutation to stop at. The manifest records that with a
/// stated reason rather than a mutation, and
/// `every_band_is_exercised_by_at_least_one_scene` requires some scene to
/// exercise every band that is declared.
pub const PROFILE_LOFI_SCENE: ToleranceBand = ToleranceBand {
    rule: "profile-lofi-scene",
    channel_delta: 8,
    differing_fraction: 0.05,
    // No gate: nothing measured on this band's frames hides under its
    // budget, so a second number would pin something no evidence chose.
    gate: None,
};

/// The profile-preview bands, keyed by their manifest `band` name.
///
/// Two, not three: RAW is the null binding and is the reference arm of the
/// comparison, so it has nothing to be measured against. A band is only written
/// where a measurement decides something.
pub const PROFILE_BANDS: [&ToleranceBand; 2] = [&PROFILE_HIFI_SCENE, &PROFILE_LOFI_SCENE];

/// The profile-preview band a `rule` name selects, or `None` if it is not one
/// of the two pinned scene contracts.
///
/// A separate lookup from [`band_for`] on purpose: the two families answer
/// different questions against different references, and one name space would
/// let a manifest in either family select a band from the other.
pub fn profile_band_for(rule: &str) -> Option<&'static ToleranceBand> {
    PROFILE_BANDS.into_iter().find(|band| band.rule == rule)
}
