//! The lean painter — instanced quads and analytic SDF over `wgpu`, covering
//! native and web from one codebase
//! (`docs/decisions/wgpu-is-the-lean-painter.md`).
//!
//! # Status: the seam, and nothing behind it
//!
//! This crate is the v0.15 slice's first story (#577) and holds an
//! implementation of [`dashpaint::Painter`] that draws nothing. That is the
//! deliverable: boundary B is "the entire painter input"
//! (`docs/design/architecture.md`), so the value of compiling a second
//! implementation against it — before any pixel exists — is that the trait is
//! proven to be implementable by something other than the crate it was shaped
//! alongside.
//!
//! It carries no `wgpu` dependency yet. The instance buffer arrives with story
//! #578, the shader library with #579, and the device, pipelines and first
//! pixels with #580.
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

use dashpaint::{
    ClipTable, GlyphRunTable, GroupComposite, ImageTable, PaintTable, Painter, RectEntry,
};

/// The lean painter. Draws nothing yet (story #577).
///
/// Deliberately holds no state: a device, a queue, a surface and the pipelines
/// over them arrive with story #580, and inventing fields for them now would
/// be guessing at a shape that story is meant to decide.
#[derive(Debug, Default)]
pub struct GpuPainter {
    /// How many times [`Painter::paint`] has been called. The only thing this
    /// painter can honestly report until it owns a device, and the only way a
    /// test can tell that the seam was actually driven rather than merely
    /// compiled.
    frames: u64,
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
}

impl Painter for GpuPainter {
    /// Accepts the whole of boundary B and draws none of it.
    ///
    /// Every parameter is ignored by name rather than by a blanket `_`, so
    /// that the next story to implement one deletes an underscore instead of
    /// re-deriving the signature — and so that a widening of the trait breaks
    /// this file, which is the point of having a second implementation at all.
    ///
    /// Ignoring `dirty` is not a placeholder: the set is advisory, and a
    /// painter that redraws everything is always correct. This one redraws
    /// nothing, which is trivially identical to what honouring the set would
    /// have produced.
    fn paint(
        &mut self,
        _rects: &[RectEntry],
        _paints: &PaintTable,
        _images: &ImageTable,
        _clips: &ClipTable,
        _groups: &[GroupComposite],
        _glyphs: &GlyphRunTable,
        _dirty: Option<&[u32]>,
    ) {
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
