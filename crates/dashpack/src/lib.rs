//! Asset packer — encodes canonical payloads into per-profile derivations,
//! assembles cold banks, and records every choice in the derivation manifest
//! (`docs/design/architecture.md`, epic #345).
//!
//! # Where this sits in the pipeline
//!
//! `dashc` stays deterministic and lossless: it identifies image bytes and
//! parses their headers, and never decodes
//! (`docs/decisions/dashc-identifies-images-never-decodes.md`). **This crate
//! owns every lossy step.** Every asset has one canonical payload — the
//! original imported bytes, or lossless KTX2 where no original exists — and a
//! production encoding is a *derivation* of it, keyed by hash and mapped back
//! to canonical in a derivation manifest.
//!
//! # A quality profile is a band contract, not a format
//!
//! RAW, HiFi and LoFi are sets of per-asset-class tolerance bands. The packer
//! escalates per asset — cheap, then better, then lossless — until the band
//! holds, so over-compression is structurally impossible and entry hardware
//! never silently shows banding. RAW is the *null binding*: a regular `.dsb`
//! resolved against canonical payloads, which is exactly what v0.11 already
//! ships.
//!
//! # Why it lives in this workspace
//!
//! This is plain cargo, so it is a workspace member on the ordinary terms. Its
//! coupling is deep: it compiles against `dashbuf`'s asset and manifest
//! schemas, its band oracle reuses the golden oracle, and its weld and
//! profile-preview tests span packer output and the reference painter. The
//! standalone-tool requirement is met by the binary artifact
//! (`cargo build -p dashpack`), not by repo ownership.
//!
//! # Status
//!
//! Story #429 is the crate and its registered name. Story #430 adds [`astc`] —
//! ASTC encode and the matching reference decode, through a vendored,
//! version-pinned astcenc linked in process. Story #431 adds [`ktx2`] — the
//! container writer that wraps either an encoded payload or an uncompressed
//! one. Story #432 adds [`profile`] — the band contracts and the escalation
//! that picks a rung — and story #434 adds [`bank`], which packs a whole
//! document and assembles it into a `.dsb`. Story #435 adds [`preview`], the
//! read-side inverse: a derived block payload decoded back to RGBA so the Skia
//! reference painter can show what a profile costs, before any target bench
//! exists.

pub mod astc;
pub mod band;
pub mod bank;
pub mod ktx2;
#[cfg(feature = "preview")]
pub mod preview;
pub mod profile;

/// The packer's version, as reported by the `dashpack` binary.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reported_version_is_the_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
