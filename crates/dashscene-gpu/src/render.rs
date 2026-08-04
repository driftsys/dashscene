//! The device, the pipeline, and the frame path (stories #580 and #585).
//!
//! # What this draws, and what it does not
//!
//! Rounded rects with a solid fill, their outline stroke and their image fill,
//! positioned glyph runs, and a solid fill masked by a baked vector field — all
//! clipped by their region. Gradients are issue #715's, shadows and backdrop
//! blur story #584's, group opacity #583's.
//!
//! A stroke is one of two kinds whose ink does not coincide with the instance's
//! own `bounds` — see `instance_outset` in `shaders/paint.wgsl`, which grows the
//! quad so an Outside stroke is not clipped by its own geometry. The other is a
//! masked instance, whose quad is the coverage field's padded plane quad
//! *instead of* the node's box, and which the vertex stage substitutes there.
//!
//! An instance whose kind this shader does not implement draws nothing. It does
//! not fall through to a colour: [`InstanceKind`](crate::InstanceKind) carries
//! the sub-kind, so a shader that reads the discriminant alone cannot resolve a
//! shadow against the solid-fill table — the collision that made story #580
//! paint an inner shadow from `solids[shadow_row]` is unrepresentable now.
//!
//! # Two targets, one device
//!
//! This renderer draws into a texture view. Which view is the caller's:
//! [`Renderer::render`] makes its own offscreen one and reads the pixels back,
//! which is what lets layer 3 run as an ordinary test; [`crate::surface`] hands
//! it a window's swapchain texture, which is how the host draws (story #585).
//! Everything between the two — the device, the pipeline, the buffers and the
//! upload — is the same code, so the picture the host shows is drawn by the
//! path the tests exercise.
//!
//! # The frame path allocates nothing (R-T4)
//!
//! `docs/specification/03-target-hardware-rules.md` R-T4 bounds per-frame CPU
//! cost to "dirty-range instance-buffer upload from the rect table +
//! submission. Nothing else." Until story #585 this call allocated four
//! buffers, a texture, a view and a bind group **per frame**, because its only
//! caller rendered one frame and then dropped the renderer. It now holds them
//! across frames, grows them only when a frame outgrows one, and uploads only
//! the byte ranges the dirty rects name — see [`Frame::upload_instances`] for
//! the condition under which a partial upload is sound, and for the check that
//! fires when it is not.
//!
//! # Layer 3 is a gate on the pipeline, not a fidelity check
//!
//! `docs/decisions/shader-library-and-layer-2.md` draws the line and epic #569
//! insists on it: that pipelines build, that naga validates the modules, that
//! coverage is high inside a shape and zero outside it, and that a clip
//! rejects. None of that says how the painter looks on a real driver, which is
//! layer 4's job and needs hardware.

use bytemuck::{Pod, Zeroable};
use dashpaint::{ClipTable, GlyphRunTable, ImageTable, PaintTable, ScaleMode};

use crate::instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan};
use crate::residency::{PayloadKey, Residency, ResidencyError};

/// The viewport uniform the shaders read.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Pod, Zeroable)]
struct Viewport {
    size: [f32; 2],
    aa: f32,
    _pad: f32,
}

/// One stroke, in the shader's own layout.
///
/// `dashpaint::Stroke` is `{f32, StrokeAlign, Color}`, which is 24 bytes with a
/// Rust-layout enum in the middle. This is the std430 shape the shader reads:
/// the colour first so it sits at a 16-byte offset, then the width and the
/// alignment as a plain `u32`.
///
/// The alignment is mapped by an exhaustive `match` rather than by `as u32`
/// (see [`stroke_align`]). A copy through a local type rather than a cast, for
/// the reason [`GpuClipBox`] gives.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuStroke {
    color: [f32; 4],
    width: f32,
    align: u32,
    _pad: [u32; 2],
}

/// The value [`GpuStroke::align`] carries, and the one place the mapping is
/// written.
///
/// An exhaustive `match`, never `align as u32`: a reordered variant in
/// `dashpaint` would silently change the number a shader compares against, and
/// nothing would catch it — not the compiler, not a golden, which pins the
/// packer's output rather than the shader's reading of it. This is the same
/// hazard `InstanceKind` was merged into one enum to remove
/// (`crates/dashscene-gpu/src/instance.rs`). A new alignment is a compile error
/// here.
///
/// The numbers are the ones `sdf.wgsl`'s `stroke_coverage` documents, and the
/// ones its layer-2 conformance suite is stated over.
const fn stroke_align(align: dashpaint::StrokeAlign) -> u32 {
    match align {
        dashpaint::StrokeAlign::Inside => 0,
        dashpaint::StrokeAlign::Center => 1,
        dashpaint::StrokeAlign::Outside => 2,
    }
}

/// One image fill, in the shader's own layout, with its residency slot resolved
/// into it.
///
/// `dashpaint::ImageFill` names an image-table index; this names a rectangle of
/// the atlas that index was made resident in. That resolution is the whole of
/// what residency adds to a frame, and it happens once per table row rather
/// than once per instance.
///
/// The extent comes from the payload rather than from the slot, so that the two
/// cannot disagree: a slot's rectangle is where the texels are, and `size` is
/// how many of them there are.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuImage {
    /// The payload's rectangle in its atlas, normalised: `[u0, v0, du, dv]`.
    uv: [f32; 4],
    /// `Mat23`'s linear part, row-major: `[a, b, c, d]`.
    transform: [f32; 4],
    /// `Mat23`'s translation.
    translate: [f32; 2],
    /// The payload's extent in texels.
    size: [f32; 2],
    scale_mode: u32,
    tile_scale: f32,
    _pad: [u32; 2],
}

/// The value [`GpuImage::scale_mode`] carries.
///
/// An exhaustive `match`, never `mode as u32`, for the reason [`stroke_align`]
/// gives: a reordered variant in `dashpaint` would change the number the shader
/// compares against and nothing would catch it.
const fn scale_mode(mode: ScaleMode) -> u32 {
    match mode {
        ScaleMode::Fill => 0,
        ScaleMode::Fit => 1,
        ScaleMode::Crop => 2,
        ScaleMode::Tile => 3,
    }
}

/// One glyph run, in the shader's own layout, with its atlas's residency slot
/// resolved into it.
///
/// Per run rather than per glyph: the colour, the screen-pixel range and the
/// atlas mapping are constant across a run, and the one thing that is not — the
/// glyph's own rectangle — rides on [`Instance::corners`], which a glyph has no
/// other use for.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuGlyphRun {
    /// The run's fill colour, with its free-path alpha still on
    /// [`Instance::opacity`]. The MSDF coverage modulates it.
    color: [f32; 4],
    /// Source-atlas texels to residency-atlas normalised coordinates:
    /// `[origin_u, origin_v, scale_u, scale_v]`, so texel `t` of the run's own
    /// atlas sits at `origin + t * scale`.
    ///
    /// Two mappings composed on the CPU rather than two in the shader: the
    /// atlas's own extent normalises the texel, and the residency slot places
    /// that normalised point inside the atlas texture. Both are constant per
    /// run.
    uv: [f32; 4],
    /// Half a source texel, in residency-atlas normalised units — what a sample
    /// is held inside the glyph's own rectangle by.
    ///
    /// Before [`px_range`](Self::px_range), not after it. WGSL aligns a `vec2f`
    /// to eight bytes, so the other order puts it at offset 40 with a hole at
    /// 36 and rounds the struct to 64 — while Rust packs it at 36 and makes the
    /// struct 48. Every row after the first would then be read from the wrong
    /// offset. The `size_of` assertion at the foot of this file is what holds
    /// the Rust half of that; this ordering is the WGSL half.
    half_uv: [f32; 2],
    /// The field's range in **screen** pixels:
    /// `distance_range_px * size / px_per_em`, which is `dashscene-skia`'s own
    /// formula. The painter draws at unit scale, so the run's size in document
    /// units is its size in pixels.
    px_range: f32,
    _pad: f32,
}

/// One baked-vector coverage mask, in the shader's own layout, with its atlas's
/// residency slot resolved into it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuShape {
    /// The padded field quad in shape space, node-box-relative and y-down:
    /// `[left, top, right, bottom]`, straight from
    /// `dashpaint::VectorField::plane_bounds`. The device quad is the node's
    /// origin plus this, at unit scale.
    plane: [f32; 4],
    /// The shape's sub-rect in its residency atlas, normalised:
    /// `[u0, v0, du, dv]`.
    uv: [f32; 4],
    /// Half an atlas texel, in residency-atlas normalised units. Before
    /// `px_range` for the alignment reason [`GpuGlyphRun::half_uv`] gives.
    half_uv: [f32; 2],
    /// The field's range in screen pixels: `distance_range` scaled by the
    /// device pixels one atlas texel covers. That scale is the field's own
    /// quad over its atlas rectangle — a vector field carries no `px_per_em`,
    /// because that ratio already is the scale.
    px_range: f32,
    _pad: f32,
}

/// One clip box, in the shader's own layout.
///
/// `dashpaint::ClipBox` is `{f32 x4, CornerRadii}` and is already exactly this
/// shape, but it is copied through a local type rather than cast: boundary B's
/// row is a contract with every painter, and a std430 array stride is this
/// painter's business. Tying them together would make a layout change in one a
/// silent change in the other.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuClipBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    corners: [f32; 4],
}

/// A device, a queue, the one pipeline, and the buffers a frame reuses.
pub struct Renderer {
    /// Held because a [`wgpu::Surface`] is created from it and must not outlive
    /// it. The offscreen path needs it only to build the adapter, but keeping
    /// it here means there is one lifetime rule rather than two.
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    adapter_info: wgpu::AdapterInfo,
    frame: Frame,
    /// The colour format the pipeline writes. [`TARGET_FORMAT`] offscreen, the
    /// window's own format behind a surface — both sRGB-encoded, which is what
    /// `docs/decisions/pipelines-and-layer-3.md` D3 requires.
    format: wgpu::TextureFormat,
    /// The offscreen target [`Renderer::render`] draws into, kept across calls
    /// for the same reason the frame buffers are, and rebuilt when the extent
    /// changes.
    offscreen: Option<Offscreen>,
    /// The largest either dimension of a drawable may be on this device — the
    /// device's own `max_texture_dimension_2d`. Copied out at construction
    /// rather than read per call: it cannot change, and `wgpu::Device::limits`
    /// returns the whole limit set by value.
    max_extent: u32,
    /// Device objects allocated for the offscreen target, counted beside
    /// [`Frame::allocations`] — see [`Renderer::allocations`].
    offscreen_allocations: u64,
    /// Which payloads are on the device and where (story #581).
    residency: Residency,
    /// The sampler an image fill's payload is read through: nearest, clamped.
    /// See [`crate::residency`] for why nearest, and for what changing it costs.
    sampler: wgpu::Sampler,
    /// The sampler an MSDF payload — a glyph atlas or a coverage mask — is read
    /// through: linear, clamped. Built where it is, with the reason.
    msdf_sampler: wgpu::Sampler,
    /// A 1x1 texture bound when a frame samples no atlas at all.
    ///
    /// A bind group must name a texture for every texture binding its layout
    /// declares, and a frame with no image fills has no atlas to name. Building
    /// a second pipeline for the textureless case would be a second thing to
    /// keep in step with the first.
    placeholder: wgpu::TextureView,
}

/// What a renderer could not be built for, or could not be asked to draw.
#[derive(Debug)]
pub enum RendererError {
    /// No adapter at all — a machine or a runner with no GPU and no software
    /// device installed.
    NoAdapter,
    /// An adapter that will not give a device at the limits this painter needs.
    NoDevice(wgpu::RequestDeviceError),
    /// The window handle produced no surface.
    NoSurface(wgpu::CreateSurfaceError),
    /// Every format the surface offers converts to sRGB in the hardware, so
    /// blending would happen in linear light.
    ///
    /// Refused rather than accepted, because
    /// `docs/decisions/pipelines-and-layer-3.md` D3 makes the blending space a
    /// term of the contract and measures the two spaces roughly 50 code points
    /// apart across a saturated seam. A picture that is wrong in a way nobody
    /// named is worse than a window that did not open.
    NoLinearFormat(Vec<wgpu::TextureFormat>),
    /// A drawable larger on either axis than [`Renderer::max_extent`], the
    /// maximum this device can address.
    ///
    /// Reported *before* the call that would fail rather than caught after it,
    /// because there is nothing to catch. Both `wgpu::Surface::configure` and
    /// `wgpu::Device::create_texture` raise a validation error for an
    /// over-large extent, and a wgpu validation error reaches the uncaptured
    /// error handler, which panics; inside the swapchain configure that panic
    /// is non-unwinding and takes the process down with it. Issue #714 aborted
    /// the showcase host that way on an ordinary window resize.
    Extent { width: u32, height: u32, max: u32 },
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::NoAdapter => write!(
                f,
                "no wgpu adapter is available; on a runner this means no software device is \
                 installed (CI installs mesa-vulkan-drivers)"
            ),
            RendererError::NoDevice(e) => write!(f, "the adapter provided no device: {e}"),
            RendererError::NoSurface(e) => write!(f, "the window provided no surface: {e}"),
            RendererError::NoLinearFormat(offered) => write!(
                f,
                "the surface offers only sRGB-converting formats ({offered:?}); this painter \
                 blends in sRGB-encoded space and has no format to do it in"
            ),
            RendererError::Extent { width, height, max } => write!(
                f,
                "a {width}x{height} drawable exceeds the {max} px maximum this device can \
                 address on either dimension"
            ),
        }
    }
}

impl std::error::Error for RendererError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RendererError::NoDevice(e) => Some(e),
            RendererError::NoSurface(e) => Some(e),
            RendererError::NoAdapter
            | RendererError::NoLinearFormat(_)
            | RendererError::Extent { .. } => None,
        }
    }
}

/// What a caller knows about how this frame differs from the last one.
///
/// # Why the generation travels with the dirty set
///
/// A dirty set names the rects whose entry differs **from the commit before
/// it**. That makes a partial upload sound only when the device holds the
/// commit immediately before this one — and a host cannot promise that. Story
/// #585's own presenter breaks it three ways: a swapchain acquire can time out,
/// a window can be occluded, and a minimised window has no drawable. Each of
/// those declines a frame while the host still records the commit as shown, and
/// the next commit's dirty set will not mention what the declined one changed.
///
/// It is not a theoretical gap. It was found by running the showcase for two
/// minutes: a spring's *last* step landed on a declined frame, the value then
/// converged and never changed again, and the device kept a rect 0.02 units too
/// narrow with no later frame that could correct it. Invisible in the picture,
/// permanent, and caught only because the renderer checks itself.
///
/// Carrying the generation makes the gap unrepresentable rather than forbidden:
/// the renderer applies ranges only when this frame is the immediate successor
/// of the one on the device, and writes everything otherwise. A caller that
/// skips a frame, restarts an arena, or hands over frames out of order gets a
/// correct picture without having to know that it did any of those things.
#[derive(Debug, Clone, Copy)]
pub struct Changes<'a> {
    /// Boundary B's advisory dirty set: sorted rect indices whose entry differs
    /// from the previous commit's.
    pub rects: &'a [u32],
    /// The commit these rects were reported against —
    /// `dashscene_core::CommittedScene::generation`.
    pub generation: u64,
}

/// How one frame's instance rows reached the device.
///
/// Public because it is the instrument the frame-path tests are stated over,
/// and there is no other way to tell the two paths apart from outside: both
/// draw the same picture, which is the whole point of the partial one. A test
/// that asserted only the picture would pass just as happily if every frame
/// quietly wrote the whole buffer — the exact green-for-the-wrong-reason this
/// crate has already been caught by twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceUpload {
    /// Every row was written: the first frame, a frame that outgrew the buffer,
    /// a frame whose spans moved, a frame that does not follow the one on the
    /// device, or a caller that passed no [`Changes`] at all.
    Whole { rows: usize },
    /// Only the ranges the dirty rects named, as a count of `write_buffer`
    /// calls and of rows. Zero of both is a frame that redrew the commit the
    /// device already held.
    Ranges { ranges: usize, rows: usize },
}

/// The texture format the renderer draws into and reads back.
///
/// `Rgba8Unorm` rather than `Rgba8UnormSrgb`: this painter blends in
/// sRGB-encoded space, which `docs/decisions/blur-blends-in-srgb-encoded-space.md`
/// makes a term of the boundary-B contract rather than a per-painter choice. A
/// `Srgb` format would have the hardware convert on write and blend in linear
/// light, which is the divergence that record measures at roughly 50 code
/// points across a saturated seam.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// How many texels on a side each residency atlas is, before the device's own
/// maximum is applied.
///
/// # Why a budget rather than the device's maximum
///
/// An atlas is allocated whole, the first time a payload of its format appears,
/// so its extent is a memory commitment and not a ceiling: 2048 square is 16 MiB
/// of `Rgba8Unorm`, and the 16384 an Apple M3 reports would be 1 GiB. That is the
/// opposite of the question [`Renderer::max_extent`] answers, which is how large
/// a *drawable* the hardware can address — issue #714 took that one from the
/// adapter deliberately, and this one must not follow it.
///
/// 2048 is `wgpu::Limits::downlevel_defaults`' own texture maximum, which makes
/// it the largest atlas the entry-tier floor this painter targets is guaranteed
/// to hold. It is stated here as a number with a reason rather than read back
/// out of `downlevel_defaults`, because the device is no longer requested at
/// those limits and reading it there would say something untrue.
///
/// A payload larger than this is refused by name
/// ([`crate::ResidencyError::TooLarge`]) rather than downscaled or tiled. On a
/// device that could hold it that is a real limitation, and issue #720 carries
/// it: the fix is a dedicated texture outside the atlas, not a bigger atlas.
pub const ATLAS_EXTENT: u32 = 2048;

impl Renderer {
    /// Acquires an adapter and builds the pipeline, drawing offscreen.
    ///
    /// Fallible where the conformance harness panics, because a renderer is
    /// something a host constructs and a host can report; the harness is a test
    /// and a missing device there is the runner being wrong.
    pub fn new() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| RendererError::NoAdapter)?;
        Self::on_adapter(instance, adapter, TARGET_FORMAT)
    }

    /// Requests a device from `adapter` and builds everything over it.
    ///
    /// Shared with the surface path, which differs only in how the adapter was
    /// chosen — compatible with a window — and in the format the pipeline
    /// writes.
    pub(crate) fn on_adapter(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let adapter_info = adapter.get_info();
        // ASTC when the adapter has it, nothing else. A requested feature the
        // adapter lacks fails the request outright, so this is intersected
        // rather than asked for — the painter draws on an adapter without it,
        // and says so through `GpuPainter::samples` instead of failing to
        // start. It has to be *requested*, though: a feature the adapter
        // advertises and the device did not ask for is not a feature the device
        // has, and the atlas texture is created on the device.
        let baked = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_ASTC;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dashscene-gpu"),
            required_features: baked,
            // Downlevel defaults, so this painter runs on the entry-tier class
            // of device R3 names rather than only on a desktop one — but with
            // the adapter's own resolution limits rather than downlevel's,
            // which cap `max_texture_dimension_2d` at 2048.
            //
            // A drawable's size is a property of the window the host opened
            // rather than of the features this painter uses, and a 2288x1410
            // window is an ordinary one: issue #714 aborted the host on the
            // first resize past 2048 on a device whose own maximum is 16384.
            // An entry-tier adapter still reports its own smaller maximum
            // here, so the painter stays bounded by the real constraint rather
            // than by a synthetic one — which is what `using_resolution` is
            // for, and it leaves every other downlevel limit in place.
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .map_err(RendererError::NoDevice)?;

        // The shader library and the render entry points, concatenated. Naga
        // validates the result when the module is created, which is the "naga
        // validates" half of layer 3.
        let source = format!(
            "{}\n{}",
            crate::shader::SDF_WGSL,
            include_str!("shaders/paint.wgsl")
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dashscene-gpu paint"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        // Visibility is per binding, and it is a correctness constraint rather
        // than a tidiness one: `wgpu::Limits::downlevel_defaults` allows four
        // storage buffers **per shader stage**, and this pipeline binds seven.
        // Declaring each where it is actually read is what makes seven fit:
        //
        //     vertex    instances(0), strokes(4), glyph runs(8), shapes(9)
        //     fragment  solids(1), clips(2), strokes(4), images(5)
        //
        // Four and four, with nothing spare on either side. The fragment stage
        // reads no instance array at all; `VertexOut` in `shaders/paint.wgsl`
        // carries the values it needs across, and story #581 is why.
        //
        // Story #582's two tables took the same route deliberately. A glyph
        // run's parameters and a coverage mask's are five and eleven floats
        // that are **constant across the instance**, so the stage that runs
        // four times per quad can read them and hand the fragment stage the
        // values — which costs the fragment stage no binding at all. That works
        // because neither is a variable-length array. A gradient's stops are
        // (issue #715), which is why that story still has to change the
        // structure rather than take this route a third time
        // (`docs/decisions/tables-the-vertex-stage-reads.md`).
        let storage = |binding: u32, visibility: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dashscene-gpu paint"),
            entries: &[
                // The instance rows: vertex only, since story #581.
                storage(0, wgpu::ShaderStages::VERTEX),
                storage(1, wgpu::ShaderStages::FRAGMENT),
                storage(2, wgpu::ShaderStages::FRAGMENT),
                // The stroke rows are the one table both stages read: the
                // vertex stage for the quad's outset, the fragment stage for
                // the band and its colour.
                storage(4, wgpu::ShaderStages::VERTEX_FRAGMENT),
                storage(5, wgpu::ShaderStages::FRAGMENT),
                // Story #582's two tables, vertex only — see the comment above.
                storage(8, wgpu::ShaderStages::VERTEX),
                storage(9, wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // The atlas, and the sampler it is read through. One texture
                // binding rather than one per format: a frame that needs two
                // atlases is drawn as two runs over the same pipeline, because
                // `wgpu::Limits::downlevel_defaults` has no binding arrays and
                // `docs/decisions/pipelines-and-layer-3.md` D2 holds this
                // painter to those limits.
                // Declared filterable because binding 10 filters it. That is a
                // constraint on the atlas *formats*, and every format this set
                // holds meets it: `Rgba8Unorm` is filterable on every adapter,
                // and an ASTC format is filterable wherever
                // `TEXTURE_COMPRESSION_ASTC` is supported at all — which is the
                // only condition under which one of those textures exists here.
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dashscene-gpu paint"),
            // Option-wrapped since wgpu 30; `immediate_size` is its
            // replacement for push constants and this pipeline uses none.
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dashscene-gpu paint"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // No vertex buffers: the quad's corners come from the vertex
                // index and the instance's own bounds, so a frame uploads the
                // instance rows and nothing else. That is what R-T4 bounds the
                // per-frame cost to.
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Premultiplied source-over: the fragment shader multiplies
                    // colour by alpha, so the source factor is one.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let max_extent = device.limits().max_texture_dimension_2d;
        // Nearest and clamped, matching the reference painter's own
        // `SamplingOptions::default()`. Declared `NonFiltering` in the layout to
        // match, which also means an atlas texture needs no `filterable` format
        // capability — `Rgba8Unorm` has it anyway, and an ASTC format's
        // filterability is a device property this painter does not have to ask
        // about.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dashscene-gpu atlas"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        // The second sampler, and the one thing it is for.
        //
        // A distance field is not a colour. `dashscene-skia` samples its MSDF
        // atlases `Linear` and its image fills `Nearest`, deliberately and for
        // two different reasons, and this painter needs both for the same two.
        // Nearest on a distance field quantises the edge ramp to the atlas's
        // own texel grid: at a 48-unit render size off a 32 px/em atlas one
        // texel covers 1.5 pixels while the ramp is 6 pixels wide, so a smooth
        // edge becomes a four-step staircase.
        //
        // The gutter `crate::residency` names as the first thing to add if
        // filtering arrived is **not** needed for this, and that is a property
        // of the read rather than of the allocator: `msdf_sample` in
        // `shaders/paint.wgsl` clamps half a source texel inside the payload's
        // own sub-rect, and a bilinear footprint taken from there weights only
        // texels of that payload. It is the same clamp `image_colour` already
        // relies on for the nearest case, doing more work than it had to.
        let msdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dashscene-gpu msdf"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let placeholder = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("dashscene-gpu no atlas"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Every atlas is this many texels on a side, and never more than the
        // device will give.
        //
        // Clamped rather than taken from the device, which is the opposite of
        // what `max_extent` above does and is deliberate: an atlas is a
        // *budget*, not a maximum. Sizing it by the adapter would ask a
        // 16384-capable device for a 1 GiB texture the moment one image fill
        // appeared. `ATLAS_EXTENT` is what that budget is and says why.
        let residency = Residency::new(ATLAS_EXTENT.min(max_extent));

        let frame = Frame::new(&device, &layout, &sampler, &msdf_sampler, &placeholder);
        Ok(Self {
            _instance: instance,
            device,
            queue,
            pipeline,
            layout,
            adapter_info,
            frame,
            format,
            offscreen: None,
            offscreen_allocations: 0,
            max_extent,
            residency,
            sampler,
            msdf_sampler,
            placeholder,
        })
    }

    /// The adapter this renderer runs on, for a measurement to be recorded
    /// beside.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// The largest either dimension of a drawable may be — a texture rendered
    /// into, or a window's swapchain.
    ///
    /// This is the adapter's own `max_texture_dimension_2d` and not a number
    /// this painter chose. The device is requested at downlevel limits with the
    /// adapter's resolution, so a drawable is bounded by the hardware rather
    /// than by the entry-tier feature floor the painter targets.
    pub fn max_extent(&self) -> u32 {
        self.max_extent
    }

    /// Refuses a drawable this device cannot address, on either axis.
    ///
    /// Every caller that is about to hand an extent to `wgpu` goes through
    /// here first. See [`RendererError::Extent`] for why the check is made
    /// ahead of the call rather than around it.
    pub(crate) fn check_extent(&self, width: u32, height: u32) -> Result<(), RendererError> {
        if width > self.max_extent || height > self.max_extent {
            return Err(RendererError::Extent {
                width,
                height,
                max: self.max_extent,
            });
        }
        Ok(())
    }

    /// Whether this device can hold an ASTC block texture at all.
    ///
    /// Asked of the device rather than the adapter: a feature the adapter
    /// advertises but that was not requested is not a feature the device has,
    /// and it is the device the atlas is created on.
    pub fn samples_astc(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC)
    }

    /// How this frame's instance rows reached the device.
    ///
    /// Reports the frame most recently drawn; before the first, the whole of
    /// nothing.
    pub fn last_instance_upload(&self) -> InstanceUpload {
        self.frame.last_upload
    }

    /// Forgets what the device holds, so the next frame is written whole.
    ///
    /// A caller must call this when the commits it is about to hand over come
    /// from a **different chain** than the ones before them — a document
    /// replaced, an arena rebuilt, a scene swapped. [`Changes`] carries a
    /// generation, and a generation is only meaningful within one chain: a
    /// fresh arena counts from the start, so its generation *G+1* can follow
    /// the old arena's *G* by arithmetic while naming a completely different
    /// picture. Nothing in the rows themselves distinguishes the two, and the
    /// spans of one scene rebuilt at a new extent are identical.
    ///
    /// The host is the only thing that knows, which is why this is a call and
    /// not a check.
    pub fn forget_uploaded(&mut self) {
        self.frame.uploaded.clear();
        self.frame.spans.clear();
        self.frame.uploaded_generation = None;
        // Residency is keyed by the image table's own row, and a rebuilt arena
        // starts that table again from zero — so the same key can name a
        // different picture across this call. See `Residency::forget_resident`.
        self.residency.forget_resident();
    }

    /// Every device object this renderer has allocated since it was built —
    /// buffers, textures, views and bind groups.
    ///
    /// R-T4 budgets a steady-state frame for a dirty-range upload and a
    /// submission, so the number a test should see is one that stops moving:
    /// it rises while a frame outgrows a buffer or changes extent, and not
    /// otherwise. It is a counter rather than a claim in a comment because the
    /// per-frame allocation this replaced looked exactly like correct code
    /// while it ran.
    pub fn allocations(&self) -> u64 {
        // Residency's textures and views are counted here rather than only on
        // its own getter. They were not, and the omission had teeth: the test
        // that asserts a steady-state frame allocates nothing "residency
        // included" could not have failed if residency had reallocated an atlas
        // every frame. Found in review.
        //
        // The constants are the two samplers and the placeholder texture with
        // its view, built once in `new` and never again.
        const AT_CONSTRUCTION: u64 = 4;
        self.frame.allocations
            + self.offscreen_allocations
            + self.residency.allocations()
            + AT_CONSTRUCTION
    }

    /// The device, for [`crate::surface`] to configure a swapchain against.
    /// Crate-private: a device handed to a host is a device a host can build
    /// pipelines on, and boundary B has one painter per device by design.
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue, for [`crate::surface`] to present a frame on.
    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Draws `buffer` into a `width` x `height` texture and returns its pixels
    /// as unpremultiplied RGBA8, the space `goldens/README.md` compares in.
    ///
    /// # Errors
    ///
    /// [`RendererError::Extent`] if either dimension is past
    /// [`Renderer::max_extent`]. That is the one failure a caller can be told
    /// about rather than aborted by, and it is a `Result` where the empty
    /// buffer below is a panic because an extent is a number a caller computes
    /// — from a window, from a fixture — while an empty pack is a bug in the
    /// call itself.
    ///
    /// # Panics
    ///
    /// Panics if the frame has no instances to draw: a caller asking for an
    /// empty frame wants a cleared texture and should say so, and silently
    /// returning one hides an empty pack.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RendererError> {
        self.render_dirty(buffer, paints, images, clips, glyphs, None, width, height)
    }

    /// [`Renderer::render`], with boundary B's dirty set passed through.
    ///
    /// Separate from `render` rather than a parameter on it, because every
    /// caller but the incremental-upload test wants the whole frame written and
    /// an `Option` at each of those call sites would say nothing. Passing `None`
    /// is always correct; see [`Frame::upload_instances`] for what passing the
    /// set buys and for what it must not be trusted for.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`].
    ///
    /// # Panics
    ///
    /// As [`Renderer::render`].
    #[allow(clippy::too_many_arguments)]
    pub fn render_dirty(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        changes: Option<Changes<'_>>,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RendererError> {
        // Before the assert, and before anything is allocated: an over-large
        // extent reaches `Device::create_texture` two statements below, and a
        // caller cannot be told about a validation error that panicked.
        self.check_extent(width, height)?;
        assert!(
            !buffer.instances().is_empty(),
            "render was given a frame with no instances"
        );

        let offscreen = match self.offscreen.take() {
            Some(offscreen) if offscreen.width == width && offscreen.height == height => offscreen,
            _ => {
                // A texture, its view and the staging buffer: three objects,
                // and the extent is the only thing that makes them stale.
                self.offscreen_allocations += 3;
                Offscreen::new(&self.device, self.format, width, height)
            }
        };
        self.draw(
            &offscreen.view,
            buffer,
            paints,
            images,
            clips,
            glyphs,
            changes,
            width,
            height,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dashscene-gpu readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &offscreen.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &offscreen.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(offscreen.padded as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = offscreen.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| {
            r.expect("the readback buffer maps");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device completes the frame");
        let data = slice
            .get_mapped_range()
            .expect("the mapped range is readable");
        let mut pixels = Vec::with_capacity(offscreen.unpadded * height as usize);
        for row in 0..height as usize {
            let start = row * offscreen.padded;
            pixels.extend_from_slice(&data[start..start + offscreen.unpadded]);
        }
        drop(data);
        offscreen.readback.unmap();
        self.offscreen = Some(offscreen);
        unpremultiply(&mut pixels);
        Ok(pixels)
    }

    /// Uploads what this frame changed and draws it into `view`.
    ///
    /// The whole of the per-frame work, and the one path both targets take. An
    /// empty frame clears and draws nothing rather than failing: a host whose
    /// document has no ink still has a window to fill, where the offscreen
    /// caller in [`Renderer::render`] asked for a picture and gets a panic.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        view: &wgpu::TextureView,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        changes: Option<Changes<'_>>,
        width: u32,
        height: u32,
    ) {
        // The solid fills, as the shader reads them. Written whole every frame,
        // and deliberately not filtered by the dirty set: a colour that animates
        // changes this table without changing the rect entry that names its row,
        // so no rect is dirty and the row still has to arrive.
        let mut solids: Vec<[f32; 4]> = paints
            .all_solids()
            .iter()
            .map(|c| [c.r, c.g, c.b, c.a])
            .collect();
        // An empty table would make a zero-sized binding, which wgpu refuses, so
        // one dead row stands in — no instance can name it, because an
        // instance's row comes from the table it was packed against.
        if solids.is_empty() {
            solids.push([0.0; 4]);
        }
        let mut boxes: Vec<GpuClipBox> = clips
            .all_boxes()
            .iter()
            .map(|b| GpuClipBox {
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
                corners: [
                    b.corners.top_left,
                    b.corners.top_right,
                    b.corners.bottom_right,
                    b.corners.bottom_left,
                ],
            })
            .collect();
        if boxes.is_empty() {
            boxes.push(GpuClipBox::default());
        }
        let mut strokes: Vec<GpuStroke> = paints
            .all_strokes()
            .iter()
            .map(|stroke| GpuStroke {
                color: [
                    stroke.color.r,
                    stroke.color.g,
                    stroke.color.b,
                    stroke.color.a,
                ],
                width: stroke.width,
                align: stroke_align(stroke.align),
                _pad: [0; 2],
            })
            .collect();
        if strokes.is_empty() {
            strokes.push(GpuStroke::default());
        }
        let viewport = Viewport {
            size: [width as f32, height as f32],
            aa: 1.0,
            _pad: 0.0,
        };

        // Residency, and the rows it resolves into. Before the upload, because
        // making a payload resident can create an atlas, and a bind group names
        // the atlas it draws from.
        let resolved = self.resolve_frame(buffer, paints, images, glyphs);
        let atlases = self.residency.atlas_count();

        self.frame.upload(
            &self.device,
            &self.queue,
            &self.layout,
            &self.sampler,
            &self.msdf_sampler,
            &self.placeholder,
            &self.residency,
            buffer,
            &solids,
            &boxes,
            &strokes,
            &resolved,
            viewport,
            changes,
        );

        let runs = draw_runs(buffer, &resolved);
        debug_assert!(
            runs.iter()
                .all(|run| run.atlas.is_none_or(|a| (a as usize) < atlases)),
            "a draw run names an atlas that does not exist"
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dashscene-gpu frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dashscene-gpu frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            // Four vertices per instance, as a triangle strip, and one draw per
            // run. Slice order is draw order, and the runs partition the buffer
            // in order, so the buffer's own order is still the stacking order.
            for run in &runs {
                pass.set_bind_group(0, self.frame.bind_group(run.atlas), &[]);
                pass.draw(0..4, run.instances.clone());
            }
        }
        self.queue.submit([encoder.finish()]);
        self.frame.last_runs = runs.len();
    }

    /// Every payload-backed table as the shaders read it, with each row's
    /// residency slot resolved into it.
    ///
    /// One walk over the instance rows, not three. Three tables reach the
    /// device through residency — image fills, glyph atlases and baked vector
    /// fields — and an instance names at most one row of one of them, so the
    /// walk that finds an image fill is the same walk that would find a glyph.
    ///
    /// # Residency follows the frame, not the table
    ///
    /// Only the rows some instance names are made resident. That is the whole
    /// point of an eviction policy: a document's asset table is every image it
    /// could ever show, and what has to fit in VRAM is what it shows *now*.
    ///
    /// Resolving the table instead was the first shape of this function, and it
    /// was wrong in a way worse than slow. A document holding more image assets
    /// than one atlas can carry would fail `ResidencyError::FrameExceedsAtlas`
    /// while drawing two of them, because the whole table was the working set by
    /// construction — and the LRU could never help, since it would be asked for
    /// every row every frame. Issue #460's measurement is about exactly this,
    /// one level up.
    ///
    /// It costs one pass over the instance rows, and only in a frame that has an
    /// image fill at all. R-T4 bounds the per-frame cost to the dirty-range
    /// upload and the submission; this is outside that budget and is stated
    /// rather than hidden. The alternative is for the packer to record the rows
    /// it emitted, which is free — and which would be a second record of a fact
    /// the instances already carry, so it is not taken lightly and is not taken
    /// here.
    ///
    /// A row the frame does not draw still gets a `GpuImage`, zeroed, so that a
    /// row index means the same thing in this array as in the table. Nothing
    /// samples it: an instance naming it is what would have made it resident.
    ///
    /// # Panics
    ///
    /// Panics on a payload that cannot be made resident. Every arm of
    /// [`ResidencyError`] is a broken promise rather than a condition to
    /// recover from — a payload larger than this device's largest texture, a
    /// frame whose working set does not fit, or a format the adapter cannot
    /// sample after `Painter::samples` said it could. `Painter::paint` returns
    /// nothing by decision, so there is no channel to report any of them on,
    /// and P4 forbids resolving them into a silently different picture.
    ///
    /// **Issue #720 is that first arm, and story #582 widened what it covers.**
    /// It was filed against image fills: a payload larger than [`ATLAS_EXTENT`]
    /// panics rather than getting its own texture. Glyph atlases and
    /// baked-vector atlases now take the same path, and a glyph atlas is the
    /// more likely of the three to reach 2048 square — it is one sheet for a
    /// whole script at a whole weight, so a CJK coverage set is exactly the
    /// case that exceeds it, where an oversized *photograph* has to be authored
    /// deliberately. Named here rather than left to be discovered, and recorded
    /// on the issue.
    fn resolve_frame(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        glyphs: &GlyphRunTable,
    ) -> Resolved {
        self.residency.begin_frame();
        let fills = paints.all_images();
        let fields = paints.all_shapes();
        let runs = glyphs.runs();
        let mut out = Resolved {
            images: vec![GpuImage::default(); fills.len()],
            runs: vec![GpuGlyphRun::default(); runs.len()],
            shapes: vec![GpuShape::default(); fields.len()],
            atlas_of_image: vec![None; fills.len()],
            atlas_of_run: vec![None; runs.len()],
            atlas_of_shape: vec![None; fields.len()],
        };

        let entries = images.all_entries();
        for instance in buffer.instances() {
            // A coverage mask is read for whatever kind carries it, so the
            // fill and the backdrop of one masked node resolve the same row.
            // They name the same field, so the second is a cache hit and adds
            // nothing to the working set.
            if instance.shape != Instance::NONE {
                let row = instance.shape as usize - 1;
                debug_assert!(
                    instance.kind != InstanceKind::FillImage.as_u32(),
                    "a masked image fill would need two atlases for one quad; the packer emits \
                     none, matching the reference painter"
                );
                let field = &fields[row];
                if out.atlas_of_shape[row].is_none()
                    && field_draws(field)
                    && let Some(slot) =
                        self.resident_image(images, entries, field.image, "a vector field's atlas")
                {
                    let extent = self.residency.atlas_extent(slot.atlas);
                    let asset = images.resolve(field.image);
                    out.atlas_of_shape[row] = Some(slot.atlas);
                    out.shapes[row] = gpu_shape(field, slot.uv(extent), asset.width, asset.height);
                }
            }

            if instance.kind == InstanceKind::FillImage.as_u32() {
                let row = instance.row as usize;
                if out.atlas_of_image[row].is_some() {
                    continue;
                }
                let fill = &fills[row];
                let Some(slot) =
                    self.resident_image(images, entries, fill.image, "an image fill's payload")
                else {
                    continue;
                };
                let asset = images.resolve(fill.image);
                out.atlas_of_image[row] = Some(slot.atlas);
                let t = fill.transform;
                out.images[row] = GpuImage {
                    // Normalised against the atlas this slot landed in, not
                    // against the residency set's nominal extent: a compressed
                    // atlas is rounded down to whole blocks and the two differ.
                    uv: slot.uv(self.residency.atlas_extent(slot.atlas)),
                    transform: [t.a, t.b, t.c, t.d],
                    translate: [t.tx, t.ty],
                    size: [asset.width as f32, asset.height as f32],
                    scale_mode: scale_mode(fill.scale_mode),
                    tile_scale: fill.tile_scale,
                    _pad: [0; 2],
                };
            } else if instance.kind == InstanceKind::Text.as_u32() {
                let row = instance.row as usize;
                if out.atlas_of_run[row].is_some() {
                    continue;
                }
                let run = &runs[row];
                let atlas = glyphs.atlas(run.atlas);
                // An atlas with no extent has no texels to sample, and every
                // mapping below divides by it. The same case, and the same
                // treatment, as a zero-extent image payload.
                if atlas.width == 0 || atlas.height == 0 {
                    continue;
                }
                let slot = self
                    .residency
                    .resident(
                        &self.device,
                        &self.queue,
                        PayloadKey::atlas(run.atlas.0, atlas),
                        // Built here rather than through `ImageAsset::as_ref`,
                        // which re-parses the payload's header on every call:
                        // an `Atlas` already states its extent, and this runs
                        // once per run per frame.
                        dashpaint::ImageRef {
                            format: atlas.image.format,
                            bytes: &atlas.image.bytes,
                            width: atlas.width,
                            height: atlas.height,
                        },
                    )
                    .unwrap_or_else(|error: ResidencyError| {
                        panic!(
                            "glyph atlas {} could not be made resident: {error}",
                            run.atlas.0
                        )
                    });
                let extent = self.residency.atlas_extent(slot.atlas);
                out.atlas_of_run[row] = Some(slot.atlas);
                out.runs[row] = gpu_glyph_run(run, atlas, slot.uv(extent));
            }
        }
        out
    }

    /// Makes image-table row `index` resident and returns where it sits, or
    /// `None` for a payload with no extent.
    ///
    /// # A payload with no extent draws nothing, and is never made resident
    ///
    /// Boundary B stores a payload whose binding supplied no bytes at 0 x 0
    /// rather than refusing it, because `dashscene-validator`'s image.no-bytes
    /// rule is what names that case. Left to reach the residency path, an
    /// encoded one panics in the decoder — on a payload the validator has
    /// already reported — and a baked one divides by zero in the shader. Its row
    /// stays zeroed, its atlas stays `None`, and `paint.wgsl`'s own guards cover
    /// the same case from the other side.
    ///
    /// # Panics
    ///
    /// Panics on a payload that cannot be made resident, for the reason
    /// [`Renderer::resolve_frame`] gives. `what` names the caller in the
    /// message, because an image fill and a vector field's atlas are the same
    /// table row with very different symptoms.
    fn resident_image(
        &mut self,
        images: &ImageTable,
        entries: &[dashpaint::ImageEntry],
        index: u32,
        what: &str,
    ) -> Option<crate::residency::Slot> {
        let asset = images.resolve(index);
        if asset.width == 0 || asset.height == 0 {
            return None;
        }
        let slot = self
            .residency
            .resident(
                &self.device,
                &self.queue,
                PayloadKey::image(index, &entries[index as usize]),
                asset,
            )
            .unwrap_or_else(|error: ResidencyError| {
                panic!("{what} (image asset {index}) could not be made resident: {error}")
            });
        Some(slot)
    }

    /// How many draw calls the frame most recently drawn took.
    ///
    /// One unless the frame's image fills sat in more than one atlas. Public
    /// because a test that asserted only the picture could not tell a frame
    /// that batched from one that did not, and the batching is the property
    /// R-T2 cares about.
    pub fn last_draw_runs(&self) -> usize {
        self.frame.last_runs
    }

    /// Payloads evicted from the atlases to make room, since this renderer was
    /// built.
    pub fn evictions(&self) -> u64 {
        self.residency.evictions()
    }

    /// How many encoded payloads have been decoded since this renderer was
    /// built — see [`crate::Residency::decodes`].
    pub fn decodes(&self) -> u64 {
        self.residency.decodes()
    }
}

/// Every payload-backed table of one frame, as the shaders read it, plus which
/// atlas each row landed in — `None` for a row this frame does not draw.
///
/// A row the frame does not draw still gets a zeroed row, so that a row index
/// means the same thing in these arrays as in the table it came from. Nothing
/// samples it: an instance naming it is what would have made it resident.
struct Resolved {
    images: Vec<GpuImage>,
    runs: Vec<GpuGlyphRun>,
    shapes: Vec<GpuShape>,
    atlas_of_image: Vec<Option<u32>>,
    atlas_of_run: Vec<Option<u32>>,
    atlas_of_shape: Vec<Option<u32>>,
}

impl Resolved {
    /// The atlas `instance` samples, or `None` when it samples none.
    ///
    /// A masked instance samples its coverage field whatever its kind, and the
    /// packer emits no masked image fill — which is what makes "at most one
    /// atlas per instance" true, and what the debug assertion in
    /// [`Renderer::resolve_frame`] holds.
    fn atlas_of(&self, instance: &Instance) -> Option<u32> {
        if instance.shape != Instance::NONE {
            return self.atlas_of_shape[instance.shape as usize - 1];
        }
        if instance.kind == InstanceKind::FillImage.as_u32() {
            return self.atlas_of_image[instance.row as usize];
        }
        if instance.kind == InstanceKind::Text.as_u32() {
            return self.atlas_of_run[instance.row as usize];
        }
        None
    }

    /// Every atlas this frame samples, ascending and without repeats.
    fn atlases(&self) -> Vec<u32> {
        let mut distinct: Vec<u32> = self
            .atlas_of_image
            .iter()
            .chain(&self.atlas_of_run)
            .chain(&self.atlas_of_shape)
            .flatten()
            .copied()
            .collect();
        distinct.sort_unstable();
        distinct.dedup();
        distinct
    }
}

/// One glyph run as the shader reads it, over the slot its atlas landed in.
///
/// `uv` arrives as the atlas payload's own rectangle in the residency texture,
/// normalised. What the shader wants is a map from a *source* texel to that
/// texture, so the atlas's own extent is folded in here — once per run, rather
/// than once per fragment.
fn gpu_glyph_run(run: &dashpaint::GlyphRun, atlas: &dashpaint::Atlas, uv: [f32; 4]) -> GpuGlyphRun {
    let scale = [uv[2] / atlas.width as f32, uv[3] / atlas.height as f32];
    GpuGlyphRun {
        color: [run.color.r, run.color.g, run.color.b, run.color.a],
        uv: [uv[0], uv[1], scale[0], scale[1]],
        half_uv: [0.5 * scale[0], 0.5 * scale[1]],
        // `dashscene-skia`'s own formula. `plane_em` and `atlas_px` bake the
        // range into the bounds, so this scales the sharpness of the edge and
        // not the size.
        px_range: atlas.distance_range_px * run.size / f32::from(atlas.px_per_em),
        _pad: 0.0,
    }
}

/// One coverage mask as the shader reads it, over the slot its atlas landed in.
///
/// `uv` is the whole atlas payload's rectangle in the residency texture; the
/// field occupies `atlas_rect` texels of that payload, so the two are composed
/// here into the field's own sub-rect.
fn gpu_shape(
    field: &dashpaint::VectorField,
    uv: [f32; 4],
    atlas_width: u32,
    atlas_height: u32,
) -> GpuShape {
    let [ax, ay, aw, ah] = field.atlas_rect;
    let (width, height) = (atlas_width as f32, atlas_height as f32);
    let sub = [
        uv[0] + ax as f32 / width * uv[2],
        uv[1] + ay as f32 / height * uv[3],
        aw as f32 / width * uv[2],
        ah as f32 / height * uv[3],
    ];
    let [left, _, right, _] = field.plane_bounds;
    GpuShape {
        plane: field.plane_bounds,
        uv: sub,
        half_uv: [0.5 * sub[2] / aw as f32, 0.5 * sub[3] / ah as f32],
        // Device pixels per atlas texel, at unit scale. `dashscene-skia` takes
        // the x ratio alone, and this matches it rather than re-deriving it.
        px_range: field.distance_range * (right - left) / aw as f32,
        _pad: 0.0,
    }
}

/// Whether a coverage mask has a quad and an atlas rectangle to sample.
///
/// The reference painter's own degenerate guard, and the reason it is checked
/// before the payload is made resident rather than after: every mapping in
/// [`gpu_shape`] divides by the atlas rectangle, and a field with no quad
/// sampled nothing anyway.
fn field_draws(field: &dashpaint::VectorField) -> bool {
    let [left, top, right, bottom] = field.plane_bounds;
    right > left && bottom > top && field.atlas_rect[2] > 0 && field.atlas_rect[3] > 0
}

/// A contiguous range of instances drawn with one atlas bound.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DrawRun {
    instances: std::ops::Range<u32>,
    /// The atlas these instances sample, or `None` when none of them samples
    /// one.
    atlas: Option<u32>,
}

/// Splits the frame into runs, one per atlas the instances need.
///
/// **Every kind that samples**, not only image fills: a glyph instance samples
/// its run's atlas and a masked instance samples its coverage field's, and
/// [`Resolved::atlas_of`] is the one place that mapping is written. Three tables
/// reach residency and an instance names at most one row of one of them.
///
/// # Why this is not a per-frame walk in the common case
///
/// A frame whose payloads all landed in one atlas — which is every frame of one
/// texel format, and so every frame this repository draws today, since a glyph
/// atlas and a decoded image are both `Rgba8` — takes one run over the whole
/// buffer, decided from the resolved rows alone without looking at an instance.
/// Segmenting happens only when a frame genuinely mixes texel formats, which a
/// document does when a host binds `dashpack` derivations for some assets and
/// not others.
///
/// That is a claim about *this* pass and not about the frame:
/// [`Renderer::resolve_frame`] already walks the instance rows once in any
/// frame that samples anything, and says why.
fn draw_runs(buffer: &InstanceBuffer, resolved: &Resolved) -> Vec<DrawRun> {
    let total = buffer.instances().len() as u32;
    // The rows this frame did not draw are `None` and contribute no atlas: a
    // sentinel counted here would conjure a run for an atlas nothing samples.
    let distinct = resolved.atlases();

    match distinct.as_slice() {
        // No image row at all: nothing samples an atlas.
        [] => vec![DrawRun {
            instances: 0..total,
            atlas: None,
        }],
        // Every image row in one atlas: one run, and the instance rows are
        // never read.
        [only] => vec![DrawRun {
            instances: 0..total,
            atlas: Some(*only),
        }],
        _ => {
            let mut runs: Vec<DrawRun> = Vec::new();
            let mut start = 0u32;
            let mut current: Option<u32> = None;
            for (index, instance) in buffer.instances().iter().enumerate() {
                // An instance that samples nothing, and a row that was not made
                // resident — a zero-extent payload — both draw without an
                // atlas, so neither constrains a run.
                let Some(wanted) = resolved.atlas_of(instance) else {
                    continue;
                };
                match current {
                    Some(atlas) if atlas == wanted => {}
                    None => current = Some(wanted),
                    Some(_) => {
                        let index = index as u32;
                        runs.push(DrawRun {
                            instances: start..index,
                            atlas: current,
                        });
                        start = index;
                        current = Some(wanted);
                    }
                }
            }
            runs.push(DrawRun {
                instances: start..total,
                atlas: current,
            });
            runs
        }
    }
}

/// The offscreen target and the staging buffer its pixels are read back
/// through, held across calls for the reason [`Frame`] is.
struct Offscreen {
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    /// One row of pixels in bytes, and that row padded to wgpu's 256-byte copy
    /// alignment. Kept because the readback re-assembles the unpadded rows.
    unpadded: usize,
    padded: usize,
}

impl Offscreen {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dashscene-gpu target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = width as usize * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * height as usize) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            target,
            view,
            readback,
            width,
            height,
            unpadded,
            padded,
        }
    }
}

/// The buffers one frame binds, and the record of what they hold.
///
/// Held across frames rather than built per frame: R-T4 budgets a frame for a
/// dirty-range upload and a submission, and four buffer allocations, a texture,
/// a view and a bind group are none of those.
struct Frame {
    instances: wgpu::Buffer,
    solids: wgpu::Buffer,
    clips: wgpu::Buffer,
    viewport: wgpu::Buffer,
    strokes: wgpu::Buffer,
    images: wgpu::Buffer,
    glyph_runs: wgpu::Buffer,
    shapes: wgpu::Buffer,
    /// One bind group per atlas, plus [`Frame::NO_ATLAS`] at the front for a
    /// frame that samples none.
    ///
    /// They differ in exactly one entry — the texture view — so they are built
    /// together and rebuilt together: a bind group that named a stale buffer
    /// after a reallocation would draw one run of a frame from the previous
    /// frame's rows.
    bind_groups: Vec<wgpu::BindGroup>,
    /// How many atlases [`bind_groups`](Self::bind_groups) was built for, so a
    /// frame that created one rebuilds and a frame that did not does nothing.
    bound_atlases: usize,
    /// Capacities in elements, not bytes. A buffer is reallocated only when a
    /// frame needs more than it holds.
    instance_capacity: usize,
    solid_capacity: usize,
    clip_capacity: usize,
    stroke_capacity: usize,
    image_capacity: usize,
    glyph_run_capacity: usize,
    shape_capacity: usize,
    /// How many draw calls the frame most recently drawn took.
    last_runs: usize,
    /// What the instance buffer on the device currently holds, and the spans
    /// those rows were packed against. This is the record a partial upload is
    /// stated over: without it there is nothing to say what the device already
    /// has, and every frame would have to send everything.
    uploaded: Vec<Instance>,
    spans: Vec<InstanceSpan>,
    /// The viewport currently on the device, so a frame at an unchanged extent
    /// writes nothing.
    uploaded_viewport: Viewport,
    /// The commit the device's rows came from, or `None` when they came from a
    /// caller that named no commit. A frame may be applied incrementally only
    /// if it follows this one — see [`Changes`].
    uploaded_generation: Option<u64>,
    /// How the rows of the frame most recently drawn reached the device.
    last_upload: InstanceUpload,
    /// Device objects this frame has allocated — see [`Renderer::allocations`].
    allocations: u64,
}

/// The smallest buffer this painter allocates, in elements. A zero-sized
/// binding is a validation error rather than an empty draw, so every buffer
/// holds at least one element even when the frame it was built for has none.
const MINIMUM_CAPACITY: usize = 1;

impl Frame {
    /// The [`Frame::bind_groups`] entry that names no atlas.
    const NO_ATLAS: usize = 0;

    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
    ) -> Self {
        let storage = |label: &'static str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let instances = storage(
            "instances",
            (size_of::<Instance>() * MINIMUM_CAPACITY) as u64,
        );
        let solids = storage("solids", (size_of::<[f32; 4]>() * MINIMUM_CAPACITY) as u64);
        let clips = storage(
            "clip boxes",
            (size_of::<GpuClipBox>() * MINIMUM_CAPACITY) as u64,
        );
        let strokes = storage(
            "strokes",
            (size_of::<GpuStroke>() * MINIMUM_CAPACITY) as u64,
        );
        let images = storage(
            "image fills",
            (size_of::<GpuImage>() * MINIMUM_CAPACITY) as u64,
        );
        let glyph_runs = storage(
            "glyph runs",
            (size_of::<GpuGlyphRun>() * MINIMUM_CAPACITY) as u64,
        );
        let shapes = storage(
            "coverage masks",
            (size_of::<GpuShape>() * MINIMUM_CAPACITY) as u64,
        );
        let viewport = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport"),
            size: size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut frame = Self {
            instances,
            solids,
            clips,
            viewport,
            strokes,
            images,
            glyph_runs,
            shapes,
            bind_groups: Vec::new(),
            bound_atlases: 0,
            instance_capacity: MINIMUM_CAPACITY,
            solid_capacity: MINIMUM_CAPACITY,
            clip_capacity: MINIMUM_CAPACITY,
            stroke_capacity: MINIMUM_CAPACITY,
            image_capacity: MINIMUM_CAPACITY,
            glyph_run_capacity: MINIMUM_CAPACITY,
            shape_capacity: MINIMUM_CAPACITY,
            last_runs: 0,
            uploaded: Vec::new(),
            spans: Vec::new(),
            // Zero on both axes, which no drawable is, so the first frame always
            // writes it.
            uploaded_viewport: Viewport::default(),
            uploaded_generation: None,
            last_upload: InstanceUpload::Whole { rows: 0 },
            // The eight buffers above.
            allocations: 8,
        };
        frame.rebind(device, layout, sampler, msdf_sampler, placeholder, &[]);
        frame
    }

    /// The bind group for a run that samples `atlas`, or none.
    fn bind_group(&self, atlas: Option<u32>) -> &wgpu::BindGroup {
        let index = match atlas {
            Some(atlas) => atlas as usize + 1,
            None => Self::NO_ATLAS,
        };
        &self.bind_groups[index]
    }

    /// Rebuilds every bind group over the buffers this frame now holds.
    ///
    /// One per atlas view plus the no-atlas one, all built here so that a
    /// reallocated buffer cannot be reflected in some of them and not others.
    #[allow(clippy::too_many_arguments)]
    fn rebind(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
        atlases: &[&wgpu::TextureView],
    ) {
        self.bind_groups.clear();
        for view in std::iter::once(placeholder).chain(atlases.iter().copied()) {
            self.bind_groups.push(bind(
                device,
                layout,
                &self.instances,
                &self.solids,
                &self.clips,
                &self.viewport,
                &self.strokes,
                &self.images,
                &self.glyph_runs,
                &self.shapes,
                view,
                sampler,
                msdf_sampler,
            ));
            self.allocations += 1;
        }
        self.bound_atlases = atlases.len();
    }

    /// Puts this frame's data on the device, writing as little as it can.
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
        residency: &Residency,
        buffer: &InstanceBuffer,
        solids: &[[f32; 4]],
        boxes: &[GpuClipBox],
        strokes: &[GpuStroke],
        resolved: &Resolved,
        viewport: Viewport,
        changes: Option<Changes<'_>>,
    ) {
        let mut rebind = self.upload_instances(device, queue, buffer, changes);

        // The two tables are written whole. A dirty set says which *rect*
        // changed and nothing about which table row did, so filtering these by
        // it would be reading it for a claim it does not make.
        if solids.len() > self.solid_capacity {
            self.solid_capacity = grown(solids.len());
            self.solids = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("solids"),
                size: (size_of::<[f32; 4]>() * self.solid_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.solids, 0, bytemuck::cast_slice(solids));

        if boxes.len() > self.clip_capacity {
            self.clip_capacity = grown(boxes.len());
            self.clips = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("clip boxes"),
                size: (size_of::<GpuClipBox>() * self.clip_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.clips, 0, bytemuck::cast_slice(boxes));

        if strokes.len() > self.stroke_capacity {
            self.stroke_capacity = grown(strokes.len());
            self.strokes = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strokes"),
                size: (size_of::<GpuStroke>() * self.stroke_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.strokes, 0, bytemuck::cast_slice(strokes));

        // The three resolved tables, written whole for the same reason the two
        // above are. A frame that has none of a kind still writes one dead row,
        // because a zero-sized binding is a validation error.
        let dead_image = [GpuImage::default()];
        let gpu_images = or_dead(&resolved.images, &dead_image);
        if gpu_images.len() > self.image_capacity {
            self.image_capacity = grown(gpu_images.len());
            self.images = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image fills"),
                size: (size_of::<GpuImage>() * self.image_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.images, 0, bytemuck::cast_slice(gpu_images));

        let dead_run = [GpuGlyphRun::default()];
        let gpu_runs = or_dead(&resolved.runs, &dead_run);
        if gpu_runs.len() > self.glyph_run_capacity {
            self.glyph_run_capacity = grown(gpu_runs.len());
            self.glyph_runs = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph runs"),
                size: (size_of::<GpuGlyphRun>() * self.glyph_run_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.glyph_runs, 0, bytemuck::cast_slice(gpu_runs));

        let dead_shape = [GpuShape::default()];
        let gpu_shapes = or_dead(&resolved.shapes, &dead_shape);
        if gpu_shapes.len() > self.shape_capacity {
            self.shape_capacity = grown(gpu_shapes.len());
            self.shapes = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("coverage masks"),
                size: (size_of::<GpuShape>() * self.shape_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.shapes, 0, bytemuck::cast_slice(gpu_shapes));

        if viewport != self.uploaded_viewport {
            queue.write_buffer(&self.viewport, 0, bytemuck::bytes_of(&viewport));
            self.uploaded_viewport = viewport;
        }

        // A new atlas is as much a reason to rebuild as a reallocated buffer:
        // the bind groups are per atlas, and one that does not exist yet cannot
        // be bound.
        if rebind || residency.atlas_count() != self.bound_atlases {
            let views: Vec<&wgpu::TextureView> = (0..residency.atlas_count())
                .map(|index| residency.view(index as u32))
                .collect();
            self.rebind(device, layout, sampler, msdf_sampler, placeholder, &views);
        }
    }

    /// Writes the instance rows, and reports whether the buffer was
    /// reallocated.
    ///
    /// # When a partial upload is sound, and when it is not
    ///
    /// Three things have to hold, and none of them is assumed.
    ///
    /// **This frame follows the one on the device.** A dirty set is stated
    /// against the commit before it, so it says nothing about a commit the
    /// device never received. [`Changes`] carries the generation for exactly
    /// this reason, and the arithmetic below — `held + 1` — is the whole of the
    /// check. A frame that skipped one, or that came from a fresh arena whose
    /// generations start again, is written whole.
    ///
    /// **The buffer has the same shape.** An instance is a function of the rect
    /// entry it was packed from, of the rows that entry names, and of the group
    /// stack the packer was in when it reached it. A rect the set leaves out can
    /// still have moved for a reason the set does not carry — a group opening
    /// over its range changes its instances' `layer` without touching its entry
    /// — and every such change moves a span, so comparing spans catches it.
    ///
    /// **The set names rects this buffer has.** A dirty index past the span
    /// table means the two disagree about what a rect index is.
    ///
    /// The debug assertion at the end holds the rest: it compares every row
    /// against what the device now has, so an assumption that stops being true
    /// fails a test run rather than leaving a stale quad on the screen of a
    /// release build. That is not hypothetical — it is what found the missing
    /// generation check, on a value that had converged and could never be
    /// reported again.
    fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: &InstanceBuffer,
        changes: Option<Changes<'_>>,
    ) -> bool {
        let rows = buffer.instances();
        let spans = buffer.spans();

        let mut rebind = false;
        if rows.len() > self.instance_capacity {
            self.instance_capacity = grown(rows.len());
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instances"),
                size: (size_of::<Instance>() * self.instance_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            // Nothing of the previous frame survives a reallocation, so the
            // partial path cannot apply to this frame.
            self.uploaded.clear();
            self.uploaded_generation = None;
            rebind = true;
        }

        let held = self.uploaded_generation;
        let ranges = match changes {
            // The commit the device already holds, handed over again — a forced
            // redraw with no tick between. The rows are a pure function of the
            // tables, so there is nothing to write.
            Some(changes) if held == Some(changes.generation) => Vec::new(),
            Some(changes)
                if held.map(|held| held + 1) == Some(changes.generation)
                    // The spans partition the buffer, so equal spans already
                    // imply an equal row count and no mutation of this line
                    // alone changes a frame's path. It stays as the bound that
                    // keeps the `self.uploaded[range]` indexing below in range
                    // without appealing to that invariant, which is held in
                    // another module.
                    && self.uploaded.len() == rows.len()
                    && self.spans == spans
                    && changes
                        .rects
                        .iter()
                        .all(|&rect| (rect as usize) < spans.len()) =>
            {
                dirty_ranges(changes.rects, spans)
            }
            _ => {
                queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(rows));
                self.uploaded.clear();
                self.uploaded.extend_from_slice(rows);
                self.spans.clear();
                self.spans.extend_from_slice(spans);
                self.uploaded_generation = changes.map(|changes| changes.generation);
                self.last_upload = InstanceUpload::Whole { rows: rows.len() };
                return rebind;
            }
        };

        let mut written = 0;
        for range in &ranges {
            queue.write_buffer(
                &self.instances,
                (range.start * size_of::<Instance>()) as wgpu::BufferAddress,
                bytemuck::cast_slice(&rows[range.clone()]),
            );
            self.uploaded[range.clone()].copy_from_slice(&rows[range.clone()]);
            written += range.len();
        }
        self.uploaded_generation = changes.map(|changes| changes.generation);
        self.last_upload = InstanceUpload::Ranges {
            ranges: ranges.len(),
            rows: written,
        };

        debug_assert!(
            self.uploaded == rows,
            "a row outside the dirty set changed, so the device now holds a stale instance: this \
             frame follows the one on the device and its spans match, so something the set does \
             not report has moved a row"
        );
        rebind
    }
}

/// The instance ranges a dirty set names, with adjacent ones merged.
///
/// One `write_buffer` per range rather than per rect: a scene where most rects
/// changed would otherwise queue one staging copy each, and consecutive rects
/// are consecutive in the buffer by construction.
///
/// `CommittedScene::dirty` is sorted and this does not require it to be — an
/// unsorted set merges less and writes the same bytes.
///
/// # Panics
///
/// Panics if a dirty index names no span. The caller checks that before
/// choosing this path, because a dirty set and a rect table that disagree are
/// two views of one frame that cannot both be right.
fn dirty_ranges(dirty: &[u32], spans: &[InstanceSpan]) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for &rect in dirty {
        let span = spans[rect as usize];
        // A layout-only container draws nothing. Its span still records where
        // the next rect begins, so it must not break the merge either.
        if span.count == 0 {
            continue;
        }
        let start = span.offset as usize;
        let end = start + span.count as usize;
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// `rows` unless it is empty, in which case the one dead row `dead` holds.
///
/// A zero-sized binding is a validation error rather than an empty draw, so a
/// frame that has no row of a kind still binds one. Nothing can name it: an
/// instance's row comes from the table it was packed against, and that table
/// had no rows.
fn or_dead<'a, T: Pod>(rows: &'a [T], dead: &'a [T; 1]) -> &'a [T] {
    if rows.is_empty() { &dead[..] } else { rows }
}

/// The capacity a buffer is grown to when a frame outgrows it.
///
/// Rounded up to a power of two, so a scene that adds a rect per frame
/// reallocates a logarithmic number of times rather than every frame.
fn grown(needed: usize) -> usize {
    needed.max(MINIMUM_CAPACITY).next_power_of_two()
}

/// Binds everything the shaders read. One place, so the bind group a frame is
/// built with and the one it is rebuilt with cannot drift apart.
#[allow(clippy::too_many_arguments)]
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    solids: &wgpu::Buffer,
    clips: &wgpu::Buffer,
    viewport: &wgpu::Buffer,
    strokes: &wgpu::Buffer,
    images: &wgpu::Buffer,
    glyph_runs: &wgpu::Buffer,
    shapes: &wgpu::Buffer,
    atlas: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    msdf_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dashscene-gpu paint"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: solids.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: viewport.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: strokes.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: images.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(atlas),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: glyph_runs.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: shapes.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::Sampler(msdf_sampler),
            },
        ],
    })
}

/// Undoes the premultiplication the blend state produced.
///
/// `goldens/README.md` compares decoded pixels in unpremultiplied RGBA8888, and
/// `docs/decisions/golden-comparison-space.md` is why. A painter that returned
/// premultiplied bytes would be comparable only against itself.
fn unpremultiply(pixels: &mut [u8]) {
    for texel in pixels.chunks_exact_mut(4) {
        let a = texel[3];
        if a == 0 || a == 255 {
            continue;
        }
        for channel in &mut texel[..3] {
            *channel = ((*channel as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        }
    }
}

/// The stroke rows, as the shader's array reads them. A `vec4f` puts the WGSL
/// struct's alignment at 16 and rounds its stride to 32, so a Rust type of any
/// other size would have every element after the first read from the wrong
/// offset — the same hazard `Instance::_pad` exists for, and the reason both
/// are asserted rather than reasoned about.
const _: () = assert!(size_of::<GpuStroke>() == 32);

/// The instance rows, as bytes. `Instance` is `#[repr(C)]` with no padding
/// (`docs/decisions/instance-buffer-contract.md` D2), which is what lets the
/// buffer be cast rather than rebuilt.
const _: () = assert!(size_of::<Instance>() == 64);

/// The image rows, for the reason [`GpuStroke`]'s assertion gives — a `vec4f`
/// puts the WGSL struct's alignment at 16 and rounds its stride to 64, so a
/// Rust type of any other size makes every row after the first read from the
/// wrong offset. Added in review: this struct is read as `array<Image>` exactly
/// as the two above are read, and was the only one of the three pinned by
/// nothing.
const _: () = assert!(size_of::<GpuImage>() == 64);

/// The glyph-run rows and the coverage-mask rows, for the same reason: three
/// 16-byte-aligned groups each, so both strides are 48 and neither type may
/// change size without this failing.
const _: () = assert!(size_of::<GpuGlyphRun>() == 48);
const _: () = assert!(size_of::<GpuShape>() == 48);

#[cfg(test)]
mod tests {
    use super::{
        DrawRun, GpuGlyphRun, GpuImage, GpuShape, MINIMUM_CAPACITY, Resolved, dirty_ranges,
        draw_runs, grown, scale_mode,
    };
    use crate::instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan};
    use dashpaint::ScaleMode;

    /// A resolved frame whose image rows landed in `images`, whose glyph runs
    /// landed in `runs`, and whose coverage masks landed in `shapes`.
    ///
    /// Only the atlas maps matter to `draw_runs`; the row arrays are sized to
    /// match so that an index into either means the same thing.
    fn resolved(images: &[Option<u32>], runs: &[Option<u32>], shapes: &[Option<u32>]) -> Resolved {
        Resolved {
            images: vec![GpuImage::default(); images.len()],
            runs: vec![GpuGlyphRun::default(); runs.len()],
            shapes: vec![GpuShape::default(); shapes.len()],
            atlas_of_image: images.to_vec(),
            atlas_of_run: runs.to_vec(),
            atlas_of_shape: shapes.to_vec(),
        }
    }

    /// A resolved frame with image rows only — the shape every pre-#582 case
    /// here was written against.
    fn images_only(images: &[Option<u32>]) -> Resolved {
        resolved(images, &[], &[])
    }

    fn span(offset: u32, count: u32) -> InstanceSpan {
        InstanceSpan { offset, count }
    }

    /// A buffer of one rect whose instances are `kinds` paired with the rows
    /// they name. Built through the buffer's own API, so the rows are laid out
    /// the way the packer lays them out.
    fn buffer(kinds: &[(InstanceKind, u32)]) -> InstanceBuffer {
        let mut out = InstanceBuffer::new();
        out.begin_rect(0);
        for &(kind, row) in kinds {
            out.push(Instance {
                kind: kind.as_u32(),
                row,
                ..Instance::default()
            });
        }
        out
    }

    fn run(instances: std::ops::Range<u32>, atlas: Option<u32>) -> DrawRun {
        DrawRun { instances, atlas }
    }

    /// A frame whose image rows all sit in one atlas is one draw call, decided
    /// from the resolved rows without segmenting the buffer.
    #[test]
    fn one_atlas_is_one_run_over_the_whole_buffer() {
        let frame = buffer(&[
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
        ]);
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(0)])),
            vec![run(0..3, Some(0))]
        );
    }

    /// A frame with no image fill binds no atlas and is still one run.
    #[test]
    fn no_image_row_is_one_run_naming_no_atlas() {
        let frame = buffer(&[(InstanceKind::FillSolid, 0), (InstanceKind::Stroke, 0)]);
        assert_eq!(draw_runs(&frame, &images_only(&[])), vec![run(0..2, None)]);
    }

    /// A table row this frame does not draw contributes no atlas and no run.
    ///
    /// Residency follows the frame rather than the table, so an undrawn row is
    /// `None` — and a `None` counted as an atlas would conjure a run binding a
    /// texture nothing samples, or worse, split the frame around it.
    #[test]
    fn an_undrawn_image_row_contributes_no_run() {
        let frame = buffer(&[(InstanceKind::FillSolid, 0), (InstanceKind::FillImage, 1)]);
        // Row 0 exists in the table and no instance names it; row 1 is drawn.
        assert_eq!(
            draw_runs(&frame, &images_only(&[None, Some(0)])),
            vec![run(0..2, Some(0))]
        );
        // And a table whose rows are all undrawn is the no-atlas case.
        let solid_only = buffer(&[(InstanceKind::FillSolid, 0)]);
        assert_eq!(
            draw_runs(&solid_only, &images_only(&[None, None])),
            vec![run(0..1, None)]
        );
    }

    /// Two atlases split the frame where the atlas changes, and nowhere else.
    ///
    /// The boundary is at the *second* image instance rather than at the first,
    /// because everything before an atlas is first needed can be drawn with it
    /// bound. A split that started a run at every image instance would draw the
    /// same picture with more calls, so the count is what pins it.
    #[test]
    fn a_frame_mixing_two_atlases_splits_where_the_atlas_changes() {
        let frame = buffer(&[
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillSolid, 0),
        ]);
        // Row 0 is in atlas 0 and row 1 in atlas 1.
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(1)])),
            vec![run(0..3, Some(0)), run(3..5, Some(1))]
        );
    }

    /// Consecutive image instances of the same atlas do not split, even when a
    /// third atlas is in the frame — the run boundary follows the atlas, not the
    /// row.
    #[test]
    fn instances_sharing_an_atlas_stay_in_one_run() {
        let frame = buffer(&[
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillImage, 2),
        ]);
        // Rows 0 and 1 share atlas 0; row 2 is in atlas 1.
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(0), Some(1)])),
            vec![run(0..2, Some(0)), run(2..3, Some(1))]
        );
    }

    /// The runs partition the buffer, in order, with no gap and no overlap.
    ///
    /// Stated separately from the cases above because it is the property that
    /// makes slice order still be draw order: a run boundary that dropped or
    /// repeated an instance would draw a wrong picture in a way a per-case
    /// expectation might not name.
    #[test]
    fn the_runs_partition_the_buffer_in_order() {
        let frame = buffer(&[
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 2),
            (InstanceKind::FillImage, 0),
        ]);
        let runs = draw_runs(&frame, &images_only(&[Some(0), Some(1), Some(0)]));
        assert_eq!(runs.first().expect("at least one run").instances.start, 0);
        assert_eq!(
            runs.last().expect("at least one run").instances.end,
            frame.instances().len() as u32
        );
        for pair in runs.windows(2) {
            assert_eq!(
                pair[0].instances.end, pair[1].instances.start,
                "the runs must meet exactly: {runs:?}"
            );
        }
    }

    /// The four scale modes are four distinct numbers, and they are the ones
    /// `paint.wgsl` compares against.
    #[test]
    fn the_scale_modes_are_distinct_and_match_the_shader() {
        let mapped = [
            scale_mode(ScaleMode::Fill),
            scale_mode(ScaleMode::Fit),
            scale_mode(ScaleMode::Crop),
            scale_mode(ScaleMode::Tile),
        ];
        assert_eq!(mapped, [0, 1, 2, 3]);
        let shader = include_str!("shaders/paint.wgsl");
        for (name, value) in [
            ("SCALE_FILL", 0),
            ("SCALE_FIT", 1),
            ("SCALE_CROP", 2),
            ("SCALE_TILE", 3),
        ] {
            assert!(
                shader.contains(&format!("const {name}: u32 = {value}u;")),
                "{name} must be {value} in the shader, which is what the Rust mapping assigns"
            );
        }
    }

    /// The row a fill instance names is the row the shader indexes, so an
    /// instance built here is the one the runs are stated over.
    #[test]
    fn an_image_instance_names_its_table_row() {
        let frame = buffer(&[(InstanceKind::FillImage, 7)]);
        assert_eq!(frame.instances()[0].row, 7);
        assert_eq!(frame.instances()[0].kind, InstanceKind::FillImage.as_u32());
        let _ = Instance::default();
    }

    /// The property the merge exists for: rects that follow each other in the
    /// buffer are written as one copy.
    #[test]
    fn adjacent_dirty_rects_merge_into_one_range() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[0, 1, 2], &spans), vec![0..6]);
    }

    /// And the property that would make the merge wrong if it went further: the
    /// gap between two dirty rects is a rect that did not change, and its rows
    /// must keep the values the device already holds.
    #[test]
    fn a_clean_rect_between_two_dirty_ones_splits_the_range() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[0, 2], &spans), vec![0..2, 5..6]);
    }

    /// A layout-only container packs no instances. Its span still records where
    /// the next rect begins, so skipping it must not break the merge of the
    /// rects on either side of it.
    #[test]
    fn a_rect_that_draws_nothing_contributes_no_range() {
        let spans = [span(0, 2), span(2, 0), span(2, 1)];
        assert!(dirty_ranges(&[1], &spans).is_empty());
        assert_eq!(dirty_ranges(&[0, 1, 2], &spans), vec![0..3]);
    }

    /// `CommittedScene::dirty` is sorted, so this is not a case that arises —
    /// but the merge must not lose a write if it ever does. Every named rect's
    /// rows are still written; only the merging is worse.
    #[test]
    fn an_unsorted_dirty_set_still_writes_every_named_rect() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[2, 0, 1], &spans), vec![5..6, 0..5]);
    }

    /// A buffer is never grown to nothing, because a zero-sized binding is a
    /// validation error rather than an empty draw.
    #[test]
    fn a_buffer_is_never_grown_to_nothing() {
        assert_eq!(grown(0), MINIMUM_CAPACITY);
        assert_eq!(grown(1), 1);
        assert_eq!(grown(3), 4);
        assert_eq!(grown(4), 4);
        assert_eq!(grown(5), 8);
    }
}
