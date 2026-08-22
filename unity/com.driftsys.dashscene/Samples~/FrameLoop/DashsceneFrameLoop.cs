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
// Everything this file does that is worth testing lives in `Runtime/` and is
// executed by `just unity-ffi`. What is left here is the part that genuinely
// needs an editor: `Time.deltaTime`, a component lifecycle, and a place to put
// the painter when story #1122 lands.
//
// **This draws nothing.** `AcquireFrame` hands over the committed tables and
// the painter that consumes them is story #1122; the glyph atlases those runs
// sample do not cross the ABI at all until story #1123.

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
        [Tooltip("Absolute path to a .dsb, or a path under Application.streamingAssetsPath.")]
        [SerializeField]
        private string documentPath = "scene.dsb";

        [Tooltip("Document ordinal of the artboard to show. Named once, at load.")]
        [SerializeField]
        private uint shownRoot;

        [Tooltip("Commits per second. 0 commits once per Unity frame.")]
        [SerializeField]
        private int commitHz;

        private DashsceneRuntime _runtime;
        private CommitPacer _pacer;

        private void Awake()
        {
            // The runtime is thread-affine to whichever thread minted it, and
            // every MonoBehaviour callback runs on Unity's main thread — so
            // creating it here is what makes Update, LateUpdate and OnDestroy
            // all legal call sites. Creating it on a worker would strand it.
            _pacer = new CommitPacer(commitHz);

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

            var path = Path.IsPathRooted(documentPath)
                ? documentPath
                : Path.Combine(Application.streamingAssetsPath, documentPath);

            try
            {
                // The mapped path rather than the owning one: it costs the
                // artboard being shown rather than the whole file, and on
                // Android it is what keeps demand paging (story #1124).
                _runtime.LoadDocumentMapped(path, shownRoot);
            }
            catch (DashsceneException e)
            {
                Debug.LogError($"[dashscene] could not load {path}: {e.Message}", this);
                enabled = false;
            }

            WarnIfCommitRateDoesNotDivideTheDisplay();
        }

        private void Update()
        {
            if (_runtime == null)
            {
                return;
            }

            if (!_pacer.ShouldCommit(Time.deltaTime, out var dt))
            {
                return;
            }

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

                // Story #1122's painter reads frame.Frame here and calls
                // frame.MarkDrawn() when it has. Until then nothing paints, so
                // the commit is deliberately NOT marked shown — claiming it was
                // drawn would make a settled scene stop reporting that it has
                // something worth drawing.
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
            // On the main thread, which is where Awake minted it. There is no
            // finalizer on DashsceneRuntime precisely because the GC's thread
            // is not this one and a free from there would be refused.
            _runtime?.Dispose();
            _runtime = null;
        }

        /// Hook for story #1122: rect indices have been renumbered.
        private void OnDocumentReplaced()
        {
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
