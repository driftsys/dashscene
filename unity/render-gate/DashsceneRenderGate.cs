// The render gate's player half: draw a document, read the pixels back, and
// decide.
//
// **Not part of the package.** This file and `RenderGateBuild.cs` are copied
// into a throwaway Unity project by `just unity-render` and live outside
// `unity/com.driftsys.dashscene/`, so nothing here ships and
// `unity/package-compat`'s glob never sees them. The same arrangement as
// `unity/editor-compat/`.
//
// **Why it runs in a player and not in a batchmode editor.** Unity strips a
// shader that no scene or material references out of a PLAYER build, and
// nothing is stripped in an editor — which is why every gate this repository
// had passed while the package could not draw as installed (issue #1313). A
// batchmode editor render would inherit that blindness exactly. So this builds
// a player, runs it, and the question it answers is about the package as a
// consumer receives it.
//
// **It renders into a RenderTexture rather than to the screen, and reads that
// back.** Measured on 2026-08-23, `6000.3.22f1`, macOS/Metal:
// `ScreenCapture.CaptureScreenshotAsTexture` returned once and then never
// returned again — the main thread and `UnityGfxDeviceWorker` both sat in
// `semaphore_wait_trap`, and the run had to be killed. Instead the camera is
// disabled and rendered on demand through `RenderPipeline.SubmitRenderRequest`
// into a `RenderTexture` the request names as its destination, which
// `Texture2D.ReadPixels` then reads. That needs no end-of-frame coroutine and
// no visible window, so the gate does not depend on where the player's window
// happens to be stacked — and `camera.targetTexture` is deliberately never
// assigned.
//
// **What it does NOT cover**, so its name is not read as the stronger claim:
//
// - It runs on whatever graphics API the developer's machine gives it. On
//   macOS that is Metal, and Metal is a translation of the shaders rather than
//   the GLES 3.2 or Vulkan the target fleet runs. Issue #1195 is a measured
//   case of that difference mattering. Every number this gate prints carries
//   the API beside it.
// - It is not an oracle. It asserts that ink landed where the committed tables
//   put a node, not that the ink is the right colour — that is issue #828's
//   portable conformance suite — except at three of the order check's seven
//   points, one per material, whose predicates read the colour to R-E22's
//   tolerance; the four over the veil bound it loosely.
// - It draws two documents: `goldens/dsb/v03-paint.dsb` for the ink checks,
//   and `unity/render-gate/order.dsb` for the order check below.
//
// **The order check (issue #1402)** draws a second document through the
// cascade — a full-bleed backdrop, black glyphs over an opaque fill, a
// half-alpha veil over both, and white bold glyphs from a second atlas over
// the veil — and reads seven points of the composite. Every opaque colour in
// that fixture is 0 or 1 per channel, so each point's predicate holds under
// any monotone colour transfer that fixes both ends, and among the orders
// that move one node over another only the painter's satisfies all seven — a
// permutation inside one glyph run moves no node and is not distinguished.
// `unity/package-gate`'s `order_fixture` re-derives the `.dsb` from
// `order.json` and pins the colours and boxes the predicates are written
// against. Every probe is run on the undrawn control frame first and must
// fail there, one by one.
//
// **The negative control runs on every pass**, and it is the reason this file
// is longer than it looks like it needs to be. Issue #1029 is this repository's
// own case of a "did it draw" check passing over a fully black frame, and
// #1232 and #1191 are two more in the same family. So the verdict predicate
// [`Inked`] is evaluated on a frame the painter deliberately did not draw
// before it is evaluated on the drawn one, and the run FAILS if the control
// frame passes. A gate that cannot fail is worse than no gate.

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using Driftsys.Dashscene;
using Unity.Profiling;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

/// <summary>Draws a document in a player and checks that ink landed.</summary>
public sealed class DashsceneRenderGate : MonoBehaviour
{
    /// The document, relative to StreamingAssets. Written there by the recipe.
    private const string DocumentFile = "document.dsb";

    /// The order fixture, relative to StreamingAssets. Written there by the
    /// recipe beside the two faces its text needs.
    private const string OrderDocumentFile = "order.dsb";

    /// The lower bound of a channel that is `1` in the fixture, and the upper
    /// bound of one that is `0`, after whatever the pipeline does to colours.
    ///
    /// Every opaque colour in `order.dsb` sits at 0 or 1 per channel, and a
    /// transfer that fixes both ends — the identity, or sRGB encoding — leaves
    /// them there, so the opaque probes are held to R-E22's own tolerance,
    /// [`ColourEpsilon`], in the encoding the frame is read back in. The
    /// half-alpha veil is the one term between the two, and [`MidLow`] and
    /// [`MidHigh`] bound it loosely: a blend of 0 and 1 at alpha one half is
    /// 0.5 in linear and 0.735 sRGB-encoded, and the alternative the predicate
    /// has to tell it from is always a 0 or a 1. [`MidLow`] is also the
    /// camera's clear colour on its red and green channels
    /// (`RenderGateBuild.cs`), so on the undrawn frame the veil probes' `>
    /// MidLow` term is false by one quantisation step and a term at a channel
    /// end — `< Low` on green, or `> High` on red for `veil-over-fill` — is
    /// what fails them; a clear colour that moved would show in the
    /// per-probe control below rather than in a verdict.
    private const float High = 1.0f - ColourEpsilon;

    private const float Low = ColourEpsilon;

    private const float MidLow = 0.15f;

    private const float MidHigh = 0.9f;

    /// The render target's size, in pixels.
    ///
    /// **Fixed here rather than taken from `Screen`**, so the framing below is
    /// the same whatever window the player opened — a batch-mode player reports
    /// 640x480 and a windowed one reports what the build asked for. The camera's
    /// aspect is set from these two numbers for the same reason.
    private const int Width = 1024;

    /// The render target's height. See [`Width`].
    private const int Height = 768;

    /// Half the view's height in world units.
    ///
    /// The document is placed one world unit per document unit, so 400 gives an
    /// 800-unit view over the fixture's 680-unit height and a 60-unit margin
    /// above and below. At the aspect above, the horizontal margin is 53 units
    /// on each side of its 960-unit width. **That margin is load-bearing**:
    /// [`Background`] reads the frame's own clear colour at a corner, and the
    /// corner has to be outside the document for that to be what it reads.
    private const float OrthographicSize = 400.0f;

    /// The fixture's own extent in document units, which the camera is centred
    /// on.
    private const float DocumentWidth = 960.0f;

    /// The fixture's height. See [`DocumentWidth`].
    private const float DocumentHeight = 680.0f;

    /// Frames to let a step settle before its capture is taken.
    ///
    /// **Two rather than one, and the reason is the read-back's timing.** The
    /// target holds the previous frame's render when a step's capture is taken,
    /// so one frame of margin is arithmetic rather than superstition; the
    /// second is for the frame in which a step change registers or removes
    /// batches.
    private const int SettleFrames = 2;

    /// How far apart two colours must be to count as different.
    ///
    /// Chebyshev distance over the three channels, in the 0..1 range the
    /// framebuffer reads back in. 4/255 is above the one or two least
    /// significant bits a colour-space round trip moves and far below any fill
    /// in the document; every run prints the smallest distance from the clear
    /// colour it measured, which is the quantity this threshold governs, so
    /// this stops being a guess. The stronger per-instance form has a threshold
    /// of its own — zero — and [`Inked`] reports its headroom separately for
    /// exactly that reason.
    private const float ColourEpsilon = 4.0f / 255.0f;

    /// A cutoff no coverage can reach.
    ///
    /// [`BrgPainter.Cutoff`] feeds `clip(shaded.a - _DsCutoff)`, and that alpha
    /// — the fill's own, times coverage, times the clip, times the node's
    /// opacity — is at most 1, so every fragment is discarded at this value
    /// **if the value reaches the fragment stage at all**. That is issue #1307's discriminator
    /// and it assumes nothing about what an unresolved read returns: if
    /// `_DsCutoff` does not resolve, this step and [`CutoffLow`]'s draw exactly
    /// the same picture, whatever the stage reads instead.
    ///
    /// **Above the `Range(0, 1)` the shader's `Properties` block declares, on
    /// purpose.** That range constrains the inspector's slider and not
    /// `Material.SetFloat`, which is the route `BrgPainter.Cutoff` takes — and
    /// the run confirms it rather than resting on the reading: at this value
    /// the cutout class drew nothing at all.
    private const float CutoffHigh = 2.0f;

    /// The cutout class's default threshold.
    private const float CutoffLow = 0.5f;

    /// One step of the run: what the painter is, and whether it draws.
    private readonly struct Step
    {
        public Step(string label, MaterialClass materialClass, bool draw, float cutoff)
            : this(label, materialClass, draw, cutoff, SettleFrames)
        {
        }

        public Step(
            string label, MaterialClass materialClass, bool draw, float cutoff, int frames)
        {
            Label = label;
            MaterialClass = materialClass;
            Draw = draw;
            Cutoff = cutoff;
            Frames = frames;
        }

        /// The name the capture is written under.
        public string Label { get; }

        /// Which of the three classes the painter is on.
        public MaterialClass MaterialClass { get; }

        /// Whether `BrgPainter.Draw` is called at all.
        public bool Draw { get; }

        /// The cutout class's threshold. Ignored by the other two.
        public float Cutoff { get; }

        /// How many frames this step runs for before its capture is taken.
        ///
        /// **A property of the step and not of the gate**, because the four
        /// picture steps need a settled frame and the thread-cost step needs a
        /// whole sampling window. Each step's body runs `Frames + 1` times: the
        /// capture is taken on the frame AFTER the last one that rendered,
        /// which is the offset `Update` documents.
        public int Frames { get; }
    }

    /// The run, in order.
    ///
    /// **The control is first and it is not optional.** It is the same process,
    /// the same camera, the same clear and the same painter as the step after
    /// it, differing only in whether `Draw` is called — so it is the frame the
    /// verdict predicate has to fail on.
    private static readonly Step[] Plan =
    {
        new Step("control", MaterialClass.UnlitOverlay, false, CutoffLow),
        new Step("overlay", MaterialClass.UnlitOverlay, true, CutoffLow),
        new Step("cutout-low", MaterialClass.LitCutout, true, CutoffLow),
        new Step("cutout-high", MaterialClass.LitCutout, true, CutoffHigh),
        new Step(
            "thread-cost", MaterialClass.UnlitOverlay, true, CutoffLow, ThreadCostFrames),
        new Step(SettleLabel, MaterialClass.UnlitOverlay, true, CutoffLow, SettleStepFrames),
    };

    /// The settle step's label, compared in [`Update`] to route that one step
    /// through [`DriveSettleFrame`].
    ///
    /// **A label rather than a sixth `Step` field**, because exactly one step
    /// of the plan is driven differently and every other one would carry a
    /// `false` that says nothing about it.
    private const string SettleLabel = "settle";

    /// How many frames the settle window observes.
    ///
    /// One second at the 60 Hz this player asks for, and the number story
    /// #1445's own report line is stated over.
    private const int SettleWindow = 60;

    /// How many frames the settle step runs.
    ///
    /// The window, plus a forced redraw, a changed drawable extent, and one
    /// frame that only reads the allocation counter for the frame before it.
    /// `Update` runs a step's body `Frames + 1` times, which is the offset that
    /// constant documents, so this is one less than the sixty-three bodies the
    /// step runs.
    private const int SettleStepFrames = SettleWindow + 2;

    /// The two frames of the settle step that take a capture.
    ///
    /// **Named because two methods read them.** `DriveSettleFrame` takes the
    /// captures and `NoteSettleAllocation` excludes those frames from both
    /// allocation populations, and holding two copies of `2` and
    /// `SettleWindow + 1` in step by hand is how a capture frame — a 1024x768
    /// `Texture2D` and a PNG encode — silently joins the skipped population and
    /// carries the verdict with it.
    private const int SettleFirstCaptureFrame = 2;

    /// See [`SettleFirstCaptureFrame`].
    private const int SettleLastCaptureFrame = SettleWindow + 1;

    /// How many frames the thread-cost step runs.
    ///
    /// **Derived from the accumulator rather than written as 300**, so a change
    /// to either constant cannot leave this step one frame short of a sample —
    /// which would read as the instrument being broken.
    ///
    /// **Pushed once per `Update`, never in a loop inside `Judge`.** A
    /// `ProfilerRecorder`'s `LastValue` moves when a Unity FRAME ends, and this
    /// gate renders on demand inside `Update` — so 300 synchronous `Render()`
    /// calls would read one frame's value 300 times, close a sample, and
    /// publish a mean over one reading.
    private const int ThreadCostFrames =
        ThreadCostAccumulator.WarmUp + ThreadCostAccumulator.Sample;

    /// One place the gate expects ink, and one it expects none.
    private struct Sample
    {
        /// The node's box centre, in viewport coordinates.
        public Vector2 Centre;

        /// A point inside the node's box and outside its rounded corner, or
        /// `null` where the node has no corner radius to test.
        public Vector2? OutsideCorner;

        /// The instance's kind, so a stroke — whose box centre carries no ink —
        /// is excluded rather than silently failing.
        public uint Kind;

        /// Whether a clip box excludes the node's own centre.
        ///
        /// **Evaluated rather than assumed from the clip count.** Nearly every
        /// node in a real document carries a clip — its parent frame — and
        /// nearly every one of those clips contains the node entirely. A first
        /// version excluded any instance with a clip region at all and reduced
        /// the fixture's sixteen samples to ONE, which is a gate stated over
        /// almost nothing while reporting that it passed.
        public bool CentreClipped;

        /// The node's opacity. Zero draws nothing, legitimately.
        public float Opacity;

        /// The instance's own solid fill colour, where it is a near-opaque
        /// solid fill and can therefore be told apart from what is behind it.
        ///
        /// **This is what makes the ink check per-node rather than per-picture.**
        /// Comparing a node's centre against the frame's clear colour asks
        /// whether *something* is drawn there, and a document is drawn back to
        /// front: `v03-paint.dsb` has a full-bleed parent frame under every
        /// child, so fifteen of sixteen instances could fail to draw and every
        /// sampled centre would still read the parent's white. A shader that
        /// failed in the player and was replaced by Unity's magenta error
        /// shader would pass the same way — the #1029 family this gate's own
        /// header cites.
        public Color? Solid;
    }

    private DashsceneRuntime _runtime;
    private BrgPainter _painter;

    /// D3's thread-time instrument, constructed with an empty command line so
    /// nothing in this player can disarm it: the point of the step below is to
    /// confirm on an editor that Unity carries the five counter names, and an
    /// instrument that quietly did not arm would report that as a pass.
    ///
    /// **This player carries three of the five counters, and that is not a
    /// defect.** Measured on 6000.3.23f1, macOS/Metal, 2026-09-05: it is
    /// `-batchmode`, so there is no `Render Thread`, and it draws no Canvas, so
    /// there is no `Canvas.SendWillRenderCanvases`. Constructing the instrument
    /// after several frames had rendered changed neither — the counters are
    /// absent, not late. Those two terms report an em dash on the line below
    /// rather than a zero, which is what lets this gate confirm the three names
    /// it can without publishing a measurement it did not take.
    private readonly DashsceneThreadCost _threadCost = new DashsceneThreadCost(new string[0]);

    /// The one sample the thread-cost step closes, or null.
    private ThreadCostSample _threadSample;

    /// The settle step's own host loop, the same class the three samples run.
    private readonly SettleLoop _settle = new SettleLoop();

    /// The drawable height the settle step reports, moved by one pixel on its
    /// last frame.
    private int _settleHeight = Height;

    /// What the settle window measured, or -1 where the step never reached the
    /// frame that records it.
    ///
    /// **Seeded negative rather than zero**, so a judgement over a step that
    /// did not run fails instead of reading a plausible number.
    private int _settleDrawn = -1;

    /// How many of the window's frames were skipped. See [`_settleDrawn`].
    private int _settleSkipped = -1;

    /// Unity's own per-frame managed-allocation counter, over the settle step.
    ///
    /// **`GC.GetAllocatedBytesForCurrentThread` is blind in this player, and
    /// that is measured rather than suspected.** A run of 2026-09-06 allocated
    /// a 4096-byte array inside every one of the settle step's sixty loop
    /// bodies and the counter still reported zero for all of them, so an
    /// assertion resting on it passes over any allocation whatever — the
    /// fail-open shape this gate exists to refuse. The recorder below is the
    /// one `Runtime/Engine/DashsceneThreadCost.cs` reads and the one
    /// `docs/design/android-toolchain.md` records as reporting 832 B/frame in
    /// this player, so it is known to work here. Issue #1468 carries the
    /// measurement and what is owed elsewhere, since D3 names the blind API and
    /// `Samples~/Showcase/DashsceneCanvasBaseline.cs` reads it in a player this
    /// has not been measured in.
    ///
    /// It counts a whole Unity frame rather than the loop body, which is why
    /// the judgement below is a comparison between this step's own skipped and
    /// drawn frames rather than a threshold: both carry the gate's own 832 B
    /// and their difference does not.
    private ProfilerRecorder _settleAlloc;

    /// The largest whole-frame allocation over the settle step's skipped
    /// frames, and how many frames that is stated over.
    private long _settleSkippedAllocMax = -1;

    private int _settleSkippedAllocFrames;

    /// The smallest whole-frame allocation over the settle step's drawn
    /// frames, and how many frames that is stated over.
    private long _settleDrawnAllocMin = -1;

    private int _settleDrawnAllocFrames;

    /// Whether the body before this one drew, so a reading taken one frame
    /// late is attributed to the frame it describes.
    private bool _settlePreviousDrew;

    /// [`BrgPainter.HeapBindCount`] before the settle step's first frame.
    ///
    /// **A baseline rather than an absolute**, because the painter is shared
    /// with the step before this one: `Advance` constructs a new one only when
    /// the material class changes. Every judgement below is a difference from
    /// this, so a step added to the plan moves the reported number and not the
    /// verdict.
    private int _heapBindsBeforeSettle = -1;

    /// The count after a forced redraw over an unchanged document.
    private int _heapBindsAfterForcedRedraw = -1;

    /// The count after the drawable extent moved by one pixel.
    private int _heapBindsAfterResize = -1;

    /// Whether the binding was still pending after the forced redraw drew.
    ///
    /// **Seeded true**, so a step that never recorded it fails.
    private bool _pendingAfterForcedRedraw = true;
    private Camera _camera;
    private RenderTexture _target;
    private string _outDir;

    private int _step = -1;
    private int _framesInStep;
    private bool _finished;

    private List<Sample> _samples;

    /// The packing the samples were built from, kept so the background probe
    /// can be tested against the same quads the corner probes are.
    private FramePacker _probe;
    private readonly Dictionary<string, Texture2D> _shots = new Dictionary<string, Texture2D>();

    private readonly StringBuilder _report = new StringBuilder();
    private readonly List<string> _failures = new List<string>();

    /// Every R-E5 warning the painter logged during this run.
    ///
    /// **This gate's project meets R-E5**, so the correct count is zero and any
    /// entry is issue #1317 restored. Collected rather than asserted at the
    /// point of logging, because the painter reports from `Draw` and this
    /// object judges at the end.
    private readonly List<string> _batcherWarnings = new List<string>();

    private int _overlayInstances;
    private int _cutoutInstances;

    /// The first instance index at which the two packings disagree, or -1.
    private int _packingDiffersAt = -1;
    private bool _readBatcher;

    /// Which class the live painter is on.
    private MaterialClass _currentClass = MaterialClass.UnlitOverlay;

    /// Whether the run has moved on from the plan above to the order fixture.
    private bool _orderPhase;

    /// The order fixture's packing, from which the glyph probes are placed.
    private FramePacker _orderProbe;

    /// The two sheets the order fixture's runs sample.
    private TextAtlasSet _orderAtlases;

    /// `BrgPainter.KindSetKeywordAgreement` taken during the ORDER phase, which
    /// is the only part of this run with text materials on the painter.
    ///
    /// **`Judge` runs before `BeginOrder`**, so a reading taken there compares
    /// the class material and nothing else — measured: it reported one
    /// material. Seeded to -1 so a phase that never took it fails rather than
    /// reporting a comparison that did not happen.
    private int _orderKindSetCompared = -1;

    private int _orderKindSetDisagreeing = -1;

    private void Awake()
    {
        // **The measurement, made by the gate rather than by a person.** Issue
        // #1317 was `BrgPainter` warning that R-E5 was unmet on a project that
        // meets it. This project sets `useSRPBatcher` true
        // (`RenderGateBuild.cs`) and fails if it reads back false, so any such
        // warning here is that defect returning. Until this handler, the check
        // was a developer grepping `player.log` once and writing the result
        // into a record — which the next run could not repeat and no run could
        // fail on.
        Application.logMessageReceived += OnPainterLog;

        Application.targetFrameRate = 60;
        _outDir = ArgumentAfter("-ds-out") ?? Application.persistentDataPath;
        Directory.CreateDirectory(_outDir);

        Line($"unity {Application.unityVersion}");
        Line($"graphics api {SystemInfo.graphicsDeviceType}");
        Line($"graphics device {SystemInfo.graphicsDeviceName}");
        Line($"render pipeline {GraphicsSettings.currentRenderPipeline}");

        _camera = Camera.main;
        if (_camera == null)
        {
            Fail("there is no camera tagged MainCamera in the scene.");
            Finish();
            return;
        }

        // **The camera is disabled and rendered by an explicit request**, into
        // a `RenderTexture` this object owns. Two measurements forced that,
        // both on `6000.3.22f1`, macOS/Metal, 2026-08-23:
        //
        // - A **windowed** player launched from a shell that macOS never
        //   composites stops making progress within a few frames — the main
        //   thread and `UnityGfxDeviceWorker` both sit in `semaphore_wait_trap`
        //   waiting for a drawable that never comes, and the run has to be
        //   killed. A gate that hangs depending on where a window happens to be
        //   stacked is not a gate.
        // - A **batch-mode** player runs its loop and renders NOTHING on its
        //   own: with the camera left to Unity, four captures came back as the
        //   uninitialised target and `GraphicsSettings
        //   .useScriptableRenderPipelineBatching` never turned true, because no
        //   pipeline instance was ever created.
        //
        // `RenderPipeline.SubmitRenderRequest` renders the camera when this
        // object asks, into the destination it names, which needs neither a
        // visible window nor Unity's automatic camera pass. Disabling the
        // camera is what keeps Unity from also rendering it to the back buffer,
        // which is the drawable wait above.
        //
        // **`Camera.main` returns only an ENABLED camera**, so the reference is
        // taken before this line and never re-queried.
        _target = new RenderTexture(Width, Height, 24, RenderTextureFormat.ARGB32)
        {
            name = "DashsceneRenderGate",
        };
        _camera.enabled = false;
        _camera.aspect = (float)Width / Height;
        _camera.orthographic = true;
        _camera.orthographicSize = OrthographicSize;
        _camera.transform.position =
            new Vector3(DocumentWidth * 0.5f, DocumentHeight * -0.5f, -10.0f);

        try
        {
            _runtime = new DashsceneRuntime();
        }
        catch (Exception e)
        {
            Fail($"the runtime could not be created: {e.GetType().Name}: {e.Message}");
            Finish();
            return;
        }

        try
        {
            _runtime.LoadDocument(
                File.ReadAllBytes(Path.Combine(Application.streamingAssetsPath, DocumentFile)));
        }
        catch (Exception e)
        {
            Fail($"{DocumentFile} did not load: {e.GetType().Name}: {e.Message}");
            Finish();
            return;
        }

        // The verdict is `Finish`'s to record; `Advance` returning false has
        // already ended the run.
        Advance();
    }

    /// Construct a painter of one class and place the document under it.
    private bool MakePainter(MaterialClass materialClass, float cutoff)
    {
        try
        {
            _painter = new BrgPainter(materialClass);
        }
        catch (Exception e)
        {
            // **This is where issue #1313 lands in a player.** A stripped
            // shader is a null from the load and the painter throws its own
            // diagnostic — so a gate that only ran in an editor would never see
            // it, and this one reports it as the failure it is.
            // **`Finish()` here, and it is the whole of the fix.** Returning
            // false alone leaves `_finished` clear and `_runtime` live, so
            // `Update` keeps stepping: `Advance` finds `_painter` null, calls
            // this again, and the report carries the same exception once per
            // step with "no frame was packed" — the consequence — as its
            // headline. An earlier attempt at this checked `Advance`'s return
            // in `Awake`, where the `return` is the last statement and changes
            // nothing.
            Fail($"the {materialClass} painter could not be created: "
                 + $"{e.GetType().Name}: {e.Message}");
            Finish();
            return false;
        }

        _currentClass = materialClass;

        // The document's y runs down, so scaling y by -1 is the identity
        // placement; the camera above is positioned for it.
        _painter.DocumentToWorld = Matrix4x4.Scale(new Vector3(1, -1, 1));
        _painter.EdgeWidth = OrthographicSize * 2.0f / Height;
        _painter.Cutoff = cutoff;
        Line($"{materialClass}: rung {_painter.Rung}, cutoff "
             + $"{cutoff.ToString(CultureInfo.InvariantCulture)}, edge width "
             + $"{_painter.EdgeWidth.ToString("0.0000", CultureInfo.InvariantCulture)} "
             + "document units per pixel");
        return true;
    }

    private void Update()
    {
        if (_runtime == null || _finished)
        {
            return;
        }

        // **The target holds the previous frame's render.** [`Render`] is
        // called at the END of this method, so at the top of frame N the target
        // carries what frame N-1 asked for — which is why a step's capture is
        // taken one frame after its last settled frame, and why no end-of-frame
        // hook is needed anywhere in this file.
        // **The step says how long it runs.** The order phase runs past the
        // end of `Plan`, so it takes the settle count directly rather than
        // indexing a step that does not exist.
        var runFor = _orderPhase ? SettleFrames : Plan[_step].Frames;
        if (_framesInStep > runFor)
        {
            if (_orderPhase)
            {
                Capture("order");
                JudgeOrder();
                Finish();
                return;
            }
            Capture(Plan[_step].Label);
            if (!Advance())
            {
                return;
            }
        }

        _framesInStep++;

        try
        {
            if (!_orderPhase && Plan[_step].Label == SettleLabel)
            {
                DriveSettleFrame();

                // **Rendered like every other frame, including the skipped
                // ones.** A skipped frame leaves the batches registered and the
                // culling callback re-emitting the description the last `Draw`
                // laid out, so the camera draws the same picture with the host
                // having done nothing — which is the positive pin an absence
                // scan cannot give, and which the two captures below compare.
                Render();
                return;
            }

            _runtime.Tick(Time.deltaTime);
            using (var lease = _runtime.AcquireFrame())
            {
                if (_painter != null && (_orderPhase || Plan[_step].Draw))
                {
                    _painter.Draw(lease);
                    lease.MarkDrawn();

                    if (_orderPhase)
                    {
                        if (_orderProbe == null)
                        {
                            _orderProbe = new FramePacker();
                            _orderProbe.Pack(
                                lease.Frame, MaterialClass.UnlitOverlay, _orderAtlases);
                        }

                        // **After a draw, not after `SetAtlases`.** The
                        // materials exist from the install, and the keywords
                        // reach them from `ApplyKindSet` inside `Draw` — so a
                        // reading taken before this frame would report the
                        // shader defaults every text material starts at, which
                        // is exactly the state this is here to catch.
                        if (_orderKindSetCompared < 0)
                        {
                            var agreement = _painter.KindSetKeywordAgreement();
                            _orderKindSetCompared = agreement.Compared;
                            _orderKindSetDisagreeing = agreement.Disagreeing;
                        }
                    }
                    else if (_samples == null)
                    {
                        BuildSamples(lease);
                    }
                }
            }

            // **One push per `Update`, on the steps that draw.** This is the
            // same phase the showcase host pushes from, and one Unity frame has
            // ended between two `Update` calls — so each push is one reading.
            // The accumulator is keyed on the entry, so the four picture steps
            // reset it and only the thread-cost step's window closes.
            if (!_orderPhase && Plan[_step].Draw)
            {
                var sample = _threadCost.Push(Plan[_step].Label, Width, Height);
                if (sample != null)
                {
                    _threadSample = sample;
                }
            }

            Render();
        }
        catch (Exception e)
        {
            Fail($"the frame loop threw: {e.GetType().Name}: {e.Message}");
            Finish();
        }
    }

    /// One frame of the settle step, driven the way a host loop drives one.
    ///
    /// The same three calls the three samples make — note the extent, read the
    /// tick's answer, decide through [`SettleLoop`] — so what this step
    /// measures is the loop the package ships rather than an imitation of it.
    ///
    /// **The frames are Unity frames.** One body per `Update`, one `Render` per
    /// body, so a skipped frame is a frame Unity actually rendered with the
    /// host having done nothing. A synchronous loop inside `Judge` would render
    /// sixty times inside one frame and measure nothing about idling.
    ///
    /// The plan of the step, by frame:
    ///
    /// - 1: the first `NoteExtent` is a change from the extent `SettleLoop`
    ///   starts at, so this frame draws whatever the tick reports.
    /// - 2: the target now holds frame 1's render, and is captured.
    /// - 2 to 60: the document is static and the commit is marked, so the tick
    ///   reports no advance and every one of these frames is skipped.
    /// - 61: frame 60's render is captured, the window's counts are recorded,
    ///   and a redraw is forced — the frame a host takes when its surface came
    ///   back under a document that did not change. Nothing the binding
    ///   carries has moved, so the heap must not rebind.
    /// - 62: the drawable is reported one pixel taller and the anti-aliasing
    ///   width with it, which moves the scalars `BindHeap` carries and nothing
    ///   else. The heap must rebind.
    /// - 63: nothing but the allocation reading for frame 62, which is a drawn
    ///   frame and would otherwise have no reader. The body itself skips.
    ///
    /// **The allocation reading is one frame late and two frames are dropped.**
    /// A `ProfilerRecorder`'s `LastValue` moves when a Unity frame ends, so the
    /// value read at the top of body k describes body k-1. Bodies 2 and 61 take
    /// a capture — a 1024x768 `Texture2D` and a PNG encode — so their frames
    /// are excluded rather than compared: they are the harness allocating, not
    /// the host loop.
    private void DriveSettleFrame()
    {
        NoteSettleAllocation();

        if (_framesInStep == 1)
        {
            _heapBindsBeforeSettle = _painter.HeapBindCount;
            _settleAlloc = ProfilerRecorder.StartNew(
                ProfilerCategory.Memory, "GC Allocated In Frame", 1);
        }
        else if (_framesInStep == SettleFirstCaptureFrame)
        {
            Capture($"{SettleLabel}-1");
        }
        else if (_framesInStep == SettleLastCaptureFrame)
        {
            Capture($"{SettleLabel}-{SettleWindow}");
            _settleDrawn = _settle.FramesDrawn;
            _settleSkipped = _settle.FramesSkipped;
            _settle.ForceRedraw();
        }
        else if (_framesInStep == SettleWindow + 2)
        {
            _heapBindsAfterForcedRedraw = _painter.HeapBindCount;
            _pendingAfterForcedRedraw = _painter.HeapBindingPending;

            // **The extent is reported, not resized.** What the binding
            // carries is the anti-aliasing width, and `MakePainter` derives it
            // from the target's height — so a drawable one pixel taller is
            // these two lines. Recreating the `RenderTexture` would move
            // `Width` and `Height`, which every capture, every viewport
            // conversion and the camera's aspect are stated over, and would
            // measure the resize rather than the rebinding.
            _settleHeight = Height + 1;
            _painter.EdgeWidth = OrthographicSize * 2.0f / _settleHeight;
        }

        _settle.NoteExtent(Width, _settleHeight);
        var advanced = _runtime.Tick(Time.deltaTime);
        var drew = _settle.ShouldDraw(advanced);
        if (drew)
        {
            using (var lease = _runtime.AcquireFrame())
            {
                _painter.Draw(lease);
                lease.MarkDrawn();
            }
        }

        _settlePreviousDrew = drew;

        if (_framesInStep == SettleWindow + 2)
        {
            _heapBindsAfterResize = _painter.HeapBindCount;
        }
    }

    /// Attribute the previous frame's managed allocation to the body that ran
    /// in it.
    ///
    /// The counter is whole-frame and one frame late, so what this collects is
    /// two populations from the same step under the same harness: the frames
    /// this host skipped and the frames it drew. Their difference is the
    /// allocation the settle path removes, and the gate's own 832 B is in both.
    private void NoteSettleAllocation()
    {
        var previous = _framesInStep - 1;
        if (previous < 1 || !_settleAlloc.Valid)
        {
            return;
        }

        // The two capture frames are the harness allocating a `Texture2D` and
        // a PNG, which is neither population.
        //
        // **One of the two is the forced-redraw draw**, so that frame is
        // measured by nothing. It runs the same six statements as the other two
        // drawn frames — note the extent, tick, decide, acquire, draw, mark —
        // and no statement anywhere runs only because a redraw was forced, so
        // there is no forced-redraw-only path for a regression to hide in. The
        // capture has to sit on that frame: it reads the target, which holds
        // frame 60's render only there.
        if (previous == SettleFirstCaptureFrame || previous == SettleLastCaptureFrame)
        {
            return;
        }

        var bytes = _settleAlloc.LastValue;
        if (_settlePreviousDrew)
        {
            _settleDrawnAllocFrames++;
            if (_settleDrawnAllocMin < 0 || bytes < _settleDrawnAllocMin)
            {
                _settleDrawnAllocMin = bytes;
            }
        }
        else
        {
            _settleSkippedAllocFrames++;
            if (bytes > _settleSkippedAllocMax)
            {
                _settleSkippedAllocMax = bytes;
            }
        }
    }

    /// Render the camera into the target, now.
    private void Render()
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
            Fail(
                "the active render pipeline does not support RenderPipeline.StandardRequest, "
                + "so this gate cannot render on demand and every capture below would be the "
                + "uninitialised target.");
            Finish();
            return;
        }

        RenderPipeline.SubmitRenderRequest(_camera, request);
    }

    /// Move to the next step, or judge and stop. False when the run is over.
    private bool Advance()
    {
        var previous = _step;
        _step++;
        _framesInStep = 0;

        if (previous >= 0)
        {
            // **Read after a frame has actually rendered, never in `Awake`.**
            // `GraphicsSettings.useScriptableRenderPipelineBatching` is assigned
            // by `UniversalRenderPipeline`'s constructor from the asset's
            // `useSRPBatcher` — one line in `UniversalRenderPipeline.cs` — and
            // that constructor runs when Unity first creates a pipeline
            // INSTANCE, at the first render. Measured on 6000.3.22f1,
            // macOS/Metal, 2026-08-23: a player whose URP asset had
            // `useSRPBatcher` true reported this global false in `Awake` and
            // true four frames later. `BrgPainter` read it in its own
            // constructor and so warned on a correctly configured project,
            // which is issue #1317; it now guards the read on
            // `RenderPipelineManager.currentPipeline` and takes it from `Draw`.
            ReadBatcherOnce();
        }

        if (_step >= Plan.Length)
        {
            Judge();
            if (!BeginOrder())
            {
                Finish();
                return false;
            }
            return true;
        }

        var step = Plan[_step];
        if (_painter == null)
        {
            return MakePainter(step.MaterialClass, step.Cutoff);
        }

        if (_painter.Rung == BrgRung.InstancedWithoutBrg)
        {
            // R-E19 selects rung 3 where a BatchRendererGroup is unsupported,
            // and nothing is built for it. Reported here rather than measured
            // around, because every number below would be the number of a frame
            // that drew nothing.
            Fail($"the painter reports rung {_painter.Rung}, which draws nothing. Nothing is "
                 + "built for rung 3, so this run cannot say anything about the picture.");
            Finish();
            return false;
        }

        // **One painter is replaced only when the class changes**, and the
        // cutoff alone never needs a new one. A painter owns its
        // BatchRendererGroup, its mesh and its materials, and this gate asks
        // about one material class at a time — so disposing before constructing
        // is what keeps exactly one alive and one class under measurement.
        //
        // **The reason used to be a different one**, and PR #1372 retired it:
        // the paint heap was bound with `Shader.SetGlobalBuffer`, so two live
        // painters shaded from each other's tables (issue #1297, now closed).
        // The heap binds per material now, so that collision is gone; the
        // dispose-before-construct order is kept for the reason above.
        if (step.MaterialClass != _currentClass)
        {
            _painter.Dispose();
            _painter = null;
            return MakePainter(step.MaterialClass, step.Cutoff);
        }

        _painter.Cutoff = step.Cutoff;
        Line($"step {step.Label}: {step.MaterialClass}, cutoff "
             + $"{step.Cutoff.ToString(CultureInfo.InvariantCulture)}, "
             + $"draw {step.Draw}");
        return true;
    }

    private void ReadBatcherOnce()
    {
        if (_readBatcher)
        {
            return;
        }
        _readBatcher = true;

        // **The painter's guard rests on this, so measure it rather than
        // assume it.** Since issue #1317 `BrgPainter.ReportBatcherOnce` says
        // nothing while `RenderPipelineManager.currentPipeline` is null, on the
        // grounds that the global is not a verdict before URP has constructed
        // an instance. This gate drives rendering with `SubmitRenderRequest`
        // rather than letting Unity render a camera, and nothing established
        // that a pipeline instance exists under that arrangement — so the
        // painter staying silent here would be indistinguishable from a guard
        // that never opens, and the absence of an R-E5 warning would be
        // evidence of nothing. Failing is right rather than merely reporting:
        // the batcher read on the next line is meaningless without an instance,
        // which is the whole reason this method is called late.
        var live = RenderPipelineManager.currentPipeline != null;
        Line($"render pipeline instance live {live}");
        if (!live)
        {
            Fail(
                "no render pipeline instance exists after a frame has rendered, so "
                + "GraphicsSettings.useScriptableRenderPipelineBatching has not been assigned "
                + "and neither this gate nor BrgPainter.ReportBatcherOnce can read it as a "
                + "verdict (issue #1317).");
            return;
        }

        var on = GraphicsSettings.useScriptableRenderPipelineBatching;
        Line($"srp batcher after the first render {on}");
        if (!on)
        {
            Fail(
                "the SRP Batcher is off in this player after a frame has rendered, which is "
                + "R-E5. BatchRendererGroup needs it, so whether anything drew says nothing "
                + "about the painter while it is off.");
        }
    }

    /// Read the render target back into a texture this object keeps.
    private void Capture(string label)
    {
        var shot = new Texture2D(Width, Height, TextureFormat.RGBA32, false);
        var previous = RenderTexture.active;
        RenderTexture.active = _target;
        shot.ReadPixels(new Rect(0, 0, Width, Height), 0, 0);
        shot.Apply();
        RenderTexture.active = previous;
        _shots[label] = shot;

        var path = Path.Combine(_outDir, $"{label}.png");
        File.WriteAllBytes(path, shot.EncodeToPNG());
        Line($"captured {label} -> {path}");
    }

    /// Where the committed tables say ink belongs, in viewport coordinates.
    ///
    /// **Packed a second time here rather than read off the painter.** The
    /// painter keeps its staging arrays private, and a gate that read the
    /// painter's own idea of where it drew would be asking the painter to mark
    /// its own work. `FramePacker` is the engine-free half that decides what
    /// the picture is, and running it here derives the expectation from the
    /// committed tables instead.
    ///
    /// It is still the packer's arithmetic on both sides, and that is the limit
    /// of what this gate claims: it says ink landed where the document places a
    /// node, not that the node's own geometry is right. Issue #828's suite is
    /// what judges the second.
    private void BuildSamples(FrameLease lease)
    {
        var probe = new FramePacker();
        probe.Pack(lease.Frame, MaterialClass.UnlitOverlay);
        _probe = probe;
        _overlayInstances = probe.InstanceCount;

        var cutoutProbe = new FramePacker();
        cutoutProbe.Pack(lease.Frame, MaterialClass.LitCutout);
        _cutoutInstances = cutoutProbe.InstanceCount;
        _packingDiffersAt = FirstDisagreement(probe, cutoutProbe);

        _samples = new List<Sample>(probe.InstanceCount);
        for (var i = 0; i < probe.InstanceCount; i++)
        {
            var x = probe.Quad[(i * 4) + 0];
            var y = probe.Quad[(i * 4) + 1];
            var w = probe.Quad[(i * 4) + 2];
            var h = probe.Quad[(i * 4) + 3];

            // **Placed, not the box's own centre.** The clip test, the
            // coverage test and the pixel read all have to be asking about the
            // same document point, and the shader's is the turned one.
            var centre = Placed(probe, i, new Vector2(x + (w * 0.5f), y + (h * 0.5f)));
            var sample = new Sample
            {
                Kind = probe.Paint[(i * 4) + 0],
                CentreClipped = !ClipContains(probe, i, centre),
                Opacity = probe.Shade[(i * 4) + 0],
                Solid = SolidColour(probe, i, centre),
                Centre = ToViewport(centre),
                OutsideCorner = null,
            };

            // The top-left corner radius, and a point inside the box that the
            // rounded shape excludes. `(0.2r, 0.2r)` in from the box corner sits
            // 1.13r from the corner's centre of curvature, outside the arc by a
            // margin no one-pixel anti-aliasing ramp closes on a radius this
            // large.
            //
            // **Only where no other instance can ink that point.** A document
            // is drawn back to front and its nodes overlap, so "outside this
            // node's rounded corner" is not "background" — a parent frame's own
            // fill sits under every child. A first version asserted on the
            // point regardless and reported a square corner on a picture whose
            // corners are round, on both material classes at once, which is
            // what a probe measuring the node behind looks like.
            var radius = probe.Corners[(i * 4) + 0];
            var corner = Placed(probe, i, new Vector2(x + (radius * 0.2f), y + (radius * 0.2f)));
            if (radius > 8.0f
                && radius < Mathf.Min(w, h) * 0.5f
                && NothingElseCovers(probe, i, corner))
            {
                sample.OutsideCorner = ToViewport(corner);
            }

            _samples.Add(sample);
        }

        var strokes = 0;
        var clipped = 0;
        var transparent = 0;
        foreach (var sample in _samples)
        {
            if (sample.Kind == (uint)PaintKindTag.Stroke)
            {
                strokes++;
            }
            else if (sample.CentreClipped)
            {
                clipped++;
            }
            else if (sample.Opacity <= 0.01f)
            {
                transparent++;
            }
        }

        Line($"instances: {_overlayInstances} on UnlitOverlay, {_cutoutInstances} on LitCutout");
        Line($"sampled: {Sampled()} node centres, {CornerProbes()} outside-the-corner probes");
        // **Printed rather than left implicit.** Every exclusion below is a
        // node this gate stops asserting anything about, and an exclusion rule
        // that quietly swallowed the whole document would otherwise read as a
        // pass over sixteen instances.
        Line($"excluded: {strokes} stroke(s), {clipped} clipped centre(s), "
             + $"{transparent} transparent");
        Line($"diagnostics: {probe.Diagnostics}");
    }

    /// Whether every clip box the instance names contains a document point.
    ///
    /// **The PLACED point.** `DashsceneInstance.hlsl` evaluates the clip at
    /// `input.placed` — the point after the instance's own rotation — and its
    /// varying's comment says why: a clip box belongs to an ancestor that is
    /// not rotating, so testing against the unturned point rotates the clip
    /// along with the node it clips. That file records a first version which
    /// passed the unturned point and cut every rotated clipped node along the
    /// wrong rectangle; this gate made the same mistake and it is corrected
    /// here.
    ///
    /// The corner radii are ignored, which makes this answer "inside the clip's
    /// box" rather than "inside the clip". A centre inside the box and outside
    /// a rounded corner of it would be counted as unclipped and then fail the
    /// ink check — a false failure this fixture does not produce and a reader
    /// should know is possible.
    private static bool ClipContains(FramePacker packer, int instance, Vector2 point)
    {
        var offset = packer.Paint[(instance * 4) + 2];
        var count = packer.Paint[(instance * 4) + 3];
        for (var i = 0u; i < count; i++)
        {
            var at = (int)((offset + i) * PaintHeap.ClipWords * 4);
            var x = packer.ClipBoxes[at + 0];
            var y = packer.ClipBoxes[at + 1];
            var w = packer.ClipBoxes[at + 2];
            var h = packer.ClipBoxes[at + 3];
            if (point.x < x || point.x > x + w || point.y < y || point.y > y + h)
            {
                return false;
            }
        }
        return true;
    }

    /// Whether this instance is the only one whose quad reaches a document
    /// point.
    ///
    /// The quad each instance rasterises is its box grown on every side by
    /// `outset + aa`, which is what `DashsceneInstance.hlsl`'s vertex stage
    /// builds, and it is turned about that instance's pivot. So `point` is a
    /// PLACED document point, and each candidate's rotation is inverted to ask
    /// the question in that candidate's own frame — the inverse of [`Placed`].
    ///
    /// **Conservative on purpose**: a quad that reaches the point is counted as
    /// covering it even where its own shape would leave the point uninked.
    /// That costs probes and never produces a false failure, which is the right
    /// way round for a check whose failure means "a corner radius is drawing
    /// square".
    private bool NothingElseCovers(FramePacker packer, int instance, Vector2 point)
    {
        return CoveringInstance(packer, instance, point) == null;
    }

    /// Whether any instance drawn AFTER this one reaches a placed document
    /// point.
    ///
    /// **This assumes draw order is submission order, and the same run
    /// measures that assumption** rather than trusting it. Without the sorting
    /// keys `BatchRendererGroup` groups the commands by material (issue
    /// #1389); with them, and one instance per command, the draw order is the
    /// keys' rank — the painter emits its draw commands in rect order and lays
    /// command 0 farthest, so a higher index is drawn later and on top
    /// (issue #1402, `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
    /// D1). [`JudgeOrder`] checks exactly that on `order.dsb` in the run this
    /// method's verdicts belong to, and fails the run otherwise.
    ///
    /// The wrong verdict this predicate would give under any other order —
    /// where a lower-indexed node covers this centre and its colour is nearer
    /// this node's own than the clear colour is, the run PASSES on ink that is
    /// not this node's — is therefore caught by the order phase rather than
    /// left to the assumption. The predicate is not widened, for the reason
    /// the next paragraph gives.
    ///
    /// **The assumption cannot be dropped by widening the search**, which is
    /// why the sibling [`NothingElseCovers`] is not used here. A document's
    /// parent frame reaches every one of its children, and it is drawn first —
    /// so asking "does ANY other instance reach this point" would downgrade
    /// every child of a filled frame and leave the stronger form with nothing
    /// to judge. A corner probe can afford that predicate because it sits
    /// outside its own node; a box centre cannot.
    ///
    /// If the order were the reverse, a centre hidden under a lower-indexed
    /// node keeps the stronger form, and either outcome is wrong in a different
    /// way: where the covering node's colour is nearer this node's own than the
    /// clear colour the run PASSES on ink that is not its own, and where it is
    /// not the run FAILS and blames the painter for drawing nothing. Neither is
    /// silent — a re-sorted range is a real defect — but only the first is a
    /// wrong verdict, and the second names the wrong cause.
    private bool LaterInstanceCovers(FramePacker packer, int instance, Vector2 point)
    {
        for (var j = instance + 1; j < packer.InstanceCount; j++)
        {
            if (Reaches(packer, j, point))
            {
                return true;
            }
        }
        return false;
    }

    /// The first instance other than `except` whose quad reaches a placed
    /// document point, or null when none does.
    private int? CoveringInstance(FramePacker packer, int except, Vector2 point)
    {
        for (var j = 0; j < packer.InstanceCount; j++)
        {
            if (j != except && Reaches(packer, j, point))
            {
                return j;
            }
        }
        return null;
    }

    /// Whether one instance's rasterised quad reaches a placed document point.
    private bool Reaches(FramePacker packer, int instance, Vector2 point)
    {
        var margin = packer.Shade[(instance * 4) + 1] + _painter.EdgeWidth;
        var x = packer.Quad[(instance * 4) + 0] - margin;
        var y = packer.Quad[(instance * 4) + 1] - margin;
        var w = packer.Quad[(instance * 4) + 2] + (2.0f * margin);
        var h = packer.Quad[(instance * 4) + 3] + (2.0f * margin);

        // The point taken back into this instance's own frame, which is the
        // inverse of what `Placed` did to build it.
        var angle = -packer.Shade[(instance * 4) + 2];
        var pivot = new Vector2(
            packer.Pivot[(instance * 4) + 0], packer.Pivot[(instance * 4) + 1]);
        var d = point - pivot;
        var s = Mathf.Sin(angle);
        var c = Mathf.Cos(angle);
        var own = pivot + new Vector2((d.x * c) - (d.y * s), (d.x * s) + (d.y * c));

        return own.x >= x && own.x <= x + w && own.y >= y && own.y <= y + h;
    }

    /// One instance's own point, turned into the shared document frame.
    ///
    /// **`DashsceneInstance.hlsl`'s vertex stage turns the quad about
    /// `_DsPivot` by `_DsShade.z`**, and everything downstream of that — the
    /// rasterised position AND the clip coverage — is evaluated on the turned
    /// point, which the shader calls `placed`. So the box centre of a rotated
    /// node is not where its ink is, and it is not the point its clip is tested
    /// at either.
    ///
    /// **Every point this gate derives from an INSTANCE goes through here.** A
    /// first version applied the rotation in `ToViewport` alone and left
    /// `ClipContains` and `NothingElseCovers` reading unrotated points, which
    /// disagreed with the shader for any rotated node — quietly, because the
    /// only fixture has no rotation. [`BackgroundDocumentPoint`] is the one
    /// point that does not come from an instance: it comes from the camera, and
    /// is unrotated by construction.
    private static Vector2 Placed(FramePacker packer, int instance, Vector2 own)
    {
        var angle = packer.Shade[(instance * 4) + 2];
        var pivot = new Vector2(
            packer.Pivot[(instance * 4) + 0], packer.Pivot[(instance * 4) + 1]);
        var d = own - pivot;
        var s = Mathf.Sin(angle);
        var c = Mathf.Cos(angle);
        return pivot + new Vector2((d.x * c) - (d.y * s), (d.x * s) + (d.y * c));
    }

    /// The first instance the two packings describe differently, or -1.
    ///
    /// Compares what every judgement below depends on: the node's box, its
    /// pivot, its corner radii, its `(opacity, outset, rotation)` and its
    /// `(kind, row, clip)`.
    ///
    /// **`Corners` is in the list because a corner judgement reads it.**
    /// [`InkedCorners`] runs on the two cutout frames at points derived from
    /// `probe.Corners` in the OVERLAY packing, so a radius that differed
    /// between the two packings would move the probe off the corner it is
    /// meant to sit outside of, and every other field could still agree.
    private static int FirstDisagreement(FramePacker a, FramePacker b)
    {
        var count = Mathf.Min(a.InstanceCount, b.InstanceCount);
        for (var i = 0; i < count * 4; i++)
        {
            if (a.Quad[i] != b.Quad[i]
                || a.Pivot[i] != b.Pivot[i]
                || a.Corners[i] != b.Corners[i]
                || a.Shade[i] != b.Shade[i]
                || a.Paint[i] != b.Paint[i])
            {
                return i / 4;
            }
        }
        return -1;
    }

    /// A placed document point in viewport coordinates.
    private Vector2 ToViewport(Vector2 placed)
    {
        var world = _painter.DocumentToWorld.MultiplyPoint3x4(new Vector3(placed.x, placed.y, 0));
        var viewport = _camera.WorldToViewportPoint(world);
        return new Vector2(viewport.x, viewport.y);
    }

    /// Every assertion, and the negative control that makes them mean something.
    private void Judge()
    {
        // **The painter's silence, checked rather than assumed.** Liveness
        // above says the painter's guard could open; this says that when it
        // did, it found R-E5 met. Both halves are needed: without liveness a
        // silent painter proves nothing, and without this a painter that
        // warned on every frame of a conforming project would still pass.
        Line($"painter R-E5 warnings {_batcherWarnings.Count}");
        if (_batcherWarnings.Count > 0)
        {
            Fail(
                $"the painter logged {_batcherWarnings.Count} R-E5 warning(s) on a project "
                + "whose URP asset has useSRPBatcher true, which is issue #1317: the SRP "
                + $"Batcher read is not a verdict where it was taken. First: {_batcherWarnings[0]}");
        }

        JudgeKindSetKeywords();

        if (_samples == null || _overlayInstances == 0)
        {
            Fail("no frame was packed, so the gate has nowhere to look for ink.");
            return;
        }

        // 1. THE NEGATIVE CONTROL. The verdict predicate, run first on a frame
        //    the painter did not draw. A run where this passes is a run whose
        //    verdict means nothing, and it has happened in this repository
        //    before (issue #1029).
        var control = Shot("control");
        if (control == null)
        {
            return;
        }

        // **The corner the clear colour is read at carries no ink**, which
        // the camera framing is supposed to guarantee and which a different
        // document would break silently — every distance below is measured
        // against that pixel. `NothingElseCovers` is the same test the corner
        // probes use, asked of the background probe.
        var backgroundPoint = BackgroundDocumentPoint();
        if (backgroundPoint != null)
        {
            Fail(
                "the pixel the clear colour is read from is inside instance "
                + $"{backgroundPoint}'s quad, so every ink measurement in this run is taken "
                + "against a fill rather than against the clear colour. The camera framing "
                + "and the document no longer agree.");
            return;
        }

        var controlInked = Inked(control, true, out var controlCount, out _);
        Line($"control: ink at {controlCount} of {Sampled()} sampled centres");
        if (controlInked)
        {
            Fail(
                $"the CONTROL frame passes the ink check at all {Sampled()} sampled centres. "
                + "The painter did not draw it, so this check cannot distinguish a drawn "
                + "frame from an undrawn one and its verdict on the drawn frames is void. "
                + "The clear colour is probably one the document also paints.");
            return;
        }

        // 2. The drawn frame, by the same predicate.
        var overlay = Shot("overlay");
        if (overlay == null)
        {
            return;
        }

        var overlayInked = Inked(
            overlay,
            true,
            out var overlayCount,
            out var weakest,
            out var headroom,
            out var perInstance);
        Line($"overlay: ink at {overlayCount} of {Sampled()} sampled centres, smallest distance "
             + $"from the clear colour {weakest.ToString("0.000", CultureInfo.InvariantCulture)} "
             + $"(epsilon {ColourEpsilon.ToString("0.000", CultureInfo.InvariantCulture)})");

        // **The two numbers are governed by two thresholds**, so they are
        // printed apart. See [`Inked`].
        Line($"overlay: {perInstance} of {Sampled()} centres judged against the instance's own "
             + "colour, the rest against the clear colour; smallest advantage over the clear "
             + $"colour {Headroom(headroom)} (threshold 0.000)");

        // **Said out loud on every run.** The stronger per-instance assertion
        // excludes a centre only when a HIGHER-indexed instance reaches it,
        // which is sound only while draw order is submission order — and this
        // run's order phase measures that on its own fixture rather than
        // assuming it (issue #1402). The predicate is unchanged because
        // widening it leaves the stronger form nothing to judge.
        Line(
            "overlay: the per-instance assertion assumes a lower-indexed node cannot cover "
            + "a centre; the order phase below measures that order on order.dsb, so a "
            + "PASS here depends on a PASS there.");

        // **A run where none got the stronger form is a run of the weaker gate,
        // whatever its verdict says.** The document has solid fills that
        // nothing later covers; if none reached this check, `SolidColour` has
        // stopped finding them.
        if (perInstance == 0)
        {
            Fail(
                "no sampled centre was judged against its own instance's colour, so every "
                + "one fell back to 'differs from the clear colour' — which a parent frame's "
                + "fill satisfies for every child. The per-instance check has stopped "
                + "reaching any sample.");
        }
        if (!overlayInked)
        {
            Fail(
                $"the UnlitOverlay frame carries ink at {overlayCount} of {Sampled()} sampled "
                + "node centres. Every one of them is a node the committed tables place and "
                + "the packer emitted, so the painter drew nothing there.");
        }

        // 3. The overlay's silhouette: a point inside each node's box and
        //    outside its rounded corner carries no ink.
        var overlayCorners = InkedCorners(overlay);
        Line($"overlay: {overlayCorners} of {CornerProbes()} outside-the-corner probes "
             + "carry ink");
        if (CornerProbes() == 0)
        {
            // **Said out loud rather than passed over.** Two guards above can
            // empty the set — a radius outside the band in which a probe is
            // meaningful, and a point some other instance's quad reaches — and
            // neither the count nor this line distinguishes them, so the
            // message does not claim which. What it does claim is the part
            // that matters: on this run the corner silhouette is checked by
            // nothing. A fixture with one isolated rounded node would change
            // that; issue #828's suite is where that belongs.
            Line(
                "overlay: no outside-the-corner probe reached the assertion — a radius "
                + "outside the band a probe is meaningful in, or a point another "
                + "instance's quad reaches, and this line does not say which — so this "
                + "run says nothing about whether corner radii are drawn round.");
        }
        if (CornerProbes() > 0 && overlayCorners > 0)
        {
            Fail(
                $"{overlayCorners} of {CornerProbes()} points inside a node's box and outside "
                + "its rounded corner carry ink on the overlay class, so a corner radius is "
                + "drawing square.");
        }

        // **The samples come from the overlay packing and judge cutout
        // frames**, so the two packings agreeing is a precondition of
        // `JudgeCutoff` rather than a curiosity. `FramePacker.Pack` branches on
        // the material class, and this gate does not own that file.
        if (_cutoutInstances != _overlayInstances)
        {
            Fail(
                $"the packer emits {_overlayInstances} instances on UnlitOverlay and "
                + $"{_cutoutInstances} on LitCutout, so the sample positions below were "
                + "derived from a different picture than the cutout frames draw.");
        }
        else if (_packingDiffersAt >= 0)
        {
            // **Equal counts are not the property.** What has to hold is that
            // instance `i` is the same node in both packings — its box, its
            // pivot, its rotation and its paint. A future class rule that
            // substituted or reordered an instance would keep the count and
            // move the geometry, and every cutout judgement below is taken at
            // the overlay packing's positions.
            Fail(
                $"the two packings disagree at instance {_packingDiffersAt}: the sample "
                + "positions come from the UnlitOverlay packing and the cutout frames are "
                + "drawn from the LitCutout one, so the two are being compared at different "
                + "nodes.");
        }

        // 4. THE THREAD-TIME INSTRUMENT, and the URP floor it is read over.
        //    Story #1443's first "done when": the five counter names D3 gives
        //    are confirmed on an editor here rather than assumed, because a
        //    `ProfilerRecorder` over a name Unity does not carry is not an
        //    error — it stays invalid and reports `LastValue` 0 for ever, and a
        //    zero Canvas-rebuild term reads as a Canvas that rebuilds nothing.
        JudgeThreadCost();

        // 5. THE SETTLE PATH. Story #1445: the host idles when the tick reports
        //    no advance, and the heap binds when the binding goes stale rather
        //    than on every frame. Judged last because its step runs last —
        //    everything above is stated over pictures this gate had already
        //    captured before that step began.
        JudgeSettle();

        JudgeCutoff();
    }

    /// The host idled, the picture survived the idling, and the binding
    /// refreshed on the change that moves it and on nothing else.
    ///
    /// **Four readings, because each of them can be wrong on its own.** A host
    /// that drew every frame is the cost story #1445 removes, still there. A
    /// host that idled and lost the picture is a saving that cannot be taken. A
    /// skipped frame that still allocates is D3's steady-frame rule unmet with
    /// the draw already gone. And a binding refreshed on every frame, or on
    /// none, is the same picture drawn at two different costs — one of which is
    /// wrong.
    private void JudgeSettle()
    {
        Line($"settle — drew {_settleDrawn} of {SettleWindow} frames, "
             + $"skipped {_settleSkipped}");

        if (_settleDrawn != 1)
        {
            Fail(
                $"the settle step drew {_settleDrawn} of {SettleWindow} frames over a static "
                + "document whose commit was marked shown, where exactly one — the first, "
                + "forced by the extent this loop had not yet reported — should have drawn. "
                + "A count equal to the window is a tick that keeps reporting an advance or a "
                + "loop that ignores it; a count of zero is a first frame that never drew.");
        }

        // **The positive pin.** `settle_path.rs` can see that the acquire is
        // behind the decision and cannot see that the picture survives it: a
        // skipped frame leaves the batches registered and the culling callback
        // re-emitting them, and only a photograph says so.
        var first = Shot($"{SettleLabel}-1");
        var last = Shot($"{SettleLabel}-{SettleWindow}");
        if (first != null && last != null)
        {
            var differing = DifferingPixels(first, last);
            Line($"settle — {differing} of {Width * Height} pixels differ between the "
                 + $"capture after frame 1 and the capture after frame {SettleWindow}");
            if (differing != 0)
            {
                Fail(
                    $"{differing} pixel(s) differ between the settle step's first drawn frame "
                    + $"and its {SettleWindow}th, on a document nothing changed. The host "
                    + "stopped drawing and the picture did not survive it, so the frames this "
                    + "step skipped were frames that had work to do.");
            }
        }

        Line($"settle — managed allocation per Unity frame: skipped max "
             + $"{_settleSkippedAllocMax} B over {_settleSkippedAllocFrames} frame(s), "
             + $"drawn min {_settleDrawnAllocMin} B over {_settleDrawnAllocFrames} frame(s)");

        // **The counts first, because a comparison between two empty sets
        // passes.** Fifty-eight skipped frames — the window's fifty-nine less
        // the one that took a capture — and two drawn ones, frames 1 and 62.
        if (_settleSkippedAllocFrames != SettleWindow - 2 || _settleDrawnAllocFrames != 2)
        {
            Fail(
                $"the settle step read the allocation counter over "
                + $"{_settleSkippedAllocFrames} skipped and {_settleDrawnAllocFrames} drawn "
                + $"frame(s), not {SettleWindow - 2} and 2. The comparison below is stated "
                + "over those two populations, and over an empty one it says nothing.");
        }

        // **The instrument is proved alive before it is believed**, and this is
        // the assertion that would have caught the one it replaced.
        // `GC.GetAllocatedBytesForCurrentThread` reported zero in this player
        // over sixty loop bodies that each allocated a 4096-byte array, so a
        // gate resting on it passed over any allocation whatever. A drawn Unity
        // frame allocates — this gate's own frame is 832 B by this counter,
        // which `docs/design/android-toolchain.md` records — so a zero here is
        // a counter that is not reporting rather than a frame that allocated
        // nothing.
        if (_settleDrawnAllocMin <= 0)
        {
            Fail(
                $"the `GC Allocated In Frame` counter read {_settleDrawnAllocMin} B over the "
                + "settle step's drawn frames. A drawn frame acquires a lease, packs, uploads "
                + "and marks, and this gate's own frame allocates 832 B by this counter — so "
                + "this is a counter that is not reporting, and every allocation judgement in "
                + "this step would be a judgement over a constant.");
        }
        else if (_settleSkippedAllocMax >= _settleDrawnAllocMin)
        {
            // **Strictly cheaper, not merely no dearer**, and the strictness is
            // the second half of proving the instrument. A `ProfilerRecorder`
            // that stopped updating and reported its last real sample for ever
            // would pass every guard above — it is `Valid`, and it is not zero
            // — and would report the two populations as equal, which is what
            // this refuses. The measured gap is 688 B, so this is not a
            // marginal comparison.
            Fail(
                $"a skipped settle frame allocated {_settleSkippedAllocMax} B and the "
                + $"cheapest drawn one {_settleDrawnAllocMin} B. Skipping does the acquire's, "
                + "the pack's, the upload's and the bind's work without any of them, so it "
                + "must cost less managed memory than doing them — and two populations that "
                + "read exactly alike are a counter that has stopped moving.");
        }
        else
        {
            // **The detection floor, said out loud.** This is a comparison
            // between two populations and not a zero bar: the gate's own frame
            // is in both, so an allocation added to the skipped path smaller
            // than the gap below is inside the noise this cannot separate.
            // D3's zero is stated over a per-loop-body instrument, and the one
            // it names does not report in this player — issue #1468.
            Line($"settle — the skipped frame is {_settleDrawnAllocMin - _settleSkippedAllocMax}"
                 + " B/frame cheaper than the cheapest drawn one, both carrying this gate's "
                 + "own frame; that difference is also this comparison's detection floor");
        }

        Line($"settle — heap bound {_heapBindsAfterResize} time(s) after the resize, "
             + $"{_heapBindsAfterForcedRedraw} after the forced redraw, "
             + $"{_heapBindsBeforeSettle} before the step");

        if (_heapBindsAfterForcedRedraw != _heapBindsBeforeSettle)
        {
            Fail(
                $"the heap bound {_heapBindsAfterForcedRedraw - _heapBindsBeforeSettle} "
                + "time(s) over the settle window and the forced redraw that follows it. "
                + "Nothing the binding carries moved: no table was reallocated, no atlas set "
                + "changed, and the scalars are the ones the step's first frame bound. Every "
                + "one of those binds is five Material.Set… calls per material for a binding "
                + "that was already correct.");
        }

        if (_pendingAfterForcedRedraw)
        {
            Fail(
                "the heap binding was still pending after the forced redraw drew. `BindHeap` "
                + "clears the flag, so a flag left raised is a `Draw` that read it and did "
                + "not bind, or a reason raising it on every frame — either of which makes "
                + "the guard measure nothing.");
        }

        if (_heapBindsAfterResize != _heapBindsBeforeSettle + 1)
        {
            Fail(
                $"the drawable extent moved by one pixel and the heap bound "
                + $"{_heapBindsAfterResize - _heapBindsAfterForcedRedraw} time(s). The "
                + "anti-aliasing width is one of the four scalars `BindHeap` carries and it "
                + "is derived from that extent, so a binding that did not refresh here shades "
                + "the resized document at the previous size's edge width — with no "
                + "reallocation anywhere to raise the flag for it.");
        }
    }

    /// The painter's material carries the keywords this document's kind set
    /// names, and no others (story #1449).
    ///
    /// **Nothing else in CI can see this.** `BrgPainter.ApplyKindSet` is
    /// `Runtime/Engine/`, which `unity/ffi-check` excludes and no CI job
    /// compiles; `unity/package-gate` reads it as text; and
    /// `KindSetKeywords.KeywordsFor` — which `unity/ffi-check` does execute —
    /// is the mapping alone. Between the mapping and the variant a fragment
    /// runs there are three steps this is the only reader of: that
    /// `ApplyKindSet` ran at all, that the shader declares the keyword it was
    /// handed, and that the material took it. A keyword no shader declares
    /// selects nothing and reports nothing.
    ///
    /// **The expectation is derived from the document, not written here.**
    /// `v03-paint.dsb` reports both bits today, and a re-recorded fixture that
    /// reported neither would make a literal expectation pass over a painter
    /// that enabled nothing. The gate fails a fixture whose set is empty for
    /// the same reason: with nothing to enable, `ApplyKindSet` never toggling
    /// would draw exactly the same picture.
    private void JudgeKindSetKeywords()
    {
        if (_painter == null || _runtime == null)
        {
            Fail("no painter or runtime survived the plan, so the keyword state is unreadable.");
            return;
        }

        var bits = _runtime.KindSet();
        var expected = KindSetKeywords.KeywordsFor(bits);
        var actual = _painter.EnabledKindSetKeywords();

        Line($"kind set {bits} — keywords enabled [{string.Join(", ", actual)}], "
             + $"expected [{string.Join(", ", expected)}]");

        if (expected.Length == 0)
        {
            Fail(
                $"the loaded document reports a kind set of {bits}, so there is no keyword "
                + "for this check to observe: a painter that never applied one would enable "
                + "the same nothing. The fixture this gate draws has to reach at least one "
                + "arm for the check to have teeth.");
            return;
        }

        if (!actual.SequenceEqual(expected))
        {
            Fail(
                $"the document reports a kind set of {bits}, whose keywords are "
                + $"[{string.Join(", ", expected)}], and the painter's class material carries "
                + $"[{string.Join(", ", actual)}]. Unity selects a variant from the material's "
                + "own keyword state, so a missing one removes that arm from every fragment — "
                + "a clipped document drawing ink outside its clip, or a stroke drawing as its "
                + "node's fill — and an extra one compiles in an arm the document cannot reach.");
        }
    }

    /// The instrument armed, closed a sample, and the sample is not zero.
    ///
    /// **Three separate failures, because they send a reader to three different
    /// places.** A disarmed instrument names the counter this player cannot
    /// record; no sample means the step is shorter than a window or the push
    /// left `Update`; a zero mean means a counter that exists and is not
    /// written to, which is the same wrong number as an invalid one with
    /// nothing saying so.
    private void JudgeThreadCost()
    {
        Line($"thread cost armed {_threadCost.Armed}");

        // **Said out loud on every run**, because these are the terms that
        // report an em dash rather than a number: a reader of the line below
        // who does not know which counters this player carries would read a
        // dash as a defect in the instrument.
        Line("thread cost counters this player cannot record: "
             + (_threadCost.UnrecordedCounters.Length == 0
                 ? "none"
                 : _threadCost.UnrecordedCounters));

        if (!_threadCost.Armed)
        {
            Fail(
                "the thread-time instrument did not arm in this player: "
                + $"{_threadCost.Reason}. It refuses only on the Main Thread counter, which "
                + "every player carries and which D3's criterion is stated on — the other "
                + "four report an em dash where this player lacks them, so a refusal here "
                + "is not a missing Canvas marker.");
        }
        else if (_threadSample == null)
        {
            Fail(
                $"the instrument armed and closed no sample over the {ThreadCostFrames}-frame "
                + $"step, which is {ThreadCostAccumulator.WarmUp} warm-up frames and "
                + $"{ThreadCostAccumulator.Sample} collected ones. Every push must be a "
                + "separate Unity frame; a push moved inside a synchronous render loop reads "
                + "one frame's value repeatedly.");
        }
        else
        {
            Line($"thread cost — {_threadSample.Line()}");
            if (_threadSample.MainMean <= 0.0)
            {
                Fail(
                    "the main-thread counter reported a mean of "
                    + $"{_threadSample.MainMean.ToString(CultureInfo.InvariantCulture)} ms over "
                    + $"{_threadSample.Frames} frames. A recorder that is valid and reads zero "
                    + "is a counter that exists and is not being written to, which publishes "
                    + "the same wrong number as an invalid one and says nothing about it.");
            }
        }

        JudgeUrpFloor();
    }

    /// The five URP fields, read off the pipeline this player is running.
    ///
    /// **The built asset, not the source that made it.**
    /// `unity/package-gate`'s `thread_cost_instrument` pins the five
    /// assignments in `DemoBuild.cs` and `RenderGateBuild.cs`; this pins the
    /// values that reached the player, so a later import, a preset or a
    /// serialised override that overwrote one of them is caught. Both are
    /// needed: a scan cannot see the asset, and the asset cannot say which
    /// source built it.
    private void JudgeUrpFloor()
    {
        var urp = GraphicsSettings.currentRenderPipeline as UniversalRenderPipelineAsset;
        if (urp == null)
        {
            Fail(
                "GraphicsSettings.currentRenderPipeline is not a UniversalRenderPipelineAsset, "
                + "so the five floor fields cannot be read back at all. R-E4 is checked at "
                + "build time; this is the same asset seen from inside the player.");
            return;
        }

        var list = urp.rendererDataList;
        var renderer = list.Length > 0 ? list[0] as UniversalRendererData : null;
        var post = renderer == null
            ? "unreadable"
            : renderer.postProcessData == null ? "null" : "set";
        Line(
            $"urp floor: supportsHDR {urp.supportsHDR}, msaa {urp.msaaSampleCount}, "
            + $"depthTexture {urp.supportsCameraDepthTexture}, "
            + $"opaqueTexture {urp.supportsCameraOpaqueTexture}, postProcessData {post}");

        if (urp.supportsHDR)
        {
            Fail("the URP asset in this player has supportsHDR on, which costs an "
                 + "intermediate target and its blit on both renderers.");
        }

        if (urp.msaaSampleCount != 1)
        {
            Fail($"the URP asset in this player has msaaSampleCount {urp.msaaSampleCount}, "
                 + "which costs bandwidth on a tiler and buys a painter of screen-aligned "
                 + "quads nothing (R-T1).");
        }

        if (urp.supportsCameraDepthTexture)
        {
            Fail("the URP asset in this player has supportsCameraDepthTexture on, which "
                 + "copies the frame's depth every frame and is read by neither renderer.");
        }

        if (urp.supportsCameraOpaqueTexture)
        {
            Fail("the URP asset in this player has supportsCameraOpaqueTexture on, which "
                 + "copies the frame's colour every frame and is read by neither renderer.");
        }

        if (renderer == null)
        {
            Fail("the URP asset carries no UniversalRendererData, so post-processing cannot "
                 + "be read back. The other four fields above were read.");
        }
        else if (renderer.postProcessData != null)
        {
            Fail("the URP renderer in this player carries post-process data, which adds "
                 + "passes to every frame of both renderers.");
        }
    }

    /// Issue #1307: does `_DsCutoff` reach the fragment stage?
    private void JudgeCutoff()
    {
        var low = Shot("cutout-low");
        var high = Shot("cutout-high");
        if (low == null || high == null)
        {
            return;
        }

        // **A cutout painter that never drew produces the same two blank
        // frames a resolving `_DsCutoff` would**, and the verdict below would
        // then pin a shader-resolution failure — the exact regression this gate
        // exists to catch — on issue #1307. So the reading is refused unless
        // something already failed, in which case that failure is the report.
        if (_failures.Count > 0)
        {
            Line("cutout: _DsCutoff is NOT judged — an earlier step failed, so the two "
                 + "cutout frames say nothing about whether the value reaches the stage.");
            return;
        }

        // **`false`: the cutout class shades the albedo**, so the per-instance
        // colour comparison does not hold on these two frames. See [`Inked`].
        // `albedoReachesThePixel` is false on both, so no centre reaches the
        // stronger form and `ColourEpsilon` governs every number printed below.
        Inked(low, false, out var lowCount, out var lowMargin);
        Inked(high, false, out var highCount, out _);
        var lowCorners = InkedCorners(low);
        var highCorners = InkedCorners(high);
        var changed = DifferingPixels(low, high);

        Line($"cutout at {CutoffLow.ToString(CultureInfo.InvariantCulture)}: ink at "
             + $"{lowCount} of {Sampled()} centres, {lowCorners} of {CornerProbes()} "
             + "outside-the-corner probes, smallest distance from the clear colour "
             + $"{lowMargin.ToString("0.000", CultureInfo.InvariantCulture)} "
             + $"(epsilon {ColourEpsilon.ToString("0.000", CultureInfo.InvariantCulture)})");
        Line($"cutout at {CutoffHigh.ToString(CultureInfo.InvariantCulture)}: ink at "
             + $"{highCount} of {Sampled()} centres, {highCorners} of {CornerProbes()} "
             + "outside-the-corner probes");
        Line($"cutout: {changed} of {Width * Height} pixels differ between the two cutoffs");

        // **The discriminator, and it assumes nothing about what an unresolved
        // read returns.** `clip(shaded.a - _DsCutoff)` with that alpha at most
        // 1 discards every fragment at a cutoff of 2, so the two frames must
        // differ if the value reaches the stage. If it does not, the stage
        // reads the same thing in both runs, whatever that is, and the two
        // frames are identical.
        var resolves = changed > 0;
        Line($"cutout: _DsCutoff {(resolves ? "RESOLVES" : "DOES NOT RESOLVE")} under "
             + "DOTS_INSTANCING_ON");

        if (!resolves)
        {
            Fail(
                "_DsCutoff does not reach the fragment stage under DOTS_INSTANCING_ON: a "
                + $"frame drawn at {CutoffLow.ToString(CultureInfo.InvariantCulture)} and a "
                + $"frame drawn at {CutoffHigh.ToString(CultureInfo.InvariantCulture)} are "
                + "pixel identical, and a cutoff above any achievable coverage must discard "
                + "every fragment. The LitCutout class is thresholding at whatever the shader "
                + "reads instead. Issue #1307.");
            return;
        }

        // **What "resolves" has to mean, spelled out as two numbers**, because
        // "the two frames differ" alone would also be satisfied by a shader
        // reading garbage that happened to differ. At a cutoff above any
        // achievable coverage nothing survives the `clip`; at the class's own
        // default the node's silhouette does, and its rounded corner does not.
        if (highCount != 0)
        {
            Fail(
                $"the cutout class carries ink at {highCount} of {Sampled()} node centres at "
                + $"a cutoff of {CutoffHigh.ToString(CultureInfo.InvariantCulture)}, which is "
                + "above any coverage a fragment can have — so `clip` should have discarded "
                + "every one of them.");
        }
        if (lowCount != Sampled())
        {
            Fail(
                $"the cutout class carries ink at {lowCount} of {Sampled()} node centres at "
                + $"its default cutoff of {CutoffLow.ToString(CultureInfo.InvariantCulture)}, "
                + "where the node's own silhouette should survive.");
        }
        if (CornerProbes() > 0 && lowCorners > 0)
        {
            Fail(
                $"{lowCorners} of {CornerProbes()} points outside a node's rounded corner "
                + "carry ink on the LitCutout class at its default cutoff, so the class is "
                + "drawing every fragment of its quad rather than the node's silhouette.");
        }
    }

    /// Load the order fixture through the cascade and put an overlay painter
    /// over it. False with the failure recorded, or when nothing is left to
    /// judge.
    ///
    /// **Refused after any failure**, for `JudgeCutoff`'s reason: a painter
    /// that already drew nothing would give the order predicate a blank frame
    /// and the report would then blame the order.
    private bool BeginOrder()
    {
        if (_failures.Count > 0)
        {
            Line("order: NOT judged — an earlier step failed, so the order frame would say "
                 + "nothing about the order.");
            return false;
        }

        var path = Path.Combine(Application.streamingAssetsPath, OrderDocumentFile);
        if (!File.Exists(path))
        {
            Fail($"no {OrderDocumentFile} under StreamingAssets, so the draw order is judged by "
                 + "nothing. The recipe stages it beside the cascade.");
            return false;
        }

        // The cutout painter of the last plan step is replaced, not reused:
        // the order question is the overlay class's (D3 of the order record),
        // and a painter owns its group, mesh and materials.
        _painter?.Dispose();
        _painter = null;

        try
        {
            _runtime.LoadDocumentWithText(File.ReadAllBytes(path), Cascade());
            _orderAtlases = _runtime.ReadAtlases();
        }
        catch (Exception e)
        {
            Fail($"{OrderDocumentFile}, or a cascade file under StreamingAssets/cascade/, did "
                 + "not load: "
                 + $"{e.GetType().Name}: {e.Message}");
            return false;
        }

        // **Two, and it is a property the fixture is written against.** The
        // regular and bold runs sample different sheets, which is what makes
        // the bold glyphs a second material — the case issue #1389 measured
        // the grouping on. One atlas would collapse it back to one material
        // and the probe over the bold glyphs would then test nothing.
        if (_orderAtlases.Count != 2)
        {
            Fail($"the cascade installed {_orderAtlases.Count} atlas(es); the order fixture "
                 + "needs its regular and bold faces on two sheets.");
            return false;
        }

        if (!MakePainter(MaterialClass.UnlitOverlay, CutoffLow))
        {
            return false;
        }
        if (_painter.Rung == BrgRung.InstancedWithoutBrg)
        {
            Fail($"the order phase's painter reports rung {_painter.Rung}, which draws "
                 + "nothing, so an order verdict would be a verdict on an undrawn frame.");
            return false;
        }
        // Caught here, for `MakePainter`'s reason: `SetAtlases` throws when the
        // text shader is missing from the player — issue #1313's class — or a
        // sheet does not decode, and `Advance` runs before `Update`'s own try,
        // so an uncaught throw here leaves `_step` past the plan and the next
        // frame reports an index out of range instead of the cause.
        try
        {
            _painter.SetAtlases(_orderAtlases);
        }
        catch (Exception e)
        {
            Fail($"the order phase's painter refused the cascade's atlases: "
                 + $"{e.GetType().Name}: {e.Message}");
            return false;
        }

        _orderPhase = true;
        _orderProbe = null;
        _framesInStep = 0;
        Line($"step order: {MaterialClass.UnlitOverlay}, {OrderDocumentFile} through "
             + $"{_orderAtlases.Count} atlases");
        return true;
    }

    /// The two Inter faces the order fixture names, each with its committed
    /// sheet — `corpus/fonts/inter` and `corpus/atlas/inter-ascii*`, staged by
    /// the recipe.
    private static IReadOnlyList<TextFontFace> Cascade()
    {
        return new[]
        {
            new TextFontFace
            {
                Family = "Inter",
                Weight = 400,
                FontBytes = Streaming("cascade/Inter-Regular.otf"),
                AtlasPng = Streaming("cascade/regular/atlas.png"),
                AtlasMetrics = Streaming("cascade/regular/atlas.metrics"),
            },
            new TextFontFace
            {
                Family = "Inter",
                Weight = 700,
                FontBytes = Streaming("cascade/Inter-Bold.otf"),
                AtlasPng = Streaming("cascade/bold/atlas.png"),
                AtlasMetrics = Streaming("cascade/bold/atlas.metrics"),
            },
        };
    }

    private static byte[] Streaming(string relative)
    {
        return File.ReadAllBytes(Path.Combine(Application.streamingAssetsPath, relative));
    }

    /// One point of the order composite: where it is, what covers it, and what
    /// the painter's order leaves there.
    private struct OrderProbe
    {
        public string Name;

        /// The point, in document units.
        public Vector2 Document;

        /// Which instance classes the fixture puts over this point. Checked
        /// against the packing, so a probe that drifted off its node fails
        /// as a fixture defect rather than as an order defect.
        public string[] Covered;

        /// What the painter's order composites here, as a predicate on the
        /// pixel read back, and in words.
        public Func<Color, bool> Expect;

        public string Means;
    }

    /// Issue #1402: is the composite the painter's order, and no other?
    ///
    /// **Seven points, one predicate each, and among the orders that move one
    /// node over another only the painter's satisfies all seven.** The fixture's opaque colours are 0 or 1 per
    /// channel, so what the alternative orders put at each point is always a
    /// 0 or a 1 where the expected value is the other end or the veil's
    /// midpoint — the predicate never has to know what the pipeline did to a
    /// colour beyond fixing its ends and keeping it monotone.
    ///
    /// `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` D2: a
    /// legible frame is not evidence of a correct order, so no count of
    /// bright pixels appears here. The undrawn control frame is put through
    /// every probe first and must fail each of them.
    /// Every material the painter draws with carries the same `DS_HAS_*` set,
    /// not the class material alone (story #1449).
    ///
    /// **Judged here because this is the only phase with text materials on the
    /// painter.** `Judge` runs before `BeginOrder`, and the plan's own steps
    /// install no atlas set — measured: a reading taken in `Judge` compared one
    /// material. The order fixture installs two sheets, so the painter carries
    /// a class material and two text materials here.
    ///
    /// **What it catches that `JudgeKindSetKeywords` cannot.** A text material
    /// is minted by `SetAtlases` long after the constructor and starts at its
    /// shader's defaults — every keyword undefined. Only `ApplyKindSet` walking
    /// `_textMaterials`, and the `_kindSetApplied = false` that `SetAtlases`
    /// sets so the next `Draw` re-applies, put a set on it. Drop either and a
    /// clipped document's GLYPHS shade through the variant with the clip loop
    /// removed — inking outside the region that clips them — while the class
    /// material still reports the right set.
    private void JudgeKindSetAcrossMaterials()
    {
        Line($"kind set — {_orderKindSetCompared} material(s) compared, "
             + $"{_orderKindSetDisagreeing} disagreeing");

        if (_orderKindSetCompared < 2)
        {
            Fail(
                $"the order phase compared {_orderKindSetCompared} material(s), so this run "
                + "cannot tell a painter that applies the kind set to every material from one "
                + "that applies it to the class material alone. The order fixture's two "
                + "atlases are what mint the text materials this needs.");
            return;
        }

        if (_orderKindSetDisagreeing > 0)
        {
            Fail(
                $"{_orderKindSetDisagreeing} of {_orderKindSetCompared} materials carry a "
                + "different `DS_HAS_*` set from the class material. Unity selects a variant "
                + "per material, so a text material left at its shader's defaults draws its "
                + "glyphs through the variant with the clip loop removed.");
        }
    }

    private void JudgeOrder()
    {
        JudgeKindSetAcrossMaterials();

        var order = Shot("order");
        var control = Shot("control");
        if (order == null || control == null || _orderProbe == null)
        {
            if (_orderProbe == null)
            {
                Fail("the order fixture was never packed, so no probe has a position.");
            }
            return;
        }

        var packer = _orderProbe;
        Line($"order: {packer.InstanceCount} instances, diagnostics {packer.Diagnostics}");
        if (!packer.Diagnostics.IsClean)
        {
            Fail("the order fixture packs with diagnostics, so part of it is not drawn and "
                 + "the composite is not the one the probes are written against.");
            return;
        }

        // **Every instance named, so a probe can be checked against what the
        // packer actually emitted.** The three solids and every glyph are here
        // — a text node's own box packs no instance; the report prints the
        // table so a mismatch can be read rather than guessed.
        var classes = new string[packer.InstanceCount];
        int? backdrop = null, fill = null, veil = null;
        var regularOutside = new List<Vector2>();
        var regularUnderVeil = new List<Vector2>();
        var bold = new List<Vector2>();
        var regularIndices = new List<int>();
        var boldIndices = new List<int>();
        for (var i = 0; i < packer.InstanceCount; i++)
        {
            var x = packer.Quad[(i * 4) + 0];
            var y = packer.Quad[(i * 4) + 1];
            var w = packer.Quad[(i * 4) + 2];
            var h = packer.Quad[(i * 4) + 3];
            var kind = packer.Paint[(i * 4) + 0];
            // Placed, not the box's own centre: the invariant `Placed` states.
            var centre = Placed(packer, i, new Vector2(x + (w * 0.5f), y + (h * 0.5f)));
            var solid = SolidRow(packer, i);
            var alpha = solid?.a ?? 0.0f;

            var cls = "other";
            if (kind == (uint)PaintKindTag.FillSolid && alpha > 0.99f
                && Near(x, 0) && Near(y, 0) && Near(w, 960) && Near(h, 680))
            {
                cls = "backdrop";
                backdrop ??= i;
            }
            else if (kind == (uint)PaintKindTag.FillSolid && alpha > 0.99f
                     && Near(x, 160) && Near(y, 160) && Near(w, 480) && Near(h, 260))
            {
                cls = "fill";
                fill ??= i;
            }
            else if (kind == (uint)PaintKindTag.FillSolid && alpha > 0.4f && alpha < 0.6f)
            {
                cls = "veil";
                veil ??= i;
            }
            else if (kind == (uint)PaintKindTag.Text && centre.x < 690.0f)
            {
                // The regular run. A glyph is a probe only where its whole
                // quad, antialiasing margin included, sits clear of the veil's
                // left edge at x = 420 — a glyph that straddles it (one, on this
                // fixture) is a regular glyph and probes nothing. The split
                // reads the box, not the placed centre: the fixture rotates
                // nothing, and a rotated run would need both halves placed.
                cls = "regular";
                regularIndices.Add(i);
                var margin = _painter.EdgeWidth + 2.0f;
                if (x + w + margin < 420.0f)
                {
                    regularOutside.Add(centre);
                }
                else if (x - margin > 420.0f)
                {
                    regularUnderVeil.Add(centre);
                }
            }
            else if (kind == (uint)PaintKindTag.Text && centre.x > 690.0f)
            {
                cls = "bold";
                boldIndices.Add(i);
                bold.Add(centre);
            }
            classes[i] = cls;
            Line($"order: instance {i} {cls} kind {kind} box ({F(x)}, {F(y)}, {F(w)}, {F(h)}) "
                 + $"solid {(solid == null ? "none" : Rgba(solid.Value))} "
                 + $"opacity {F(packer.Shade[(i * 4) + 0])}");
        }

        if (backdrop == null || fill == null || veil == null
            || regularOutside.Count == 0 || regularUnderVeil.Count == 0 || bold.Count == 0)
        {
            Fail("the order fixture's packing does not carry every node the probes need: "
                 + $"backdrop {backdrop != null}, fill {fill != null}, veil {veil != null}, "
                 + $"regular glyphs outside the veil {regularOutside.Count}, under it "
                 + $"{regularUnderVeil.Count}, bold glyphs {bold.Count}. Read the instance "
                 + "table above.");
            return;
        }
        if (!(backdrop < fill && fill < veil))
        {
            Fail($"the packer emits backdrop {backdrop}, fill {fill} and veil {veil}, which is "
                 + "not the fixture's paint order, so the probes' expectations do not "
                 + "describe this packing.");
            return;
        }
        // The glyph runs' places in the emission order are what the three
        // glyph probes are written against, and a packer that moved a run
        // would otherwise be reported as Unity's order defect.
        if (!(fill < regularIndices.Min() && regularIndices.Max() < veil
              && veil < boldIndices.Min()))
        {
            Fail($"the packer emits the regular glyphs at {regularIndices.Min()}..{regularIndices.Max()} "
                 + $"and the bold glyphs from {boldIndices.Min()}, around fill {fill} and veil "
                 + $"{veil}, which is not the fixture's paint order, so the glyph probes' "
                 + "expectations do not describe this packing.");
            return;
        }
        // **Two materials, read from the packing and not counted at the
        // cascade.** R-E22 asks for one probed pixel per registered material,
        // and a bold run resolved to the regular sheet would draw the same
        // white `I` from the same glyph slot — the probe cannot tell the
        // sheets apart, so the atlas each run samples is held here.
        var regularAtlas = packer.InstanceAtlas[regularIndices[0]];
        var boldAtlas = packer.InstanceAtlas[boldIndices[0]];
        if (regularAtlas < 0 || boldAtlas < 0 || regularAtlas == boldAtlas
            || regularIndices.Any(i => packer.InstanceAtlas[i] != regularAtlas)
            || boldIndices.Any(i => packer.InstanceAtlas[i] != boldAtlas))
        {
            Fail($"the regular run samples atlas {regularAtlas} and the bold run atlas "
                 + $"{boldAtlas}; the order fixture needs the two runs on two sheets, so that "
                 + "the bold probe is a second text material (R-E22).");
            return;
        }
        Line($"order: the regular run samples atlas {regularAtlas}, the bold run atlas {boldAtlas}");
        for (var i = 0; i < classes.Length; i++)
        {
            if (classes[i] == "other")
            {
                Fail($"instance {i} of the order fixture is nothing the probes account for.");
                return;
            }
        }

        var probes = new[]
        {
            new OrderProbe
            {
                Name = "backdrop",
                Document = new Vector2(80, 600),
                Covered = new[] { "backdrop" },
                Expect = c => c.r < Low && c.g < Low && c.b > High,
                Means = "blue: the backdrop drew",
            },
            new OrderProbe
            {
                Name = "fill",
                Document = new Vector2(180, 400),
                Covered = new[] { "backdrop", "fill" },
                Expect = c => c.r > High && c.g > High && c.b < Low,
                Means = "yellow: the fill is over the backdrop",
            },
            new OrderProbe
            {
                Name = "regular-glyph",
                Document = regularOutside[0],
                Covered = new[] { "backdrop", "fill", "regular" },
                Expect = c => c.r < Low && c.g < Low && c.b < Low,
                Means = "black: the regular glyph is over the fill and the backdrop",
            },
            new OrderProbe
            {
                Name = "veil-over-backdrop",
                Document = new Vector2(800, 400),
                Covered = new[] { "backdrop", "veil" },
                Expect = c => c.r > MidLow && c.r < MidHigh && c.g < Low,
                Means = "half red over blue: the veil is over the backdrop and translucent",
            },
            new OrderProbe
            {
                Name = "veil-over-fill",
                Document = new Vector2(560, 400),
                Covered = new[] { "backdrop", "fill", "veil" },
                Expect = c => c.r > High && c.g > MidLow && c.g < MidHigh && c.b < Low,
                Means = "half red over yellow: the veil is over the fill",
            },
            new OrderProbe
            {
                Name = "veil-over-regular-glyph",
                Document = regularUnderVeil[regularUnderVeil.Count - 1],
                Covered = new[] { "backdrop", "fill", "regular", "veil" },
                Expect = c => c.r > MidLow && c.r < MidHigh && c.g < Low && c.b < Low,
                Means = "half red over black: the veil is over the regular glyph",
            },
            new OrderProbe
            {
                Name = "bold-glyph",
                Document = bold[0],
                Covered = new[] { "backdrop", "veil", "bold" },
                Expect = c => c.r > High && c.g > High && c.b > High,
                Means = "white: the bold glyph, from the second atlas, is over the veil",
            },
        };

        // The fixture and the probes have to agree before the frame is read:
        // a probe that a class the fixture puts there does not reach, or that
        // an unexpected one does, is a fixture defect and is reported as one.
        foreach (var probe in probes)
        {
            var covering = new List<string>();
            for (var i = 0; i < packer.InstanceCount; i++)
            {
                if (Reaches(packer, i, probe.Document))
                {
                    if (!covering.Contains(classes[i]))
                    {
                        covering.Add(classes[i]);
                    }
                }
            }
            covering.Sort(StringComparer.Ordinal);
            var expected = new List<string>(probe.Covered);
            expected.Sort(StringComparer.Ordinal);
            if (!covering.SequenceEqual(expected))
            {
                Fail($"order probe {probe.Name} at ({F(probe.Document.x)}, "
                     + $"{F(probe.Document.y)}) is reached by [{string.Join(", ", covering)}] "
                     + $"and was written for [{string.Join(", ", expected)}]. The fixture and "
                     + "the probes no longer agree.");
            }
        }
        if (_failures.Count > 0)
        {
            return;
        }

        // 1. The negative control: the undrawn frame must fail every probe,
        // one by one — a probe the clear colour satisfies discriminates
        // nothing on the order frame, so one holding is enough to void the
        // verdict. A first version failed only when all seven held, which two
        // contradictory probes made unreachable.
        var controlHolds = new List<string>();
        foreach (var probe in probes)
        {
            var pixel = Read(control, ToViewport(probe.Document));
            var held = probe.Expect(pixel);
            Line($"order: control {probe.Name} reads {Rgb(pixel)}: {(held ? "HOLDS" : "fails")}");
            if (held)
            {
                controlHolds.Add(probe.Name);
            }
        }
        Line($"order: control frame satisfies {controlHolds.Count} of {probes.Length} probes");
        if (controlHolds.Count != 0)
        {
            Fail($"the order predicate holds on the CONTROL frame at [{string.Join(", ", controlHolds)}], "
                 + "which the painter did not draw, so those probes' verdicts on the order "
                 + "frame mean nothing.");
            return;
        }

        // 2. The order frame.
        var holds = 0;
        foreach (var probe in probes)
        {
            var pixel = Read(order, ToViewport(probe.Document));
            var ok = probe.Expect(pixel);
            holds += ok ? 1 : 0;
            Line($"order: {probe.Name} at ({F(probe.Document.x)}, {F(probe.Document.y)}) "
                 + $"reads {Rgb(pixel)} — expected {probe.Means}: {(ok ? "OK" : "WRONG")}");
        }
        Line($"order: {holds} of {probes.Length} probes composite in the painter's order");
        if (holds != probes.Length)
        {
            Fail($"{probes.Length - holds} of {probes.Length} order probes do not read what "
                 + "the painter's order composites. The draw order is not the emission order "
                 + "(issue #1402).");
        }
    }

    /// The solid row an instance names with the node's opacity folded into its
    /// alpha, whatever that alpha is, or null for a kind that has none.
    private static Color? SolidRow(FramePacker packer, int instance)
    {
        var raw = RawSolid(packer, instance);
        if (raw == null)
        {
            return null;
        }
        var c = raw.Value;
        return new Color(c.r, c.g, c.b, c.a * packer.Shade[(instance * 4) + 0]);
    }

    /// The solid row an instance names, as the heap holds it, or null for a
    /// kind that has none or a row past the heap. The one place the row's
    /// address is decoded; [`SolidRow`] and [`SolidColour`] both read through
    /// it.
    private static Color? RawSolid(FramePacker packer, int instance)
    {
        if (packer.Paint[(instance * 4) + 0] != (uint)PaintKindTag.FillSolid)
        {
            return null;
        }
        var at = (int)(packer.SolidBase + packer.Paint[(instance * 4) + 1]) * 4;
        if (at < 0 || at + 3 >= packer.PaintFloats)
        {
            return null;
        }
        return new Color(
            packer.Paints[at + 0],
            packer.Paints[at + 1],
            packer.Paints[at + 2],
            packer.Paints[at + 3]);
    }

    private static bool Near(float a, float b)
    {
        return Mathf.Abs(a - b) < 0.5f;
    }

    private static string F(float value)
    {
        return value.ToString("0.0", CultureInfo.InvariantCulture);
    }

    private static string Rgb(Color c)
    {
        return $"({F3(c.r)}, {F3(c.g)}, {F3(c.b)})";
    }

    private static string F3(float value)
    {
        return value.ToString("0.000", CultureInfo.InvariantCulture);
    }

    private static string Rgba(Color c)
    {
        return $"({F(c.r)}, {F(c.g)}, {F(c.b)}, {F(c.a)})";
    }

    /// Whether every countable sample centre carries the ink it should.
    ///
    /// **Every centre answers the weak question, and some also answer a
    /// stronger one.** The weak question compares the centre against a screen
    /// corner of the same frame — the clear colour this run produced, since the
    /// camera framing keeps the corner outside the document — and asks only
    /// "something drew here". That is the form the control frame is evaluated
    /// under, where it must be false.
    ///
    /// **A centre also gets the stronger "THIS node drew" only where it has
    /// already passed the weak one and all three of these hold.** Each
    /// exclusion is measured rather than cautious:
    ///
    /// - `albedoReachesThePixel` — false on the two lit classes, whose `DsLit`
    ///   multiplies the albedo by the light, so a correctly drawn node would
    ///   read as uninked below some light intensity.
    /// - The instance is a near-opaque solid fill, so this gate can predict the
    ///   colour at its centre without evaluating the shading arithmetic that
    ///   issue #828's suite judges. A gradient and a translucent fill cannot.
    /// - No later instance's quad reaches that centre, since a document is
    ///   drawn back to front and the pixel would then carry the later node's
    ///   ink. `v03-paint.dsb` has such a pair.
    ///
    /// So the stronger form is what stops a parent frame's fill, or Unity's
    /// magenta error shader, standing in for every child — and the run prints
    /// how many centres reached it, because a change that quietly emptied that
    /// set would leave a character-identical report. Strokes reach neither
    /// form: [`Countable`] excludes them.
    private bool Inked(
        Texture2D frame, bool albedoReachesThePixel, out int inked, out float weakest)
    {
        return Inked(frame, albedoReachesThePixel, out inked, out weakest, out _, out _);
    }

    /// [`Inked`], also reporting the stronger form's headroom and its count.
    ///
    /// **Two thresholds govern this predicate, so it reports two numbers.**
    /// `weakest` is the smallest distance any countable centre kept from the
    /// clear colour, and [`ColourEpsilon`] is what that has to clear — it is
    /// taken over every countable centre, because every one of them is asked
    /// the weak question first. `headroom` is the smallest amount by which a
    /// centre on the stronger form was nearer its own instance's colour than
    /// the clear colour, and the threshold there is zero, not `ColourEpsilon`.
    /// A single reported minimum over both was a number governed by whichever
    /// threshold happened to produce it, printed beside the other one.
    ///
    /// **`perInstance` is not a curiosity.** Every sample the stronger form
    /// excludes falls back to "differs from the clear colour", and nothing in
    /// the printed report used to distinguish the two — so an off-by-one in
    /// `SolidColour`'s row index, or a tightened alpha threshold, would revert
    /// every sample to the weaker test and print a character-identical run.
    /// `headroom` reads `none` when that set is empty rather than a sentinel.
    private bool Inked(
        Texture2D frame,
        bool albedoReachesThePixel,
        out int inked,
        out float weakest,
        out float headroom,
        out int perInstance)
    {
        inked = 0;
        perInstance = 0;
        weakest = float.MaxValue;
        headroom = float.MaxValue;
        var countable = 0;

        // Constant per frame by construction — that is what `Background` is —
        // and `BackgroundDocumentPoint` is what validates it once per run.
        var background = Background(frame);
        foreach (var sample in _samples)
        {
            if (!Countable(sample))
            {
                continue;
            }
            countable++;

            var pixel = Read(frame, sample.Centre);
            var fromBackground = Distance(pixel, background);
            var ok = fromBackground > ColourEpsilon;
            weakest = Mathf.Min(weakest, fromBackground);

            // **The stronger question, and only where the class lets the
            // instance answer it.** A near-opaque solid fill covers its own
            // centre completely, so on a class that puts the albedo on the
            // pixel the value there should be nearer that instance's own colour
            // than the clear colour. Stated as a comparison of two distances
            // rather than as a colour match, because the value read back has
            // been through the pipeline's colour handling and this gate does
            // not model that — a monotonic transfer applied to everything moves
            // both references the same way.
            //
            // **The two lit classes are excluded, and that is not caution.**
            // `DashsceneLighting.hlsl`'s `DsLit` multiplies the albedo by the
            // main light's contribution, which moves the pixel toward the clear
            // colour while leaving both references where they are — so a
            // correctly drawn node would read as uninked below some light
            // intensity, and the gate would blame the painter for the scene's
            // lighting. The lit frames keep the weaker test, which is what
            // issue #1307's discriminator needs anyway: it compares one cutoff
            // against the other, not a pixel against a colour.
            if (ok && albedoReachesThePixel && sample.Solid is { } own)
            {
                // **Reported separately from `weakest`, because a different
                // threshold governs it.** This asks how much nearer the pixel
                // is to the instance's own colour than to the clear colour, and
                // anything above zero passes; the weak question above already
                // held it away from the clear colour by `ColourEpsilon`. Taking
                // one minimum over both quantities printed a number under
                // whichever threshold happened to produce it.
                var toOwn = Distance(pixel, own);
                var advantage = fromBackground - toOwn;
                ok = advantage > 0.0f;
                headroom = Mathf.Min(headroom, advantage);
                perInstance++;
            }

            if (ok)
            {
                inked++;
            }
        }

        if (weakest == float.MaxValue)
        {
            weakest = 0.0f;
        }
        return countable > 0 && inked == countable;
    }

    /// The stronger form's smallest headroom, or `none` where nothing reached
    /// it.
    ///
    /// A run in which no centre reached the stronger form is already refused
    /// further down; this exists so the report says so in words rather than
    /// printing `float.MaxValue` beside a threshold of zero.
    private static string Headroom(float headroom)
    {
        return headroom == float.MaxValue
            ? "none"
            : headroom.ToString("0.000", CultureInfo.InvariantCulture);
    }

    /// The instance's own solid fill colour, where it has one.
    ///
    /// `_DsPaint.x` is the kind and `.y` the heap row; a solid row is four
    /// floats at `row * 4` from `SolidBase`, which `FramePacker` documents as
    /// always zero.
    ///
    /// **Null for anything this gate cannot predict the centre of**: a
    /// gradient, whose colour at a point is the shading arithmetic issue #828's
    /// suite judges; a translucent fill, whose centre is a composite over
    /// whatever is behind it; and a solid a later instance's QUAD reaches.
    /// Those fall back to the weaker "differs from the clear colour" test.
    ///
    /// **That last one is deliberately over-broad.** [`LaterInstanceCovers`]
    /// asks whether a later quad reaches the point, not whether that quad's own
    /// shape inks it — so a solid whose centre merely falls inside a later
    /// node's box loses the stronger form even where nothing is drawn over it.
    /// The alternative is evaluating each later instance's silhouette here,
    /// which is the shading arithmetic this gate exists not to re-implement.
    /// The cost is a smaller stronger-form set, which the run prints.
    ///
    /// A **stroke** never reaches here — [`Countable`] excludes it, because its
    /// box centre carries no ink at all.
    private Color? SolidColour(FramePacker packer, int instance, Vector2 centre)
    {
        if (packer.Paint[(instance * 4) + 0] != (uint)PaintKindTag.FillSolid)
        {
            return null;
        }

        // **Only where no later instance's quad reaches that point** — see
        // [`LaterInstanceCovers`] for what that assumes and why it cannot be
        // widened. `v03-paint.dsb` has the pair this guard was written for:
        // instance 13 is a fill clipped to exactly the box instance 12 draws,
        // and it is packed after it. Without the guard the stronger check
        // passed on instance 12, which means the value read at 12's centre was
        // nearer 12's own colour than the clear colour while 12's ink is
        // covered — so a check meant to say "this node drew" answered for a
        // node whose ink is not what is on the pixel.
        if (LaterInstanceCovers(packer, instance, centre))
        {
            return null;
        }

        var raw = RawSolid(packer, instance);
        if (raw == null)
        {
            return null;
        }
        var colour = raw.Value;
        var alpha = colour.a * packer.Shade[(instance * 4) + 0];
        return alpha >= 0.99f ? colour : (Color?)null;
    }

    /// How many outside-the-corner probes carry ink in one frame.
    private int InkedCorners(Texture2D frame)
    {
        var n = 0;
        foreach (var sample in _samples)
        {
            if (sample.OutsideCorner == null || !Countable(sample))
            {
                continue;
            }
            if (Differs(frame, sample.OutsideCorner.Value))
            {
                n++;
            }
        }
        return n;
    }

    /// Whether a sample is one the gate may demand ink at.
    ///
    /// Three exclusions, each because the node legitimately paints nothing at
    /// its own box centre:
    ///
    /// - a **stroke** instance inks a band around the box and not its middle;
    /// - an instance whose centre a clip box actually excludes;
    /// - a **transparent** instance draws nothing anywhere.
    ///
    /// Excluding them is what keeps a false failure out of the gate. The run
    /// prints how many each rule removed, so an exclusion that swallowed the
    /// whole set is visible rather than silent, and [`Inked`] fails on an empty
    /// set.
    ///
    /// **The opacity rule is the node's, not the fill's, and it is the overlay
    /// class's survival rule reused.** A node at full opacity whose fill colour
    /// has alpha zero draws nothing and would still be demanded to ink; and the
    /// cutout class survives at `shaded.a >= _DsCutoff` rather than above zero,
    /// so a translucent node it legitimately discards would be demanded too.
    /// Neither exists in `goldens/dsb/v03-paint.dsb`. A fixture that grew one
    /// would need this rule split per class.
    private static bool Countable(Sample sample)
    {
        return sample.Kind != (uint)PaintKindTag.Stroke
            && !sample.CentreClipped
            && sample.Opacity > 0.01f;
    }

    private int Sampled()
    {
        var n = 0;
        foreach (var sample in _samples)
        {
            if (Countable(sample))
            {
                n++;
            }
        }
        return n;
    }

    private int CornerProbes()
    {
        var n = 0;
        foreach (var sample in _samples)
        {
            if (Countable(sample) && sample.OutsideCorner != null)
            {
                n++;
            }
        }
        return n;
    }

    /// Which instance's quad reaches the pixel [`Background`] reads, or null
    /// when none does.
    ///
    /// The inverse of [`NothingElseCovers`], asked about the one point every
    /// measurement in the run is taken against. Returns an index so the failure
    /// can name it.
    private int? BackgroundDocumentPoint()
    {
        // **`_painter` too.** This method and `CoveringInstance` read
        // `DocumentToWorld` and `EdgeWidth`, so a run whose painter failed to
        // construct would throw here rather than report — and it would throw
        // from `Judge`, which `Update` calls OUTSIDE its `try`, so nothing
        // would write the report at all. A premise check that throws is worse
        // than one that is skipped.
        if (_probe == null || _camera == null || _painter == null)
        {
            return null;
        }

        // The viewport coordinate `Background` reads, back through the
        // placement: `Read` maps viewport to pixels, so (1, 1) of a Width by
        // Height texture is this fraction of the view.
        var viewport = new Vector3(1.0f / Width, 1.0f / Height, 0.0f);
        var world = _camera.ViewportToWorldPoint(viewport);
        var inverse = _painter.DocumentToWorld.inverse.MultiplyPoint3x4(world);
        var document = new Vector2(inverse.x, inverse.y);

        // Every instance, excluding none: `-1` is an index no instance has.
        return CoveringInstance(_probe, -1, document);
    }

    /// The capture taken for one step, or null with the failure recorded.
    private Texture2D Shot(string label)
    {
        if (_shots.TryGetValue(label, out var shot))
        {
            return shot;
        }
        Fail($"no capture was taken for the '{label}' step.");
        return null;
    }

    /// Whether one point of one frame differs from that frame's own clear
    /// colour.
    private static bool Differs(Texture2D frame, Vector2 viewport)
    {
        return Distance(Read(frame, viewport), Background(frame)) > ColourEpsilon;
    }

    /// The frame's own clear colour, read where the camera framing guarantees
    /// no document.
    private static Color Background(Texture2D frame)
    {
        return frame.GetPixel(1, 1);
    }

    private static Color Read(Texture2D frame, Vector2 viewport)
    {
        var x = Mathf.Clamp(Mathf.RoundToInt(viewport.x * frame.width), 0, frame.width - 1);
        var y = Mathf.Clamp(Mathf.RoundToInt(viewport.y * frame.height), 0, frame.height - 1);
        return frame.GetPixel(x, y);
    }

    private static float Distance(Color a, Color b)
    {
        return Mathf.Max(
            Mathf.Abs(a.r - b.r), Mathf.Max(Mathf.Abs(a.g - b.g), Mathf.Abs(a.b - b.b)));
    }

    private static int DifferingPixels(Texture2D a, Texture2D b)
    {
        var pa = a.GetPixels();
        var pb = b.GetPixels();
        var n = 0;
        for (var i = 0; i < pa.Length; i++)
        {
            if (Distance(pa[i], pb[i]) > ColourEpsilon)
            {
                n++;
            }
        }
        return n;
    }

    private void Line(string text)
    {
        _report.AppendLine(text);
        Debug.Log($"[render-gate] {text}");
    }

    private void Fail(string text)
    {
        _failures.Add(text);
        _report.AppendLine($"FAILURE: {text}");
        Debug.LogError($"[render-gate] FAILURE: {text}");
    }

    private void Finish()
    {
        if (_finished)
        {
            return;
        }
        _finished = true;

        _painter?.Dispose();
        _painter = null;
        _runtime?.Dispose();
        _runtime = null;

        // **`_camera.targetTexture` is deliberately never assigned** — the
        // destination travels on the render request — so there is nothing to
        // clear here, and a line clearing it would read as evidence of the
        // arrangement this file argues against.
        if (_target != null)
        {
            _target.Release();
            _target = null;
        }

        var verdict = _failures.Count == 0 ? "PASS" : $"FAIL with {_failures.Count} problem(s)";
        _report.AppendLine(verdict);
        Debug.Log($"[render-gate] {verdict}");

        try
        {
            File.WriteAllText(Path.Combine(_outDir, "report.txt"), _report.ToString());
        }
        catch (Exception e)
        {
            Debug.LogError($"[render-gate] the report could not be written: {e.Message}");
        }

        Application.Quit(_failures.Count == 0 ? 0 : 1);
    }

    private static string ArgumentAfter(string flag)
    {
        var args = Environment.GetCommandLineArgs();
        for (var i = 0; i < args.Length - 1; i++)
        {
            if (string.Equals(args[i], flag, StringComparison.Ordinal))
            {
                return args[i + 1];
            }
        }
        return null;
    }

    /// Collect the painter's R-E5 warning, if it ever emits one.
    ///
    /// Matched on the sentence rather than on a type or a code, because the
    /// package has neither — `BrgPainter` reports R-E5 with
    /// `Debug.LogWarning` and a message. `unity/package-gate` holds that
    /// message's text on the painter's side, so the two cannot drift apart
    /// silently.
    private void OnPainterLog(string condition, string stackTrace, LogType type)
    {
        if (type == LogType.Warning && condition.Contains("the SRP Batcher is off"))
        {
            _batcherWarnings.Add(condition);
        }
    }

    private void OnDestroy()
    {
        Application.logMessageReceived -= OnPainterLog;

        // Five Unity recorders, and the settle step's sixth. `Finish`
        // deliberately does not release them: it ends the RUN, and this object
        // outlives its own verdict — so a run stopped by a Play-Mode timeout or
        // a scene unload, which reaches this and never reaches `Finish`, is the
        // path that would otherwise leak a native handle per run.
        _threadCost.Dispose();
        if (_settleAlloc.Valid)
        {
            _settleAlloc.Dispose();
        }
        _painter?.Dispose();
        _painter = null;
        _runtime?.Dispose();
        _runtime = null;
    }
}
