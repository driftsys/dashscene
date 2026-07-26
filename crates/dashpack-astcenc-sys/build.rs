//! Compiles the vendored astcenc codec library into a static library that
//! `dashpack-astcenc-sys` links.
//!
//! Upstream builds with CMake. This script does not: reproducing the handful of
//! compiler settings that matter is a shorter and more auditable dependency
//! than requiring CMake on every machine that builds the workspace. Each
//! setting below names the upstream option it mirrors, so the two can be
//! compared when the pin moves (`vendor/VENDOR.md`).

use std::env;
use std::path::Path;

/// The codec library's translation units, in upstream's `cmake_core.cmake`
/// order.
///
/// Listed one by one rather than globbed: a re-vendoring that adds a file
/// should fail to link until someone decides the file belongs here, instead of
/// being compiled in unnoticed.
const SOURCES: &[&str] = &[
    "astcenc_averages_and_directions.cpp",
    "astcenc_block_sizes.cpp",
    "astcenc_color_quantize.cpp",
    "astcenc_color_unquantize.cpp",
    "astcenc_compress_symbolic.cpp",
    "astcenc_compute_variance.cpp",
    "astcenc_decompress_symbolic.cpp",
    "astcenc_diagnostic_trace.cpp",
    "astcenc_entry.cpp",
    "astcenc_find_best_partitioning.cpp",
    "astcenc_ideal_endpoints_and_weights.cpp",
    "astcenc_image.cpp",
    "astcenc_integer_sequence.cpp",
    "astcenc_mathlib.cpp",
    "astcenc_mathlib_softfloat.cpp",
    "astcenc_partition_tables.cpp",
    "astcenc_percentile_tables.cpp",
    "astcenc_pick_best_endpoint_format.cpp",
    "astcenc_quantization.cpp",
    "astcenc_symbolic_physical.cpp",
    "astcenc_weight_align.cpp",
    "astcenc_weight_quant_xfer_tables.cpp",
];

fn main() {
    let vendor = Path::new("vendor/astcenc");

    // cc records a rerun trigger per compiled file, which covers the .cpp
    // sources but not the headers they include. astcenc_internal.h alone is
    // 81 kB of definitions, so watch the whole directory.
    println!("cargo:rerun-if-changed=vendor/astcenc");
    println!("cargo:rerun-if-changed=src/layout.cpp");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("cargo sets CARGO_CFG_TARGET_ARCH");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").expect("cargo sets CARGO_CFG_TARGET_ENV");
    let is_msvc = target_env == "msvc";

    let mut build = cc::Build::new();
    build.cpp(true).std("c++14").include(vendor);

    for source in SOURCES {
        build.file(vendor.join(source));
    }

    // Ours, not Arm's: it reports the layout the vendored header actually
    // produces, so the hand-written `astcenc_config` in src/lib.rs is checked
    // against the compiler rather than trusted.
    build.file("src/layout.cpp");

    // This code is vendored, so its warnings are not ours to act on and would
    // only bury warnings from code we do own.
    build.warnings(false);

    // Only 2D block sizes are ever packed, and 12x12 is the largest of them.
    // Upstream's -DASTCENC_BLOCK_MAX_TEXELS=144. This lowers the codec's memory
    // footprint; block sizes outside the limit return
    // ASTCENC_ERR_NOT_IMPLEMENTED from context allocation, which is a named
    // refusal rather than a silent fallback.
    build.define("ASTCENC_BLOCK_MAX_TEXELS", "144");

    // Upstream's ASTCENC_ISA_SIMD selection. Every arm here picks instructions
    // that are part of the target architecture's own baseline, so the build
    // needs no runtime dispatch and no host CPU probe: aarch64 always has NEON,
    // and x86-64 always has SSE2. Anything else falls back to the scalar path,
    // which is slow but correct.
    //
    // Choosing by architecture rather than by host capability is safe only
    // because the build is invariant (see the floating-point flags below):
    // upstream guarantees that every invariant build of one revision produces
    // bit-identical output, whatever the compiler and whatever the CPU. That
    // guarantee is what this crate exists to preserve — a bank derived on an
    // arm64 laptop must equal a bank derived on an x86-64 build machine.
    match target_arch.as_str() {
        "aarch64" => {
            build
                .define("ASTCENC_NEON", "1")
                .define("ASTCENC_SVE", "0")
                .define("ASTCENC_SSE", "0")
                .define("ASTCENC_AVX", "0")
                .define("ASTCENC_POPCNT", "0")
                .define("ASTCENC_F16C", "0");
        }
        "x86_64" => {
            build
                .define("ASTCENC_NEON", "0")
                .define("ASTCENC_SVE", "0")
                .define("ASTCENC_SSE", "20")
                .define("ASTCENC_AVX", "0")
                .define("ASTCENC_POPCNT", "0")
                .define("ASTCENC_F16C", "0")
                .define("ASTCENC_X86_GATHERS", "0");
            if !is_msvc {
                // Apple's clang defaults to SSE4.1 on x86-64. Upstream pins the
                // SSE2 build down to SSE2 for exactly that reason.
                build.flag("-msse2").flag("-mno-sse4.1");
            }
        }
        _ => {
            build
                .define("ASTCENC_NEON", "0")
                .define("ASTCENC_SVE", "0")
                .define("ASTCENC_SSE", "0")
                .define("ASTCENC_AVX", "0")
                .define("ASTCENC_POPCNT", "0")
                .define("ASTCENC_F16C", "0");
        }
    }

    // Invariance. Upstream's ASTCENC_INVARIANCE defaults to ON and this build
    // keeps it on, which costs some encode speed and buys bit-identical output
    // across machines. ASTCENC_NO_INVARIANCE is deliberately left undefined.
    //
    // Contraction is the setting that matters: with it on, the compiler may
    // fuse a multiply and an add into one FMA instruction, which rounds once
    // instead of twice and changes the encoder's search decisions. The other
    // two flags stop the compiler assuming it may reassociate floating point or
    // that it must maintain errno.
    if is_msvc {
        build.flag("/fp:precise");
    } else {
        build.flag("-ffp-contract=off").flag("-fno-math-errno");
        build.flag_if_supported("-fno-unsafe-math-optimizations");
    }

    // The codec's internal work scheduler uses std::mutex and
    // std::condition_variable. Upstream passes -pthread on Linux and macOS.
    if env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("unix") {
        build.flag_if_supported("-pthread");
    }

    build.compile("dashpack_astcenc");
}
