//! The lean painter — instanced quads and analytic SDF over `wgpu`, covering
//! native and web from one codebase
//! (`docs/decisions/wgpu-is-the-lean-painter.md`).
//!
//! # Status: the whole v0 paint vocabulary draws
//!
//! Story #577 stood the crate up against boundary B — "the entire painter
//! input" (`docs/design/architecture.md`) — so that the trait was proven
//! implementable by something other than the crate it was shaped alongside.
//! Story #578 put the painter's CPU half behind that seam: [`instance`] holds
//! the per-instance struct and [`pack`] turns boundary B's tables into one
//! ordered frame of them, verified bit-exactly with no GPU, which is layer 1
//! of epic #569's four-layer net
//! (`docs/decisions/instance-buffer-contract.md`).
//!
//! Story #579 added the shader library beside it: [`shader::SDF_WGSL`] is the
//! one file the painter's signed-distance math lives in, and every consumer
//! includes that string rather than copying from it
//! (`docs/decisions/shader-library-and-layer-2.md`).
//!
//! Story #580 gave it a device and a pipeline, and drew the first pixels
//! offscreen ([`render`], `docs/decisions/pipelines-and-layer-3.md`). Story
//! #585 added the second target: [`surface`] presents the same frame to a
//! window's swapchain, which is what the showcase host draws through.
//!
//! Story #710 added the outline stroke beside the fill. It exists because no
//! story in epic #569's breakdown drew one: [`pack`] has emitted
//! [`InstanceKind::Stroke`] since story #578 and nothing after it named the
//! kind, which running the two painters against one scene made visible.
//!
//! Story #581 added atlas residency ([`residency`]) and the image fill that
//! uses it: a payload reaches a texture, and `Painter::samples` stops being the
//! trait default and starts naming what this painter's device can actually take
//! (`docs/decisions/atlas-residency-and-image-fills.md`).
//!
//! Story #582 added text and baked vector fields: a glyph run's atlas reaches
//! the same residency set an image fill's payload does, and a node's coverage
//! mask now confines its fill instead of being ignored
//! (`docs/decisions/tables-the-vertex-stage-reads.md`). Both tables are read by
//! the *vertex* stage, because the fragment stage has no binding left.
//!
//! Issue #715 added the gradient fill, and with it the paint-parameter heap two
//! earlier records forecast: a gradient's stop array is indexed by a value the
//! fragment stage computes, so it can cross as no varying, and that stage had
//! no binding left to give it. The solid colours and the gradient rows share
//! one storage buffer instead
//! (`docs/decisions/the-paint-parameter-heap.md`).
//!
//! Story #583 drew render-target group opacity, the one part of the vocabulary
//! that is not an `InstanceKind` at all: `Instance::layer` has named the
//! innermost enclosing group since story #578, and nothing read it. A group
//! whose members overlap now draws into its own layer and composites at the
//! group's alpha, through a second pipeline — the route anything that samples
//! a rendered target has to take, since the paint pipeline's fragment stage is
//! full
//! (`docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`).
//!
//! Story #584 drew the two shadow kinds. The closed form was already in
//! [`shader::SDF_WGSL`] and layer 2 already measured it; what was missing is
//! that a shadow's parameters reached the device through nothing, so the row
//! every shadow instance named pointed into a table no binding carried. They
//! extend the paint heap by a third region, and the quad growth a drop shadow
//! needs moved onto `Instance::outset` — the vertex stage cannot read that
//! heap, and it stopped reading the stroke rows in the same move
//! (`docs/decisions/instance-buffer-contract.md` D9).
//!
//! Story #733 drew the last of it. A backdrop blur reads what is already in
//! the render target, which no binding on the paint pipeline can do and no
//! pass can do for its own attachment — so the planner ends the pass at a
//! backdrop instance, the renderer snapshots the target between the two, and
//! two more pipelines run a separable Gaussian over the snapshot and write the
//! result back
//! (`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`).
//!
//! What draws is rounded rects with a solid, gradient or image fill, their
//! stroke, positioned glyph runs, a fill masked by a baked vector field, both
//! shadow kinds and the backdrop blur — all clipped by their region, and
//! render-target group opacity as an offscreen layer composited at the group's
//! alpha
//! (`docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`).
//! That is the whole of the v0 paint vocabulary; what remains between this
//! painter and the reference one is fidelity, which story #586 measures.
//!
//! # Why this crate is named for the role
//!
//! `wgpu` is the backend, not the contract. The strategy record's contingency,
//! if `wgpu`'s GL backend fails on a target, is a direct-GLES backend written
//! over the same instance buffer and the same shaders — so a crate named for
//! the backend would need renaming on the day that contingency was taken.
//!
//! # What this painter will and will not do
//!
//! P2 binds it: painters only colour. It never measures, wraps, kerns, or
//! moves anything. Every primitive in the v0 vocabulary maps onto an instanced
//! quad with a fragment shader — rounded rects and their anti-aliased fringe
//! as an analytic rounded-box SDF, gradients and MSDF text and baked vector
//! fields in the fragment shader, group opacity and backdrop blur as
//! render-to-texture. There is no path primitive at boundary B, and that is
//! what makes this a translation of the paint table into draw calls rather
//! than a 2D rasteriser.

pub mod composite;
pub mod instance;
pub mod pack;
pub mod render;
pub mod residency;
pub mod shader;
pub mod surface;

use dashpaint::{
    ClipTable, GlyphRunTable, GroupComposite, ImageFormat, ImageTable, PaintTable, Painter,
    RectEntry,
};

pub use composite::{Pass, Step};
pub use instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan, Layer};
pub use render::{ATLAS_EXTENT, Changes, InstanceUpload, Renderer, RendererError};
pub use residency::{AtlasFormat, Residency, ResidencyError};
pub use shader::SDF_WGSL;
pub use surface::{Drawn, FrameError, SurfaceRenderer};

/// Which payload formats this painter can be handed, on the device it will draw
/// on.
///
/// The value behind [`Painter::samples`], and the reason that question is asked
/// before a payload is bound rather than inside a frame
/// (`docs/decisions/baked-texel-payloads-cross-boundary-b.md` D6).
///
/// # Why the adapter is part of the answer
///
/// ASTC is a device capability, not a property of this crate: an adapter that
/// does not advertise `TEXTURE_COMPRESSION_ASTC` cannot hold a block texture at
/// all. A painter that claimed the block formats unconditionally would have a
/// host bind a derivation the device then refuses, which is the failure
/// `samples` exists to make impossible. So the declaration is built from an
/// adapter, and [`Default`] is the conservative answer for a painter that has
/// not met one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SampledFormats {
    /// Whether the device can sample ASTC LDR block textures.
    astc: bool,
}

impl SampledFormats {
    /// What a painter drawing on `renderer`'s device can be handed.
    pub fn of(renderer: &Renderer) -> Self {
        Self {
            astc: renderer.samples_astc(),
        }
    }

    /// Whether a payload in `format` can be used as it stands.
    ///
    /// PNG is decoded, the baked formats are uploaded, and JPEG and GIF are
    /// refused: this painter links one decoder, because the trim profile whose
    /// existence justifies the crate removes `libpng`, `libjpeg` and `libwebp`
    /// alike, and `dashpack` derives every canonical container away before a
    /// product build ships (issue #718).
    pub fn contains(self, format: ImageFormat) -> bool {
        match format {
            ImageFormat::Png => true,
            ImageFormat::Jpeg | ImageFormat::Gif => false,
            ImageFormat::Rgba8Srgb | ImageFormat::Rgba8Unorm => true,
            other => {
                debug_assert!(
                    AtlasFormat::of(other).required_feature()
                        == Some(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
                    "a baked format that is neither RGBA8 nor ASTC reached this declaration"
                );
                self.astc
            }
        }
    }
}

/// The lean painter: boundary B's tables in, one ordered instance buffer out.
///
/// It draws nothing itself. What draws the buffer is [`Renderer`], offscreen,
/// or [`SurfaceRenderer`], to a window — the split boundary B's own shape asks
/// for, since a `Painter` is handed tables and returns nothing and a device is
/// not part of that contract.
///
/// It holds the buffer across frames rather than returning a fresh one, so a
/// steady-state frame repacks into an allocation it already has. The other half
/// of what R-T4 asks for — uploading only the changed rects' spans — is the
/// renderer's, and landed with story #585.
#[derive(Debug, Default)]
pub struct GpuPainter {
    /// How many times [`Painter::paint`] has been called.
    frames: u64,
    instances: InstanceBuffer,
    /// What this painter declares it can be handed — see [`SampledFormats`].
    samples: SampledFormats,
}

impl GpuPainter {
    /// A painter that draws nothing, and that claims no baked block format.
    ///
    /// The conservative declaration, because this constructor has met no
    /// adapter and ASTC is an adapter's property. A host that will draw through
    /// a [`Renderer`] should build the painter with [`GpuPainter::on`] instead,
    /// which is what makes `dashpack`'s block output bindable.
    pub fn new() -> Self {
        Self::default()
    }

    /// A painter that will draw on `renderer`'s device, and declares what that
    /// device can sample.
    pub fn on(renderer: &Renderer) -> Self {
        Self {
            samples: SampledFormats::of(renderer),
            ..Self::default()
        }
    }

    /// What this painter declares it can be handed.
    pub fn sampled_formats(&self) -> SampledFormats {
        self.samples
    }

    /// How many times this painter has been asked to paint.
    pub fn frames_painted(&self) -> u64 {
        self.frames
    }

    /// The frame most recently packed — the painter's whole output, what a
    /// layer-1 golden is stated over, and what a renderer is handed to draw.
    pub fn instances(&self) -> &InstanceBuffer {
        &self.instances
    }
}

impl Painter for GpuPainter {
    /// What this painter can be handed, on the device it was built for.
    ///
    /// The first override of the trait's default, which claims the
    /// source-encoded half and nothing else. This one claims less of that half
    /// and more of the other: PNG but not JPEG or GIF, and every baked format
    /// the adapter can sample. That is the whole point of a declaration rather
    /// than a fixed list — see [`SampledFormats`].
    fn samples(&self, format: ImageFormat) -> bool {
        self.samples.contains(format)
    }

    /// The lean painter draws `RectEntry::rotation` (story #832).
    ///
    /// The whole of it is in the vertex stage: the quad's four corners turn
    /// about `Instance::rotation_pivot`, and `out.local` keeps the *unrotated*
    /// coordinate, so every SDF, gradient and image fill in the fragment stage
    /// evaluates in the node's own axis-aligned frame exactly as before. A
    /// rounded rect therefore stays a true rounded rect at an angle rather than
    /// an axis-aligned approximation of one, and no fragment pays for a
    /// rotation it does not have.
    ///
    /// This was `false` between stories #770 and #832, declaring the gap rather
    /// than drawing the node upright — the silent drop P4 forbids.
    fn rotates(&self) -> bool {
        true
    }

    /// Packs the whole of boundary B into [`instances`](Self::instances) and
    /// submits none of it.
    ///
    /// Ignoring `dirty` here is not a placeholder: the set is advisory, and a
    /// painter that repacks everything is always correct. The set is honoured
    /// one level down, where it decides which byte ranges of the instance
    /// buffer are uploaded ([`Renderer::render_dirty`],
    /// [`SurfaceRenderer::present`]). Repacking only the changed rects as well
    /// needs the previous frame's tables held for comparison, which is issue
    /// #708 and not this crate's shape today.
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        groups: &[GroupComposite],
        glyphs: &GlyphRunTable,
        _dirty: Option<&[u32]>,
    ) {
        pack::pack(
            &mut self.instances,
            rects,
            paints,
            images,
            clips,
            groups,
            glyphs,
        );
        self.frames += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seam compiles and can be driven through the trait object, which is
    /// the whole claim this story makes. Called through `&mut dyn Painter`
    /// rather than on the concrete type, because object safety is a property
    /// `painter-trait-infallible-slice-input.md` chose deliberately and a
    /// second implementation is the first chance to check it holds.
    #[test]
    fn the_seam_accepts_an_empty_scene_through_the_trait_object() {
        let mut painter = GpuPainter::new();
        {
            let dynamic: &mut dyn Painter = &mut painter;
            dynamic.paint(
                &[],
                &PaintTable::new(),
                &ImageTable::new(),
                &ClipTable::new(),
                &[],
                &GlyphRunTable::new(),
                None,
            );
        }
        assert_eq!(
            painter.frames_painted(),
            1,
            "the painter was driven, not merely constructed"
        );
    }
}
