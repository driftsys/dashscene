//! The showcase as a [`Frames`] implementation, and the JNI entry points over
//! it.
//!
//! Compiled on Android and nowhere else. The render thread, the looper, the
//! vsync callback and the destroy handshake are `dashscene-android`'s and are
//! not restated here; what this module owns is what a scene built in code needs
//! and a `.dsb` does not — the arena, the `LiveScene`, the painter and the
//! surface.

use std::ffi::c_void;
use std::time::Instant;

use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_android::{AttachError, Frames, Step, log};
use dashscene_core::Arena;
use dashscene_gpu::{Changes, GpuPainter, SurfaceRenderer};
use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong};

use crate::refusal::Refusal;
use crate::select;
use crate::timing::Timing;

/// How long each scripted phase lasts, in seconds.
///
/// The showcase's pulse advances by phase index rather than by time, so the host
/// decides the rate. **2.5 s, which is `demo/src/shell.rs`'s `PULSE_INTERVAL`**
/// — the two are the same demonstration and should step at the same rate. This
/// said 1.0 with a comment claiming it matched, which review caught: the
/// Android host was running the script two and a half times faster.
const PHASE_SECONDS: f32 = 2.5;

/// One showcase scene, drawn through the lean painter.
struct ShowcaseFrames {
    scene: &'static showcase::Showcase,
    arena: Arena,
    live: Option<LiveScene>,
    painter: GpuPainter,
    renderer: Option<SurfaceRenderer>,
    /// Seconds since the first frame, which the scripted pulse steps on.
    elapsed: f32,
    /// The phase the pulse was last driven to, so it is written once per phase
    /// rather than every frame. Writing the same signal every frame marks its
    /// binding dirty, so `tick` never takes its idle early return and the loop
    /// never parks — which is the trap `dashscene-web`'s `FrameHook`
    /// documentation names.
    phase: u64,
    timing: Timing,
    /// Why the last [`Frames::resize`] answered `false`, for
    /// [`Frames::refusal_reason`].
    ///
    /// **Recorded rather than logged** (issue #1194, the same defect issue
    /// #1157 named in `dashscene-android`). The loop offers a refused extent
    /// again every frame on purpose, so a logcat line written where the refusal
    /// is detected is one line per vsync for as long as that extent is offered
    /// — and logcat's buffer is a ring, so that rate overwrites the attach
    /// markers a device run is read through. `LoopState::step` asks
    /// [`Frames::refusal_reason`] from the branch that reports a refusal, which
    /// `record_refusal` bounds to once per refused extent rather than once per
    /// frame.
    ///
    /// [`Refusal`] rather than a bare `String`, so the same bound holds for the
    /// message itself and so the ordering this depends on is testable off
    /// Android — that module's own documentation gives both reasons.
    refusal: Refusal,
}

impl ShowcaseFrames {
    /// Builds the scene for this extent, into a fresh arena.
    ///
    /// A scene built in code derives every offset from the drawable it is given,
    /// so a new extent means a new scene — the same answer `demo` and
    /// `demo-web` give. The scene brings its own solver, which is why its text
    /// has a typesetter at all.
    fn build(&mut self, width: u32, height: u32) {
        self.arena = Arena::new();
        self.live = Some((self.scene.build)(&mut self.arena, width, height));
        // The incoming arena's generations start again, and nothing in the
        // frames themselves says so.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.document_replaced();
        }
        // The sample in hand measured a different extent through a different
        // swapchain. `Timing` discards a part-sample when the scene *name*
        // changes, and a rebuild is the larger discontinuity — same name, whole
        // new scene — so it has to be said here.
        self.timing = Timing::new();
        // The new scene holds nothing the pulse wrote into the old one, so the
        // phase is re-applied rather than resumed.
        self.phase = u64::MAX;
    }
}

impl Frames for ShowcaseFrames {
    unsafe fn attach(
        &mut self,
        window: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), AttachError> {
        let Some(window) = std::ptr::NonNull::new(window) else {
            return Err("the window is null".to_owned());
        };
        // SAFETY: the loop promises the window outlives this object, which is
        // exactly what `for_android_ndk` asks.
        let renderer = unsafe { SurfaceRenderer::for_android_ndk(window, width, height) }
            .map_err(|error| format!("{error:?}"))?;
        let info = renderer.adapter_info();
        log(&format!(
            "dashscene-gpu ({}, {:?}, {:?}) — {width}x{height}, scene {}",
            info.name,
            info.backend,
            renderer.format(),
            self.scene.name,
        ));
        self.renderer = Some(renderer);
        // Not `build`: that reports `document_replaced`, and this renderer was
        // constructed three lines ago with nothing uploaded. `dashscene-web`
        // names that as the second mechanism not to add — the constructor
        // already establishes the state it would clear.
        self.arena = Arena::new();
        self.live = Some((self.scene.build)(&mut self.arena, width, height));
        self.phase = u64::MAX;
        Ok(())
    }

    /// **Answers what `resize` recorded, and reads nothing live**, so the loop
    /// may ask whenever it likes — which is what [`Frames::refusal_reason`]
    /// asks of an implementation.
    fn refusal_reason(&self) -> Option<String> {
        self.refusal.reason()
    }

    fn resize(&mut self, width: u32, height: u32) -> bool {
        // Matched rather than chained: `if let Some(..) && let Err(..)` takes
        // its branch only when a renderer exists *and* the resize failed, so
        // the no-renderer case fell through to `build` and reported success —
        // conflating "there is nothing to resize" with "the resize worked".
        match self.renderer.as_mut() {
            Some(renderer) => {
                if let Err(error) = renderer.resize(width, height) {
                    // Recorded rather than logged (issue #1194); see `refusal`.
                    //
                    // `Display` rather than `Debug`: `RendererError` writes one
                    // by hand, and for the one error this call can raise it
                    // gives the extent, the device maximum and which axis
                    // exceeded it, where `Debug` gives the three numbers as a
                    // struct literal. `frame` below already logs `{error}` for
                    // the same type. It costs the same and logcat is the only
                    // channel a device run has.
                    self.refusal.refused(width, height, || format!("{error}"));
                    return false;
                }
            }
            None => {
                // **Defensive, and not reachable through the loop.**
                // `LoopState::step` runs only after `acquire` returned from a
                // successful `attach`, and a rebuild detaches and re-attaches
                // back to back, stopping the loop if that fails — so no vsync
                // reaches here with no renderer. It is recorded rather than
                // left silent because the arm answers `false` and the loop's
                // report would otherwise name "no reason given", which is what
                // an embedder driving `Frames` itself would see.
                self.refusal
                    .refused(width, height, || "no renderer is attached".to_owned());
                return false;
            }
        }
        // Cleared on the way out, so a refusal that is later taken up does not
        // leave its reason to be reported against the next one.
        self.refusal.clear();
        self.build(width, height);
        true
    }

    fn frame(&mut self, dt: f32, forced: bool) -> Step {
        let Some(live) = self.live.as_mut() else {
            return Step::Stop;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return Step::Stop;
        };

        // The scripted pulse, on the host's clock. This is the demonstration's
        // whole per-frame job, and it is what makes the scene move at all: with
        // nothing writing a signal, `tick` takes its idle early return after the
        // first commit and the picture parks.
        self.elapsed += dt;
        let phase = (self.elapsed / PHASE_SECONDS) as u64;
        if phase != self.phase {
            self.phase = phase;
            (self.scene.pulse)(live, phase);
        }

        let before_tick = Instant::now();
        live.tick(dt, &mut self.arena);
        let tick_took = before_tick.elapsed();

        // The renumbering gate, read here for the same reason the three
        // published hosts read it (issue #945): this is a **fourth** loop that
        // drives a `LiveScene` against a `SurfaceRenderer` it owns, and it
        // reaches neither `dashscene-desktop` nor `dashscene-ffi` on this path
        // — `DocumentFrames` goes through the ABI, this does not.
        //
        // Nothing can raise a renumbering here today: a showcase scene is built
        // in code and never names a shown root, so `renumbered` is always
        // false. It is read anyway so that "every loop that ticks a `LiveScene`
        // reports its renumbering" is true of the workspace rather than true of
        // the crates that happen to be published, which is what the records
        // claim.
        //
        // Before `advanced()`, because a renumbering with no other change would
        // otherwise take the early return below and never be reported.
        //
        // No renderer check, unlike the three published hosts: this function
        // has already returned above if there is none, so the report cannot be
        // stamped against a renderer that is not there.
        if live.take_renumbering(&self.arena) {
            renderer.document_replaced();
        }

        if !live.advanced() && !forced {
            return Step::Continue;
        }

        // **Two timers, not one** (2026-08-17). `paint` is this project's own
        // instance packing and is pure CPU; `present` is the upload, the command
        // encoding, the submit and the swapchain. A single figure around both
        // cannot say which of them a frame's cost is, and they have nothing in
        // common as optimisation targets — the device measurement that motivated
        // the split is in `docs/design/android-toolchain.md`.
        let before_paint = Instant::now();
        let committed = self.arena.committed();
        let changes = Changes {
            rects: committed.dirty(),
            generation: committed.generation(),
        };
        self.painter.paint(
            committed.rects(),
            committed.paints(),
            committed.images(),
            committed.clips(),
            committed.groups(),
            committed.glyphs(),
            Some(changes.rects),
        );
        let paint_took = before_paint.elapsed();

        let before_present = Instant::now();
        let presented = renderer.present(
            self.painter.instances(),
            committed.paints(),
            committed.images(),
            committed.clips(),
            committed.glyphs(),
            Some(changes),
        );
        let present_took = before_present.elapsed();

        // What **this frame's** text amounts to, so a cost per glyph is
        // arithmetic rather than an inference from which scene was running.
        // Counted here because `committed` is borrowed for the paint anyway.
        //
        // **A snapshot of the reporting frame, not a property of the sample.**
        // Only the closing frame's values reach the line, and a showcase scene's
        // text is not static — `typography` binds a formatted speed and the badge
        // binds a label — so the count can move within one sample. It moved:
        // consecutive samples of `typography` reported 444 and 446. Read it as
        // the order of magnitude the sample was drawing, never as a denominator
        // exact for every frame in it.
        let runs = committed.glyphs().runs().len();
        let quads = committed.glyphs().all_quads().len();

        match presented {
            Ok(_) => {
                // Marked shown whenever presenting returns, not only when a
                // frame reached the window — which is what `LiveScene::advanced`
                // requires, and gating on it would leave `advanced()` true on
                // every tick while the window is occluded.
                live.mark_shown();
            }
            Err(error) => {
                log(&format!("present: {error}"));
                // The one rule, read rather than restated:
                // `FrameError::is_recoverable` is what `dashscene-web` and
                // `dashscene-desktop` both branch on, and a third host
                // answering differently is the divergence story #834 exists to
                // prevent. This collapsed every failure to `Stop` until review
                // caught it.
                return if error.is_recoverable() {
                    Step::Rebuild
                } else {
                    Step::Stop
                };
            }
        }

        if let Some(sample) = self
            .timing
            .push(self.scene.name, tick_took, paint_took, present_took)
        {
            log(&format!(
                "{} — {runs} run(s), {quads} glyph(s)",
                sample.line()
            ));
        }
        Step::Continue
    }

    fn detach(&mut self) {
        // Dropping the renderer is what drops the `wgpu::Surface`, and this is
        // the call the destroy handshake waits on.
        self.renderer = None;
        // **And everything else.** The arena and the painter are the large
        // ones, and nothing here is kept for a rebuild: `attach` builds the
        // arena and the scene again whatever this leaves, so this implementation
        // has nothing the next attach needs.
        //
        // It is no longer also a leak. The loop's state is leaked — a posted
        // vsync callback cannot be cancelled, so it must stay readable — and
        // this object used to be retained inside it for the life of the
        // process, once per surface cycle. `LoopState::shut_down` now drops the
        // implementation after calling this (issue #1085).
        self.live = None;
        self.arena = Arena::new();
        self.painter = GpuPainter::new();
        // **The refusal record too**, for the reason `LoopState::acquire`
        // clears its own `last_refused`: the device after a rebuild is not the
        // one the record was written against. Left set, it would answer
        // `refusal_reason` for a renderer that has refused nothing — and the
        // loop asks only when it reports a refusal, so a `wanted == configured`
        // frame never calls `resize` to overwrite it (issue #1194).
        self.refusal.clear();
    }
}

/// Starts the showcase on a Surface.
///
/// `scene` names one of the showcase's scenes; an unknown or absent name draws
/// the first rather than failing the launch. Returns an opaque handle, or 0.
///
/// # Safety
///
/// Called by the JVM with a valid environment and a live `Surface`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeStart<'local>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    surface: JObject<'local>,
    scene: JString<'local>,
    width: jint,
    height: jint,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<jlong> {
            let name: Option<String> = if scene.is_null() {
                None
            } else {
                Some(scene.try_to_string(env)?)
            };
            let chosen = select(name.as_deref());
            log(&format!("scene {} — {}", chosen.name, chosen.summary));

            // SAFETY: `env` and `surface` are the JVM's own, valid for this call.
            let window = unsafe {
                ndk_sys::ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast())
            };
            if window.is_null() {
                log("ANativeWindow_fromSurface returned null");
                return Ok(0);
            }

            // A factory: the value is built on the render thread, because an
            // `Arena` and a `LiveScene` hold a boxed solver and boxed closures
            // that are not `Send`. Only `chosen` crosses, and a `&'static` is.
            let frames = move || -> Box<dyn Frames> {
                Box::new(ShowcaseFrames {
                    scene: chosen,
                    arena: Arena::new(),
                    live: None,
                    painter: GpuPainter::new(),
                    renderer: None,
                    elapsed: 0.0,
                    phase: u64::MAX,
                    timing: Timing::new(),
                    refusal: Refusal::default(),
                })
            };
            // SAFETY: `window` is the reference `fromSurface` returned, which is
            // owned until the handshake completes.
            let host = unsafe {
                dashscene_android::loop_::start(
                    window.cast(),
                    frames,
                    width.max(0) as u32,
                    height.max(0) as u32,
                )
            };
            if host.is_null() {
                // SAFETY: the one reference `fromSurface` gave.
                unsafe { ndk_sys::ANativeWindow_release(window) };
                return Ok(0);
            }
            Ok(host as jlong)
        })
        .resolve::<LogErrorAndDefault>()
}

/// Reports a new **physical**-pixel extent.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeResize(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    width: jint,
    height: jint,
) {
    // SAFETY: the caller promises `handle` is live.
    unsafe {
        dashscene_android::loop_::resize(
            handle as *mut dashscene_android::AndroidHost,
            width.max(0) as u32,
            height.max(0) as u32,
        )
    };
}

/// **The destroy handshake.** Blocks until the loop has stopped and the surface
/// has been dropped.
///
/// # Safety
///
/// `handle` must be live, and must not be used again.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeStop(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    // SAFETY: the caller promises this is the handle's last use.
    unsafe { dashscene_android::loop_::destroy(handle as *mut dashscene_android::AndroidHost) };
    log("stopped — the surface is gone");
}

/// Whether the frame loop is still live.
///
/// # Safety
///
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeIsRunning(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jboolean {
    // SAFETY: the caller promises `handle` is live.
    unsafe { dashscene_android::loop_::is_running(handle as *const dashscene_android::AndroidHost) }
}
