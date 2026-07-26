//! Rendering a document under a quality profile — the Gfx QA profile preview
//! (story #435).
//!
//! # What this is for
//!
//! The packer chooses an encoding per asset by a measured per-asset band
//! (`dashpack::profile`). That band is the right gate for the asset, and it is
//! blind to the one thing a designer actually looks at: the asset **in
//! context**. Banding across a gradient reads differently behind a caption than
//! it does alone, and an ASTC block boundary that measures as a rounding error
//! can line up against a stroke and become a visible step. This module renders
//! the whole scene under each profile so those are measured too.
//!
//! # Skia+profile against Skia+RAW
//!
//! Both sides of the comparison are the same painter, the same solver, the same
//! typesetter and the same canvas. The only variable is which bytes the asset
//! entries resolve to, so a difference is the asset axis and nothing else. That
//! is what makes this a purer measurement than any comparison against a design
//! source: no backend, no export pipeline, no resampling in the loop.
//!
//! # What desk cannot show
//!
//! Repeated from `dashpack::preview` because this is where a reader arrives:
//! GPU filtering behaviour, driver-level effects (vendor bandwidth compression
//! such as UBWC, and the NVIDIA case where ASTC is emulated rather than sampled
//! natively — the pack-time probe's job), and where in a target pipeline the
//! sRGB transfer function is applied. The bench confirms that short list. It
//! does not discover quality, because quality is settled here.
//!
//! # Why the canonical decode is Skia's
//!
//! [`derive`] decodes each canonical payload with the reference painter's own
//! codec, not with the `png` crate that `dashpack`'s band tests use. The two
//! arms of the comparison have to start from one decode: RAW paints the
//! canonical bytes through Skia, so the packer must measure and encode the
//! texels *Skia* reads out of them. Decoding with a second library would put a
//! decoder disagreement into every profile diff, where it would be
//! indistinguishable from encoder loss.
//!
//! `goldens/tooling/tests/derived_bank.rs` deliberately does the opposite and
//! decodes with the `png` crate: its golden is byte-exact over encoder output,
//! so it must feed the encoder the same texels `crates/dashpack/tests/
//! band_contract.rs` does. Different question, different correct answer.

use dashbuf::AssetKind;
use dashpack::astc::Rgba8;
use dashpack::bank::{Asset, pack_bank};
use dashpack::profile::Profile;

use crate::oracle::decode_rgba;

/// Why a document could not be rendered under a profile.
#[derive(Debug, Clone, PartialEq)]
pub enum DeriveError {
    /// The input is not a `.dsb` this build can open.
    Open { message: String },
    /// A canonical payload would not decode to texels. Named with its entry
    /// index, because a document may carry many and only one is at fault.
    Decode { index: usize, message: String },
    /// The packer refused an asset.
    Pack { message: String },
    /// The document and the derived bank would not assemble.
    Assemble { message: String },
}

impl std::fmt::Display for DeriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { message } => write!(f, "the document does not open: {message}"),
            Self::Decode { index, message } => write!(
                f,
                "asset {index}'s canonical payload does not decode to texels: {message}"
            ),
            Self::Pack { message } => write!(f, "the packer refused an asset: {message}"),
            Self::Assemble { message } => {
                write!(
                    f,
                    "the document and its derived bank do not assemble: {message}"
                )
            }
        }
    }
}

impl std::error::Error for DeriveError {}

/// The `.dsb` bytes `profile` binds this document's assets to.
///
/// Under [`Profile::Raw`] the input is returned unchanged: RAW is the null
/// binding, so re-deriving it would be a no-op that could only introduce a
/// difference. Under a production profile every asset is decoded, packed, and
/// the whole document reassembled with a derivation manifest.
///
/// The returned file is a complete, ordinary `.dsb`: `dashbuf::open` resolves
/// its canonical hashes through the manifest, and [`crate::render::render_dsb`]
/// renders it.
pub fn derive(dsb: &[u8], profile: Profile) -> Result<Vec<u8>, DeriveError> {
    if profile == Profile::Raw {
        return Ok(dsb.to_vec());
    }

    let (document, payloads) = dashbuf::open(dsb).map_err(|error| DeriveError::Open {
        message: error.to_string(),
    })?;
    let ui = dashbuf::container::ui_document(dsb)
        .map_err(|error| DeriveError::Open {
            message: error.to_string(),
        })?
        .to_vec();

    let entries = document.assets().unwrap_or_default();
    let kinds: Vec<AssetKind> = entries.iter().map(|entry| entry.kind()).collect();
    // Decoded up front and held, because `Rgba8` borrows its texels and the
    // packer takes a slice of assets rather than one at a time.
    let decoded = payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            decode_rgba(payload, "a canonical asset payload")
                .map_err(|message| DeriveError::Decode { index, message })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let assets: Vec<Asset<'_>> = payloads
        .iter()
        .zip(&kinds)
        .zip(&decoded)
        .map(|((canonical, &kind), ((width, height), texels))| Asset {
            canonical,
            kind,
            image: Rgba8::new(*width as u32, *height as u32, texels)
                .expect("a Skia decode returns width*height*4 texels"),
        })
        .collect();

    let bank = pack_bank(profile, &assets).map_err(|error| DeriveError::Pack {
        message: error.to_string(),
    })?;
    bank.assemble(&ui).map_err(|error| DeriveError::Assemble {
        message: error.to_string(),
    })
}

/// Renders `dsb` under `profile` and returns the PNG.
///
/// [`derive`] then [`crate::render::render_dsb`]. Under RAW this is exactly
/// `render_dsb` on the input, which is what makes RAW the reference arm of the
/// triptych rather than a fourth thing being compared.
pub fn render_under(dsb: &[u8], profile: Profile) -> Result<Vec<u8>, DeriveError> {
    Ok(crate::render::render_dsb(&derive(dsb, profile)?))
}

/// Parses a profile name as `just render --profile` and the oracle manifest
/// spell it: `raw`, `hifi`, `lite`. Case-insensitive.
///
/// `None` for anything else, so a caller reports the name it was given rather
/// than falling back to a default — silently rendering RAW when the user asked
/// for Lite is the one wrong answer this tool must not give.
pub fn profile_named(name: &str) -> Option<Profile> {
    match name.to_ascii_lowercase().as_str() {
        "raw" => Some(Profile::Raw),
        "hifi" => Some(Profile::HiFi),
        "lite" => Some(Profile::Lite),
        _ => None,
    }
}

/// The three profile names, in triptych order, for a usage message.
pub const PROFILE_NAMES: [&str; 3] = ["raw", "hifi", "lite"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_name_round_trips_and_an_unknown_one_is_refused() {
        for name in PROFILE_NAMES {
            assert!(profile_named(name).is_some(), "{name} names a profile");
        }
        assert_eq!(profile_named("HiFi"), Some(Profile::HiFi));
        assert_eq!(
            profile_named("medium"),
            None,
            "an unknown name is refused rather than defaulted to RAW"
        );
    }

    #[test]
    fn raw_returns_the_input_unchanged() {
        let file = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../goldens/dsb/v03-paint.dsb"),
        )
        .expect("the RAW golden is readable");
        assert_eq!(
            derive(&file, Profile::Raw).unwrap(),
            file,
            "RAW is the null binding, so deriving it must move no byte"
        );
    }
}
