//! Packing a document's assets under one profile, and assembling the result
//! into a `.dsb` cold bank (story #434).
//!
//! [`crate::profile::pack`] answers "what does this profile bind this one asset
//! to". This module is the step above it: every asset a document names, packed,
//! collected into a [`dashbuf::bank::ColdBank`], and assembled into a file whose
//! derivation manifest records the binding
//! (`docs/decisions/derivation-manifest-section.md`).
//!
//! # Why this is not a second `dashc`
//!
//! `dashc` assembles too, but only ever the null binding: it identifies image
//! bytes and never decodes them
//! (`docs/decisions/dashc-identifies-images-never-decodes.md`), so the bank it
//! builds is the identity map and its assembly cannot fail on any bank shape it
//! can construct — which is what lets it use an `expect`. This crate builds the
//! other kind. A derived bank can be unbound, can hold a payload no entry
//! names, and can bind one canonical hash to two payloads, so [`PackedBank::assemble`]
//! returns the refusal rather than panicking on it (P4).
//!
//! # What this does not do
//!
//! Decode. [`Asset::image`] is the canonical payload already in 8-bit RGBA, as
//! [`crate::profile::pack`] takes it: the canonical-to-texels ingest is a later
//! story and belongs with the format identification `dashc` already does, not
//! here.

use std::fmt;

use dashbuf::AssetKind;
use dashbuf::bank::{AssembleError, ColdBank};
use dashbuf::container::HASH_LEN;

use crate::astc::Rgba8;
use crate::profile::{Binding, PackError, Profile};

/// One asset offered to the packer.
#[derive(Debug, Clone, Copy)]
pub struct Asset<'a> {
    /// The canonical payload — the original imported bytes. Its BLAKE3 is the
    /// asset's identity, and it is what the document's `AssetEntry` names.
    pub canonical: &'a [u8],
    /// What the payload is, which fixes whether it may ever be encoded lossily.
    pub kind: AssetKind,
    /// The same payload decoded to 8-bit RGBA.
    pub image: Rgba8<'a>,
}

/// One asset after packing: its canonical identity, and what this profile bound
/// it to.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedAsset<'a> {
    /// BLAKE3 of [`Asset::canonical`], computed here rather than taken from the
    /// caller. The binding is anchored to the bytes themselves, so a caller
    /// cannot bind a derivation to an identity it does not belong to.
    pub canonical_hash: [u8; HASH_LEN],
    /// The canonical payload, borrowed.
    pub canonical: &'a [u8],
    /// What the profile bound it to, with the escalation record where there was
    /// one.
    pub binding: Binding,
}

impl PackedAsset<'_> {
    /// The bytes the assembled file carries for this asset.
    ///
    /// Under [`Binding::Canonical`] that is the canonical payload itself, which
    /// is what makes RAW cost nothing; under [`Binding::Derived`] it is the
    /// derivation's KTX2 file.
    pub fn resident(&self) -> &[u8] {
        match &self.binding {
            Binding::Canonical => self.canonical,
            Binding::Derived(derivation) => &derivation.file,
        }
    }
}

/// Every asset of one document, packed under one profile.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedBank<'a> {
    /// The profile that produced these bindings.
    pub profile: Profile,
    /// The assets, in the order they were offered.
    pub assets: Vec<PackedAsset<'a>>,
}

/// Packs every asset under `profile`.
///
/// Order is preserved but does not matter to assembly: a binding is found by
/// its canonical hash, so the caller is free to offer assets in any order and
/// the file still lays its blobs out in the document's entry order.
pub fn pack_bank<'a>(profile: Profile, assets: &[Asset<'a>]) -> Result<PackedBank<'a>, PackError> {
    let packed = assets
        .iter()
        .map(|asset| {
            Ok(PackedAsset {
                canonical_hash: blake3::hash(asset.canonical).into(),
                canonical: asset.canonical,
                binding: crate::profile::pack(profile, asset.kind, asset.image)?,
            })
        })
        .collect::<Result<Vec<_>, PackError>>()?;
    Ok(PackedBank {
        profile,
        assets: packed,
    })
}

impl PackedBank<'_> {
    /// This bank as the cold bank [`dashbuf::bank::assemble`] takes.
    pub fn cold_bank(&self) -> ColdBank<'_> {
        ColdBank::derived(
            self.assets
                .iter()
                .map(|asset| (asset.canonical_hash, asset.resident())),
        )
    }

    /// Assembles `ui_section` and this bank into `.dsb` file bytes.
    ///
    /// The refusal is returned, not panicked on. Three of
    /// [`AssembleError`]'s arms describe a disagreement between a document and
    /// a bank, and a packer whose profile disagreed with the document it was
    /// handed must report which — a panic would name none of them.
    pub fn assemble(&self, ui_section: &[u8]) -> Result<Vec<u8>, AssembleError> {
        dashbuf::bank::assemble(ui_section, &self.cold_bank())
    }

    /// What this profile cost, per asset and in total.
    pub fn report(&self) -> Report<'_> {
        Report(self)
    }
}

/// A packed bank's size analysis, rendered by its [`fmt::Display`].
///
/// Bytes and a ratio per asset, then the totals. The ratio is resident over
/// canonical, so it reads as "what the file pays": above 1.0 the profile made
/// this asset larger than shipping it untouched, which a small image can do —
/// a block-compressed payload has a floor of one block plus its container
/// framing, and the canonical PNG has neither.
#[derive(Debug, Clone, Copy)]
pub struct Report<'a>(&'a PackedBank<'a>);

impl Report<'_> {
    /// Canonical bytes across every asset.
    pub fn canonical_bytes(&self) -> usize {
        self.0.assets.iter().map(|a| a.canonical.len()).sum()
    }

    /// Resident bytes across every asset — what the assembled file's cold
    /// region carries, before alignment padding.
    pub fn resident_bytes(&self) -> usize {
        self.0.assets.iter().map(|a| a.resident().len()).sum()
    }
}

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "profile {:?}, {} asset(s)",
            self.0.profile,
            self.0.assets.len()
        )?;
        writeln!(
            f,
            "{:>5}  {:>12}  {:>10}  {:>10}  {:>7}",
            "asset", "rung", "canonical", "resident", "ratio"
        )?;
        for (index, asset) in self.0.assets.iter().enumerate() {
            let rung = match &asset.binding {
                Binding::Canonical => "canonical".to_string(),
                Binding::Derived(derivation) => derivation.rung.to_string(),
            };
            let (canonical, resident) = (asset.canonical.len(), asset.resident().len());
            writeln!(
                f,
                "{index:>5}  {rung:>12}  {canonical:>10}  {resident:>10}  {:>7.3}",
                resident as f64 / canonical as f64,
            )?;
        }
        let (canonical, resident) = (self.canonical_bytes(), self.resident_bytes());
        write!(
            f,
            "{:>5}  {:>12}  {canonical:>10}  {resident:>10}  {:>7.3}",
            "all",
            "",
            resident as f64 / canonical as f64,
        )
    }
}
