//! The native producer the Unity demonstration draws the showcase scenes
//! through (story #1342).
//!
//! # What this library is
//!
//! `dashscene-ffi` plus an appendix. [`pub use dashscene_ffi::*`] re-exports the
//! shipped C ABI — the same code a customer's library is compiled from, linked
//! here as an `rlib` — and this module adds `ds_demo_*` beside it. The
//! demonstration player loads this and nothing else, which is what keeps the
//! runtime table single: `dashscene-ffi`'s `TABLE` is a `thread_local!` living
//! in one instantiation, so a handle `ds_runtime_new` mints resolves in
//! [`ds_demo_build`] only because both are inside this library.
//!
//! **The re-export states the intent; it is measured not to be the mechanism.**
//! Three builds on 2026-08-26, macOS, debug and release alike:
//!
//! - a `cdylib` that names nothing from the `rlib` exports **zero** `ds_*`
//!   symbols against the shipped library's seventeen — the linker keeps no
//!   object nothing references, `#[unsafe(no_mangle)]` or not;
//! - this crate **without** the `pub use`, but calling `demo::install_scene`,
//!   exports all seventeen — one reference pulls the object in and its other
//!   `no_mangle` symbols come with it;
//! - with the `pub use`, the same seventeen.
//!
//! So the line below does not cause the effect it looks like it causes. It is
//! kept because it says out loud what this library is, and because it makes the
//! property independent of how the linker happens to partition objects rather
//! than incidental to it. **`just demo-exports` is the guarantee**, and it holds
//! the property whichever way that goes.
//!
//! # Why the demonstration needs a producer at all
//!
//! `showcase::SceneBuilder` is `fn(&mut Arena, u32, u32) -> LiveScene` and the
//! scenes' motion is host-driven: `ScenePulse` writes a signal every frame and
//! `SceneAction` runs a variant switch. No entry point in the shipped ABI
//! mutates a document, because that is layer 1 and layer 1 is `v1` for every
//! host (issues #1261, #1262). So a C# host cannot animate these scenes, and
//! re-authoring them in C# would be a second definition that drifts from the one
//! `demo-android` draws — which is the comparison the Unity demonstration exists
//! to make.
//!
//! P3 is satisfied rather than bent: producers mutate and the runtime owns time,
//! and a native producer that stages into the arena between ticks does exactly
//! that. Nothing here commits; `ds_runtime_tick` does.
//!
//! # What it is not
//!
//! Not a shipped surface, and not a proposal for one. When layers 1 and 2 land,
//! the demonstration moves to C# and this library stops existing. Until then a
//! C# demonstration would advertise a capability the product does not ship.

use std::cell::Cell;
use std::ffi::c_char;

use showcase::{SCENES, Showcase};

// The shipped ABI, re-exported so the linker keeps it. See the module note.
pub use dashscene_ffi::*;

thread_local! {
    /// The scene [`ds_demo_build`] last installed, and the runtime it went into.
    ///
    /// **Held rather than passed back in on every call**, so a pulse cannot be run
    /// against a scene the host did not build: the alternative is a `scene_index`
    /// parameter on [`ds_demo_pulse`] and [`ds_demo_action`], where a host that
    /// passed the wrong one would drive the wrong scene's script with no error at
    /// all. `LiveScene::signal_named` would simply not find the name.
    ///
    /// **One slot, keyed by handle.** The demonstration shows one scene at a time
    /// and takes its runtime down between them, so a second runtime evicting the
    /// first is not a case it reaches. A host that did hold two would find the
    /// evicted one's pulse refused with [`DsStatus::NoDocument`] and a message
    /// naming the cause — fail-closed, rather than pulsing the wrong scene.
    ///
    /// **The document generation is held with it**, and it is what makes the
    /// slot safe against something the handle cannot express: a
    /// `ds_runtime_load_document` into the same runtime replaces the arena and
    /// the scene while the handle stays valid. Without the generation, a later
    /// `ds_demo_action` would run `layout::switch_variant` against a foreign
    /// arena — succeeding, and silently mutating that document, wherever it
    /// happens to carry a variant set at the same ordinal, and panicking where
    /// it does not. `dashscene_ffi::demo::with_scene` refuses on a mismatch.
    ///
    /// `thread_local!` because the whole ABI is thread-affine: `ds_runtime_new`
    /// records the thread that called it and every later call on that handle is
    /// checked against it, so a per-thread slot cannot be reached from a thread
    /// whose calls would be refused anyway.
    static INSTALLED: Cell<Option<Installed>> = const { Cell::new(None) };
}

/// What [`ds_demo_build`] recorded: which runtime, which scene, and which
/// document generation it was installed at.
#[derive(Clone, Copy)]
struct Installed {
    runtime: DsRuntime,
    generation: u64,
    index: usize,
}

/// How many scenes [`ds_demo_scene_name`] and [`ds_demo_build`] will accept an
/// index for.
///
/// From `showcase::SCENES` rather than a constant here: the list is the corpus's
/// to grow, and a count written down twice is the census-in-prose defect this
/// repository keeps finding.
#[unsafe(no_mangle)]
pub extern "C" fn ds_demo_scene_count() -> u32 {
    u32::try_from(SCENES.len()).unwrap_or(u32::MAX)
}

/// The name of scene `index`, as `ds_last_error_message` writes a string:
/// NUL-terminated into `buf`, truncated on a character boundary to fit `cap`,
/// returning the length the whole string needs including its terminator.
///
/// Zero for an index past [`ds_demo_scene_count`], which is the one answer a
/// caller cannot mistake for a name.
///
/// # Safety
///
/// `buf` is either null or writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_demo_scene_name(index: u32, buf: *mut c_char, cap: usize) -> usize {
    match scene(index) {
        Some(scene) => unsafe { write_c_string(scene.name, buf, cap) },
        None => 0,
    }
}

/// The one-line summary of scene `index`, written as [`ds_demo_scene_name`]
/// writes a name.
///
/// This is what the demonstration puts on screen beside the painter's refusals,
/// so a viewer can compare what the scene claims to show against what arrived.
///
/// # Safety
///
/// `buf` is either null or writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_demo_scene_summary(index: u32, buf: *mut c_char, cap: usize) -> usize {
    match scene(index) {
        Some(scene) => unsafe { write_c_string(scene.summary, buf, cap) },
        None => 0,
    }
}

/// Builds scene `index` into `runtime`'s arena for a drawable of `width` by
/// `height` physical pixels, and installs it as the loaded document.
///
/// This is what a demonstration calls instead of `ds_runtime_load_document`.
/// Afterwards every shipped call behaves as it does over a loaded document:
/// `ds_runtime_tick` commits, `ds_runtime_acquire_frame` leases the tables, and
/// `ds_runtime_atlas` hands out the scene's own sheets.
///
/// **The scene's solver carries a typesetter and those sheets**, which the
/// untexted `.dsb` load path's does not (issue #863) — so text in these scenes
/// lays out
/// and shades, and a viewer is not comparing against a silently text-free
/// document.
#[unsafe(no_mangle)]
pub extern "C" fn ds_demo_build(
    runtime: DsRuntime,
    index: u32,
    width: u32,
    height: u32,
) -> DsStatus {
    let Some(entry) = scene(index) else {
        return out_of_range("ds_demo_build", index);
    };

    // **`showcase::resources::atlases()` is called from INSIDE the closure, and
    // that placement is the whole point.** As an argument it would be evaluated
    // before `install_scene` was entered, so it would run outside that
    // function's panic guard — and it can panic: `corpus_bytes!` reads the
    // corpus by absolute path and panics on a failure, and `from_faces`
    // panics on a sheet it cannot parse. A panic crossing this
    // `extern "C"` boundary aborts the process rather than producing a status.
    let mut generation = 0u64;
    let status = demo::install_scene(runtime, &mut generation, |arena| {
        let live = (entry.build)(arena, width, height);
        (live, showcase::resources::atlases())
    });
    if status == DsStatus::Ok {
        INSTALLED.set(Some(Installed {
            runtime,
            generation,
            index: index as usize,
        }));
    }
    status
}

/// Applies the installed scene's scripted signal change for phase `phase`.
///
/// The scene is a pure function of its phase, which is why this takes a number
/// rather than a direction: the three Rust hosts re-apply the current phase
/// after a rebuild on resize for exactly that reason.
///
/// Stages, never commits. The write is visible to the next `ds_runtime_tick`.
#[unsafe(no_mangle)]
pub extern "C" fn ds_demo_pulse(runtime: DsRuntime, phase: u64) -> DsStatus {
    let Some((entry, generation)) = installed(runtime) else {
        return nothing_installed("ds_demo_pulse");
    };
    demo::with_scene(runtime, generation, "ds_demo_pulse", |live, _arena| {
        (entry.pulse)(live, phase);
    })
}

/// Sets the installed scene's own scalar signal to `value`.
///
/// Distinct from [`ds_demo_pulse`], which applies the scene's *scripted* phase
/// and is a function of a frame counter. This one is the channel a person
/// drives — a drag, or the two keys that put it at either end of its range —
/// and the channel a capture pins so that two hosts can be photographed in the
/// same state.
///
/// **`value` is clamped to `0.0..=1.0`**, the range every showcase signal is
/// authored over, because a C caller can pass anything and the drag path on
/// every Rust host clamps for the same reason.
///
/// **A non-finite value lands at the bottom of the range**, and is not
/// refused. `f32::clamp` propagates NaN rather than clamping it, and a NaN
/// written into a signal reaches the solver, so it cannot be passed through.
/// Refusing it would need a `DsStatus` for an out-of-range scalar and none
/// exists — adding one is a C ABI change, which a demonstration entry point is
/// the wrong place to force. `Mathf.Clamp01` returns NaN for NaN, so a Unity
/// host is a caller that can reach this.
///
/// A scene declaring no such signal name is not an error: the write reports
/// that nothing happened, which is the rule `showcase::input::set_signal`
/// carries for every host.
///
/// Stages, never commits. The write is visible to the next `ds_runtime_tick`.
#[unsafe(no_mangle)]
pub extern "C" fn ds_demo_signal(runtime: DsRuntime, value: f32) -> DsStatus {
    let Some((entry, generation)) = installed(runtime) else {
        return nothing_installed("ds_demo_signal");
    };
    let clamped = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    demo::with_scene(runtime, generation, "ds_demo_signal", |live, _arena| {
        showcase::input::set_signal(live, entry.signal, clamped);
    })
}

/// Runs the installed scene's own variant switch, if it declares one.
///
/// `out_ran` reports whether there was one to run, so a host can say "this scene
/// has no switch" rather than inventing a fallback — which is the seam
/// `showcase::Showcase::action` exists to give it.
///
/// Takes the arena as well as the live scene because that is the whole point:
/// `Txn::set_variant` is an arena mutation and has no signal equivalent.
///
/// # Safety
///
/// `out_ran` is either null or writable for one `bool`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_demo_action(runtime: DsRuntime, out_ran: *mut bool) -> DsStatus {
    // **Written first, on every path.** The `# Safety` section calls this an
    // out-parameter this function writes, and a caller that reads it after a
    // refusal would otherwise read whatever was in its own stack slot.
    if !out_ran.is_null() {
        unsafe { *out_ran = false };
    }
    let Some((entry, generation)) = installed(runtime) else {
        return nothing_installed("ds_demo_action");
    };
    let Some(action) = entry.action else {
        return DsStatus::Ok;
    };
    let status = demo::with_scene(runtime, generation, "ds_demo_action", |live, arena| {
        action(live, arena);
    });
    if !out_ran.is_null() {
        unsafe { *out_ran = status == DsStatus::Ok };
    }
    status
}

/// Refuses an index past the end of the scene list, naming what was asked for
/// and what exists — the convention every indexable refusal in `dashscene-ffi`
/// follows.
///
/// # Why [`DsStatus::NoSuchRoot`] and not a status of its own
///
/// `dashscene-ffi` gives each indexable thing its own status — `NoSuchRoot` for
/// a shown-root ordinal, `NoSuchAtlas` for a sheet — so a twenty-second variant
/// would be the consistent move. It is deliberately not made: `DsStatus` is the
/// **shipped** enum, and it is compiled into `include/dashscene.h`, the
/// package's own `DsStatus`, and `unity/ffi-check`'s status checks. A failure
/// only a demonstration can reach does not belong on a surface every customer
/// compiles against.
///
/// `NoSuchRoot` is the closest shipped meaning rather than an arbitrary pick: a
/// scene index selects which artboard the runtime shows, which is what a
/// shown-root ordinal does on the load path. The message is what distinguishes
/// them, and it names this call.
fn out_of_range(what: &str, index: u32) -> DsStatus {
    demo::refuse(
        DsStatus::NoSuchRoot,
        format!(
            "{what}: no scene {index}; this build carries {} (ds_demo_scene_count reports it)",
            SCENES.len()
        ),
    )
}

/// Refuses a call that names no installed scene, with a message saying so.
///
/// **Through `demo::refuse` rather than a bare status.** A host builds its
/// message from `ds_last_error_message`, so a status returned without one hands
/// the host whatever the last unrelated failure wrote — the package's
/// `DashsceneException` would report a stale document-open error against a
/// pulse. Measured against this crate's own doc comment, which promised "a
/// message naming the cause" that nothing wrote.
fn nothing_installed(what: &str) -> DsStatus {
    demo::refuse(
        DsStatus::NoDocument,
        format!(
            "{what}: no scene has been built into this runtime. \
             ds_demo_build installs one, and only the most recent \
             ds_demo_build on this thread is remembered."
        ),
    )
}

/// Scene `index`, or `None` past the end of the list.
fn scene(index: u32) -> Option<&'static Showcase> {
    SCENES.get(usize::try_from(index).ok()?)
}

/// The scene [`ds_demo_build`] installed into `runtime`, and the document
/// generation it was installed at — or `None` when nothing was installed into
/// *this* handle.
///
/// The generation is not compared here: `dashscene_ffi::demo::with_scene` is
/// what compares it, because only that function is inside a runtime checkout
/// and can read the current one.
fn installed(runtime: DsRuntime) -> Option<(&'static Showcase, u64)> {
    let slot = INSTALLED.get()?;
    if slot.runtime != runtime {
        return None;
    }
    Some((SCENES.get(slot.index)?, slot.generation))
}

/// Writes `text` into `buf` the way `ds_last_error_message` does.
///
/// The truncation is on a character boundary rather than a byte one, for that
/// function's reason: a caller doing a strict UTF-8 decode gets a failure
/// instead of the string it asked for when a multi-byte sequence is cut. Scene
/// names are ASCII today and the summaries are the corpus's to write, so this
/// does not rely on that staying true.
///
/// # Safety
///
/// `buf` is either null or writable for `cap` bytes.
unsafe fn write_c_string(text: &str, buf: *mut c_char, cap: usize) -> usize {
    let bytes = text.as_bytes();
    let needed = bytes.len() + 1;
    if buf.is_null() || cap == 0 {
        return needed;
    }
    let mut take = bytes.len().min(cap - 1);
    while take > 0 && !text.is_char_boundary(take) {
        take -= 1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), take);
        *buf.add(take) = 0;
    }
    needed
}

/// The producer's own behaviour, pinned in Rust.
///
/// **A `cdylib` crate can carry unit tests, and the belief that it could not is
/// what left this file untested at first.** `crate-type = ["cdylib"]` blocks a
/// `tests/` integration target, not an in-file `#[cfg(test)]` module — these
/// compile and run under `cargo test -p demo-producer`, and the sanity tier
/// picks them up.
///
/// **What belongs here rather than in `unity/ffi-check`.** That gate drives the
/// same entry points through the package's C# declarations, which is a
/// different question and a slower one — it needs the .NET SDK, so it is
/// outside `just check`. Everything below is reachable from Rust and costs
/// milliseconds, and each test names the mutation it exists to kill, because
/// every one of them was measured surviving before it was written: the review
/// of PR #1365 emptied both `(entry.pulse)` and `action(...)` and reported
/// 52 of 52 checks passing.
#[cfg(test)]
mod tests {
    use std::ffi::c_char;

    use super::*;

    /// A fresh runtime, or the test fails naming the status.
    fn runtime() -> DsRuntime {
        let mut handle: DsRuntime = 0;
        let status = unsafe { ds_runtime_new(&mut handle) };
        assert_eq!(status, DsStatus::Ok, "ds_runtime_new");
        handle
    }

    /// The last error, as a host reads it.
    fn last_error() -> String {
        let needed = unsafe { ds_last_error_message(std::ptr::null_mut(), 0) };
        let mut buf = vec![0u8; needed];
        unsafe { ds_last_error_message(buf.as_mut_ptr().cast::<c_char>(), needed) };
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    /// The committed rect table's bytes, and the glyph-run table's, after
    /// `ticks` frames.
    ///
    /// **Both, because the scenes drive different things.** `layout`'s signal
    /// moves geometry and `typography`'s drives a string, so a snapshot of one
    /// table alone would make one of the two scenes untestable here.
    fn committed(handle: DsRuntime, ticks: u32) -> (Vec<u8>, Vec<u8>) {
        for _ in 0..ticks {
            let mut advanced = false;
            assert_eq!(
                unsafe { ds_runtime_tick(handle, 1.0 / 60.0, &mut advanced) },
                DsStatus::Ok,
                "ds_runtime_tick"
            );
        }

        let mut frame = std::mem::MaybeUninit::<DsFrame>::zeroed();
        assert_eq!(
            unsafe { ds_runtime_acquire_frame(handle, frame.as_mut_ptr()) },
            DsStatus::Ok,
            "ds_runtime_acquire_frame"
        );
        let frame = unsafe { frame.assume_init() };
        let copy = |slice: &DsSlice| -> Vec<u8> {
            if slice.ptr.is_null() || slice.count == 0 {
                return Vec::new();
            }
            unsafe {
                std::slice::from_raw_parts(slice.ptr.cast::<u8>(), slice.count * slice.stride)
                    .to_vec()
            }
        };
        let taken = (copy(&frame.rects), copy(&frame.glyph_runs));

        let mut was_leased = false;
        assert_eq!(
            unsafe { ds_runtime_release_frame(handle, 1, &mut was_leased) },
            DsStatus::Ok,
            "ds_runtime_release_frame"
        );
        taken
    }

    fn index_of(name: &str) -> u32 {
        u32::try_from(
            SCENES
                .iter()
                .position(|scene| scene.name == name)
                .unwrap_or_else(|| panic!("no scene named {name}")),
        )
        .expect("the scene list is short")
    }

    fn read(f: unsafe extern "C" fn(u32, *mut c_char, usize) -> usize, index: u32) -> String {
        let needed = unsafe { f(index, std::ptr::null_mut(), 0) };
        if needed <= 1 {
            return String::new();
        }
        let mut buf = vec![0u8; needed];
        unsafe { f(index, buf.as_mut_ptr().cast::<c_char>(), needed) };
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    /// **Kills nothing the tests below do not already kill, and specifically
    /// not the literal it is named for.** The assertion compares `SCENES.len()`
    /// against a function whose body is `SCENES.len()`, so a literal `3`
    /// written today passes it — measured. An off-by-one body does fail here,
    /// but it fails the identity test below too, and there as an out-of-bounds
    /// panic rather than a comparison. It is kept because it is the one place
    /// the count's contract is stated, and because that literal would start
    /// failing the moment the corpus grows a scene.
    #[test]
    fn the_count_is_the_scene_list_and_not_a_number_written_here() {
        assert_eq!(ds_demo_scene_count() as usize, SCENES.len());
    }

    /// Kills: both readers returning `SCENES[0]`'s strings, and the two being
    /// swapped. A length check alone passes both — measured.
    #[test]
    fn every_scene_names_itself_distinctly_and_a_name_is_not_a_summary() {
        let mut names = Vec::new();
        for index in 0..ds_demo_scene_count() {
            let name = read(ds_demo_scene_name, index);
            let summary = read(ds_demo_scene_summary, index);
            assert_eq!(
                name, SCENES[index as usize].name,
                "scene {index} reported the wrong name"
            );
            assert_eq!(
                summary, SCENES[index as usize].summary,
                "scene {index} reported the wrong summary"
            );
            assert_ne!(name, summary, "scene {index}'s name is its summary");
            assert!(!names.contains(&name), "two scenes report the name {name}");
            names.push(name);
        }
        assert!(names.len() > 1, "one scene cannot show a distinctness rule");
    }

    #[test]
    fn an_index_past_the_end_names_nothing() {
        let past = ds_demo_scene_count();
        assert_eq!(read(ds_demo_scene_name, past), "");
        assert_eq!(read(ds_demo_scene_summary, past), "");
    }

    /// Kills: `else { return DsStatus::Ok }` on the out-of-range arm, which
    /// leaves a host with no document, no diagnostic and a success status.
    #[test]
    fn an_out_of_range_build_is_refused_and_the_message_names_both_numbers() {
        let handle = runtime();

        // **Not `ds_demo_scene_count()` itself.** The index and the count are
        // then the same number, so "the message contains both" is one predicate
        // twice — measured: dropping the count from the format string entirely
        // left this test passing.
        let past = ds_demo_scene_count() + 7;
        assert_eq!(ds_demo_build(handle, past, 640, 480), DsStatus::NoSuchRoot);

        let message = last_error();
        assert!(
            message.contains(&past.to_string()),
            "the refusal names the index that was asked for: {message:?}"
        );
        assert!(
            message.contains(&SCENES.len().to_string()),
            "the refusal names the count that exists: {message:?}"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: emptying `(entry.pulse)(live, phase)`, and ignoring `phase`.
    ///
    /// **Two phases compared, not one frame inspected.** A static scene commits
    /// rects and glyph runs perfectly well, so "the tables are non-empty" is
    /// satisfied by a producer whose pulse does nothing at all — which is the
    /// exact mutation that survived 52 of 52 checks.
    #[test]
    fn a_pulse_changes_what_the_scene_commits() {
        for name in ["layout", "typography"] {
            let handle = runtime();
            assert_eq!(
                ds_demo_build(handle, index_of(name), 1280, 800),
                DsStatus::Ok
            );

            // Enough frames for a spring-driven binding to travel: the
            // showcase's smoothed channels only dirty layout during the
            // scheduler drain, so a single tick can show nothing.
            let before = committed(handle, 600);
            assert_eq!(ds_demo_pulse(handle, 1), DsStatus::Ok);
            let after = committed(handle, 600);

            assert!(
                before != after,
                "{name}: phase 1 committed exactly what phase 0 did, so the pulse \
                 reached nothing"
            );
            assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
        }
    }

    /// Kills: emptying the signal write, and ignoring `value`.
    ///
    /// Two values compared, not one frame inspected, for
    /// `a_pulse_changes_what_the_scene_commits`'s reason: a scene commits rects
    /// and glyph runs perfectly well with a signal nobody ever wrote.
    #[test]
    fn a_signal_changes_what_the_scene_commits() {
        for name in ["layout", "typography"] {
            let handle = runtime();
            assert_eq!(
                ds_demo_build(handle, index_of(name), 1280, 800),
                DsStatus::Ok
            );

            assert_eq!(ds_demo_signal(handle, 0.0), DsStatus::Ok);
            let low = committed(handle, 600);
            assert_eq!(ds_demo_signal(handle, 1.0), DsStatus::Ok);
            let high = committed(handle, 600);

            assert!(
                low != high,
                "{name}: the top of the signal's range committed exactly what the \
                 bottom did, so the write reached nothing"
            );
            assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
        }
    }

    /// Kills: passing `value` through unclamped, and letting NaN through.
    ///
    /// Every showcase signal is authored over `0.0..=1.0` and a C caller can
    /// pass anything, so out of range must land at the end of the range —
    /// which is what the drag path does on every Rust host. NaN needs its own
    /// case because `f32::clamp` propagates it rather than clamping it.
    #[test]
    fn a_signal_outside_the_range_lands_at_the_end_of_it() {
        let handle = runtime();
        assert_eq!(
            ds_demo_build(handle, index_of("layout"), 1280, 800),
            DsStatus::Ok
        );

        assert_eq!(ds_demo_signal(handle, 1.0), DsStatus::Ok);
        let at_the_top = committed(handle, 600);
        assert_eq!(ds_demo_signal(handle, 40.0), DsStatus::Ok);
        let past_the_top = committed(handle, 600);
        assert_eq!(
            at_the_top, past_the_top,
            "40.0 committed something 1.0 did not, so the value was not clamped"
        );

        assert_eq!(ds_demo_signal(handle, 0.0), DsStatus::Ok);
        let at_the_bottom = committed(handle, 600);
        assert_eq!(ds_demo_signal(handle, f32::NAN), DsStatus::Ok);
        let not_a_number = committed(handle, 600);
        assert_eq!(
            at_the_bottom, not_a_number,
            "NaN committed something 0.0 did not, so it reached the solver"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: returning a bare `NoDocument` with no `set_last_error`, which
    /// hands a host whatever unrelated failure was recorded last.
    #[test]
    fn a_signal_with_nothing_built_is_refused_and_says_why() {
        let handle = runtime();
        assert_ne!(
            unsafe { ds_runtime_load_document(handle, b"not a dsb".as_ptr(), 9) },
            DsStatus::Ok
        );
        assert_ne!(ds_demo_signal(handle, 0.5), DsStatus::Ok);
        let message = last_error();
        assert!(
            message.contains("ds_demo_signal"),
            "the refusal names the entry point that refused: {message:?}"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: emptying `action(live, arena)`.
    #[test]
    fn the_variant_switch_changes_what_the_scene_commits() {
        let with_a_set = SCENES
            .iter()
            .position(|scene| scene.action.is_some())
            .expect("one showcase scene declares a variant set, or the space bar binds nothing");

        let handle = runtime();
        assert_eq!(
            ds_demo_build(handle, with_a_set as u32, 1280, 800),
            DsStatus::Ok
        );
        let before = committed(handle, 600);

        let mut ran = false;
        assert_eq!(unsafe { ds_demo_action(handle, &mut ran) }, DsStatus::Ok);
        assert!(ran, "the scene declaring a set reported no switch");

        let after = committed(handle, 600);
        assert!(
            before != after,
            "the variant switch committed exactly what was there before it"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: `out_ran = true` unconditionally. Asserting "at least one scene
    /// reports a switch" passes that; the whole vector does not.
    #[test]
    fn only_the_scene_that_declares_a_set_reports_a_switch() {
        for (index, scene) in SCENES.iter().enumerate() {
            let handle = runtime();
            assert_eq!(ds_demo_build(handle, index as u32, 640, 480), DsStatus::Ok);

            let mut ran = false;
            assert_eq!(unsafe { ds_demo_action(handle, &mut ran) }, DsStatus::Ok);
            assert_eq!(
                ran,
                scene.action.is_some(),
                "{}: reported ran={ran} against a declared set of {}",
                scene.name,
                scene.action.is_some()
            );
            assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
        }
    }

    /// Kills: returning a bare `NoDocument` with no `set_last_error`, which
    /// hands a host whatever unrelated failure was recorded last.
    #[test]
    fn a_pulse_with_nothing_built_is_refused_and_says_why() {
        let handle = runtime();
        // A different failure first, so a stale message would be visibly wrong
        // rather than empty.
        assert_ne!(
            unsafe { ds_runtime_load_document(handle, b"not a dsb".as_ptr(), 9) },
            DsStatus::Ok
        );

        assert_eq!(ds_demo_pulse(handle, 1), DsStatus::NoDocument);
        let message = last_error();
        assert!(
            message.contains("ds_demo_pulse"),
            "the refusal names this call rather than the previous failure: {message:?}"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: keying the slot on the handle alone.
    ///
    /// A load replaces the arena and the scene while the handle stays valid, so
    /// without the document generation the producer would drive the showcase's
    /// script against a document it knows nothing about — a variant switch
    /// against a foreign arena, which either mutates that document's own set or
    /// panics.
    #[test]
    fn a_document_load_after_a_build_makes_the_producer_refuse() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../goldens/dsb/v03-paint.dsb"
        ))
        .expect("the committed fixture is readable");

        let handle = runtime();
        assert_eq!(
            ds_demo_build(handle, index_of("layout"), 640, 480),
            DsStatus::Ok
        );
        assert_eq!(
            unsafe { ds_runtime_load_document(handle, bytes.as_ptr(), bytes.len()) },
            DsStatus::Ok
        );

        assert_eq!(
            ds_demo_pulse(handle, 1),
            DsStatus::NoDocument,
            "the pulse drove a document the producer did not build"
        );
        let mut ran = true;
        assert_eq!(
            unsafe { ds_demo_action(handle, &mut ran) },
            DsStatus::NoDocument
        );
        assert!(!ran, "a refused action reported that it ran");
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: deleting the handle comparison in `installed`. The other branch —
    /// nothing built at all — leaves the slot `None` and short-circuits before
    /// the comparison, so only a second live runtime reaches it.
    #[test]
    fn a_second_runtime_evicts_the_first_and_the_first_then_refuses() {
        let first = runtime();
        let second = runtime();
        assert_ne!(first, second);

        assert_eq!(
            ds_demo_build(first, index_of("layout"), 640, 480),
            DsStatus::Ok
        );
        assert_eq!(
            ds_demo_build(second, index_of("typography"), 640, 480),
            DsStatus::Ok
        );

        assert_eq!(
            ds_demo_pulse(first, 1),
            DsStatus::NoDocument,
            "the evicted runtime's pulse ran against the scene the other one built"
        );
        assert_eq!(ds_demo_pulse(second, 1), DsStatus::Ok);

        assert_eq!(ds_runtime_free(first), DsStatus::Ok);
        assert_eq!(ds_runtime_free(second), DsStatus::Ok);
    }

    /// Kills: deleting `announce_document_replaced` from `install_scene`.
    ///
    /// That call is the only notice an attached painter gets that the arena's
    /// generations restarted, and its own doc comment says so — but removing it
    /// passed every gate this change had before this test, because nothing read
    /// the flag it sets.
    #[test]
    fn building_a_scene_announces_the_replacement_to_the_next_frame() {
        let handle = runtime();
        assert_eq!(
            ds_demo_build(handle, index_of("layout"), 640, 480),
            DsStatus::Ok
        );

        let mut advanced = false;
        assert_eq!(
            unsafe { ds_runtime_tick(handle, 1.0 / 60.0, &mut advanced) },
            DsStatus::Ok
        );

        let mut frame = std::mem::MaybeUninit::<DsFrame>::zeroed();
        assert_eq!(
            unsafe { ds_runtime_acquire_frame(handle, frame.as_mut_ptr()) },
            DsStatus::Ok
        );
        let replaced = unsafe { frame.assume_init() }.document_replaced;
        let mut was_leased = false;
        assert_eq!(
            unsafe { ds_runtime_release_frame(handle, 1, &mut was_leased) },
            DsStatus::Ok
        );

        assert!(
            replaced,
            "the first frame after a build did not report the document replaced, so a \
             host would keep every rect index it had cached for the previous one"
        );
        assert_eq!(ds_runtime_free(handle), DsStatus::Ok);
    }

    /// Kills: `min(cap)` instead of `min(cap - 1)` — a NUL written one past the
    /// end — and deleting the character-boundary loop.
    ///
    /// **Unreachable from the package's own reader**, which sizes its buffer to
    /// `needed` before every read, so `cap == needed` on every real call and
    /// neither path is ever taken there. That is exactly why it is tested here.
    #[test]
    fn write_c_string_never_writes_past_its_capacity() {
        const GUARD: u8 = 0xAB;
        for text in ["surfaces", "a\u{00e9}b\u{4e2d}c"] {
            let needed = text.len() + 1;
            for cap in 0..=needed + 1 {
                let mut buf = vec![GUARD; needed + 4];
                let reported =
                    unsafe { write_c_string(text, buf.as_mut_ptr().cast::<c_char>(), cap) };
                assert_eq!(
                    reported, needed,
                    "the size reported is always what it needs"
                );

                if cap == 0 {
                    assert!(buf.iter().all(|b| *b == GUARD), "cap 0 wrote something");
                    continue;
                }
                assert!(
                    buf[cap..].iter().all(|b| *b == GUARD),
                    "text {text:?} at cap {cap} wrote past its capacity"
                );

                let end = buf[..cap]
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap_or_else(|| panic!("text {text:?} at cap {cap} wrote no terminator"));
                let written = std::str::from_utf8(&buf[..end]).unwrap_or_else(|e| {
                    panic!("text {text:?} at cap {cap} was cut mid-character: {e}")
                });
                assert!(text.starts_with(written), "what was written is a prefix");
            }
        }
    }
}
