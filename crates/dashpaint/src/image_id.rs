//! Magic-byte identification and header parse for PNG, JPEG, and GIF —
//! **never decode**.
//!
//! [`identify`] answers one question about a byte slice: which of the three
//! containers is this, and what intrinsic extent does its header report? It
//! is the shared P4 primitive behind two different rules, in two different
//! crates:
//!
//! - **`dashc`'s compile gate** — the bytes claim a format on the wire
//!   (`ImageAsset::format`), and `identify` answers whether the bytes
//!   themselves back that claim, without trusting it (story #400).
//! - **`dashscene-validator`'s load gate** — an `AssetEntry` records a
//!   `format`, `width`, and `height` for a payload stored elsewhere in the
//!   file, and `identify` answers whether the payload agrees (story #437,
//!   debt #416).
//!
//! # Why it lives in `dashpaint`
//!
//! Because both of those callers must reach it. `dashpaint` publishes second
//! in the workspace order, before `dashscene-validator`, `dashc`, and
//! `dashpack` alike, so one implementation serves every writer and the gate
//! that checks them. It already owns [`ImageFormat`], which is the type this
//! module's answer is phrased in. Recorded in
//! `docs/decisions/image-header-parser-lives-in-dashpaint.md`.
//!
//! # The boundary this keeps
//!
//! [`identify`] does bounds-checked slicing over the input and returns. It
//! never reads pixel data and never allocates per pixel — no zlib inflate, no
//! Huffman/entropy decode, no LZW. That is deliberate and permanent: entropy
//! coding and pixel reconstruction are the part of an image codec that
//! carries the CVEs (decompression bombs, out-of-bounds writes from a
//! malformed Huffman table, LZW dictionary overruns), and keeping the
//! compiler on the header-only side of that line is the whole point of this
//! module existing instead of a decoder crate. A later change that wants a
//! thumbnail, a checksum of the pixels, or anything else that requires
//! walking compressed data does not belong here — it belongs in a painter,
//! behind the same trust boundary every other pixel decode already sits
//! behind (docs/specification/03-target-hardware-rules.md).
//!
//! Living in a crate that `dashc` depends on makes that boundary load-bearing
//! rather than advisory: anything added here reaches the compiler. The guard
//! is that `dashpaint` carries no third-party dependencies at all — a decoder
//! needs one, so a decode cannot arrive here without a manifest change that
//! `manifest_carries_no_third_party_dependencies` fails. The packer's decode
//! belongs in the packer, which publishes after everything here.
//!
//! # Scope
//!
//! Hand-rolled and scoped to exactly the PNG/JPEG/GIF closure — the three
//! containers Figma's REST API can ever serve an image fill as. No
//! third-party crate sits on this path on purpose: the emit path stays
//! zero-dependency for image bytes, and the accept-list below is this
//! module's own, not a library's. Revisit only if the format closure widens
//! beyond what one module can hold (the design capture's own caveat).

use crate::ImageFormat;

/// The intrinsic size and confirmed container format of an image's bytes.
///
/// `width`/`height` come straight from the header with no validation beyond
/// "the bytes parsed" — a caller that needs to refuse zero has its own
/// diagnostic for that (`crates/dashc/src/figma/mod.rs`, `rule::IMAGE_ZERO_DIMENSION`),
/// because only the caller knows whether zero is refusable in its context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeader {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

/// Why [`identify`] could not read a header.
///
/// Deliberately does not know about a producer's *claimed* format — that
/// comparison needs the caller's `ImageAsset::format`, which this function
/// never sees (its only input is the bytes). A caller wanting the
/// signature-contradicts-tag diagnostic compares [`ImageHeader::format`]
/// against its own tag itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageIdError {
    /// The bytes open with none of the three signatures this module knows
    /// (PNG, JPEG, GIF).
    UnknownSignature,
    /// The signature named `format`, but the header could not be parsed:
    /// truncated input, a chunk/segment/descriptor whose own shape is
    /// inconsistent, or — JPEG only — a frame marker outside the
    /// baseline/progressive set this module accepts.
    Malformed { format: ImageFormat, detail: String },
}

impl std::fmt::Display for ImageIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSignature => {
                write!(
                    f,
                    "the bytes match no known image signature (PNG, JPEG, GIF)"
                )
            }
            Self::Malformed { format, detail } => write!(f, "{format:?} header {detail}"),
        }
    }
}

impl std::error::Error for ImageIdError {}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Identifies `bytes` as PNG, JPEG, or GIF by magic signature, then parses
/// just enough of the header to recover the intrinsic width and height.
/// Never decodes a pixel (see the module doc for why that boundary is
/// permanent).
pub fn identify(bytes: &[u8]) -> Result<ImageHeader, ImageIdError> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        identify_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        identify_jpeg(bytes)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        identify_gif(bytes)
    } else {
        Err(ImageIdError::UnknownSignature)
    }
}

/// The 8-byte signature, then the `IHDR` chunk: a 4-byte big-endian chunk
/// length (always 13 for `IHDR`), the 4-byte chunk type (`IHDR` is always
/// the first chunk — the PNG spec guarantees it), then width and height as
/// big-endian `u32` at a fixed offset. Nothing past byte 24 is read.
fn identify_png(bytes: &[u8]) -> Result<ImageHeader, ImageIdError> {
    let malformed = |detail: String| ImageIdError::Malformed {
        format: ImageFormat::Png,
        detail,
    };

    let chunk_len = bytes
        .get(8..12)
        .ok_or_else(|| malformed("is truncated before the IHDR chunk length".to_string()))?;
    let chunk_len = u32::from_be_bytes(chunk_len.try_into().expect("a 4-byte slice is 4 bytes"));

    let chunk_type = bytes
        .get(12..16)
        .ok_or_else(|| malformed("is truncated before the IHDR chunk type".to_string()))?;
    if chunk_type != b"IHDR" {
        return Err(malformed(format!(
            "opens with chunk type {chunk_type:?}, not IHDR — IHDR must be the first chunk"
        )));
    }
    if chunk_len != 13 {
        return Err(malformed(format!(
            "declares an IHDR chunk length of {chunk_len}, not 13"
        )));
    }

    let width = bytes
        .get(16..20)
        .ok_or_else(|| malformed("is truncated inside the IHDR width".to_string()))?;
    let height = bytes
        .get(20..24)
        .ok_or_else(|| malformed("is truncated inside the IHDR height".to_string()))?;

    Ok(ImageHeader {
        format: ImageFormat::Png,
        width: u32::from_be_bytes(width.try_into().expect("a 4-byte slice is 4 bytes")),
        height: u32::from_be_bytes(height.try_into().expect("a 4-byte slice is 4 bytes")),
    })
}

/// `GIF87a`/`GIF89a`, then the logical screen descriptor's little-endian
/// `u16` width and height, immediately after the 6-byte signature. Nothing
/// past byte 10 is read — the descriptor's packed fields, background index,
/// and pixel aspect ratio carry no size information.
fn identify_gif(bytes: &[u8]) -> Result<ImageHeader, ImageIdError> {
    let malformed = |detail: &str| ImageIdError::Malformed {
        format: ImageFormat::Gif,
        detail: detail.to_string(),
    };

    let width = bytes
        .get(6..8)
        .ok_or_else(|| malformed("is truncated inside the logical screen descriptor width"))?;
    let height = bytes
        .get(8..10)
        .ok_or_else(|| malformed("is truncated inside the logical screen descriptor height"))?;

    Ok(ImageHeader {
        format: ImageFormat::Gif,
        width: u16::from_le_bytes(width.try_into().expect("a 2-byte slice is 2 bytes")) as u32,
        height: u16::from_le_bytes(height.try_into().expect("a 2-byte slice is 2 bytes")) as u32,
    })
}

/// `FFD8`, then walks the segment markers to the first `SOF0`/`SOF2` frame
/// header (baseline / progressive — the only two Figma's re-encoder ever
/// emits) and reads its big-endian height and width.
///
/// `FFD8`/`FFD9`/`FF01`/`FFD0`-`FFD7` carry no payload and are skipped
/// two bytes at a time; a `0xFF` fill byte before a marker is consumed one at
/// a time (ITU T.81 B.1.1.2); every other marker carries a big-endian length
/// (itself included) that bounds a payload to skip over — except a frame
/// marker outside the baseline/progressive set, and `FFDA` (start of scan,
/// meaning no frame header ever appeared), which refuse by name rather than
/// being parsed against a header shape they may not have (SOF3/5/6/7 are
/// lossless/differential with a different payload shape; SOF9/10/11/13/14/15
/// are the arithmetic-coded variants of the same; `FFF7` is JPEG-LS, an
/// unrelated ISO extension that reuses the marker range).
fn identify_jpeg(bytes: &[u8]) -> Result<ImageHeader, ImageIdError> {
    let malformed = |detail: String| ImageIdError::Malformed {
        format: ImageFormat::Jpeg,
        detail,
    };

    // Mirrors `crate::abi::wire::Reader`: every offset arrives through
    // `checked_add` and every read through `.get()`, so a truncated or
    // adversarial input runs out of bytes and returns an `Err` — it never
    // panics and never reads out of bounds.
    let mut at: usize = 2; // past FFD8, checked above by the caller's starts_with

    loop {
        let marker = bytes
            .get(
                at..at
                    .checked_add(2)
                    .ok_or_else(|| malformed("offset overflow".to_string()))?,
            )
            .ok_or_else(|| malformed(format!("is truncated before a marker at offset {at}")))?;
        if marker[0] != 0xFF {
            return Err(malformed(format!(
                "expected a marker (0xFF..) at offset {at}, found {:#04x}{:02x}",
                marker[0], marker[1]
            )));
        }
        let code = marker[1];

        // ITU T.81 B.1.1.2: any marker may be preceded by any number of fill
        // bytes, all 0xFF. Consume one and re-read — the marker code is the
        // first byte after the run that is not 0xFF. Without this, a legal
        // stream that pads before a marker is refused, and the refusal reads
        // as a truncation, because the second 0xFF is taken for the high byte
        // of a segment length. `at` still advances every iteration, so the
        // walk terminates.
        if code == 0xFF {
            at = at
                .checked_add(1)
                .ok_or_else(|| malformed("offset overflow".to_string()))?;
            continue;
        }

        at = at
            .checked_add(2)
            .ok_or_else(|| malformed("offset overflow".to_string()))?;

        // No-payload markers: TEM (01), RST0-RST7 (D0-D7), SOI (D8) — a
        // re-encountered SOI is tolerated the same way a real decoder skips
        // it, since it carries nothing to read either.
        if code == 0x01 || (0xD0..=0xD7).contains(&code) || code == 0xD8 {
            continue;
        }
        if code == 0xD9 {
            return Err(malformed("reached EOI before any frame header".to_string()));
        }
        if code == 0xDA {
            return Err(malformed(
                "reached the start of scan before any frame header".to_string(),
            ));
        }

        // Every remaining marker's length is big-endian and includes the
        // 2 length bytes themselves, so it is always >= 2.
        let len_bytes = bytes
            .get(
                at..at
                    .checked_add(2)
                    .ok_or_else(|| malformed("offset overflow".to_string()))?,
            )
            .ok_or_else(|| {
                malformed(format!(
                    "is truncated before a segment length at offset {at}"
                ))
            })?;
        let len =
            u16::from_be_bytes(len_bytes.try_into().expect("a 2-byte slice is 2 bytes")) as usize;
        if len < 2 {
            return Err(malformed(format!(
                "declares a segment length of {len} at offset {at}, under the 2-byte minimum"
            )));
        }
        let payload_start = at
            .checked_add(2)
            .ok_or_else(|| malformed("offset overflow".to_string()))?;
        let payload_end = at
            .checked_add(len)
            .ok_or_else(|| malformed("offset overflow".to_string()))?;

        match code {
            // SOF0 (baseline DCT) / SOF2 (progressive DCT) — the two Figma's
            // re-encoder produces (docs/wip 2026-07-19 design capture).
            0xC0 | 0xC2 => {
                // Only precision (1 byte) + height (2 BE) + width (2 BE) are
                // read; the per-component table after width never is. A
                // segment whose *declared* length is too short for even
                // those five bytes is malformed regardless of how much
                // buffer remains — reading past a too-short declared length
                // would misread the next segment's bytes as this one's
                // fields, not just be truncated.
                if payload_end < payload_start + 5 {
                    return Err(malformed(format!(
                        "declares a frame header segment of {len} bytes, too short for its \
                         own precision/height/width fields"
                    )));
                }
                let height = bytes
                    .get(payload_start + 1..payload_start + 3)
                    .ok_or_else(|| {
                        malformed("has a frame header truncated before its height".to_string())
                    })?;
                let width = bytes
                    .get(payload_start + 3..payload_start + 5)
                    .ok_or_else(|| {
                        malformed("has a frame header truncated before its width".to_string())
                    })?;
                return Ok(ImageHeader {
                    format: ImageFormat::Jpeg,
                    width: u16::from_be_bytes(width.try_into().expect("a 2-byte slice is 2 bytes"))
                        as u32,
                    height: u16::from_be_bytes(
                        height.try_into().expect("a 2-byte slice is 2 bytes"),
                    ) as u32,
                });
            }
            // Every other SOFn: lossless/differential (SOF1/3/5/6/7) and
            // their arithmetic-coded counterparts (SOF9/10/11/13/14/15), plus
            // JPEG-LS (0xF7, sometimes labelled SOF48/SOF55 — it reuses the
            // marker range but is an unrelated ISO extension). Named by the
            // marker byte rather than guessed at: none of these carry a
            // baseline/progressive-shaped payload, so parsing one as if it
            // did would read the wrong fields as width/height.
            0xC1 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF | 0xF7 => {
                return Err(malformed(format!(
                    "has frame marker {code:#04x}, which is neither SOF0 (baseline) nor SOF2 \
                     (progressive) — refused by name, not parsed against a layout it may not match"
                )));
            }
            // DQT, DHT, DAC, APPn, COM, DRI, and anything else this module
            // does not need: skip its length-prefixed payload and keep
            // walking toward the frame header.
            _ => {
                at = payload_end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real, minimal fixtures generated with ImageMagick (`magick -size WxH
    // xc:color out.ext`) and independently confirmed — by `sips` (macOS's
    // own image framework) and `file` (libmagic), neither of which shares a
    // line of code with this module — to report exactly these dimensions:
    // PNG 7x5, JPEG 9x6, GIF 11x8. Deliberately non-square on both axes, so
    // a width/height swap in the parser would fail every one of these
    // assertions, not pass by coincidence.
    //
    // `crates/dashc/tests/fixtures/image_id/` and
    // `crates/dashscene-validator/tests/fixtures/image_id/` carry
    // byte-identical copies, because each crate's tests have to build from
    // its own published tarball — a crate cannot reach into a sibling's
    // `tests/` directory. Three files under 700 bytes total is the cheaper
    // side of that trade.
    const SAMPLE_PNG: &[u8] = include_bytes!("../tests/fixtures/image_id/sample.png");
    const SAMPLE_JPEG: &[u8] = include_bytes!("../tests/fixtures/image_id/sample.jpg");
    const SAMPLE_GIF: &[u8] = include_bytes!("../tests/fixtures/image_id/sample.gif");

    #[test]
    fn a_png_reports_its_independently_confirmed_size() {
        assert_eq!(
            identify(SAMPLE_PNG),
            Ok(ImageHeader {
                format: ImageFormat::Png,
                width: 7,
                height: 5,
            })
        );
    }

    #[test]
    fn a_jpeg_reports_its_independently_confirmed_size() {
        assert_eq!(
            identify(SAMPLE_JPEG),
            Ok(ImageHeader {
                format: ImageFormat::Jpeg,
                width: 9,
                height: 6,
            })
        );
    }

    #[test]
    fn a_gif_reports_its_independently_confirmed_size() {
        assert_eq!(
            identify(SAMPLE_GIF),
            Ok(ImageHeader {
                format: ImageFormat::Gif,
                width: 11,
                height: 8,
            })
        );
    }

    #[test]
    fn bytes_matching_no_signature_are_unknown() {
        let bytes = b"this is plain text, not an image container of any kind";
        assert_eq!(identify(bytes), Err(ImageIdError::UnknownSignature));
    }

    #[test]
    fn an_empty_input_is_unknown_not_a_panic() {
        assert_eq!(identify(&[]), Err(ImageIdError::UnknownSignature));
    }

    #[test]
    fn a_png_whose_first_chunk_is_not_ihdr_is_malformed() {
        let mut bytes = SAMPLE_PNG.to_vec();
        // The chunk type at offset 12..16 is "IHDR"; corrupt one byte of it.
        assert_eq!(&bytes[12..16], b"IHDR");
        bytes[12] = b'x';
        match identify(&bytes) {
            Err(ImageIdError::Malformed { format, .. }) => assert_eq!(format, ImageFormat::Png),
            other => panic!("want Malformed, got {other:?}"),
        }
    }

    #[test]
    fn a_jpeg_frame_marker_outside_baseline_or_progressive_is_refused() {
        // Find the real fixture's FFC0 (SOF0) marker and replace it with
        // FFC3 (SOF3, lossless) — a real frame marker this module must
        // refuse by name rather than parse as if it were baseline-shaped.
        let mut bytes = SAMPLE_JPEG.to_vec();
        let at = bytes
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("the fixture carries a baseline SOF0 marker");
        bytes[at + 1] = 0xC3;
        match identify(&bytes) {
            Err(ImageIdError::Malformed { format, detail }) => {
                assert_eq!(format, ImageFormat::Jpeg);
                assert!(detail.contains("0xc3"), "detail names the marker: {detail}");
            }
            other => panic!("want Malformed, got {other:?}"),
        }
    }

    /// Fill bytes before a marker are legal (ITU T.81 B.1.1.2) and must not
    /// change what the header reports. Before this was handled, a padded
    /// stream was refused as malformed — a false refusal of valid input, which
    /// would block a legitimate import.
    #[test]
    fn fill_bytes_before_a_marker_do_not_change_the_result() {
        let at = SAMPLE_JPEG
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("the fixture carries a baseline SOF0 marker");
        let plain = identify(SAMPLE_JPEG).expect("the unpadded fixture parses");

        // One, two, and seven fill bytes: one exercises the rewind, and more
        // than one proves the walk consumes a run rather than a single byte.
        for pad in [1usize, 2, 7] {
            let mut padded = SAMPLE_JPEG[..at].to_vec();
            padded.extend(std::iter::repeat_n(0xFFu8, pad));
            padded.extend_from_slice(&SAMPLE_JPEG[at..]);
            assert_eq!(
                identify(&padded),
                Ok(plain),
                "{pad} fill byte(s) before the frame marker changed the result"
            );
        }
    }

    /// A JPEG signature followed by nothing but fill bytes must terminate with
    /// an error rather than looping: the fill-byte path advances one byte at a
    /// time, so the only thing stopping it is running out of input.
    #[test]
    fn a_stream_of_nothing_but_fill_bytes_terminates_with_an_error() {
        let mut bytes = vec![0xFFu8, 0xD8];
        bytes.extend(std::iter::repeat_n(0xFFu8, 4094));
        assert!(matches!(
            identify(&bytes),
            Err(ImageIdError::Malformed {
                format: ImageFormat::Jpeg,
                ..
            })
        ));
    }

    #[test]
    fn a_jpeg_with_no_frame_header_before_eoi_is_malformed() {
        // FFD8 immediately followed by FFD9 (EOI): a structurally valid
        // marker stream with no SOF at all.
        assert!(matches!(
            identify(&[0xFF, 0xD8, 0xFF, 0xD9]),
            Err(ImageIdError::Malformed {
                format: ImageFormat::Jpeg,
                ..
            })
        ));
    }

    /// Truncating each fixture at every length from 0 up to (but excluding)
    /// its independently-known header end must return an error, never a
    /// panic, and never a false `Ok`. The header-end offsets are fixed
    /// properties of these specific, already-committed fixture bytes —
    /// derived by an independent read of the files (`xxd` plus a
    /// from-scratch Python parse of the PNG/JPEG/GIF header layouts, not
    /// this module), not by asking `identify` where it happens to succeed.
    /// Truncating to exactly the header end must still succeed, which pins
    /// the boundary exactly instead of only checking "somewhere before this,
    /// it fails."
    #[test]
    fn truncation_below_the_header_end_always_errors_never_panics() {
        // (fixture, header end): PNG = 8-byte signature + 4-byte IHDR length
        // + 4-byte "IHDR" type + 4-byte width + 4-byte height. GIF = 6-byte
        // signature + 2-byte width + 2-byte height. JPEG = up through the
        // width field of this fixture's SOF0 payload (offset 158 marker +
        // 2 length + 1 precision + 2 height + 2 width = 167); specific to
        // this fixture's own segment layout, not a general JPEG constant.
        for (name, fixture, header_end) in [
            ("png", SAMPLE_PNG, 24),
            ("gif", SAMPLE_GIF, 10),
            ("jpeg", SAMPLE_JPEG, 167),
        ] {
            for cut in 0..header_end {
                assert!(
                    identify(&fixture[..cut]).is_err(),
                    "{name} at {cut} bytes (header end {header_end}) must not parse"
                );
            }
            assert!(
                identify(&fixture[..header_end]).is_ok(),
                "{name} at exactly {header_end} bytes (its header end) must parse"
            );
        }
    }

    /// Beyond the header-end robustness test above, every prefix of every
    /// fixture — the full truncation space, not just up to the header end —
    /// must never panic. `is_err()`/`is_ok()` are both fine; only a panic is
    /// a failure here.
    #[test]
    fn no_prefix_of_any_fixture_ever_panics() {
        for fixture in [SAMPLE_PNG, SAMPLE_JPEG, SAMPLE_GIF] {
            for cut in 0..=fixture.len() {
                let _ = identify(&fixture[..cut]);
            }
        }
    }

    /// The decode boundary, enforced rather than asserted in prose.
    ///
    /// `dashc` depends on `dashpaint`, so anything reachable from this crate
    /// is reachable from the compiler. `identify` is header-only by
    /// construction, but the risk this test addresses is the *next* change:
    /// a packer wanting real decode (entropy coding, pixel reconstruction —
    /// the CVE-bearing part) and putting it here because the format types
    /// are already here. That would hand the compiler a decoder through the
    /// back door, which
    /// `docs/decisions/dashc-identifies-images-never-decodes.md` forbids.
    ///
    /// No production-grade decoder is written without a dependency — `png`,
    /// `zune-jpeg`, `gif`, or `image`. So "this crate has no third-party
    /// dependencies" is a cheap, mechanical proxy for "no decoder lives
    /// here", and it fails loudly at the manifest line that would introduce
    /// one. Adding a dependency is then a deliberate act with a failing test
    /// attached, not an unremarked edit
    /// (`docs/decisions/image-header-parser-lives-in-dashpaint.md`).
    #[test]
    fn manifest_carries_no_third_party_dependencies() {
        let breach = |what: &str| -> ! {
            panic!(
                "dashpaint declares {what}. This crate is depended on by dashc, so a \
                 dependency here is reachable from the compiler, and a decoder dependency \
                 would breach the boundary \
                 docs/decisions/dashc-identifies-images-never-decodes.md draws. If the \
                 dependency is genuinely not a decoder, widen this test deliberately and say \
                 why in docs/decisions/image-header-parser-lives-in-dashpaint.md."
            )
        };

        let manifest = include_str!("../Cargo.toml");
        let mut in_dependencies = false;
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                let header = line.trim_start_matches('[').trim_end_matches(']');
                // Cargo lets one dependency have its own table —
                // `[dependencies.png]`, or `[target.'cfg(unix)'.dependencies.png]`.
                // That header names a dependency directly, so it is a breach on
                // sight; matching only the plain table headers below would let
                // the `version = "..."` line inside it pass as ordinary key/value
                // under no dependency table at all.
                if header.contains("dependencies.") {
                    breach(&format!("the dependency table `{line}`"));
                }
                // The plain dependency tables: `[dependencies]`,
                // `[dev-dependencies]`, `[build-dependencies]`, and their
                // `[target.'cfg(..)'.dependencies]` forms.
                in_dependencies = header.ends_with("dependencies");
                continue;
            }
            if !in_dependencies || line.is_empty() || line.starts_with('#') {
                continue;
            }
            breach(&format!("the dependency `{line}`"));
        }
    }

    /// The guard above has to fail on the shapes a dependency really arrives
    /// in, not only the one shape someone thought of. A guard that passes on
    /// `[dependencies.png]` is worse than no guard, because the decision record
    /// leans on it — it is the reason the parser lives in this crate rather
    /// than in one of its own.
    ///
    /// This drives the same scanner over synthetic manifests, so it pins the
    /// detection without touching the real `Cargo.toml`.
    #[test]
    fn the_dependency_guard_catches_every_shape_a_dependency_arrives_in() {
        // Mirrors the scanner above. Kept beside it deliberately: if one
        // changes, this fails until both agree.
        fn declares_a_dependency(manifest: &str) -> bool {
            let mut in_dependencies = false;
            for line in manifest.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    let header = line.trim_start_matches('[').trim_end_matches(']');
                    if header.contains("dependencies.") {
                        return true;
                    }
                    in_dependencies = header.ends_with("dependencies");
                    continue;
                }
                if !in_dependencies || line.is_empty() || line.starts_with('#') {
                    continue;
                }
                return true;
            }
            false
        }

        for (name, manifest) in [
            ("plain table", "[dependencies]\npng = \"0.17\"\n"),
            ("dev table", "[dev-dependencies]\npng = \"0.17\"\n"),
            ("build table", "[build-dependencies]\ncc = \"1\"\n"),
            (
                "per-dependency table",
                "[dependencies.png]\nversion = \"0.17\"\n",
            ),
            (
                "target dependency",
                "[target.'cfg(unix)'.dependencies]\npng = \"0.17\"\n",
            ),
            (
                "target per-dependency table",
                "[target.'cfg(unix)'.dependencies.png]\nversion = \"0.17\"\n",
            ),
        ] {
            assert!(
                declares_a_dependency(manifest),
                "the guard misses a dependency declared as a {name}"
            );
        }

        for (name, manifest) in [
            (
                "no dependencies at all",
                "[package]\nname = \"dashpaint\"\n",
            ),
            (
                "an empty dependency table",
                "[package]\nname = \"dashpaint\"\n\n[dependencies]\n",
            ),
            (
                "a commented-out dependency",
                "[dependencies]\n# png = \"0.17\"\n",
            ),
            (
                "a non-dependency table after one",
                "[dependencies]\n\n[lib]\nname = \"dashpaint\"\n",
            ),
        ] {
            assert!(
                !declares_a_dependency(manifest),
                "the guard falsely reports a dependency for {name}"
            );
        }
    }
}
