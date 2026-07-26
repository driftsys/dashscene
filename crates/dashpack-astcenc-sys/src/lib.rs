//! Raw bindings to a vendored, version-pinned astcenc.
//!
//! # Why the source is vendored
//!
//! The packer runs on every pack, and a bank derived on one machine has to
//! equal a bank derived on another. An encoder installed on `PATH` cannot carry
//! that guarantee: the pin would be whatever version each machine happens to
//! have. In-tree source pins the encoder by commit, which is stricter, and it
//! removes the external-binary dependency the packer is not allowed to have.
//!
//! This deliberately does not follow the `msdf-atlas-gen` precedent
//! (`docs/decisions/atlas-gen-external-pinned-binary.md`), which shells out to
//! an installed binary. That tool runs when a font atlas is regenerated, which
//! is rare; the packer runs on every pack, and its output is compared byte for
//! byte.
//!
//! # The same library decodes
//!
//! `astcenc_decompress_image` is the reference decode, in process. One pinned
//! library in both directions is what later lets the reference painter preview
//! a compressed profile, and lets a pure-Rust block decoder be welded to this
//! one by byte equality, without a second implementation of the codec to
//! disagree with.
//!
//! # What "standalone" means here
//!
//! No production-grade pure-Rust ASTC encoder exists. Standalone means the
//! encoder is vendored and linked into the binary, not that it was rewritten.
//! That caveat is recorded rather than left to be rediscovered.
//!
//! # What this crate is not
//!
//! It declares the C entry points and the types they need, and nothing else. It
//! is not a safe wrapper: every function here is `unsafe`, takes raw pointers,
//! and reports failure through an `astcenc_error` code that the caller must
//! check. The safe surface lives in `dashpack::astc`.
//!
//! The pin, the copied file set, the license position and the build settings
//! are all in `vendor/VENDOR.md`.

// The C names are kept exactly as astcenc.h spells them. Renaming them to Rust
// case would make this file harder to check against the header, which is the
// only thing that makes a hand-written binding trustworthy.
#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_float, c_uint, c_void};

/// The upstream release tag the sources under `vendor/astcenc` were taken from.
///
/// The codec library exposes no version symbol — only the command line tool
/// does, and that is not vendored — so this is a record of the vendoring step
/// rather than a value read back out of the compiled library. `vendor/VENDOR.md`
/// holds the full pin and must be updated in the same commit as this constant.
pub const VENDORED_VERSION: &str = "5.6.0";

/// The upstream commit the sources under `vendor/astcenc` were taken from.
pub const VENDORED_COMMIT: &str = "2c9eafa70960bfabaa4701aed99e51857b70839a";

/// A codec API error code, as returned by every entry point below.
///
/// Declared as an integer rather than a Rust `enum` on purpose: a Rust `enum`
/// holding a value outside its variant list is undefined behaviour, and the
/// codec is free to grow a code this binding has not been told about.
pub type astcenc_error = c_uint;

pub const ASTCENC_SUCCESS: astcenc_error = 0;
pub const ASTCENC_ERR_OUT_OF_MEM: astcenc_error = 1;
pub const ASTCENC_ERR_BAD_CPU_FLOAT: astcenc_error = 2;
pub const ASTCENC_ERR_BAD_PARAM: astcenc_error = 3;
pub const ASTCENC_ERR_BAD_BLOCK_SIZE: astcenc_error = 4;
pub const ASTCENC_ERR_BAD_PROFILE: astcenc_error = 5;
pub const ASTCENC_ERR_BAD_QUALITY: astcenc_error = 6;
pub const ASTCENC_ERR_BAD_SWIZZLE: astcenc_error = 7;
pub const ASTCENC_ERR_BAD_FLAGS: astcenc_error = 8;
pub const ASTCENC_ERR_BAD_CONTEXT: astcenc_error = 9;
pub const ASTCENC_ERR_NOT_IMPLEMENTED: astcenc_error = 10;
pub const ASTCENC_ERR_BAD_DECODE_MODE: astcenc_error = 11;

/// A codec color profile.
pub type astcenc_profile = c_uint;

pub const ASTCENC_PRF_LDR_SRGB: astcenc_profile = 0;
pub const ASTCENC_PRF_LDR: astcenc_profile = 1;
pub const ASTCENC_PRF_HDR_RGB_LDR_A: astcenc_profile = 2;
pub const ASTCENC_PRF_HDR: astcenc_profile = 3;

/// Search quality presets. Any value in `0.0 ..= 100.0` is accepted; effort is
/// not linear across that range.
pub const ASTCENC_PRE_FASTEST: c_float = 0.0;
pub const ASTCENC_PRE_FAST: c_float = 10.0;
pub const ASTCENC_PRE_MEDIUM: c_float = 60.0;
pub const ASTCENC_PRE_THOROUGH: c_float = 98.0;
pub const ASTCENC_PRE_VERYTHOROUGH: c_float = 99.0;
pub const ASTCENC_PRE_EXHAUSTIVE: c_float = 100.0;

/// A codec component swizzle selector.
pub type astcenc_swz = c_uint;

pub const ASTCENC_SWZ_R: astcenc_swz = 0;
pub const ASTCENC_SWZ_G: astcenc_swz = 1;
pub const ASTCENC_SWZ_B: astcenc_swz = 2;
pub const ASTCENC_SWZ_A: astcenc_swz = 3;
pub const ASTCENC_SWZ_0: astcenc_swz = 4;
pub const ASTCENC_SWZ_1: astcenc_swz = 5;
pub const ASTCENC_SWZ_Z: astcenc_swz = 6;

/// A texel component data format.
pub type astcenc_type = c_uint;

pub const ASTCENC_TYPE_U8: astcenc_type = 0;
pub const ASTCENC_TYPE_F16: astcenc_type = 1;
pub const ASTCENC_TYPE_F32: astcenc_type = 2;

/// Config flag bits, passed to [`astcenc_config_init`].
pub const ASTCENC_FLG_MAP_NORMAL: c_uint = 1 << 0;
pub const ASTCENC_FLG_USE_DECODE_UNORM8: c_uint = 1 << 1;
pub const ASTCENC_FLG_USE_ALPHA_WEIGHT: c_uint = 1 << 2;
pub const ASTCENC_FLG_USE_PERCEPTUAL: c_uint = 1 << 3;
pub const ASTCENC_FLG_DECOMPRESS_ONLY: c_uint = 1 << 4;
pub const ASTCENC_FLG_SELF_DECOMPRESS_ONLY: c_uint = 1 << 5;
pub const ASTCENC_FLG_MAP_RGBM: c_uint = 1 << 6;

/// A texel component swizzle.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct astcenc_swizzle {
    pub r: astcenc_swz,
    pub g: astcenc_swz,
    pub b: astcenc_swz,
    pub a: astcenc_swz,
}

/// An uncompressed 2D or 3D image. 3D images are an array of 2D slices.
#[repr(C)]
#[derive(Debug)]
pub struct astcenc_image {
    pub dim_x: c_uint,
    pub dim_y: c_uint,
    pub dim_z: c_uint,
    pub data_type: astcenc_type,
    /// An array of `dim_z` pointers, one per 2D slice.
    pub data: *mut *mut c_void,
}

/// A compression progress callback, invoked from a codec worker thread with a
/// percentage between 0 and 100.
pub type astcenc_progress_callback = Option<unsafe extern "C" fn(c_float)>;

/// The codec config.
///
/// Populated by [`astcenc_config_init`], optionally adjusted, then handed to
/// [`astcenc_context_alloc`]. The field order is the header's field order, and
/// the layout test in this crate checks that claim against the compiler rather
/// than taking it on trust.
///
/// The header adds a `trace_file_path` field under `ASTCENC_DIAGNOSTICS`. This
/// build does not define it, so the field is absent here too.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct astcenc_config {
    pub profile: astcenc_profile,
    pub flags: c_uint,
    pub block_x: c_uint,
    pub block_y: c_uint,
    pub block_z: c_uint,
    pub cw_r_weight: c_float,
    pub cw_g_weight: c_float,
    pub cw_b_weight: c_float,
    pub cw_a_weight: c_float,
    pub a_scale_radius: c_uint,
    pub rgbm_m_scale: c_float,
    pub tune_partition_count_limit: c_uint,
    pub tune_2partition_index_limit: c_uint,
    pub tune_3partition_index_limit: c_uint,
    pub tune_4partition_index_limit: c_uint,
    pub tune_block_mode_limit: c_uint,
    pub tune_refinement_limit: c_uint,
    pub tune_candidate_limit: c_uint,
    pub tune_2partitioning_candidate_limit: c_uint,
    pub tune_3partitioning_candidate_limit: c_uint,
    pub tune_4partitioning_candidate_limit: c_uint,
    pub tune_db_limit: c_float,
    pub tune_mse_overshoot: c_float,
    pub tune_2partition_early_out_limit_factor: c_float,
    pub tune_3partition_early_out_limit_factor: c_float,
    pub tune_2plane_early_out_limit_correlation: c_float,
    pub tune_search_mode0_enable: c_float,
    pub progress_callback: astcenc_progress_callback,
}

/// An opaque codec context. Only ever handled through a pointer.
#[repr(C)]
pub struct astcenc_context {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    /// Populates `config` from the default settings for the given profile,
    /// block size, quality and flags.
    pub fn astcenc_config_init(
        profile: astcenc_profile,
        block_x: c_uint,
        block_y: c_uint,
        block_z: c_uint,
        quality: c_float,
        flags: c_uint,
        config: *mut astcenc_config,
    ) -> astcenc_error;

    /// Allocates a codec context. `parent_context` may be null for a standalone
    /// context. Every successful call must be paired with
    /// [`astcenc_context_free`].
    pub fn astcenc_context_alloc(
        config: *const astcenc_config,
        thread_count: c_uint,
        context: *mut *mut astcenc_context,
        parent_context: *const astcenc_context,
    ) -> astcenc_error;

    /// Compresses one image into `data_out`, which must be exactly the size the
    /// block grid needs.
    pub fn astcenc_compress_image(
        context: *mut astcenc_context,
        image: *mut astcenc_image,
        swizzle: *const astcenc_swizzle,
        data_out: *mut u8,
        data_len: usize,
        thread_index: c_uint,
    ) -> astcenc_error;

    /// Decompresses `data` into `image_out`, whose dimensions and data type
    /// select what is written.
    pub fn astcenc_decompress_image(
        context: *mut astcenc_context,
        data: *const u8,
        data_len: usize,
        image_out: *mut astcenc_image,
        swizzle: *const astcenc_swizzle,
        thread_index: c_uint,
    ) -> astcenc_error;

    /// Frees a context allocated by [`astcenc_context_alloc`].
    pub fn astcenc_context_free(context: *mut astcenc_context);

    /// Returns a static, nul-terminated description of a status code, or null
    /// for a value astcenc does not name.
    ///
    /// The null return is easy to miss and matters: the implementation is a
    /// switch over the named codes with a null `default`, so any status this
    /// binding has not been told about comes back as a null pointer rather than
    /// as a fallback string. Callers must check it.
    pub fn astcenc_get_error_string(status: astcenc_error) -> *const c_char;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    unsafe extern "C" {
        /// Not part of astcenc. Reports the layout the vendored header
        /// produces: element 0 is the size of `astcenc_config`, element 1 its
        /// alignment, and the rest are its field offsets in declaration order.
        /// Defined in `src/layout.cpp`, which is compiled into the library on
        /// every build; only the test reads it.
        fn dashpack_astcenc_config_layout(count: *mut usize) -> *const usize;
    }

    /// The size, alignment and field offsets Rust computes for
    /// [`astcenc_config`], in the same order `src/layout.cpp` reports them.
    fn rust_layout() -> Vec<usize> {
        vec![
            size_of::<astcenc_config>(),
            align_of::<astcenc_config>(),
            offset_of!(astcenc_config, profile),
            offset_of!(astcenc_config, flags),
            offset_of!(astcenc_config, block_x),
            offset_of!(astcenc_config, block_y),
            offset_of!(astcenc_config, block_z),
            offset_of!(astcenc_config, cw_r_weight),
            offset_of!(astcenc_config, cw_g_weight),
            offset_of!(astcenc_config, cw_b_weight),
            offset_of!(astcenc_config, cw_a_weight),
            offset_of!(astcenc_config, a_scale_radius),
            offset_of!(astcenc_config, rgbm_m_scale),
            offset_of!(astcenc_config, tune_partition_count_limit),
            offset_of!(astcenc_config, tune_2partition_index_limit),
            offset_of!(astcenc_config, tune_3partition_index_limit),
            offset_of!(astcenc_config, tune_4partition_index_limit),
            offset_of!(astcenc_config, tune_block_mode_limit),
            offset_of!(astcenc_config, tune_refinement_limit),
            offset_of!(astcenc_config, tune_candidate_limit),
            offset_of!(astcenc_config, tune_2partitioning_candidate_limit),
            offset_of!(astcenc_config, tune_3partitioning_candidate_limit),
            offset_of!(astcenc_config, tune_4partitioning_candidate_limit),
            offset_of!(astcenc_config, tune_db_limit),
            offset_of!(astcenc_config, tune_mse_overshoot),
            offset_of!(astcenc_config, tune_2partition_early_out_limit_factor),
            offset_of!(astcenc_config, tune_3partition_early_out_limit_factor),
            offset_of!(astcenc_config, tune_2plane_early_out_limit_correlation),
            offset_of!(astcenc_config, tune_search_mode0_enable),
            offset_of!(astcenc_config, progress_callback),
        ]
    }

    /// The check that makes the hand-written `astcenc_config` safe to pass
    /// across the boundary. A field added, removed or reordered upstream shows
    /// up here as a failed assertion instead of as a value read from the wrong
    /// place.
    #[test]
    fn the_config_layout_matches_the_vendored_header() {
        let mut count = 0usize;
        // SAFETY: the C side writes one `usize` through `count` and returns a
        // pointer to a static array of that length, which outlives this call.
        let reported = unsafe {
            let ptr = dashpack_astcenc_config_layout(&raw mut count);
            core::slice::from_raw_parts(ptr, count)
        };

        let expected = rust_layout();
        assert_eq!(
            reported.len(),
            expected.len(),
            "src/layout.cpp reports {} entries, lib.rs lists {} — the two lists have drifted apart",
            reported.len(),
            expected.len()
        );
        assert_eq!(reported[0], expected[0], "sizeof(astcenc_config) differs");
        assert_eq!(reported[1], expected[1], "alignof(astcenc_config) differs");
        for (index, (c_offset, rust_offset)) in
            reported[2..].iter().zip(expected[2..].iter()).enumerate()
        {
            assert_eq!(
                c_offset, rust_offset,
                "field {index} of astcenc_config is at {c_offset} in C and {rust_offset} in Rust"
            );
        }
    }

    /// The library is reachable and reports its own status codes, so a linking
    /// failure is a test failure rather than something the first encode
    /// discovers.
    #[test]
    fn the_linked_library_describes_its_status_codes() {
        // SAFETY: `ASTCENC_ERR_BAD_BLOCK_SIZE` is a code astcenc names, so the
        // returned pointer is a static, nul-terminated string.
        let message = unsafe {
            let ptr = astcenc_get_error_string(ASTCENC_ERR_BAD_BLOCK_SIZE);
            assert!(!ptr.is_null());
            core::ffi::CStr::from_ptr(ptr)
        };
        assert!(!message.to_bytes().is_empty());
    }

    /// The null return that every caller has to handle. astcenc's lookup is a
    /// switch over the codes it names, with a null `default`, so a status
    /// outside that set yields no string at all. Pinned here because the
    /// signature does not say so and a caller that assumed a fallback string
    /// would dereference null.
    #[test]
    fn an_unnamed_status_code_has_no_description() {
        // SAFETY: the call is valid for any input; the result may be null,
        // which is what this test is about.
        let ptr = unsafe { astcenc_get_error_string(9999) };
        assert!(ptr.is_null());
    }

    /// A default config for a 4x4 LDR block comes back populated, which proves
    /// the struct is being written through in the layout Rust expects.
    #[test]
    fn a_default_config_comes_back_populated() {
        let mut config = core::mem::MaybeUninit::<astcenc_config>::uninit();
        // SAFETY: `astcenc_config_init` writes every field of the config it is
        // given before returning success.
        let status = unsafe {
            astcenc_config_init(
                ASTCENC_PRF_LDR,
                4,
                4,
                1,
                ASTCENC_PRE_MEDIUM,
                0,
                config.as_mut_ptr(),
            )
        };
        assert_eq!(status, ASTCENC_SUCCESS);
        // SAFETY: the call above returned success, so the config is initialised.
        let config = unsafe { config.assume_init() };
        assert_eq!(config.profile, ASTCENC_PRF_LDR);
        assert_eq!((config.block_x, config.block_y, config.block_z), (4, 4, 1));
        assert!(config.tune_db_limit > 0.0);
        assert!(config.tune_partition_count_limit >= 1);
    }

    /// The 2D-only build limit is a named refusal, not a silent fallback. 6x6x6
    /// is a legal ASTC block size at 216 texels, and this build stops at 144,
    /// so it comes back as `ASTCENC_ERR_NOT_IMPLEMENTED` rather than as a
    /// quietly substituted block size.
    #[test]
    fn a_block_size_beyond_the_build_limit_is_refused_by_name() {
        let mut config = core::mem::MaybeUninit::<astcenc_config>::uninit();
        // SAFETY: the config pointer is valid for writes; on failure astcenc
        // leaves it untouched, and this test never reads it.
        let status = unsafe {
            astcenc_config_init(
                ASTCENC_PRF_LDR,
                6,
                6,
                6,
                ASTCENC_PRE_MEDIUM,
                0,
                config.as_mut_ptr(),
            )
        };
        assert_eq!(status, ASTCENC_ERR_NOT_IMPLEMENTED);
    }

    /// Every 2D block size the format defines stays available under the 144
    /// texel limit, 12x12 being the largest.
    #[test]
    fn the_largest_two_dimensional_block_size_is_still_available() {
        let mut config = core::mem::MaybeUninit::<astcenc_config>::uninit();
        // SAFETY: the config pointer is valid for writes.
        let status = unsafe {
            astcenc_config_init(
                ASTCENC_PRF_LDR,
                12,
                12,
                1,
                ASTCENC_PRE_MEDIUM,
                0,
                config.as_mut_ptr(),
            )
        };
        assert_eq!(status, ASTCENC_SUCCESS);
    }
}
