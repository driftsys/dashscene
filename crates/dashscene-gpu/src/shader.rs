//! The shader library, as source.
//!
//! One file holds the painter's signed-distance math, and R-T5 asks for it to be
//! "single-sourced (common include) into both painters' shading languages".
//!
//! **Two mechanisms serve that, not one.** Everything in THIS crate that
//! evaluates the math includes this string — the render pipelines (story #580)
//! and the layer-2 conformance harness (story #579) — which is textual
//! inclusion, the one mechanism WGSL needs. A third evaluator lives outside the
//! crate and uses neither the string nor textual inclusion: `unity/package-gate`
//! compiles the same module to HLSL with `naga` for the Unity painter (story
//! #1122), and the Unity shaders `#include` that generated file. An earlier
//! revision of this comment named the second consumer and then said R-T5
//! reduces to textual inclusion, which contradicted it.
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
