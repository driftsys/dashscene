//! Android integration for dashscene: what an Android embedder must have and
//! would otherwise write for itself (story #841).
//!
//! The third integration crate, beside `dashscene-web` and `dashscene-desktop`,
//! and a crate rather than a `cfg` arm inside one of those for the reason the
//! v0.17 close established: it looked for the common part between the two,
//! found it, and it was one constant and two methods on `LiveScene`. There is no
//! host abstraction to extend.
//!
//! It differs from the other two in one structural way. They drive
//! `dashscene-gpu` and `dashlang` directly; this one sits on the **C ABI**
//! (`dashscene-ffi`), because D2 of
//! `docs/decisions/host-integration-in-three-layers.md` says every platform host
//! does, and because a Kotlin host reaches native code through JNI in any case.
//! Driving the ABI as a C caller would is also what proves it sufficient for
//! layer 0 rather than merely plausible.
//!
//! # The four pieces
//!
//! 1. **The surface handoff** (D3) — `AndroidExternalSurface` or a
//!    `SurfaceHolder.Callback` hands over an `android.view.Surface`;
//!    `ANativeWindow_fromSurface` turns it into an `ANativeWindow *`; that
//!    reaches `SurfaceRenderer::for_android_ndk` through
//!    `ds_runtime_attach_surface`. Nothing in the painter moves for it.
//! 2. **The frame loop** (D6) — `AChoreographer`, driven from the native side on
//!    its own thread. P3 says producers mutate and the runtime owns time; a host
//!    that called a tick from its UI thread would invert that, and on Android it
//!    would also put the frame loop on the thread that has to run the destroy
//!    handshake.
//! 3. **The destroy handshake** (D4) — [`Handshake`], and the part of this crate
//!    most worth reading. `surfaceDestroyed` blocks until the loop has stopped
//!    and the surface has been dropped.
//! 4. **Resize** — `surfaceChanged` reports **physical** pixels, which is what
//!    `ds_runtime_resize` takes and what `check_extent` already guards against
//!    the adapter maximum (issue #714).
//!
//! # What an embedder still writes
//!
//! Named here rather than left as whatever did not fit, which is what epic #793
//! asked of the other two.
//!
//! - **The view, and its lifecycle callbacks.** This crate exports JNI entry
//!   points; binding them to an `AndroidExternalSurface`, a `SurfaceView` or a
//!   `SurfaceHolder.Callback` is the embedder's, and so is the Kotlin class that
//!   declares them. `demo-android` shows one arrangement; it is not the only
//!   one.
//! - **Where the document comes from.** [`ds_runtime_load_document`] takes
//!   bytes. An asset, a download, a file — and the reading of it — is the
//!   embedder's. This crate has no opinion and no scene registry.
//! - **`System.loadLibrary`, and the ABI check.** A host should call
//!   `ds_abi_version` once and refuse a library it does not recognise, because
//!   the alternative is discovering the mismatch as a corrupted argument.
//! - **What happens on failure.** Every entry point here returns a status; where
//!   that goes — a log line, a crash, a fallback view — is the embedder's.
//! - **Input, and anything above layer 0.** Signals (layer 1) and scenes
//!   authored in Kotlin (layer 2) are deferred with their layers. A host that
//!   wants a touch to change the picture is asking for layer 1.
//!
//! # `SurfaceView` semantics only
//!
//! D5. A `SurfaceView` is its own layer, composited by SurfaceFlinger and able
//! to land on a hardware overlay: no extra copy. `TextureView` — through
//! `AndroidEmbeddedExternalSurface` or a plain `TextureView` — is v1, with the
//! case that motivates it: a scene the composition has to transform, clip or
//! z-order. Deferring costs nothing structurally, because both arrive at the
//! same `android.view.Surface` and therefore at the same handle type.
//!
//! # What is not established yet
//!
//! **Whether the target device class exposes Vulkan** — D3a, story #839. The
//! painter binds four fragment-stage storage buffers and
//! `wgpu::Limits::downlevel_defaults` allows exactly four, so a device without
//! Vulkan meets the same wall that makes WebGL2 unbuildable for this painter.
//! Until that measurement exists on hardware, **nothing here describes Android
//! as working**: this crate compiles, its handshake is tested, and what it does
//! on a device is a question the device has to answer.
//!
//! [`ds_runtime_load_document`]: dashscene_ffi::ds_runtime_load_document

mod handshake;

pub use handshake::Handshake;

// The whole of the platform half. Behind the `cfg` because every symbol in it
// binds an NDK or JNI function that exists on no other target — which is also
// why the handshake above is *not* in here.
#[cfg(target_os = "android")]
mod host;
