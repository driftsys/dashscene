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
        ImportNativeLibrary(failures);

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

    /// The native library into the player.
    ///
    /// **Set rather than left to the importer's default.** A native plugin's
    /// default platform set is not the same across Unity versions, and one that
    /// does not include the build target produces a player whose first P/Invoke
    /// raises `DllNotFoundException` — which reads as the package shipping no
    /// binary rather than as this project failing to carry one.
    ///
    /// This is the throwaway project's own plugin layout and says nothing about
    /// the package's:
    /// `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D2
    /// and D3 are what decide where a shipped library sits, and R-E21 is what
    /// checks it. Nothing here is that check.
    private static void ImportNativeLibrary(List<string> failures)
    {
        const string Plugins = "Assets/Plugins";
        var libraries = Directory.Exists(Plugins)
            ? Directory.GetFiles(Plugins, "*.*", SearchOption.TopDirectoryOnly)
            : Array.Empty<string>();

        var imported = 0;
        foreach (var path in libraries)
        {
            if (path.EndsWith(".meta", StringComparison.Ordinal))
            {
                continue;
            }
            if (AssetImporter.GetAtPath(path.Replace('\\', '/')) is not PluginImporter importer)
            {
                failures.Add($"{path} did not import as a native plugin.");
                continue;
            }

            importer.SetCompatibleWithAnyPlatform(false);
            importer.SetCompatibleWithEditor(true);
            importer.SetCompatibleWithPlatform(EditorUserBuildSettings.activeBuildTarget, true);
            importer.SaveAndReimport();
            imported++;
        }

        if (imported == 0)
        {
            failures.Add(
                $"no native library under {Plugins}. The gate's player would raise "
                + "DllNotFoundException on its first call into dashscene-ffi.");
            return;
        }
        Debug.Log($"[render-gate-build] {imported} native plugin(s) enabled for "
                  + $"{EditorUserBuildSettings.activeBuildTarget}");
    }

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
