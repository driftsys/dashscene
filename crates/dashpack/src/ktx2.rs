//! A KTX2 container writer, narrow on purpose: one mip level, 2D, LDR, always
//! Zstd-supercompressed, with the data format descriptor the container requires
//! (story #431).
//!
//! # Why the container is the shared standard, and the codec is not
//!
//! There is no hardware codec every target decodes. ASTC is the mobile universe
//! and BC is the desktop one, and the intersection is empty
//! (`docs/decisions/native-astc-codec-table.md`). What every target does share
//! is the container, so KTX2 is the distribution format on all of them and the
//! codec inside it is a per-target choice.
//!
//! # Why the writer is ours and the reader is not
//!
//! Emitting bytes is a decision the packer has to own: which vkFormat, which
//! colour space, which supercompression, which metadata. That is a few hundred
//! lines against a published layout, and putting a third-party writer there
//! would put a third-party's defaults into every bank. Parsing bytes back is
//! the opposite — it is a check, and a check is worth more when someone else
//! wrote it. So the [`ktx2`](https://crates.io/crates/ktx2) crate reads and this
//! module writes: it is a dev-dependency for this module's own round-trip
//! tests, and an optional dependency behind the `preview` feature for
//! [`crate::preview`], which is a read path too (story #435). **It never
//! appears on the emit path** — that is the line, rather than "it is only ever a
//! dev-dependency", which stopped being true when the preview landed.
//!
//! # What "narrow" means
//!
//! The writer emits exactly what the packer needs and refuses the rest:
//!
//! - **One level.** No mip chain. `levelCount` is always 1.
//! - **2D, not an array, not a cubemap.** `pixelDepth` 0, `layerCount` 0,
//!   `faceCount` 1.
//! - **LDR only.** Uncompressed 8-bit RGBA, or ASTC at one of the fourteen 2D
//!   footprints. No HDR, no float, no depth or stencil.
//! - **Always Zstd.** Never uncompressed level data. That is not only a size
//!   decision: `mipPadding` exists only when `supercompressionScheme` is 0, so
//!   fixing the scheme removes the alignment arithmetic from the writer
//!   entirely, and there is then no padding rule left to get wrong.
//!
//! Anything outside that comes back as a named [`Ktx2Error`], never as a file
//! with a plausible header and wrong contents.
//!
//! # Reproducibility
//!
//! The same payload, dimensions and format always produce the same bytes.
//! libzstd is compiled from the vendored sources the `zstd` crate carries, and
//! `Cargo.lock` is committed (`docs/decisions/cargo-lock-is-committed.md`), so
//! the compressor is pinned the same way the ASTC encoder is. Every input is
//! either the caller's or a pinned constant, including the `KTXwriter` value —
//! see [`WRITER`], which is deliberately not the crate version.

use crate::astc::{self, BlockSize, ColorSpace};

/// The 12-byte KTX2 file identifier, from the specification's own table:
/// `0xAB`, `KTX 20`, `0xBB`, `\r\n`, `\x1A`, `\n`.
///
/// The trailing bytes are a transfer check rather than decoration. A file
/// mangled by a text-mode copy has its `\r\n` rewritten, and a truncating
/// transfer loses the `\x1A`, so both corruptions are caught by the first
/// twelve bytes instead of surfacing as a malformed header.
///
/// Public because the preview load path sniffs it to tell a derived block
/// payload from a canonical PNG, JPEG or GIF before choosing a decoder
/// (`crate::preview::is_ktx2`).
pub const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

/// The fixed size of the KTX2 header: the identifier, nine `UInt32` fields, and
/// the index of the four sections that follow it.
const HEADER_BYTES: usize = 80;

/// The size of one level index entry: `byteOffset`, `byteLength` and
/// `uncompressedByteLength`, each a `UInt64`.
const LEVEL_INDEX_ENTRY_BYTES: usize = 24;

/// `supercompressionScheme` for Zstandard, from the specification's scheme
/// table. 0 is none, 1 is BasisLZ, 3 is ZLIB.
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// `typeSize`, the size in bytes of one channel's data type. It is 1 for every
/// format this writer emits: the specification fixes it at 1 for block-compressed
/// formats, and 8-bit channels give 1 for the uncompressed one.
///
/// It exists so that a big-endian host knows how to byte-swap image data. At 1
/// there is nothing to swap, which is why this writer never has to.
const TYPE_SIZE: u32 = 1;

/// The Zstd compression level every level payload is compressed at.
///
/// Fixed rather than a parameter. The packer is an offline step and its output
/// is what ships, so bytes at rest are worth more than pack time; 19 is the
/// highest level that does not enable the long-distance matcher, whose window
/// would make decompression cost memory on the target rather than on the build
/// machine.
///
/// Changing it changes every byte of every file this writer emits, so it is a
/// deliberate re-baseline and not a tuning knob.
pub const ZSTD_LEVEL: i32 = 19;

/// The generation of this writer's own byte layout.
///
/// The handle for a deliberate re-baseline of everything that is this crate's
/// choice rather than the caller's: the key/value set, the level layout,
/// [`ZSTD_LEVEL`]. Bump it in the same commit that changes any of them, so a
/// file says which layout produced it.
///
/// It is not the crate version, and the difference is the point — see
/// [`WRITER`].
pub const WRITER_GENERATION: u32 = 1;

/// The `KTXwriter` value recorded in every file.
///
/// The specification asks a writer to identify itself, and a bank is auditable
/// only if the tool that produced it is named in the file — the same reason
/// [`astc::vendored_astcenc`] pins the encoder.
///
/// # Why this is not the crate version
///
/// It was `concat!("dashpack ", env!("CARGO_PKG_VERSION"))` when this writer
/// landed (story #431), which made every emitted byte a function of the release
/// cadence. Two consequences, and the second is the one that settled it:
///
/// - A `git std bump` would move every byte-exact golden over packer output, so
///   a routine release and a real encoder regression would produce the same
///   signal. Story #434 is where committed artifacts began carrying KTX2
///   output, so that stopped being hypothetical.
/// - Every texture payload in every shipped cold bank would change on every
///   release, whether or not a single texel differed. A cold bank is the large
///   part of a `.dsb`, and the asset model budgets flash and OTA size
///   explicitly (story #434), so a version string that invalidates every
///   texture in an OTA delta is a cost paid for nothing.
///
/// What a texture file's provenance is *for* is answering which pipeline
/// produced these bytes. That is the encoder pin and this writer's own layout
/// generation — both of which change exactly when the output can — and not the
/// release number, which changes when it cannot. Per-release provenance, if it
/// is ever wanted, belongs once in the container envelope rather than repeated
/// inside every texture.
///
/// Both the generation and the astcenc version appear here as one literal, so
/// the emitted string can be read straight off this line. The two assertions
/// below are what keep it welded to [`WRITER_GENERATION`] and to the vendored
/// pin, so a bump that forgot this string fails the build rather than shipping
/// a file that misnames its own encoder.
pub const WRITER: &str = "dashpack gen1 astcenc 5.6.0";

const _: () = assert!(WRITER_GENERATION == 1, "the gen in WRITER is written out");
const _: () = assert!(
    str_eq(dashpack_astcenc_sys::VENDORED_VERSION, "5.6.0"),
    "the astcenc version in WRITER no longer matches the vendored pin"
);

// **`WRITER` actually carries that version**, which nothing checked until issue
// #1178: the assertion above welds the literal `"5.6.0"` to the vendored pin and
// never reads `WRITER`, so changing `WRITER` alone left the file misnaming its
// own encoder with every assertion green.
const _: () = assert!(
    str_eq(WRITER, "dashpack gen1 astcenc 5.6.0",),
    "WRITER no longer spells the version the assertion above pins"
);

// **`str_eq` itself, pinned in both length directions** (issue #1178).
//
// Its only non-test caller is the assertion above, which evaluates it on a pair
// that agrees — so a weakened comparison is never exercised on one it would get
// wrong. A body returning `true` unconditionally lets the version pin accept
// **any** version, with every tier green. `dashscene-android`'s twin gained a
// test for exactly that in PR #1163.
//
// **`const` rather than a `#[test]`**, which is where this side differs from the
// twin and why: that copy's own pin sits behind `#[cfg(target_os = "android")]`,
// so no host build evaluates it and a runtime test is the only thing that can.
// This one is unconditional, so an assertion here fails the **build** on every
// target and every tier — `just wasm` and `just android` included — rather than
// only where a tier runs.
//
// Both orderings, which is the case a single direction misses: a guard weakened
// to `a.len() < b.len()` with the loop bounded by `b.len()` passes every
// shorter-first case while `str_eq("5.6.0.1", "5.6.0")` answers true, and the
// pin then accepts any version starting with 5.6.0.
const _: () = assert!(str_eq("5.6.0", "5.6.0"), "equal strings compare equal");
const _: () = assert!(str_eq("", ""), "and so do two empty ones");
const _: () = assert!(
    !str_eq("5.6.0", "5.6.1"),
    "same length, different byte: what a length-only comparison would accept"
);
const _: () = assert!(!str_eq("5.6", "5.6.0"), "a prefix is not a match");
const _: () = assert!(
    !str_eq("5.6.0.1", "5.6.0"),
    "nor is it one with the longer string first, which is the direction a \
     one-sided length guard admits"
);
const _: () = assert!(!str_eq("6.0", "5.6.0"), "nor is a suffix");
const _: () = assert!(!str_eq("5.6.0", "6.0"), "in either order");

/// `&str` equality in a const context, which the standard library does not yet
/// offer. Exists only for the assertion above.
///
/// **A second copy of this exists**, in `dashscene-android`'s
/// `face::same_descriptor`, welding a JNI descriptor to a `jni_sig!` literal
/// inside the same `const _: () = assert!(..)` shape. Same algorithm, same
/// length-guard-then-byte-loop, same reason.
///
/// **They are deliberately not shared** (issue #1178, and
/// `docs/decisions/crate-name-map.md` records the ruling). The duplication is
/// accepted; what is not accepted is two copies with no check and no pointer
/// between them, which is what that issue was filed for. The assertions below
/// close this side.
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// The `KTXorientation` value recorded in every file: S increases to the right,
/// T increases downwards.
///
/// That is a top-left origin, which is how every payload reaches this crate —
/// [`astc::Rgba8`] is documented as rows top to bottom, and so is a decoded PNG.
/// Written explicitly rather than left to a consumer's default, because the
/// two conventions differ by a vertical flip and a silently flipped texture is
/// a defect no header check would catch.
const ORIENTATION: &str = "rd";

/// `colorModel` `KHR_DF_MODEL_RGBSDA`, the red/green/blue/stencil/depth/alpha
/// model an uncompressed colour image uses.
const COLOR_MODEL_RGBSDA: u8 = 1;

/// `colorModel` `KHR_DF_MODEL_ASTC`. A block-compressed format carries its own
/// model rather than RGBSDA, because its samples are blocks and not channels.
const COLOR_MODEL_ASTC: u8 = 162;

/// `colorPrimaries` `KHR_DF_PRIMARIES_BT709` — the Rec. 709 primaries, which
/// are sRGB's. Every payload in this pipeline is authored against a display
/// that uses them.
const COLOR_PRIMARIES_BT709: u8 = 1;

/// `transferFunction` `KHR_DF_TRANSFER_LINEAR`.
const TRANSFER_LINEAR: u8 = 1;

/// `transferFunction` `KHR_DF_TRANSFER_SRGB`.
const TRANSFER_SRGB: u8 = 2;

/// `flags` with `KHR_DF_FLAG_ALPHA_STRAIGHT` — colour channels are not scaled
/// by alpha. Nothing in this pipeline premultiplies before the painter.
const FLAGS_STRAIGHT_ALPHA: u8 = 0;

/// The `KHR_DF_SAMPLE_DATATYPE_LINEAR` qualifier, which says one sample ignores
/// the format's transfer function.
const QUALIFIER_LINEAR: u8 = 1;

/// `channelType` values within `KHR_DF_MODEL_RGBSDA`. Alpha is 15 rather than
/// 3: the model numbers depth and stencil in between.
const CHANNEL_RED: u8 = 0;
const CHANNEL_GREEN: u8 = 1;
const CHANNEL_BLUE: u8 = 2;
const CHANNEL_ALPHA: u8 = 15;

/// The only `channelType` within `KHR_DF_MODEL_ASTC`: the block data itself.
const CHANNEL_ASTC_DATA: u8 = 0;

/// `VK_FORMAT_R8G8B8A8_UNORM`.
const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;

/// `VK_FORMAT_R8G8B8A8_SRGB`.
const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;

/// Every 2D LDR ASTC footprint the format defines, as `(block width, block
/// height, VK_FORMAT_ASTC_<w>x<h>_UNORM_BLOCK, VK_FORMAT_ASTC_<w>x<h>_SRGB_BLOCK)`.
///
/// [`BlockSize`] deliberately keeps no such list — an illegal footprint is
/// astcenc's to name, and a second copy of the legal set would go stale. Here
/// the list is unavoidable and is not a copy of astcenc's: `VkFormat` is the
/// container's own enumeration, and astcenc has no idea what a `VkFormat` is.
/// A footprint astcenc will encode but this table does not name comes back as
/// [`Ktx2Error::UnsupportedBlock`], which is a capability gap rather than a
/// wrongly labelled file.
///
/// Both format values are written out per row. The pairs happen to be adjacent
/// in the Vulkan enumeration, and deriving one from the other by adding 1 would
/// turn that coincidence into a rule.
const ASTC_FOOTPRINTS: [(u32, u32, u32, u32); 14] = [
    (4, 4, 157, 158),
    (5, 4, 159, 160),
    (5, 5, 161, 162),
    (6, 5, 163, 164),
    (6, 6, 165, 166),
    (8, 5, 167, 168),
    (8, 6, 169, 170),
    (8, 8, 171, 172),
    (10, 5, 173, 174),
    (10, 6, 175, 176),
    (10, 8, 177, 178),
    (10, 10, 179, 180),
    (12, 10, 181, 182),
    (12, 12, 183, 184),
];

/// What one file holds, one level of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Uncompressed 8-bit RGBA, four bytes per texel, rows top to bottom with
    /// no padding between them — [`astc::Rgba8`]'s layout.
    Rgba8 {
        /// How the stored channel values are to be read back.
        color: ColorSpace,
    },
    /// ASTC LDR at one of the footprints in `ASTC_FOOTPRINTS`, 16 bytes per
    /// block, blocks in raster order — what [`astc::encode`] returns.
    Astc {
        /// The block footprint the payload was encoded at.
        block: BlockSize,
        /// The profile the payload was encoded under.
        color: ColorSpace,
    },
}

/// The texel block a format stores, which is what both the payload length and
/// the data format descriptor are expressed in.
///
/// An uncompressed format has a 1x1 block, so one formula covers both cases and
/// the descriptor reads its dimensions off the same value the length used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TexelBlock {
    /// Block width in texels. Never zero.
    width: u32,
    /// Block height in texels. Never zero.
    height: u32,
    /// Bytes one block occupies — the descriptor's `bytesPlane0`.
    bytes: u8,
}

impl Format {
    /// The `VkFormat` value that names this format.
    ///
    /// This is also the gate on the ASTC footprint: a footprint with no
    /// `VkFormat` has no legal representation in the container, so it is
    /// refused here rather than written under a nearby format's name.
    pub fn vk_format(self) -> Result<u32, Ktx2Error> {
        match self {
            Self::Rgba8 { color } => Ok(match color {
                ColorSpace::Linear => VK_FORMAT_R8G8B8A8_UNORM,
                ColorSpace::Srgb => VK_FORMAT_R8G8B8A8_SRGB,
            }),
            Self::Astc { block, color } => {
                let &(_, _, unorm, srgb) = ASTC_FOOTPRINTS
                    .iter()
                    .find(|&&(x, y, _, _)| x == block.x && y == block.y)
                    .ok_or(Ktx2Error::UnsupportedBlock {
                        x: block.x,
                        y: block.y,
                    })?;
                Ok(match color {
                    ColorSpace::Linear => unorm,
                    ColorSpace::Srgb => srgb,
                })
            }
        }
    }

    /// The format a `VkFormat` value names, or `None` for any value outside the
    /// narrow set [`write()`] emits.
    ///
    /// The exact inverse of [`Format::vk_format`], and the reason the preview
    /// path can recover a payload's block footprint and colour space from the
    /// file rather than being told them by its caller
    /// (`crate::preview`). Being told is what a weld test cannot catch: a
    /// preview that decodes at the footprint it was handed agrees with the
    /// encoder even when the file says something else.
    ///
    /// `None` rather than an error, because a `VkFormat` this writer does not
    /// emit is not a malformed value — it is a legal format the preview path
    /// has no decoder for, and the caller names it in its own refusal.
    pub fn from_vk_format(vk_format: u32) -> Option<Self> {
        if vk_format == VK_FORMAT_R8G8B8A8_UNORM {
            return Some(Self::Rgba8 {
                color: ColorSpace::Linear,
            });
        }
        if vk_format == VK_FORMAT_R8G8B8A8_SRGB {
            return Some(Self::Rgba8 {
                color: ColorSpace::Srgb,
            });
        }
        ASTC_FOOTPRINTS.iter().find_map(|&(x, y, unorm, srgb)| {
            let color = if vk_format == unorm {
                ColorSpace::Linear
            } else if vk_format == srgb {
                ColorSpace::Srgb
            } else {
                return None;
            };
            Some(Self::Astc {
                block: BlockSize { x, y },
                color,
            })
        })
    }

    /// The exact payload length a `width` by `height` image occupies in this
    /// format, with partial blocks at the right and bottom edges counted as
    /// whole blocks.
    ///
    /// The footprint is checked first, so this never divides by a zero block
    /// dimension. The block-grid arithmetic itself is
    /// `astc::block_grid_bytes`, shared with [`BlockSize::payload_len`]
    /// (debt #452) — this format's own part is picking the texel block an
    /// uncompressed or an ASTC payload stores.
    pub fn payload_len(self, width: u32, height: u32) -> Result<usize, Ktx2Error> {
        self.vk_format()?;
        let block = self.texel_block();
        astc::block_grid_bytes(
            width,
            height,
            block.width,
            block.height,
            block.bytes as usize,
        )
        .ok_or(Ktx2Error::ImageTooLarge { width, height })
    }

    /// The texel block this format stores.
    ///
    /// For [`Format::Astc`] the footprint is returned as given, including one
    /// no `VkFormat` names; every caller passes through [`Format::vk_format`]
    /// first, which is where an unnamed footprint is refused.
    fn texel_block(self) -> TexelBlock {
        match self {
            Self::Rgba8 { .. } => TexelBlock {
                width: 1,
                height: 1,
                bytes: 4,
            },
            Self::Astc { block, .. } => TexelBlock {
                width: block.x,
                height: block.y,
                bytes: ASTC_BLOCK_BYTES,
            },
        }
    }

    /// The transfer function that reads the stored values back to light.
    fn transfer_function(self) -> u8 {
        let color = match self {
            Self::Rgba8 { color } | Self::Astc { color, .. } => color,
        };
        match color {
            ColorSpace::Linear => TRANSFER_LINEAR,
            ColorSpace::Srgb => TRANSFER_SRGB,
        }
    }

    /// The `colorModel` this format's samples belong to.
    fn color_model(self) -> u8 {
        match self {
            Self::Rgba8 { .. } => COLOR_MODEL_RGBSDA,
            Self::Astc { .. } => COLOR_MODEL_ASTC,
        }
    }

    /// The samples the descriptor lists for this format.
    fn samples(self) -> Vec<Sample> {
        match self {
            Self::Rgba8 { color } => [CHANNEL_RED, CHANNEL_GREEN, CHANNEL_BLUE, CHANNEL_ALPHA]
                .into_iter()
                .enumerate()
                .map(|(index, channel)| Sample {
                    bit_offset: index as u16 * 8,
                    bit_length: 8,
                    channel,
                    // Alpha is a coverage fraction, not a colour, so it stays
                    // linear even when the colour channels are sRGB-encoded.
                    // The transfer function is a property of the whole format,
                    // so the one channel it does not apply to has to say so.
                    qualifiers: if channel == CHANNEL_ALPHA && color == ColorSpace::Srgb {
                        QUALIFIER_LINEAR
                    } else {
                        0
                    },
                    // 0 reads back as 0.0 and 255 as 1.0: an unsigned
                    // normalised 8-bit channel.
                    lower: 0,
                    upper: 255,
                })
                .collect(),
            Self::Astc { .. } => vec![Sample {
                bit_offset: 0,
                // One 128-bit block is one sample. A block-compressed format
                // has no per-channel layout to describe, because the channels
                // only exist after the block is decoded.
                bit_length: 128,
                channel: CHANNEL_ASTC_DATA,
                qualifiers: 0,
                // The Khronos Data Format specification fixes these for a
                // block-compressed unsigned normalised format: the block is not
                // a number with a range, so the widest one is used.
                lower: 0,
                upper: u32::MAX,
            }],
        }
    }

    /// The complete data format descriptor for this format, `dfdTotalSize`
    /// first, as it is laid out in the file.
    fn data_format_descriptor(self) -> Vec<u8> {
        let block = self.texel_block();
        let samples = self.samples();

        // A basic descriptor block is an 8-byte block header, 16 bytes of fixed
        // fields, then 16 bytes per sample. The specification requires the size
        // to be a multiple of 4, which every value this produces is.
        let descriptor_block_size = 8 + 16 + 16 * samples.len();
        let total_size = 4 + descriptor_block_size;

        let mut out = Vec::with_capacity(total_size);
        out.extend_from_slice(&(total_size as u32).to_le_bytes());

        // Block header: vendorId in the low 17 bits and descriptorType in the
        // high 15. Both are 0 — Khronos, basic format descriptor.
        out.extend_from_slice(&0u32.to_le_bytes());
        // versionNumber 2 is the Khronos Data Format 1.3 encoding KTX2 uses.
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&(descriptor_block_size as u16).to_le_bytes());

        out.push(self.color_model());
        out.push(COLOR_PRIMARIES_BT709);
        out.push(self.transfer_function());
        out.push(FLAGS_STRAIGHT_ALPHA);

        // texelBlockDimension0..3, each stored as one less than the real
        // dimension so that a 1x1x1x1 block is four zeroes. Dimensions 2 and 3
        // are the 3D and array extents, which are 1 for a 2D image.
        out.push((block.width - 1) as u8);
        out.push((block.height - 1) as u8);
        out.push(0);
        out.push(0);

        // bytesPlane0..7. The image is one plane, so the rest are zero.
        //
        // These carry the size before supercompression, not after. Revisions of
        // the specification before the current one required all eight to be 0
        // when a supercompression scheme was set; the current text requires the
        // pre-deflation values to be preserved, which is what this writes.
        out.push(block.bytes);
        out.extend_from_slice(&[0u8; 7]);

        for sample in samples {
            out.extend_from_slice(&sample.bit_offset.to_le_bytes());
            // bitLength is stored as one less than the real length, which is
            // why a zero-length sample cannot be expressed and 128 fits a byte.
            out.push((sample.bit_length - 1) as u8);
            out.push(sample.channel | (sample.qualifiers << 4));
            // samplePosition0..3: where this sample sits inside the texel
            // block. Every sample here covers the whole block, so all zero.
            out.extend_from_slice(&[0u8; 4]);
            out.extend_from_slice(&sample.lower.to_le_bytes());
            out.extend_from_slice(&sample.upper.to_le_bytes());
        }

        debug_assert_eq!(out.len(), total_size);
        out
    }
}

/// What one ASTC block occupies, as the descriptor's `bytesPlane0` — a single
/// byte, which is why the width of [`astc::BLOCK_BYTES`] cannot simply be cast.
///
/// The cast is checked at compile time rather than trusted. 16 is fixed by the
/// ASTC format for every footprint, so this can only ever fire if that constant
/// is redefined, and then it fails the build instead of silently truncating the
/// descriptor field to a wrong block size.
const ASTC_BLOCK_BYTES: u8 = {
    assert!(astc::BLOCK_BYTES <= u8::MAX as usize);
    astc::BLOCK_BYTES as u8
};

/// One entry of a basic descriptor block's sample list: which bits of a texel
/// block hold which channel, and how to read their values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    /// Where this sample starts inside the texel block, in bits.
    bit_offset: u16,
    /// How many bits it occupies. At least 1 — the file stores this less one.
    bit_length: u16,
    /// The channel it carries, numbered within the format's `colorModel`.
    channel: u8,
    /// Qualifiers that change how the value is read, four bits wide.
    qualifiers: u8,
    /// The stored value that means the format's logical minimum.
    lower: u32,
    /// The stored value that means the format's logical maximum.
    upper: u32,
}

/// Why a file could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ktx2Error {
    /// An image dimension was zero. The container requires `pixelWidth` and
    /// `pixelHeight` to be non-zero for a 2D texture.
    ZeroDimension { width: u32, height: u32 },
    /// The ASTC footprint has no `VkFormat`, so no legal file can name it.
    UnsupportedBlock { x: u32, y: u32 },
    /// The image is large enough that its payload length does not fit in a
    /// `usize`. Reported rather than allowed to wrap, because a wrapped length
    /// is smaller than the true one and would compare equal to a short payload.
    ImageTooLarge { width: u32, height: u32 },
    /// The payload was not the length the format and dimensions require.
    /// Writing it would produce a file whose header describes an image its
    /// level data does not hold.
    PayloadLen { expected: usize, found: usize },
    /// libzstd refused to compress the level. `message` is its own text.
    Compress { message: String },
}

impl std::fmt::Display for Ktx2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDimension { width, height } => {
                write!(f, "an image dimension is zero: {width}x{height}")
            }
            Self::UnsupportedBlock { x, y } => write!(
                f,
                "no VkFormat names the {x}x{y} ASTC footprint, so it cannot be written to KTX2"
            ),
            Self::ImageTooLarge { width, height } => write!(
                f,
                "a {width}x{height} image needs more bytes than a usize can count"
            ),
            Self::PayloadLen { expected, found } => write!(
                f,
                "the payload holds {found} bytes, but this format and size need exactly {expected}"
            ),
            Self::Compress { message } => {
                write!(f, "zstd refused to compress the level: {message}")
            }
        }
    }
}

impl std::error::Error for Ktx2Error {}

/// Writes one image as a complete KTX2 file.
///
/// `payload` must be exactly [`Format::payload_len`] bytes — the texels of an
/// uncompressed image, or the blocks of an encoded one. A payload of any other
/// length is refused rather than padded or truncated, because either would
/// produce a file whose header and level data disagree.
///
/// The level is Zstd-compressed at [`ZSTD_LEVEL`]; the returned bytes are the
/// whole file, ready to write to disk.
pub fn write(
    payload: &[u8],
    width: u32,
    height: u32,
    format: Format,
) -> Result<Vec<u8>, Ktx2Error> {
    if width == 0 || height == 0 {
        return Err(Ktx2Error::ZeroDimension { width, height });
    }
    // Before anything else, so that an unnamed footprint is refused rather than
    // reaching the block arithmetic below.
    let vk_format = format.vk_format()?;

    let expected = format.payload_len(width, height)?;
    if payload.len() != expected {
        return Err(Ktx2Error::PayloadLen {
            expected,
            found: payload.len(),
        });
    }

    let descriptor = format.data_format_descriptor();
    let key_values = key_value_data();
    let level = compress(payload)?;

    // The four sections sit end to end. The header and the single level index
    // entry are fixed-size, and a descriptor block's size is a multiple of 4,
    // so the descriptor and the key/value data both start on the 4-byte
    // boundary the specification requires without any padding between them.
    // The specification checks exactly that by requiring
    // `dfdTotalSize == kvdByteOffset - dfdByteOffset`.
    let descriptor_offset = HEADER_BYTES + LEVEL_INDEX_ENTRY_BYTES;
    let key_values_offset = descriptor_offset + descriptor.len();
    let level_offset = key_values_offset + key_values.len();
    debug_assert_eq!(descriptor_offset % 4, 0);
    debug_assert_eq!(key_values_offset % 4, 0);

    let mut out = Vec::with_capacity(level_offset + level.len());
    out.extend_from_slice(&IDENTIFIER);
    out.extend_from_slice(&vk_format.to_le_bytes());
    out.extend_from_slice(&TYPE_SIZE.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    // A 2D texture has a non-zero pixelHeight and a zero pixelDepth; a zero
    // layerCount means it is not an array, and one face means it is not a
    // cubemap. The specification's texture-type table calls any other
    // combination of these five invalid.
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    // One level, stated. A levelCount of 0 asks the consumer to generate the
    // rest of the mip chain, which is not what this writer means: it emits the
    // base level and nothing is missing. The specification also forbids 0
    // outright for block-compressed formats, which every ASTC format here is.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&SUPERCOMPRESSION_ZSTD.to_le_bytes());

    // Index: the offset and length of each of the three metadata sections.
    out.extend_from_slice(&(descriptor_offset as u32).to_le_bytes());
    out.extend_from_slice(&(descriptor.len() as u32).to_le_bytes());
    out.extend_from_slice(&(key_values_offset as u32).to_le_bytes());
    out.extend_from_slice(&(key_values.len() as u32).to_le_bytes());
    // Supercompression global data is BasisLZ's codebooks. Zstd has none, and
    // the specification requires the offset to be 0 when the length is.
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());

    // Level index. `uncompressedByteLength` is what the level inflates to,
    // which is the payload the caller handed in.
    out.extend_from_slice(&(level_offset as u64).to_le_bytes());
    out.extend_from_slice(&(level.len() as u64).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());

    out.extend_from_slice(&descriptor);
    out.extend_from_slice(&key_values);
    // No mipPadding: the specification requires it only when the
    // supercompression scheme is 0, and this writer always sets Zstd.
    out.extend_from_slice(&level);

    debug_assert_eq!(out.len(), level_offset + level.len());
    Ok(out)
}

/// The key/value data section: the writer's identity and the image orientation.
///
/// Keys are written sorted by Unicode code point, which the specification
/// requires. `KTXorientation` sorts before `KTXwriter` because `o` precedes `w`.
fn key_value_data() -> Vec<u8> {
    let mut out = Vec::new();
    push_key_value(&mut out, "KTXorientation", ORIENTATION);
    push_key_value(&mut out, "KTXwriter", WRITER);
    out
}

/// Appends one key/value pair, with the NUL terminators and the padding the
/// specification requires.
///
/// Both the key and a string value are NUL-terminated, and both terminators
/// count towards `keyAndValueByteLength`; the padding that follows does not,
/// but it is part of `kvdByteLength`. The padding is what keeps every
/// `keyAndValueByteLength` field 4-byte aligned, and the section itself starts
/// on a 4-byte boundary, so aligning within the section aligns within the file.
fn push_key_value(out: &mut Vec<u8>, key: &str, value: &str) {
    let key_and_value_byte_length = key.len() + 1 + value.len() + 1;
    out.extend_from_slice(&(key_and_value_byte_length as u32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.push(0);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Compresses one level, reporting libzstd's own refusal rather than a reworded
/// one.
fn compress(payload: &[u8]) -> Result<Vec<u8>, Ktx2Error> {
    zstd::bulk::compress(payload, ZSTD_LEVEL).map_err(|error| Ktx2Error::Compress {
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astc::Quality;

    /// A deterministic image with variation in every channel, so that a
    /// compressor cannot collapse it to a constant and a byte comparison is
    /// actually comparing something.
    fn test_texels(width: u32, height: u32) -> Vec<u8> {
        let mut texels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                texels.push((x * 7 % 256) as u8);
                texels.push((y * 11 % 256) as u8);
                texels.push(((x + y) * 3 % 256) as u8);
                texels.push(((x * y) % 256) as u8);
            }
        }
        texels
    }

    /// Reads a file back with the independent reader, failing the test if it
    /// will not parse at all.
    fn read(file: &[u8]) -> ktx2::Reader<&[u8]> {
        ktx2::Reader::new(file).expect("the independent reader must accept what this writer emits")
    }

    /// The inflated level 0 payload.
    fn level_payload(reader: &ktx2::Reader<&[u8]>) -> Vec<u8> {
        let level = reader.levels().next().expect("there is exactly one level");
        let inflated = zstd::bulk::decompress(level.data, level.uncompressed_byte_length as usize)
            .expect("the level must inflate");
        assert_eq!(
            inflated.len() as u64,
            level.uncompressed_byte_length,
            "the recorded uncompressed length must be the length the level inflates to"
        );
        inflated
    }

    #[test]
    fn the_file_begins_with_the_ktx2_identifier() {
        let texels = test_texels(4, 4);
        let file = write(
            &texels,
            4,
            4,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        assert_eq!(&file[..12], &ktx2::MAGIC);
    }

    #[test]
    fn an_uncompressed_image_round_trips_every_header_field() {
        let texels = test_texels(19, 7);
        let file = write(
            &texels,
            19,
            7,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let header = reader.header();

        assert_eq!(header.format, Some(ktx2::Format::R8G8B8A8_SRGB));
        assert_eq!(header.type_size, 1);
        assert_eq!(header.pixel_width, 19);
        assert_eq!(header.pixel_height, 7);
        assert_eq!(header.pixel_depth, 0, "a 2D texture has no depth");
        assert_eq!(header.layer_count, 0, "a 2D texture is not an array");
        assert_eq!(header.face_count, 1, "a 2D texture is not a cubemap");
        assert_eq!(header.level_count, 1, "this writer emits one level");
        assert_eq!(
            header.supercompression_scheme,
            Some(ktx2::SupercompressionScheme::Zstandard)
        );
        assert_eq!(reader.levels().len(), 1);
    }

    #[test]
    fn a_linear_uncompressed_image_is_written_as_the_unorm_format() {
        let texels = test_texels(4, 4);
        let file = write(
            &texels,
            4,
            4,
            Format::Rgba8 {
                color: ColorSpace::Linear,
            },
        )
        .unwrap();
        let reader = read(&file);
        assert_eq!(reader.header().format, Some(ktx2::Format::R8G8B8A8_UNORM));
        assert_eq!(
            reader.transfer_function(),
            Some(ktx2::TransferFunction::Linear)
        );
    }

    #[test]
    fn the_level_payload_round_trips_through_zstd() {
        let texels = test_texels(37, 23);
        let file = write(
            &texels,
            37,
            23,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        assert_eq!(level_payload(&reader), texels);
    }

    #[test]
    fn the_level_is_actually_compressed_rather_than_stored() {
        // A constant image compresses to far less than its texel count, so a
        // writer that forgot to compress and only labelled the scheme would
        // fail this.
        let texels = vec![0x5Au8; 256 * 256 * 4];
        let file = write(
            &texels,
            256,
            256,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let level = reader.levels().next().unwrap();
        assert!(
            level.data.len() < texels.len() / 100,
            "a constant image must not be stored uncompressed: {} bytes for {}",
            level.data.len(),
            texels.len()
        );
        assert_eq!(level_payload(&reader), texels);
    }

    #[test]
    fn an_astc_payload_round_trips_through_the_container() {
        let width = 61;
        let height = 29;
        let block = BlockSize::ASTC_4X4;
        let texels = test_texels(width, height);
        let image = astc::Rgba8::new(width, height, &texels).unwrap();
        let blocks = astc::encode(image, block, ColorSpace::Srgb, Quality::Fastest).unwrap();

        let file = write(
            &blocks,
            width,
            height,
            Format::Astc {
                block,
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let header = reader.header();
        assert_eq!(header.format, Some(ktx2::Format::ASTC_4x4_SRGB_BLOCK));
        assert_eq!(header.pixel_width, width);
        assert_eq!(header.pixel_height, height);
        assert_eq!(
            level_payload(&reader),
            blocks,
            "the blocks must come back byte for byte, so that the reference decoder sees exactly what the encoder produced"
        );
    }

    #[test]
    fn every_astc_footprint_is_written_under_its_own_vk_format() {
        // Written out rather than derived from `ASTC_FOOTPRINTS`, so that a
        // wrong entry in that table is a failure here rather than a shared
        // mistake.
        let expected = [
            (
                4,
                4,
                ktx2::Format::ASTC_4x4_UNORM_BLOCK,
                ktx2::Format::ASTC_4x4_SRGB_BLOCK,
            ),
            (
                5,
                4,
                ktx2::Format::ASTC_5x4_UNORM_BLOCK,
                ktx2::Format::ASTC_5x4_SRGB_BLOCK,
            ),
            (
                5,
                5,
                ktx2::Format::ASTC_5x5_UNORM_BLOCK,
                ktx2::Format::ASTC_5x5_SRGB_BLOCK,
            ),
            (
                6,
                5,
                ktx2::Format::ASTC_6x5_UNORM_BLOCK,
                ktx2::Format::ASTC_6x5_SRGB_BLOCK,
            ),
            (
                6,
                6,
                ktx2::Format::ASTC_6x6_UNORM_BLOCK,
                ktx2::Format::ASTC_6x6_SRGB_BLOCK,
            ),
            (
                8,
                5,
                ktx2::Format::ASTC_8x5_UNORM_BLOCK,
                ktx2::Format::ASTC_8x5_SRGB_BLOCK,
            ),
            (
                8,
                6,
                ktx2::Format::ASTC_8x6_UNORM_BLOCK,
                ktx2::Format::ASTC_8x6_SRGB_BLOCK,
            ),
            (
                8,
                8,
                ktx2::Format::ASTC_8x8_UNORM_BLOCK,
                ktx2::Format::ASTC_8x8_SRGB_BLOCK,
            ),
            (
                10,
                5,
                ktx2::Format::ASTC_10x5_UNORM_BLOCK,
                ktx2::Format::ASTC_10x5_SRGB_BLOCK,
            ),
            (
                10,
                6,
                ktx2::Format::ASTC_10x6_UNORM_BLOCK,
                ktx2::Format::ASTC_10x6_SRGB_BLOCK,
            ),
            (
                10,
                8,
                ktx2::Format::ASTC_10x8_UNORM_BLOCK,
                ktx2::Format::ASTC_10x8_SRGB_BLOCK,
            ),
            (
                10,
                10,
                ktx2::Format::ASTC_10x10_UNORM_BLOCK,
                ktx2::Format::ASTC_10x10_SRGB_BLOCK,
            ),
            (
                12,
                10,
                ktx2::Format::ASTC_12x10_UNORM_BLOCK,
                ktx2::Format::ASTC_12x10_SRGB_BLOCK,
            ),
            (
                12,
                12,
                ktx2::Format::ASTC_12x12_UNORM_BLOCK,
                ktx2::Format::ASTC_12x12_SRGB_BLOCK,
            ),
        ];
        assert_eq!(
            expected.len(),
            ASTC_FOOTPRINTS.len(),
            "every footprint the writer names must be checked here"
        );

        for (x, y, unorm, srgb) in expected {
            let block = BlockSize { x, y };
            for (color, want) in [(ColorSpace::Linear, unorm), (ColorSpace::Srgb, srgb)] {
                let format = Format::Astc { block, color };
                let payload = vec![0u8; format.payload_len(24, 24).unwrap()];
                let file = write(&payload, 24, 24, format).unwrap();
                assert_eq!(
                    read(&file).header().format,
                    Some(want),
                    "{x}x{y} {color:?} was written under the wrong VkFormat"
                );
            }
        }
    }

    #[test]
    fn every_named_footprint_is_one_the_encoder_will_actually_produce() {
        // The container's footprint table and astcenc's legal set are separate
        // lists that must not drift apart. This walks the container's list and
        // makes the encoder agree with each entry.
        let texels = test_texels(16, 16);
        for (x, y, _, _) in ASTC_FOOTPRINTS {
            let block = BlockSize { x, y };
            let image = astc::Rgba8::new(16, 16, &texels).unwrap();
            let blocks = astc::encode(image, block, ColorSpace::Srgb, Quality::Fastest)
                .unwrap_or_else(|error| {
                    panic!("the writer names the {x}x{y} footprint but astcenc refuses it: {error}")
                });
            assert_eq!(
                blocks.len(),
                Format::Astc {
                    block,
                    color: ColorSpace::Srgb
                }
                .payload_len(16, 16)
                .unwrap(),
                "the container and the codec disagree on the payload length for {x}x{y}"
            );
        }
    }

    #[test]
    fn the_descriptor_for_an_uncompressed_image_matches_the_independent_generator() {
        for (color, format) in [
            (ColorSpace::Linear, ktx2::Format::R8G8B8A8_UNORM),
            (ColorSpace::Srgb, ktx2::Format::R8G8B8A8_SRGB),
        ] {
            let texels = test_texels(8, 8);
            let file = write(&texels, 8, 8, Format::Rgba8 { color }).unwrap();
            let reader = read(&file);
            let (expected, type_size) = ktx2::dfd::Basic::from_format(format).unwrap();
            assert_eq!(reader.basic_dfd(), Some(&expected), "{color:?}");
            assert_eq!(reader.header().type_size, type_size, "{color:?}");
        }
    }

    #[test]
    fn the_descriptor_for_an_astc_image_matches_the_independent_generator() {
        for (color, format) in [
            (ColorSpace::Linear, ktx2::Format::ASTC_6x5_UNORM_BLOCK),
            (ColorSpace::Srgb, ktx2::Format::ASTC_6x5_SRGB_BLOCK),
        ] {
            let block = BlockSize { x: 6, y: 5 };
            let dashpack_format = Format::Astc { block, color };
            let payload = vec![0u8; dashpack_format.payload_len(18, 15).unwrap()];
            let file = write(&payload, 18, 15, dashpack_format).unwrap();
            let reader = read(&file);
            let (expected, type_size) = ktx2::dfd::Basic::from_format(format).unwrap();
            assert_eq!(reader.basic_dfd(), Some(&expected), "{color:?}");
            assert_eq!(reader.header().type_size, type_size, "{color:?}");
        }
    }

    #[test]
    fn the_descriptor_is_the_only_block_and_its_length_matches_the_index() {
        let texels = test_texels(4, 4);
        let file = write(
            &texels,
            4,
            4,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        assert_eq!(reader.dfd_blocks().len(), 1);

        let index = reader.header().index;
        // The specification calls a file invalid unless the descriptor's own
        // declared size spans exactly the gap to the key/value data.
        let declared_total_size = u32::from_le_bytes(
            file[index.dfd_byte_offset as usize..][..4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(declared_total_size, index.dfd_byte_length);
        assert_eq!(
            declared_total_size,
            index.kvd_byte_offset - index.dfd_byte_offset
        );
    }

    #[test]
    fn the_key_value_data_records_the_writer_and_the_orientation() {
        let texels = test_texels(4, 4);
        let file = write(
            &texels,
            4,
            4,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let pairs: Vec<_> = reader.key_value_data().collect();

        assert_eq!(
            pairs,
            vec![
                ("KTXorientation", b"rd\0".as_slice()),
                ("KTXwriter", format!("{WRITER}\0").as_bytes()),
            ]
        );
        // The literal, spelled out rather than rebuilt from the constant: a
        // change to `WRITER` moves every emitted file and must be a decision
        // someone made, not one a `concat!` made for them.
        assert_eq!(reader.writer(), Some("dashpack gen1 astcenc 5.6.0\0"));
        assert!(
            pairs[0].0 < pairs[1].0,
            "the specification requires keys sorted by code point"
        );
    }

    #[test]
    fn the_writer_does_not_carry_the_crate_version() {
        // The guard on the regression story #434 recorded: while `WRITER`
        // carried `CARGO_PKG_VERSION`, a `git std bump` moved every emitted
        // byte, so a routine release and a real encoder regression produced one
        // signal — and every texture in a shipped cold bank changed on every
        // release, for an OTA delta to carry for nothing.
        //
        // The whole string is asserted above. This says why it is that string,
        // so a change back to the crate version fails with the reason attached
        // rather than only as a moved golden.
        assert!(
            !WRITER.contains(env!("CARGO_PKG_VERSION")),
            "WRITER is {WRITER}, which carries the crate version {}. The emitted \
             bytes must not depend on the release cadence — see the WRITER doc \
             comment. If the crate version has merely grown into a substring of \
             the encoder pin, widen this check rather than the string.",
            env!("CARGO_PKG_VERSION"),
        );
    }

    #[test]
    fn the_key_value_section_length_satisfies_the_specification_sum() {
        let texels = test_texels(4, 4);
        let file = write(
            &texels,
            4,
            4,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let index = read(&file).header().index;

        // The specification calls a file invalid unless the section length is
        // the sum, over the pairs, of the length field plus its data rounded up
        // to 4. This recomputes it from the pairs rather than from the writer's
        // own arithmetic.
        let pairs = [
            "KTXorientation".len() + 1 + ORIENTATION.len() + 1,
            "KTXwriter".len() + 1 + WRITER.len() + 1,
        ];
        let expected: usize = pairs.iter().map(|length| length.div_ceil(4) * 4 + 4).sum();
        assert_eq!(index.kvd_byte_length as usize, expected);

        // The specification's own worked example writes the same
        // `KTXorientation` pair and gives its `keyAndValueByteLength` as 18.
        // Pinned as a literal so that this one entry is checked against the
        // specification's arithmetic rather than against the writer's.
        assert_eq!(pairs[0], 18);
        let recorded = u32::from_le_bytes(
            file[index.kvd_byte_offset as usize..][..4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(recorded, 18);
    }

    #[test]
    fn the_sections_sit_end_to_end_and_cover_the_whole_file() {
        let texels = test_texels(13, 11);
        let file = write(
            &texels,
            13,
            11,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let index = reader.header().index;

        assert_eq!(
            index.dfd_byte_offset as usize,
            HEADER_BYTES + LEVEL_INDEX_ENTRY_BYTES
        );
        assert_eq!(index.dfd_byte_offset % 4, 0);
        assert_eq!(index.kvd_byte_offset % 4, 0);
        assert_eq!(
            index.kvd_byte_offset,
            index.dfd_byte_offset + index.dfd_byte_length
        );
        // No supercompression global data, and the specification requires the
        // offset to be zero when the length is.
        assert_eq!(index.sgd_byte_length, 0);
        assert_eq!(index.sgd_byte_offset, 0);

        let level = reader.levels().next().unwrap();
        let level_offset = u64::from(index.kvd_byte_offset + index.kvd_byte_length);
        let recorded_offset = u64::from_le_bytes(file[HEADER_BYTES..][..8].try_into().unwrap());
        assert_eq!(
            recorded_offset, level_offset,
            "the level starts immediately after the key/value data: a Zstd file has no mipPadding"
        );
        assert_eq!(file.len() as u64, level_offset + level.data.len() as u64);
    }

    #[test]
    fn the_level_is_compressed_at_the_level_the_constant_names() {
        // Not a check that 19 is the right level — a compression level has no
        // correct value. It checks that the writer used the level it documents,
        // so that `ZSTD_LEVEL` is the single place the emitted bytes are
        // decided from.
        let texels = test_texels(23, 29);
        let file = write(
            &texels,
            23,
            29,
            Format::Rgba8 {
                color: ColorSpace::Srgb,
            },
        )
        .unwrap();
        let reader = read(&file);
        let level = reader.levels().next().unwrap();
        assert_eq!(
            level.data,
            zstd::bulk::compress(&texels, ZSTD_LEVEL).unwrap()
        );
    }

    #[test]
    fn writing_the_same_image_twice_produces_the_same_bytes() {
        let texels = test_texels(31, 17);
        let format = Format::Rgba8 {
            color: ColorSpace::Srgb,
        };
        let first = write(&texels, 31, 17, format).unwrap();
        let second = write(&texels, 31, 17, format).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_partial_block_at_the_edge_counts_as_a_whole_block() {
        let block = BlockSize::ASTC_4X4;
        let format = Format::Astc {
            block,
            color: ColorSpace::Srgb,
        };
        // 7x5 texels is two block columns and two block rows.
        assert_eq!(format.payload_len(7, 5).unwrap(), 4 * astc::BLOCK_BYTES);
        // And the container agrees with the codec module, which computes it
        // independently.
        assert_eq!(
            format.payload_len(7, 5).unwrap(),
            block.payload_len(7, 5).unwrap()
        );
    }

    #[test]
    fn the_uncompressed_payload_length_is_four_bytes_a_texel() {
        let format = Format::Rgba8 {
            color: ColorSpace::Linear,
        };
        assert_eq!(format.payload_len(19, 7).unwrap(), 19 * 7 * 4);
    }

    #[test]
    fn a_payload_of_the_wrong_length_is_refused() {
        let format = Format::Rgba8 {
            color: ColorSpace::Srgb,
        };
        let short = vec![0u8; 4 * 4 * 4 - 1];
        assert_eq!(
            write(&short, 4, 4, format),
            Err(Ktx2Error::PayloadLen {
                expected: 64,
                found: 63
            })
        );
        let long = vec![0u8; 4 * 4 * 4 + 1];
        assert_eq!(
            write(&long, 4, 4, format),
            Err(Ktx2Error::PayloadLen {
                expected: 64,
                found: 65
            })
        );
    }

    #[test]
    fn a_zero_dimension_is_refused() {
        let format = Format::Rgba8 {
            color: ColorSpace::Srgb,
        };
        assert_eq!(
            write(&[], 0, 4, format),
            Err(Ktx2Error::ZeroDimension {
                width: 0,
                height: 4
            })
        );
        assert_eq!(
            write(&[], 4, 0, format),
            Err(Ktx2Error::ZeroDimension {
                width: 4,
                height: 0
            })
        );
    }

    #[test]
    fn a_footprint_no_vk_format_names_is_refused() {
        // 3x3 is a legal ASTC 3D footprint but not a 2D one, so no 2D VkFormat
        // names it.
        let format = Format::Astc {
            block: BlockSize { x: 3, y: 3 },
            color: ColorSpace::Srgb,
        };
        assert_eq!(
            format.vk_format(),
            Err(Ktx2Error::UnsupportedBlock { x: 3, y: 3 })
        );
        assert_eq!(
            write(&[0u8; 16], 3, 3, format),
            Err(Ktx2Error::UnsupportedBlock { x: 3, y: 3 })
        );
    }

    #[test]
    fn a_zero_footprint_is_refused_before_it_is_divided_by() {
        let format = Format::Astc {
            block: BlockSize { x: 0, y: 0 },
            color: ColorSpace::Srgb,
        };
        assert_eq!(
            format.payload_len(4, 4),
            Err(Ktx2Error::UnsupportedBlock { x: 0, y: 0 })
        );
    }

    #[test]
    fn an_image_too_large_to_measure_is_refused_rather_than_wrapped() {
        let format = Format::Rgba8 {
            color: ColorSpace::Linear,
        };
        // 2^31 by 2^31 texels at four bytes each is 2^64 bytes, which wraps a
        // 64-bit usize to exactly zero.
        let side = 1u32 << 31;
        assert_eq!(
            format.payload_len(side, side),
            Err(Ktx2Error::ImageTooLarge {
                width: side,
                height: side
            })
        );
    }

    #[test]
    fn every_error_has_a_message_that_names_its_numbers() {
        // Each case carries values chosen to be distinctive, and every one of
        // them must appear in the rendered text. Asserting only that the
        // message is non-empty would pass even if a `Display` arm dropped a
        // field, which is the whole reason these errors carry numbers.
        let cases: [(Ktx2Error, &[&str]); 5] = [
            (
                Ktx2Error::ZeroDimension {
                    width: 0,
                    height: 7,
                },
                &["0", "7"],
            ),
            (Ktx2Error::UnsupportedBlock { x: 3, y: 5 }, &["3", "5"]),
            (
                Ktx2Error::ImageTooLarge {
                    width: 91,
                    height: 92,
                },
                &["91", "92"],
            ),
            (
                Ktx2Error::PayloadLen {
                    expected: 64,
                    found: 63,
                },
                &["64", "63"],
            ),
            (
                Ktx2Error::Compress {
                    message: "no space".to_owned(),
                },
                &["no space"],
            ),
        ];
        for (case, must_name) in cases {
            let rendered = case.to_string();
            for fragment in must_name {
                assert!(
                    rendered.contains(fragment),
                    "{case:?} renders as {rendered:?}, which does not name {fragment:?}"
                );
            }
        }
    }
}
