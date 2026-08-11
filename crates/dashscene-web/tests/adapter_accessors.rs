#![cfg(target_arch = "wasm32")]

//! The adapter, reachable as types rather than only as a formatted line
//! (issue #815, story #835).
//!
//! The desktop half of this check is
//! `crates/dashscene-desktop/tests/adapter_accessors.rs`, and the reasoning is
//! the same: `Surface` needs a canvas and a device, so what is checked is the
//! property the issue is about — that an embedder can reach the adapter and the
//! swapchain format **from outside this crate**, as the `wgpu` types, without
//! parsing `Surface::describe` and without declaring a `wgpu` dependency of its
//! own.
//!
//! # Why it is compiled and not run
//!
//! `Surface` is compiled for `wasm32-unknown-unknown` only — the whole `host`
//! module is — so a check of it on the host target would be a check of nothing,
//! and this file is empty on every other target. What compiles it for the one
//! where it says something is `just lint`, whose
//! `clippy -p dashscene-web --target wasm32-unknown-unknown --all-targets`
//! reaches every target of this crate including this one. Compiling is the
//! whole of the check; there is no browser here to run it in.

use dashscene_web::{AdapterInfo, Backend, DeviceType, Surface, TextureFormat};

/// The two accessors exist on `Surface` and hand back the `wgpu` types.
#[test]
fn the_adapter_and_the_format_are_reachable_as_wgpu_types() {
    let _adapter_info: fn(&Surface) -> &AdapterInfo = Surface::adapter_info;
    let _format: fn(&Surface) -> TextureFormat = Surface::format;
}

/// Branching on the adapter — warn on a software one, choose a path by format —
/// means naming the types `AdapterInfo`'s fields have, not only the struct. An
/// embedder that had to add `wgpu` to name them would also have to keep its
/// version in step with this crate's, so they are re-exported here.
#[test]
fn the_adapter_fields_are_nameable_without_a_wgpu_dependency() {
    let _backend: fn(&AdapterInfo) -> Backend = |info| info.backend;
    let _kind: fn(&AdapterInfo) -> DeviceType = |info| info.device_type;
}

/// The formatted line stays. `demo-web` logs it, and a caller that only wants
/// it should not have to build it from the parts.
#[test]
fn the_formatted_line_stays() {
    let _describe: fn(&Surface) -> String = Surface::describe;
}
