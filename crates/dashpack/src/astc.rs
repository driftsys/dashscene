//! ASTC encode and decode, through the vendored astcenc that
//! `dashpack-astcenc-sys` links (story #430).
//!
//! # One library, both directions
//!
//! [`encode`] and [`decode`] call the same pinned encoder. That is deliberate:
//! the reference decode has to be the encoder's own, in process, so that a
//! compressed profile can be previewed by the reference painter and a later
//! pure-Rust block decoder can be welded to this one by byte equality. Two
//! codec implementations would leave a difference no test could attribute.
//!
//! # Reproducibility
//!
//! astcenc is built invariant, so one revision produces bit-identical output
//! for every compiler and CPU architecture
//! (`crates/dashpack-astcenc-sys/vendor/VENDOR.md`). A bank derived on one
//! machine therefore equals a bank derived on another, which is what the
//! packer needs and what an encoder installed on `PATH` could not promise.
//!
//! # What this module does not do
//!
//! It does not choose a block size, a quality preset, or a color space. Those
//! are profile decisions, and a profile is a set of tolerance bands rather than
//! a format; the escalation that picks between them lands with the band oracle
//! (story #432). This module is the mechanism those decisions drive.
//!
//! Only 2D LDR images are reachable. The vendored library is built with
//! `ASTCENC_BLOCK_MAX_TEXELS=144`, so 12x12 is the largest block and 3D block
//! sizes return [`AstcError::Codec`] rather than being quietly substituted.

use std::ffi::CStr;

use dashpack_astcenc_sys as sys;

/// The number of bytes one ASTC block occupies, for every block size the format
/// defines.
pub const BLOCK_BYTES: usize = 16;

/// The astcenc release this binary links, and the upstream commit it was
/// vendored from.
///
/// The pin is what makes a bank auditable: two banks are comparable only if the
/// encoder that produced them is the same one. Reported by the `dashpack`
/// binary so the pin can be read off the artifact rather than off the source
/// tree.
pub fn vendored_astcenc() -> (&'static str, &'static str) {
    (sys::VENDORED_VERSION, sys::VENDORED_COMMIT)
}

/// An ASTC block footprint, in texels.
///
/// Not validated here. The legal footprints are the format's, astcenc already
/// knows them, and duplicating the list would create a second place for it to
/// go stale — an illegal footprint comes back as [`AstcError::Codec`] carrying
/// astcenc's own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockSize {
    pub x: u32,
    pub y: u32,
}

impl BlockSize {
    /// The highest-bitrate 2D footprint, 8 bits per texel.
    pub const ASTC_4X4: Self = Self { x: 4, y: 4 };

    /// The exact payload length for a `width` by `height` image at this
    /// footprint. Partial blocks at the right and bottom edges count as whole
    /// blocks, because that is what the format stores.
    ///
    /// Zero is refused here rather than deferred to astcenc, because it is the
    /// one illegal footprint this function cannot survive reaching: the block
    /// count is a division by the footprint. Every other illegal footprint is
    /// still astcenc's to name.
    pub fn payload_len(self, width: u32, height: u32) -> Result<usize, AstcError> {
        if self.x == 0 || self.y == 0 {
            return Err(AstcError::ZeroBlockDimension {
                x: self.x,
                y: self.y,
            });
        }
        let columns = width.div_ceil(self.x) as usize;
        let rows = height.div_ceil(self.y) as usize;
        columns
            .checked_mul(rows)
            .and_then(|blocks| blocks.checked_mul(BLOCK_BYTES))
            .ok_or(AstcError::ImageTooLarge { width, height })
    }
}

/// The length of an 8-bit RGBA buffer for a `width` by `height` image.
///
/// Checked rather than plain arithmetic. `width as usize * height as usize * 4`
/// overflows a 64-bit `usize` at `width = height = 2^31` — both of which are
/// legal `u32` values — and wraps to exactly zero. In a release build, where
/// overflow checks are off, that would size a buffer at zero while astcenc is
/// told the true dimensions and writes the full image through it. astcenc's own
/// overflow guards do not catch it, because the texel count it computes fits in
/// a `size_t` perfectly well; it is only the byte count that wraps. On a 32-bit
/// target the same wrap arrives at `width = height = 65536`, which is an
/// ordinary image size rather than an adversarial one.
fn texel_bytes(width: u32, height: u32) -> Result<usize, AstcError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|texels| texels.checked_mul(4))
        .ok_or(AstcError::ImageTooLarge { width, height })
}

/// How the codec interprets the 8-bit channel values it is given.
///
/// Both variants are LDR. HDR profiles exist in astcenc and are not offered
/// here: nothing in the pipeline produces HDR source, and an unreachable option
/// is one more thing a profile contract could be written against by mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    /// Values are linear. Use this for data that is not a displayed color —
    /// distance fields, masks, normal maps.
    Linear,
    /// RGB is sRGB-encoded and alpha is linear, which is how an imported image
    /// fill arrives.
    Srgb,
}

impl ColorSpace {
    fn profile(self) -> sys::astcenc_profile {
        match self {
            Self::Linear => sys::ASTCENC_PRF_LDR,
            Self::Srgb => sys::ASTCENC_PRF_LDR_SRGB,
        }
    }
}

/// How hard the encoder searches.
///
/// These are astcenc's own presets. Effort is not linear across them: the gap
/// from [`Quality::Thorough`] to [`Quality::Exhaustive`] costs far more time
/// than it recovers quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    Fastest,
    Fast,
    Medium,
    Thorough,
    VeryThorough,
    Exhaustive,
}

impl Quality {
    fn effort(self) -> f32 {
        match self {
            Self::Fastest => sys::ASTCENC_PRE_FASTEST,
            Self::Fast => sys::ASTCENC_PRE_FAST,
            Self::Medium => sys::ASTCENC_PRE_MEDIUM,
            Self::Thorough => sys::ASTCENC_PRE_THOROUGH,
            Self::VeryThorough => sys::ASTCENC_PRE_VERYTHOROUGH,
            Self::Exhaustive => sys::ASTCENC_PRE_EXHAUSTIVE,
        }
    }
}

/// A borrowed uncompressed image, 8 bits per channel, four channels per texel,
/// rows top to bottom with no padding between them.
#[derive(Debug, Clone, Copy)]
pub struct Rgba8<'a> {
    width: u32,
    height: u32,
    texels: &'a [u8],
}

impl<'a> Rgba8<'a> {
    /// Borrows `texels` as a `width` by `height` image.
    ///
    /// Refuses a zero dimension and refuses a buffer whose length is not
    /// exactly `width * height * 4`. Both would otherwise reach astcenc as a
    /// pointer and a pair of dimensions it has no way to check.
    pub fn new(width: u32, height: u32, texels: &'a [u8]) -> Result<Self, AstcError> {
        if width == 0 || height == 0 {
            return Err(AstcError::ZeroDimension { width, height });
        }
        let expected = texel_bytes(width, height)?;
        if texels.len() != expected {
            return Err(AstcError::TexelCount {
                expected,
                found: texels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            texels,
        })
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }

    pub fn texels(self) -> &'a [u8] {
        self.texels
    }
}

/// Why an encode or decode could not be performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstcError {
    /// astcenc refused the request. `message` is astcenc's own text for
    /// `status`, so a codec-level refusal is never reworded on the way out.
    Codec { status: u32, message: String },
    /// An image dimension was zero.
    ZeroDimension { width: u32, height: u32 },
    /// A block footprint dimension was zero, which has no block count.
    ZeroBlockDimension { x: u32, y: u32 },
    /// The image is large enough that its buffer length does not fit in a
    /// `usize`. Reported rather than allowed to wrap, because a wrapped length
    /// is smaller than the true one and would size a buffer short.
    ImageTooLarge { width: u32, height: u32 },
    /// An uncompressed buffer was not `width * height * 4` bytes long.
    TexelCount { expected: usize, found: usize },
    /// A compressed payload was not the length the block grid needs. Decoding
    /// it would either read past the end or silently ignore trailing blocks.
    PayloadLen { expected: usize, found: usize },
}

impl std::fmt::Display for AstcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec { status, message } => {
                write!(
                    f,
                    "astcenc refused the request: {message} (status {status})"
                )
            }
            Self::ZeroDimension { width, height } => {
                write!(f, "an image dimension is zero: {width}x{height}")
            }
            Self::ZeroBlockDimension { x, y } => {
                write!(f, "a block footprint dimension is zero: {x}x{y}")
            }
            Self::ImageTooLarge { width, height } => write!(
                f,
                "a {width}x{height} image needs more bytes than a usize can count"
            ),
            Self::TexelCount { expected, found } => write!(
                f,
                "the image buffer holds {found} bytes, but {expected} bytes are needed for 8-bit RGBA"
            ),
            Self::PayloadLen { expected, found } => write!(
                f,
                "the compressed payload holds {found} bytes, but the block grid needs exactly {expected}"
            ),
        }
    }
}

impl std::error::Error for AstcError {}

impl AstcError {
    /// Turns a non-success astcenc status into an error carrying astcenc's own
    /// description of it.
    fn from_status(status: sys::astcenc_error) -> Self {
        // SAFETY: for a status astcenc names, `astcenc_get_error_string`
        // returns a pointer to a static, nul-terminated string that outlives
        // this call. For any other value it returns null — its implementation
        // is a switch with a null `default` — so the null branch below is
        // reachable, not defensive padding, and must stay.
        let message = unsafe {
            let ptr = sys::astcenc_get_error_string(status);
            if ptr.is_null() {
                format!("astcenc reports no description for status {status}")
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        Self::Codec { status, message }
    }
}

/// Turns an astcenc status into `Ok(())` or an [`AstcError::Codec`].
fn check(status: sys::astcenc_error) -> Result<(), AstcError> {
    if status == sys::ASTCENC_SUCCESS {
        Ok(())
    } else {
        Err(AstcError::from_status(status))
    }
}

/// The identity swizzle: each output channel takes the matching input channel.
const IDENTITY_SWIZZLE: sys::astcenc_swizzle = sys::astcenc_swizzle {
    r: sys::ASTCENC_SWZ_R,
    g: sys::ASTCENC_SWZ_G,
    b: sys::ASTCENC_SWZ_B,
    a: sys::ASTCENC_SWZ_A,
};

/// One thread. The codec schedules blocks across as many threads as its context
/// was allocated for, and every block is independent, so raising this is a
/// throughput change rather than an output change. It stays at one until a
/// measurement says the packer needs more, and until that measurement can also
/// show the output did not move.
const THREAD_COUNT: u32 = 1;

/// An astcenc context, freed when dropped.
///
/// Holding it in a type with a `Drop` is what keeps the free paired with the
/// allocation across the `?` returns below.
struct Context {
    raw: *mut sys::astcenc_context,
}

impl Context {
    /// Allocates a context for `config`.
    fn new(config: &sys::astcenc_config) -> Result<Self, AstcError> {
        let mut raw: *mut sys::astcenc_context = std::ptr::null_mut();
        // SAFETY: `config` is a live, fully initialised config; `raw` is a
        // valid out-pointer; a null parent asks for a standalone context.
        let status = unsafe {
            sys::astcenc_context_alloc(config, THREAD_COUNT, &raw mut raw, std::ptr::null())
        };
        check(status)?;
        debug_assert!(!raw.is_null(), "astcenc reported success without a context");
        Ok(Self { raw })
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `raw` came from a successful `astcenc_context_alloc` and is
        // freed exactly once, here.
        unsafe { sys::astcenc_context_free(self.raw) };
    }
}

/// Builds a config, letting astcenc reject an illegal block size, quality or
/// flag combination by name.
fn config_for(
    block: BlockSize,
    color: ColorSpace,
    quality: f32,
    flags: u32,
) -> Result<sys::astcenc_config, AstcError> {
    let mut config = std::mem::MaybeUninit::<sys::astcenc_config>::uninit();
    // SAFETY: the out-pointer is valid for writes. astcenc writes every field
    // before returning success, and leaves the memory untouched on failure —
    // which is why the value is only read on the success path below.
    let status = unsafe {
        sys::astcenc_config_init(
            color.profile(),
            block.x,
            block.y,
            1,
            quality,
            flags,
            config.as_mut_ptr(),
        )
    };
    check(status)?;
    // SAFETY: the call above returned success, so the config is initialised.
    Ok(unsafe { config.assume_init() })
}

/// Compresses `image` to ASTC at `block`.
///
/// The returned payload is exactly [`BlockSize::payload_len`] bytes: the blocks
/// of the grid, in raster order, 16 bytes each. It carries no header — a
/// container is the KTX2 writer's job (story #431).
///
/// The same input, block size, color space and quality always produce the same
/// bytes, on any machine that builds this workspace.
pub fn encode(
    image: Rgba8<'_>,
    block: BlockSize,
    color: ColorSpace,
    quality: Quality,
) -> Result<Vec<u8>, AstcError> {
    // The config is built first so that an illegal footprint comes back as
    // astcenc's own named refusal, before anything here divides by it.
    let config = config_for(block, color, quality.effort(), 0)?;
    let context = Context::new(&config)?;

    let mut payload = vec![0u8; block.payload_len(image.width, image.height)?];

    // astcenc takes an array of slice pointers, one per 2D slice; a 2D image is
    // one slice.
    //
    // The cast drops const because `astcenc_image::data` is `void**` and
    // `astcenc_compress_image` takes the image by mutable pointer. The
    // compressor does not write through it: inside `astcenc_compress_image` the
    // image is immediately rebound and handed to `init_compute_averages` and
    // `compress_image`, both of which take `const astcenc_image&`. Nothing
    // reachable from the compress path takes it any other way. Verified against
    // the vendored 5.6.0 source; recheck it when the pin moves, because the
    // public signature does not enforce this and the header's own wording
    // ("also holds output data") is left over from the command line tool.
    let mut slice: *mut std::ffi::c_void =
        image.texels.as_ptr().cast::<std::ffi::c_void>().cast_mut();
    let mut raw_image = sys::astcenc_image {
        dim_x: image.width,
        dim_y: image.height,
        dim_z: 1,
        data_type: sys::ASTCENC_TYPE_U8,
        data: &raw mut slice,
    };

    // SAFETY: `raw_image` describes exactly the buffer `image` borrows, whose
    // length `Rgba8::new` already checked; `payload` is sized for the block
    // grid; the context was allocated for one thread, so index 0 is the only
    // valid one. Every pointer outlives the call.
    let status = unsafe {
        sys::astcenc_compress_image(
            context.raw,
            &raw mut raw_image,
            &IDENTITY_SWIZZLE,
            payload.as_mut_ptr(),
            payload.len(),
            0,
        )
    };
    check(status)?;
    Ok(payload)
}

/// Decompresses `payload` back to 8-bit RGBA — the reference decode, run by the
/// same library that encoded it.
///
/// `payload` must be exactly the length the block grid needs, which
/// [`BlockSize::payload_len`] reports; a payload of any other length is refused
/// rather than truncated. The returned buffer is `width * height * 4` bytes,
/// rows top to bottom.
pub fn decode(
    payload: &[u8],
    width: u32,
    height: u32,
    block: BlockSize,
    color: ColorSpace,
) -> Result<Vec<u8>, AstcError> {
    if width == 0 || height == 0 {
        return Err(AstcError::ZeroDimension { width, height });
    }

    // A decompress-only context skips the compressor's transient buffers. The
    // quality preset is irrelevant to a decode; astcenc still validates it, so
    // it has to be a legal value.
    //
    // Built before the payload length is computed, so that an illegal footprint
    // comes back as astcenc's own named refusal rather than reaching the
    // division in `payload_len`. `encode` orders the two the same way.
    let config = config_for(
        block,
        color,
        Quality::Medium.effort(),
        sys::ASTCENC_FLG_DECOMPRESS_ONLY,
    )?;
    let context = Context::new(&config)?;

    let expected = block.payload_len(width, height)?;
    if payload.len() != expected {
        return Err(AstcError::PayloadLen {
            expected,
            found: payload.len(),
        });
    }

    let mut texels = vec![0u8; texel_bytes(width, height)?];
    let mut slice: *mut std::ffi::c_void = texels.as_mut_ptr().cast::<std::ffi::c_void>();
    let mut raw_image = sys::astcenc_image {
        dim_x: width,
        dim_y: height,
        dim_z: 1,
        data_type: sys::ASTCENC_TYPE_U8,
        data: &raw mut slice,
    };

    // SAFETY: `texels` is sized for the requested dimensions and `raw_image`
    // describes it exactly; `payload` was length-checked against the same block
    // grid above; the context was allocated for one thread, so index 0 is the
    // only valid one.
    let status = unsafe {
        sys::astcenc_decompress_image(
            context.raw,
            payload.as_ptr(),
            payload.len(),
            &raw mut raw_image,
            &IDENTITY_SWIZZLE,
            0,
        )
    };
    check(status)?;
    Ok(texels)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic test image with a horizontal red ramp, a vertical green
    /// ramp, a constant blue and a diagonal alpha ramp — enough variation that
    /// the encoder cannot fall back to constant-color blocks everywhere.
    fn ramp(width: u32, height: u32) -> Vec<u8> {
        let mut texels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                texels.push((x * 255 / width.max(1)) as u8);
                texels.push((y * 255 / height.max(1)) as u8);
                texels.push(64);
                texels.push(((x + y) * 255 / (width + height).max(1)) as u8);
            }
        }
        texels
    }

    fn solid(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        color
            .iter()
            .copied()
            .cycle()
            .take(width as usize * height as usize * 4)
            .collect()
    }

    /// Unwraps a payload length that is expected to be computable.
    fn payload_len(block: BlockSize, width: u32, height: u32) -> usize {
        block
            .payload_len(width, height)
            .expect("these dimensions are small enough to count")
    }

    #[test]
    fn the_payload_is_one_sixteen_byte_block_per_grid_cell() {
        let block = BlockSize::ASTC_4X4;
        assert_eq!(payload_len(block, 4, 4), 16);
        assert_eq!(payload_len(block, 8, 4), 32);
        // A partial block at an edge still occupies a whole block.
        assert_eq!(payload_len(block, 5, 4), 32);
        assert_eq!(payload_len(block, 1, 1), 16);
        assert_eq!(payload_len(BlockSize { x: 6, y: 6 }, 12, 7), 2 * 2 * 16);
    }

    /// The overflow guard. `2^31 * 2^31 * 4` is exactly `2^64`, so the
    /// unchecked form wraps to zero — a buffer length shorter than the truth,
    /// which is the dangerous direction. Both dimensions are legal `u32`
    /// values, so nothing upstream would have refused them.
    #[test]
    fn dimensions_whose_byte_count_does_not_fit_are_refused() {
        let huge = 1u32 << 31;
        assert_eq!(
            texel_bytes(huge, huge),
            Err(AstcError::ImageTooLarge {
                width: huge,
                height: huge
            })
        );
        assert_eq!(
            BlockSize { x: 4, y: 4 }.payload_len(u32::MAX, u32::MAX),
            Err(AstcError::ImageTooLarge {
                width: u32::MAX,
                height: u32::MAX
            })
        );
    }

    /// A zero block dimension has no block count. `payload_len` is public and
    /// divides by the footprint, so it refuses zero itself rather than dividing
    /// and panicking.
    #[test]
    fn a_zero_block_dimension_is_refused_rather_than_divided_by() {
        assert_eq!(
            BlockSize { x: 0, y: 4 }.payload_len(8, 8),
            Err(AstcError::ZeroBlockDimension { x: 0, y: 4 })
        );
        assert_eq!(
            BlockSize { x: 4, y: 0 }.payload_len(8, 8),
            Err(AstcError::ZeroBlockDimension { x: 4, y: 0 })
        );
    }

    /// `encode` and `decode` must both survive a zero footprint, and must both
    /// report it the same way. They order their steps so astcenc names it
    /// first; before that ordering existed, `decode` divided by zero and
    /// panicked while `encode` returned an error.
    #[test]
    fn a_zero_block_dimension_is_refused_by_both_directions() {
        let texels = ramp(8, 8);
        let image = Rgba8::new(8, 8, &texels).expect("the buffer matches the dimensions");
        let zero = BlockSize { x: 0, y: 0 };

        let encode_error = encode(image, zero, ColorSpace::Linear, Quality::Fast)
            .expect_err("a zero footprint has no block grid");
        let payload = vec![0u8; BLOCK_BYTES];
        let decode_error = decode(&payload, 8, 8, zero, ColorSpace::Linear)
            .expect_err("a zero footprint has no block grid");

        assert!(matches!(encode_error, AstcError::Codec { .. }));
        assert_eq!(encode_error, decode_error);
    }

    #[test]
    fn an_encode_produces_exactly_the_payload_the_grid_needs() {
        let texels = ramp(16, 12);
        let image = Rgba8::new(16, 12, &texels).expect("the buffer matches the dimensions");
        let payload = encode(image, BlockSize::ASTC_4X4, ColorSpace::Srgb, Quality::Fast)
            .expect("a 16x12 sRGB image encodes at 4x4");
        assert_eq!(payload.len(), payload_len(BlockSize::ASTC_4X4, 16, 12));
        assert_eq!(payload.len(), 4 * 3 * BLOCK_BYTES);
    }

    /// The property the packer depends on. If this ever fails, no bank is
    /// reproducible and no golden derived from one means anything.
    #[test]
    fn encoding_the_same_input_twice_produces_identical_bytes() {
        let texels = ramp(32, 32);
        let image = Rgba8::new(32, 32, &texels).expect("the buffer matches the dimensions");
        let first = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Srgb,
            Quality::Medium,
        )
        .expect("a 32x32 sRGB image encodes at 4x4");
        let second = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Srgb,
            Quality::Medium,
        )
        .expect("a 32x32 sRGB image encodes at 4x4");
        assert_eq!(first, second);
    }

    /// A single flat color is the one case ASTC reproduces exactly, through a
    /// void-extent block. Anything less than bit equality here would mean the
    /// encode and the decode disagree about the color space or the channel
    /// order, which a tolerance-based check could hide.
    #[test]
    fn a_solid_color_survives_a_round_trip_unchanged() {
        let texels = solid(8, 8, [200, 30, 90, 255]);
        let image = Rgba8::new(8, 8, &texels).expect("the buffer matches the dimensions");
        let payload = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Medium,
        )
        .expect("a solid 8x8 linear image encodes at 4x4");
        let decoded = decode(&payload, 8, 8, BlockSize::ASTC_4X4, ColorSpace::Linear)
            .expect("the payload decodes at the size it was encoded for");
        assert_eq!(decoded, texels);
    }

    /// The round trip that matters: a varied image comes back close, through
    /// the encoder's own decoder. The bound is loose on purpose — this test
    /// asserts that encode and decode are talking to each other, not a quality
    /// band. Bands are per asset class and belong to the profile contracts
    /// (story #432).
    #[test]
    fn a_ramp_survives_a_round_trip_within_a_loose_bound() {
        let texels = ramp(64, 48);
        let image = Rgba8::new(64, 48, &texels).expect("the buffer matches the dimensions");
        let payload = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Medium,
        )
        .expect("a 64x48 linear image encodes at 4x4");
        let decoded = decode(&payload, 64, 48, BlockSize::ASTC_4X4, ColorSpace::Linear)
            .expect("the payload decodes at the size it was encoded for");

        assert_eq!(decoded.len(), texels.len());
        let worst = texels
            .iter()
            .zip(decoded.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .expect("the image is not empty");
        // The observed value for this fixture at this preset is 6, and it is
        // deterministic, because the encoder is built invariant. The bound is 8
        // rather than 6 so an encoder bump does not fail on a one-unit change,
        // and it is not much looser than that because a loose bound stops
        // discriminating: at 8x8 this same fixture reaches 11, which is the
        // regression the bound is here to catch. If a pin bump ever trips this,
        // re-derive the observed value before widening the bound.
        assert!(
            worst <= 8,
            "the worst channel difference over a 4x4 round trip is {worst}, which is too large to be compression alone"
        );
    }

    /// A one-texel image occupies one whole block and comes back exactly,
    /// through the same void-extent path a solid color takes. It is the
    /// smallest input the grid arithmetic has to survive.
    #[test]
    fn a_single_texel_image_round_trips() {
        let texels = vec![17u8, 200, 5, 128];
        let image = Rgba8::new(1, 1, &texels).expect("the buffer matches the dimensions");
        let payload = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Medium,
        )
        .expect("a 1x1 linear image encodes at 4x4");
        assert_eq!(payload.len(), BLOCK_BYTES);
        let decoded = decode(&payload, 1, 1, BlockSize::ASTC_4X4, ColorSpace::Linear)
            .expect("the payload decodes at the size it was encoded for");
        assert_eq!(decoded, texels);
    }

    /// Every quality preset reaches the codec and is accepted, and the six map
    /// to six distinct effort values in increasing order. Without this, two
    /// presets swapped in the mapping would encode fine and go unnoticed —
    /// four of the six are otherwise never exercised.
    #[test]
    fn the_quality_presets_are_distinct_and_ordered() {
        let presets = [
            Quality::Fastest,
            Quality::Fast,
            Quality::Medium,
            Quality::Thorough,
            Quality::VeryThorough,
            Quality::Exhaustive,
        ];
        let efforts: Vec<f32> = presets.iter().map(|preset| preset.effort()).collect();
        assert!(
            efforts.windows(2).all(|pair| pair[0] < pair[1]),
            "the presets do not increase in effort: {efforts:?}"
        );

        let texels = ramp(8, 8);
        let image = Rgba8::new(8, 8, &texels).expect("the buffer matches the dimensions");
        for preset in presets {
            encode(image, BlockSize::ASTC_4X4, ColorSpace::Linear, preset)
                .unwrap_or_else(|error| panic!("{preset:?} was refused: {error}"));
        }
    }

    /// Decoding with a different color space than the encode used changes the
    /// result, so the color space genuinely reaches the decoder.
    ///
    /// Worth pinning because the difference is far smaller than it looks like
    /// it should be: astcenc's sRGB decode to 8-bit returns stored values
    /// rather than applying a transfer function, so a mismatch moves about one
    /// byte in seven and never by more than a unit or two. A caller cannot rely
    /// on a mismatch being obvious in the pixels; it has to pass the same color
    /// space to both calls.
    #[test]
    fn decoding_with_the_other_color_space_changes_the_result() {
        let texels = ramp(64, 48);
        let image = Rgba8::new(64, 48, &texels).expect("the buffer matches the dimensions");
        let payload = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Medium,
        )
        .expect("a 64x48 linear image encodes at 4x4");
        let matched = decode(&payload, 64, 48, BlockSize::ASTC_4X4, ColorSpace::Linear)
            .expect("the payload decodes at the size it was encoded for");
        let mismatched = decode(&payload, 64, 48, BlockSize::ASTC_4X4, ColorSpace::Srgb)
            .expect("the payload decodes at the size it was encoded for");
        assert_ne!(matched, mismatched);
    }

    /// The pin is reported in a shape an auditor can use: a release tag and a
    /// full 40-character commit hash.
    #[test]
    fn the_reported_pin_names_a_release_and_a_full_commit() {
        let (version, commit) = vendored_astcenc();
        assert!(!version.is_empty());
        assert_eq!(commit.len(), 40, "a git commit hash is 40 hex characters");
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    /// An image whose dimensions are not multiples of the block size exercises
    /// the padded edge blocks, which is where an off-by-one in the grid would
    /// show up.
    #[test]
    fn an_image_that_does_not_fill_its_edge_blocks_round_trips() {
        let texels = ramp(13, 7);
        let image = Rgba8::new(13, 7, &texels).expect("the buffer matches the dimensions");
        let payload = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Fast,
        )
        .expect("a 13x7 linear image encodes at 4x4");
        assert_eq!(payload.len(), 4 * 2 * BLOCK_BYTES);
        let decoded = decode(&payload, 13, 7, BlockSize::ASTC_4X4, ColorSpace::Linear)
            .expect("the payload decodes at the size it was encoded for");
        assert_eq!(decoded.len(), 13 * 7 * 4);
    }

    /// A larger footprint is reachable, and it really does cost fewer bytes.
    #[test]
    fn a_larger_block_size_produces_a_smaller_payload() {
        let texels = ramp(24, 24);
        let image = Rgba8::new(24, 24, &texels).expect("the buffer matches the dimensions");
        let fine = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Fast,
        )
        .expect("a 24x24 linear image encodes at 4x4");
        let coarse = encode(
            image,
            BlockSize { x: 8, y: 8 },
            ColorSpace::Linear,
            Quality::Fast,
        )
        .expect("a 24x24 linear image encodes at 8x8");
        assert!(coarse.len() < fine.len());
        assert_eq!(coarse.len(), 3 * 3 * BLOCK_BYTES);
    }

    #[test]
    fn a_buffer_that_does_not_match_its_dimensions_is_refused() {
        let texels = vec![0u8; 4 * 4 * 4 - 1];
        let error = Rgba8::new(4, 4, &texels).expect_err("63 bytes is not a 4x4 RGBA image");
        assert_eq!(
            error,
            AstcError::TexelCount {
                expected: 64,
                found: 63
            }
        );
    }

    #[test]
    fn a_zero_dimension_is_refused() {
        let texels: Vec<u8> = Vec::new();
        let error = Rgba8::new(0, 4, &texels).expect_err("an image cannot be zero texels wide");
        assert_eq!(
            error,
            AstcError::ZeroDimension {
                width: 0,
                height: 4
            }
        );
    }

    /// A truncated payload is refused before it reaches astcenc, which would
    /// otherwise be handed a length it cannot check against the dimensions.
    #[test]
    fn a_payload_of_the_wrong_length_is_refused() {
        let payload = vec![0u8; BLOCK_BYTES];
        assert_eq!(
            decode(&payload, 8, 8, BlockSize::ASTC_4X4, ColorSpace::Linear),
            Err(AstcError::PayloadLen {
                expected: 64,
                found: 16
            })
        );
    }

    /// An illegal block size comes back as astcenc's own named refusal rather
    /// than a panic or a substituted footprint.
    #[test]
    fn an_illegal_block_size_is_refused_by_the_codec() {
        let texels = ramp(8, 8);
        let image = Rgba8::new(8, 8, &texels).expect("the buffer matches the dimensions");
        let error = encode(
            image,
            BlockSize { x: 7, y: 7 },
            ColorSpace::Linear,
            Quality::Fast,
        )
        .expect_err("7x7 is not an ASTC block size");
        match error {
            AstcError::Codec { status, message } => {
                assert_ne!(status, sys::ASTCENC_SUCCESS);
                assert!(!message.is_empty());
            }
            other => panic!("expected a codec refusal, got {other}"),
        }
    }

    /// The two color spaces are genuinely different encodes, so a caller
    /// picking the wrong one cannot go unnoticed downstream.
    #[test]
    fn the_two_color_spaces_encode_differently() {
        let texels = ramp(16, 16);
        let image = Rgba8::new(16, 16, &texels).expect("the buffer matches the dimensions");
        let linear = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Linear,
            Quality::Medium,
        )
        .expect("a 16x16 linear image encodes at 4x4");
        let srgb = encode(
            image,
            BlockSize::ASTC_4X4,
            ColorSpace::Srgb,
            Quality::Medium,
        )
        .expect("a 16x16 sRGB image encodes at 4x4");
        assert_ne!(linear, srgb);
    }
}
