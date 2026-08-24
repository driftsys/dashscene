// The showcase: several documents, one at a time, drawn by the Unity painter.
//
// **What this is.** `FrameLoop` beside it is the smallest thing that draws one
// document — the shape to copy into a host. This one is the demonstration: it
// reads a manifest of documents from `StreamingAssets`, switches between them
// on a key, and reports on screen what the painter did with each. It is the
// Unity counterpart of `demo/`, `demo-web/` and `demo-android/`, and it is
// deliberately narrower than they are. Issue #1329.
//
// **Three things it does not show, and none of them is an oversight.**
//
// - **No signal sweep and no variant switch.** The three Rust hosts drive both
//   every frame — `ScenePulse` sets a signal, `SceneAction` runs a variant
//   switch. Not one entry point in `crates/dashscene-ffi/include/dashscene.h`
//   mutates a document: a host loads, ticks, acquires a frame and draws. The
//   count is deliberately not stated here — it moved twice in one slice, and
//   `unity/ffi-check` is what holds the set. Signal binding is layer 1 and is `v1` for every
//   host by the ruling of 2026-08-18 (issues #1261 and #1262), so this is a
//   property of the boundary rather than of this file.
// - **Not the showcase scenes.** `corpus/showcase` builds its scenes in Rust
//   code against a live arena, and nothing in the repository emits them as a
//   `.dsb`. What this draws is committed documents. Issue #1329's comment
//   carries the three ways that could change.
// - **No shipped native library.** The package ships no binary; the recipe that
//   builds this demo stages one into the project. Issue #1334 is the shipped
//   form.
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
using Driftsys.Dashscene;
using UnityEngine;

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

        private readonly List<ShowcaseEntry> _entries = new List<ShowcaseEntry>();
        private DashsceneRuntime _runtime;
        private BrgPainter _painter;
        private CommitPacer _pacer;
        private int _index;
        private string _status = string.Empty;
        private GUIStyle _readout;

        private int _readoutHeight;

        /// Seconds between automatic switches, from `-cycle <seconds>` on the
        /// command line. Zero leaves switching to the arrow keys.
        ///
        /// **A demonstration takes a hand on a key; a check cannot.** Without
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

        private void Awake()
        {
            _pacer = new CommitPacer(commitHz);

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

            _cycleSeconds = CycleSecondsFromCommandLine();
            _quitWhenEveryEntryHasDrawn =
                Array.IndexOf(Environment.GetCommandLineArgs(), "-quit") >= 0;
            LoadManifest();

            if (_entries.Count == 0)
            {
                Fail($"no documents: {manifestPath} lists none. It is the only thing read — "
                     + "nothing scans the directory beside it.");
                return;
            }

            Show(0);
        }

        private void Update()
        {
            if (_failed || _painter == null || _entries.Count == 0)
            {
                return;
            }

            ReadInput();

            if (_cycleSeconds > 0.0f && _entries.Count > 1)
            {
                _sinceSwitch += Time.deltaTime;
                if (_sinceSwitch >= _cycleSeconds)
                {
                    Show((_index + 1) % _entries.Count);
                }
            }

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
                _runtime.Tick(dt);

                using var frame = _runtime.AcquireFrame();
                _painter.Draw(frame);

                if (!_reported)
                {
                    // Once per document rather than once per frame: what a run
                    // needs is that each one reached the painter, and a line a
                    // frame would bury it.
                    _reported = true;
                    _drawnEntries.Add(_index);
                    Debug.Log($"[showcase] drew {Label(_entries[_index])}: "
                              + $"{_painter.InstanceCount} instance(s), rung {_painter.Rung}"
                              + (_painter.Diagnostics.IsClean
                                  ? string.Empty
                                  : $", refused {_painter.Diagnostics}"));
                    AnnounceIfEveryEntryHasDrawn();
                }

                // `Draw` packs and uploads; whether the frame reached a screen
                // is Unity's answer, later. A lease disposed unmarked leaves
                // every commit unshown.
                frame.MarkDrawn();
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

        private void ReadInput()
        {
            // Left and right only, which is what the readout and every
            // document about this sample say. A third binding named nowhere is
            // a smaller version of the same defect.
            if (Input.GetKeyDown(KeyCode.RightArrow))
            {
                Show((_index + 1) % _entries.Count);
            }
            else if (Input.GetKeyDown(KeyCode.LeftArrow))
            {
                Show((_index + _entries.Count - 1) % _entries.Count);
            }
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
            _reported = false;
            var entry = _entries[index];

            // **Refused before a runtime is minted**, because this is a
            // property of the manifest rather than of the load: the loader
            // that takes a font cascade takes no root ordinal (issue #1332),
            // so a non-zero root on a text entry cannot be honoured whatever
            // the runtime does.
            if (entry.text && entry.shownRoot != 0)
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
                     + "This package ships no binary — `just unity-demo` stages one, and "
                     + "issue #1334 is the shipped form.");
                return;
            }
            catch (Exception e)
            {
                Fail($"the runtime could not be created: {e.Message}");
                return;
            }

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

            _status = string.Empty;
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

        private void LoadManifest()
        {
            var path = Path.Combine(Application.streamingAssetsPath, manifestPath);
            if (!File.Exists(path))
            {
                // Said here rather than collapsed into "lists none" below: a
                // manifest that is absent and one that is empty are different
                // mistakes, and the reader fixes them differently.
                Fail($"{path} does not exist. The recipe writes it beside the documents; "
                     + "a player built by hand needs it written by hand.");
                return;
            }

            ShowcaseManifest manifest;
            try
            {
                manifest = JsonUtility.FromJson<ShowcaseManifest>(File.ReadAllText(path));
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
            if (_announcedEveryEntry || _drawnEntries.Count < _entries.Count)
            {
                return;
            }

            _announcedEveryEntry = true;
            Debug.Log($"[showcase] all {_entries.Count} document(s) drew");

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

        private static string Label(ShowcaseEntry entry)
        {
            return entry == null
                ? "no document"
                : string.IsNullOrEmpty(entry.label)
                    ? Path.GetFileName(entry.path)
                    : entry.label;
        }

        private void OnGUI()
        {
            var entry = _entries.Count > 0 ? _entries[_index] : null;
            var label = Label(entry);

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
                $"{label}   [{_index + 1}/{Math.Max(_entries.Count, 1)}]   "
                + "left/right to switch",
                $"rung {_painter?.Rung.ToString() ?? "none"}   "
                + $"{_painter?.InstanceCount ?? 0} instances",
            };

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
