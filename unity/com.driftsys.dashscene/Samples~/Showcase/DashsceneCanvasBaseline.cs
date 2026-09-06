// One player, two renderers and a floor: the painter beside a faithful uGUI
// Canvas over the same document, on one key.
//
// Story #1444, D3 of
// `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`.
//
// **Why this component draws through the painter as well as the Canvas.** D3's
// own argument is that a comparison built on two harnesses is between the
// harnesses rather than between the renderers — the trap issue #1347's body
// names. So the entry walk, the pacer, the settle decision, the capture and the
// `drew` line are written once here and the renderer is the only term that
// changes. `DashsceneShowcase` remains the demonstration and is untouched; this
// component is inert until `-renderer` is passed, and takes the scene over when
// it is.
//
// **`-renderer none` is D3's floor**: the camera, the clear and nothing else. Its
// readings are what "above the empty-scene floor" is measured against, and it is
// the reason this file destroys the showcase rather than disabling it — a
// disabled `DashsceneShowcase` has already constructed a `BrgPainter`, and a
// registered `BatchRendererGroup` keeps emitting draw commands whatever the
// component that made it is doing.
//
// **`DestroyImmediate`, and it is not interchangeable with `Destroy`.** A
// deferred `Destroy` runs at the end of the frame, so the showcase's own
// `Update` would run once first: it would acquire a frame, pack it, draw it and
// write a `drew` line into the log this run's assertions read.
//
// **Attached at `AfterSceneLoad` rather than placed in the scene.**
// `unity/demo/DemoBuild.cs` builds the scene with one `DashsceneShowcase` on one
// object and belongs to another lane, so this component adds itself to that
// object instead. The consequence is that the showcase's `Awake` HAS already run
// when this one does — it has read the manifest and written the census — which
// is why the census below is written only when this component takes over.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using Driftsys.Dashscene;
using Unity.Profiling;
using UnityEngine;
using UnityEngine.Rendering;
using Stopwatch = System.Diagnostics.Stopwatch;

namespace Driftsys.Dashscene.Samples
{
    [DefaultExecutionOrder(-100)]
    [DisallowMultipleComponent]
    public sealed class DashsceneCanvasBaseline : MonoBehaviour
    {
        /// Which renderer draws the entries.
        public enum Renderer
        {
            /// The Unity painter, through `BrgPainter`.
            Painter,

            /// A faithful uGUI Canvas, through `CanvasScene`.
            Canvas,

            /// Neither: the camera, the clear and nothing else. D3's floor.
            None,
        }

        /// The renderer `-renderer painter|canvas|none` names.
        ///
        /// **An unrecognised value is the painter**, which is the value a run
        /// with no argument at all takes: this parses a switch rather than
        /// validating one, and [`Requested`] is what says whether the switch was
        /// there.
        public static Renderer FromArguments(string[] args)
        {
            for (var i = 0; i + 1 < args.Length; i++)
            {
                if (args[i] == "-renderer")
                {
                    return args[i + 1] == "canvas"
                        ? Renderer.Canvas
                        : args[i + 1] == "none"
                            ? Renderer.None
                            : Renderer.Painter;
                }
            }
            return Renderer.Painter;
        }

        /// Whether this launch asked for a renderer at all.
        ///
        /// Without it the showcase runs as it always has, which is what keeps
        /// `just unity-demo run` a demonstration rather than a harness.
        public static bool Requested(string[] args)
        {
            return Array.IndexOf(args, "-renderer") >= 0;
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void Attach()
        {
            if (!Requested(Environment.GetCommandLineArgs()))
            {
                return;
            }

            var showcase = FindAnyObjectByType<DashsceneShowcase>();
            if (showcase == null)
            {
                Debug.LogError(
                    "[dashscene] -renderer was passed and this scene carries no "
                    + "DashsceneShowcase to take over from. unity/demo/DemoBuild.cs is what "
                    + "puts one there.");
                return;
            }

            showcase.gameObject.AddComponent<DashsceneCanvasBaseline>();
        }

        private const float PulseSeconds = 2.5f;

        /// The dt every tick is driven with under `-judge`.
        ///
        /// **`Time.captureDeltaTime`, not a constant passed to `Tick`.** A spring
        /// in flight is a function of the time that has passed, so two launches
        /// at different frame rates photograph two different pictures — and the
        /// comparison would then measure the machine's load. Setting the capture
        /// delta makes `Time.deltaTime` exactly this on every frame of both runs.
        private const float JudgeDelta = 1.0f / 60.0f;

        /// Frames each entry is held for under `-judge`, before it is
        /// photographed.
        ///
        /// 1.5 seconds of scene time at [`JudgeDelta`]: long enough for the
        /// scripted pulse's spring to settle, and short of the 2.5 s at which the
        /// next pulse would fire and put the two runs in different phases.
        private const int JudgeFrames = 90;

        private Renderer _renderer;
        private DashsceneRuntime _runtime;
        private BrgPainter _painter;
        private CanvasScene _scene;
        private CanvasSprites _sprites;
        private Canvas _canvas;
        private Camera _camera;
        private CommitPacer _pacer;
        private readonly DashsceneFrameCost _frameCost = new DashsceneFrameCost();

        private readonly List<ShowcaseEntry> _entries = new List<ShowcaseEntry>();
        private string[] _sceneNames = Array.Empty<string>();
        private int _index;
        private ulong _phase;
        private float _sincePulse;
        private float _sinceSwitch;
        private float _cycleSeconds;
        private bool _reported;
        private bool _failed;
        private bool _announced;
        private readonly HashSet<int> _drawnEntries = new HashSet<int>();
        private bool _quitWhenEveryEntryHasDrawn;

        private uint _builtWidth;
        private uint _builtHeight;
        private float _documentSize;
        private Vector3 _documentCameraPosition;

        private string _judgeDirectory;
        private int _framesOnEntry;
        private RenderTexture _target;

        /// The three run-time pins the judged run carries beside the pictures.
        ///
        /// A capture answers "does it draw the same thing"; these answer the
        /// three things a picture cannot show — that the Canvas touched only the
        /// dirty rows, that it rebuilt nothing at rest (rule 8), and that a
        /// settled frame allocates nothing.
        private ProfilerRecorder _batchBuilds;
        private int _skippedFrames;
        private int _rebuiltAtRest;
        private long _allocatedAtSettle = -1;
        private long _allocatedDelta;
        private bool _noFrameCost;

        private int SceneCount => _sceneNames.Length;

        private int TotalCount => SceneCount + _entries.Count;

        private bool IsScene(int index) => index < SceneCount;

        private void Awake()
        {
            var args = Environment.GetCommandLineArgs();
            _renderer = FromArguments(args);

            // Before anything of this component's own runs, and immediate: the
            // file header says what a deferred destroy would let through.
            DestroyImmediate(GetComponent<DashsceneShowcase>());

            _camera = Camera.main;
            if (_camera == null)
            {
                Fail("there is no camera tagged MainCamera in this scene.");
                return;
            }
            _documentSize = _camera.orthographicSize;
            _documentCameraPosition = _camera.transform.position;

            _pacer = new CommitPacer(0);
            _cycleSeconds = FloatAfter(args, "-cycle");
            _quitWhenEveryEntryHasDrawn = Array.IndexOf(args, "-quit") >= 0;
            // `DashsceneFrameCost.Push` builds a key string per push by design —
            // `entry + "@" + width + "x" + height` — and the instrument is not the
            // renderer under test, so the allocation pin is read in a run that
            // disarms it. It is the one instrument this component carries;
            // `DashsceneThreadCost` is story #1443's and is not wired here, which
            // the pull request records as a gap rather than a choice.
            _noFrameCost = Array.IndexOf(args, "-no-frame-cost") >= 0;
            _judgeDirectory = ArgumentAfter(args, "-judge");
            if (_judgeDirectory != null)
            {
                // Rule 8's own pin. `Canvas.BuildBatch` is the marker a uGUI
                // batch rebuild raises, and D3 names it beside
                // `Canvas.SendWillRenderCanvases` as what the thread-time
                // instrument watches; this reads it locally, because the pin is
                // a property of THIS run rather than a reading to report.
                _batchBuilds = ProfilerRecorder.StartNew(
                    ProfilerCategory.Render, "Canvas.BuildBatch");
                Time.captureDeltaTime = JudgeDelta;
                _judgeDirectory = Path.Combine(_judgeDirectory, Name(_renderer));
                Directory.CreateDirectory(_judgeDirectory);
                PrepareRenderTarget();
            }

            if (!ReadSceneTable() || !LoadManifest())
            {
                return;
            }

            if (TotalCount == 0)
            {
                Fail("nothing to show: the manifest lists no document and this build "
                     + "carries no showcase scene either.");
                return;
            }

            // The census `just unity-demo`'s `cycle` action reads to learn how
            // many entries to wait for. Written by whichever component is
            // driving, which under `-renderer` is this one.
            Debug.Log($"[showcase] entries: {TotalCount} ({SceneCount} scene(s), "
                      + $"{_entries.Count} document(s))");
            Debug.Log($"[showcase] renderer: {Name(_renderer)}");
            Debug.Log($"[showcase] graphics: {SystemInfo.graphicsDeviceType}, "
                      + $"{SystemInfo.graphicsDeviceName}");

            Show(0);
        }

        private void Update()
        {
            if (_failed || TotalCount == 0)
            {
                return;
            }

            ReadInput();
            AdvanceOnItsOwn();
            PulseIfShowingAScene();

            // **The floor reports a drawn entry, having drawn nothing.** The
            // `cycle` action's bound is the player's own "all N entries drew"
            // line, and `-renderer none` is one of the three renderers issue
            // #1444 asks that action to pass under — so an entry it showed and
            // deliberately did not draw is still an entry it reached.
            if (_renderer == Renderer.None)
            {
                if (!_reported)
                {
                    _reported = true;
                    _drawnEntries.Add(_index);
                    Debug.Log($"[showcase] drew {Label(_index)}: the empty floor");
                    AnnounceIfEveryEntryHasDrawn();
                }
                return;
            }

            if (_failed || _runtime == null
                || !_pacer.ShouldCommit(Time.deltaTime, out var dt))
            {
                return;
            }

            try
            {
                var tickStart = Stopwatch.GetTimestamp();
                var advanced = _runtime.Tick(dt);
                var tickTicks = Stopwatch.GetTimestamp() - tickStart;

                // The settle decision, written once for both renderers. Task 4's
                // `SettleLoop` is what this becomes when that story lands; until
                // then the skip is inline and the two loops must agree, which is
                // why it is one expression here rather than one per renderer.
                if (!advanced && _reported)
                {
                    _skippedFrames++;

                    // **Rule 8, read at rest.** A settled Canvas rebuilds
                    // nothing: every element the pulses move is on its own child
                    // canvas by now, and nothing is moving. A non-zero count here
                    // is a Canvas being given work the record says it is not
                    // given, which would make its CPU reading a measurement of
                    // this file's mistake.
                    if (_renderer == Renderer.Canvas && _batchBuilds.Valid
                        && _batchBuilds.LastValue > 0)
                    {
                        _rebuiltAtRest++;
                    }

                    // **Allocation, over settled frames only.** The first
                    // settled frame records the total and each one after it
                    // takes the difference, so a steady frame that allocates is
                    // visible as a non-zero delta rather than as a number that
                    // has to be interpreted.
                    var allocated = GC.GetAllocatedBytesForCurrentThread();
                    if (_allocatedAtSettle < 0)
                    {
                        _allocatedAtSettle = allocated;
                    }
                    else
                    {
                        _allocatedDelta = allocated - _allocatedAtSettle;
                    }
                    return;
                }

                var first = !_reported;
                var drawStart = Stopwatch.GetTimestamp();
                using (var frame = _runtime.AcquireFrame())
                {
                    if (_renderer == Renderer.Painter)
                    {
                        _painter.Draw(frame);
                    }
                    else
                    {
                        _scene.Apply(frame.Frame);

                        // **Rule 5's own pin.** The scan in `canvas_baseline.rs`
                        // holds `Apply` to one loop over the dirty set; this
                        // holds the loop to having actually walked it. A scan
                        // cannot see a loop that runs zero times, and a Canvas
                        // that applied nothing would draw the right picture for
                        // every frame after the first and cost nothing to do it.
                        var dirty = frame.Frame.Dirty.CountAsLong;
                        if (_scene.AppliedLastFrame != dirty)
                        {
                            Fail($"the canvas applied {_scene.AppliedLastFrame} row(s) "
                                 + $"and the frame's dirty set holds {dirty}.");
                        }
                    }

                    frame.MarkDrawn();
                }
                var drawTicks = Stopwatch.GetTimestamp() - drawStart;

                if (first)
                {
                    _reported = true;
                    _drawnEntries.Add(_index);
                    ReportDrew();
                    AnnounceIfEveryEntryHasDrawn();
                }

                if (_noFrameCost)
                {
                    return;
                }

                var cost = _frameCost.Push(
                    Label(_index), Screen.width, Screen.height, tickTicks, drawTicks);
                if (cost != null)
                {
                    Debug.Log($"[showcase] frame cost — {cost.Line()}");
                }
            }
            catch (DashsceneException e)
            {
                Fail($"frame failed: {e.Message}");
            }
            catch (Exception e)
            {
                // **Broader than the showcase's four catches, and deliberately.**
                // `CanvasScene.Apply` indexes the paint tables by the indices a
                // rect names, where `FramePacker.PackRect` bounds-checks each one
                // and skips the rect. A span index throws rather than reading
                // native memory, so a corrupt row is memory-safe here — but the
                // exception is not a `DashsceneException`, and without this it
                // would escape before `MarkDrawn`, leaving the same dirty rows to
                // throw again on every frame with nothing reported and no run
                // ever ending.
                Fail($"the frame threw: {e.GetType().Name}: {e.Message}");
            }
        }

        /// The photograph, after everything else has had its frame.
        ///
        /// **`LateUpdate`, so the painter's own `Update` has packed and uploaded
        /// before the camera renders.** The capture family is
        /// `unity/render-gate`'s — the camera disabled and rendered on demand
        /// into a `RenderTexture` this object owns, read back with `ReadPixels`.
        /// `ScreenCapture.CaptureScreenshotAsTexture` returned once and then hung
        /// on macOS/Metal, which that gate's header records as a measurement.
        private void LateUpdate()
        {
            if (_judgeDirectory == null || _failed || TotalCount == 0)
            {
                return;
            }

            RenderToTarget();
            _framesOnEntry++;
            if (_framesOnEntry < JudgeFrames)
            {
                return;
            }

            Capture(_index);
            JudgeThePins();
            if (_index + 1 >= TotalCount)
            {
                Debug.Log($"[showcase] judged {TotalCount} entries into {_judgeDirectory}");
                Application.Quit(0);
                enabled = false;
                return;
            }
            Show(_index + 1);
        }

        /// Reads the three settled-frame pins, reports them, and fails the run on
        /// the two that are verdicts.
        ///
        /// **They were computed and thrown away, which is worse than not
        /// computing them.** The fields' own remarks called them "the three
        /// run-time pins the judged run carries beside the pictures", and D2's
        /// rule 8 says the batch-build marker is "pinned at run time" — while
        /// nothing read either. A judged run in which the Canvas rebuilt a batch
        /// on every settled frame wrote its seven pictures and exited 0, so the
        /// two of D3's conditions a photograph cannot show were unverified and
        /// reported as met.
        ///
        /// The skip count is reported rather than enforced: it is the rest
        /// signal, and an entry whose scene never settles inside the judged
        /// window is a scene fact rather than a Canvas defect.
        private void JudgeThePins()
        {
            if (_renderer != Renderer.Canvas)
            {
                return;
            }

            Debug.Log($"[showcase] pins {Label(_index)}: {_skippedFrames} settled frame(s), "
                      + $"{_rebuiltAtRest} rebuild(s) at rest, {_allocatedDelta} byte(s) "
                      + "allocated while settled");

            if (_rebuiltAtRest > 0)
            {
                Fail($"the canvas rebuilt a batch on {_rebuiltAtRest} settled frame(s) of "
                     + $"{Label(_index)}. Rule 8 isolates a moving rect onto its own child "
                     + "canvas so a rebuild covers the moving elements and not the scene, "
                     + "and at rest nothing is moving — so this is the Canvas being given "
                     + "work the record says it is not given.");
            }

            if (_allocatedDelta != 0 && _noFrameCost)
            {
                Fail($"a settled frame of {Label(_index)} allocated {_allocatedDelta} "
                     + "managed byte(s) on the main thread. D3 makes allocation part of "
                     + "CPU and asks a steady frame for zero.");
            }
        }

        private void ReportDrew()
        {
            var refused = _renderer == Renderer.Canvas
                ? _scene.Diagnostics
                : _painter.Diagnostics;
            var tail = refused.IsClean ? string.Empty : $", refused {refused}";

            if (_renderer == Renderer.Canvas)
            {
                Debug.Log($"[showcase] drew {Label(_index)}: {_scene.ElementCount} "
                          + $"element(s) through the canvas, {_sprites.Count} sprite(s), "
                          + $"{_scene.IsolatedCount} isolated, {_scene.TextRefused} run(s) "
                          + $"without text{tail}");
            }
            else
            {
                Debug.Log($"[showcase] drew {Label(_index)}: {_painter.InstanceCount} "
                          + $"instance(s), rung {_painter.Rung}{tail}");
            }
        }

        private void ReadInput()
        {
            if (Input.GetKeyDown(KeyCode.C))
            {
                var next = _renderer == Renderer.Painter
                    ? Renderer.Canvas
                    : _renderer == Renderer.Canvas ? Renderer.None : Renderer.Painter;
                Debug.Log($"[showcase] renderer: {Name(next)}");
                _renderer = next;
                Show(_index);
            }
            else if (Input.GetKeyDown(KeyCode.PageDown))
            {
                Show((_index + 1) % TotalCount);
            }
            else if (Input.GetKeyDown(KeyCode.PageUp))
            {
                Show((_index + TotalCount - 1) % TotalCount);
            }
        }

        /// The `-cycle` timer, which is how a run with no keyboard walks the
        /// entries. Off under `-judge`, which walks them on its own frame count.
        private void AdvanceOnItsOwn()
        {
            if (_judgeDirectory != null || _cycleSeconds <= 0.0f || TotalCount <= 1)
            {
                return;
            }

            _sinceSwitch += Time.deltaTime;
            if (_sinceSwitch >= _cycleSeconds)
            {
                Show((_index + 1) % TotalCount);
            }
        }

        private void PulseIfShowingAScene()
        {
#if DASHSCENE_DEMO_PRODUCER
            if (!IsScene(_index) || _runtime == null || _failed)
            {
                return;
            }

            _sincePulse += Time.deltaTime;
            if (_sincePulse < PulseSeconds)
            {
                return;
            }

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

        /// Loads entry `index` and builds whatever the showing renderer draws it
        /// with.
        private void Show(int index)
        {
            _index = index;
            _sinceSwitch = 0.0f;
            _sincePulse = 0.0f;
            _phase = 0;
            _reported = false;
            _framesOnEntry = 0;

            // Per entry, because the verdict is per entry: without this the
            // first entry that rebuilt at rest would fail every entry after it,
            // and an entry that settled cleanly would inherit a pass it did not
            // earn once the counters were read.
            _skippedFrames = 0;
            _rebuiltAtRest = 0;
            _allocatedDelta = 0;
            _allocatedAtSettle = -1;

            TearDownScene();

            if (_renderer == Renderer.None)
            {
                return;
            }

            var entry = IsScene(index) ? null : _entries[index - SceneCount];
            if (entry != null && entry.text && entry.shownRoot != 0)
            {
                Fail($"{entry.path} asks for root {entry.shownRoot} and carries text, which "
                     + "the loader that takes a font cascade cannot honour (issue #1332).");
                return;
            }

            try
            {
                _runtime = new DashsceneRuntime();
            }
            catch (Exception e)
            {
                Fail($"the runtime could not be created: {e.Message}");
                return;
            }

            if (_renderer == Renderer.Painter)
            {
                try
                {
                    _painter = new BrgPainter(MaterialClass.UnlitOverlay);
                }
                catch (DashscenePainterException e)
                {
                    Fail($"the painter could not be created: {e.Message}");
                    return;
                }
                _painter.DocumentToWorld = Matrix4x4.Scale(new Vector3(1, -1, 1));
            }

            if (!LoadEntry(entry, index))
            {
                return;
            }

            FrameCamera();
            UpdateEdgeWidth();

            if (_renderer == Renderer.Canvas && !BuildCanvas())
            {
                return;
            }
        }

        private bool LoadEntry(ShowcaseEntry entry, int index)
        {
            try
            {
                if (entry == null)
                {
#if DASHSCENE_DEMO_PRODUCER
                    _builtWidth = (uint)Screen.width;
                    _builtHeight = (uint)Screen.height;
                    _runtime.BuildDemoScene(index, _builtWidth, _builtHeight);
                    _runtime.PulseDemoScene(_phase);
#else
                    Fail($"scene {index} was listed and this build carries no producer.");
                    return false;
#endif
                }
                else if (entry.text)
                {
                    _runtime.LoadDocumentWithText(ReadBytes(entry.path), Cascade());
                }
                else
                {
                    _runtime.LoadDocumentMapped(
                        StreamingAssetDocument.Resolve(entry.path), entry.shownRoot);
                }

                if (_painter != null)
                {
                    _painter.SetAtlases(_runtime.ReadAtlases());
                }
                return true;
            }
            catch (Exception e)
            {
                Fail($"could not load entry {index}: {e.Message}");
                return false;
            }
        }

        /// The Canvas, and the hierarchy under it.
        ///
        /// `ScreenSpaceCamera` at `planeDistance 1`, so the Canvas is inside the
        /// camera's frustum and `RenderPipeline.SubmitRenderRequest` draws it —
        /// a `ScreenSpaceOverlay` canvas is composited outside the camera's
        /// render and would be absent from every capture.
        private bool BuildCanvas()
        {
            try
            {
                var host = new GameObject("DashsceneCanvas", typeof(RectTransform), typeof(Canvas));
                _canvas = host.GetComponent<Canvas>();
                _canvas.renderMode = RenderMode.ScreenSpaceCamera;
                _canvas.worldCamera = _camera;
                _canvas.planeDistance = 1.0f;

                // **One sprite texel is one canvas unit, and this is what makes
                // that true.** A sliced `Image` scales its sprite's border by
                // `referencePixelsPerUnit / sprite.pixelsPerUnit`, which at
                // Unity's default 100 against `CanvasSprites`' 1 draws every
                // 9-slice corner a hundred times too large — the corner slices
                // then cover the whole element and every rect renders as a
                // blurred ellipse. Measured on the first judged run with text:
                // the geometry and the colours were exactly right and every
                // shape was a blob.
                _canvas.referencePixelsPerUnit = 1.0f;

                _sprites = new CanvasSprites();
#if DASHSCENE_DEMO_PRODUCER
                // Both halves come from `ds_demo_*`, the library a customer does
                // not install: boundary B carries shaped glyph ids and neither
                // the string nor the face behind them. A document entry answers
                // the empty string here — the producer reaches an arena through
                // the scene it installed — and `CanvasText` counts each run it
                // could not draw.
                var text = new CanvasText(DemoScenes.FaceKey, _runtime.DemoRunText);
#else
                var text = new CanvasText(null, null);
#endif
                // **Forced before the height is read.** Unity sizes a
                // screen-space canvas's own RectTransform during the canvas
                // update, so `rect.height` on a canvas created two statements
                // ago can still be 0 — and a zero height is a zero scale, which
                // draws nothing while every count reads right.
                Canvas.ForceUpdateCanvases();
                var canvasHeight = ((RectTransform)host.transform).rect.height;
                if (canvasHeight <= 0.0f)
                {
                    canvasHeight = Screen.height;
                }

                using (var lease = _runtime.AcquireFrame())
                {
                    var map = new DocumentToCanvas(_camera, canvasHeight);
                    _scene = CanvasScene.Build(
                        lease.Frame, host.transform, _sprites, map, _runtime.ReadAtlases());
                }

                // **After the lease, and that is not tidiness.** Every call on
                // the producer's demonstration seam is refused while a frame
                // lease is outstanding, because the views the lease handed out
                // would stop being valid under it — so a `PlaceText` inside the
                // `using` above answers the empty string for every run and the
                // Canvas draws no text at all, with nothing reported. Measured.
                _scene.PlaceText(text);
                return true;
            }
            catch (Exception e)
            {
                Fail($"the canvas could not be built: {e.GetType().Name}: {e.Message}");
                return false;
            }
        }

        /// What the last `Dispose` reported, which is silent unless it is read.
        ///
        /// `DashsceneRuntime.Dispose` cannot throw, so a refused free leaves an
        /// `Ok` status beside a non-empty detail (issue #1308) and leaks one
        /// runtime per entry switch. `DashsceneShowcase` reads both on every
        /// switch for this reason, and this component switches at least as
        /// often — once per judged entry, and again on every `C` key.
        private void ReportDisposeVerdict(DashsceneRuntime runtime)
        {
            if (runtime.LastDisposeStatus == DsStatus.Ok
                && string.IsNullOrEmpty(runtime.LastDisposeDetail))
            {
                return;
            }

            Fail($"the runtime was not freed cleanly: {runtime.LastDisposeStatus}. "
                 + runtime.LastDisposeDetail);
        }

        private void TearDownScene()
        {
            _scene?.Dispose();
            _scene = null;
            _sprites?.Dispose();
            _sprites = null;

            if (_canvas != null)
            {
                Destroy(_canvas.gameObject);
                _canvas = null;
            }

            _painter?.Dispose();
            _painter = null;

            if (_runtime != null)
            {
                _runtime.Dispose();
                ReportDisposeVerdict(_runtime);
                _runtime = null;
            }
        }

        private void PrepareRenderTarget()
        {
            _target = new RenderTexture(Screen.width, Screen.height, 24, RenderTextureFormat.ARGB32)
            {
                name = "DashsceneCanvasBaseline",
            };
            _camera.enabled = false;
        }

        private void RenderToTarget()
        {
            var request = new RenderPipeline.StandardRequest
            {
                destination = _target,
                mipLevel = 0,
                slice = 0,
                face = CubemapFace.Unknown,
            };

            if (!RenderPipeline.SupportsRenderRequest(_camera, request))
            {
                Fail("the active render pipeline does not support "
                     + "RenderPipeline.StandardRequest, so every capture would be the "
                     + "uninitialised target.");
                return;
            }

            RenderPipeline.SubmitRenderRequest(_camera, request);
        }

        private void Capture(int index)
        {
            var shot = new Texture2D(_target.width, _target.height, TextureFormat.RGBA32, false);
            var previous = RenderTexture.active;
            RenderTexture.active = _target;
            shot.ReadPixels(new Rect(0, 0, _target.width, _target.height), 0, 0);
            RenderTexture.active = previous;

            // **The alpha channel is flattened, on both sides equally.**
            // `Runtime/Resources/Dashscene/UnlitOverlay.shader` blends
            // `SrcAlpha OneMinusSrcAlpha`, so what accumulates in the TARGET's
            // alpha is `src.a * src.a` where uGUI's own blend leaves `src.a` —
            // that shader's remarks say so, and say it is irrelevant against an
            // opaque backbuffer and not irrelevant the moment a host draws into
            // a render texture whose alpha it then consumes. Capturing is
            // exactly that host. Measured on the first judged run: a blended
            // pixel read `[52, 88, 120, 198]` through the painter and
            // `[51, 87, 120, 255]` through the Canvas — the same colour and a
            // different alpha, on most of the frame.
            //
            // Flattening removes a difference that is a property of the capture
            // rather than of either renderer. It hides nothing: both sides are
            // opaque against the camera's own clear, so alpha carries no
            // information here, and the colour channels are untouched.
            var pixels = shot.GetPixels32();
            for (var i = 0; i < pixels.Length; i++)
            {
                pixels[i].a = 255;
            }
            shot.SetPixels32(pixels);
            shot.Apply();

            var path = Path.Combine(_judgeDirectory, $"{index:D2}.png");
            File.WriteAllBytes(path, shot.EncodeToPNG());
            Destroy(shot);

            File.AppendAllText(
                Path.Combine(_judgeDirectory, "entries.txt"),
                $"{index:D2}\t{Label(_index)}\n");
            Debug.Log($"[showcase] judged {Label(index)} -> {path}");
        }

        private void FrameCamera()
        {
            if (!_camera.orthographic)
            {
                return;
            }

            var z = _camera.transform.position.z;
            if (IsScene(_index) && _builtHeight > 0)
            {
                _camera.orthographicSize = _builtHeight / 2.0f;
                _camera.transform.position =
                    new Vector3(_builtWidth / 2.0f, -(_builtHeight / 2.0f), z);
            }
            else
            {
                _camera.orthographicSize = _documentSize;
                _camera.transform.position = new Vector3(
                    _documentCameraPosition.x, _documentCameraPosition.y, z);
            }
        }

        private void UpdateEdgeWidth()
        {
            if (_painter != null && _camera.orthographic && _camera.pixelHeight > 0)
            {
                _painter.EdgeWidth = _camera.orthographicSize * 2.0f / _camera.pixelHeight;
            }
        }

        private bool ReadSceneTable()
        {
#if DASHSCENE_DEMO_PRODUCER
            try
            {
                var count = DemoScenes.Count;
                _sceneNames = new string[count];
                for (var i = 0; i < count; i++)
                {
                    _sceneNames[i] = DemoScenes.Name(i);
                }
                return true;
            }
            catch (Exception e)
            {
                Fail($"the staged library exports no usable ds_demo_* ({e.GetType().Name}: "
                     + $"{e.Message}).");
                return false;
            }
#else
            return true;
#endif
        }

        private bool LoadManifest()
        {
            try
            {
                var text = File.ReadAllText(
                    Path.Combine(Application.streamingAssetsPath, "showcase.json"));
                var manifest = JsonUtility.FromJson<ShowcaseManifest>(text);
                if (manifest?.documents != null)
                {
                    _entries.AddRange(manifest.documents);
                }
                return true;
            }
            catch (Exception e)
            {
                Fail($"showcase.json did not read: {e.Message}");
                return false;
            }
        }

        private IReadOnlyList<TextFontFace> Cascade()
        {
            return new[]
            {
                new TextFontFace
                {
                    Family = "Inter",
                    Weight = 400,
                    FontBytes = ReadBytes("cascade/Inter-Regular.otf"),
                    AtlasPng = ReadBytes("cascade/atlas.png"),
                    AtlasMetrics = ReadBytes("cascade/atlas.metrics"),
                },
            };
        }

        private static byte[] ReadBytes(string relative)
        {
            return File.ReadAllBytes(Path.Combine(Application.streamingAssetsPath, relative));
        }

        private void AnnounceIfEveryEntryHasDrawn()
        {
            if (_announced || _drawnEntries.Count < TotalCount)
            {
                return;
            }

            _announced = true;
            Debug.Log($"[showcase] all {TotalCount} entries drew "
                      + $"({SceneCount} scene(s), {_entries.Count} document(s))");

            if (_quitWhenEveryEntryHasDrawn && _judgeDirectory == null)
            {
                Application.Quit(0);
            }
        }

        private string Label(int index)
        {
            if (IsScene(index))
            {
                return $"scene {_sceneNames[index]}";
            }

            var entry = index - SceneCount < _entries.Count ? _entries[index - SceneCount] : null;
            return entry == null
                ? "no document"
                : string.IsNullOrEmpty(entry.label) ? Path.GetFileName(entry.path) : entry.label;
        }

        private static string Name(Renderer renderer)
        {
            return renderer == Renderer.Painter
                ? "painter"
                : renderer == Renderer.Canvas ? "canvas" : "none";
        }

        private static string ArgumentAfter(string[] args, string flag)
        {
            var at = Array.IndexOf(args, flag);
            return at >= 0 && at + 1 < args.Length ? args[at + 1] : null;
        }

        private static float FloatAfter(string[] args, string flag)
        {
            var value = ArgumentAfter(args, flag);
            return value != null
                   && float.TryParse(
                       value, NumberStyles.Float, CultureInfo.InvariantCulture, out var seconds)
                   && seconds > 0.0f
                ? seconds
                : 0.0f;
        }

        /// **A judged run that fails quits, where a demonstration stays up.**
        /// `Update` and `LateUpdate` both return once this is set, and under
        /// `-judge` the camera is disabled and driven by hand — so a failure that
        /// only logged would leave a process that renders nothing, writes
        /// nothing and exits never, which is indistinguishable from a slow run to
        /// whatever is waiting on it. A person watching a `run` window is the
        /// case that wants the opposite, and gets it.
        private void Fail(string message)
        {
            _failed = true;
            Debug.LogError($"[dashscene] {message}", this);
            if (_judgeDirectory != null)
            {
                Application.Quit(1);
            }
        }

        private void OnDestroy()
        {
            TearDownScene();

            // A `ProfilerRecorder` holds a native handle and accumulates while
            // it is valid, which is why the package's own thread-cost instrument
            // disposes its recorders too.
            if (_batchBuilds.Valid)
            {
                _batchBuilds.Dispose();
            }
            if (_target != null)
            {
                // Re-enabled before the target it was disabled for goes away: a
                // camera left disabled draws nothing for whatever runs next in
                // this player, and the disable is this component's doing.
                if (_camera != null)
                {
                    _camera.enabled = true;
                }
                _target.Release();
                Destroy(_target);
                _target = null;
            }
        }
    }
}
