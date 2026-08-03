//! The device, the pipeline, and the frame path (stories #580 and #585).
//!
//! # What this draws, and what it does not
//!
//! Opaque rounded rects with a solid fill, clipped by their region. Gradients
//! and image fills are story #582's, shadows and backdrop blur #584's, group
//! opacity #583's.
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
use dashpaint::{ClipTable, PaintTable};

use crate::instance::{Instance, InstanceBuffer, InstanceSpan};

/// The viewport uniform the shaders read.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Pod, Zeroable)]
struct Viewport {
    size: [f32; 2],
    aa: f32,
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
    /// Device objects allocated for the offscreen target, counted beside
    /// [`Frame::allocations`] — see [`Renderer::allocations`].
    offscreen_allocations: u64,
}

/// What a renderer could not be built for.
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
        }
    }
}

impl std::error::Error for RendererError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RendererError::NoDevice(e) => Some(e),
            RendererError::NoSurface(e) => Some(e),
            RendererError::NoAdapter | RendererError::NoLinearFormat(_) => None,
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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dashscene-gpu"),
            required_features: wgpu::Features::empty(),
            // Downlevel defaults, so this painter runs on the entry-tier class
            // of device R3 names rather than only on a desktop one.
            required_limits: wgpu::Limits::downlevel_defaults(),
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

        let storage = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                storage(0),
                storage(1),
                storage(2),
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

        let frame = Frame::new(&device, &layout);
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
        })
    }

    /// The adapter this renderer runs on, for a measurement to be recorded
    /// beside.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
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
        self.frame.allocations + self.offscreen_allocations
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
    /// # Panics
    ///
    /// Panics if the frame has no instances to draw: a caller asking for an
    /// empty frame wants a cleared texture and should say so, and silently
    /// returning one hides an empty pack.
    pub fn render(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        clips: &ClipTable,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.render_dirty(buffer, paints, clips, None, width, height)
    }

    /// [`Renderer::render`], with boundary B's dirty set passed through.
    ///
    /// Separate from `render` rather than a parameter on it, because every
    /// caller but the incremental-upload test wants the whole frame written and
    /// an `Option` at each of those call sites would say nothing. Passing `None`
    /// is always correct; see [`Frame::upload_instances`] for what passing the
    /// set buys and for what it must not be trusted for.
    ///
    /// # Panics
    ///
    /// As [`Renderer::render`].
    pub fn render_dirty(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        clips: &ClipTable,
        changes: Option<Changes<'_>>,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
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
            clips,
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
        pixels
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
        clips: &ClipTable,
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
        let viewport = Viewport {
            size: [width as f32, height as f32],
            aa: 1.0,
            _pad: 0.0,
        };

        self.frame.upload(
            &self.device,
            &self.queue,
            &self.layout,
            buffer,
            &solids,
            &boxes,
            viewport,
            changes,
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
            pass.set_bind_group(0, &self.frame.bind_group, &[]);
            // Four vertices per instance, as a triangle strip, and one draw for
            // the whole frame. Slice order is draw order, which is what makes
            // the buffer's own order the stacking order.
            pass.draw(0..4, 0..buffer.instances().len() as u32);
        }
        self.queue.submit([encoder.finish()]);
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
    bind_group: wgpu::BindGroup,
    /// Capacities in elements, not bytes. A buffer is reallocated only when a
    /// frame needs more than it holds.
    instance_capacity: usize,
    solid_capacity: usize,
    clip_capacity: usize,
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
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
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
        let viewport = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport"),
            size: size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = bind(device, layout, &instances, &solids, &clips, &viewport);
        Self {
            instances,
            solids,
            clips,
            viewport,
            bind_group,
            instance_capacity: MINIMUM_CAPACITY,
            solid_capacity: MINIMUM_CAPACITY,
            clip_capacity: MINIMUM_CAPACITY,
            uploaded: Vec::new(),
            spans: Vec::new(),
            // Zero on both axes, which no drawable is, so the first frame always
            // writes it.
            uploaded_viewport: Viewport::default(),
            uploaded_generation: None,
            last_upload: InstanceUpload::Whole { rows: 0 },
            // The four buffers above and the bind group over them.
            allocations: 5,
        }
    }

    /// Puts this frame's data on the device, writing as little as it can.
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        buffer: &InstanceBuffer,
        solids: &[[f32; 4]],
        boxes: &[GpuClipBox],
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

        if viewport != self.uploaded_viewport {
            queue.write_buffer(&self.viewport, 0, bytemuck::bytes_of(&viewport));
            self.uploaded_viewport = viewport;
        }

        if rebind {
            self.bind_group = bind(
                device,
                layout,
                &self.instances,
                &self.solids,
                &self.clips,
                &self.viewport,
            );
            self.allocations += 1;
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

/// The capacity a buffer is grown to when a frame outgrows it.
///
/// Rounded up to a power of two, so a scene that adds a rect per frame
/// reallocates a logarithmic number of times rather than every frame.
fn grown(needed: usize) -> usize {
    needed.max(MINIMUM_CAPACITY).next_power_of_two()
}

/// Binds the four buffers the shaders read. One place, so the bind group a
/// frame is built with and the one it is rebuilt with cannot drift apart.
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    solids: &wgpu::Buffer,
    clips: &wgpu::Buffer,
    viewport: &wgpu::Buffer,
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

/// The instance rows, as bytes. `Instance` is `#[repr(C)]` with no padding
/// (`docs/decisions/instance-buffer-contract.md` D2), which is what lets the
/// buffer be cast rather than rebuilt.
const _: () = assert!(size_of::<Instance>() == 64);

#[cfg(test)]
mod tests {
    use super::{MINIMUM_CAPACITY, dirty_ranges, grown};
    use crate::instance::InstanceSpan;

    fn span(offset: u32, count: u32) -> InstanceSpan {
        InstanceSpan { offset, count }
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
