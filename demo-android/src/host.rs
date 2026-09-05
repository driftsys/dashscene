//! The showcase as a [`Frames`] implementation, and the JNI entry points over
//! it.
//!
//! Compiled on Android and nowhere else. The render thread, the looper, the
//! vsync callback and the destroy handshake are `dashscene-android`'s and are
//! not restated here; what this module owns is what a scene built in code needs
//! and a `.dsb` does not — the arena, the `LiveScene`, the painter and the
//! surface.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Instant;

use dashlang::LiveScene;
use dashpaint::Painter;

use dashscene_android::{AttachError, Frames, Step, log};
use dashscene_core::Arena;
use dashscene_gpu::{Changes, GpuPainter, SurfaceRenderer};
use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong};

use crate::refusal::Refusal;
use crate::timing::Sample;
use crate::timing::Timing;
use crate::{Capture, Command, Walk, advance, readout, select};

/// How long each scripted phase lasts, in seconds.
///
/// The showcase's pulse advances by phase index rather than by time, so the host
/// decides the rate. **2.5 s, which is `demo/src/shell.rs`'s `PULSE_INTERVAL`**
/// — the two are the same demonstration and should step at the same rate. This
/// said 1.0 with a comment claiming it matched, which review caught: the
/// Android host was running the script two and a half times faster.
const PHASE_SECONDS: f32 = 2.5;

/// What the UI thread has asked the render thread to do.
///
/// **A channel this crate owns, not one `dashscene-android` grew.** On Android
/// the input arrives on the UI thread and `ShowcaseFrames` lives on the render
/// thread and is not `Send`, so a command has to cross. `loop_` exposes
/// `start`, `resize`, `is_running` and `destroy` and nothing else, and input is
/// not in that crate's scope — the surface handoff, the choreographer loop and
/// the destroy handshake are. This module's own header states the rule this
/// follows: the scene registry, the scripted pulse and the instrument are
/// demonstration concerns, and an embedder writes its own `Frames`. So the
/// queue is a demonstration concern too.
///
/// Only plain values cross. `Command` and `f32` are `Send`; nothing here holds
/// a reference into either thread's state.
static PENDING: Mutex<Pending> = Mutex::new(Pending::new());

/// The other direction: what the render thread last measured, for the UI
/// thread's readout to show. Written once per completed sample.
static READOUT: Mutex<String> = Mutex::new(String::new());

/// Commands and drags waiting for the next frame.
struct Pending {
    commands: VecDeque<Command>,
    /// The most recent drag position, in physical pixels. Coalesced rather than
    /// queued: a drag produces a `MotionEvent` per touch sample and only the
    /// latest names where the finger is now, so a queue would replay a stale
    /// path one position per frame.
    drag_x: Option<f32>,
}

impl Pending {
    const fn new() -> Self {
        Self {
            commands: VecDeque::new(),
            drag_x: None,
        }
    }
}

/// One showcase scene, drawn through the lean painter.
struct ShowcaseFrames {
    scene: &'static showcase::Showcase,
    /// Where `scene` sits in `showcase::SCENES`, so `advance` can walk from it.
    scene_index: usize,
    /// The extent the scene was last built for, so a scene change can rebuild
    /// at the same size without waiting for a resize that is not coming.
    extent: (u32, u32),
    /// Set when this launch is photographing one state rather than running the
    /// demonstration: the phase does not advance, the signal is held, and the
    /// readout stays hidden.
    capture: Option<Capture>,
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
    /// Installs `self.scene` at this extent: a fresh arena, a fresh
    /// `LiveScene`, and the extent recorded beside them.
    ///
    /// **The only place `extent`, `arena` and `live` are written**, because
    /// they are one fact and a path that writes two of the three is a defect
    /// this host has already had. `attach` built the scene itself and left
    /// `extent` at the `(0, 0)` its factory gives it:
    /// `dashscene_android::LoopState::step` calls `Frames::resize` only when
    /// the extent **changes**, so a launch whose surface reports one extent and
    /// never moves — which the fullscreen theme, the hidden bars and the
    /// consumed insets in `DemoActivity` exist to produce — never reached
    /// `build` at all. Every input needing a width was then dead:
    /// `showcase::input::signal_from_x(x, 0)` answers `None`, so the drag and
    /// both arrow keys wrote no signal; a scene change rebuilt at 0x0; and the
    /// readout printed `0x0` as the extent the shared-surface record says both
    /// hosts must agree on.
    ///
    /// A scene built in code derives every offset from the drawable it is given,
    /// so a new extent means a new scene — the same answer `demo` and
    /// `demo-web` give. The scene brings its own solver, which is why its text
    /// has a typesetter at all.
    fn install(&mut self, width: u32, height: u32) {
        self.extent = (width, height);
        self.arena = Arena::new();
        self.live = Some((self.scene.build)(&mut self.arena, width, height));
    }

    /// [`ShowcaseFrames::install`], plus everything a **re**build has to
    /// discard that a first install has nothing of.
    fn build(&mut self, width: u32, height: u32) {
        self.install(width, height);
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

impl ShowcaseFrames {
    /// Applies everything the UI thread queued since the last frame.
    ///
    /// Before anything borrows `live`, because a scene change rebuilds through
    /// `build`, which takes the whole of `self`.
    fn drain_input(&mut self) {
        let (commands, drag_x) = {
            let mut pending = PENDING.lock().expect("the input queue is never poisoned");
            (std::mem::take(&mut pending.commands), pending.drag_x.take())
        };

        for command in commands {
            match command {
                Command::Next | Command::Previous => {
                    let walk = if command == Command::Next {
                        Walk::Next
                    } else {
                        Walk::Previous
                    };
                    self.scene_index = advance(self.scene_index, showcase::SCENES.len(), walk);
                    self.scene = &showcase::SCENES[self.scene_index];
                    let (width, height) = self.extent;
                    self.build(width, height);
                    log(&format!(
                        "entry {} — {}",
                        self.scene.name, self.scene.summary
                    ));
                }
                Command::Action => {
                    let action = self.scene.action;
                    if let Some(live) = self.live.as_mut() {
                        showcase::input::run_action(live, &mut self.arena, action);
                    }
                }
                // **The Java half's, not this thread's.** `setRequestedOrientation`
                // and a `View`'s visibility are UI-thread calls, and routing them
                // down here only to send them back would add a thread hop to
                // reach the same object.
                Command::Orientation | Command::Readout => {}
            }
        }

        if let Some(x) = drag_x {
            let signal = self.scene.signal;
            let width = self.extent.0;
            if let Some(live) = self.live.as_mut()
                && let Some(value) = showcase::input::signal_from_x(x as f64, width)
            {
                showcase::input::set_signal(live, signal, value);
            }
        }
    }

    /// Hands the finished sample to the UI thread's readout.
    ///
    /// Skipped under a capture: the readout would be composited into the frame
    /// `adb screencap` takes, and a comparison of two hosts' overlays is not a
    /// comparison of two painters.
    fn publish_readout(&self, sample: &Sample) {
        if self.capture.is_some() {
            return;
        }
        let (width, height) = self.extent;
        *READOUT.lock().expect("the readout is never poisoned") = readout(sample, width, height);
    }
}

impl Frames for ShowcaseFrames {
    unsafe fn attach(
        &mut self,
        window: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), AttachError> {
        // **Anything queued before this attach belongs to a surface that is
        // gone.** `PENDING` is process-global, so a command sent while the last
        // surface was tearing down would otherwise be applied to the first
        // frame of the next one — a scene change nobody asked for, at a moment
        // that reads as a launch bug.
        {
            let mut pending = PENDING.lock().expect("the input queue is never poisoned");
            pending.commands.clear();
            pending.drag_x = None;
        }

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
        // `install` rather than `build`: the latter reports `document_replaced`
        // and resets `Timing`, and this renderer was constructed three lines ago
        // with nothing uploaded and nothing sampled. `dashscene-web` names that
        // as the second mechanism not to add — the constructor already
        // establishes the state it would clear. What this must NOT do is build
        // the scene itself, which is what it did until the extent went with it.
        self.install(width, height);
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
        self.drain_input();

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
        match self.capture.as_ref() {
            // **A capture holds one state rather than running the script.** The
            // phase is written once and the signal with it, because the whole
            // point of the mode is that the other host can be photographed in
            // the same state — and a phase advancing on a clock is a different
            // state on every launch.
            Some(capture) => {
                if self.phase != capture.phase {
                    self.phase = capture.phase;
                    (self.scene.pulse)(live, capture.phase);
                    showcase::input::set_signal(live, self.scene.signal, capture.signal);
                }
            }
            None => {
                self.elapsed += dt;
                let phase = (self.elapsed / PHASE_SECONDS) as u64;
                if phase != self.phase {
                    self.phase = phase;
                    (self.scene.pulse)(live, phase);
                }
            }
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
            self.publish_readout(&sample);
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
    capture_scene: JString<'local>,
    capture_phase: jint,
    capture_signal: jfloat,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<jlong> {
            let name: Option<String> = if scene.is_null() {
                None
            } else {
                Some(scene.try_to_string(env)?)
            };
            let capture_name: Option<String> = if capture_scene.is_null() {
                None
            } else {
                Some(capture_scene.try_to_string(env)?)
            };
            // **Three outcomes, and the middle one is why this is not an
            // `Option`.** A capture naming a scene the registry does not carry
            // is refused rather than defaulted — `DemoActivity` has already
            // hidden the readout by then, so falling back to `SCENES[0]` would
            // photograph the wrong scene, in a state the wall clock chose,
            // with nothing on screen to say so. One mistyped letter in
            // `--es capture_scene` is the whole input needed.
            let request = CaptureRequest::of(
                capture_name.as_deref(),
                Some(i64::from(capture_phase)),
                Some(capture_signal),
            );
            let capture = match request {
                CaptureRequest::UnknownScene(name) => {
                    log(&format!(
                        "capture_scene '{name}' is not one of the {} showcase \
                         scenes — refusing rather than photographing another \
                         one. Launch without the capture extras to run the \
                         demonstration.",
                        showcase::SCENES.len(),
                    ));
                    return Ok(0);
                }
                CaptureRequest::Partial => {
                    log("capture_scene arrived without a usable phase and \
                         signal — running the demonstration instead. A capture \
                         takes all three of capture_scene, capture_phase and \
                         capture_signal.");
                    None
                }
                CaptureRequest::Absent => None,
                CaptureRequest::Ready(capture) => Some(capture),
            };
            let chosen = match capture.as_ref() {
                Some(capture) => capture.scene,
                None => select(name.as_deref()),
            };
            let chosen_index = showcase::SCENES
                .iter()
                .position(|entry| entry.name == chosen.name)
                .unwrap_or(0);
            match capture.as_ref() {
                Some(capture) => log(&format!(
                    "capture {} phase {} signal {:.3}",
                    chosen.name, capture.phase, capture.signal
                )),
                None => log(&format!("scene {} — {}", chosen.name, chosen.summary)),
            }

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
                    scene_index: chosen_index,
                    extent: (0, 0),
                    capture,
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

/// Queues one command for the render thread to apply on its next frame.
///
/// Takes no handle: `PENDING` is process-global because this demonstration runs
/// one activity and one loop at a time. An unknown code is dropped rather than
/// guessed at — the codes are the contract's, and inventing a meaning for a
/// sixth would bind a gesture nobody wrote.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeCommand(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    code: jint,
) {
    let Some(command) = Command::from_code(code) else {
        log(&format!("command {code} is not one this host binds"));
        return;
    };
    PENDING
        .lock()
        .expect("the input queue is never poisoned")
        .commands
        .push_back(command);
}

/// Reports where a horizontal drag currently is, in physical pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeDrag(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    x_physical: jfloat,
) {
    PENDING
        .lock()
        .expect("the input queue is never poisoned")
        .drag_x = Some(x_physical);
}

/// The readout text the render thread last published.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_demo_DemoNative_nativeReadout<'local>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
) -> jni::sys::jstring {
    unowned
        .with_env(|env| -> jni::errors::Result<jni::sys::jstring> {
            let text = READOUT
                .lock()
                .expect("the readout is never poisoned")
                .clone();
            Ok(env.new_string(text)?.into_raw())
        })
        .resolve::<LogErrorAndDefault>()
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
