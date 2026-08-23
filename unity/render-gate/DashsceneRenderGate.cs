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
//   portable conformance suite.
// - It draws one document, `goldens/dsb/v03-paint.dsb`.
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
using System.Text;
using Driftsys.Dashscene;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Draws a document in a player and checks that ink landed.</summary>
public sealed class DashsceneRenderGate : MonoBehaviour
{
    /// The document, relative to StreamingAssets. Written there by the recipe.
    private const string DocumentFile = "document.dsb";

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
        {
            Label = label;
            MaterialClass = materialClass;
            Draw = draw;
            Cutoff = cutoff;
        }

        /// The name the capture is written under.
        public string Label { get; }

        /// Which of the three classes the painter is on.
        public MaterialClass MaterialClass { get; }

        /// Whether `BrgPainter.Draw` is called at all.
        public bool Draw { get; }

        /// The cutout class's threshold. Ignored by the other two.
        public float Cutoff { get; }
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
    };

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
        if (_framesInStep > SettleFrames)
        {
            Capture(Plan[_step].Label);
            if (!Advance())
            {
                return;
            }
        }

        _framesInStep++;

        try
        {
            _runtime.Tick(Time.deltaTime);
            using (var lease = _runtime.AcquireFrame())
            {
                if (_painter != null && Plan[_step].Draw)
                {
                    _painter.Draw(lease);
                    lease.MarkDrawn();

                    if (_samples == null)
                    {
                        BuildSamples(lease);
                    }
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
            Finish();
            return false;
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
        // cutoff alone never needs a new one. The painter binds its paint heap
        // with `Shader.SetGlobalBuffer`, so two live painters would shade from
        // each other's tables (issue #1297) — disposing before constructing is
        // what keeps exactly one alive.
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
    /// **This ASSUMES draw order is submission order, and nothing here has
    /// confirmed that** — `docs/design/unity-csharp-host.md` names it as an
    /// open gap and this is what now rests on it. The painter emits its draw
    /// commands in rect order inside one `BatchDrawRange` with `allDepthSorted`
    /// false, so a higher index should be drawn later and on top: an earlier
    /// instance cannot change this one's pixel and a later one can.
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

        JudgeCutoff();
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

        var at = (int)(packer.SolidBase + packer.Paint[(instance * 4) + 1]) * 4;
        if (at < 0 || at + 3 >= packer.PaintFloats)
        {
            return null;
        }
        var colour = new Color(
            packer.Paints[at + 0],
            packer.Paints[at + 1],
            packer.Paints[at + 2],
            packer.Paints[at + 3]);
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
        _painter?.Dispose();
        _painter = null;
        _runtime?.Dispose();
        _runtime = null;
    }
}
