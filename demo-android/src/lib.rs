//! The Android showcase host: the same demonstration `demo` and `demo-web` run,
//! on a `SurfaceView` (story #842).
//!
//! # Why this does not go through the C ABI
//!
//! `dashscene-android`'s own document path does, and that is D2 working as
//! intended. This host cannot. The showcase's scenes are **built in code** —
//! `SceneBuilder` is `fn(&mut Arena, u32, u32) -> LiveScene` — and the ABI's
//! arena lives inside an opaque `DsRuntime`, with no builder entry point: that
//! is layer 2, D8, deferred with its layer rather than invented here.
//!
//! So this host owns the arena, the scene, the painter and the surface, exactly
//! as `demo` and `demo-web` do, and meets the platform half at
//! `dashscene_android::Frames`. The render thread, the looper, the vsync
//! callback and the destroy handshake are that crate's and are not restated
//! here — a second Android frame loop, written beside the first because the
//! first only knew about `.dsb` bytes, is the divergence story #834 exists to
//! prevent.
//!
//! # Why the text draws at all
//!
//! Each scene builds its own solver. `typography::build` takes the one
//! `showcase::resources::solver` builds, carrying the fonts and the
//! typesetter, so the `LiveScene`
//! this host is handed can already measure and stage glyph runs. That is worth
//! saying because a **loaded document** needs the same thing supplied to it:
//! `ds_runtime_load_document` injects a bare `TaffySolver`, which has no
//! typesetter and no atlases, so a `.dsb` with text collapses its boxes and
//! draws no glyphs.
//!
//! **A second entry point takes them** (story #947). Story #863 gave
//! `dashscene-desktop` and `dashscene-web` a `TextResources` parameter their
//! embedder fills, and neither a `Typesetter` nor an `Atlas` has a C
//! representation — so `ds_runtime_load_document_with_text` takes their inputs
//! instead: one descriptor per face, pairing the font file's bytes with the
//! committed sheet its glyphs sample. `dashscene_android`'s
//! `nativeSurfaceCreatedWithText` carries them through. This host draws scenes
//! built in code, so it neither hits the gap nor calls the JNI entry point that
//! closes it. **Nothing in this repository calls that one from Java yet**
//! (issue #969); the C entry point beneath it is covered by this workspace's
//! own tests.
//!
//! # What an embedder should not read into this
//!
//! `publish = false`. This is a demonstration, and the scene registry, the
//! scripted pulse and the timing instrument below are demonstration concerns. An
//! embedder writes its own `Frames` and keeps the loop.

mod refusal;
mod timing;

pub use refusal::Refusal;
pub use timing::{Sample, Timing};

/// Picks a scene by name, falling back to the first.
///
/// Compiled on every target and tested on the host: it is one of the things in
/// this crate that can be wrong without a device, so it is kept out of the
/// platform half for the reason `dashscene-web` keeps `fetch` and `shown` out of
/// its own. [`Refusal`] is the other, and its module states the same rule.
pub fn select(name: Option<&str>) -> &'static showcase::Showcase {
    match name.and_then(showcase::by_name) {
        Some(scene) => scene,
        // Not an error. A launch with no extra, or with one naming a scene that
        // does not exist, should still draw something rather than a blank
        // window with a log line — this is a demonstration, and the first scene
        // is as good a default as any.
        None => &showcase::SCENES[0],
    }
}

/// Which way [`advance`] walks the scene registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walk {
    Next,
    Previous,
}

/// The next or previous index over `count` entries, wrapping at both ends.
///
/// **`count` is a parameter rather than `showcase::SCENES.len()` read inside**,
/// so that the wrap is testable at counts the registry does not currently have.
/// With exactly three scenes committed, an implementation hard-coding `% 3` and
/// one wrapping over the registry are the same function, and no test could tell
/// them apart — which matters because the registry is the entry order the Unity
/// host walks too, and a length written down here would leave the two hosts
/// disagreeing about which scene "next twice" reaches the day a fourth lands.
///
/// Returns `index` unchanged for a `count` of zero, which no caller can
/// produce: the registry is a non-empty constant.
pub fn advance(index: usize, count: usize, walk: Walk) -> usize {
    if count == 0 {
        return index;
    }
    match walk {
        Walk::Next => (index + 1) % count,
        Walk::Previous => (index + count - 1) % count,
    }
}

/// One command from the shared showcase vocabulary.
///
/// The discriminants are the contract's and cross JNI as plain integers, so
/// renumbering them here silently rebinds every gesture and every key the
/// harness sends. `docs/decisions/the-showcase-hosts-share-one-surface.md`
/// carries the bindings; this carries the codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Next,
    Previous,
    Action,
    Orientation,
    Readout,
}

impl Command {
    /// The command `code` names, or `None` for a code no gesture produces.
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Next),
            1 => Some(Self::Previous),
            2 => Some(Self::Action),
            3 => Some(Self::Orientation),
            4 => Some(Self::Readout),
            _ => None,
        }
    }
}

/// A launch that photographs one scene in one state, rather than running the
/// demonstration.
///
/// Every field is required. A capture with a defaulted signal or phase is a
/// capture of the wrong state, and the host-to-host comparison it feeds would
/// be meaningless rather than merely wrong — so a partial specification is not
/// a capture at all, and the launch runs the demonstration instead.
#[derive(Clone)]
pub struct Capture {
    pub scene: &'static showcase::Showcase,
    pub phase: u64,
    pub signal: f32,
}

impl std::fmt::Debug for Capture {
    /// Names the scene rather than the whole registry entry: `Showcase` holds
    /// function pointers and derives nothing, and the name is what a log line
    /// about a capture needs anyway.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("scene", &self.scene.name)
            .field("phase", &self.phase)
            .field("signal", &self.signal)
            .finish()
    }
}

impl Capture {
    /// The capture these three launch extras name, or `None`.
    ///
    /// **An unknown scene name is refused rather than falling back to the
    /// first**, which is the opposite of [`select`]'s rule and deliberately so:
    /// drawing something anyway is right for a demonstration and wrong for a
    /// measurement, where it would silently photograph the wrong scene.
    pub fn parse(scene: Option<&str>, phase: Option<i64>, signal: Option<f32>) -> Option<Self> {
        let scene = showcase::by_name(scene?)?;
        let phase = u64::try_from(phase?).ok()?;
        let signal = signal?;
        if !signal.is_finite() {
            return None;
        }
        Some(Self {
            scene,
            phase,
            signal: signal.clamp(0.0, 1.0),
        })
    }
}

/// What the three capture extras asked for.
///
/// **Three outcomes, not two.** `Capture::parse` answers `Option`, and a
/// caller reading `None` as "no capture was asked for" runs the demonstration
/// on `SCENES[0]` while `DemoActivity` has already hidden the readout — so a
/// single mistyped letter in `--es capture_scene` produced a plausible
/// photograph of the wrong scene at whatever phase the wall clock had reached,
/// silently. That is the exact outcome
/// `docs/decisions/the-showcase-hosts-share-one-surface.md` forbids, and the
/// Unity sample already refuses it by name.
// No `PartialEq`: `Ready` carries a `&'static Showcase`, which holds function
// pointers and derives nothing. Tests match on the variant.
#[derive(Debug, Clone)]
pub enum CaptureRequest {
    /// No `capture_scene` extra. Run the demonstration.
    Absent,
    /// `capture_scene` named a scene the registry does not carry. **Refused**:
    /// a measurement that photographs a scene nobody asked for is worse than
    /// no measurement, which is why this is not `select`'s fall back to the
    /// first.
    UnknownScene(String),
    /// `capture_scene` arrived without a usable phase or signal. Not a
    /// capture, so the demonstration runs — and the readout is shown, because
    /// nothing is being photographed.
    Partial,
    /// All three, and the scene exists.
    Ready(Capture),
}

impl CaptureRequest {
    /// Reads the three extras.
    ///
    /// The sentinels are `DemoNative.nativeStart`'s: a phase of `-1` and a
    /// signal of `NaN` are what `getIntExtra`/`getFloatExtra` return for an
    /// absent extra, and both read as [`CaptureRequest::Partial`].
    /// **Partial is decided before the name is looked up**, which is the order
    /// `DashsceneShowcase.ReadCaptureRequest` uses and the order that keeps a
    /// launch recoverable. Deciding the name first makes a request that is
    /// both mistyped and partial — `--es capture_scene typograhy` with no
    /// other extra, one keystroke away — a refusal, and a refusal stops the
    /// loop before it starts, so the surface stays black for the life of the
    /// activity. A partial set was never a capture; it runs the demonstration
    /// whatever the name says.
    #[must_use]
    pub fn of(scene: Option<&str>, phase: Option<i64>, signal: Option<f32>) -> Self {
        let Some(name) = scene else {
            return Self::Absent;
        };
        if name.is_empty() {
            return Self::Partial;
        }
        let (Some(phase), Some(signal)) = (phase, signal) else {
            return Self::Partial;
        };
        if u64::try_from(phase).is_err() || !signal.is_finite() {
            return Self::Partial;
        }
        match Capture::parse(Some(name), Some(phase), Some(signal)) {
            Some(capture) => Self::Ready(capture),
            None => Self::UnknownScene(name.to_owned()),
        }
    }
}

/// The on-screen readout's text: the scene, the extent, and every term this
/// host measures.
///
/// **Shape shared with the Unity host, term names not.** The contract fixes the
/// order — entry, extent, then one labelled term per measured span — and each
/// host names its own spans, because `paint` and `submit` here and `draw` there
/// measure genuinely different parts of a frame. Printing them under one word
/// is the error `demo/src/shell.rs` warns about.
pub fn readout(sample: &Sample, width: u32, height: u32) -> String {
    format!(
        "{}  {width}x{height}\ntick {:.2} ms   paint {:.2} ms   submit {:.2} ms\np50 {:.2}   p95 {:.2}   max {:.2}   {} frames",
        sample.scene,
        sample.tick_mean,
        sample.paint_mean,
        sample.mean,
        sample.p50,
        sample.p95,
        sample.max,
        sample.frames,
    )
}

#[cfg(target_os = "android")]
mod host;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_walks_the_registry_and_wraps() {
        let mut index = 0usize;
        index = advance(index, showcase::SCENES.len(), Walk::Next);
        assert_eq!(showcase::SCENES[index].name, "typography");
        index = advance(index, showcase::SCENES.len(), Walk::Next);
        assert_eq!(showcase::SCENES[index].name, "layout");
        index = advance(index, showcase::SCENES.len(), Walk::Next);
        assert_eq!(showcase::SCENES[index].name, "surfaces", "next wraps");
    }

    #[test]
    fn previous_walks_the_other_way_and_wraps() {
        assert_eq!(
            showcase::SCENES[advance(0, showcase::SCENES.len(), Walk::Previous)].name,
            "layout"
        );
    }

    /// The registry is the entry order both Android hosts walk. A host that
    /// hard-coded the length would silently stop walking a fourth scene, and
    /// the two hosts would then disagree about what "next twice" reaches.
    #[test]
    fn the_walk_reaches_every_scene_and_returns_to_the_start() {
        let mut seen = std::collections::BTreeSet::new();
        let mut index = 0usize;
        for _ in 0..showcase::SCENES.len() {
            seen.insert(showcase::SCENES[index].name);
            index = advance(index, showcase::SCENES.len(), Walk::Next);
        }
        assert_eq!(
            seen.len(),
            showcase::SCENES.len(),
            "the walk visited {} of {} scenes",
            seen.len(),
            showcase::SCENES.len()
        );
        assert_eq!(index, 0, "a full walk returns to where it started");
    }

    /// The wrap is over the count it is given, not over the three scenes that
    /// happen to be committed. This is the case that tells an implementation
    /// wrapping at `% 3` apart from one wrapping at the registry's length.
    #[test]
    fn the_wrap_follows_the_count_it_is_given() {
        assert_eq!(advance(3, 4, Walk::Next), 0, "wraps at four");
        assert_eq!(advance(0, 4, Walk::Previous), 3, "wraps back at four");
        assert_eq!(advance(2, 4, Walk::Next), 3, "does not wrap early");
        assert_eq!(advance(0, 1, Walk::Next), 0, "a single entry stays put");
        assert_eq!(advance(0, 1, Walk::Previous), 0);
    }

    /// The discriminants are the contract's, and Java sends them as plain
    /// integers. A renumbering here silently rebinds every gesture.
    #[test]
    fn every_command_code_round_trips_and_nothing_else_decodes() {
        for (code, expected) in [
            (0, Command::Next),
            (1, Command::Previous),
            (2, Command::Action),
            (3, Command::Orientation),
            (4, Command::Readout),
        ] {
            assert_eq!(Command::from_code(code), Some(expected), "code {code}");
        }
        assert_eq!(Command::from_code(5), None);
        assert_eq!(Command::from_code(-1), None);
    }

    /// A capture with a defaulted signal is a capture of the wrong state, and
    /// the comparison it feeds would be meaningless rather than merely wrong.
    /// All three are required together or the launch is not a capture at all.
    #[test]
    fn a_capture_needs_all_three_of_its_parameters() {
        assert!(Capture::parse(Some("layout"), Some(2), Some(0.5)).is_some());
        assert!(Capture::parse(None, None, None).is_none());
        assert!(Capture::parse(Some("layout"), Some(2), None).is_none());
        assert!(Capture::parse(Some("layout"), None, Some(0.5)).is_none());
        assert!(Capture::parse(None, Some(2), Some(0.5)).is_none());
    }

    /// An unknown scene name in a capture is refused rather than falling back
    /// to the first, which is the opposite of `select`'s rule and deliberately
    /// so: a launch that draws something is right for a demonstration and
    /// wrong for a measurement.
    #[test]
    fn a_capture_of_an_unknown_scene_is_refused_rather_than_defaulted() {
        assert!(Capture::parse(Some("not-a-scene"), Some(0), Some(0.0)).is_none());
    }

    /// The three outcomes are distinguishable, which is the whole point of the
    /// type: reading a bare `None` as "no capture was asked for" ran the
    /// demonstration on the first scene with the readout already hidden.
    #[test]
    fn a_refused_capture_is_not_the_same_answer_as_no_capture() {
        assert!(matches!(
            CaptureRequest::of(None, None, None),
            CaptureRequest::Absent
        ));
        assert!(
            matches!(
                CaptureRequest::of(Some("typograhy"), Some(2), Some(0.5)),
                CaptureRequest::UnknownScene(ref name) if name == "typograhy"
            ),
            "one transposed letter must not read as `no capture was asked for`"
        );
        assert!(matches!(
            CaptureRequest::of(Some("layout"), None, Some(0.5)),
            CaptureRequest::Partial
        ));
        assert!(matches!(
            CaptureRequest::of(Some("layout"), Some(2), Some(0.5)),
            CaptureRequest::Ready(_)
        ));
    }

    /// A request that is both mistyped and partial is **partial**, not
    /// refused — and the order matters because a refusal stops the loop
    /// before it starts.
    ///
    /// `--es capture_scene typograhy` with no other extra is one keystroke
    /// from a real launch. Looking the name up first made it a refusal, so the
    /// surface stayed black for the life of the activity while the Java half,
    /// which never sees the name, had left the readout visible and logged that
    /// the demonstration was running. `DashsceneShowcase.ReadCaptureRequest`
    /// guards the phase and the signal before the name for the same reason.
    #[test]
    fn a_mistyped_name_without_the_other_extras_runs_the_demonstration() {
        assert!(
            matches!(
                CaptureRequest::of(Some("typograhy"), Some(-1), Some(f32::NAN)),
                CaptureRequest::Partial
            ),
            "a partial set was never a capture, whatever the name says"
        );
        assert!(
            matches!(
                CaptureRequest::of(Some(""), Some(2), Some(0.5)),
                CaptureRequest::Partial
            ),
            "an empty name is what `getStringExtra` gives for `--es capture_scene \"\"`"
        );
        // Refused only when the request is otherwise whole, which is the one
        // case where drawing something anyway photographs the wrong scene.
        assert!(matches!(
            CaptureRequest::of(Some("typograhy"), Some(2), Some(0.5)),
            CaptureRequest::UnknownScene(_)
        ));
    }

    /// The two sentinels `DemoNative.nativeStart` actually delivers for an
    /// absent extra. Neither may become a capture, and `-1` in particular must
    /// not wrap: `u64::MAX` is the "phase not yet written" value
    /// `ShowcaseFrames` initialises to, so a wrapped `-1` would make the first
    /// frame skip both the pulse and the signal.
    #[test]
    fn the_absent_extra_sentinels_are_not_a_capture() {
        assert!(
            matches!(
                CaptureRequest::of(Some("layout"), Some(-1), Some(0.5)),
                CaptureRequest::Partial
            ),
            "getIntExtra's -1 default must not become a phase"
        );
        assert!(
            matches!(
                CaptureRequest::of(Some("layout"), Some(2), Some(f32::NAN)),
                CaptureRequest::Partial
            ),
            "getFloatExtra's NaN default must not become a signal"
        );
        assert!(matches!(
            CaptureRequest::of(Some("layout"), Some(2), Some(f32::INFINITY)),
            CaptureRequest::Partial
        ));
    }

    /// A signal outside the authored range lands at the end of it rather than
    /// being refused, because a harness rounding 1.0 to 1.0000001 is not asking
    /// for a different state.
    #[test]
    fn a_capture_signal_outside_the_range_is_clamped_into_it() {
        let above = Capture::parse(Some("layout"), Some(0), Some(40.0)).expect("a capture");
        assert!(
            (above.signal - 1.0).abs() < f32::EPSILON,
            "{}",
            above.signal
        );
        let below = Capture::parse(Some("layout"), Some(0), Some(-40.0)).expect("a capture");
        assert!(below.signal.abs() < f32::EPSILON, "{}", below.signal);
    }

    /// Every path that installs a scene records the extent it installed it at.
    ///
    /// **A structural scan, and it says what it cannot do.** `host.rs` is
    /// `#[cfg(target_os = "android")]`, so no test on a workstation can
    /// construct a `ShowcaseFrames` or drive `attach` — and the defect this
    /// pins was exactly that: `attach` built a scene and left `extent` at
    /// `(0, 0)`, while `dashscene_android::LoopState::step` calls `resize` only
    /// when the extent CHANGES, so a surface that never resized left every
    /// width-dependent input dead for its whole life. What holds the invariant
    /// now is that `extent`, `arena` and `live` are written in one place;
    /// this asserts there is still only one such place. It cannot tell whether
    /// the extent written is the right one — that is the device pass on issue
    /// #1329.
    #[test]
    fn a_scene_is_installed_in_exactly_one_place_and_it_records_the_extent() {
        let host = include_str!("host.rs");
        assert_eq!(
            host.matches("self.live = Some(").count(),
            1,
            "a second place that installs a scene is a second place that can \
             forget the extent; put it through `install` instead"
        );
        assert_eq!(
            host.matches("self.extent = ").count(),
            1,
            "the extent is written where the scene is installed and nowhere else"
        );
        let install = host
            .split_once("fn install(&mut self, width: u32, height: u32) {")
            .expect("host.rs still declares `fn install`")
            .1;
        let body = &install[..install.find("\n    }").expect("`install` still ends")];
        assert!(
            body.contains("self.extent = ") && body.contains("self.live = Some("),
            "both writes belong to `install`, so neither path can take one \
             without the other: {body}"
        );
        // Issue #1396: `elapsed` is the third thing a fresh scene needs, and
        // it ran on across a scene change, so the new scene skipped every
        // phase the old one had reached. It belongs to the same one place, for
        // the same reason.
        assert!(
            body.contains("self.elapsed = 0.0;"),
            "installing a scene starts its clock; a scene that inherits the \
             last one's `elapsed` skips its own phase 0: {body}"
        );
    }

    /// Issue #1395. A capture holds one state, so the host that photographs it
    /// must not apply input — `DashsceneShowcase.ReadInput` returns early for
    /// this reason and this host did not, so a stray touch or an overlapping
    /// `adb shell input` moved the scene off the state being photographed.
    ///
    /// **A source scan, and it says what it cannot do.** `host.rs` is
    /// `#[cfg(target_os = "android")]`, so nothing here can construct a
    /// `ShowcaseFrames` and drive `frame`. This asserts the guard is present
    /// and that it precedes every application; it cannot assert that the guard
    /// is reached. The device pass on issue #1329 is what would.
    #[test]
    fn a_capture_drains_input_without_applying_it() {
        let host = include_str!("host.rs");
        let drain = host
            .split_once("fn drain_input(&mut self) {")
            .expect("host.rs still declares `fn drain_input`")
            .1;
        let body = &drain[..drain.find("\n    }").expect("`drain_input` still ends")];
        let guard = body
            .find("if self.capture.is_some() {")
            .expect("drain_input still refuses to apply input under a capture");
        for applied in ["self.build(", "run_action(", "set_signal("] {
            let at = body
                .find(applied)
                .unwrap_or_else(|| panic!("drain_input no longer applies `{applied}`"));
            assert!(
                guard < at,
                "the capture guard must precede `{applied}`, or a capture \
                 applies it before refusing"
            );
        }
    }

    #[test]
    fn the_readout_names_the_scene_the_extent_and_every_measured_term() {
        let sample = Sample {
            scene: "layout".to_string(),
            frames: 240,
            tick_mean: 0.183,
            paint_mean: 0.012,
            paint_p50: 0.011,
            mean: 9.94,
            p50: 9.90,
            p95: 10.84,
            max: 11.17,
            fps_if_unpaced: 98.5,
        };
        let text = readout(&sample, 1080, 2340);
        assert!(text.contains("layout"), "{text}");
        assert!(text.contains("1080x2340"), "{text}");
        // **Label and value together, not each separately.** Asserting
        // `contains("tick")` and `contains("0.18")` passed with the arguments
        // transposed, which is the one thing the readout's stated order exists
        // to prevent.
        assert!(text.contains("tick 0.18 ms"), "{text}");
        assert!(text.contains("paint 0.01 ms"), "{text}");
        assert!(text.contains("submit 9.94 ms"), "{text}");
        assert!(text.contains("p50 9.90"), "{text}");
        assert!(text.contains("p95 10.84"), "{text}");
        assert!(text.contains("max 11.17"), "{text}");
        assert!(text.contains("240 frames"), "{text}");
        let tick = text.find("tick").expect("tick is named");
        let paint = text.find("paint").expect("paint is named");
        let submit = text.find("submit").expect("submit is named");
        assert!(tick < paint && paint < submit, "the stated order: {text}");
    }

    #[test]
    fn a_named_scene_is_selected() {
        assert_eq!(select(Some("typography")).name, "typography");
        assert_eq!(select(Some("layout")).name, "layout");
    }

    /// A missing or unknown name still draws, rather than failing a launch.
    #[test]
    fn an_unknown_or_absent_name_falls_back_to_the_first_scene() {
        let first = showcase::SCENES[0].name;
        assert_eq!(select(None).name, first);
        assert_eq!(select(Some("not-a-scene")).name, first);
        assert_eq!(select(Some("")).name, first);
    }

    /// Every scene the registry offers can be selected by its own name, so the
    /// launch parameter reaches all of them rather than the two anyone tried.
    #[test]
    fn every_scene_is_reachable_by_name() {
        for scene in showcase::SCENES {
            assert_eq!(
                select(Some(scene.name)).name,
                scene.name,
                "{} is in the registry and not selectable",
                scene.name
            );
        }
    }
}
