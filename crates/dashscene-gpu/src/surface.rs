//! Drawing to a window, rather than to a texture and back (story #585).
//!
//! # Why the surface is here and not in the host
//!
//! `demo/src/present.rs` draws the seam between a host and a painter, and it is
//! the host that owns the window. The *surface* is a different thing: it is the
//! swapchain the device presents through, and its format has to agree with the
//! format the render pipeline was built for. Those two live one field apart in
//! [`Renderer`], and putting the surface anywhere else would make that
//! agreement something a caller has to hold rather than something the type
//! does.
//!
//! So the host still owns the window, still owns the seam, and hands the window
//! handle to [`SurfaceRenderer::new`] once. What it gets back presents frames
//! and never returns a pixel — which is the property `demo/src/present.rs`
//! argues the seam has to have, and the reason the trait names no buffer.
//!
//! # This is also the web target's path
//!
//! A browser canvas is a `wgpu::SurfaceTarget` exactly as a window is, so story
//! #587 configures and presents through this same type rather than a second
//! one.

use dashpaint::{ClipTable, GlyphRunTable, ImageTable, PaintTable};

use crate::instance::InstanceBuffer;
use crate::render::{Changes, Renderer, RendererError, TARGET_FORMAT};

/// A renderer bound to a window's swapchain.
pub struct SurfaceRenderer {
    /// Declared before [`SurfaceRenderer::renderer`] so it is dropped first:
    /// the surface was created from the `wgpu::Instance` the renderer holds,
    /// and outliving it is the one ordering that is not allowed.
    surface: wgpu::Surface<'static>,
    renderer: Renderer,
    /// Never holds an extent past [`Renderer::max_extent`]. [`Self::new`] and
    /// [`Self::resize`] are the only two writers and both refuse one, which is
    /// what makes [`Self::configure`] — called from three places, two of them
    /// on the frame path with no way to report — safe to call unconditionally.
    /// See [`RendererError::Extent`] for what configuring past it does.
    config: wgpu::SurfaceConfiguration,
    /// Set when the swapchain reported itself out of date but still handed over
    /// a texture. It cannot be reconfigured while that texture is alive —
    /// `Surface::configure` panics — so the next frame does it before
    /// acquiring.
    stale: bool,
}

/// Whether a frame reached the window, for the recoverable outcomes that are
/// not errors (story #586).
///
/// A timed-out acquire, an occluded window and a zero-area drawable are all
/// ordinary: the frame is skipped and the next one is tried. They were reported
/// as `Ok(())` and were therefore **indistinguishable from a frame that drew**,
/// which is fine for correctness — the generation chain already handles a
/// declined frame, and `render.rs` says why — and wrong for measurement.
///
/// A frame-cost instrument that cannot tell them apart averages in every frame
/// the window happened to be off-screen for, and a declined frame costs
/// almost nothing. That produces a number which quietly depends on whether
/// anyone moved the window, which is exactly the kind of unreproducible
/// measurement story #586 exists to replace: the first sweep taken with this
/// instrument reported a present time two orders of magnitude too low, and only
/// the owner mentioning that they had moved the window explained it.
///
/// **This is a measurement distinction, not a correctness one.** The generation
/// chain already handles a declined frame — `render::Changes` says why a commit
/// that never reached the device must not be treated as the predecessor of the
/// next — and nothing about that behaviour changed when this type was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// The frame was drawn and presented.
    Yes,
    /// The frame was skipped, and nothing reached the window.
    No,
}

impl Drawn {
    pub fn drew(self) -> bool {
        self == Drawn::Yes
    }
}

/// Why a frame did not reach the window.
///
/// Every variant is terminal for this renderer. The recoverable outcomes are
/// handled inside [`SurfaceRenderer::present`] and never reach here, but they
/// do not all end the same way: **a timed-out acquire and an occluded window
/// are reported as [`Drawn::No`]**, while an **out-of-date swapchain is
/// reconfigured and retried** — so it ends as a real [`Drawn::Yes`] if the
/// retry succeeds, and as [`FrameError::Outdated`] below if it does not. The
/// third case is therefore the one recoverable outcome that can still become an
/// error.
#[derive(Debug)]
pub enum FrameError {
    /// The surface was lost. Recovering means creating a new surface from the
    /// window handle, and this renderer does not keep one: the handle is
    /// consumed by `wgpu::Instance::create_surface`, and holding a second
    /// reference to it would make this type generic over the host's window
    /// type for a case no host has yet hit. Reported so the host can rebuild
    /// the presenter, which is the recovery it already has.
    Lost,
    /// The swapchain was still out of date after being reconfigured. One retry
    /// rather than a loop, because a second failure is a state the frame loop
    /// cannot spin its way out of.
    Outdated,
    /// A validation error was raised inside the acquire and caught by an error
    /// scope.
    Validation,
}

impl FrameError {
    /// Whether rebuilding the presenter is the remedy.
    ///
    /// One rule, read by every host, which is why it is here rather than
    /// restated as a `match` in each of them. Three integration crates classify
    /// this failure — `dashscene-web`, `dashscene-desktop` and
    /// `dashscene-android` — and story #834 exists because the first two had
    /// already flattened the distinction to a string and diverged on what a
    /// recoverable failure means.
    ///
    /// [`FrameError::Lost`] is the only one. [`FrameError::Outdated`] is a
    /// swapchain that was already reconfigured and retried once, which is a
    /// state the frame loop cannot spin its way out of; [`FrameError::Validation`]
    /// is a defect rather than a condition. Rebuilding the presenter for either
    /// would be a loop rebuilding a device to meet the same failure.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, FrameError::Lost)
    }
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Lost => write!(
                f,
                "the window surface was lost and this renderer cannot recreate it; rebuild the \
                 presenter"
            ),
            FrameError::Outdated => write!(
                f,
                "the swapchain was still out of date after being reconfigured"
            ),
            FrameError::Validation => {
                write!(f, "acquiring the next frame raised a validation error")
            }
        }
    }
}

impl std::error::Error for FrameError {}

impl SurfaceRenderer {
    /// Binds a device and a pipeline to `target`, sized for a `width` x
    /// `height` **physical**-pixel drawable.
    ///
    /// A zero dimension is not an error — a minimised window reports one, and
    /// the surface is left unconfigured until [`SurfaceRenderer::resize`]
    /// brings a real extent.
    ///
    /// # Errors
    ///
    /// Beyond the device and format failures [`RendererError`] names,
    /// [`RendererError::Extent`] if the drawable is larger than the device can
    /// address on either axis.
    /// Native only, for the reason [`Renderer::new`] gives: a browser's main
    /// thread cannot block on the adapter request without deadlocking against
    /// the event loop that would resolve it. A web host calls
    /// `SurfaceRenderer::for_canvas`, which is where a canvas becomes a
    /// surface target.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        pollster::block_on(Self::new_async(target, width, height))
    }

    /// Binds the painter and a swapchain to a `<canvas>`.
    ///
    /// The web counterpart of [`SurfaceRenderer::new`], and the reason it
    /// exists here rather than in the host: `wgpu` has a blanket
    /// `From<W> for SurfaceTarget` for anything window-shaped, and **no such
    /// conversion for a canvas** — it must be wrapped as
    /// `SurfaceTarget::Canvas` explicitly. Leaving that to the host would make
    /// the host name a `wgpu` type and take a `wgpu` dependency, which is
    /// exactly what `demo/Cargo.toml` records this crate owning the surface in
    /// order to avoid.
    ///
    /// `width` and `height` are **device** pixels, as everywhere else on this
    /// type. A canvas's CSS size is not its drawable size on a display with a
    /// scale factor, and the host is what knows the difference.
    #[cfg(target_arch = "wasm32")]
    pub async fn for_canvas(
        canvas: web_sys::HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        Self::new_async(wgpu::SurfaceTarget::Canvas(canvas), width, height).await
    }

    /// Binds the painter and a swapchain to an `ANativeWindow`.
    ///
    /// The Android counterpart of `SurfaceRenderer::for_canvas`, and here for
    /// the same reason: `wgpu` has a blanket `From<T> for SurfaceTarget` for
    /// anything implementing `HasWindowHandle + HasDisplayHandle`, and an
    /// `ANativeWindow *` is a raw pointer rather than such a type. Wrapping it
    /// here is what keeps `wgpu` out of the host — this crate's own `Cargo.toml`
    /// records that property for the canvas case, and `dashscene-ffi` would
    /// otherwise take a `wgpu` dependency to name one handle type.
    ///
    /// `width` and `height` are **physical** pixels, which is what Android's
    /// `surfaceChanged` reports and what [`SurfaceRenderer::resize`] already
    /// guards against the adapter maximum (issue #714).
    ///
    /// # Safety
    ///
    /// `window` must be a live `ANativeWindow *` — one whose reference from
    /// `ANativeWindow_fromSurface` is still held — and it must stay live until
    /// this renderer is dropped. On Android that is the `surfaceDestroyed`
    /// handshake: the callback must not return until rendering has stopped and
    /// this value has been dropped. Getting it wrong is use-after-free on
    /// rotation, backgrounding and split-screen.
    #[cfg(target_os = "android")]
    pub unsafe fn for_android_ndk(
        window: std::ptr::NonNull<std::ffi::c_void>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        use wgpu::rwh;

        /// The small handle type D3 of
        /// `docs/decisions/host-integration-in-three-layers.md` says each
        /// platform contributes. Nothing in the painter moves for it.
        struct AndroidNdkSurface {
            window: std::ptr::NonNull<std::ffi::c_void>,
        }

        // `SurfaceTarget<'static>` requires the boxed handle to be `Send + Sync`,
        // and `NonNull` is neither by default. Opting in explicitly rather than
        // storing the pointer as a `usize`, which would satisfy the compiler
        // while hiding the same question.
        //
        // SAFETY: this type never dereferences the pointer — it hands it to
        // `wgpu` as a raw handle and nothing else. An `ANativeWindow` is
        // reference-counted and may be referenced from a thread other than the
        // one that obtained it, and `for_android_ndk`'s own contract is that the
        // caller keeps it live for at least as long as the renderer. So moving
        // this across threads adds no hazard the contract does not already
        // cover.
        unsafe impl Send for AndroidNdkSurface {}
        unsafe impl Sync for AndroidNdkSurface {}

        impl rwh::HasWindowHandle for AndroidNdkSurface {
            fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
                let handle = rwh::AndroidNdkWindowHandle::new(self.window);
                // SAFETY: this type is only constructed below, from a pointer
                // the caller of `for_android_ndk` promises stays live for at
                // least as long as the renderer.
                Ok(unsafe {
                    rwh::WindowHandle::borrow_raw(rwh::RawWindowHandle::AndroidNdk(handle))
                })
            }
        }

        impl rwh::HasDisplayHandle for AndroidNdkSurface {
            fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
                Ok(rwh::DisplayHandle::android())
            }
        }

        Self::new(AndroidNdkSurface { window }, width, height)
    }

    /// [`SurfaceRenderer::new`] without the blocking wait — the constructor a
    /// web host reaches through `SurfaceRenderer::for_canvas`, and the one
    /// every target has.
    pub async fn new_async(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(target)
            .map_err(RendererError::NoSurface)?;
        // Compatible with *this* surface, unlike the offscreen path: an adapter
        // that cannot present to the window would build every pipeline and fail
        // at the first acquire.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|_| RendererError::NoAdapter)?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = linear_format(&capabilities.formats)
            .ok_or_else(|| RendererError::NoLinearFormat(capabilities.formats.clone()))?;
        let renderer = Renderer::on_adapter(instance, adapter, format).await?;
        // Before the configuration is built, so the invariant on `config` holds
        // from the first value ever stored in it.
        renderer.check_extent(width, height)?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            desired_maximum_frame_latency: 2,
            // Fifo is the one present mode every backend supports, and it is
            // what the host's own pacing assumes: `demo/src/shell.rs` waits on a
            // 60 Hz deadline while the generation advances and parks when it is
            // steady, so a mode that presents as fast as the GPU allows would be
            // a second pacer disagreeing with the first.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        let mut renderer = Self {
            surface,
            renderer,
            config,
            stale: false,
        };
        renderer.configure();
        Ok(renderer)
    }

    /// The adapter this renderer runs on — what the host prints, and what a
    /// layer-4 measurement is recorded beside.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        self.renderer.adapter_info()
    }

    /// The swapchain format this window was configured with.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Tells the renderer that the commits from here on come from a different
    /// chain — see [`Renderer::forget_uploaded`], which is the whole of it.
    /// What the frame most recently presented could not make resident.
    ///
    /// Forwarded from [`Renderer::refusals`], because **every production host
    /// holds a `SurfaceRenderer` and none holds a `Renderer`** —
    /// `dashscene-desktop`, `dashscene-web` and `dashscene-ffi` all sit on this
    /// type. Without this the refusal channel issues #718 and #720 added would be
    /// reachable only from the offscreen renderer, which is to say only from
    /// tests, and P4's "never a silent drop" would hold in the test binary and
    /// nowhere a user runs.
    pub fn refusals(&self) -> &[crate::Refusal] {
        self.renderer.refusals()
    }

    /// How many refusals this renderer has recorded since it was built.
    ///
    /// Monotonic, so a host that samples rather than walking
    /// [`refusals`](Self::refusals) every frame still learns that something was
    /// refused.
    pub fn refusals_seen(&self) -> u64 {
        self.renderer.refusals_seen()
    }

    pub fn document_replaced(&mut self) {
        self.renderer.forget_uploaded();
    }

    /// Reconfigures for a drawable of `width` x `height` physical pixels.
    ///
    /// A zero dimension leaves the surface unconfigured: `Surface::configure`
    /// panics on one, and a minimised window has nothing to present to anyway.
    ///
    /// # Errors
    ///
    /// [`RendererError::Extent`] if the drawable is larger than the device can
    /// address on either axis. The configuration is left as it was, so a
    /// caller that reports the error and stops is presenting the extent it was
    /// presenting before, rather than an extent nothing configured.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        if (width, height) == (self.config.width, self.config.height) {
            return Ok(());
        }
        self.renderer.check_extent(width, height)?;
        self.config.width = width;
        self.config.height = height;
        self.configure();
        Ok(())
    }

    /// Draws `buffer` and puts it on the window.
    ///
    /// Returns [`Drawn::No`] without drawing when there is nothing to draw
    /// into: a zero extent, an occluded window, or an acquire that timed out.
    /// None of the three is a failure, and all three are states a frame loop
    /// passes through in normal use.
    ///
    /// An **out-of-date** swapchain is not among them. It is reconfigured and
    /// retried inside `SurfaceRenderer::acquire`, so it ends either as an
    /// ordinary [`Drawn::Yes`] or, if the retry still reports it, as
    /// [`FrameError::Outdated`].
    ///
    /// A declined frame needs no other bookkeeping, and that is by construction
    /// rather than by care: the renderer's record of what the device holds is
    /// advanced by drawing, so a frame that does not draw leaves it where it
    /// was, and the next frame is no longer the successor of it. The whole
    /// buffer is written then. See [`Changes`] for the defect that shaped this
    /// — a converged animation, one declined frame, and a rect that stayed
    /// 0.02 units narrow for the rest of the run.
    pub fn present(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        changes: Option<Changes<'_>>,
    ) -> Result<Drawn, FrameError> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(Drawn::No);
        }
        if self.stale {
            // Deferred from the frame that reported it, which was holding the
            // texture that made reconfiguring illegal.
            self.stale = false;
            self.configure();
        }
        let Some(frame) = self.acquire()? else {
            return Ok(Drawn::No);
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.draw(
            &view,
            buffer,
            paints,
            images,
            clips,
            glyphs,
            changes,
            self.config.width,
            self.config.height,
        );
        self.renderer.queue().present(frame);
        Ok(Drawn::Yes)
    }

    /// Applies [`SurfaceRenderer::config`] to the surface, unless the drawable
    /// has no area.
    ///
    /// Infallible because of the invariant on [`SurfaceRenderer::config`], and
    /// it has to be: two of the three callers are on the frame path, where an
    /// out-of-date swapchain is reconfigured and retried with nowhere to report
    /// a refusal to. The `debug_assert` is what holds the invariant against a
    /// future third writer — an over-large extent reaching the line below does
    /// not return an error, it aborts the process (issue #714).
    fn configure(&mut self) {
        if self.config.width == 0 || self.config.height == 0 {
            return;
        }
        debug_assert!(
            self.renderer
                .check_extent(self.config.width, self.config.height)
                .is_ok(),
            "the surface configuration outgrew what the device can address"
        );
        self.surface.configure(self.renderer.device(), &self.config);
        // After the configure, never before: `wgpu` writes the layer's colour
        // space as part of it, so applying this first would be overwritten by
        // the line above. This is also why it lives here rather than in
        // `new` — every reconfigure resets it, and a resize is a reconfigure.
        #[cfg(target_os = "macos")]
        self.match_layer_to_srgb();
    }

    /// Tells the window server that this swapchain's contents are sRGB, so it
    /// colour matches them to whatever display the window is on (issue #746).
    ///
    /// # Why this is needed at all
    ///
    /// `CAMetalLayer` does not colour match when its `colorspace` is nil: the
    /// contents are consumed in the display's own space. `wgpu` leaves it nil —
    /// `SurfaceColorSpace::Srgb` maps to `setColorspace(None)` in
    /// `wgpu-hal`'s Metal backend, and `Auto` resolves to `Srgb` for every
    /// format that is not `Rgba16Float`, so this painter never had a choice to
    /// make. On a wide-gamut display the effect is that every sRGB value this
    /// painter writes is shown as though it were already Display-P3, and
    /// everything saturated is oversaturated.
    ///
    /// Measured against the reference painter, whose `softbuffer` blit *is*
    /// colour managed: for four flat swatches the Skia window landed on the
    /// sRGB-to-display conversion of the painter's own bytes and this one
    /// landed on the raw bytes, byte-exact in both directions. The two painters
    /// draw identical pixels offscreen, so the whole difference was here.
    ///
    /// The shift tracks saturation, which is why it reads as a wrong colour
    /// rather than a wrong brightness: a mid-green's red channel moved 29 code
    /// points while a near-black moved 1. On a display profiled to sRGB the
    /// conversion is near identity and this function changes nothing, which is
    /// why the defect is invisible on some machines and obvious on others.
    ///
    /// # Why not the alternatives
    ///
    /// `SurfaceColorSpace::ExtendedSrgb` would be a colour-managed layer with
    /// sRGB primaries and would need no `unsafe` at all, but `wgpu-hal`
    /// advertises the extended colour spaces only for `Rgba16Float`; a
    /// `Bgra8Unorm` surface is offered `SRGB` and `DISPLAY_P3` and nothing
    /// else. Taking `DISPLAY_P3` instead would mean converting sRGB to P3 in
    /// the fragment stage, which hardcodes one display's gamut into the painter
    /// and is wrong the moment a second display with a different profile is
    /// attached — ColorSync already does that correctly for every attached
    /// display, once it is told what the pixels are, which is all this does.
    ///
    /// gfx-rs/wgpu#10013 is the upstream report. If it lands, this goes.
    #[cfg(target_os = "macos")]
    fn match_layer_to_srgb(&self) {
        use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};

        // SAFETY: `as_hal` is unsafe because the handle it yields could be used
        // to break invariants `wgpu` maintains. Setting a Core Animation
        // property creates and destroys no resource, touches no state `wgpu`
        // tracks, and takes the same lock `wgpu` takes to reach the layer.
        let Some(surface) = (unsafe { self.surface.as_hal::<wgpu::hal::api::Metal>() }) else {
            // Not a Metal surface. Unreachable while macOS has one backend, and
            // returning is still right: there is no layer to tell.
            return;
        };
        // SAFETY: reading an immutable constant Core Graphics exports.
        let name = unsafe { kCGColorSpaceSRGB };
        let space = CGColorSpace::with_name(Some(name));
        debug_assert!(
            space.is_some(),
            "Core Graphics knows the colour space its own kCGColorSpaceSRGB names"
        );
        // Only on `Some`, deliberately. `setColorspace(None)` is exactly the
        // unmanaged state this exists to leave, so a failed lookup must not be
        // written through as one — better to change nothing than to re-apply
        // the defect.
        if let Some(space) = space {
            surface.render_layer().lock().setColorspace(Some(&space));
        }
    }

    /// Checks, on the frame path and in debug builds only, that the layer is
    /// still colour matched.
    ///
    /// # Why a self-check rather than a test
    ///
    /// [`Self::match_layer_to_srgb`] holds only while **every** reconfigure is
    /// followed by it, because `wgpu` rewrites the layer's colour space inside
    /// `Surface::configure` and writes nil there. Today that holds structurally:
    /// `self.surface.configure` appears once in this crate, inside
    /// [`Self::configure`], with the call on the next line. Nothing pins it —
    /// a second configure path, or a `wgpu` upgrade that reconfigures somewhere
    /// this type does not see, silently returns the swapchain to the unmanaged
    /// state and the only symptom is that colours look wrong to somebody.
    ///
    /// It cannot be a test. It needs a window server, a real surface and a
    /// display, which is the same reason story #586 is measured by hand and why
    /// issue #746's acceptance is a screen capture rather than an assertion.
    /// So the check runs where the evidence exists — on a frame, in the process
    /// that has the layer — and costs nothing in release, where it is compiled
    /// out entirely.
    ///
    /// This is the shape that caught the dirty-set break the goldens could not
    /// see: a renderer that checks its own invariant on the frame path found in
    /// two minutes of running what no fixture had expressed.
    #[cfg(all(debug_assertions, target_os = "macos"))]
    fn assert_layer_is_colour_matched(&self) {
        // SAFETY: as `match_layer_to_srgb` — reading a Core Animation property
        // creates and destroys nothing and touches no state `wgpu` tracks.
        let Some(surface) = (unsafe { self.surface.as_hal::<wgpu::hal::api::Metal>() }) else {
            return;
        };
        debug_assert!(
            surface.render_layer().lock().colorspace().is_some(),
            "the swapchain layer has no colour space, so macOS is not colour matching it and \
             every saturated colour is drawn wrong on a wide-gamut display (issue #746). Some \
             path reconfigured the surface without applying SurfaceRenderer::match_layer_to_srgb"
        );
    }

    /// The next swapchain texture, or `None` when this frame should be skipped.
    ///
    /// One reconfigure-and-retry on an out-of-date swapchain, which is the
    /// recovery wgpu documents for it. It happens here rather than in the host
    /// because the host has nothing to act on: the condition is the surface's,
    /// and the surface is this type's.
    fn acquire(&mut self) -> Result<Option<wgpu::SurfaceTexture>, FrameError> {
        #[cfg(all(debug_assertions, target_os = "macos"))]
        self.assert_layer_is_colour_matched();
        let mut outdated = false;
        loop {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) => return Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    // Presentable, but the swapchain no longer matches the
                    // surface. Show it and reconfigure before the next one.
                    self.stale = true;
                    return Ok(Some(frame));
                }
                // Not this frame, and not an error either: the host will run
                // another when the window is visible again.
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    if outdated {
                        return Err(FrameError::Outdated);
                    }
                    outdated = true;
                    self.configure();
                }
                wgpu::CurrentSurfaceTexture::Lost => return Err(FrameError::Lost),
                wgpu::CurrentSurfaceTexture::Validation => return Err(FrameError::Validation),
            }
        }
    }
}

/// The first format in `offered` this painter may blend in.
///
/// [`TARGET_FORMAT`] first, so that a window and a golden agree byte for byte
/// wherever the surface offers it; then any format the hardware does not
/// sRGB-convert on write. `None` when every offered format converts, which
/// [`RendererError::NoLinearFormat`] refuses rather than works around —
/// `docs/decisions/pipelines-and-layer-3.md` D3 makes the blending space a term
/// of the contract, not a preference.
///
/// Channel order is deliberately not part of the choice. A fragment shader
/// writes to location 0 and the hardware maps its components onto the target's
/// channels, so `Bgra8Unorm` shows the same picture as `Rgba8Unorm` without the
/// shader knowing which it got.
fn linear_format(offered: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    offered
        .iter()
        .copied()
        .find(|format| *format == TARGET_FORMAT)
        .or_else(|| offered.iter().copied().find(|format| !format.is_srgb()))
}

#[cfg(test)]
mod tests {
    use super::linear_format;
    use crate::render::TARGET_FORMAT;

    /// The offscreen format is taken when it is on offer, whatever the surface
    /// would have preferred, so the window and the goldens hold the same bytes.
    #[test]
    fn the_offscreen_format_wins_when_the_surface_offers_it() {
        let offered = [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            TARGET_FORMAT,
        ];
        assert_eq!(linear_format(&offered), Some(TARGET_FORMAT));
    }

    /// The common macOS and Windows case: the surface offers BGRA and no RGBA
    /// at all. The picture is the same, so this is taken rather than refused.
    #[test]
    fn a_bgra_surface_is_taken_rather_than_refused() {
        let offered = [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
        ];
        assert_eq!(
            linear_format(&offered),
            Some(wgpu::TextureFormat::Bgra8Unorm)
        );
    }

    /// The one that matters: a surface offering only sRGB-converting formats
    /// yields nothing, so construction fails by name instead of quietly
    /// blending in linear light and moving every blended pixel by tens of code
    /// points.
    #[test]
    fn an_all_srgb_surface_yields_no_format() {
        let offered = [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ];
        assert_eq!(linear_format(&offered), None);
    }

    /// A surface that offers nothing is incompatible with the adapter, which
    /// `wgpu` reports as an empty list rather than an error.
    #[test]
    fn an_empty_offer_yields_no_format() {
        assert_eq!(linear_format(&[]), None);
    }
}
