//! The presentation seam — how a committed frame reaches a window — and the
//! lean painter's implementation of it (story #571, published at story #794).
//!
//! # Why this trait is here and never in `dashpaint`
//!
//! Boundary B is the painter contract: a rect table, paint indices, clip
//! regions, group composites and glyph runs. Presentation is not part of it.
//! A surface concept in `dashpaint` would make every painter carry a windowing
//! concern it does not have — the golden harness rasters offscreen, the Unity
//! painter draws into a target Unity owns, and neither has a window. So the
//! seam sits in the integration crate, above boundary B, and `dashpaint` is
//! untouched.
//!
//! # Why the trait is published and only one implementation with it
//!
//! Ruled on issue #794. `dashscene-skia` is the reference painter the goldens
//! are taken through, and `skia-safe` is a vendored C++ build; putting
//! `SkiaPresenter` here would put both in the public dependency surface of
//! every `winit` embedder that only wanted a window. The trait is what has to
//! be shared, because [`crate::App::presenter`] hands one back — so `demo`
//! keeps its Skia implementation and implements this trait for it, which is
//! also what keeps story #585's painter swap working against a loop that knows
//! about neither painter.
//!
//! # What the seam has to admit
//!
//! Two implementations exist by design — this one and `demo`'s — and they
//! differ in one structural way that the trait is shaped around.
//!
//! Skia has no windowing at all. It rasters into CPU memory, and the memory is
//! then posted to a window through `softbuffer`. A wgpu painter is the
//! opposite: it owns a `wgpu::Surface` created from the window handle, and its
//! draw calls target a texture acquired from that surface. There is no point
//! at which it can hand anybody a pixel buffer, and asking it to would mean a
//! readback that exists only to satisfy the seam.
//!
//! So the seam is **not** "give the host your pixels". A `Present`
//! implementation owns its painter and owns its surface, and the loop hands it
//! a committed frame and asks for it to appear. What differs between the two —
//! surface acquisition, reconfiguration on resize, and the presentation step
//! itself — is exactly what the trait names. What they share — being driven
//! from one document and one clock — stays in the loop, [`crate::run`].
//!
//! Construction is per-implementation and outside the trait, because this one
//! is fallible in ways (no adapter, no device, no sRGB-encoded format) that
//! have no Skia analogue. An embedder selects one and hands back a
//! `Box<dyn Present>`.
//!
//! One case story #571 named is answered differently from the way it guessed. A
//! surface that reports itself **out of date** is recovered inside `present`,
//! as expected. A surface that reports itself **lost** is not, and cannot be:
//! recovering that means building a new surface from the window handle, and the
//! handle is consumed when the first one is made. It arrives here as
//! [`PresentError::Frame`].
//!
//! **The loop recovers from it, since story #834.** The presenter is dropped and
//! [`crate::App::presenter`] is asked for another — [`crate::Reaction::Rebind`],
//! which is exactly what `dashscene_gpu::FrameError::Lost` says the remedy is,
//! and which the crate had all along. What was missing was the route to it: a
//! present failure ended the loop before an embedder could return a reaction,
//! and `PresentError::Post` flattened the failure to a string so nothing
//! downstream could tell a lost surface from any other one. Issue #818 carried
//! both halves and [`crate::recovery`] is where the decision now lives.

use std::sync::Arc;

use dashpaint::Painter;
use dashscene_core::CommittedScene;
use dashscene_gpu::{Changes, FrameError, GpuPainter, RendererError, SurfaceRenderer};
use winit::window::Window;

/// Whether a frame reached the window.
///
/// Re-exported rather than left in `dashscene-gpu`, because it is part of
/// [`Present::present`]'s signature: an implementation outside this crate —
/// `demo`'s raster presenter is one — would otherwise have to depend on the
/// lean painter to name the type it returns.
pub use dashscene_gpu::Drawn;
/// What an embedder has to name to use [`GpuPresenter::adapter_info`] and
/// [`GpuPresenter::format`], re-exported so that naming one does not oblige it
/// to declare a `wgpu` dependency and keep the version in step with this
/// crate's.
///
/// [`AdapterInfo`] and [`TextureFormat`] are the two the accessors return.
/// [`Backend`] and [`DeviceType`] are the field types a caller branches on —
/// `dashscene-gpu`'s copy of this re-export records which field type is
/// deliberately absent and why.
///
/// **The cost, which is this crate's first:** these are `wgpu` types in a
/// published signature, so a `wgpu` major bump is a breaking change to
/// `dashscene-desktop` even when nothing here changes, and an embedder pinning
/// this crate inherits that cadence. Everything else the crate exposes is
/// wrapped — `from_gpu` flattens `RendererError` for exactly that reason. The
/// trade is deliberate: an accessor returning a type nobody can name is not an
/// accessor, and issue #819 asked for the types rather than the string.
pub use dashscene_gpu::{AdapterInfo, Backend, DeviceType, TextureFormat};

/// What a presenter can say about the device it draws on.
///
/// The pair rather than either alone, because they are answered together: a
/// presenter that has an adapter has a swapchain format, and an embedder
/// branching on one usually wants the other — warn on a software adapter,
/// choose a texture path by format.
///
/// Borrowed, so answering costs nothing. `AdapterInfo` holds four `String`s and
/// a presenter is asked this on every attach.
#[derive(Clone, Copy, Debug)]
pub struct Adapter<'a> {
    /// The adapter itself.
    pub info: &'a AdapterInfo,
    /// The format the swapchain was configured with.
    pub format: TextureFormat,
}

/// Puts a committed frame on a window.
///
/// An implementation owns both the painter and the surface it presents to;
/// see the module documentation for why the seam is drawn there.
pub trait Present {
    /// The painter behind this presenter, for the frame-budget record and for
    /// telling two selectable painters apart at run time.
    ///
    /// Borrowed rather than `&'static str`: a raster painter's name is a
    /// literal, but this one's is only known once a device exists, since it
    /// carries the adapter, the backend and the swapchain format it actually
    /// got. Those are exactly what a frame-budget record has to state, and the
    /// alternative — leaking a formatted string to make it `'static` — is a
    /// leak per painter swap in exchange for nothing.
    fn name(&self) -> &str;

    /// Reconfigures for a drawable of `width` x `height` **physical** pixels.
    ///
    /// A zero dimension is not an error: a minimised window reports one, and
    /// the correct response is to configure nothing and wait. The loop
    /// calls this on a resize and on a scale-factor change; how the document is
    /// re-solved for the new extent is the loop's business, not the
    /// presenter's.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;

    /// The document this presenter has been drawing has been replaced, and
    /// nothing it holds describes the frames that follow.
    ///
    /// The loop rebuilds its arena on a resize and whenever an embedder asks
    /// for it, and a fresh arena's commit generations count from the start. A
    /// presenter that carries per-document state — `dashscene-gpu` holds a copy
    /// of what the device's instance buffer contains, and patches it by
    /// generation — cannot see that from the frames alone: the new arena's
    /// generation *G+1* follows the old one's *G* by arithmetic, and one scene
    /// rebuilt at a new extent has exactly the spans it had before.
    ///
    /// No default body, deliberately. A no-op default is what a new presenter
    /// would inherit without noticing, and the failure it causes is a stale
    /// picture rather than an error.
    fn document_replaced(&mut self);

    /// Draws `scene` and puts the result on the window, reporting whether
    /// anything reached it.
    ///
    /// The frame is drawn in full. Neither v0 painter has a partial-redraw
    /// path — the retained mode patches its instance buffer and still redraws
    /// every quad — so skipping work is a decision about whether a frame runs
    /// at all, which the loop makes by not calling this.
    ///
    /// [`Drawn::No`] is not an error and not rare: a minimised window, a
    /// zero-area drawable, an occluded surface and a timed-out acquire all
    /// reach it. It is reported because a caller that **measures** frames has
    /// to exclude the ones that did not happen — see [`Drawn`] for the
    /// measurement this distinction was added for.
    fn present(&mut self, scene: &CommittedScene) -> Result<Drawn, PresentError>;

    /// The device this presenter draws on, for a presenter that has one.
    ///
    /// The loop passes the answer to [`crate::App::attached`], which is the
    /// only way an embedder that did not build the presenter can reach it: the
    /// loop holds a `Box<dyn Present>` and this trait has no downcast. Before
    /// issue #902 there was no way at all, and the accessors on
    /// [`GpuPresenter`] were reachable only by an embedder that overrode
    /// [`crate::App::presenter`] and read the presenter before boxing it.
    ///
    /// **Defaulted, unlike [`Present::document_replaced`], and for the opposite
    /// reason.** A presenter is not obliged to have an adapter — `demo`'s
    /// raster one rasters into CPU memory and has none — so a required method
    /// would make every implementation answer a question only some can, which
    /// is what issue #819 ruled against. A presenter that inherits this default
    /// gives the true answer for a presenter with no device, so nothing goes
    /// wrong by not noticing it; the no-op default that record warns about is
    /// the kind that is silently *wrong* for the inheritor.
    fn adapter(&self) -> Option<Adapter<'_>> {
        None
    }
}

/// Why a frame did not reach the window.
#[derive(Debug)]
pub enum PresentError {
    /// The window's surface could not be created or reconfigured.
    Surface(String),
    /// The frame was drawn but could not be handed to the compositor.
    ///
    /// The raster presenter's variant: `demo`'s implementation posts a pixel
    /// buffer through `softbuffer`, which has no `FrameError` to carry. A
    /// `dashscene-gpu` failure arrives as [`PresentError::Frame`] instead.
    Post(String),
    /// A `dashscene-gpu` frame failure, carried rather than flattened.
    ///
    /// The loop branches on it: [`dashscene_gpu::FrameError::is_recoverable`]
    /// says whether the remedy is [`crate::Reaction::Rebind`]. This was
    /// `Post(error.to_string())` until story #834, which meant the one entry
    /// point that could recover a lost surface was unreachable from the failure
    /// it exists for (issue #818).
    Frame(FrameError),
    /// The drawable is larger than the painter can address on either axis, and
    /// `max` is the largest either may be.
    ///
    /// One variant for every painter deliberately, so a host reports the same
    /// refusal whichever is running. A raster painter reaches it through
    /// `skia-safe` taking surface dimensions as `i32`, where a request past
    /// `i32::MAX` would wrap into a negative extent; `dashscene-gpu` is bounded
    /// by the device's own `max_texture_dimension_2d`, and configuring a
    /// swapchain past it aborts the process instead of returning (issue #714).
    /// Neither is a number the painter chose, which is why the one it did not
    /// meet is reported alongside the one it was asked for.
    Extent { width: u32, height: u32, max: u32 },
    /// The framebuffer the windowing system handed back does not hold the
    /// number of pixels the painter drew. Reported rather than truncated: a
    /// short copy would post a torn or shifted picture, and a picture that is
    /// wrong in a way nobody named is worse than a frame that did not appear.
    ///
    /// Unreachable from this crate's own presenter, which has no readback and
    /// no framebuffer. It is here because it is the seam's vocabulary rather
    /// than one implementation's: `demo`'s raster presenter returns it, and a
    /// variant an implementation outside this crate cannot name would leave it
    /// stringifying the one refusal a caller can act on.
    ExtentMismatch { painted: usize, framebuffer: usize },
}

impl std::fmt::Display for PresentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(message) => write!(f, "window surface: {message}"),
            Self::Post(message) => write!(f, "posting the frame: {message}"),
            // Deliberately not the same prefix as `Post`. The two are different
            // failures — one is a raster presenter that could not hand its
            // pixels to the compositor, the other a `dashscene-gpu` frame that
            // may be recoverable — and a log line that reads identically for
            // both would undo the split this variant exists for.
            Self::Frame(error) => write!(f, "the frame did not reach the window: {error}"),
            Self::Extent { width, height, max } => write!(
                f,
                "a {width}x{height} drawable exceeds the {max} px maximum this painter can \
                 address on either dimension"
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

impl std::error::Error for PresentError {
    /// The `dashscene-gpu` failure, for a caller walking the chain.
    ///
    /// Without this the chain stops here, and an embedder following `source()`
    /// to reach the structured failure — which is the reason
    /// [`PresentError::Frame`] carries one rather than a string — would get this
    /// type and then `None`. The string variants have no source, because a
    /// string is where their chain already ended.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            _ => None,
        }
    }
}

/// The lean painter: pack on the CPU, draw on the GPU, present to the window's
/// own swapchain (story #585).
///
/// There is no readback and no blit. `dashscene-gpu` owns the surface and the
/// device that has to agree with it, so this presenter is the seam and nothing
/// else: it packs boundary B through the painter and hands the frame over.
///
/// This is what [`crate::App::presenter`] hands back when an embedder does not
/// override it, so a minimal embedder writes no presenter at all.
pub struct GpuPresenter {
    /// The boundary-B implementation. It produces the instance buffer and knows
    /// nothing about the window.
    painter: GpuPainter,
    /// The device, the pipeline and the swapchain, from `dashscene-gpu`. It
    /// holds the drawable extent as well: this presenter keeps no copy, so
    /// there is no second record of the extent to disagree with the first.
    renderer: SurfaceRenderer,
    /// What [`Present::name`] reports. Built once at construction rather than
    /// returned as a literal, because the interesting half of it — the adapter,
    /// the backend and the format the swapchain agreed to — is only known once
    /// a device exists.
    name: String,
}

impl GpuPresenter {
    /// Binds the lean painter and a swapchain to `window`.
    ///
    /// The window's current inner size is adopted as the drawable extent, so a
    /// caller that never resizes still gets a correctly sized first frame.
    ///
    /// `Arc<Window>` rather than `&Window` or `Rc<Window>` because
    /// `wgpu::Instance::create_surface` requires a window handle that is
    /// `'static + Send + Sync` and owned rather than borrowed.
    pub fn new(window: Arc<Window>) -> Result<Self, PresentError> {
        let size = window.inner_size();
        let renderer = SurfaceRenderer::new(window, size.width, size.height).map_err(from_gpu)?;
        let info = renderer.adapter_info();
        let name = format!(
            "dashscene-gpu ({}, {:?}, {:?})",
            info.name,
            info.backend,
            renderer.format()
        );
        Ok(Self {
            painter: GpuPainter::new(),
            renderer,
            name,
        })
    }

    /// The adapter this presenter acquired.
    ///
    /// Inherent rather than on [`Present`], because a presenter is not obliged
    /// to have an adapter — `demo`'s raster one does not — and a trait method
    /// every implementation had to answer would be the wrong shape for it
    /// (issue #819).
    ///
    /// For an embedder that wants to show the backend in its own interface, or
    /// branch on it: warn on a software adapter, choose a texture path by
    /// format. [`Present::name`] stays for the caller that only wants the line,
    /// and this is for the one that would otherwise have had to parse it.
    ///
    /// This is the concrete answer, for an embedder holding a `GpuPresenter`.
    /// An embedder that took the default presenter holds a `Box<dyn Present>`
    /// instead, and reads the same two facts from [`Adapter`] through
    /// [`crate::App::attached`] — which is what closed issue #902, where they
    /// were reachable from the default path not at all.
    pub fn adapter_info(&self) -> &AdapterInfo {
        self.renderer.adapter_info()
    }

    /// The swapchain format this window was configured with.
    pub fn format(&self) -> TextureFormat {
        self.renderer.format()
    }
}

/// Carries a `dashscene-gpu` failure across the seam.
///
/// Only the extent keeps its own variant: it is the one refusal every painter
/// shares, and the one a person reading the message can act on by making the
/// window smaller. Everything else is a surface that could not be built and
/// reads as one.
fn from_gpu(error: RendererError) -> PresentError {
    match error {
        RendererError::Extent { width, height, max } => PresentError::Extent { width, height, max },
        other => PresentError::Surface(other.to_string()),
    }
}

impl Present for GpuPresenter {
    fn name(&self) -> &str {
        &self.name
    }

    fn document_replaced(&mut self) {
        self.renderer.document_replaced();
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        // The ceiling is the device's rather than a raster painter's `i32`, and
        // the renderer is the only thing that knows it. It is checked there
        // rather than here because configuring a swapchain past it is not an
        // error wgpu returns — it is a non-unwinding panic (issue #714).
        self.renderer.resize(width, height).map_err(from_gpu)
    }

    fn adapter(&self) -> Option<Adapter<'_>> {
        Some(Adapter {
            info: self.adapter_info(),
            format: self.format(),
        })
    }

    fn present(&mut self, scene: &CommittedScene) -> Result<Drawn, PresentError> {
        // No early return on a zero extent, deliberately. The renderer is the
        // one that decides a frame cannot be drawn, because it is also the one
        // that has to know a frame was not drawn: a commit that never reaches
        // the device must not be treated as the predecessor of the next one.
        //
        // The painter takes the dirty rects; the renderer takes them *and* the
        // generation they were reported against. This painter repacks every
        // rect either way — the set is honoured one level down, where it
        // decides which byte ranges of the instance buffer are uploaded (R-T4,
        // `crates/dashscene-gpu/src/render.rs`). The generation travels with it
        // so that a declined frame breaks the chain by arithmetic rather than
        // by anyone remembering to say so.
        let changes = Changes {
            rects: scene.dirty(),
            generation: scene.generation(),
        };
        self.painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            Some(changes.rects),
        );
        self.renderer
            .present(
                self.painter.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                scene.glyphs(),
                Some(changes),
            )
            .map_err(PresentError::Frame)
    }
}
