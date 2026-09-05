// The showcase: several documents, one at a time, drawn by the Unity painter.
//
// **What this is.** `FrameLoop` beside it is the smallest thing that draws one
// document — the shape to copy into a host. This one is the demonstration: it
// reads a manifest of documents from `StreamingAssets`, switches between them
// on a key, and reports on screen what the painter did with each. It is the
// Unity counterpart of `demo/`, `demo-web/` and `demo-android/`, and it is
// deliberately narrower than they are. Issue #1329.
//
// **What it shows, and how the two halves differ.** The list is the showcase
// scenes first, then the committed documents, and the page keys walk all of
// them — the arrow keys drive the showing scene's signal
// (`docs/decisions/the-showcase-hosts-share-one-surface.md`):
//
// - **The scenes** — `surfaces`, `typography` and `layout`, built into the
//   runtime's arena by a native producer, with their scripted pulse and, where
//   a scene declares one, their variant switch on the space bar. This is what
//   `demo`, `demo-web` and `demo-android` draw, from the same
//   `showcase::SCENES` definition rather than a C# re-authoring that would
//   drift from it (story #1342).
// - **The documents** — committed `.dsb` files, loaded, ticked and drawn. No
//   pulse and no switch: not one entry point in the shipped C ABI mutates a
//   document, because that is layer 1 and layer 1 is `v1` for every host
//   (issues #1261 and #1262). The count of entry points is deliberately not
//   stated here — it moved twice in one slice, and `unity/ffi-check` holds the
//   set.
//
// **The scenes are drawn through a library a customer does not install.**
// `ds_demo_*` is exported by `unity/demo-producer`, which is `dashscene-ffi`
// plus those entry points; `just unity-demo` builds it and stages it under the
// shipped library's file name, and `just demo-exports` asserts it is the
// shipped seventeen unchanged plus a `ds_demo_`-prefixed set, and
// `unity/ffi-check`'s demonstration pass names them. With
// `DASHSCENE_DEMO_PRODUCER` undefined this file compiles to the document half
// alone, which is what a customer's own build of the sample does.
//
// **What a viewer will see missing, and why it is P4 working rather than a
// defect.** `PackDiagnostic` names six refusals; five of them are kinds both
// Rust painters draw, and the sixth, a layer blur, is one they skip too —
// `PackDiagnostic` names them — so `surfaces` in particular arrives without its
// image, its baked vector field and its shadows. The readout below reports what
// was refused for whatever is showing, so the difference from `demo-web` is
// visible here instead of surprising. Issue #1344 is the painter request; this
// sample reports what was refused rather than drawing it.
//
// **The library this runs against is staged, though the package ships two.**
// Story #1334 put `libdashscene_ffi.dylib` and `libdashscene_ffi.so` inside the
// package, and `just unity-demo` copies the demo producer into the project
// under that same file name — because the player must load ONE library, and the
// package's own copy exports no `ds_demo_*`. Issue #1352 is the follow-up on
// the shipped plugin layout, and nothing here stands in for it.
//
// **The cascade is read with `File`, so a text document is desktop-only here.**
// `StreamingAssetDocument.Resolve` is what makes a document loadable inside an
// Android APK, and it maps — but `LoadDocumentWithText` takes owned bytes, so a
// document needing a font cascade cannot also be mapped (issue #1332). This
// sample takes the mapped path where it can and the owned path where text
// needs it, and on Android the owned path would need `UnityWebRequest` rather
// than `File`.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using Driftsys.Dashscene;
using UnityEngine;
using Stopwatch = System.Diagnostics.Stopwatch;

namespace Driftsys.Dashscene.Samples
{
    /// One entry of the manifest: a document and how to load it.
    [Serializable]
    public sealed class ShowcaseEntry
    {
        /// Path relative to `StreamingAssets`.
        public string path;

        /// What to call it on screen. The file name when empty.
        public string label;

        /// Document ordinal of the artboard to show.
        public uint shownRoot;

        /// Whether this document needs the font cascade. A document with text
        /// drawn without one lays out and shades nothing.
        public bool text;
    }

    /// The manifest `StreamingAssets/showcase.json` holds.
    [Serializable]
    public sealed class ShowcaseManifest
    {
        public ShowcaseEntry[] documents;
    }

    [DisallowMultipleComponent]
    public sealed class DashsceneShowcase : MonoBehaviour
    {
        [Tooltip("The manifest, relative to StreamingAssets.")]
        [SerializeField]
        private string manifestPath = "showcase.json";

        [Tooltip("The font file the cascade uses, relative to StreamingAssets.")]
        [SerializeField]
        private string fontPath = "cascade/Inter-Regular.otf";

        [Tooltip("The committed MSDF sheet beside that font, relative to StreamingAssets.")]
        [SerializeField]
        private string atlasPngPath = "cascade/atlas.png";

        [Tooltip("The metrics blob beside the sheet, relative to StreamingAssets.")]
        [SerializeField]
        private string atlasMetricsPath = "cascade/atlas.metrics";

        [Tooltip("The family the document's text styles name, matched case-insensitively.")]
        [SerializeField]
        private string fontFamily = "Inter";

        [Tooltip("The CSS weight of that face, 1..=1000.")]
        [SerializeField]
        private ushort fontWeight = 400;

        [Tooltip("Commits per second. 0 commits once per Unity frame.")]
        [SerializeField]
        private int commitHz;

        [Tooltip("The camera the document is drawn for. Unassigned takes Camera.main.")]
        [SerializeField]
        private Camera viewCamera;

        /// How often the scripted pulse advances.
        ///
        /// **`demo/src/shell.rs`'s `PULSE_INTERVAL`, which is 2500 ms**, rather
        /// than a number chosen here: a pulse rate that differed would make the
        /// side-by-side comparison the demonstration exists for a comparison of
        /// two different scripts.
        ///
        /// That file's other constant, `PULSES_PER_SCENE`, is deliberately NOT
        /// mirrored: nothing here advances an entry on a pulse count. Without
        /// `-cycle` this sample waits for a page key, and with it the switch
        /// is on elapsed seconds. An earlier version of this comment claimed
        /// both constants, which described behaviour this file does not have.
        private const float PulseSeconds = 2.5f;

        private readonly List<ShowcaseEntry> _entries = new List<ShowcaseEntry>();

        /// Seconds since the last scripted pulse, and which pulse is in effect.
        ///
        /// The scene is a pure function of its phase, which is why the phase is
        /// a counter rather than a direction — the same reason `shell.rs` keeps
        /// one so a rebuild on resize can re-apply it.
        private float _sincePulse;

        private ulong _phase;

        /// The drawable the showing scene was built for, in document units.
        ///
        /// A scene is built for the drawable in physical pixels, which is what
        /// the three Rust hosts do — so its extent is the window's, not a few
        /// hundred units like a committed document. Kept because the camera has
        /// to frame it, and because `Screen` can change under a resize while the
        /// scene stays the size it was built at.
        private uint _builtWidth;

        private uint _builtHeight;

        /// The framing `unity/demo/DemoBuild.cs` set up, read once rather than
        /// restated here — it is the one that suits a committed document.
        private float _documentSize;

        private Vector3 _documentCameraPosition;
        private DashsceneRuntime _runtime;
        private BrgPainter _painter;

        /// The frame-cost instrument, whose definition is stated against
        /// `demo/src/shell.rs`'s in `DashsceneFrameCost.cs`. Issue #1329's
        /// third limb, and the figure issue #1347 sets beside the lean
        /// painter's.
        private readonly DashsceneFrameCost _frameCost = new DashsceneFrameCost();
        private CommitPacer _pacer;
        private int _index;
        private string _status = string.Empty;
        private GUIStyle _readout;

        /// Whether the frame-cost readout is drawn.
        ///
        /// **Read inside `OnGUI` rather than by disabling this behaviour.**
        /// Unity delivers `OnGUI` only to an enabled behaviour, so disabling
        /// took the readout down together with the frame loop — the trap this
        /// file already records one field below. A capture starts with it off,
        /// because `adb screencap` composites the readout into the photograph
        /// and a comparison of two overlays is not a comparison of two painters.
        private bool _readoutVisible = true;

        /// Below this, a movement is a tap that wandered rather than a swipe.
        private const float SwipeMinPixels = 120.0f;

        /// Above this, a touch that neither swiped nor dragged is a long press.
        private const float LongPressSeconds = 0.5f;

        /// A launch photographing one scene in one state, rather than running
        /// the demonstration.
        ///
        /// The three parameters arrive together or not at all. A capture with a
        /// defaulted phase or signal photographs a different state than the
        /// other host is holding, and the comparison it feeds would be
        /// meaningless rather than merely wrong — so a partial set is not a
        /// capture, and the player runs the demonstration instead.
        private sealed class CaptureRequest
        {
            internal string Scene;
            internal ulong Phase;
            internal float Signal;
        }

        private CaptureRequest _capture;

        private Vector2 _touchStart;

        private float _touchStartedAt;

        private bool _touchDragged;

        /// <summary>
        /// Whether this capture's phase and signal have been written.
        /// </summary>
        /// <remarks>
        /// Issue #1394. Tracked separately from <c>_phase</c>, which
        /// <c>Show</c> initialises to 0 — so a <c>capture_phase 0</c> launch,
        /// the one a harness is most likely to ask for, compared equal on its
        /// first frame and never reached <c>SetDemoSignal</c>.
        /// </remarks>
        private bool _captureApplied;

        /// <summary>
        /// Set while a two-finger gesture is in flight, and cleared on the
        /// next single-finger <c>Began</c>.
        /// </summary>
        /// <remarks>
        /// Issue #1397. Without it the two fingers lifting completes finger
        /// 0's gesture — a tap, or a long press — because that finger's
        /// <c>Began</c> ran on the frame before the second arrived.
        /// </remarks>
        private bool _multiTouch;

        private int _readoutHeight;

        /// Seconds between automatic switches, from `-cycle <seconds>` on the
        /// command line. Zero leaves switching to the page keys.
        ///
        /// **A demonstration takes a key press; a check cannot.** Without
        /// this, whether every document in the manifest loads and draws is
        /// answered by a person watching, which is the shape a run cannot
        /// report.
        private float _cycleSeconds;

        private float _sinceSwitch;

        private bool _reported;

        /// Set by [`Fail`]. Stops the frame loop without disabling the
        /// component, so `OnGUI` keeps drawing the reason.
        private bool _failed;

        /// Whether `-quit` was passed: the player exits once every entry has
        /// drawn, which is what lets a run report rather than a person watch.
        private bool _quitWhenEveryEntryHasDrawn;

        private readonly HashSet<int> _drawnEntries = new HashSet<int>();

        private bool _announcedEveryEntry;

        /// How many showcase scenes the staged library carries, or zero when
        /// this build has no producer.
        ///
        /// **Asked of the library once, in `Awake`, and cached.** The value
        /// cannot change for the life of the process, and the reason for asking
        /// at all — that a count written in C# would be a second definition of
        /// `showcase::SCENES` — is served by asking once. Reading it as a
        /// property made a P/Invoke out of `TotalCount`, `IsScene`, `Label` and
        /// `Summary`, several times per frame, in the demonstration whose whole
        /// purpose is a per-frame comparison.
        private int SceneCount => _sceneNames.Length;

        /// Each scene's name and summary, read once beside the count.
        ///
        /// `OnGUI` runs at least twice a frame and asked for both every time,
        /// which was two sizing calls, two reads, two `byte[]` allocations and
        /// two strings per call.
        private string[] _sceneNames = Array.Empty<string>();

        private string[] _sceneSummaries = Array.Empty<string>();

        /// Reads the scene table from the library, or reports why it could not.
        ///
        /// **This is the component's first native call, and until the review of
        /// PR #1365 it was outside every `catch` this file has.** A player built
        /// with `DASHSCENE_DEMO_PRODUCER` but running against a library with no
        /// `ds_demo_*` — which the package's own shipped
        /// `Runtime/Plugins/macOS/libdashscene_ffi.dylib` is — threw out of
        /// `Awake`, so no census line was written, `Fail` was never reached, and
        /// `just unity-demo cycle` reported only that the player had not
        /// reached the end of `Awake`.
        private bool ReadSceneTable()
        {
#if DASHSCENE_DEMO_PRODUCER
            try
            {
                var count = DemoScenes.Count;
                _sceneNames = new string[count];
                _sceneSummaries = new string[count];
                for (var i = 0; i < count; i++)
                {
                    _sceneNames[i] = DemoScenes.Name(i);
                    _sceneSummaries[i] = DemoScenes.Summary(i);
                }
                return true;
            }
            catch (Exception e)
            {
                Fail($"the staged library exports no usable ds_demo_* ({e.GetType().Name}: "
                     + $"{e.Message}). This player was built with DASHSCENE_DEMO_PRODUCER, so "
                     + "it needs unity/demo-producer — the package's own shipped "
                     + $"'{DashsceneRuntime.LibraryName}' does not export them, and both files "
                     + "carry that name. `just unity-demo` builds and stages the right one.");
                return false;
            }
#else
            return true;
#endif
        }

        /// Everything left and right walk: the scenes, then the documents.
        private int TotalCount => SceneCount + _entries.Count;

        /// Whether entry `index` is a scene rather than a document.
        private bool IsScene(int index) => index < SceneCount;

        private void Awake()
        {
            _pacer = new CommitPacer(commitHz);
            _capture = ReadCaptureRequest();
            if (_capture != null)
            {
                _readoutVisible = false;
                Debug.Log($"[showcase] capture: {_capture.Scene} phase {_capture.Phase} "
                          + $"signal {_capture.Signal:F3}");
            }

            try
            {
                // The overlay class: the one of the three that expresses partial
                // coverage, so corners, strokes and clip edges are anti-aliased.
                _painter = new BrgPainter(MaterialClass.UnlitOverlay);
            }
            catch (DashscenePainterException e)
            {
                Fail($"the painter could not be created: {e.Message}");
                return;
            }

            if (_painter.Rung == BrgRung.InstancedWithoutBrg)
            {
                // Nothing is built for that rung, so every frame would be blank
                // with no exception and no log — the one failure that is silent
                // on screen and in the console both.
                Fail($"the painter is on rung {_painter.Rung}: BatchRendererGroup is "
                     + "unsupported on this graphics API and nothing is built for it "
                     + "(docs/decisions/unity-painter-uses-brg.md D3).");
                return;
            }

            // The document's y runs down, so scaling y by -1 is the identity
            // placement.
            _painter.DocumentToWorld = Matrix4x4.Scale(new Vector3(1, -1, 1));

            // **Read from the camera, not copied from `DemoBuild`.** That
            // script picks a size framing the committed documents, and a second
            // copy of the number here would drift from it.
            var startingCamera = viewCamera != null ? viewCamera : Camera.main;
            if (startingCamera != null)
            {
                _documentSize = startingCamera.orthographicSize;
                _documentCameraPosition = startingCamera.transform.position;
            }

            _cycleSeconds = CycleSecondsFromCommandLine();
            _quitWhenEveryEntryHasDrawn =
                Array.IndexOf(Environment.GetCommandLineArgs(), "-quit") >= 0;
            LoadManifest();

            // **A failed manifest read stops here.** `LoadManifest` reports
            // through `Fail` and returns void, so before the reorder below it
            // relied on the `TotalCount == 0` guard to end `Awake` — which held
            // only while a failed read left the total at zero. With the scenes
            // counted first the total is non-zero, the guard is skipped, and
            // `Show(0)` runs and clears `_status` at its end, erasing the one
            // message that says why nothing will be drawn. The log still
            // carries it; the readout, which `Fail`'s own comment calls the
            // only place a person running the player learns anything, did not.
            if (_failed)
            {
                return;
            }

            // **Before the count below, not after it.** `SceneCount` is
            // `_sceneNames.Length`, and that array is empty until this runs — so
            // reading the total first made a player with three scenes and an
            // empty manifest report that it carried no scenes either, abort
            // `Awake`, and never write the census line. It was a P/Invoke
            // property when the guard was written, which is why the order was
            // not load-bearing then and is now.
            if (!ReadSceneTable())
            {
                return;
            }

            if (TotalCount == 0)
            {
                Fail($"nothing to show: {manifestPath} lists no document, and this build "
                     + "carries no showcase scenes either. A player built without "
                     + "DASHSCENE_DEMO_PRODUCER has only the manifest, and nothing scans "
                     + "the directory beside it.");
                return;
            }

            // **The census, before anything is drawn.** `just unity-demo`'s
            // `cycle` action reads it to learn how many entries to wait for and
            // how long to wait — it knows the manifest it wrote and cannot know
            // how many scenes the staged library carries. Without it that
            // recipe would hold a count of its own, which is the drift this
            // repository keeps finding, and it held exactly that until story
            // #1342 added the scenes and the grep silently stopped matching.
            Debug.Log($"[showcase] entries: {TotalCount} ({SceneCount} scene(s), "
                      + $"{_entries.Count} document(s))");

            // **The drawable, in the units a scene is built in.** A scene built
            // for the drawable and a camera framing a fixed extent disagree by
            // exactly the display's backing scale, and nothing printed either
            // number — so the mismatch was visible only to a person looking at
            // the window.
            Debug.Log($"[showcase] drawable: {Screen.width}x{Screen.height} px, "
                      + $"document framing {_documentSize}");

            // **The graphics API, beside the rung it decides.** A frame cost
            // taken on one API is not a figure about another: the painter's
            // rung comes from `BatchRendererGroup.BufferTarget`, which Unity
            // answers per API, and a Pixel 5 gives `RawBuffer` under Vulkan and
            // `ConstantBuffer` under GLES — measured 2026-08-29. A record that
            // does not name the API is a number about no device (issue #1347).
            Debug.Log($"[showcase] graphics: {SystemInfo.graphicsDeviceType}, "
                      + $"{SystemInfo.graphicsDeviceName}, "
                      + $"{SystemInfo.graphicsDeviceVersion}");

            // A capture opens on the scene it names, not on the first entry.
            // An unknown name is refused rather than defaulted: drawing
            // something anyway is right for a demonstration and wrong for a
            // measurement, where it photographs the wrong scene silently. This
            // is `Capture::parse`'s rule on the Rust side, and the two hosts
            // must agree about it or a capture pair is not a pair.
            var opening = 0;
            if (_capture != null)
            {
                opening = Array.IndexOf(_sceneNames, _capture.Scene);
                if (opening < 0)
                {
                    Fail($"capture_scene '{_capture.Scene}' is not one of the "
                         + $"{SceneCount} scenes this library carries");
                    return;
                }
            }
            Show(opening);
        }

        private void Update()
        {
            if (_failed || _painter == null || TotalCount == 0)
            {
                return;
            }

            ReadInput();

            if (_cycleSeconds > 0.0f && TotalCount > 1)
            {
                _sinceSwitch += Time.deltaTime;
                if (_sinceSwitch >= _cycleSeconds)
                {
                    Show((_index + 1) % TotalCount);
                }
            }

            PulseIfShowingAScene();

            // **`_failed` is re-read here** and not only above: `ReadInput` and
            // the cycle both reach `Show`, which fails on a document that will
            // not load — and without this the same frame went on to tick a
            // runtime with no document and reported a second, unrelated error.
            if (_failed || _runtime == null
                || !_pacer.ShouldCommit(Time.deltaTime, out var dt))
            {
                return;
            }

            UpdateEdgeWidth();

            try
            {
                // **The two brackets the frame cost is defined by.** `tick` is
                // the same quantity `demo/src/shell.rs` reports — the same
                // `ds_runtime_tick` onto the same solver — and `draw` is
                // everything of the frame this project executes: the lease,
                // the packing and upload, marking it drawn, and the release.
                // What Unity does after `Update` returns is outside both, and
                // `DashsceneFrameCost.cs` states exactly which parts those are.
                var tickStart = Stopwatch.GetTimestamp();
                _runtime.Tick(dt);
                var tickTicks = Stopwatch.GetTimestamp() - tickStart;

                var first = !_reported;
                var drawStart = Stopwatch.GetTimestamp();
                using (var frame = _runtime.AcquireFrame())
                {
                    _painter.Draw(frame);

                    // `Draw` packs and uploads; whether the frame reached a
                    // screen is Unity's answer, later. A lease disposed
                    // unmarked leaves every commit unshown.
                    frame.MarkDrawn();
                }

                var drawTicks = Stopwatch.GetTimestamp() - drawStart;

                if (first)
                {
                    // Once per document rather than once per frame: what a run
                    // needs is that each one reached the painter, and a line a
                    // frame would bury it.
                    //
                    // **Outside the bracket above**, because a line written
                    // once per document from inside it would land in one frame
                    // of one sample and move that sample's `max` — the column
                    // the first sample of an entry already carries warm-up in.
                    _reported = true;
                    _drawnEntries.Add(_index);
                    Debug.Log($"[showcase] drew {Label(_index)}: "
                              + $"{_painter.InstanceCount} instance(s), rung {_painter.Rung}"
                              + (_painter.Diagnostics.IsClean
                                  ? string.Empty
                                  : $", refused {_painter.Diagnostics}"));
                    AnnounceIfEveryEntryHasDrawn();
                }

                // **`Screen.width`/`Screen.height`, read every frame.** They
                // are the drawable, and they change under a rotation — which
                // is the boundary the instrument discards a part-sample on,
                // and which issue #1346 exercises on purpose.
                var cost = _frameCost.Push(
                    Label(_index), Screen.width, Screen.height, tickTicks, drawTicks);
                if (cost != null)
                {
                    Debug.Log($"[showcase] frame cost — {cost.Line()}");
                }
            }
            catch (DashsceneAbiMismatchException e)
            {
                Fail(e.Message);
            }
            catch (DashscenePainterException e)
            {
                Fail($"the painter refused the frame: {e.Message}");
            }
            catch (DashsceneStrideMismatchException e)
            {
                Fail(e.Message);
            }
            catch (DashsceneException e)
            {
                Fail($"frame failed: {e.Message}");
            }
        }

        /// The capture this launch asked for, or null.
        ///
        /// **Read from the activity's `Intent`**, the same three extras
        /// `demo-android` reads, so one harness command drives both hosts:
        ///
        /// <code>--es capture_scene layout --ei capture_phase 2 --ef capture_signal 0.5</code>
        ///
        /// Returns null off Android, and null for any partial or unparseable
        /// set — never a capture with a defaulted field.
        private CaptureRequest ReadCaptureRequest()
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            try
            {
                using var player = new AndroidJavaClass("com.unity3d.player.UnityPlayer");
                using var activity = player.GetStatic<AndroidJavaObject>("currentActivity");
                using var intent = activity.Call<AndroidJavaObject>("getIntent");

                var scene = intent.Call<string>("getStringExtra", "capture_scene");
                if (string.IsNullOrEmpty(scene))
                {
                    return null;
                }
                var phase = intent.Call<int>("getIntExtra", "capture_phase", -1);
                var signal = intent.Call<float>("getFloatExtra", "capture_signal", float.NaN);
                if (phase < 0 || float.IsNaN(signal) || float.IsInfinity(signal))
                {
                    Debug.LogWarning($"[showcase] capture_scene {scene} came without a usable "
                                     + $"phase ({phase}) or signal ({signal}); running the "
                                     + "demonstration instead");
                    return null;
                }
                return new CaptureRequest
                {
                    Scene = scene,
                    Phase = (ulong)phase,
                    Signal = Mathf.Clamp01(signal),
                };
            }
            catch (Exception e)
            {
                Debug.LogWarning($"[showcase] the capture extras could not be read "
                                 + $"({e.GetType().Name}: {e.Message})");
                return null;
            }
#else
            return null;
#endif
        }

        /// Advances the showing scene's scripted signal, on `demo`'s own
        /// cadence.
        ///
        /// **Staged here and committed by the next `Tick`**, which is P3: the
        /// producer mutates and the runtime owns time. Nothing in this method
        /// commits.
        ///
        /// A document entry has no pulse to run, because no shipped entry point
        /// mutates a document — the header above says why.
        private void PulseIfShowingAScene()
        {
#if DASHSCENE_DEMO_PRODUCER
            if (!IsScene(_index) || _runtime == null || _failed)
            {
                return;
            }

            if (_capture != null)
            {
                // **Written once and then held.** The whole point of a capture
                // is that the other host can be photographed in the same state,
                // and a phase advancing on a clock is a different state on
                // every launch.
                //
                // **Tracked separately from the phase value** (issue #1394).
                // `Show` sets `_phase = 0`, so comparing against `_phase`
                // returned on the first frame of a `capture_phase 0` launch —
                // the phase a harness is most likely to ask for — and
                // `SetDemoSignal` was never reached, leaving the scene on its
                // built-in default signal while the other host held the one it
                // was told. The Rust host has no such hole because
                // `ShowcaseFrames` starts `phase` at `u64::MAX`.
                if (_captureApplied)
                {
                    return;
                }
                _captureApplied = true;
                _phase = _capture.Phase;
                try
                {
                    _runtime.PulseDemoScene(_phase);
                    _runtime.SetDemoSignal(_capture.Signal);
                }
                catch (DashsceneException e)
                {
                    Fail($"the capture state could not be set: {e.Message}");
                }
                return;
            }

            _sincePulse += Time.deltaTime;
            if (_sincePulse < PulseSeconds)
            {
                return;
            }

            // **Subtracted rather than zeroed**, so a frame that overran the
            // interval does not lose the remainder and drift the cadence away
            // from the 2500 ms the Rust hosts pulse at.
            _sincePulse -= PulseSeconds;
            _phase++;

            try
            {
                _runtime.PulseDemoScene(_phase);
            }
            catch (DashsceneException e)
            {
                Fail($"the scripted pulse failed: {e.Message}");
            }
#endif
        }

        /// Runs the showing scene's own variant switch, and says so on screen
        /// when the scene declares none.
        ///
        /// **The scene owns the switch, not this host.** `Txn.set_variant` is an
        /// arena mutation with no signal equivalent, so a host that wanted to
        /// offer one would have to author a variant set against a node it knew
        /// by name — which is the host authoring content, the thing the `demo/`
        /// and `corpus/showcase/` split exists to prevent.
        private void RunTheScenesOwnSwitch()
        {
#if DASHSCENE_DEMO_PRODUCER
            if (!IsScene(_index))
            {
                _status = "space runs a scene's variant switch; this entry is a document.";
                return;
            }

            // **Defensive, and unreachable today.** `Update` returns before
            // `ReadInput` when `_failed` is set, and every `Show` path that
            // leaves `_runtime` null calls `Fail` — so no viewer sees this
            // message. It is kept because the null dereference below is what
            // would happen without it, and split from the branch above because
            // one message for both told a viewer looking at a scene's own label
            // that the entry was a document.
            if (_failed || _runtime == null)
            {
                _status = "space does nothing while this entry has failed to load.";
                return;
            }

            try
            {
                _status = _runtime.RunDemoAction()
                    ? string.Empty
                    : $"{_sceneNames[_index]} declares no variant set, so space does "
                      + "nothing here rather than this host inventing something for it to do.";
            }
            catch (DashsceneException e)
            {
                Fail($"the variant switch failed: {e.Message}");
            }
#else
            _status = "this build carries no showcase scenes (DASHSCENE_DEMO_PRODUCER is off).";
#endif
        }

        /// Reads the shared showcase vocabulary, from keys and from touch.
        ///
        /// **The arrow keys drive the signal and do not navigate.** This file
        /// bound them to the previous and next entry, and `demo/src/input.rs`
        /// binds them to the two ends of the scene's own signal range — two
        /// hosts of one showcase disagreeing about what a key means. The owner
        /// settled it on 2026-08-29 in favour of the desktop binding, which is
        /// the older of the two and the one written to name no scene, so
        /// navigation moved to the page keys.
        /// `docs/decisions/the-showcase-hosts-share-one-surface.md` carries the
        /// table; `measure/android/unity-frame-cost.sh` and the `unity-demo-android`
        /// recipe send the page key because of it.
        private void ReadInput()
        {
            if (_capture != null)
            {
                // A capture holds one state. Input would move it off the state
                // the other host is being photographed in.
                return;
            }

            if (Input.GetKeyDown(KeyCode.PageDown))
            {
                Show((_index + 1) % TotalCount);
            }
            else if (Input.GetKeyDown(KeyCode.PageUp))
            {
                Show((_index + TotalCount - 1) % TotalCount);
            }
            else if (Input.GetKeyDown(KeyCode.Space))
            {
                RunTheScenesOwnSwitch();
            }
            else if (Input.GetKeyDown(KeyCode.UpArrow))
            {
                ToggleOrientation();
            }
            else if (Input.GetKeyDown(KeyCode.R))
            {
                _readoutVisible = !_readoutVisible;
                Debug.Log($"[showcase] readout: {(_readoutVisible ? "shown" : "hidden")}");
            }
            else if (Input.GetKeyDown(KeyCode.LeftArrow))
            {
                DriveSignal(0.0f);
            }
            else if (Input.GetKeyDown(KeyCode.RightArrow))
            {
                DriveSignal(1.0f);
            }

            ReadTouch();
        }

        /// The same five bindings on touch, so the player is drivable by hand.
        ///
        /// A device has no keyboard, so before this the player could only be
        /// driven by `adb shell input keyevent` — which made it a harness and
        /// not a demonstration.
        private void ReadTouch()
        {
            if (Input.touchCount >= 2)
            {
                var second = Input.GetTouch(1);
                if (second.phase == TouchPhase.Began)
                {
                    ToggleOrientation();
                }
                // **Latched, so the fingers lifting is not also a tap** (issue
                // #1397). Finger 0's `Began` had already recorded
                // `_touchStart` and `_touchStartedAt` on the frame before
                // finger 1 arrived, so when both lift, `touchCount` passes
                // through 1 and finger 0's `Ended` arrives with a near-zero
                // displacement — the long-press branch if the gesture lasted
                // half a second, `RunTheScenesOwnSwitch` if it did not.
                // `DemoActivity.onTouchEvent` guards the same case on the
                // other host, and its comment says so.
                _multiTouch = true;
                return;
            }

            if (Input.touchCount != 1)
            {
                return;
            }

            var touch = Input.GetTouch(0);
            switch (touch.phase)
            {
                case TouchPhase.Began:
                    _touchStart = touch.position;
                    _touchStartedAt = Time.unscaledTime;
                    _touchDragged = false;
                    // A fresh single-finger gesture, so whatever the last
                    // two-finger one latched is spent.
                    _multiTouch = false;
                    break;
                case TouchPhase.Moved:
                    if (Mathf.Abs(touch.position.x - _touchStart.x)
                        > Mathf.Abs(touch.position.y - _touchStart.y))
                    {
                        // Horizontal is the signal's, and it is written while
                        // the finger moves rather than when it lifts.
                        _touchDragged = true;
                        DriveSignal(touch.position.x / Mathf.Max(1, Screen.width));
                    }
                    break;
                case TouchPhase.Ended:
                    var held = Time.unscaledTime - _touchStartedAt;
                    var dy = touch.position.y - _touchStart.y;
                    var dx = touch.position.x - _touchStart.x;
                    if (_multiTouch)
                    {
                        // The tail of a two-finger gesture, which has already
                        // done what it means. It stays latched until the next
                        // `Began`, because a two-finger lift can deliver more
                        // than one `Ended`.
                        break;
                    }
                    if (!_touchDragged && Mathf.Abs(dy) > SwipeMinPixels
                        && Mathf.Abs(dy) > Mathf.Abs(dx))
                    {
                        // Unity's y grows upward, so a swipe up is a positive
                        // dy — the opposite sign to the Android host's, which
                        // reads a MotionEvent whose y grows downward. Both bind
                        // "swipe up" to the next entry.
                        Show(dy > 0
                            ? (_index + 1) % TotalCount
                            : (_index + TotalCount - 1) % TotalCount);
                    }
                    else if (!_touchDragged && held >= LongPressSeconds)
                    {
                        _readoutVisible = !_readoutVisible;
                        Debug.Log($"[showcase] readout: {(_readoutVisible ? "shown" : "hidden")}");
                    }
                    else if (!_touchDragged)
                    {
                        RunTheScenesOwnSwitch();
                    }
                    break;
            }
        }

        /// Writes the showing scene's own scalar signal.
        ///
        /// A no-op on a committed `.dsb` document: `ds_demo_signal` addresses
        /// the scene the producer installed, and a loaded document has none.
        private void DriveSignal(float value)
        {
#if DASHSCENE_DEMO_PRODUCER
            if (_runtime == null || !IsScene(_index))
            {
                return;
            }
            try
            {
                _runtime.SetDemoSignal(value);
            }
            catch (DashsceneException e)
            {
                Fail($"the signal write failed: {e.Message}");
            }
#endif
        }

        /// Rotates the player between portrait and landscape.
        ///
        /// **This exists because a device will not rotate for a script**, and
        /// issue #1346's rotation case needs one that does. Measured on a Pixel
        /// 5 on 2026-08-29: neither `settings put system user_rotation` nor
        /// `wm user-rotation lock` moved this player. A Unity build that allows
        /// all four orientations carries a sensor-following `screenOrientation`
        /// in its own manifest, so the display rotation follows the
        /// accelerometer — and a handset lying on a desk reports portrait
        /// whatever the window manager is told. `mUserRotationMode` read
        /// `USER_ROTATION_FREE` and `mRotation=0` after both.
        ///
        /// **It is the same path a real rotation takes.** Assigning
        /// `Screen.orientation` calls `setRequestedOrientation` on the activity,
        /// which is an ordinary Android configuration change: the surface is
        /// destroyed and recreated and the drawable changes, which is exactly
        /// what `host-integration-in-three-layers.md` D4's first case is about.
        /// What it does not reproduce is the sensor path into that change.
        ///
        /// The binding is on the up arrow, so `adb shell input keyevent 19`
        /// drives it — the same way left and right already walk the entries.
        private void ToggleOrientation()
        {
            Screen.orientation = Screen.orientation == ScreenOrientation.LandscapeLeft
                ? ScreenOrientation.Portrait
                : ScreenOrientation.LandscapeLeft;
            Debug.Log($"[showcase] orientation: {Screen.orientation} requested");
        }

        /// Loads entry `index`, replacing whatever was loaded before.
        ///
        /// **A fresh runtime per document rather than a second load into the
        /// live one.** A document swap invalidates every rect index a caller
        /// cached, and the painter holds the atlases of the document it was
        /// last given; taking the runtime down is the shape with the fewest
        /// things left over, and a demonstration pays that cost once per key
        /// press.
        private void Show(int index)
        {
            _index = index;
            _sinceSwitch = 0.0f;
            _sincePulse = 0.0f;
            _phase = 0;
            _reported = false;
            var entry = IsScene(index) ? null : _entries[index - SceneCount];

            // **Refused before a runtime is minted**, because this is a
            // property of the manifest rather than of the load: the loader
            // that takes a font cascade takes no root ordinal (issue #1332),
            // so a non-zero root on a text entry cannot be honoured whatever
            // the runtime does.
            if (entry != null && entry.text && entry.shownRoot != 0)
            {
                Fail($"{entry.path} asks for root {entry.shownRoot} and carries text. "
                     + "The loader that takes a font cascade takes no root ordinal "
                     + "(issue #1332), so this entry cannot be shown as written.");
                return;
            }

            if (_runtime != null)
            {
                _runtime.Dispose();

                // **Read on every switch, because this component frees a
                // runtime more often than anything else here does.** `Dispose`
                // cannot throw, so a refused free is silent unless the status
                // is read — and it would leak one runtime per key press.
                ReportDisposeVerdict();
                _runtime = null;
            }

            try
            {
                _runtime = new DashsceneRuntime();
            }
            catch (DllNotFoundException)
            {
                Fail($"the native library '{DashsceneRuntime.LibraryName}' was not found. "
                     + "The package ships one for this platform and `just unity-demo` "
                     + "stages the demo producer over it; issue #1352 is the shipped "
                     + "plugin layout.");
                return;
            }
            catch (Exception e)
            {
                Fail($"the runtime could not be created: {e.Message}");
                return;
            }

            if (entry == null)
            {
                if (!BuildScene(index))
                {
                    return;
                }
            }
            else
            {
                try
                {
                    if (entry.text)
                    {
                        // The root was refused above, before this runtime existed.
                        _runtime.LoadDocumentWithText(ReadBytes(entry.path), Cascade());
                        _painter.SetAtlases(_runtime.ReadAtlases());
                    }
                    else
                    {
                        _runtime.LoadDocumentMapped(
                            StreamingAssetDocument.Resolve(entry.path), entry.shownRoot);
                    }
                }
                catch (Exception e)
                {
                    Fail($"could not load {entry.path}: {e.Message}");
                    return;
                }
            }

            FrameCamera();

            // **Cleared only when this entry is actually showing.** `Show`
            // raises two failures it does not return from: `ReportDisposeVerdict`
            // above, whose `Fail` is the one in this method with no `return`
            // after it, and a `Fail` raised earlier in the same frame by
            // `RunTheScenesOwnSwitch` before the `-cycle` timer reaches `Show`.
            // Either way the component is finished — `Update` returns at its
            // `_failed` guard — so an unconditional clear erased the only
            // description of why, which is what the readout exists for.
            //
            // Round three fixed this class in `Awake`; round four found these
            // two, the first of them older than this branch.
            if (!_failed)
            {
                _status = string.Empty;
            }
        }

        /// Builds showcase scene `index` into the runtime just minted, in place
        /// of a document load. Returns whether it worked.
        ///
        /// **The atlases are read back and handed to the painter, as the text
        /// document path does.** A scene's own solver carries a typesetter and
        /// its sheets — which a plain `LoadDocument` does not (issue #863;
        /// `LoadDocumentWithText` is the loader that does, and it is why the
        /// manifest marks one entry) — so
        /// every scene here can shade text, not only the one entry the manifest
        /// marks.
        ///
        /// **Sized from the screen rather than from a serialized field**, which
        /// is what the three Rust hosts do: a scene is built for a drawable, and
        /// `Screen` is this host's.
        private bool BuildScene(int index)
        {
#if DASHSCENE_DEMO_PRODUCER
            try
            {
                _builtWidth = (uint)Screen.width;
                _builtHeight = (uint)Screen.height;
                _runtime.BuildDemoScene(index, _builtWidth, _builtHeight);
                _painter.SetAtlases(_runtime.ReadAtlases());

                // **Phase 0 is applied here, not skipped.** `demo/src/shell.rs`
                // builds and then immediately calls `pulse(&mut live, phase)`,
                // so the Rust hosts show phases 0, 1, 2, 3. Without this the
                // first pulse increments to 1 and phase 0 is never seen — and
                // for `layout` the two differ, because its phase 0 sets the
                // spread and the topology explicitly rather than leaving the
                // scene at its built-in default.
                _runtime.PulseDemoScene(_phase);
                return true;
            }
            catch (DllNotFoundException)
            {
                Fail($"the native library '{DashsceneRuntime.LibraryName}' was not found.");
                return false;
            }
            catch (Exception e)
            {
                Fail($"could not build scene {index}: {e.Message}. The staged library "
                     + "exports ds_demo_* only when it is unity/demo-producer — "
                     + "`just unity-demo` builds and stages that one.");
                return false;
            }
#else
            Fail($"scene {index} was listed but this build carries no producer: "
                 + "DASHSCENE_DEMO_PRODUCER is undefined, so nothing here can build one.");
            return false;
#endif
        }

        private IReadOnlyList<TextFontFace> Cascade()
        {
            return new[]
            {
                new TextFontFace
                {
                    Family = fontFamily,
                    Weight = fontWeight,
                    FontBytes = ReadBytes(fontPath),
                    AtlasPng = ReadBytes(atlasPngPath),
                    AtlasMetrics = ReadBytes(atlasMetricsPath),
                },
            };
        }

        /// What the last `Dispose` reported, on screen as well as in the log.
        ///
        /// **Both properties, which is what `DashsceneRuntime` documents.**
        /// `LastDisposeStatus` alone is not a "was it freed" flag: a free that
        /// never reached the library leaves `Ok` beside a non-empty detail
        /// (issue #1308), and that is a runtime this component would otherwise
        /// leak once per key press.
        private void ReportDisposeVerdict()
        {
            if (_runtime.LastDisposeStatus == DsStatus.Ok
                && string.IsNullOrEmpty(_runtime.LastDisposeDetail))
            {
                return;
            }

            // Through `Fail` rather than `Debug.LogError` alone, so it reaches
            // the readout — which the comment on `Fail` argues is the only
            // place a person running the player learns anything.
            Fail($"the runtime was not freed cleanly: {_runtime.LastDisposeStatus}. "
                 + _runtime.LastDisposeDetail);
        }

        private static byte[] ReadBytes(string relative)
        {
            return File.ReadAllBytes(Path.Combine(Application.streamingAssetsPath, relative));
        }

        /// The text of a `StreamingAssets` file, on whichever platform this is.
        ///
        /// **`File` cannot read one on Android**, and the failure is silent in
        /// the worst way: `Application.streamingAssetsPath` is
        /// `jar:file:///data/app/<pkg>/base.apk!/assets`, so `File.Exists`
        /// answers false for a file that is present and the reader concludes it
        /// was never staged. Measured on a Pixel 5 on 2026-08-29: the player
        /// reported the manifest missing, `Awake` ended, and the SCENES — which
        /// need no manifest at all — never loaded either.
        ///
        /// **It goes through `StreamingAssetDocument.Resolve`** rather than
        /// carrying a second answer to the same question. That resolver asks
        /// the APK's own `AssetManager` where the entry is and hands back a
        /// container path with a byte range, which is what the mapped document
        /// loader already uses — so this reads the asset where it is packed,
        /// and there is one place that knows how an APK stores one.
        ///
        /// A manifest is a few hundred bytes, so reading it is not the cost
        /// `Resolve` exists to avoid for a document; the point here is that it
        /// is the same LOOKUP.
        private static string ReadStreamingAssetText(string relative)
        {
            var range = StreamingAssetDocument.Resolve(relative);
            if (range.IsWholeFile)
            {
                return File.ReadAllText(range.ContainerPath);
            }

            using var stream = new FileStream(
                range.ContainerPath, FileMode.Open, FileAccess.Read);
            stream.Seek((long)range.Offset, SeekOrigin.Begin);
            var bytes = new byte[range.Length];
            var read = 0;
            // **Looped, because one `Read` is not obliged to fill the buffer.**
            while (read < bytes.Length)
            {
                var got = stream.Read(bytes, read, bytes.Length - read);
                if (got <= 0)
                {
                    break;
                }

                read += got;
            }

            // **A short read is refused, not returned.** Breaking out of the
            // loop and handing back the partial bytes is exactly the outcome
            // the loop exists to prevent: `JsonUtility` would then report a
            // malformed manifest where the manifest is fine and the READ was
            // partial — and a truncation landing after a syntactically complete
            // prefix parses, silently, with a short document list.
            if (read != bytes.Length)
            {
                throw new IOException(
                    $"{relative}: read {read} of {bytes.Length} byte(s) from "
                    + $"{range.ContainerPath} at offset {range.Offset}. The entry is "
                    + "shorter than the asset manager reported, so the content is "
                    + "partial rather than malformed.");
            }

            return Encoding.UTF8.GetString(bytes, 0, read);
        }

        private void LoadManifest()
        {
            string text;
            try
            {
                text = ReadStreamingAssetText(manifestPath);
            }
            catch (Exception e)
            {
                // Said here rather than collapsed into "lists none" below: a
                // manifest that is absent and one that is empty are different
                // mistakes, and the reader fixes them differently.
                Fail($"{manifestPath} could not be read: {e.GetType().Name}: {e.Message}. "
                     + "The recipe writes it beside the documents; a player built by hand "
                     + "needs it written by hand.");
                return;
            }

            ShowcaseManifest manifest;
            try
            {
                manifest = JsonUtility.FromJson<ShowcaseManifest>(text);
            }
            catch (Exception e)
            {
                // `JsonUtility` raises on malformed JSON, and an uncaught raise
                // here left the component enabled with no entries and no reason
                // on screen.
                Fail($"{manifestPath} does not parse: {e.Message}");
                return;
            }

            if (manifest?.documents == null)
            {
                return;
            }

            foreach (var entry in manifest.documents)
            {
                if (!string.IsNullOrEmpty(entry?.path))
                {
                    _entries.Add(entry);
                }
            }
        }

        /// Frames the camera on the entry that is showing.
        ///
        /// **The two entry classes want different framing, and the camera only
        /// ever had one** — which is the defect a person running this found and
        /// no gate could. `just unity-demo cycle` asserts that every entry
        /// reached the painter and says out loud that it asserts nothing about
        /// pixels, so a scene drawn at twice its size passed every check.
        ///
        /// - **A scene** is built for the drawable in physical pixels, as the
        ///   three Rust hosts build it, so its extent is the window's. Framing
        ///   it means one document unit per pixel: half the built height, centred
        ///   on the built rectangle. That is what `demo` shows, and what makes
        ///   the two comparable.
        /// - **A document** carries a fixed extent of a few hundred units, and
        ///   `unity/demo/DemoBuild.cs` chose a size that frames the committed
        ///   ones. That framing is restored rather than recomputed, because
        ///   nothing on this ABI reports a document's size — the same reason
        ///   `BrgPainter.GlobalBounds` is left at its default.
        ///
        /// **From the built size, not from `Screen`.** A window resized after a
        /// scene was built leaves the scene the size it was built at; framing
        /// the current screen would then crop or shrink it. The Rust hosts
        /// rebuild on resize instead, which this sample does not do — issue
        /// #1329's host is where that belongs.
        private void FrameCamera()
        {
            var camera = viewCamera != null ? viewCamera : Camera.main;
            if (camera == null || !camera.orthographic)
            {
                return;
            }

            var z = camera.transform.position.z;
            if (IsScene(_index) && _builtHeight > 0)
            {
                camera.orthographicSize = _builtHeight / 2.0f;
                camera.transform.position =
                    new Vector3(_builtWidth / 2.0f, -(_builtHeight / 2.0f), z);
            }
            else
            {
                camera.orthographicSize = _documentSize;
                camera.transform.position = new Vector3(
                    _documentCameraPosition.x, _documentCameraPosition.y, z);
            }
        }

        private void UpdateEdgeWidth()
        {
            var camera = viewCamera != null ? viewCamera : Camera.main;
            if (camera != null && camera.orthographic && camera.pixelHeight > 0)
            {
                // World units per device pixel under the placement above. A
                // perspective camera cannot express it as one scalar, so the
                // painter keeps its own default there.
                _painter.EdgeWidth = camera.orthographicSize * 2f / camera.pixelHeight;
            }
        }

        /// The one line a run reads: every manifest entry reached the painter.
        ///
        /// **This is what makes `-cycle` more than a convenience.** Without it
        /// a cycling player is still answered by a person watching it, which
        /// the field's own comment says is the shape a run cannot report.
        private void AnnounceIfEveryEntryHasDrawn()
        {
            if (_announcedEveryEntry || _drawnEntries.Count < TotalCount)
            {
                return;
            }

            _announcedEveryEntry = true;
            Debug.Log($"[showcase] all {TotalCount} entries drew "
                      + $"({SceneCount} scene(s), {_entries.Count} document(s))");

            if (_quitWhenEveryEntryHasDrawn)
            {
                Application.Quit(0);
            }
        }

        /// The command line's `-cycle <seconds>`, or zero.
        private static float CycleSecondsFromCommandLine()
        {
            var args = Environment.GetCommandLineArgs();
            for (var i = 0; i < args.Length - 1; i++)
            {
                // **`InvariantCulture`, as everything else in this
                // repository's C# does.** Under a comma-decimal locale the
                // ambient culture reads `-cycle 2.5` as 25.
                if (args[i] == "-cycle"
                    && float.TryParse(
                        args[i + 1],
                        NumberStyles.Float,
                        CultureInfo.InvariantCulture,
                        out var seconds)
                    && seconds > 0.0f)
                {
                    return seconds;
                }
            }
            return 0.0f;
        }

        /// What entry `index` is called on screen and in the log.
        private string Label(int index)
        {
            if (IsScene(index))
            {
                return $"scene {_sceneNames[index]}";
            }

            var entry = index - SceneCount < _entries.Count ? _entries[index - SceneCount] : null;
            return entry == null
                ? "no document"
                : string.IsNullOrEmpty(entry.label)
                    ? Path.GetFileName(entry.path)
                    : entry.label;
        }

        /// The one line describing what the showing entry is meant to show.
        ///
        /// For a scene it is `showcase::Showcase::summary`, read from the
        /// library — so a viewer can hold what the scene claims to draw against
        /// the refusals printed under it, which is the comparison story #1342
        /// asks this sample to make visible.
        private string Summary(int index)
        {
            return IsScene(index) ? _sceneSummaries[index] : string.Empty;
        }

        private void OnGUI()
        {
            if (!_readoutVisible)
            {
                return;
            }

            var label = TotalCount > 0 ? Label(_index) : "nothing to show";
            var summary = TotalCount > 0 ? Summary(_index) : string.Empty;

            // **Sized from the surface rather than left at the default.** A
            // player on a high-density display reports its height in pixels —
            // 1832 on the machine this was written for — and `GUI.skin.label`'s
            // default is a fixed point size, so the readout is legible in the
            // editor and unreadable in the player. Measured: the first build of
            // this file was unreadable at 1280x800 on an Apple M3.
            // **Rebuilt when the surface changes, not cached once.**
            // `DemoBuild` builds a resizable window, so a style sized on the
            // first frame keeps its point size across a resize — the failure
            // this sizing exists to prevent.
            if (_readout == null || _readoutHeight != Screen.height)
            {
                _readoutHeight = Screen.height;
                _readout = new GUIStyle(GUI.skin.label)
                {
                    fontSize = Mathf.Max(14, Screen.height / 40),
                    wordWrap = false,
                };
            }

            var lines = new List<string>
            {
                $"{label}   [{_index + 1}/{Math.Max(TotalCount, 1)}]   "
                + "page up/down or swipe to switch, left/right or drag for the "
                + "signal, space or tap for a scene's variant switch, up or two "
                + "fingers to rotate, R or long press for this readout",
                $"rung {_painter?.Rung.ToString() ?? "none"}   "
                + $"{_painter?.InstanceCount ?? 0} instances",
            };

            if (!string.IsNullOrEmpty(summary))
            {
                lines.Add(summary);
            }

            // What the painter was handed and did not draw. P4 is why this is
            // reported rather than dropped.
            if (_painter != null && !_painter.Diagnostics.IsClean)
            {
                lines.AddRange(_painter.Diagnostics.Describe());
            }

            if (!string.IsNullOrEmpty(_status))
            {
                lines.Add(_status);
            }

            GUI.Label(new Rect(16, 16, Screen.width - 32, Screen.height - 32),
                string.Join("\n", lines), _readout);
        }

        private void Fail(string message)
        {
            _status = message;
            Debug.LogError($"[dashscene] {message}", this);

            // **Not `enabled = false`, which an earlier version did.** Unity
            // delivers `OnGUI` only to an enabled behaviour, so disabling here
            // took the readout down together with the frame loop — and the
            // readout is the only place a person running the player learns why
            // it stopped. `_failed` stops the loop and leaves the report up.
            _failed = true;
        }

        private void OnDestroy()
        {
            // The painter first: it holds the group and the buffers the runtime
            // knows nothing about, and the runtime owns the frame it was drawing.
            _painter?.Dispose();
            _painter = null;

            if (_runtime != null)
            {
                _runtime.Dispose();
                ReportDisposeVerdict();
                _runtime = null;
            }
        }
    }
}
