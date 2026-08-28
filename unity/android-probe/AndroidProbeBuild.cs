// The Android probe's editor half: configure the first project R-E7, R-E8 and
// R-E9 bind, and build an Android player from it.
//
// **Not part of the package**, like `unity/render-gate/RenderGateBuild.cs` on
// which this file is modelled. `just unity-android` copies it and
// `DashsceneAndroidProbe.cs` into a throwaway project under `target/`.
//
// **This file does NOT implement R-E7, R-E8 or R-E9's check, and must not be
// read as doing so.** It SETS the Android scripting backend, target
// architectures and minimum SDK to the values those three require, so that the
// player this gate measures is representative of a shipping one. Two reasons it
// is not their check, both from records this repository already holds:
//
//   - A check that writes the values it then reads cannot fail. The justfile
//     says exactly that where it explains why `unity-abi` is a separate entry
//     point. Every read-back below catches a write the object rejected, and
//     nothing more.
//   - `docs/specification/07-embedding-and-distribution.md` scopes those three
//     to the project producing the SHIPPING artifact, and says a project built
//     to drive a device is a different project with its own settings. The
//     project this configures is regenerated under `target/` on every run.
//
// Issue #1353 stays open. What would discharge it is a check that reads a
// project it did not configure, or one that reads the built APK's manifest —
// two independently produced numbers rather than one value compared to itself.
//
// An earlier version of this comment claimed the opposite, and added that "a
// player whose architecture setting did not persist fails to install on the
// device, and the recipe reports that". That is false: a fat ARMv7+ARM64 APK
// installs on any device retaining 32-bit support, including the Pixel 5 this
// gate was brought up on. Nothing downstream settles it.

using System;
using System.Collections.Generic;
using System.IO;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

/// <summary>Builds the Android probe's player. `-executeMethod` names `Build`.</summary>
public static class AndroidProbeBuild
{
    private const string ScenePath = "Assets/Scenes/AndroidProbe.unity";
    private const string ProductName = "AndroidProbe";
    private const string PackageName = "com.driftsys.dashscene";

    /// Where the APK is written, relative to the throwaway project.
    private const string ApkPath = "Build/AndroidProbe.apk";

    /// The application id, written to `Build/application-id.txt` so the recipe
    /// launches what was built rather than a literal of its own.
    private const string ApplicationId = "com.driftsys.dashscene.androidprobe";

    /// <summary>The entry point.</summary>
    public static void Build()
    {
        var failures = new List<string>();

        // **Before anything reads a per-platform setting.** `PlayerSettings`
        // is asked for the Android values by name below rather than through the
        // active target, but `CheckPackageNativeLibrary` asks Unity whether the
        // package's `.meta` makes a library reachable *for the target being
        // built* — which is the active one. Switching first is what makes that
        // question about Android rather than about macOS.
        if (EditorUserBuildSettings.activeBuildTarget != BuildTarget.Android)
        {
            if (!EditorUserBuildSettings.SwitchActiveBuildTarget(
                    NamedBuildTarget.Android, BuildTarget.Android))
            {
                failures.Add(
                    "the active build target could not be switched to Android. Android Build "
                    + "Support is not installed in this editor, and every check below would "
                    + "then report on a macOS build.");
            }
        }

        if (failures.Count == 0)
        {
            CreatePipeline(failures);
            SetBrgStripping(failures);
            SetAndroidPlayerSettings(failures);
            CheckPackageNativeLibrary(failures);
        }

        // Nothing is built once the verdict is decided — `RenderGateBuild`'s
        // rule, and an Android player build is the expensive half here too.
        if (failures.Count == 0)
        {
            BuildScene();
            BuildPlayer(failures);
        }

        foreach (var failure in failures)
        {
            Debug.LogError($"[android-probe-build] {failure}");
        }
        Debug.Log(
            failures.Count == 0
                ? "[android-probe-build] OK"
                : $"[android-probe-build] FAILED with {failures.Count} problem(s).");
        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }

    /// R-E7, R-E8 and R-E9 — set, then read back.
    ///
    /// **R-E9's number is not written here.** The requirement says to compare
    /// against `ANDROID_API` in the `justfile`, so the recipe passes that
    /// variable in and this reads it. A literal here would be a second copy of
    /// the floor, and the two would drift the first time one moved.
    private static void SetAndroidPlayerSettings(List<string> failures)
    {
        // R-E7. Unity ships no arm64 Mono runtime — `Variations/mono/Release/Libs/`
        // holds `armeabi-v7a` only — so this is the only backend that can
        // produce the ARM64 player R-E8 requires.
        PlayerSettings.SetScriptingBackend(NamedBuildTarget.Android, ScriptingImplementation.IL2CPP);
        var backend = PlayerSettings.GetScriptingBackend(NamedBuildTarget.Android);
        if (backend != ScriptingImplementation.IL2CPP)
        {
            failures.Add(
                $"the Android scripting backend reads back {backend} rather than IL2CPP, "
                + "which is R-E7.");
        }
        else
        {
            Debug.Log("[android-probe-build] scripting backend = IL2CPP (R-E7)");
        }

        // R-E8. **Equality, not membership** — the requirement says so in terms,
        // and a value also carrying ARMv7 fails. Testing with `HasFlag` would
        // pass exactly the configuration the rule exists to reject.
        PlayerSettings.Android.targetArchitectures = AndroidArchitecture.ARM64;
        var architectures = PlayerSettings.Android.targetArchitectures;
        if (architectures != AndroidArchitecture.ARM64)
        {
            failures.Add(
                $"AndroidTargetArchitectures reads back {architectures} rather than exactly "
                + "ARM64, which is R-E8. A second ABI belongs to a separate project rather "
                + "than to a widening of this rule.");
        }
        else
        {
            Debug.Log("[android-probe-build] target architectures = ARM64 exactly (R-E8)");
        }

        // R-E9.
        var floorText = Environment.GetEnvironmentVariable("DASHSCENE_ANDROID_API");
        if (!int.TryParse(floorText, out var floor))
        {
            failures.Add(
                "DASHSCENE_ANDROID_API is unset or not an integer, so R-E9 has no value to "
                + $"compare against (read '{floorText}'). The recipe passes the justfile's "
                + "ANDROID_API in; a literal here would be a second copy of the floor.");
            return;
        }

        PlayerSettings.Android.minSdkVersion = (AndroidSdkVersions)floor;
        var minSdk = (int)PlayerSettings.Android.minSdkVersion;
        if (minSdk < floor)
        {
            failures.Add(
                $"AndroidMinSdkVersion reads back {minSdk}, below the ANDROID_API floor of "
                + $"{floor}, which is R-E9. That floor binds the shipped artifact because "
                + "every Android target — including the libdashscene_ffi.so this package "
                + "ships — is built through aarch64-linux-android<ANDROID_API>-clang.");
            return;
        }
        Debug.Log($"[android-probe-build] minSdkVersion = {minSdk}, floor {floor} (R-E9)");
    }

    /// R-E4 and R-E5: a URP asset, with the SRP Batcher on.
    ///
    /// R-E5 is read on the ASSET, not on
    /// `GraphicsSettings.useScriptableRenderPipelineBatching` — that global is
    /// assigned when a pipeline INSTANCE is created, which a batch-mode editor
    /// never does, so it reads false however the asset is set.
    /// `RenderGateBuild` carries the measurement behind that.
    private static void CreatePipeline(List<string> failures)
    {
        var renderer = ScriptableObject.CreateInstance<UniversalRendererData>();
        AssetDatabase.CreateAsset(renderer, "Assets/AndroidProbeRenderer.asset");
        var urp = UniversalRenderPipelineAsset.Create(renderer);
        AssetDatabase.CreateAsset(urp, "Assets/AndroidProbeURP.asset");

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
        if (!urp.useSRPBatcher)
        {
            failures.Add(
                "the URP asset's useSRPBatcher reads back false after being set, R-E5.");
        }
        Debug.Log($"[android-probe-build] URP useSRPBatcher {urp.useSRPBatcher} (R-E5)");
    }

    /// R-E6: keep the BatchRendererGroup shader variants.
    ///
    /// At the default of 0 the painter packs and submits every instance and
    /// draws nothing, logging Unity's own "wrong cbuffer setup" per frame. A
    /// blank Android frame is the hardest of all to attribute, so this is set
    /// before a device ever sees the player.
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
                "GraphicsSettings has no m_BrgStripping property, so R-E6 cannot be set.");
            return;
        }

        stripping.intValue = 2;
        settings.ApplyModifiedProperties();
        AssetDatabase.SaveAssets();

        var reloaded = AssetDatabase.LoadAllAssetsAtPath("ProjectSettings/GraphicsSettings.asset");
        var confirmed = reloaded == null || reloaded.Length == 0
            ? null
            : new SerializedObject(reloaded[0]).FindProperty("m_BrgStripping");
        if (confirmed == null || confirmed.intValue != 2)
        {
            failures.Add(
                $"m_BrgStripping reads back {confirmed?.intValue.ToString() ?? "nothing"} "
                + "rather than 2 (KeepAll), R-E6.");
            return;
        }
        Debug.Log("[android-probe-build] m_BrgStripping = 2 (KeepAll), per R-E6");
    }

    /// The package's own Android native library, checked and never configured.
    ///
    /// Nothing is staged here. The library travels inside the package beside a
    /// committed `.meta` since story #1334, and this asks Unity whether that
    /// `.meta` makes it reachable for Android. A failure is a defect in the
    /// package rather than a gap in this project.
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
            failures.Add(
                $"no importable native library under {packagePlugins}. Either the package "
                + "ships none — run `just unity-plugins` — or a file is there and Unity "
                + "imported it as something other than a native plugin, which is what a "
                + "missing or wrong `.meta` produces.");
            return;
        }

        // **Exactly one, ending in .so.** The package ships a macOS `.dylib`
        // too, and counting compatible libraries and stopping above zero would
        // let it satisfy an Android build.
        if (compatible.Count != 1 || !compatible[0].EndsWith(".so", StringComparison.Ordinal))
        {
            failures.Add(
                $"{target} needs exactly one compatible native library ending in .so; the "
                + $"package offers [{string.Join(", ", compatible)}] of {found} shipped. "
                + "R-E21 is the requirement that breaks.");
            return;
        }
        Debug.Log(
            $"[android-probe-build] {compatible[0]} is the one of {found} shipped libraries "
            + "compatible with Android, from the package's own .meta");
    }

    /// A camera, a light for the lit classes, and the probe component.
    private static void BuildScene()
    {
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);

        var cameraObject = new GameObject("Main Camera", typeof(Camera));
        cameraObject.tag = "MainCamera";
        var camera = cameraObject.GetComponent<Camera>();
        camera.clearFlags = CameraClearFlags.SolidColor;
        camera.backgroundColor = new Color(0.15f, 0.15f, 0.18f, 1.0f);

        var lightObject = new GameObject("Directional Light", typeof(Light));
        lightObject.GetComponent<Light>().type = LightType.Directional;
        lightObject.transform.rotation = Quaternion.Euler(50.0f, -30.0f, 0.0f);

        new GameObject("Dashscene Android Probe", typeof(DashsceneAndroidProbe));

        Directory.CreateDirectory("Assets/Scenes");
        EditorSceneManager.SaveScene(scene, ScenePath);
    }

    /// Builds the APK.
    private static void BuildPlayer(List<string> failures)
    {
        PlayerSettings.productName = ProductName;
        PlayerSettings.SetApplicationIdentifier(NamedBuildTarget.Android, ApplicationId);

        // **Written out rather than repeated in the recipe.** `unity-render`
        // writes `Build/player-path.txt` for the same reason: a second copy of
        // this literal in the justfile drifts, and the failure it produces is
        // monkey reporting "No activities found to run" into a stream the
        // recipe then blames on the device.
        Directory.CreateDirectory("Build");
        File.WriteAllText("Build/application-id.txt", ApplicationId);

        // **Development build, and that is deliberate.** The probe reports
        // through `Debug.Log`, which reaches logcat from a release player too,
        // but a development build keeps the managed stack trace on an exception
        // — and an exception out of the painter is one of the two outcomes this
        // probe exists to distinguish.
        var options = new BuildPlayerOptions
        {
            scenes = new[] { ScenePath },
            locationPathName = ApkPath,
            target = BuildTarget.Android,
            options = BuildOptions.Development,
        };

        // **Wrapped, as `RenderGateBuild` wraps its own.** A BuildFailedException
        // out of an IPreprocessBuildWithReport callback, or out of Gradle/IL2CPP
        // setup, otherwise escapes `Build` — `EditorApplication.Exit` never
        // runs, no `[android-probe-build]` line is ever written, and the recipe
        // prints an empty excerpt beside a non-zero exit.
        BuildReport report;
        try
        {
            report = BuildPipeline.BuildPlayer(options);
        }
        catch (Exception e)
        {
            failures.Add(
                $"the Android player build threw {e.GetType().Name}: {e.Message}");
            return;
        }

        if (report.summary.result != BuildResult.Succeeded)
        {
            failures.Add(
                $"the Android player build ended {report.summary.result} with "
                + $"{report.summary.totalErrors} error(s).");
            return;
        }
        // **The file's own size, not `report.summary.totalSize`.** Those differ by
        // more than a rounding: the first run of this gate reported 903,555,960
        // bytes for an APK that is 52.1 MB on disk. A number that reads as a
        // file size and is not one is the defect class this repository's memory
        // records most often, and an APK size is exactly the figure a later
        // packaging story would quote.
        var apk = new FileInfo(ApkPath);
        Debug.Log(
            $"[android-probe-build] APK at {ApkPath}, {apk.Length} bytes on disk "
            + $"({report.summary.totalSize} reported as the build's total size, which is "
            + "not the same quantity)");
    }
}
