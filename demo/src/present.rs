//! The presentation seam — how a committed frame reaches a window — and the
//! Skia implementation behind it (story #571).
//!
//! # Why this trait lives here and never in `dashpaint`
//!
//! Boundary B is the painter contract: a rect table, paint indices, clip
//! regions, group composites and glyph runs. Presentation is not part of it.
//! A surface concept in `dashpaint` would make every painter carry a windowing
//! concern it does not have — the golden harness rasters offscreen, the Unity
//! painter draws into a target Unity owns, and neither has a window. So the
//! seam sits in the host, above boundary B, and `dashpaint` is untouched.
//!
//! # What the seam has to admit
//!
//! Two implementations exist by design: this one, and `dashscene-wgpu` in
//! v0.15. They differ in one structural way, and the trait is shaped around
//! it.
//!
//! Skia has no windowing at all. It rasters into CPU memory, and the memory is
//! then posted to a window — here, through `softbuffer`. A wgpu painter is the
//! opposite: it owns a `wgpu::Surface` created from the window handle, and its
//! draw calls target a texture acquired from that surface. There is no point
//! at which it can hand anybody a pixel buffer, and asking it to would mean a
//! readback that exists only to satisfy the seam.
//!
//! So the seam is **not** "give the host your pixels". A `Present`
//! implementation owns its painter and owns its surface, and the host hands it
//! a committed frame and asks for it to appear. What differs between the two
//! implementations — surface acquisition, reconfiguration on resize, and the
//! presentation step itself — is exactly what the trait names. What they share
//! — being driven from one document and one clock — stays in the host.
//!
//! Concretely, `dashscene-wgpu` sits behind this trait as follows.
//!
//! - **Construction.** [`SkiaPresenter::new`] takes an `Arc<Window>`; a
//!   `WgpuPresenter::new` takes the same `Arc<Window>` and passes it to
//!   `wgpu::Instance::create_surface`, which requires a window handle that is
//!   `'static + Send + Sync` and owned rather than borrowed. That requirement
//!   is why the constructor takes `Arc<Window>` rather than `&Window` or
//!   `Rc<Window>` — the Skia path does not need it, and the seam pays for it
//!   now so the wgpu path does not have to change the signature later.
//!   Construction is per-implementation and outside the trait, because the
//!   wgpu one is fallible in ways (no adapter, no device) that have no Skia
//!   analogue; the host selects one and stores the result as
//!   `Box<dyn Present>`.
//! - **[`Present::resize`].** Here it re-allocates the raster surface and
//!   resizes the `softbuffer` surface. There it calls `Surface::configure`
//!   with the new extent. Both take physical pixels and both must tolerate a
//!   zero dimension, which is what a minimised window reports.
//! - **[`Present::present`].** Here it paints into the raster surface, reads
//!   the pixels back, and posts them. There it calls
//!   `Surface::get_current_texture`, paints into that texture's view, submits
//!   the queue, and calls `present` on the frame. A lost or outdated surface
//!   is recovered inside that call by reconfiguring and retrying, because the
//!   presenter owns the surface and nothing above it can act on the condition.
//! - **[`Present::name`].** The slice records a frame budget by hand with the
//!   painter named beside the number, and v0.15 runs both painters against one
//!   document. The name is what the host prints so the two are told apart.
//!
//! Nothing in the trait mentions a pixel buffer, a colour format, or a raster
//! surface, and that is the property that makes it a seam rather than a
//! description of the Skia path.

use std::num::NonZeroU32;
use std::sync::Arc;

use dashpaint::Painter;
use dashscene_core::CommittedScene;
use dashscene_skia::SkiaPainter;
use winit::window::Window;

/// Puts a committed frame on a window.
///
/// An implementation owns both the painter and the surface it presents to;
/// see the module documentation for why the seam is drawn there.
pub trait Present {
    /// The painter behind this presenter, for the frame-budget record and for
    /// telling two selectable painters apart at run time.
    fn name(&self) -> &'static str;

    /// Reconfigures for a drawable of `width` x `height` **physical** pixels.
    ///
    /// A zero dimension is not an error: a minimised window reports one, and
    /// the correct response is to configure nothing and wait. The host calls
    /// this on a resize and on a scale-factor change; how the document is
    /// re-solved for the new extent is the host's business, not the
    /// presenter's.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;

    /// Draws `scene` and puts the result on the window.
    ///
    /// The frame is drawn in full. Neither v0 painter has a partial-redraw
    /// path — `dashscene-skia`'s retained mode patches its instance buffer and
    /// still redraws every quad — so skipping work is a decision about whether
    /// a frame runs at all, which the host makes by not calling this.
    fn present(&mut self, scene: &CommittedScene) -> Result<(), PresentError>;
}

/// Why a frame did not reach the window.
#[derive(Debug)]
pub enum PresentError {
    /// The window's surface could not be created or reconfigured.
    Surface(String),
    /// The frame was drawn but could not be handed to the compositor.
    Post(String),
    /// The drawable is larger than the painter can address. `skia-safe` takes
    /// surface dimensions as `i32`, so a request past `i32::MAX` is refused by
    /// name rather than wrapped into a negative extent.
    Extent { width: u32, height: u32 },
    /// The framebuffer the windowing system handed back does not hold the
    /// number of pixels the painter drew. Reported rather than truncated: a
    /// short copy would post a torn or shifted picture, and a picture that is
    /// wrong in a way nobody named is worse than a frame that did not appear.
    ExtentMismatch { painted: usize, framebuffer: usize },
}

impl std::fmt::Display for PresentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(message) => write!(f, "window surface: {message}"),
            Self::Post(message) => write!(f, "posting the frame: {message}"),
            Self::Extent { width, height } => write!(
                f,
                "a {width}x{height} drawable exceeds the largest raster surface the painter can \
                 address"
            ),
            Self::ExtentMismatch {
                painted,
                framebuffer,
            } => write!(
                f,
                "the painter drew {painted} pixels and the framebuffer holds {framebuffer}"
            ),
        }
    }
}

impl std::error::Error for PresentError {}

/// The Skia implementation: raster on the CPU, then post the pixels.
///
/// No GL context, no Ganesh, and no `skia_use_gl` build — `skia-safe` is
/// pinned here with none of the `gl`, `vulkan` or `metal` features, and this
/// keeps it that way. Skia provides no windowing of any kind, so wrapping the
/// raster it already produces is both the cheapest correct path and the one
/// that does not tie the project to the Ganesh-to-Graphite transition.
pub struct SkiaPresenter {
    painter: SkiaPainter,
    /// The `softbuffer::Context` it was built from is deliberately **not**
    /// kept: `Surface<D, W>` carries no lifetime tied to the context, and each
    /// backend that needs the display connection clones it out of the context
    /// behind an `Arc` (`X11Impl` and `WaylandImpl` both hold
    /// `Arc<...DisplayImpl<D>>`; the CoreGraphics backend holds nothing from
    /// it). Storing the context as well would suggest a dependency that does
    /// not exist.
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    /// The painter's readback, held across frames rather than allocated per
    /// frame. It is 9.2 MB at 1920x1200, and a presenter posts one of these
    /// every frame it runs, so allocating it in [`Present::present`] put a
    /// window-sized allocation and its first-touch page faults on the frame
    /// path for nothing (issue #603). [`SkiaPainter::read_premul_into`]
    /// resizes it only when the extent changes and overwrites every byte.
    frame: Vec<u8>,
    /// The drawable extent in physical pixels, as last configured. Zero on
    /// either axis means there is nothing to draw into.
    width: u32,
    height: u32,
}

impl SkiaPresenter {
    /// Binds a raster painter and a `softbuffer` surface to `window`.
    ///
    /// The window's current inner size is adopted as the drawable extent, so a
    /// caller that never resizes still gets a correctly sized first frame.
    pub fn new(window: Arc<Window>) -> Result<Self, PresentError> {
        let context = softbuffer::Context::new(Arc::clone(&window))
            .map_err(|error| PresentError::Surface(error.to_string()))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window))
            .map_err(|error| PresentError::Surface(error.to_string()))?;
        let size = window.inner_size();
        // A 1x1 placeholder: `SkiaPainter::new` refuses a non-positive extent,
        // and `resize` below installs the real one. It is never drawn into,
        // because `present` returns early until an extent is configured.
        let mut presenter = Self {
            painter: SkiaPainter::new(1, 1),
            surface,
            frame: Vec::new(),
            width: 0,
            height: 0,
        };
        presenter.resize(size.width, size.height)?;
        Ok(presenter)
    }
}

impl Present for SkiaPresenter {
    fn name(&self) -> &'static str {
        "dashscene-skia (CPU raster, softbuffer blit)"
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        // A window system delivers `Resized` with an unchanged extent: macOS
        // does during a drag, and a scale-factor change reports the same
        // physical size. Reconfiguring for it would allocate a whole raster
        // surface — about 9.2 MB at 1920x1200 — for no change in output.
        //
        // Correct against the one state that could make the stored pair agree
        // with an unconfigured surface: the constructor leaves it (0, 0) and
        // immediately calls this. An early-out there means the requested
        // extent is also zero, which is the branch below that configures
        // nothing on purpose.
        if width == self.width && height == self.height {
            return Ok(());
        }

        let (Some(nz_width), Some(nz_height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
        else {
            // Minimised, or a drawable the windowing system has not sized yet.
            // Forget the extent so `present` stays out of the way until a real
            // one arrives.
            self.width = 0;
            self.height = 0;
            return Ok(());
        };
        let (Ok(raster_width), Ok(raster_height)) = (i32::try_from(width), i32::try_from(height))
        else {
            return Err(PresentError::Extent { width, height });
        };
        self.surface
            .resize(nz_width, nz_height)
            .map_err(|error| PresentError::Surface(error.to_string()))?;
        self.painter = SkiaPainter::new(raster_width, raster_height);
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn present(&mut self, scene: &CommittedScene) -> Result<(), PresentError> {
        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        // `None` rather than `scene.dirty()`: this painter is in
        // `DirtyMode::Full`, which ignores the set and redraws every rect, and
        // the set is advisory precisely so a caller may decline to pass it.
        self.painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            None,
        );
        self.painter.read_premul_into(&mut self.frame);

        let mut framebuffer = self
            .surface
            .buffer_mut()
            .map_err(|error| PresentError::Post(error.to_string()))?;
        let painted = self.frame.len() / 4;
        if painted != framebuffer.len() {
            return Err(PresentError::ExtentMismatch {
                painted,
                framebuffer: framebuffer.len(),
            });
        }
        pack_premul_over_black(&self.frame, &mut framebuffer);
        framebuffer
            .present()
            .map_err(|error| PresentError::Post(error.to_string()))
    }
}

/// Composites **premultiplied** RGBA8888 rows over opaque black into
/// `softbuffer`'s `0RGB` words.
///
/// # Why compositing costs nothing here
///
/// A window is opaque and takes no alpha, so every pixel the painter drew has
/// to be composited over an opaque background — the painter clears its
/// surface to transparent, so a pixel no rect covers arrives with alpha 0 and
/// a partially covered one arrives scaled by its coverage. Simply dropping
/// alpha would draw a 10 %-opacity white as opaque white.
///
/// Source-over with a premultiplied source is
/// `dst = src + (1 - alpha) * backdrop`. The backdrop is black, so the second
/// term is zero for every channel and `dst = src`: **the composite of a
/// premultiplied pixel over black is the pixel**. That identity is the whole
/// correctness argument for this function having no arithmetic in it, and it
/// is why the readback asks Skia for `AlphaType::Premul` rather than the
/// `Unpremul` the golden route uses.
///
/// It was written the other way round until issue #603: the readback divided
/// every channel by alpha and this function multiplied it back. Beyond being
/// two passes of work that cancel, the integer division truncates, so every
/// semi-transparent pixel reached the window up to one code point darker per
/// channel than the value the surface holds.
///
/// # Channel order
///
/// This is the one conversion that remains, and neither side of it is
/// negotiable. The readback is RGBA8888, one byte per channel, in the order
/// the golden tooling and the `.dsb` colour tables use; `softbuffer` takes one
/// `u32` per pixel laid out as `0RGB`, red in bits 16..24. So the blit
/// swizzles, and dropping alpha is what leaves the high byte zero.
fn pack_premul_over_black(premul: &[u8], framebuffer: &mut [u32]) {
    for (pixel, word) in premul.chunks_exact(4).zip(framebuffer.iter_mut()) {
        *word = (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
    }
}

#[cfg(test)]
mod tests {
    use dashpaint::{Color, Painter};
    use dashscene_core::{Arena, Prop};
    use dashscene_skia::SkiaPainter;

    use super::pack_premul_over_black;

    /// What the blit did before issue #603: take the unpremultiplied readback
    /// and multiply every channel by alpha on the way into the framebuffer.
    ///
    /// Kept in the test module only, as the reference the new path is measured
    /// against by `the_premultiplied_path_matches_the_round_trip_it_replaced`.
    /// Nothing outside these tests may call it — it is the code the change
    /// removed.
    fn legacy_composite_over_black(unpremul: &[u8], framebuffer: &mut [u32]) {
        for (pixel, word) in unpremul.chunks_exact(4).zip(framebuffer.iter_mut()) {
            let alpha = u32::from(pixel[3]);
            let red = u32::from(pixel[0]) * alpha / 255;
            let green = u32::from(pixel[1]) * alpha / 255;
            let blue = u32::from(pixel[2]) * alpha / 255;
            *word = (red << 16) | (green << 8) | blue;
        }
    }

    /// Pins the channel order alone. It says nothing about whether the
    /// demonstration draws anything — that is confirmed by running it, not by
    /// this suite (epic #568: `cargo build -p demo` is the only CI claim).
    #[test]
    fn an_opaque_pixel_keeps_its_channels_in_softbuffer_order() {
        let mut framebuffer = [0u32; 1];
        pack_premul_over_black(&[0x12, 0x34, 0x56, 0xff], &mut framebuffer);
        assert_eq!(framebuffer[0], 0x0012_3456);
    }

    /// A fully transparent pixel is all zeroes once premultiplied, whatever
    /// colour it was authored in, so this also pins that the framebuffer word
    /// is overwritten rather than blended into.
    #[test]
    fn a_fully_transparent_pixel_composites_to_black() {
        let mut framebuffer = [0xffff_ffffu32; 1];
        pack_premul_over_black(&[0x00, 0x00, 0x00, 0x00], &mut framebuffer);
        assert_eq!(framebuffer[0], 0);
    }

    /// The same word the old path produced from the unpremultiplied
    /// `[0xff, 0xff, 0xff, 0x80]`, which is what this premultiplied pixel is
    /// the surface's own storage of.
    #[test]
    fn a_half_transparent_white_composites_to_half_grey() {
        let mut framebuffer = [0u32; 1];
        pack_premul_over_black(&[0x80, 0x80, 0x80, 0x80], &mut framebuffer);
        assert_eq!(framebuffer[0], 0x0080_8080);
    }

    /// The premultiplied path must put the same picture on the window as the
    /// unpremultiplied round trip did, apart from the precision that round
    /// trip was losing.
    ///
    /// This is the assertion that issue #603 removed a cancellation rather
    /// than changing what is drawn, and it is the only one that can make it:
    /// the blit is not on the golden route — goldens go through `png_bytes`
    /// and `rgba_bytes` — so no golden moves whether this is right or wrong.
    ///
    /// The bound is one code point per channel, which is exactly what the
    /// integer divide-then-multiply could lose.
    #[test]
    fn the_premultiplied_path_matches_the_round_trip_it_replaced() {
        const EXTENT: i32 = 8;
        let mut arena = Arena::new();
        let mut txn = arena.open();
        let root = txn.add_node(None, Some("bg"));
        txn.set_prop(root, Prop::Width(EXTENT as f32));
        txn.set_prop(root, Prop::Height(EXTENT as f32));
        // Semi-transparent on purpose: an opaque fill round-trips byte-exact
        // through the unpremultiplied readback, so a scene of opaque rects
        // would agree with the old path for the wrong reason.
        txn.set_prop(
            root,
            Prop::Fill(Color {
                r: 0.87,
                g: 0.31,
                b: 0.13,
                a: 0.4,
            }),
        );
        let wash = txn.add_node(Some(root), Some("wash"));
        txn.set_prop(wash, Prop::X(2.0));
        txn.set_prop(wash, Prop::Y(2.0));
        txn.set_prop(wash, Prop::Width(4.0));
        txn.set_prop(wash, Prop::Height(4.0));
        txn.set_prop(
            wash,
            Prop::Fill(Color {
                r: 0.05,
                g: 0.62,
                b: 0.94,
                a: 0.55,
            }),
        );
        txn.commit();

        let scene = arena.committed();
        let mut painter = SkiaPainter::new(EXTENT, EXTENT);
        painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            None,
        );

        let unpremul = painter.rgba_bytes();
        let mut premul = Vec::new();
        painter.read_premul_into(&mut premul);
        assert_eq!(premul.len(), unpremul.len());

        let pixels = (EXTENT * EXTENT) as usize;
        let mut before = vec![0u32; pixels];
        legacy_composite_over_black(&unpremul, &mut before);
        let mut after = vec![0u32; pixels];
        pack_premul_over_black(&premul, &mut after);

        for (index, (old, new)) in before.iter().zip(after.iter()).enumerate() {
            assert_eq!(new >> 24, 0, "pixel {index}: the high byte must stay clear");
            for shift in [16u32, 8, 0] {
                let old_channel = i32::from((old >> shift) as u8);
                let new_channel = i32::from((new >> shift) as u8);
                assert!(
                    (old_channel - new_channel).abs() <= 1,
                    "pixel {index}, channel at bit {shift}: {old_channel:#04x} became \
                     {new_channel:#04x}, which is more than the round trip could lose"
                );
            }
        }
    }
}
