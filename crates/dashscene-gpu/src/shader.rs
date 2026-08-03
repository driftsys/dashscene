//! The shader library, as source.
//!
//! One file holds the painter's signed-distance math, and everything that
//! evaluates it includes this string: the render pipelines (story #580) and the
//! layer-2 conformance harness (story #579). That is what R-T5 asks for —
//! "SDF shader math single-sourced (common include) into both painters'
//! shading languages" — reduced to the one mechanism WGSL needs, which is
//! textual inclusion.
//!
//! Exposed as a `&str` rather than compiled here because this crate owns no
//! device until story #580. A consumer concatenates it with its own entry
//! points and hands the result to `wgpu::Device::create_shader_module`.

/// The signed-distance math: rounded-box distance, antialiased coverage, the
/// MSDF median resolve, the four gradient parameterizations, and stroke
/// coverage.
///
/// Contains no entry point, samples no texture, reads no uniform and takes no
/// derivative. That is deliberate: it makes every function here evaluable in a
/// compute shader, which is what lets layer 2 check the arithmetic on a runner
/// with no GPU.
pub const SDF_WGSL: &str = include_str!("shaders/sdf.wgsl");
