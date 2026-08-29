// Builds the Unity showcase player. `-executeMethod` names `Build`.
//
// **This is a fourth copy of the throwaway-project bring-up**, after
// `unity/render-gate/RenderGateBuild.cs` and the two recipes that configure a
// project inline. It is a copy on purpose: issue #1316 carries factoring the
// duplication out of all of them together, and a shared helper written from one
// lane can break the others silently — the note at the top of the `unity-render`
// recipe states that rule. What differs here is the point: the gate asserts and
// exits, this one builds something a person runs.
//
// **The assertions are the gate's, kept rather than trimmed.** R-E4 and R-E5
// (a URP asset with the SRP Batcher on), R-E6 (`m_BrgStripping` at KeepAll) and
// the refusal to add the package's shaders to Always Included Shaders all stay,
// because a demonstration that quietly worked around a packaging defect would
// be worse than no demonstration: issue #1313 was exactly that class, and the
// player build is what found it.
//
// What it does NOT do is anything about the shipped plugin layout. The library
// is staged into `Assets/Plugins` by the recipe, which says nothing about where
// a released package carries one —
// `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D2 and
// D3 decide that and issue #1334 is the work.

using System;
using System.Collections.Generic;
using System.IO;
using Driftsys.Dashscene.Samples;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

/// <summary>Builds the showcase player.</summary>
public static class DemoBuild
{
    private const string ScenePath = "Assets/Scenes/Showcase.unity";

    private const string ProductName = "DashsceneShowcase";

    private const string PackageName = "com.driftsys.dashscene";

    /// The Android application id, written to `Build/application-id.txt` so the
    /// recipe launches what was built rather than a literal of its own — the
    /// rule `AndroidProbeBuild` already follows, and the failure it avoids is
    /// `am` reporting "No activities found to run" into a stream the recipe
    /// would then blame on the device.
    private const string ApplicationId = "com.driftsys.dashscene.showcase";

    /// Which player to build, read from the environment.
    ///
    /// **An environment variable rather than `-buildTarget`**, for
    /// `AndroidProbeBuild`'s reason: the switch has to happen before anything
    /// reads a per-platform setting, and doing it here rather than trusting the
    /// command line's ordering is what makes that observable.
    private static bool BuildingForAndroid =>
        Environment.GetEnvironmentVariable("DASHSCENE_DEMO_TARGET") == "android";

    private const int WindowWidth = 1280;

    private const int WindowHeight = 800;

    /// **The framing is a guess, and it has to be.** Nothing on boundary B
    /// reports the shown root's extent, so neither this scene nor the sample
    /// can read a document's size — the same reason `BrgPainter.GlobalBounds`
    /// is left at its default. These numbers frame the committed documents in
    /// `goldens/dsb/`, which are a few hundred units on a side, and a document
    /// larger than that is off screen rather than mis-drawn.
    private const float OrthographicSize = 400.0f;

    public static void Build()
    {
        var failures = new List<string>();

        if (BuildingForAndroid
            && EditorUserBuildSettings.activeBuildTarget != BuildTarget.Android
            && !EditorUserBuildSettings.SwitchActiveBuildTarget(
                NamedBuildTarget.Android, BuildTarget.Android))
        {
            failures.Add(
                "the active build target could not be switched to Android. Android Build "
                + "Support is not installed in this editor, and everything below would then "
                + "configure and build a macOS player instead.");
            Debug.LogError($"[demo-build] {failures[0]}");
            EditorApplication.Exit(1);
            return;
        }

        CreatePipeline(failures);
        RefuseAlwaysIncludedShaders(failures);
        if (BuildingForAndroid)
        {
            SetAndroidPlayerSettings(failures);
        }

        SetBrgStripping(failures);
        ImportNativeLibrary(failures);

        // Nothing is built once the verdict is decided: the player build is the
        // expensive half, and a misconfigured project has nothing to show.
        if (failures.Count == 0)
        {
            BuildScene();
            BuildPlayer(failures);
        }

        foreach (var failure in failures)
        {
            Debug.LogError($"[demo-build] {failure}");
        }
        Debug.Log(
            failures.Count == 0
                ? "[demo-build] OK"
                : $"[demo-build] FAILED with {failures.Count} problem(s).");
        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }

    /// R-E4 and R-E5: a URP asset, with the SRP Batcher on.
    private static void CreatePipeline(List<string> failures)
    {
        var renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
        AssetDatabase.CreateAsset(renderer, "Assets/ShowcaseRenderer.asset");
        var urp = UniversalRenderPipelineAsset.Create(renderer);
        AssetDatabase.CreateAsset(urp, "Assets/ShowcaseURP.asset");

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

        // R-E5 is read on the ASSET, not on
        // `GraphicsSettings.useScriptableRenderPipelineBatching`: that global is
        // assigned when a pipeline INSTANCE is created, which a batch-mode
        // editor never does, so it reads false here however the asset is set.
        // The gate's own file records the measurement behind that.
        if (!urp.useSRPBatcher)
        {
            failures.Add(
                "the URP asset's useSRPBatcher reads back false after being set, which is "
                + "R-E5. BatchRendererGroup refuses to draw without the SRP Batcher.");
        }
    }

    /// The demo runs the package as installed, so the host-side workaround for
    /// issue #1313 must not be present: a project that added the package's
    /// shaders by hand would demonstrate the workaround rather than the package.
    private static void RefuseAlwaysIncludedShaders(List<string> failures)
    {
        var assets = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        if (assets == null || assets.Length == 0)
        {
            failures.Add("ProjectSettings/GraphicsSettings.asset holds no object to read.");
            return;
        }

        var included = new SerializedObject(assets[0]).FindProperty("m_AlwaysIncludedShaders");
        if (included == null)
        {
            failures.Add(
                "GraphicsSettings has no m_AlwaysIncludedShaders property, so this project "
                + "cannot be shown to be free of the issue #1313 workaround.");
            return;
        }

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
                failures.Add(
                    $"Always Included Shaders names {shader.name} from this package. The demo "
                    + "must run on what the package itself makes reachable (issue #1313).");
            }
        }
    }

    /// R-E6: `m_BrgStripping` at 2 (KeepAll). At its default of 0 every
    /// BatchRendererGroup variant is stripped from a project with no DOTS
    /// packages, and the painter submits instances that draw nothing.
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
            failures.Add("GraphicsSettings has no m_BrgStripping property, so R-E6 cannot be set.");
            return;
        }

        stripping.intValue = 2;
        settings.ApplyModifiedProperties();
        AssetDatabase.SaveAssets();

        var reloaded = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        var confirmed = reloaded == null || reloaded.Length == 0
            ? null
            : new SerializedObject(reloaded[0]).FindProperty("m_BrgStripping");
        if (confirmed == null)
        {
            failures.Add("m_BrgStripping could not be read back after being set, R-E6.");
            return;
        }

        if (confirmed.intValue != 2)
        {
            failures.Add($"m_BrgStripping reads back {confirmed.intValue} rather than 2, R-E6.");
        }
    }

    /// The staged native library into the player. Set rather than left to the
    /// importer's default, whose platform set is not the same across Unity
    /// versions — one missing the build target produces a player whose first
    /// P/Invoke raises `DllNotFoundException`, which reads as the package
    /// shipping no binary rather than as this project failing to carry one.
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

            // **Android needs the CPU as well as the compatibility.** A plugin
            // marked compatible with Android and matching no ABI slice of the
            // player is copied into nothing — Unity does not fail the build for
            // it — and the first P/Invoke then raises DllNotFoundException.
            // That is the class issue #1313 records and `just unity-android`
            // asserts against the artifact; the value here matches the one the
            // package's own committed `.meta` carries.
            if (BuildingForAndroid)
            {
                importer.SetPlatformData(BuildTarget.Android, "CPU", "ARM64");
            }

            importer.SaveAndReimport();

            // **Read back off a FRESH importer, which is `SetBrgStripping`'s
            // shape and the reason it reloads the asset.** Asking the same
            // instance what it was just told proves nothing; a count of files
            // walked proves less. A plugin left incompatible with the build
            // target produces a player whose first P/Invoke raises
            // `DllNotFoundException`.
            var reloaded = AssetImporter.GetAtPath(path.Replace('\\', '/')) as PluginImporter;
            if (reloaded == null
                || !reloaded.GetCompatibleWithPlatform(EditorUserBuildSettings.activeBuildTarget))
            {
                failures.Add(
                    $"{path} reads back as incompatible with "
                    + $"{EditorUserBuildSettings.activeBuildTarget} after being set, so the "
                    + "player would not carry it.");
                continue;
            }

            imported++;
        }

        if (imported == 0)
        {
            failures.Add(
                $"no native library under {Plugins}. The player would raise "
                + "DllNotFoundException on its first call into dashscene-ffi.");
        }
    }

    /// R-E7, R-E8 and R-E9's values, set on the Android player.
    ///
    /// **Set and reported, not checked.** `unity/android-probe` is where those
    /// three are read back, and a demonstration is not their gate — but a
    /// player built without them is not representative of a shipping one, and
    /// R-E8 in particular decides whether the staged `.so` reaches the APK at
    /// all. `docs/specification/07-embedding-and-distribution.md` scopes the
    /// three to the project producing the SHIPPING artifact, which a project
    /// regenerated under `target/` on every run is not.
    private static void SetAndroidPlayerSettings(List<string> failures)
    {
        PlayerSettings.SetApplicationIdentifier(NamedBuildTarget.Android, ApplicationId);
        PlayerSettings.SetScriptingBackend(
            NamedBuildTarget.Android, ScriptingImplementation.IL2CPP);
        PlayerSettings.Android.targetArchitectures = AndroidArchitecture.ARM64;

        // **Read from the environment rather than written here**, for
        // `AndroidProbeBuild`'s reason: the justfile's `ANDROID_API` is the one
        // copy of the floor, and every Android target — the shipped
        // `libdashscene_ffi.so` included — is built through
        // `aarch64-linux-android<ANDROID_API>-clang`.
        var floorText = Environment.GetEnvironmentVariable("DASHSCENE_ANDROID_API");
        if (!int.TryParse(floorText, out var floor))
        {
            failures.Add(
                "DASHSCENE_ANDROID_API is unset or not an integer, so the player's minimum "
                + $"SDK has no value to take (read '{floorText}').");
            return;
        }

        PlayerSettings.Android.minSdkVersion = (AndroidSdkVersions)floor;

        // **Vulkan, chosen rather than left to Unity's automatic selection.**
        // Measured on a Pixel 5 on 2026-08-29: a player built with the default
        // list ran OpenGL ES on an Adreno 620 and the painter selected the
        // `ConstantBuffer` rung — where
        // `docs/decisions/unity-painter-uses-brg.md` D4 records `RawBuffer`
        // read under Vulkan on the same device. Both are real answers, and a
        // measurement that does not say which one it took is not comparable
        // with anything: issue #1347 sets this player's cost beside the lean
        // painter's, and that painter requests Vulkan
        // (`docs/design/android-toolchain.md`, D3a).
        PlayerSettings.SetGraphicsAPIs(
            BuildTarget.Android, new[] { GraphicsDeviceType.Vulkan });

        // **Rotation is left ON, and this recipe is the reason.** Issue #1346
        // exercises the Unity host's Android lifecycle over the surface
        // handshake, and a player locked to one orientation cannot be rotated.
        PlayerSettings.allowedAutorotateToPortrait = true;
        PlayerSettings.allowedAutorotateToLandscapeLeft = true;
        PlayerSettings.allowedAutorotateToLandscapeRight = true;
        PlayerSettings.allowedAutorotateToPortraitUpsideDown = true;
        PlayerSettings.defaultInterfaceOrientation = UIOrientation.AutoRotation;

        Debug.Log(
            $"[demo-build] android: IL2CPP, ARM64 exactly, minSdk {floor}, autorotation on, "
            + $"graphics {string.Join(",", PlayerSettings.GetGraphicsAPIs(BuildTarget.Android))}, "
            + $"id {ApplicationId}");
    }

    /// A camera and the showcase component.
    private static void BuildScene()
    {
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);

        var cameraObject = new GameObject("Main Camera", typeof(Camera));
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.GetComponent<Camera>();
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.15f, 0.15f, 0.18f, 1.0f);
        camera.orthographic = true;
        camera.orthographicSize = OrthographicSize;

        // The document's y runs down and the sample's placement scales y by -1,
        // so the document occupies negative world y. Looking along +z from
        // there puts its top-left near the upper left of the window.
        cameraObject.transform.position = new Vector3(
            OrthographicSize * WindowWidth / WindowHeight, -OrthographicSize, -10.0f);

        // **No light, deliberately.** The gate this scene was adapted from
        // carries a directional one because it draws the lit classes; the
        // showcase constructs `BrgPainter(MaterialClass.UnlitOverlay)` and
        // exposes no way to choose another, so a light here would be an object
        // nothing reads.
        new GameObject("Showcase", typeof(DashsceneShowcase));

        Directory.CreateDirectory("Assets/Scenes");
        EditorSceneManager.SaveScene(scene, ScenePath);
    }

    private static void BuildPlayer(List<string> failures)
    {
        // Windowed and resizable, unlike the gate's player: a person runs this
        // one and switches documents in it.
        PlayerSettings.productName = ProductName;
        PlayerSettings.defaultIsNativeResolution = false;
        PlayerSettings.defaultScreenWidth = WindowWidth;
        PlayerSettings.defaultScreenHeight = WindowHeight;
        PlayerSettings.fullScreenMode = FullScreenMode.Windowed;
        PlayerSettings.runInBackground = true;
        PlayerSettings.resizableWindow = true;

        var target = EditorUserBuildSettings.activeBuildTarget;
        var options = new BuildPlayerOptions
        {
            scenes = new[] { ScenePath },
            locationPathName = Path.Combine("Build", ProductName + Extension(target)),
            target = target,
            options = BuildOptions.None,

            // **What turns the showcase scenes on** (story #1342). The package's
            // `Runtime/DemoProducer.cs` and the sample's scene half are both
            // behind this symbol and compile to nothing without it, which is what
            // a customer's own build of this sample does.
            //
            // `extraScriptingDefines` rather than
            // `PlayerSettings.SetScriptingDefineSymbols`: this applies to the
            // player build alone and leaves the editor's own assemblies as they
            // were, so nothing recompiles underneath a batch-mode build. Nothing
            // on the editor side here names the demo API — this method only adds
            // the component to a scene, and the component type exists either way.
            extraScriptingDefines = new[] { "DASHSCENE_DEMO_PRODUCER" },
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

        Debug.Log($"[demo-build] build {report.summary.result}, "
                  + $"{report.summary.totalErrors} error(s)");
        if (report.summary.result != BuildResult.Succeeded)
        {
            failures.Add($"the player build did not succeed: {report.summary.result}.");
            return;
        }

        // The recipe is told where the executable is rather than composing the
        // path itself: a `.app` is a directory whose binary sits under
        // `Contents/MacOS/`, a Linux build is the file, a Windows build is a
        // `.exe`. One place knows.
        var executable = Runnable(target, options.locationPathName, failures);
        if (executable == null)
        {
            return;
        }
        File.WriteAllText(Path.Combine("Build", "player-path.txt"), executable);
        if (BuildingForAndroid)
        {
            File.WriteAllText(Path.Combine("Build", "application-id.txt"), ApplicationId);
        }

        Debug.Log($"[demo-build] player {executable}");
    }

    private static string Extension(BuildTarget target)
    {
        switch (target)
        {
            case BuildTarget.StandaloneOSX:
                return ".app";
            case BuildTarget.StandaloneWindows:
            case BuildTarget.StandaloneWindows64:
                return ".exe";
            case BuildTarget.Android:
                return ".apk";
            default:
                return string.Empty;
        }
    }

    private static string Runnable(BuildTarget target, string built, List<string> failures)
    {
        if (target != BuildTarget.StandaloneOSX)
        {
            return Path.GetFullPath(built);
        }

        var macos = Path.Combine(built, "Contents", "MacOS");
        if (!Directory.Exists(macos))
        {
            failures.Add($"{built} has no Contents/MacOS, so no runnable binary was found.");
            return null;
        }

        // **Exactly one, as `RenderGateBuild.Runnable` requires.** The
        // binary's name is Unity's to choose, `Directory.GetFiles` returns no
        // guaranteed order, and a second entry — a helper, or a `.DS_Store`
        // written by opening the folder in Finder — would make this name the
        // wrong file and the recipe execute it.
        var binaries = Directory.GetFiles(macos);
        if (binaries.Length != 1)
        {
            failures.Add(
                $"{macos} holds {binaries.Length} files; which one is the player cannot be "
                + "told apart from the rest, so no runnable binary was chosen.");
            return null;
        }
        return Path.GetFullPath(binaries[0]);
    }
}
