//! The three quality profiles, as per-asset-class band contracts, and the
//! escalation that makes over-compression structurally impossible.
//!
//! # The mechanism, in the order it runs
//!
//! 1. **Hard rules by kind.** [`AssetClass::of`] reads `AssetEntry.kind` and
//!    fixes what the asset *is*. A [`AssetClass::DistanceField`] has no lossy
//!    rung at all, so no measurement can put one on a lossy path — the rule is
//!    structural, not a check that a later change could route around.
//! 2. **The band contract.** [`contract`] pairs a profile with a class and
//!    yields one of three things: RAW's null binding, a lossless-only ladder,
//!    or a lossy ladder plus the band that grades it.
//! 3. **Escalation.** [`pack`] walks the ladder cheapest first, encoding a
//!    candidate at each rung, decoding it back through the same vendored
//!    astcenc, and diffing it against the canonical texels in the band
//!    vocabulary ([`crate::band`]). The first rung whose measurement holds
//!    wins. Every rejected rung is kept with the number that rejected it.
//!
//! # Where a walk ends, and the one place fidelity is traded
//!
//! Under [`Terminal::Lossless`] the last rung is [`Rung::Uncompressed`], whose
//! payload *is* the canonical texels. It cannot fail a band, so the walk always
//! terminates with a payload that satisfies the contract: over-compression is
//! impossible rather than unlikely, and a profile can only ever make a file
//! bigger than the packer's first guess, never worse than its band. RAW and
//! LoFi are both this, as is every distance field.
//!
//! **HiFi on image fills is not.** It ends at [`Terminal::FinestLossy`], which
//! ships the finest lossy rung with its measured exceedance disclosed rather
//! than escalating past it. That is a deliberate, provisional trade: on
//! photographic content HiFi's band rejects every ASTC footprint, so the
//! lossless terminal shipped four times the residency for the content class the
//! product actually ships. The exceedance is never silent — it is in
//! [`Derivation::accepted`], and [`crate::band::BandDiff::passes`] returns false
//! on it. `docs/decisions/asset-quality-profile-bands.md` section 7 carries the
//! trade; issue #553 carries the class split that would remove the need for it.
//!
//! # Where the numbers came from
//!
//! Every band value below was measured, not chosen and then confirmed. The
//! measurements, the fixtures, and the mutation that fails each band are in
//! `crates/dashpack/tests/band_contract.rs` and
//! `docs/decisions/asset-quality-profile-bands.md`.

use dashbuf::AssetKind;

use crate::astc::{self, AstcError, BlockSize, ColorSpace, Quality, Rgba8};
use crate::band::{self, BandDiff, BandError, ToleranceBand};
use crate::ktx2::{self, Ktx2Error};

/// How hard the packer makes astcenc search.
///
/// Packing is an offline step whose output ships, so the encoder is given real
/// effort rather than the interactive presets. Not [`Quality::Exhaustive`]:
/// astcenc's own guidance, repeated in [`crate::astc`], is that the last step
/// costs far more time than it recovers quality — and under this module the
/// quality it would recover is not free width in a band, it is the difference
/// between stopping one rung earlier and one rung later, which the ladder
/// already handles.
pub const PACK_QUALITY: Quality = Quality::Thorough;

/// What an asset's payload is, which is what decides whether it may ever be
/// encoded lossily. The packer's mirror of `dashbuf::AssetKind`.
///
/// A separate enum rather than the generated one because `flatc` models an
/// append-only enum as a newtype over `u8`, which has no exhaustive match: a
/// kind added to the schema later would fall silently into whatever wildcard
/// arm this module happened to have. [`AssetClass::of`] converts once and names
/// the unknown value (P4 — every out-of-profile construct is a named
/// diagnostic, never a silent drop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetClass {
    /// Displayed picture data — an image fill's payload.
    ImageFill,
    /// A signed or multi-channel distance field: a baked vector's MSDF, a glyph
    /// atlas. Never lossy.
    DistanceField,
}

impl AssetClass {
    /// The class an `AssetEntry.kind` names, or [`PackError::UnknownKind`].
    pub fn of(kind: AssetKind) -> Result<Self, PackError> {
        match kind {
            AssetKind::Image => Ok(Self::ImageFill),
            AssetKind::DistanceField => Ok(Self::DistanceField),
            other => Err(PackError::UnknownKind { kind: other.0 }),
        }
    }

    /// How the codec must read this class's channel values.
    ///
    /// A hard rule by kind, like the ladder. An image fill's RGB is
    /// sRGB-encoded, so the encoder has to weight its error in that space. A
    /// distance field's channels are not colour at all — they are distances —
    /// so they are linear, and treating them as sRGB would apply a transfer
    /// curve to a quantity that has none.
    pub fn color_space(self) -> ColorSpace {
        match self {
            Self::ImageFill => ColorSpace::Srgb,
            Self::DistanceField => ColorSpace::Linear,
        }
    }

    /// The lossy rungs this class may be encoded at, cheapest first. Empty for
    /// a class whose kind rule forbids every lossy path.
    ///
    /// The order is strictly increasing in bitrate, which is what makes "cheap
    /// then better" a well-defined walk: 0.89, 1.28, 2.00, 3.56, 5.12 and 8.00
    /// bits per texel. Only square footprints are rungs. ASTC's ten
    /// non-square footprints trade horizontal resolution for vertical, which is
    /// a property of the *content* rather than a step in quality — an
    /// anisotropic choice needs its own evidence, and would sit beside this
    /// ladder rather than inside it.
    ///
    /// [`Rung::Uncompressed`] is deliberately not in this list: it is the
    /// terminal rung of every ladder, including an empty one, and keeping it
    /// out of the sequence is what lets [`pack`] end without an unreachable
    /// branch.
    pub fn lossy_rungs(self) -> &'static [BlockSize] {
        match self {
            Self::ImageFill => &IMAGE_FILL_RUNGS,
            // The fields-never-lossy rule, expressed as the absence of a rung
            // to choose rather than as a check to pass. Measured rather than
            // assumed: at the *finest* ASTC footprint, 4x4 at 8 bits per texel,
            // the committed MSDF atlases still put 8.6044 % and 8.8753 % of
            // their texels beyond a delta of 8, with peak per-channel errors of
            // 84 and 70; at 12x12 the peak is a full-range 255. No lossy rung
            // can hold either profile's band for this content, so the rule
            // costs nothing that a measurement would have bought back.
            Self::DistanceField => &[],
        }
    }

    /// Whether any lossy encoding is admissible for this class.
    pub fn admits_lossy(self) -> bool {
        !self.lossy_rungs().is_empty()
    }
}

/// The image-fill ladder, cheapest first.
const IMAGE_FILL_RUNGS: [BlockSize; 6] = [
    BlockSize { x: 12, y: 12 },
    BlockSize { x: 10, y: 10 },
    BlockSize { x: 8, y: 8 },
    BlockSize { x: 6, y: 6 },
    BlockSize { x: 5, y: 5 },
    BlockSize { x: 4, y: 4 },
];

/// One step of a ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rung {
    /// ASTC LDR at one footprint.
    Astc(BlockSize),
    /// Uncompressed 8-bit RGBA — the terminal rung of every ladder, and the
    /// only one that is lossless by construction.
    Uncompressed,
}

impl Rung {
    /// The container format that names this rung for `class`.
    pub fn format(self, class: AssetClass) -> ktx2::Format {
        let color = class.color_space();
        match self {
            Self::Astc(block) => ktx2::Format::Astc { block, color },
            Self::Uncompressed => ktx2::Format::Rgba8 { color },
        }
    }
}

impl std::fmt::Display for Rung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Astc(block) => write!(f, "astc-{}x{}", block.x, block.y),
            Self::Uncompressed => write!(f, "uncompressed"),
        }
    }
}

/// A quality profile: a set of per-asset-class band contracts, not a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// The truth, and the *null binding*: a `.dsb` resolved against canonical
    /// payloads, with no derivation and no manifest. Not a shipping profile —
    /// it is the qualification baseline, the oracle lane and the developer
    /// preview (`docs/decisions/asset-quality-profile-naming.md`).
    Raw,
    /// The premium production target (SA8255 class). Tight bands.
    HiFi,
    /// The entry production target (SA7255 class). Looser bands. Defined now,
    /// activated when a measured budget or OTA constraint demands it.
    LoFi,
}

/// What one profile binds one asset class to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Contract {
    /// RAW. The canonical payload ships unchanged: nothing is derived, so there
    /// is nothing to encode and nothing to measure.
    NullBinding,
    /// The class's kind rule forbids every lossy encoding, so the ladder is the
    /// lossless rung alone.
    ///
    /// No band, deliberately. A band exists to *choose* between rungs, and
    /// there is no choice here — pinning a number that no measurement can ever
    /// fail is exactly the defect issue #422 recorded against `blur-falloff`,
    /// and the fix is to not write the number.
    LosslessOnly,
    /// A lossy ladder, every rung of which is graded against `band`, and where
    /// a walk that exhausts the ladder ends.
    Banded {
        band: &'static ToleranceBand,
        terminal: Terminal,
    },
}

/// Where an escalation that exhausts every lossy rung ends.
///
/// This is the one place a profile may trade fidelity for residency, and it is
/// expressed as its own value rather than as a branch inside [`pack`] so that
/// the trade is visible in the contract a caller reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// The uncompressed rung. Its payload *is* the canonical texels, so no band
    /// can refuse it and over-compression is structurally impossible.
    Lossless,
    /// The finest lossy rung, accepted with its measured exceedance disclosed
    /// rather than escalated past.
    ///
    /// **This is the one case where a derivation ships outside its band**, and
    /// it exists because the alternative measured worse in practice: on
    /// photographic content HiFi's band rejects every ASTC footprint, so
    /// [`Terminal::Lossless`] shipped uncompressed 8-bit RGBA — four times the
    /// residency — for a class the product actually ships
    /// (`docs/decisions/asset-quality-profile-bands.md` section 7, issue #553).
    ///
    /// A caller cannot mistake it for a pass: [`Derivation::accepted`] carries
    /// the measurement, and [`crate::band::BandDiff::passes`] returns false on
    /// it. It is a **provisional** answer pending the class split that would
    /// let a photographic band be graded on its own terms.
    FinestLossy,
}

/// The contract `profile` gives `class`.
pub fn contract(profile: Profile, class: AssetClass) -> Contract {
    match profile {
        Profile::Raw => Contract::NullBinding,
        Profile::HiFi | Profile::LoFi if !class.admits_lossy() => Contract::LosslessOnly,
        // HiFi stops at the finest lossy rung rather than escalating past it.
        // Measured: on all three committed photographs its band rejects every
        // footprint, so the lossless terminal shipped 1 MiB per 512x512 asset
        // where 4x4 ships 256 KiB and still measures at or above 90 on
        // SSIMULACRA2 — visually lossless on the published scale. Section 7 of
        // the band decision record carries the trade and its one disclosed
        // cost; issue #553 carries the class split that would replace it.
        Profile::HiFi => Contract::Banded {
            band: &HIFI_IMAGE_FILL,
            terminal: Terminal::FinestLossy,
        },
        // LoFi keeps the lossless terminal. Its band already accepts a lossy
        // rung on every committed asset, so the terminal is unreached and
        // costs nothing; leaving it in place keeps over-compression
        // structurally impossible for the profile that does not need the trade.
        Profile::LoFi => Contract::Banded {
            band: &LOFI_IMAGE_FILL,
            terminal: Terminal::Lossless,
        },
    }
}

/// HiFi, image fills — the premium target's band.
///
/// **The per-texel threshold is deliberately near the encoder's noise floor,
/// not at a visibility threshold.** The failure mode HiFi exists to prevent on
/// this class is banding across a smooth gradient: a *structured* error of small
/// amplitude spread over a wide area. A high per-texel threshold is blind to
/// exactly that, which is the shape of issue #422's finding about
/// `blur-falloff` — one number sizing a residual cannot also act as a gate. So
/// this band is set the other way round from the render bands: 2 of 255 is one
/// quantisation step above bit-exact, and the 1 % budget then says "all but a
/// hundredth of the texels are within one step".
///
/// It is not a render band's number and must not be read as one. The render
/// oracle's thresholds are 24 to 50 because it compares a CPU rasterizer
/// against a server-side export, and absorbs anti-aliasing, resampling and
/// gamma disagreement. Nothing here disagrees except the codec.
///
/// **Both knobs bind, measured.** On the committed `import-image-fill` payload
/// the threshold is what rejects 12x12 (19.1129 %) and the *budget* is what
/// rejects 8x8 (2.8012 %, against 1 %) and accepts 6x6 (0.2133 %).
pub const HIFI_IMAGE_FILL: ToleranceBand = ToleranceBand {
    rule: "hifi-image-fill",
    channel_delta: 2,
    differing_fraction: 0.01,
};

/// LoFi, image fills — the entry target's band.
///
/// Four times HiFi's per-texel threshold and five times its budget. 8 of 255 is
/// about 3 % of range, roughly where a single texel's error stops being
/// invisible against a flat neighbourhood on an 8-bit panel; the 5 % budget
/// says a twentieth of the texels may carry that much.
///
/// **The budget is the binding term, measured**, which is the property issue
/// #422 found `blur-falloff` lacked: on the `detail-noise` fixture this band
/// rejects 8x8 at 10.4401 % and accepts 6x6 at 4.8187 %, so the number 5 %
/// itself is what chooses the rung — not the threshold, and not arithmetic that
/// would have chosen the same rung for any budget.
pub const LOFI_IMAGE_FILL: ToleranceBand = ToleranceBand {
    rule: "lofi-image-fill",
    channel_delta: 8,
    differing_fraction: 0.05,
};

/// Every band this crate pins, keyed by its `rule` name.
///
/// Two, not six: RAW derives nothing and the distance-field class has no rung
/// to choose between, so neither has a band. A band is only written where a
/// measurement decides something.
pub const BANDS: [&ToleranceBand; 2] = [&HIFI_IMAGE_FILL, &LOFI_IMAGE_FILL];

/// The band a `rule` name selects, or `None` if it is not one of the pinned
/// contracts.
pub fn band_for(rule: &str) -> Option<&'static ToleranceBand> {
    BANDS.into_iter().find(|band| band.rule == rule)
}

/// One rung the escalation tried and refused, with the number that refused it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attempt {
    /// The rung that was encoded and measured.
    pub rung: Rung,
    /// What it measured against the contract's band.
    pub diff: BandDiff,
}

/// The payload a profile derived for one asset, and the measurements that chose
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    /// The class the kind rule fixed.
    pub class: AssetClass,
    /// The rung the escalation stopped at.
    pub rung: Rung,
    /// The container format `rung` is written as.
    pub format: ktx2::Format,
    /// The complete KTX2 file for `rung`.
    pub file: Vec<u8>,
    /// Every rung rejected before `rung`, in ladder order, with its
    /// measurement. Empty when the first rung held, and empty for a
    /// lossless-only class, which has nothing to reject.
    pub rejected: Vec<Attempt>,
    /// What the chosen rung measured. `None` only under
    /// [`Contract::LosslessOnly`], where there is no band because there is no
    /// choice for a band to make.
    pub accepted: Option<BandDiff>,
}

/// What a profile binds one asset to.
#[derive(Debug, Clone, PartialEq)]
pub enum Binding {
    /// RAW's null binding: the canonical payload itself. Stated as a variant
    /// rather than as prose, so a caller cannot ask RAW for a derived payload
    /// and receive a re-encoded one.
    Canonical,
    /// A derived payload, with the escalation record that produced it.
    Derived(Derivation),
}

/// Why an asset could not be packed.
#[derive(Debug, Clone, PartialEq)]
pub enum PackError {
    /// The entry's `kind` is a value this packer does not know. Named rather
    /// than defaulted: guessing a rule for an unknown kind is how a distance
    /// field would reach a lossy path (P4).
    UnknownKind { kind: u8 },
    /// The encoder or the reference decoder refused.
    Astc(AstcError),
    /// The container writer refused.
    Ktx2(Ktx2Error),
    /// The candidate did not decode back to the canonical extent.
    Band(BandError),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKind { kind } => write!(
                f,
                "asset kind {kind} is not one this packer has a rule for; it is refused rather \
                 than packed under another kind's rule"
            ),
            Self::Astc(error) => write!(f, "{error}"),
            Self::Ktx2(error) => write!(f, "{error}"),
            Self::Band(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<AstcError> for PackError {
    fn from(error: AstcError) -> Self {
        Self::Astc(error)
    }
}

impl From<Ktx2Error> for PackError {
    fn from(error: Ktx2Error) -> Self {
        Self::Ktx2(error)
    }
}

impl From<BandError> for PackError {
    fn from(error: BandError) -> Self {
        Self::Band(error)
    }
}

/// Packs one asset under one profile: kind rule, then escalation until the
/// band holds.
///
/// `image` is the canonical payload already decoded to 8-bit RGBA — the
/// canonical-to-texels step is the packer's ingest and not this function's. The
/// returned [`Binding`] carries the chosen rung, its KTX2 file, and every rung
/// the escalation rejected with the number that rejected it.
///
/// The walk always terminates: the ladder's lossy rungs are finite and
/// [`Rung::Uncompressed`] reproduces the canonical texels exactly, so it
/// satisfies any band.
pub fn pack(profile: Profile, kind: AssetKind, image: Rgba8<'_>) -> Result<Binding, PackError> {
    let class = AssetClass::of(kind)?;
    // Each of the three contracts leaves by its own path, so nothing below
    // carries a value that means two different things.
    let (band, terminal) = match contract(profile, class) {
        // RAW derives nothing at all.
        Contract::NullBinding => return Ok(Binding::Canonical),
        // No lossy rung to choose between, so no band and nothing to measure:
        // straight to the terminal rung.
        Contract::LosslessOnly => {
            return Ok(Binding::Derived(finish(
                class,
                Rung::Uncompressed,
                image.texels(),
                image,
                Vec::new(),
                None,
            )?));
        }
        Contract::Banded { band, terminal } => (band, terminal),
    };

    let mut rejected = Vec::new();
    // The finest rung walked so far that did not pass, held rather than
    // discarded because [`Terminal::FinestLossy`] ships it. Under
    // [`Terminal::Lossless`] it joins `rejected` on the next iteration and
    // nothing else changes.
    let mut finest: Option<(Rung, Vec<u8>, band::BandDiff)> = None;
    for &block in class.lossy_rungs() {
        let rung = Rung::Astc(block);
        let payload = astc::encode(image, block, class.color_space(), PACK_QUALITY)?;
        // Decoded by the same linked astcenc that encoded it, so the
        // measurement is of the format rather than of two libraries
        // disagreeing.
        let decoded = astc::decode(
            &payload,
            image.width(),
            image.height(),
            block,
            class.color_space(),
        )?;
        let measured = band::diff(image.texels(), &decoded, band)?;
        if measured.passes() {
            if let Some((rung, _, diff)) = finest {
                rejected.push(Attempt { rung, diff });
            }
            return Ok(Binding::Derived(finish(
                class,
                rung,
                &payload,
                image,
                rejected,
                Some(measured),
            )?));
        }
        if let Some((rung, _, diff)) = finest.replace((rung, payload, measured)) {
            rejected.push(Attempt { rung, diff });
        }
    }

    // The ladder is exhausted. Where the walk ends is the contract's to say.
    if terminal == Terminal::FinestLossy
        && let Some((rung, payload, measured)) = finest
    {
        // Ships outside the band, with the exceedance measured and carried in
        // `accepted` rather than dropped. See [`Terminal::FinestLossy`].
        return Ok(Binding::Derived(finish(
            class,
            rung,
            &payload,
            image,
            rejected,
            Some(measured),
        )?));
    }
    if let Some((rung, _, diff)) = finest {
        rejected.push(Attempt { rung, diff });
    }

    // The lossless terminal. Its payload is the canonical texels, so no band
    // can refuse it — which is what makes over-compression impossible rather
    // than merely unlikely for a contract that reaches here. It is measured
    // anyway, because a report that says 0.0000 % is worth more than one that
    // says the code did not look.
    let accepted = band::diff(image.texels(), image.texels(), band)?;
    Ok(Binding::Derived(finish(
        class,
        Rung::Uncompressed,
        image.texels(),
        image,
        rejected,
        Some(accepted),
    )?))
}

/// Wraps a chosen payload in its container and assembles the record.
fn finish(
    class: AssetClass,
    rung: Rung,
    payload: &[u8],
    image: Rgba8<'_>,
    rejected: Vec<Attempt>,
    accepted: Option<BandDiff>,
) -> Result<Derivation, PackError> {
    let format = rung.format(class);
    let file = ktx2::write(payload, image.width(), image.height(), format)?;
    Ok(Derivation {
        class,
        rung,
        format,
        file,
        rejected,
        accepted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_rule_names_an_unknown_value_rather_than_defaulting_it() {
        // The schema knows two kinds; a third would arrive as a raw value.
        let error = AssetClass::of(AssetKind(7)).expect_err("an unknown kind is refused");
        assert_eq!(error, PackError::UnknownKind { kind: 7 });
    }

    #[test]
    fn a_distance_field_has_no_lossy_rung_under_any_profile() {
        assert!(!AssetClass::DistanceField.admits_lossy());
        assert_eq!(AssetClass::DistanceField.lossy_rungs(), &[]);
        for profile in [Profile::Raw, Profile::HiFi, Profile::LoFi] {
            let contract = contract(profile, AssetClass::DistanceField);
            assert!(
                !matches!(contract, Contract::Banded { .. }),
                "{profile:?} must not put a distance field on a banded ladder"
            );
        }
    }

    #[test]
    fn raw_is_the_null_binding_for_every_class() {
        for class in [AssetClass::ImageFill, AssetClass::DistanceField] {
            assert_eq!(contract(Profile::Raw, class), Contract::NullBinding);
        }
    }

    #[test]
    fn the_image_fill_ladder_is_strictly_increasing_in_bitrate() {
        let rungs = AssetClass::ImageFill.lossy_rungs();
        let bits = |b: &BlockSize| 128.0 / f64::from(b.x * b.y);
        for pair in rungs.windows(2) {
            assert!(
                bits(&pair[0]) < bits(&pair[1]),
                "{:?} must be cheaper than {:?}",
                pair[0],
                pair[1]
            );
        }
        // Cheapest first, finest last: the walk is "cheap then better".
        assert_eq!(rungs.first(), Some(&BlockSize { x: 12, y: 12 }));
        assert_eq!(rungs.last(), Some(&BlockSize::ASTC_4X4));
    }

    #[test]
    fn every_pinned_band_is_reachable_by_its_rule_name() {
        for band in BANDS {
            assert_eq!(band_for(band.rule), Some(band));
        }
        assert_eq!(band_for("blur-falloff"), None);
    }

    #[test]
    fn a_band_exists_only_where_a_measurement_chooses_something() {
        // The #422 discipline in structural form: a pinned number that no
        // ladder can exercise is not written at all.
        for profile in [Profile::Raw, Profile::HiFi, Profile::LoFi] {
            for class in [AssetClass::ImageFill, AssetClass::DistanceField] {
                if let Contract::Banded { .. } = contract(profile, class) {
                    assert!(
                        class.admits_lossy(),
                        "{profile:?}/{class:?} pins a band with no rung to choose between"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rung_names_the_container_format_that_carries_it() {
        assert_eq!(
            Rung::Astc(BlockSize::ASTC_4X4).format(AssetClass::ImageFill),
            ktx2::Format::Astc {
                block: BlockSize::ASTC_4X4,
                color: ColorSpace::Srgb
            }
        );
        assert_eq!(
            Rung::Uncompressed.format(AssetClass::DistanceField),
            ktx2::Format::Rgba8 {
                color: ColorSpace::Linear
            }
        );
    }
}
