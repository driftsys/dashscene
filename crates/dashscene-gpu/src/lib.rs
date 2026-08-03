//! The lean painter — instanced quads and analytic SDF over `wgpu`, covering
//! native and web from one codebase
//! (`docs/decisions/wgpu-is-the-lean-painter.md`).
//!
//! # Status: the frame is packed, and nothing draws it
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
//! Nothing submits a frame and nothing draws. The crate names `wgpu` only in
//! its dev-dependencies, where the conformance suite needs a device; the
//! painter's own device, its pipelines and its first pixels are story #580.
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

pub mod instance;
pub mod pack;
pub mod shader;

use dashpaint::{
    ClipTable, GlyphRunTable, GroupComposite, ImageTable, PaintTable, Painter, RectEntry,
};

pub use instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan};
pub use shader::SDF_WGSL;

/// The lean painter. Packs a frame into its instance buffer and draws none of
/// it: a device, a queue, a surface and the pipelines over them arrive with
/// story #580.
///
/// It holds the buffer across frames rather than returning a fresh one, so a
/// steady-state frame repacks into an allocation it already has. Uploading
/// only the changed rects' spans is the other half of what R-T4 asks for, and
/// it needs a device — story #580's.
#[derive(Debug, Default)]
pub struct GpuPainter {
    /// How many times [`Painter::paint`] has been called.
    frames: u64,
    instances: InstanceBuffer,
}

impl GpuPainter {
    /// A painter that draws nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times this painter has been asked to paint.
    pub fn frames_painted(&self) -> u64 {
        self.frames
    }

    /// The frame most recently packed — the painter's whole output until
    /// story #580 gives it a device, and what a layer-1 golden is stated over.
    pub fn instances(&self) -> &InstanceBuffer {
        &self.instances
    }
}

impl Painter for GpuPainter {
    /// Packs the whole of boundary B into [`instances`](Self::instances) and
    /// submits none of it.
    ///
    /// Ignoring `dirty` is not a placeholder: the set is advisory, and a
    /// painter that repacks everything is always correct. Honouring it means
    /// rewriting only the changed rects' spans, which needs the previous
    /// frame's tables to compare against and a device to upload to — story
    /// #580's, and the property R-T4 is stated over.
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
