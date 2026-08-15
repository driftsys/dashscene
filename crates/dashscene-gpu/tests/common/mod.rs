//! Shared helpers for this crate's layer-3 test binaries (issue #995).
//!
//! Each binary compiles its own copy of this module, so a helper unused by one
//! is still used by another — hence the `dead_code` allowance, the same pattern
//! `dashc`'s and `dashscene-typeset`'s own `tests/common` use.
//!
//! What belongs here is what three files had agreed on and were keeping in step
//! by hand. What does not is a helper one file needs: `near`, `one_glyph`,
//! `mask_field` and the scene builders stay where they are used, because a
//! shared module that collects every helper is a second place to look rather
//! than one place to change.

#![allow(dead_code)]

use dashscene_gpu::Renderer;

/// The extent every layer-3 fixture draws at.
///
/// One pair rather than four, because [`texel`] indexes by `W` and a file that
/// changed its own would read the wrong row of a picture drawn at the other.
pub const W: u32 = 64;
pub const H: u32 = 48;

/// A renderer, or a named failure.
///
/// Panics through `Display` rather than `expect`, which prints `Debug`:
/// `RendererError::NoAdapter` carries the sentence naming the environment —
/// that a runner needs a software device installed — and `Debug` renders it as
/// the bare word `NoAdapter`, losing exactly the part a reader needs. That
/// argument is `layer3_render_smoke`'s, and this is where it now lives for all
/// four binaries.
pub fn renderer() -> Renderer {
    Renderer::new().unwrap_or_else(|e| panic!("layer 3 needs a device: {e}"))
}

/// The unpremultiplied RGBA texel at `(x, y)` of a frame drawn at [`W`] x [`H`].
pub fn texel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// The corpus payload this painter links no decoder for, and the one every
/// refusal fixture is built on.
///
/// **A corpus path, which is why this is shared** (issue #995). Three test
/// binaries named it, each with its own prose restating why this payload is the
/// one to use, and a re-capture that moved or renamed
/// `corpus/figma-fixtures/jpeg-fill.images/` broke three compilation units and
/// left three explanations to be brought back into step.
///
/// The bytes are a real JPEG whose header parses, so `ImageTable::push` derives
/// an extent from it and `resolve_frame`'s no-extent arm cannot answer first.
/// What every test using it turns on is that the **decode** never happens —
/// `dashscene-gpu` links one decoder and that is `png` — so a payload that
/// decoded would prove less rather than more.
pub const JPEG_FIXTURE: &[u8] = include_bytes!(
    "../../../../corpus/figma-fixtures/jpeg-fill.images/4045fd0419fbcbbd03505d2d258c6dbbeb2da1fe.jpg"
);

/// The 7x5 PNG `dashpaint`'s own tests use, for the encoded-payload path.
///
/// Shared for the reason [`JPEG_FIXTURE`] is, and it was the more duplicated of
/// the two: `layer3_image_fills` named it twice on its own. `layer1_instances`
/// keeps its own copy — it declares no `mod common`, and it is not a layer-3
/// binary.
pub const SAMPLE_PNG: &[u8] = include_bytes!("../fixtures/image_id/sample.png");
