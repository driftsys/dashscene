//! The adapter, reachable as types rather than only as a formatted line
//! (issue #819, story #835).
//!
//! A type check rather than a behavioural one, because [`GpuPresenter`] needs a
//! window and no window exists under `cargo test`. A device does — several
//! `dashscene-gpu` tests build one — so what is out of reach here is the
//! surface, not the GPU. What needs neither is the property the issue is about:
//! that an embedder can reach the adapter and the swapchain format **from
//! outside this crate**, as the `wgpu` types, without parsing [`Present::name`]
//! and without declaring a `wgpu` dependency of its own.
//!
//! Each check names both paths — this crate's re-export and `dashscene-gpu`'s —
//! because a coercion against this crate's alias alone would still pass if the
//! re-export were replaced by a local type wearing the same name, which is the
//! one substitution that would make the whole argument for re-exporting false.
//! `dashscene-gpu` closes the last link to `wgpu` itself, beside its own
//! re-export.

use dashscene_desktop::{AdapterInfo, Backend, DeviceType, GpuPresenter, Present, TextureFormat};

/// The two accessors exist on `GpuPresenter` and hand back the `wgpu` types.
///
/// Inherent rather than on the `Present` trait, because `demo`'s raster
/// presenter has no adapter to answer with.
#[test]
fn the_adapter_and_the_format_are_reachable_as_wgpu_types() {
    let _adapter_info: fn(&GpuPresenter) -> &dashscene_gpu::AdapterInfo =
        GpuPresenter::adapter_info;
    let _format: fn(&GpuPresenter) -> dashscene_gpu::TextureFormat = GpuPresenter::format;
}

/// This crate's re-export is the painter's type, not a local one wearing its
/// name. An identity coercion compiles only if the two paths name one type.
#[test]
fn the_re_exports_are_the_painters_own_types() {
    let _info: fn(dashscene_gpu::AdapterInfo) -> AdapterInfo = |info| info;
    let _format: fn(dashscene_gpu::TextureFormat) -> TextureFormat = |format| format;
    let _backend: fn(dashscene_gpu::Backend) -> Backend = |backend| backend;
    let _kind: fn(dashscene_gpu::DeviceType) -> DeviceType = |kind| kind;
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

/// The formatted line stays. A caller that only wants it should not have to
/// build it from the parts, and the loop's own diagnostic lines read it.
#[test]
fn the_formatted_line_stays() {
    let _name: fn(&GpuPresenter) -> &str = <GpuPresenter as Present>::name;
}
