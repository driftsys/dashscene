# The Android toolchain, and the D3a probe

    status  as-built at story #839 (v0.19, epic #833). The toolchain and the
            probe are built; **the D3a measurement on target hardware is not
            taken** — no device was available, and the deferral is recorded
            under "What is not measured" below.
    source  story #839
    why     [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
            carries the decisions; D3a is the one this record exists to serve.

Android was at zero before this: no target triple, no toolchain, no CI job, and
no FFI beyond `dashscene-unity`'s Unity-facing bindings.

## The toolchain

One target, `aarch64-linux-android`. Nothing else, because nothing needs it: an
Apple-silicon emulator is arm64 too, so one triple serves both a device and the
emulator, and a second is added when something asks for one.

**Plain `cargo build --target`, with the NDK's clang wired from the
environment.** Not `cargo-ndk`. Every other target in this repository is built
with plain cargo — `just wasm`, `wasm-painter`, `wasm-host` are all one cargo
line — and a wrapper would be the only one of its kind, to save three exported
variables.

**The NDK is a documented prerequisite, not something `bootstrap` installs.**
The same trade `web-build` makes for `wasm-bindgen-cli`: it is large, one target
needs it, and every clone paying for it would be the wrong cost. `just android`
discovers it and prints the `sdkmanager` line when it is missing.

Discovery order is `ANDROID_NDK_HOME`, then `ANDROID_NDK_ROOT` — the name
GitHub's runner images set, which is what lets one recipe serve CI and a
workstation — then the highest-versioned directory under the SDK's `ndk/`.
Highest rather than first, so a machine holding several does not silently pin
the oldest by sort order. The `llvm/prebuilt/*/bin` glob absorbs the host tag,
which is `darwin-x86_64` on Apple silicon and `linux-x86_64` on a runner.

### The API floor

`ANDROID_API` in the `justfile`, currently **26**. A floor rather than a
target — the oldest device the artifacts will load on — and deliberately not
higher, because nothing built so far needs more.

**Story #841 may raise it.** `AChoreographer_getInstance` is API 24, but
`AChoreographer_postVsyncCallback` — the one carrying a frame timeline — is API
33, and D6 puts vsync on the native side. Raising the floor is a one-line
change, so it is left where the evidence is rather than guessed upward now.

## What builds

`just android` cross-compiles `dashscene-gpu` — the painter, the whole of what
a host draws through. It compiled without a source change, pulling in `ndk-sys`,
`jni-sys`, `gpu-allocator` and `wgpu-core-deps-windows-linux-android`, so wgpu's
Android backend support needed nothing from this repository.

The CI job `android-build` runs exactly that and no more. A runner has no device
and no GPU, so nothing there can measure D3a; a job that appeared to would be
the `t2-check-has-no-teeth` failure the v0.13 tiering exists to remove.

## The probe

`crates/dashscene-gpu/examples/adapter_report.rs`, run by `just android-probe`,
which cross-compiles it, pushes it to `/data/local/tmp` and runs it through
`adb`. A plain executable rather than an APK: adapter enumeration needs no
window and no Java, which is what makes the probe available before any of the
Android host exists.

**It replicates the painter's own `request_device`** rather than comparing two
numbers. `Renderer::on_adapter` asks for
`wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())`, and a
device that cannot meet it fails **at the device request** — earlier than
pipeline creation. So the verdict the probe prints is the painter's own.

It is an example rather than a new workspace member because it measures
`dashscene-gpu`'s own requirement against a device, and because a crate would
cost the registries a new crate has to be added to — thirteen of them when
story #794 added `dashscene-desktop`, enumerated in
[`../decisions/crate-name-map.md`](../decisions/crate-name-map.md).

## What was measured, and what it does not say

**Host, for comparison** — `cargo run -p dashscene-gpu --example adapter_report`
on the development machine:

    backend Metal, Apple M3, IntegratedGpu
    max_storage_buffers_per_shader_stage 29
    device request OK

**Automotive emulator** — `Automotive_1408p_landscape_with_Google_Play_API_34-ext9`,
API 34, on that same machine. **This is an emulator result and is not the D3a
measurement**; both adapters below describe the host machine's GPU and its
translation layer, not a target device.

    adapter 0  Vulkan, SwiftShader Device (LLVM 10.0.0), Cpu
               max_storage_buffers_per_shader_stage 10
               device request OK

    adapter 1  Gl, Android Emulator OpenGL ES Translator (Apple M3), IntegratedGpu
               driver OpenGL ES 3.0 (4.1 Metal - 89.4)
               max_storage_buffers_per_shader_stage 0
               device request FAILED

**The GLES adapter reporting zero is D3a's mechanism, demonstrated.** D3a says a
device without Vulkan meets the same wall that makes WebGL2 unbuildable for this
painter, where `downlevel_webgl2_defaults` allows zero storage buffers. That
adapter translates OpenGL ES 3.0, and shader storage buffers arrived in GLES
3.1 — so zero is correct rather than a reporting gap, and the painter cannot run
on it.

One precision about the failure line: wgpu names the first limit that fails,
which here was `max_compute_workgroups_per_dimension`, not the storage-buffer
one. The GLES 3.0 adapter reports zero for a whole family of limits. The request
failed and the storage-buffer limit is zero; the request did not fail
_because of_ the storage-buffer limit specifically, and this record does not
claim it did.

### The emulator cannot be made to use the host GPU for Vulkan

Worth recording so it is not retried. The emulator ships `Vulkan = off` in its
own `lib/advancedFeatures.ini`, which reads like the reason its Vulkan adapter
is a CPU rasteriser. It is not. Setting `Vulkan = on` in
`~/.android/advancedFeatures.ini` and restarting applies the flag —

    Feature 'Vulkan' (21) is overridden to 'enabled'

— and the emulator then selects SwiftShader anyway, because it sets the ICD to
SwiftShader explicitly rather than looking for a host driver:

    initIcdPaths: ICD set to 'swiftshader', using Swiftshader ICD
    Selecting Vulkan device: SwiftShader Device (LLVM 10.0.0), Version: 1.3.0
    useVulkanComposition: false
    useVulkanNativeSwapchain: false

The probe reports byte-identical output with the flag on and off. macOS exposes
no native Vulkan ICD, and this emulator build does not route to MoltenVK, so
there is nothing for the flag to select. The override was reverted.

**The consequence is the one that matters for planning.** On this emulator the
only adapter the painter can use is a CPU rasteriser, and the only adapter on
the host GPU exposes zero storage buffers. So the emulator is usable for
checking that something _works_ and is useless for checking how fast it is —
which is a second, independent reason story #842's frame-rate measurement waits
for hardware rather than being approximated here.

### What is not measured

**Whether the target device class exposes Vulkan.** That is the D3a question and
it needs hardware, which was not available (expected roughly 2026-08-23). The
emulator answers only that the probe detects the failure mode, and that a
software Vulkan implementation satisfies the painter.

Until the hardware measurement exists, **nothing may describe Android as
working** — not this record, not a design document, not an issue. D3a's status
in
[`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
stays "a risk to check", and story #839 carries the step that changes it.
