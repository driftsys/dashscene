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

use dashpaint::{ClipTable, ImageTable, PaintTable};

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

/// Why a frame did not reach the window.
///
/// Every variant is terminal for this renderer. The recoverable outcomes —
/// a timed-out acquire, an occluded window, an out-of-date swapchain — are
/// handled inside [`SurfaceRenderer::present`] and are not reported at all:
/// the first two mean the frame is skipped, and the third is reconfigured and
/// retried.
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
    pub fn new(
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
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|_| RendererError::NoAdapter)?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = linear_format(&capabilities.formats)
            .ok_or_else(|| RendererError::NoLinearFormat(capabilities.formats.clone()))?;
        let renderer = Renderer::on_adapter(instance, adapter, format)?;
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
    /// Returns `Ok(())` without drawing when there is nothing to draw into: a
    /// zero extent, an occluded window, or an acquire that timed out. None of
    /// the three is a failure, and all three are states a frame loop passes
    /// through in normal use.
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
        changes: Option<Changes<'_>>,
    ) -> Result<(), FrameError> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(());
        }
        if self.stale {
            // Deferred from the frame that reported it, which was holding the
            // texture that made reconfiguring illegal.
            self.stale = false;
            self.configure();
        }
        let Some(frame) = self.acquire()? else {
            return Ok(());
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
            changes,
            self.config.width,
            self.config.height,
        );
        self.renderer.queue().present(frame);
        Ok(())
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
    }

    /// The next swapchain texture, or `None` when this frame should be skipped.
    ///
    /// One reconfigure-and-retry on an out-of-date swapchain, which is the
    /// recovery wgpu documents for it. It happens here rather than in the host
    /// because the host has nothing to act on: the condition is the surface's,
    /// and the surface is this type's.
    fn acquire(&mut self) -> Result<Option<wgpu::SurfaceTexture>, FrameError> {
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
