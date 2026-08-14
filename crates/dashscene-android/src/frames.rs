//! What the frame loop drives, so that one loop serves every scene source
//! (story #842).
//!
//! # Why this seam exists
//!
//! Story #841 built the loop against the C ABI: create a runtime, load a `.dsb`
//! from bytes, attach, tick, draw. That is layer 0 and it is what a product host
//! embedding a compiled document needs.
//!
//! It cannot run the showcase. Those scenes are **built in code** — `SceneBuilder`
//! is `fn(&mut Arena, u32, u32) -> LiveScene`, and the scene brings its own
//! solver, which is the only reason its text has a typesetter at all. A
//! document loaded through `ds_runtime_load_document` has no such solver and
//! draws no text. Story #863 fixed the other two hosts with a `TextResources`
//! parameter, and story #947 fixed this one: neither a `Typesetter` nor an
//! `Atlas` crosses a C boundary, so `ds_runtime_load_document_with_text` takes
//! their **inputs** — font bytes and a committed sheet per face — and
//! `nativeSurfaceCreatedWithText` carries them from Java. That is a second
//! load, not a second scene source. The C ABI still has no builder entry point
//! and deliberately will not grow one here: that is layer 2, D8 of
//! `docs/decisions/host-integration-in-three-layers.md`, and inventing the
//! vocabulary now would pre-empt the story that settles it.
//!
//! So there are two scene sources and they cannot share a runtime. What they
//! **can** share is everything else — the render thread, the looper, the
//! `AChoreographer` callback, the frame delta, the resize check and the destroy
//! handshake — and this trait is the line between them. A host implements four
//! methods; the loop is written once.
//!
//! That is the point rather than tidiness. Story #834 exists because two
//! integration crates diverged on what a recoverable failure means, and the
//! fix put the rule in one place before a third host could inherit a third
//! answer. A second Android frame loop, written beside the first because the
//! first only knew about `.dsb` bytes, is the same mistake with the same shape.
//!
//! # What an implementation owns
//!
//! Everything the loop does not: the arena, the scene, the painter and the
//! surface. The loop owns the clock and the thread, which is P3 — the runtime
//! owns time and nothing producer-side runs inside the loop that the host did
//! not put there.

use std::ffi::c_void;

/// Why a surface could not be taken up.
///
/// A `String` rather than an enum: an implementation lives outside this crate
/// and its failures are its own — `demo-android`'s are `dashscene-gpu`'s, the
/// document path's are the C ABI's status plus its error channel. The loop does
/// not branch on this; it logs it and stops.
pub type AttachError = String;

/// One frame's outcome, as the loop reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Keep scheduling.
    Continue,
    /// The surface was lost, and rebuilding it is the remedy.
    ///
    /// The loop calls [`Frames::detach`] and then [`Frames::attach`] again with
    /// the same window, and carries on. The scene is not rebuilt and the clock
    /// is not reset — only the device is new.
    ///
    /// **This variant exists because the seam is what stops a host inventing a
    /// third answer.** `dashscene_gpu::FrameError::is_recoverable` is one rule
    /// read by every host, and `dashscene-web` and `dashscene-desktop` both act
    /// on it; a host that could only say `Stop` would have no way to honour it,
    /// which is the divergence story #834 was filed to prevent.
    Rebuild,
    /// Stop. The loop tears down and releases the handshake.
    Stop,
}

/// What the Android frame loop drives.
///
/// Every method runs on the **render thread**, and never concurrently.
///
/// The order is `attach`, then `resize` and `frame` any number of times, then
/// `detach`. **`attach` and `detach` may each run more than once**, because
/// [`Step::Rebuild`] is `detach` followed by a fresh `attach` on the same
/// value — so an implementation must be reusable rather than single-shot, and
/// must not consume in `attach` anything the next `attach` will need. That
/// mistake is not hypothetical: the document implementation took its `.dsb`
/// bytes on the first attach, and every rebuild then failed for want of them.
///
/// `detach` is always the last call, and after the final one nothing runs —
/// which is what makes it the point where a surface may be dropped.
///
/// **Not `Send`, deliberately.** An implementation owns an `Arena` and a
/// `LiveScene`, and those hold a boxed `LayoutSolver` and boxed closures that
/// are not `Send` — so a value of this type cannot cross a thread boundary at
/// all. It does not need to: `loop_::start` takes a **factory** that is
/// `Send` and calls it on the render thread, so everything here is born on the
/// thread that uses it and never moves. That is the stronger arrangement in any
/// case, because it is also what keeps `wgpu`'s device and queue on one thread.
pub trait Frames: 'static {
    /// Takes up a platform surface, and builds whatever draws into it.
    ///
    /// `window` is an `ANativeWindow *`; `width` and `height` are **physical**
    /// pixels, which is what `surfaceChanged` reports. The pointer is live for
    /// as long as this object is — the destroy handshake is what makes that
    /// true — so an implementation may hand it to
    /// `SurfaceRenderer::for_android_ndk` and keep the result.
    ///
    /// # Safety
    ///
    /// `window` must be a live `ANativeWindow *`. The loop upholds that; an
    /// implementation may rely on it.
    unsafe fn attach(
        &mut self,
        window: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), AttachError>;

    /// The drawable changed size, in physical pixels.
    ///
    /// Whether the scene is rebuilt for the new extent or the picture is merely
    /// reconfigured is the implementation's call: a document carries its own
    /// resolved size, a scene built in code derives every offset from the extent
    /// it was given.
    ///
    /// **Returns whether the new extent was taken up.** `false` means the loop
    /// keeps the old one as configured and offers the same extent again on the
    /// next frame, rather than believing a resize that did not happen — an
    /// over-large drawable is refused by `check_extent` against the adapter
    /// maximum (issue #714), and a loop that recorded it anyway would leave the
    /// scene laid out for the old size for the rest of the surface's life.
    fn resize(&mut self, width: u32, height: u32) -> bool;

    /// Advances by `dt` seconds and draws if anything is worth drawing.
    ///
    /// `dt` is raw. `LiveScene::tick` applies both the ceiling and the floor, so
    /// the rule has one statement rather than one per host (story #810), and an
    /// implementation should pass it through rather than clamping again.
    ///
    /// `forced` is the loop telling the implementation that the generation
    /// cannot report this frame — the first after an attach, and the first after
    /// a resize. Both are cases where the device has drawn nothing and the scene
    /// has not changed, so a host that only drew on `advanced()` would show an
    /// empty window until something else moved.
    fn frame(&mut self, dt: f32, forced: bool) -> Step;

    /// Drops the surface, **and everything else this holds**.
    ///
    /// **This is the call the destroy handshake waits on.** When it returns, the
    /// `wgpu::Surface` built from the window must be gone; the loop then
    /// releases the UI thread, which then releases the window. Returning while
    /// anything still holds that window is the use-after-free D4 exists to
    /// prevent.
    ///
    /// **It must also release whatever else the implementation owns** — the
    /// arena, the scene, any packing buffers. The loop's state is deliberately
    /// leaked, because a vsync callback that cannot be cancelled may still hold
    /// a pointer to it, and the implementation is inside that state. So
    /// anything not released here is retained for the life of the process, once
    /// per surface cycle — and on Android a surface cycle is every rotation.
    ///
    /// It may be called after [`Frames::attach`] failed, so it must tolerate
    /// having nothing to release.
    fn detach(&mut self);
}
