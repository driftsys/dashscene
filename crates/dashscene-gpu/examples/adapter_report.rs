//! Reports what an adapter exposes, and whether this painter's own device
//! request succeeds on it.
//!
//! This exists to answer D3a of
//! `docs/decisions/host-integration-in-three-layers.md`, which is recorded
//! there as **a risk to check rather than a measured fact**: that a device
//! without Vulkan meets the same wall that makes WebGL2 unbuildable for this
//! painter. The figure lives in a driver and in the GLES specification rather
//! than in the pinned crate, and this project's rule is to read a limit out of
//! the thing that enforces it.
//!
//! # What it reports, and why that is the right question
//!
//! The pipeline binds seven storage buffers — three in the vertex stage and
//! four in the fragment stage — and `wgpu::Limits::downlevel_defaults` allows
//! four **per stage**, so the fragment side has no headroom (`render.rs`, the
//! comment above `PAINT_WGSL`'s bind group). GLES makes fragment-stage storage
//! buffers optional in a way desktop Vulkan does not.
//!
//! So the number alone is not the answer. `Renderer::on_adapter` asks for
//! `downlevel_defaults().using_resolution(adapter.limits())`, and a device that
//! cannot meet it fails **at `request_device`** rather than later at pipeline
//! creation. This example makes that same request, so the verdict is the
//! painter's own rather than a comparison of two numbers that might not be the
//! ones that bind.
//!
//! # Running it
//!
//! Natively, `cargo run -p dashscene-gpu --example adapter_report`. On Android
//! it is cross-compiled, pushed and run through `adb`, which `just
//! android-probe` does in one step. Running it on the host as well as on a
//! device is deliberate —
//! the two reports are directly comparable, which is what makes a device
//! result legible.
//!
//! An emulator result describes the host machine's GPU, not a target device,
//! and must never be recorded as the D3a measurement.

fn main() {
    let instance = wgpu::Instance::default();
    // Async in wgpu 30, so it blocks here like every other native wait in this
    // crate.
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));

    if adapters.is_empty() {
        println!("adapter_report: no adapter on any backend");
        std::process::exit(1);
    }

    let mut any_usable = false;

    for (index, adapter) in adapters.iter().enumerate() {
        let info = adapter.get_info();
        let limits = adapter.limits();

        println!("adapter {index}");
        println!("  backend     {:?}", info.backend);
        println!("  name        {}", info.name);
        println!("  device type {:?}", info.device_type);
        println!("  driver      {} {}", info.driver, info.driver_info);
        // The limit D3a is about. Read off the adapter rather than taken from
        // `downlevel_defaults`, which is what the painter asks *for* and not
        // what the device *has*.
        println!(
            "  max_storage_buffers_per_shader_stage {}",
            limits.max_storage_buffers_per_shader_stage
        );

        // The painter's own request, replicated. `baked` is intersected rather
        // than required for the same reason `Renderer::on_adapter` intersects
        // it: a requested feature the adapter lacks fails the request outright,
        // and this painter draws without ASTC.
        let baked = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_ASTC;
        let requested = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("adapter_report"),
            required_features: baked,
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            ..Default::default()
        }));

        match requested {
            Ok(_) => {
                any_usable = true;
                println!("  device request OK — this painter runs on this adapter");
            }
            Err(error) => println!("  device request FAILED — {error}"),
        }
        println!();
    }

    if any_usable {
        println!("adapter_report: at least one adapter satisfies the painter's device request");
    } else {
        // Non-zero so this is usable as a gate and not only as a report: a run
        // that finds nothing must fail rather than print and succeed.
        println!("adapter_report: NO adapter satisfies the painter's device request");
        std::process::exit(1);
    }
}
