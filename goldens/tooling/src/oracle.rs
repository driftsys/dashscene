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

/// A per-rule perceptual tolerance band. A pixel counts as differing only
/// when its largest per-channel absolute delta (0..=255) exceeds
/// `channel_delta`; a frame passes when the differing fraction is at or
/// below `differing_fraction`.
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
}

/// An axis-aligned rectangle of pixels excluded from a frame's diff, in the
/// render's pixel coordinates (origin top-left, `x` right, `y` down). A pixel
/// at `(px, py)` is inside when `x <= px < x + w` and `y <= py < y + h`
/// (half-open). Excluded pixels count toward neither `differing` nor `total`
/// — they are removed from both the numerator and the denominator — so the
/// measured fraction reflects only the pixels outside every excluded region.
///
/// This exists for a frame that carries one genuine, disclosed structural
/// divergence the area budget must not silently absorb: `v08-grid-spans`'s
/// `hug me` TEXT leaf solves to 0x0 because text measurement is not wired into
/// the oracle render path, collapsing its HUG cell. Excluding that one cell
/// keeps the frame a clean grid-structure measurement (the five non-text cells
/// match the export pixel-exact) rather than hiding the cell inside the band
/// (`goldens/oracle/manifest.json`, `goldens/oracle/README.md`).
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

    /// Whether the measured difference is within the area budget of the band
    /// [`diff`] was called with. The band is carried on the diff (`self.band`),
    /// so the differing count and the budget it is graded against always come
    /// from the same rule (#291).
    pub fn passes(&self) -> bool {
        self.fraction() <= self.band.differing_fraction
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
/// A frame declares its regions in `goldens/oracle/manifest.json` for one
/// genuine, disclosed structural divergence (a text-driven HUG cell that
/// collapses because text measurement is not wired into the oracle render
/// path); excluding it keeps the frame a clean measurement of the rest rather
/// than absorbing the divergence into the area budget.
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
    }

    Ok(OracleDiff {
        differing,
        total,
        max_channel_delta,
        band,
    })
}

/// Decodes a PNG to `((width, height), unpremultiplied RGBA8888 rows)`.
/// `label` names the image in the error (reference vs design source).
fn decode_rgba(png_bytes: &[u8], label: &str) -> Result<((i32, i32), Vec<u8>), String> {
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

/// Hard rect edges (the E3 exact-layout frames: wrap, grid, baseline).
///
/// A hard edge anti-aliased against the design source disagrees on a thin
/// 1–2 px band, where the reference painter's coverage rounding and Figma's
/// server-side export resampling can swing far apart. The fraction budget is
/// the primary tolerance — an edge is a small share of the canvas — and the
/// per-pixel threshold filters sub-threshold interior noise.
pub const AA_EDGE: ToleranceBand = ToleranceBand {
    rule: "aa-edge",
    channel_delta: 40,
    differing_fraction: 0.02,
};

/// A blurred shadow's soft falloff (the `sigma = blur/2` mapping, story #45).
///
/// A blur spreads a small per-pixel disagreement across a wide falloff
/// region — many pixels off by a little. The `sigma = blur/2` mapping is an
/// approximation of Figma's blur, so the whole falloff can be systematically
/// off by a small amount; a wider area budget with a moderate per-pixel
/// threshold pins "the falloff shape is close" without demanding pixel
/// identity. This is the band that will pin `sigma = blur/2` against a real
/// capture once #265 lands (`docs/decisions/effects-vocabulary-shadows.md`).
pub const BLUR_FALLOFF: ToleranceBand = ToleranceBand {
    rule: "blur-falloff",
    channel_delta: 24,
    differing_fraction: 0.12,
};

/// MSDF glyph edges (the text frames).
///
/// MSDF glyph edges are sharp high-contrast transitions; the reference
/// painter's MSDF resolve and Figma's font rasterizer disagree at glyph
/// boundaries (hinting, gamma). Text ink is sparse, so a small area budget
/// with a higher per-pixel threshold pins the glyph shapes without
/// over-tolerating.
pub const MSDF_TEXT: ToleranceBand = ToleranceBand {
    rule: "msdf-text",
    channel_delta: 50,
    differing_fraction: 0.03,
};

/// The pinned bands, keyed by their manifest `rule` name.
pub const BANDS: [&ToleranceBand; 3] = [&AA_EDGE, &BLUR_FALLOFF, &MSDF_TEXT];

/// The band a manifest frame's `band` name selects, or `None` if the name is
/// not one of the three pinned rules.
pub fn band_for(rule: &str) -> Option<&'static ToleranceBand> {
    BANDS.into_iter().find(|band| band.rule == rule)
}
