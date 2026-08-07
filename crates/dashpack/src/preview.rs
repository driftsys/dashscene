//! Software decode of a derived block payload back to RGBA, so the Skia
//! reference painter can render a HiFi or LoFi bank on a desk (story #435).
//!
//! # The premise, and what it is worth
//!
//! ASTC decode is bit-exact by specification: every conformant decoder returns
//! the same texels for the same block. So a software decode of a derived
//! payload reconstructs the texels the target GPU samples, and a reference
//! render of a derived bank shows the quality the target ships — before any
//! target bench exists.
//!
//! # One pinned tool, both directions
//!
//! The block decode here is [`crate::astc::decode`], the same vendored,
//! version-pinned astcenc that [`crate::astc::encode`] produced the payload
//! with. That is deliberate, and it means the weld test
//! (`goldens/tooling/tests/profile_preview_weld.rs`) does **not** prove the
//! codec is right — both sides run the same codec. What it proves is
//! everything between the two: the KTX2 container round trip, the Zstd level,
//! the cold-bank assembly, `dashbuf::open_verified`'s resolution of a canonical
//! hash
//! through the derivation manifest, and — the part a second codec would not
//! have caught either — that this module *recovers* the block footprint and
//! colour space from the file instead of being told them.
//!
//! Recovery is the property worth welding. A preview that decoded at the
//! footprint its caller passed in would agree with the encoder even when the
//! file's `VkFormat` said something else, and a bank whose header disagreed
//! with its payload would render correctly here and wrongly on the target.
//! [`crate::ktx2::Format::from_vk_format`] is the recovery, and the weld's
//! wrong-footprint and wrong-colour-space mutations are what hold it.
//!
//! # Why the reader is not ours
//!
//! [`crate::ktx2`] writes with its own code and reads back in its tests with
//! the independent [`ktx2`](https://crates.io/crates/ktx2) crate, because a
//! check is worth more when someone else wrote it. The preview path is a read
//! path, so it reads with that same independent parser rather than with an
//! inverse of the writer. A writer bug that both a writer and a matching
//! hand-written reader would agree on therefore surfaces here as a preview
//! failure.
//!
//! # What a desk preview cannot show
//!
//! Stated here, and repeated wherever the preview is documented, so that a
//! target bench confirms a short list rather than discovering quality:
//!
//! - **GPU filtering behaviour.** The texels are exact; what a sampler does
//!   with them between texel centres — bilinear taps, anisotropic footprints,
//!   mip selection — is the hardware's and is not modelled here.
//! - **Driver-level effects.** Vendor bandwidth compression on top of the
//!   stored blocks (UBWC on Adreno), and the NVIDIA case where ASTC is
//!   emulated rather than sampled natively, with the residency cost that
//!   implies. Those are the pack-time probe's job, not this path's.
//! - **sRGB conversion points.** Where the transfer function is applied — in
//!   the sampler, in the shader, at the framebuffer — is a target pipeline
//!   property. This module decodes under the colour space the file records and
//!   hands back 8-bit texels; it does not model where a target converts them.
//!
//! # This is a preview, not a runtime
//!
//! Nothing here runs in a frame loop. It is a build-and-review path: the
//! goldens harness and `just render --profile <p>`. The reference painter is
//! unchanged and draws RGBA exactly as it always has, so P2 holds — a painter
//! still never measures, wraps, kerns or moves anything.

use crate::astc::{self, AstcError};
use crate::ktx2::{Format, IDENTIFIER, Ktx2Error};

/// One decoded derivation: the texels a target GPU samples, in the same 8-bit
/// RGBA layout [`crate::astc::Rgba8`] uses — rows top to bottom, four bytes per
/// texel, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Image width in texels, as the file records it.
    pub width: u32,
    /// Image height in texels, as the file records it.
    pub height: u32,
    /// The format the payload was stored in, recovered from the file's
    /// `VkFormat`. Carried so a caller can report which rung it is looking at
    /// without re-parsing.
    pub format: Format,
    /// `width * height * 4` bytes of unpremultiplied RGBA.
    pub rgba: Vec<u8>,
}

/// One derivation's stored texels, unwrapped from its container and inflated,
/// in whatever format it was written in.
///
/// What [`blocks`] returns, and what a painter that can sample the block format
/// binds — see `dashscene_core::BoundPayload`. The extent travels with the
/// texels because block payloads carry no header of their own, which is the
/// same reason boundary B's image row carries it (issue #716).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baked {
    /// Image width in texels, as the file records it.
    pub width: u32,
    /// Image height in texels, as the file records it.
    pub height: u32,
    /// The format the payload is stored in, recovered from the file's
    /// `VkFormat`.
    pub format: Format,
    /// The stored bytes: ASTC blocks, or eight-bit RGBA for the lossless rung.
    pub texels: Vec<u8>,
}

/// Why a payload could not be previewed.
///
/// Every arm names what was found. A preview that guessed would draw a
/// plausible wrong picture, which is the one outcome a quality-assurance path
/// must never produce (P4).
#[derive(Debug, Clone, PartialEq)]
pub enum PreviewError {
    /// The bytes are not a KTX2 file at all. Callers that sniff with
    /// [`is_ktx2`] first never see this; it exists so that a direct call
    /// cannot silently misread a PNG.
    NotKtx2,
    /// The independent parser refused the file. `message` is its own text.
    Parse { message: String },
    /// The file's `vkFormat` is `VK_FORMAT_UNDEFINED` — a universal format
    /// whose real encoding is decided at transcode time. This path has no
    /// transcoder.
    UndefinedFormat,
    /// A legal `VkFormat` this preview has no decoder for. Named rather than
    /// approximated by a nearby format.
    UnsupportedFormat { vk_format: u32 },
    /// The file is not the single-level 2D LDR shape [`crate::ktx2::write`]
    /// emits — a mip chain, an array, a cubemap, or a 3D texture. `found`
    /// describes what it is instead.
    UnsupportedShape { found: String },
    /// The supercompression scheme is not Zstd. `crate::ktx2::write` always
    /// writes Zstd, so anything else came from another writer.
    UnsupportedSupercompression { scheme: Option<u32> },
    /// The level would not inflate. `message` is libzstd's own text.
    Inflate { message: String },
    /// The inflated level is not the length the recorded format and dimensions
    /// require. A short payload decoded anyway would show the tail of an image
    /// as whatever the buffer held.
    PayloadLen { expected: usize, found: usize },
    /// The block decode refused the payload.
    Astc(AstcError),
    /// The recorded format could not be sized — an image large enough that its
    /// payload length does not fit in a `usize`.
    Ktx2(Ktx2Error),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotKtx2 => write!(
                f,
                "the payload does not begin with the KTX2 identifier, so it is \
                 not a derived block payload"
            ),
            Self::Parse { message } => {
                write!(f, "the KTX2 file does not parse: {message}")
            }
            Self::UndefinedFormat => write!(
                f,
                "the file's vkFormat is VK_FORMAT_UNDEFINED — a universal \
                 format needing a transcoder the preview path does not have"
            ),
            Self::UnsupportedFormat { vk_format } => write!(
                f,
                "the preview path has no decoder for VkFormat {vk_format}"
            ),
            Self::UnsupportedShape { found } => write!(
                f,
                "the preview path decodes one level of a 2D LDR image, but this \
                 file is {found}"
            ),
            Self::UnsupportedSupercompression { scheme } => match scheme {
                Some(scheme) => write!(
                    f,
                    "the level uses supercompression scheme {scheme}, but the \
                     preview path only inflates Zstd (scheme 2)"
                ),
                None => write!(
                    f,
                    "the level uses an unrecognised supercompression scheme, \
                     but the preview path only inflates Zstd (scheme 2)"
                ),
            },
            Self::Inflate { message } => {
                write!(f, "the level would not inflate: {message}")
            }
            Self::PayloadLen { expected, found } => write!(
                f,
                "the level inflated to {found} bytes, but the recorded format \
                 and size need exactly {expected}"
            ),
            Self::Astc(error) => write!(f, "the block payload would not decode: {error}"),
            Self::Ktx2(error) => write!(f, "the recorded format could not be sized: {error}"),
        }
    }
}

impl std::error::Error for PreviewError {}

impl From<AstcError> for PreviewError {
    fn from(error: AstcError) -> Self {
        Self::Astc(error)
    }
}

impl From<Ktx2Error> for PreviewError {
    fn from(error: Ktx2Error) -> Self {
        Self::Ktx2(error)
    }
}

/// Whether `bytes` begins with the KTX2 identifier.
///
/// This is the loader's discriminator: under RAW an asset's resident payload is
/// its canonical PNG, JPEG or GIF, and under a derived profile it is a KTX2
/// file. None of the three canonical containers can begin with the KTX2
/// identifier — its first byte, `0xAB`, is not the first byte of any of their
/// magic numbers — so the test is exact rather than heuristic.
pub fn is_ktx2(bytes: &[u8]) -> bool {
    bytes.starts_with(&IDENTIFIER)
}

/// Opens a derived KTX2 payload and returns the stored blocks, unwrapped but
/// **not** decoded.
///
/// The half of [`decode`] a GPU painter wants and the half a CPU painter does
/// not: it parses the container, checks its shape, inflates the Zstandard
/// supercompression and checks the level's length against what the recorded
/// format and extent require — and then stops, because a painter that can
/// sample the block format has no use for the ASTC decode that follows.
///
/// # This is not the transcode step the target-hardware rules forbid
///
/// `docs/specification/03-target-hardware-rules.md` requires product assets
/// ship "as native ASTC directly, with no Basis and no transcode step of any
/// kind". Unwrapping a container and inflating a lossless supercompression is
/// neither: the blocks that come out are byte for byte the blocks the encoder
/// wrote, at the footprint it wrote them, and nothing re-encodes anything. What
/// the rule forbids is arriving at the block format at run time from some other
/// one, which is what a Basis transcode does and what
/// [`decode`] plus a re-encode would do.
///
/// It also runs once, at load, rather than per frame — P3.
pub fn blocks(file: &[u8]) -> Result<Baked, PreviewError> {
    if !is_ktx2(file) {
        return Err(PreviewError::NotKtx2);
    }
    let reader = ktx2::Reader::new(file).map_err(|error| PreviewError::Parse {
        message: format!("{error:?}"),
    })?;
    let header = reader.header();

    // The shape checks come before the format, so a mip chain or a cubemap is
    // named as such rather than reported as a payload-length disagreement.
    let shape = if header.level_count > 1 {
        Some(format!("{} levels", header.level_count))
    } else if header.pixel_depth != 0 {
        Some(format!("3D ({} deep)", header.pixel_depth))
    } else if header.layer_count > 0 {
        Some(format!("an array of {} layers", header.layer_count))
    } else if header.face_count != 1 {
        Some(format!("a cubemap of {} faces", header.face_count))
    } else if header.pixel_height == 0 {
        Some("1D".to_string())
    } else {
        None
    };
    if let Some(found) = shape {
        return Err(PreviewError::UnsupportedShape { found });
    }

    if header.supercompression_scheme != Some(ktx2::SupercompressionScheme::Zstandard) {
        return Err(PreviewError::UnsupportedSupercompression {
            scheme: header.supercompression_scheme.map(|s| s.value()),
        });
    }

    let vk_format = header.format.ok_or(PreviewError::UndefinedFormat)?.value();
    let format =
        Format::from_vk_format(vk_format).ok_or(PreviewError::UnsupportedFormat { vk_format })?;

    let (width, height) = (header.pixel_width, header.pixel_height);
    let level = reader
        .levels()
        .next()
        .ok_or_else(|| PreviewError::UnsupportedShape {
            found: "a file with no levels".to_string(),
        })?;
    let payload = zstd::bulk::decompress(level.data, level.uncompressed_byte_length as usize)
        .map_err(|error| PreviewError::Inflate {
            message: error.to_string(),
        })?;

    // The recorded length is what the header claims; this is what the format
    // and the dimensions require. They are different claims and both are
    // checked, because a level whose recorded length matches its own inflated
    // bytes can still be the wrong length for the image the header describes.
    let expected = format.payload_len(width, height)?;
    if payload.len() != expected {
        return Err(PreviewError::PayloadLen {
            expected,
            found: payload.len(),
        });
    }

    Ok(Baked {
        width,
        height,
        format,
        texels: payload,
    })
}

/// Decodes a derived KTX2 payload back to the texels a target GPU samples.
///
/// The block footprint and colour space are read out of the file's `VkFormat`,
/// never taken from the caller — see the module documentation for why that is
/// the property the weld test exists to hold.
pub fn decode(file: &[u8]) -> Result<Preview, PreviewError> {
    let baked = blocks(file)?;
    let rgba = match baked.format {
        // The lossless rung: the level payload already is the texels. Copied
        // rather than reinterpreted, so the caller owns them.
        Format::Rgba8 { .. } => baked.texels,
        Format::Astc { block, color } => {
            astc::decode(&baked.texels, baked.width, baked.height, block, color)?
        }
    };

    Ok(Preview {
        width: baked.width,
        height: baked.height,
        format: baked.format,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astc::{ColorSpace, Rgba8};
    use crate::ktx2::write;

    /// A deterministic gradient, large enough to be more than one block at
    /// every footprint on the ladder.
    fn texels(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                out.extend_from_slice(&[
                    (x * 255 / width.max(1)) as u8,
                    (y * 255 / height.max(1)) as u8,
                    ((x + y) * 127 / (width + height).max(1)) as u8,
                    255,
                ]);
            }
        }
        out
    }

    #[test]
    fn a_lossless_file_previews_back_to_the_texels_it_was_written_from() {
        let (w, h) = (40u32, 24u32);
        let source = texels(w, h);
        let file = write(
            &source,
            w,
            h,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .expect("the lossless file writes");

        let preview = decode(&file).expect("the lossless file previews");

        assert_eq!((preview.width, preview.height), (w, h));
        assert_eq!(
            preview.format,
            Format::Rgba8 {
                color: ColorSpace::Srgb
            },
            "the colour space is recovered from the file, not assumed"
        );
        assert_eq!(
            preview.rgba, source,
            "the lossless rung is the identity: preview must return the exact texels"
        );
    }

    #[test]
    fn the_block_footprint_is_recovered_from_the_file_rather_than_assumed() {
        let (w, h) = (48u32, 48u32);
        let source = texels(w, h);
        let image = Rgba8::new(w, h, &source).unwrap();
        // Two footprints, so a preview that hard-coded one would fail on the
        // other rather than happening to agree.
        for block in [
            crate::astc::BlockSize { x: 6, y: 6 },
            crate::astc::BlockSize { x: 8, y: 8 },
        ] {
            let payload = astc::encode(image, block, ColorSpace::Srgb, crate::astc::Quality::Fast)
                .expect("the payload encodes");
            let file = write(
                &payload,
                w,
                h,
                Format::Astc {
                    block,
                    color: ColorSpace::Srgb,
                },
            )
            .expect("the file writes");

            let preview = decode(&file).expect("the file previews");

            assert_eq!(
                preview.format,
                Format::Astc {
                    block,
                    color: ColorSpace::Srgb
                },
                "the {}x{} footprint must be read back off the file",
                block.x,
                block.y
            );
            assert_eq!(preview.rgba.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn a_payload_that_is_not_ktx2_is_named_rather_than_guessed_at() {
        // A PNG signature. The loader sniffs before calling, but a direct call
        // must refuse rather than misread.
        let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(!is_ktx2(&png));
        assert_eq!(decode(&png), Err(PreviewError::NotKtx2));
    }

    #[test]
    fn a_truncated_level_is_refused_rather_than_decoded_short() {
        let (w, h) = (40u32, 24u32);
        let source = texels(w, h);
        let file = write(
            &source,
            w,
            h,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        // Drop the last byte of the level. The header still describes the full
        // image, so a preview that trusted the header would return a buffer
        // holding whatever followed.
        let truncated = &file[..file.len() - 1];

        let error = decode(truncated).expect_err("a truncated level must be refused");
        assert!(
            matches!(
                error,
                PreviewError::Inflate { .. } | PreviewError::Parse { .. }
            ),
            "a truncated level is an inflate or parse refusal, got {error:?}"
        );
    }
}
