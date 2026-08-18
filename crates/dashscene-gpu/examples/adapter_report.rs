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
//! # What a passing result does not cover (issue #890)
//!
//! **A passing result covers the device request, on some adapter.** Three
//! things a host does are outside that, and the first two can each turn a
//! passing probe into a device-side failure.
//!
//! **1. The host picks one adapter; this probe passes if any adapter passes.**
//! `SurfaceRenderer::new_async` calls `request_adapter` with
//! `PowerPreference::default()` and a `compatible_surface`, so exactly one
//! adapter is chosen and it need not be the one that passed here. This
//! repository's own recorded emulator run is that case:
//! `docs/design/android-toolchain.md` shows adapter 0 (Vulkan, SwiftShader, a
//! CPU device) passing and adapter 1 (the GLES translator, an integrated GPU)
//! failing — and `PowerPreference::default()` is `LowPower`, which ranks an
//! integrated GPU above a CPU one. The summary line below says "at least one",
//! and it means exactly that.
//!
//! **2. The surface format.** `SurfaceRenderer::new_async` calls
//! `surface.get_capabilities(&adapter)` and refuses with
//! `RendererError::NoLinearFormat` when `linear_format` finds none
//! (`docs/decisions/pipelines-and-layer-3.md` D3 makes the blending space a
//! term of the contract rather than a preference). That check runs **before**
//! `Renderer::on_adapter`, so on a real host it is reached first — a passing
//! device request here does not even mean the host got as far as requesting a
//! device.
//!
//! Surface formats belong to a surface and a surface needs a window. This probe
//! enumerates adapters with no window at all — on Android it runs under `adb`
//! with no Activity — so it cannot ask. That is what issue #890 records.
//!
//! **3. Everything after the device request.** `Renderer::on_adapter` goes on to
//! build the shader module, the bind group layouts and the pipelines, and
//! `new_async` then calls `check_extent`. Issue #714 is a recorded failure of
//! that last one: a host aborted on the first resize past 2048 on a device
//! reporting a 16384 maximum.
//!
//! All three are stated rather than closed. Closing (1) and (2) needs a surface,
//! which needs a window on the target — a larger piece of work than the probe
//! is. `docs/design/android-toolchain.md` says the same under "What is not
//! measured".
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

/// Printed on every run, passing or failing — including the run that finds no
/// adapter at all, which exits before the loop.
///
/// A reader who takes a `device request OK` for "the painter runs here" has read
/// it as more than it is, and that misreading is the whole of issue #890. The
/// module doc above carries the detail; this is what an `adb` transcript keeps.
const CAVEAT: &str = "\
This probe covers the device request, on some adapter. It does NOT cover:
  1. WHICH adapter the host picks. `SurfaceRenderer::new_async` asks for one
     with PowerPreference::default() and a compatible surface; a different
     adapter from the passing one may be chosen. The summary below says
     at-least-one, and it means exactly that.
  2. The SURFACE FORMAT. `new_async` refuses with NoLinearFormat when no
     offered format survives `linear_format`, and that runs BEFORE the device
     is requested. It needs a surface, and this probe has no window.
  3. Pipeline creation and check_extent, which come after the device request.
";

fn main() {
    let instance = wgpu::Instance::default();
    // Async in wgpu 30, so it blocks here like every other native wait in this
    // crate.
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));

    if adapters.is_empty() {
        println!("adapter_report: no adapter on any backend");
        println!();
        println!("{CAVEAT}");
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

        // **What a GPU-timing probe would need, reported because it decides
        // whether one is possible at all** (v0.21). Vendor GPU counters are not
        // reachable on a retail Android build — `perfetto --query` registers no
        // `gpu.counters`, the `kgsl` ftrace tracepoints will not enable under
        // `traced_probes`, and `/sys/class/kgsl` is refused to `shell` with no
        // root. Timestamp queries are the remaining route to GPU execution time,
        // and they are a device **feature**, so an adapter that lacks them
        // forecloses it.
        //
        // Reported rather than requested: this probe replicates the painter's own
        // device request below, and asking for a feature the painter does not ask
        // for would make the verdict a different question's answer.
        let features = adapter.features();
        for (name, bit) in [
            ("TIMESTAMP_QUERY", wgpu::Features::TIMESTAMP_QUERY),
            (
                "TIMESTAMP_QUERY_INSIDE_ENCODERS",
                wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
            ),
            (
                "TIMESTAMP_QUERY_INSIDE_PASSES",
                wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES,
            ),
            (
                "PIPELINE_STATISTICS_QUERY",
                wgpu::Features::PIPELINE_STATISTICS_QUERY,
            ),
        ] {
            println!(
                "  {name:<32} {}",
                if features.contains(bit) { "yes" } else { "no" }
            );
        }

        // The painter's own request, replicated. `baked` is intersected rather
        // than required for the same reason `Renderer::on_adapter` intersects
        // it: a requested feature the adapter lacks fails the request outright,
        // and this painter draws without ASTC.
        let baked = features & wgpu::Features::TEXTURE_COMPRESSION_ASTC;
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
                println!("  device request OK");
            }
            Err(error) => println!("  device request FAILED — {error}"),
        }
        println!();
    }

    println!("{CAVEAT}");

    if any_usable {
        println!("adapter_report: at least one adapter satisfies the painter's device request");
    } else {
        // Non-zero so this is usable as a gate and not only as a report: a run
        // that finds nothing must fail rather than print and succeed.
        println!("adapter_report: NO adapter satisfies the painter's device request");
        std::process::exit(1);
    }
}
