// The frame loop that drives a dashscene runtime from Unity.
//
// **Why this is a sample and not `Runtime/` code.**
// `docs/specification/07-embedding-and-distribution.md` R-E10 requires every C#
// type under `Runtime/` to compile against `netstandard.dll` 2.1.0, and names
// `unity/package-compat` as its check. That project has no Unity reference
// assemblies and CI runs no editor, so a `MonoBehaviour` under `Runtime/` fails
// R-E10's own check. `Samples~` is hidden from Unity's importer by its `~`
// suffix — one of the four shapes R-E2 enumerates — so it needs no `.meta` and
// is outside the compile gate.
//
// What is left here is the part that genuinely needs an editor:
// `Time.deltaTime`, a component lifecycle, and where the painter hangs.
// Everything carrying a claim lives in `Runtime/`, where a gate compiles it —
// `DocumentRange` and the loader in the engine-free half, which
// `just unity-ffi` executes against the real library, and
// `StreamingAssetDocument` in `Runtime/Engine/`, which `just unity-editor`
// compiles (`docs/decisions/r-e10-is-checked-in-two-halves.md`).
//
// **It draws.** `AcquireFrame` hands over the committed tables and `BrgPainter`
// consumes them, which is issue #1298's wiring. What it does not draw is
// reported by name through `PackDiagnostic` — the painter logs each one when
// the set changes, and `BrgPainter.Diagnostics` carries it for a host that
// wants to read rather than watch. Take the list from there rather than from a
// copy here: the copy this replaces named text, and went stale the day story
// #1123 landed the text seam.
//
// **`PackDiagnostic` reports what the painter declined to draw, not what never
// reached it**, and this sample's text falls in the second class. See below:
// with no font cascade nothing is staged, so `PackDiagnostic.GlyphRun` never
// sets and a document's text disappears with a clean console.
//
// **Text is the package's, and not yet this sample's.** Story #1123 landed the
// seam: the glyph atlas crosses the C ABI and `Dashscene/Text` draws the runs.
// Two things here stop it, not one. This component loads with
// `LoadDocumentMapped` rather than `LoadDocumentWithText`, so with no font
// cascade a document's text shapes to nothing and no glyph run is staged at
// all; and the painter samples an atlas set the HOST installs, which this
// component never does — it calls neither `DashsceneRuntime.ReadAtlases` nor
// `BrgPainter.SetAtlases`. Issue #1337 carries both, and it is not the two-call
// change it looks like: a sample needs somewhere to get fonts from first.
//
// **Two host-project settings decide whether anything appears**, and neither is
// this component's to set: R-E5's `m_UseSRPBatcher` on the active render
// pipeline asset, and R-E6's `m_BrgStripping` of `2`. At R-E6's default of `0`
// the painter packs and submits every instance and nothing is drawn, while
// Unity logs `Trying to render a BatchRendererGroup batch with wrong cbuffer
// setup. Missing DOTS_INSTANCING_ON variant?` on every frame — measured on
// 2026-08-23, macOS/Metal, Unity 6000.3.22f1.

using System;
using System.IO;
using Driftsys.Dashscene;
using UnityEngine;

namespace Driftsys.Dashscene.Samples
{
    /// Loads a `.dsb`, ticks it, and takes the committed frame each Unity frame.
    [DisallowMultipleComponent]
    public sealed class DashsceneFrameLoop : MonoBehaviour
    {
        // **On Android, StreamingAssets is not a filesystem path.** It resolves
        // to a `jar:file://…!/assets` URI inside the APK.
        // `StreamingAssetDocument.Resolve` is what turns a relative path into
        // something loadable on both shapes of platform, and story #1124 is why
        // it does it without a copy.
        [Tooltip("Absolute path to a .dsb, or a path relative to StreamingAssets. "
                 + "A relative path works on Android too — the document is mapped "
                 + "in place inside the APK.")]
        [SerializeField]
        private string documentPath = "scene.dsb";

        [Tooltip("Document ordinal of the artboard to show. Named once, at load.")]
        [SerializeField]
        private uint shownRoot;

        [Tooltip("Commits per second. 0 commits once per Unity frame.")]
        [SerializeField]
        private int commitHz;

        [Tooltip("The camera the document is drawn for. Unassigned takes Camera.main. "
                 + "An orthographic camera is what BrgPainter.EdgeWidth is derived from; "
                 + "under a perspective one the painter keeps its own default.")]
        [SerializeField]
        private Camera viewCamera;

        private DashsceneRuntime _runtime;
        private CommitPacer _pacer;
        private BrgPainter _painter;

        private void Awake()
        {
            // Captured once. An Inspector edit to commitHz during play does not
            // take effect until the component is re-enabled, which is fine for a
            // sample and worth knowing before copying it.
            _pacer = new CommitPacer(commitHz);

            // The runtime is thread-affine to whichever thread minted it, and
            // every MonoBehaviour callback runs on Unity's main thread — so
            // creating it here is what makes Update, LateUpdate and OnDestroy
            // all legal call sites. Creating it on a worker would strand it.
            try
            {
                _runtime = new DashsceneRuntime();
            }
            catch (DashsceneAbiMismatchException e)
            {
                // R-E16: refuse rather than proceed, and report both numbers.
                Debug.LogError($"[dashscene] {e.Message}", this);
                enabled = false;
                return;
            }
            catch (DllNotFoundException)
            {
                // **The failure a customer actually meets first.** This package
                // ships no native library, so the first call into it — the
                // version handshake inside the constructor — is where a fresh
                // Git-URL install lands. Left uncaught it is a bare loader
                // stack trace naming `ds_abi_version`, while the rarer version
                // mismatch above gets a sentence explaining itself.
                Debug.LogError(
                    $"[dashscene] the native library '{DashsceneRuntime.LibraryName}' was not "
                    + "found. This package ships no binary: build one with `just host-lib` and "
                    + "place it under Runtime/Plugins/<platform>/ with a .meta declaring the "
                    + "platform and CPU. See the package README.",
                    this);
                enabled = false;
                return;
            }
            catch (DashsceneException e)
            {
                Debug.LogError($"[dashscene] the runtime could not be created: {e.Message}", this);
                enabled = false;
                return;
            }

            // **After the runtime and before the document**, which is the
            // order a developer meets the failures in: a process with no
            // graphics device or no render pipeline cannot draw whatever it
            // loads, and both are project configuration rather than content.
            try
            {
                // The overlay class: the one of the three that expresses
                // partial coverage, so corners, strokes and clip edges are
                // anti-aliased. `MaterialClass` is a host choice — nothing on
                // boundary B says which node is lit.
                _painter = new BrgPainter(MaterialClass.UnlitOverlay);
            }
            catch (DashscenePainterException e)
            {
                // R-E4 and R-E14 both land here: no ScriptableRenderPipeline is
                // active, or the process holds no graphics device. A developer
                // meets one of them before anything else, and the message says
                // which.
                Debug.LogError($"[dashscene] the painter could not be created: {e.Message}", this);
                enabled = false;
                return;
            }

            if (_painter.Rung == BrgRung.InstancedWithoutBrg)
            {
                // **R-E19's rung, and nothing is built for it.** The
                // constructor selects it where `BatchRendererGroup` is
                // unsupported and returns without building a group, so `Draw`
                // returns before it binds anything and every frame is blank,
                // without an exception. **R-E6's default produces a blank frame
                // too**: Unity itself logs the missing `DOTS_INSTANCING_ON`
                // variant on every frame, measured on 2026-08-23 and recorded
                // at the top of this file. The two look identical on screen and
                // differ entirely in the console, so each blank frame has to
                // name itself. Since issue #1326 the painter warns on this arm
                // of its own constructor; this component escalates to an error
                // and stops, because a sample that draws nothing every frame
                // has nothing left to demonstrate.
                Debug.LogError(
                    $"[dashscene] the painter is on rung {_painter.Rung} — "
                    + "BatchRendererGroup is unsupported on this graphics API, and nothing "
                    + "is built for that rung, so every frame would be blank. The painter "
                    + "warns about this too; this component stops rather than drawing nothing "
                    + "for the rest of the run (docs/decisions/unity-painter-uses-brg.md D3).",
                    this);
                enabled = false;
                return;
            }

            // **The document's y runs down**, so scaling y by -1 is the
            // identity placement: one document unit on one world unit, the
            // document's origin at the world origin, and its y axis pointing
            // down — which is what a camera looking along +z sees upright.
            _painter.DocumentToWorld = Matrix4x4.Scale(new Vector3(1, -1, 1));

            // **`GlobalBounds` is left at the painter's own default**, which is
            // a 10000-unit cube centred on the world origin. A host that knows
            // its document's extent should set it; this component cannot,
            // because nothing on boundary B reports the shown root's size, and
            // a bound guessed from nothing is worse than a generous one. A
            // document reaching past 5000 units from the origin needs it set,
            // and the symptom of not setting it is a document that vanishes
            // when the camera moves.

            DocumentRange range;
            try
            {
                range = StreamingAssetDocument.Resolve(documentPath);
            }
            catch (Exception e)
            {
                // Broad on purpose, and it is the one place on this path that
                // is. `AndroidJavaException` is what a JNI call throws and it
                // cannot be named in a build where `UNITY_ANDROID` is not
                // defined, so a narrower list here would compile on one
                // platform and not the other.
                Debug.LogError(
                    $"[dashscene] could not locate {documentPath}: {e.Message}", this);
                enabled = false;
                return;
            }

            try
            {
                // The mapped path rather than the owning one: it costs the
                // artboard being shown rather than the whole file, and on
                // Android it is what keeps demand paging (story #1124).
                _runtime.LoadDocumentMapped(range, shownRoot);
            }
            catch (DashsceneAbiMismatchException e)
            {
                // **A second catch for this, at the load site, and it is not
                // redundant with the one around the constructor.**
                // `DashsceneSymbolMissingException` is the library missing an
                // entry point that `ds_abi_version` agreed about, so it arrives
                // at the first call to that symbol rather than at the
                // handshake — and it derives from `Exception`, not from
                // `DashsceneException`, so the catch below does not see it.
                Debug.LogError($"[dashscene] {e.Message}", this);
                enabled = false;
            }
            catch (DashsceneException e)
            {
                Debug.LogError($"[dashscene] could not load {range}: {e.Message}", this);
                enabled = false;
            }

            WarnIfCommitRateDoesNotDivideTheDisplay();
        }

        private void Update()
        {
            // **Both, not just the runtime.** `enabled` is set false on every
            // failure path in `Awake`, and a developer or a script can set it
            // back to true — at which point a null painter would be a
            // NullReferenceException per frame rather than a silent no-op.
            if (_runtime == null || _painter == null)
            {
                return;
            }

            if (!_pacer.ShouldCommit(Time.deltaTime, out var dt))
            {
                return;
            }

            // Re-derived on every commit rather than once: the value depends
            // on the screen's height and the camera's size, and a window resize
            // or an orthographic-size tween changes both without this component
            // hearing about it. **On every commit and not every frame** — it
            // sits below the pacer's early return, so at a reduced commit rate
            // a resize is picked up at the next commit rather than the next
            // frame. That is the right side of the trade for a sample: the
            // value is only ever used by the `Draw` below it, which is on the
            // same cadence.
            UpdateEdgeWidth();

            try
            {
                _runtime.Tick(dt);

                // Acquire every committed frame rather than only the advanced
                // ones: a host that skipped would never mark a commit shown, so
                // a settled scene would keep reporting that it advanced.
                using var frame = _runtime.AcquireFrame();

                if (frame.DocumentReplaced)
                {
                    // Every rect index you cached names something else now.
                    OnDocumentReplaced();
                }

                _painter.Draw(frame);

                // **`Draw` does not mark it, and the caller must.** The painter
                // packs and uploads; whether the frame reached a screen is
                // decided by Unity, later. A lease disposed unmarked leaves
                // every commit unshown, so a settled document reports that it
                // advanced forever and this loop re-acquires and re-packs
                // frames that will never change.
                frame.MarkDrawn();
            }
            catch (DashsceneAbiMismatchException e)
            {
                // **A third catch for this type, and the frame loop is where it
                // actually arrives.** `DashsceneSymbolMissingException` is the
                // library missing an entry point that `ds_abi_version` agreed
                // about, so it is raised at the first call to that symbol —
                // and `ds_runtime_acquire_frame` arrived at story #859, which
                // puts it squarely in the population a package newer than its
                // library can miss. The two catches in `Awake` cannot see it,
                // because the call is here; `catch (DashsceneException)` below
                // cannot either, because this type derives from `Exception`.
                // Without this, the sample throws once per frame rather than
                // reporting once and disabling itself, which is what it does
                // for every other failure it names. Issue #1315.
                Debug.LogError($"[dashscene] {e.Message}", this);
                enabled = false;
            }
            catch (DashscenePainterException e)
            {
                // **`DashscenePainterException` derives from `Exception`, not
                // from `DashsceneException`**, so the catch below does not see
                // it — the same shape as the load site's second catch above.
                // `Draw` reaches it through `EnsureCapacity`, which refuses a
                // constant-buffer window it cannot fit an instance into. That
                // is the `ConstantBuffer` rung, which no gate and no device in
                // this repository has exercised, so an uncaught throw here
                // would be once per frame on the one path nothing has tested.
                Debug.LogError($"[dashscene] the painter refused the frame: {e.Message}", this);
                enabled = false;
            }
            catch (DashsceneStrideMismatchException e)
            {
                // R-E17: refuse to read rows whose layout is not the one this
                // package declares, rather than drawing wrong geometry.
                Debug.LogError($"[dashscene] {e.Message}", this);
                enabled = false;
            }
            catch (DashsceneException e)
            {
                Debug.LogError($"[dashscene] frame failed: {e.Message}", this);
                if (e.Status == DsStatus.Panic)
                {
                    // The library is in an unspecified state: free it and make
                    // no further calls.
                    enabled = false;
                    Teardown();
                }
            }
        }

        private void OnDestroy()
        {
            Teardown();
        }

        private void Teardown()
        {
            // **The painter before the runtime.** It holds no runtime handle,
            // but it reads the borrowed tables the runtime owns, and disposing
            // the owner of a mapping while something may still read it is the
            // ordering worth stating rather than the one worth discovering.
            _painter?.Dispose();
            _painter = null;

            // On the main thread, which is where Awake minted it. There is no
            // finalizer on DashsceneRuntime precisely because the GC's thread
            // is not this one and a free from there would be refused.
            if (_runtime == null)
            {
                return;
            }

            _runtime.Dispose();
            if (_runtime.LastDisposeStatus != DsStatus.Ok)
            {
                // Dispose does not throw — it is called during unwinding — so
                // the host is what reports a runtime that was not freed.
                Debug.LogError(
                    $"[dashscene] the runtime was not freed: {_runtime.LastDisposeStatus}. "
                    + _runtime.LastDisposeDetail,
                    this);
            }

            _runtime = null;
        }

        /// The rect indices have been renumbered by a load.
        ///
        /// **Empty, and correctly so for this component.** `BrgPainter` repacks
        /// every rect from the committed tables on every `Draw` and caches
        /// nothing keyed by rect index, so it needs no notification. A host
        /// that anchors its own objects to rect indices is the one this hook is
        /// for.
        private void OnDocumentReplaced()
        {
        }

        /// Set the painter's anti-aliasing ramp to one device pixel.
        ///
        /// [`BrgPainter.EdgeWidth`] is in the document's own units, and the
        /// placement above puts one document unit on one world unit — so the
        /// value wanted is world units per device pixel. An orthographic camera
        /// reports that directly: its `orthographicSize` is half the view's
        /// height in world units.
        ///
        /// **A perspective camera cannot answer it**, because the ramp's width
        /// in world units varies with a fragment's depth and one scalar cannot
        /// express that. The painter's own default is left in place there,
        /// which draws a ramp one document unit wide.
        private void UpdateEdgeWidth()
        {
            var camera = viewCamera != null ? viewCamera : Camera.main;
            if (camera == null || !camera.orthographic || camera.pixelHeight <= 0)
            {
                return;
            }

            _painter.EdgeWidth = camera.orthographicSize * 2.0f / camera.pixelHeight;
        }

        /// **Pick a divisor of the display rate** (issue #851, issue #1121).
        ///
        /// At 60 Hz a 16 Hz commit lands on 4, 4, 4, 3 frames alternating — an
        /// uneven cadence on top of a low rate. 15 and 20 divide exactly.
        ///
        /// The other half of that rule cannot be checked from here and belongs
        /// to whatever anchors content to this scene: **anchored objects must
        /// read the committed rects, never their own interpolator.** A
        /// host-side tween running at full rate inside a reduced-rate surface
        /// produces relative motion that looks worse than either rate alone.
        private void WarnIfCommitRateDoesNotDivideTheDisplay()
        {
            if (commitHz <= 0)
            {
                return;
            }

            var refresh = (int)Math.Round(Screen.currentResolution.refreshRateRatio.value);
            var lower = CommitPacer.NearestDivisor(refresh, commitHz);
            if (lower != commitHz)
            {
                Debug.LogWarning(
                    $"[dashscene] a commit rate of {commitHz} Hz does not divide the display's "
                    + $"{refresh} Hz, so commits land on an uneven number of frames. "
                    + $"{lower} Hz divides it exactly.",
                    this);
            }
        }
    }
}
