//! The device, the pipeline, and the first pixels (story #580).
//!
//! # What this draws, and what it does not
//!
//! Opaque rounded rects with a solid fill, clipped by their region. Gradients
//! and image fills are story #582's, shadows and backdrop blur #584's, group
//! opacity #583's.
//!
//! An instance whose kind or fill tag this shader does not implement draws
//! nothing. It does not fall through to a colour: `Instance::tag` means a
//! different enum per kind and their discriminants collide — `PaintTag::Solid`,
//! `ShadowKind::Inner` and `BlurKind::Backdrop` are all 1 — so a shader reading
//! the tag alone paints a shadow with `solids[shadow_row]`. The fragment shader
//! gates on both.
//!
//! # Offscreen, not a surface
//!
//! The renderer draws into a texture it owns and reads the pixels back. A
//! surface needs a window, and the window is the host's — story #585 puts this
//! painter behind v0.14's `Present` seam. Drawing offscreen is also what lets
//! layer 3 run as an ordinary test.
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

use crate::instance::{Instance, InstanceBuffer};

/// The viewport uniform the shaders read.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
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

/// A device, a queue and the one pipeline this story builds.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    adapter_info: wgpu::AdapterInfo,
}

/// What a renderer could not be built for.
#[derive(Debug)]
pub enum RendererError {
    /// No adapter at all — a machine or a runner with no GPU and no software
    /// device installed.
    NoAdapter,
    /// An adapter that will not give a device at the limits this painter needs.
    NoDevice(wgpu::RequestDeviceError),
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
        }
    }
}

impl std::error::Error for RendererError {}

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
    /// Acquires an adapter and builds the pipeline.
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
                    format: TARGET_FORMAT,
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

        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            adapter_info,
        })
    }

    /// The adapter this renderer runs on, for a measurement to be recorded
    /// beside.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
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
        &self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        clips: &ClipTable,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        use wgpu::util::DeviceExt as _;

        assert!(
            !buffer.instances().is_empty(),
            "render was given a frame with no instances"
        );

        // The solid fills, as the shader reads them. An empty table would make
        // a zero-sized binding, which wgpu refuses, so one dead row stands in —
        // no instance can name it, because an instance's row comes from the
        // table it was packed against.
        let mut solids: Vec<[f32; 4]> = paints
            .all_solids()
            .iter()
            .map(|c| [c.r, c.g, c.b, c.a])
            .collect();
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

        let init = |label: &str, contents: &[u8], usage: wgpu::BufferUsages| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        let instances = init(
            "instances",
            bytemuck::cast_slice(buffer.instances()),
            wgpu::BufferUsages::STORAGE,
        );
        let solid_buffer = init(
            "solids",
            bytemuck::cast_slice(&solids),
            wgpu::BufferUsages::STORAGE,
        );
        let clip_buffer = init(
            "clip boxes",
            bytemuck::cast_slice(&boxes),
            wgpu::BufferUsages::STORAGE,
        );
        let viewport_buffer = init(
            "viewport",
            bytemuck::bytes_of(&viewport),
            wgpu::BufferUsages::UNIFORM,
        );

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dashscene-gpu paint"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instances.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: solid_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: clip_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: viewport_buffer.as_entire_binding(),
                },
            ],
        });

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dashscene-gpu target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        // A readback row is padded to 256 bytes, which is wgpu's copy
        // alignment; the unpadded rows are re-assembled below.
        let unpadded = width as usize * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * height as usize) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dashscene-gpu frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dashscene-gpu frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_bind_group(0, &bind_group, &[]);
            // Four vertices per instance, as a triangle strip, and one draw for
            // the whole frame. Slice order is draw order, which is what makes
            // the buffer's own order the stacking order.
            pass.draw(0..4, 0..buffer.instances().len() as u32);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
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

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| {
            r.expect("the readback buffer maps");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device completes the frame");
        let data = slice
            .get_mapped_range()
            .expect("the mapped range is readable");
        let mut pixels = Vec::with_capacity(unpadded * height as usize);
        for row in 0..height as usize {
            let start = row * padded;
            pixels.extend_from_slice(&data[start..start + unpadded]);
        }
        drop(data);
        readback.unmap();
        unpremultiply(&mut pixels);
        pixels
    }
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
