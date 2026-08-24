// The render gate's editor half: configure a project that can draw, and build a
// player from it.
//
// **Not part of the package**, like `DashsceneRenderGate.cs` beside it and
// `unity/editor-compat/DashsceneEditorCompat.cs`. `just unity-render` copies
// both into a throwaway project under `target/`.
//
// **Everything here is a host-project requirement rather than a package one**,
// and that is the point of the file: R-E4's render pipeline, R-E5's SRP
// Batcher, R-E6's `m_BrgStripping`. All three were written from Unity's
// documentation and none had been observed doing anything until 2026-08-23. Two
// of them are now measured, and this is where the measurement is repeated on
// every run: at R-E6's default the painter packs and submits every instance and
// draws nothing, which the gate's ink assertion then reports as a blank frame.
//
// It reads back what it set rather than trusting the assignment — an in-memory
// read, which catches a write that did not apply to the object rather than one
// that failed to reach the file. What the file kept is settled downstream: a
// project whose settings did not persist builds a player that draws nothing,
// and the ink check is what reports that.

using System;
using System.Collections.Generic;
using System.IO;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

/// <summary>Builds the render gate's player. `-executeMethod` names `Build`.</summary>
public static class RenderGateBuild
{
    private const string ScenePath = "Assets/Scenes/RenderGate.unity";

    /// The player's product name, which is also its executable's.
    private const string ProductName = "RenderGate";

    /// The package under test, by its UPM name.
    private const string PackageName = "com.driftsys.dashscene";

    /// The window the player opens.
    ///
    /// **Nothing is drawn into it, and the recipe does not open it.**
    /// `DashsceneRenderGate` renders into a `RenderTexture` of its own size and
    /// reads that back, so the framing, the aspect and the capture's resolution
    /// all live there. `just unity-render` then runs the player with
    /// `-batchmode`, which opens no window at all — a windowed player launched
    /// from a shell the window server never composites was measured deadlocking
    /// on a drawable, and `-batchmode` alone keeps the graphics device R-E14
    /// requires.
    ///
    /// These settings are kept so the built player is runnable by hand, with a
    /// window, by a developer who wants to watch it.
    private const int WindowWidth = 1024;

    /// The window's height. See [`WindowWidth`].
    private const int WindowHeight = 768;

    /// <summary>The entry point.</summary>
    public static void Build()
    {
        var failures = new List<string>();
        CreatePipeline(failures);
        RefuseAlwaysIncludedShaders(failures);
        SetBrgStripping(failures);
        CheckPackageNativeLibrary(failures);

        // **Nothing is built once the verdict is already decided.** The player
        // build is the expensive half of a recipe AGENTS.md records as costing
        // tens of minutes, and a run whose project is misconfigured has nothing
        // to learn from drawing.
        if (failures.Count == 0)
        {
            BuildScene();
            BuildPlayer(failures);
        }

        foreach (var failure in failures)
        {
            Debug.LogError($"[render-gate-build] {failure}");
        }
        Debug.Log(
            failures.Count == 0
                ? "[render-gate-build] OK"
                : $"[render-gate-build] FAILED with {failures.Count} problem(s).");
        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }

    /// R-E4 and R-E5: a URP asset, with the SRP Batcher on.
    private static UniversalRenderPipelineAsset CreatePipeline(List<string> failures)
    {
        var renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
        AssetDatabase.CreateAsset(renderer, "Assets/RenderGateRenderer.asset");
        var urp = UniversalRenderPipelineAsset.Create(renderer);
        AssetDatabase.CreateAsset(urp, "Assets/RenderGateURP.asset");

        urp.useSRPBatcher = true;
        EditorUtility.SetDirty(urp);

        GraphicsSettings.defaultRenderPipeline = urp;
        QualitySettings.renderPipeline = urp;
        AssetDatabase.SaveAssets();

        if (GraphicsSettings.currentRenderPipeline == null)
        {
            failures.Add(
                "GraphicsSettings.currentRenderPipeline is still null after assigning a URP "
                + "asset, which is R-E4. BrgPainter refuses to construct without one.");
        }

        // **R-E5 is read on the ASSET here, and on the global in the player.**
        // `UniversalRenderPipelineAsset.useSRPBatcher` is a plain property over
        // the serialised `m_UseSRPBatcher` field the requirement names, so this
        // read is the requirement's own check.
        //
        // `GraphicsSettings.useScriptableRenderPipelineBatching` is NOT, and a
        // first version of this file asserted it here and went red on a project
        // that was correctly configured. That global is assigned in
        // `UniversalRenderPipeline`'s constructor —
        // `GraphicsSettings.useScriptableRenderPipelineBatching =
        // asset.useSRPBatcher`, `UniversalRenderPipeline.cs` — which runs when
        // a pipeline INSTANCE is created, and a batch-mode editor that renders
        // nothing never creates one. So it reads false in this process however
        // the asset is set. `DashsceneRenderGate` asserts it in the player,
        // where a pipeline instance exists and where the painter reads it.
        if (!urp.useSRPBatcher)
        {
            failures.Add(
                "the URP asset's useSRPBatcher reads back false after being set, which is "
                + "R-E5. BatchRendererGroup refuses to draw without the SRP Batcher.");
        }
        Debug.Log(
            "[render-gate-build] URP asset useSRPBatcher "
            + $"{urp.useSRPBatcher} (R-E5); GraphicsSettings."
            + "useScriptableRenderPipelineBatching reads "
            + $"{GraphicsSettings.useScriptableRenderPipelineBatching} in this editor, which "
            + "is not the check — no pipeline instance exists here to assign it.");
        return urp;
    }

    /// **The whole premise of the gate, asserted rather than assumed.**
    ///
    /// Adding the package's shaders to Always Included Shaders is the host-side
    /// workaround issue #1313 records, and a project that applied it would be
    /// checking that workaround rather than the package. This one adds nothing
    /// by hand — but "adds nothing" was a comment on an empty method, and an
    /// empty method is satisfied equally by a Unity template default or a
    /// future editor script putting the shaders there. The run would then
    /// report PASS with `Shader.Find` restored, and the gate would have become
    /// a check of the thing it exists to refuse.
    ///
    /// So the list is read and any entry naming this package is a failure.
    private static void RefuseAlwaysIncludedShaders(List<string> failures)
    {
        var assets = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        if (assets == null || assets.Length == 0)
        {
            failures.Add(
                "ProjectSettings/GraphicsSettings.asset holds no object, so the Always "
                + "Included Shaders list cannot be read and the run's premise is unchecked.");
            return;
        }

        var included = new SerializedObject(assets[0]).FindProperty("m_AlwaysIncludedShaders");
        if (included == null)
        {
            failures.Add(
                "GraphicsSettings has no m_AlwaysIncludedShaders property, so this run "
                + "cannot show that the package's shaders reach the player on their own.");
            return;
        }

        var fromPackage = 0;
        for (var i = 0; i < included.arraySize; i++)
        {
            var shader = included.GetArrayElementAtIndex(i).objectReferenceValue as Shader;
            if (shader == null)
            {
                continue;
            }
            var path = AssetDatabase.GetAssetPath(shader);
            if (path != null && path.Contains(PackageName, StringComparison.Ordinal))
            {
                fromPackage++;
                failures.Add(
                    $"'{shader.name}' ({path}) is in this project's Always Included Shaders. "
                    + "That is issue #1313's host-side workaround, and applying it here would "
                    + "make the run a check of the workaround rather than of the package.");
            }
        }

        // **The count, not the conclusion.** A line reading "none from the
        // package" printed unconditionally beside a failure saying otherwise is
        // the kind of artefact that gets quoted later as evidence.
        Debug.Log(
            $"[render-gate-build] {included.arraySize} always-included shader(s), "
            + $"{fromPackage} from {PackageName}");
    }

    /// R-E6: keep the BatchRendererGroup shader variants.
    private static void SetBrgStripping(List<string> failures)
    {
        var assets = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        if (assets == null || assets.Length == 0)
        {
            failures.Add("ProjectSettings/GraphicsSettings.asset holds no object to edit.");
            return;
        }

        var settings = new SerializedObject(assets[0]);
        var stripping = settings.FindProperty("m_BrgStripping");
        if (stripping == null)
        {
            failures.Add(
                "GraphicsSettings has no m_BrgStripping property, so R-E6 cannot be set. At "
                + "its default of 0 (KeepIfEntitiesGraphics) every BatchRendererGroup variant "
                + "is stripped from a project with no DOTS packages, and the painter submits "
                + "instances that draw nothing.");
            return;
        }

        stripping.intValue = 2;
        settings.ApplyModifiedProperties();
        AssetDatabase.SaveAssets();

        // Read back through a fresh `SerializedObject`, which catches a write
        // `ApplyModifiedProperties` did not apply to the object.
        //
        // **It does not check what the FILE kept**, and saying so matters:
        // `LoadAllAssetsAtPath` returns the same cached in-memory object, so a
        // `SaveAssets` that failed to persist would still read back 2 here.
        // What the file kept is settled downstream — a project whose setting
        // did not persist builds a player that draws nothing, and the ink check
        // reports it. Guarded like the write above, so a load that comes back
        // empty is the named R-E6 failure rather than an
        // IndexOutOfRangeException out of `Build`.
        var reloaded = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        var confirmed = reloaded == null || reloaded.Length == 0
            ? null
            : new SerializedObject(reloaded[0]).FindProperty("m_BrgStripping");
        if (confirmed == null)
        {
            failures.Add(
                "m_BrgStripping could not be read back after being set, so R-E6 is unchecked "
                + "for this run.");
            return;
        }

        var value = confirmed.intValue;
        if (value != 2)
        {
            failures.Add($"m_BrgStripping reads back {value} rather than 2 (KeepAll), R-E6.");
            return;
        }
        Debug.Log("[render-gate-build] m_BrgStripping = 2 (KeepAll), per R-E6");
    }

    /// The package's own native library, checked and never configured.
    ///
    /// **This method used to stage one and set its platform data.** Until story
    /// #1334 the recipe copied a freshly built cdylib into `Assets/Plugins/`
    /// and this method marked it compatible with the active build target — so
    /// the player resolved a library this run had produced, under settings this
    /// file had applied. Both halves hid the question the gate exists to ask:
    /// whether the PACKAGE draws as installed. Issue #1313 is the same class
    /// one layer up, where every gate passed while the package's shaders were
    /// stripped from a player.
    ///
    /// So nothing is set here. The library travels inside the package beside a
    /// committed `.meta`
    /// (`docs/decisions/the-native-library-ships-inside-the-unity-package.md`
    /// D2 and D3), and this asks Unity whether that `.meta` makes it reachable
    /// for the target being built. A failure is a defect in the package rather
    /// than a gap in this project — which is the opposite of what the old
    /// message said.
    private static void CheckPackageNativeLibrary(List<string> failures)
    {
        var packagePlugins = $"Packages/{PackageName}/Runtime/Plugins/";
        var target = EditorUserBuildSettings.activeBuildTarget;

        var found = 0;
        var compatible = new List<string>();
        foreach (var path in AssetDatabase.GetAllAssetPaths())
        {
            if (!path.StartsWith(packagePlugins, StringComparison.Ordinal))
            {
                continue;
            }
            if (AssetImporter.GetAtPath(path) is not PluginImporter importer)
            {
                // Folders under `Plugins/` come back from this enumeration too,
                // and they import through a DefaultImporter.
                continue;
            }

            found++;
            if (importer.GetCompatibleWithPlatform(target))
            {
                compatible.Add(path);
            }
        }

        if (found == 0)
        {
            // Two causes, and they need different remedies: no file at all, or
            // a file Unity did not import as a native plugin. Saying only the
            // first sends a developer to rebuild a library that is already
            // there.
            failures.Add(
                $"no importable native library under {packagePlugins}. Either the package "
                + "ships none — run `just unity-plugins` — or a file is there and Unity "
                + "imported it as something other than a native plugin, which is what a "
                + "missing or wrong `.meta` produces. The gate's player would raise "
                + "DllNotFoundException on its first call into dashscene-ffi.");
            return;
        }

        // **Not every shipped library is for this target, and that is correct.**
        // A macOS player build sees the Android `.so` as well; D3 gives it a
        // `.meta` that excludes it here. What would be wrong is NONE being
        // compatible, which is exactly the Editor-only fallback D2 describes for
        // a library whose `.meta` is missing or wrong.
        if (compatible.Count == 0)
        {
            failures.Add(
                $"the package ships {found} native library(ies) and none is compatible with "
                + $"{target}. That is what a missing or wrong `.meta` produces — D2's "
                + "Editor-platform fallback — and R-E21 is the requirement it breaks.");
            return;
        }

        // **Exactly one, and of the right kind.** Counting compatible libraries
        // and stopping at "more than zero" would let the Android `.so` satisfy
        // a macOS build: `found` is 2, `compatible` is 1, the message reads
        // plausibly, and the run proceeds to a player that fails on ink tens of
        // minutes later. Asking for the extension the target actually loads
        // costs nothing and does not re-derive D3's table here — a third copy
        // of that table is what this file must not become.
        var wanted = ExpectedLibrarySuffix(target);
        if (compatible.Count != 1 || !compatible[0].EndsWith(wanted, StringComparison.Ordinal))
        {
            failures.Add(
                $"{target} needs exactly one compatible native library ending in {wanted}; "
                + $"the package offers [{string.Join(", ", compatible)}]. A library of "
                + "another platform marked compatible with this one builds a player that "
                + "cannot load it.");
            return;
        }

        _shippedLibrary = Path.GetFileName(compatible[0]);
        Debug.Log(
            $"[render-gate-build] {compatible.Count} of {found} shipped native library(ies) "
            + $"compatible with {target}: {_shippedLibrary}, from the package's own .meta");
    }

    /// Asserts the shipped native library is inside the built player.
    ///
    /// **Only the macOS layout is asserted, and the others are reported rather
    /// than skipped.** A player's plugin folder differs per platform, and this
    /// gate runs on macOS — a check that quietly passed on a layout it does not
    /// know would be the fail-open shape this file exists to avoid.
    private static void AssertLibraryReachedThePlayer(
        BuildTarget target, string built, List<string> failures)
    {
        if (target != BuildTarget.StandaloneOSX)
        {
            Debug.Log(
                $"[render-gate-build] the shipped library is NOT asserted inside a {target} "
                + "player: this check knows the macOS layout only.");
            return;
        }

        // **Searched, not composed.** Unity does not put a macOS plugin at the
        // top of `Contents/PlugIns/`: it writes a per-architecture directory
        // and puts it there — `Contents/PlugIns/ARM64/libdashscene_ffi.dylib`
        // on this build. A check that composed the path instead reported the
        // library missing from a player it was sitting inside, which is a
        // false failure on the exact question this gate exists to answer.
        // Asking whether the file is anywhere under the plugin folder is the
        // question; where Unity files it is Unity's business.
        var plugins = Path.Combine(built, "Contents", "PlugIns");
        var landed = Directory.Exists(plugins)
            ? Directory.GetFiles(plugins, _shippedLibrary, SearchOption.AllDirectories)
            : Array.Empty<string>();
        if (landed.Length == 0)
        {
            failures.Add(
                $"the player built, and {_shippedLibrary} is not in {plugins}. Unity copies "
                + "nothing rather than failing the build when the plugin matches no slice of "
                + "the player's architecture, so the first P/Invoke raises "
                + "DllNotFoundException. Issue #1348.");
            return;
        }

        Debug.Log(
            $"[render-gate-build] {_shippedLibrary} reached the player at {landed[0]}");
    }

    /// The file extension a player for <paramref name="target"/> loads.
    private static string ExpectedLibrarySuffix(BuildTarget target) => target switch
    {
        BuildTarget.StandaloneOSX => ".dylib",
        BuildTarget.StandaloneWindows64 => ".dll",
        _ => ".so",
    };

    /// The library <see cref="CheckPackageNativeLibrary"/> accepted, so the
    /// post-build check can look for that exact file rather than for a name it
    /// composes a second time.
    private static string _shippedLibrary;

    /// A camera, a light for the two lit classes, and the gate component.
    ///
    /// **The camera's framing is not set here.** `DashsceneRenderGate` owns the
    /// orthographic size, the position and the aspect, because it also owns the
    /// render target they have to agree with — two files holding the same three
    /// numbers is a drift surface, and the gate is where they are read.
    private static void BuildScene()
    {
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);

        var cameraObject = new GameObject("Main Camera", typeof(Camera));
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.GetComponent<Camera>();
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.15f, 0.15f, 0.18f, 1.0f);

        // **The lit classes need one.** `DsLit` takes the main light's colour
        // and direction; with no light in the scene the cutout class shades to
        // black, which is a colour the gate would still see as ink but which
        // makes every capture harder to read by eye.
        var lightObject = new GameObject("Directional Light", typeof(Light));
        var light = lightObject.GetComponent<Light>();
        light.type = LightType.Directional;
        light.intensity = 1.0f;
        lightObject.transform.rotation = Quaternion.Euler(50.0f, -30.0f, 0.0f);

        new GameObject("RenderGate", typeof(DashsceneRenderGate));

        Directory.CreateDirectory("Assets/Scenes");
        EditorSceneManager.SaveScene(scene, ScenePath);
    }

    private static void BuildPlayer(List<string> failures)
    {
        // Windowed only for a developer running the built player by hand; the
        // recipe passes `-batchmode` and no window is opened. See
        // [`WindowWidth`].
        PlayerSettings.productName = ProductName;
        PlayerSettings.defaultIsNativeResolution = false;
        PlayerSettings.defaultScreenWidth = WindowWidth;
        PlayerSettings.defaultScreenHeight = WindowHeight;
        PlayerSettings.fullScreenMode = FullScreenMode.Windowed;
        PlayerSettings.runInBackground = true;
        PlayerSettings.resizableWindow = false;

        // **The editor's own active target rather than a literal**, so this
        // file does not put the gate out of reach of a developer on Linux or
        // Windows — the reason `just unity-editor` searches for the built-in
        // packages instead of deriving their offset.
        var target = EditorUserBuildSettings.activeBuildTarget;

        // **The player must be built for the architecture the package ships,
        // and this was measured rather than reasoned about.** Unity's default
        // for a macOS player is the universal `x64ARM64`, and D3 ships one
        // macOS library, arm64. A universal build asks for a slice the package
        // does not carry, and Unity's answer is to copy NOTHING: the first run
        // after story #1334 de-staged this gate reported the plugin compatible
        // with StandaloneOSX, built with 0 errors, put no library in
        // `Contents/PlugIns/` at all, and the player raised
        // `DllNotFoundException: dashscene_ffi`.
        //
        // That is worth stating plainly, because it is a hole underneath
        // R-E21: a correct `.meta` is necessary and not sufficient, and no
        // check that reads the tree can see it. This gate is what does.
        //
        // **The pin is a statement about what the package ships, and issue
        // #1348 is where it gets ruled.** D3's macOS row says arm64, so a
        // universal player asks for something this package does not carry —
        // and an integrator building one with Unity's defaults meets the same
        // silence. Whether that row should ship a universal binary instead
        // costs permanent history and is not this gate's call.
        if (target == BuildTarget.StandaloneOSX)
        {
            UnityEditor.OSXStandalone.UserBuildSettings.architecture =
                UnityEditor.Build.OSArchitecture.ARM64;
        }

        var options = new BuildPlayerOptions
        {
            scenes = new[] { ScenePath },
            locationPathName = Path.Combine("Build", ProductName + Extension(target)),
            target = target,
            options = BuildOptions.None,
        };

        BuildReport report;
        try
        {
            report = BuildPipeline.BuildPlayer(options);
        }
        catch (Exception e)
        {
            failures.Add($"the player build threw: {e.GetType().Name}: {e.Message}");
            return;
        }

        Debug.Log(
            $"[render-gate-build] build {report.summary.result}, "
            + $"{report.summary.totalErrors} error(s)");
        if (report.summary.result != BuildResult.Succeeded)
        {
            failures.Add($"the player build did not succeed: {report.summary.result}.");
            return;
        }

        // **Did the library actually reach the player.** Everything above ran
        // before the build and asked the importer a question; this asks the
        // artifact. The two differ, and the difference is measured: at Unity's
        // default universal macOS architecture the importer reported the plugin
        // compatible, the build reported 0 errors, and `Contents/PlugIns/` held
        // no dashscene library at all — the player then raised
        // `DllNotFoundException` on its first call. Without this assertion that
        // failure arrives as a black frame minutes later, indistinguishable
        // from a painter defect. Issue #1348 carries the ruling on the
        // architecture; this is what names the symptom.
        AssertLibraryReachedThePlayer(target, options.locationPathName, failures);
        if (failures.Count > 0)
        {
            return;
        }

        // **The recipe is told where the executable is rather than composing
        // the path itself.** A `.app` on macOS is a directory whose runnable
        // binary sits under `Contents/MacOS/`, a Linux build is the file
        // itself, and a Windows build is a `.exe` — three shapes a shell
        // recipe would have to re-derive from `uname`. Writing it here keeps
        // one place that knows.
        var executable = Runnable(target, options.locationPathName, failures);
        if (executable == null)
        {
            return;
        }
        File.WriteAllText(Path.Combine("Build", "player-path.txt"), executable);
        Debug.Log($"[render-gate-build] player {executable}");
    }

    /// The suffix the built artifact carries on one target.
    private static string Extension(BuildTarget target)
    {
        switch (target)
        {
            case BuildTarget.StandaloneOSX:
                return ".app";
            case BuildTarget.StandaloneWindows:
            case BuildTarget.StandaloneWindows64:
                return ".exe";
            default:
                return string.Empty;
        }
    }

    /// The executable inside the built artifact.
    ///
    /// **Enumerated on macOS rather than composed from the product name.** A
    /// `.app` is a directory and the binary under `Contents/MacOS/` is named by
    /// Unity, so a composed path is a guess that fails as "no such file" rather
    /// than as "the name is not what this file assumed".
    private static string Runnable(BuildTarget target, string built, List<string> failures)
    {
        if (target != BuildTarget.StandaloneOSX)
        {
            return built;
        }

        var dir = Path.Combine(built, "Contents", "MacOS");
        var files = Directory.Exists(dir) ? Directory.GetFiles(dir) : Array.Empty<string>();
        if (files.Length != 1)
        {
            failures.Add(
                $"{dir} holds {files.Length} files; the gate cannot tell which one to run.");
            return null;
        }
        return files[0];
    }
}
