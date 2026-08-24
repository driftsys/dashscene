// R-E10's second check, and the only gate in this repository that compiles the
// engine-referencing half of the package.
//
// **Not part of the package.** This file is copied into a throwaway Unity
// project by `just unity-editor` and lives outside
// `unity/com.driftsys.dashscene/`, so it is not shipped and
// `unity/package-compat`'s glob never sees it.
//
// **Why an editor at all.** `unity/package-compat` compiles `Runtime/` against
// `netstandard.dll` 2.1.0 with no Unity reference assemblies, so it cannot
// compile a type that references `UnityEngine` — issue #1286. This is the other
// half: a real editor, with real reference assemblies, compiling the package
// under the API compatibility level R-E10 is actually about. It needs an editor
// install, which
// `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4
// records that no CI runner here can host — so this runs on a developer's
// machine and CI runs the other half.
//
// Each question below is numbered where it starts. **Where a question examines
// a SET, it carries an assertion that fails when that set turned out to be
// empty** — a shader list, a variant count, the shipped plugins. The two that
// read a single value instead, the compiled assembly and the API compatibility
// level, cannot be vacuous in that way and carry no such assertion.

using System;
using System.Collections.Generic;
using System.IO;
using UnityEditor;
using UnityEditor.Build;
using UnityEngine;

/// <summary>Compiles the package in an editor and reports what failed.</summary>
public static class DashsceneEditorCompat
{
    private const string RuntimeAssembly = "Driftsys.Dashscene.Runtime.dll";
    private const string PackageName = "com.driftsys.dashscene";

    /// The two programmable stages a BatchRendererGroup pass carries.
    private static readonly UnityEditor.Rendering.ShaderType[] Stages =
    {
        UnityEditor.Rendering.ShaderType.Vertex,
        UnityEditor.Rendering.ShaderType.Fragment,
    };

    /// <summary>The entry point `-executeMethod` names.</summary>
    public static void Run()
    {
        var failures = new List<string>();

        // 1. The package's runtime assembly compiled at all.
        //
        // **This is the whole of what `package-compat` cannot ask.** Unity has
        // already compiled every assembly by the time an `-executeMethod` runs;
        // if the package failed, the editor refuses to run this method and the
        // process exits non-zero before reaching here. So this assertion is the
        // belt to that brace, and it is what catches the case where the
        // assembly definition stopped covering the sources.
        var assembly = Path.Combine("Library", "ScriptAssemblies", RuntimeAssembly);
        if (!File.Exists(assembly))
        {
            failures.Add(
                $"{RuntimeAssembly} is not in Library/ScriptAssemblies. The package's "
                + "assembly definition compiled nothing, or it compiled under a different "
                + "name.");
        }

        // 1b. The SAMPLE compiled, which nothing else in this repository asks.
        //
        // `Samples~` is hidden from Unity's importer by its `~`, and
        // `package-compat` and `ffi-check` glob `Runtime/**/*.cs` — so the
        // sample was compiled by nothing until issue #1298 put the painter's
        // wiring into it. The recipe copies it into `Assets/Samples/`, where it
        // lands in `Assembly-CSharp`; if it did not compile, the editor refuses
        // to run this method at all, so reaching here with the assembly present
        // is the assertion.
        var sampleAssembly = Path.Combine("Library", "ScriptAssemblies", "Assembly-CSharp.dll");
        var sampleSources = Directory.Exists(Path.Combine("Assets", "Samples"))
            ? Directory.GetFiles(
                Path.Combine("Assets", "Samples"), "*.cs", SearchOption.AllDirectories)
            : Array.Empty<string>();
        if (sampleSources.Length == 0)
        {
            failures.Add(
                "no sample source under Assets/Samples. The recipe copies "
                + "Samples~/*/ there so that something compiles them; without it "
                + "the package's sample MonoBehaviours are compiled by nothing.");
        }
        else if (!File.Exists(sampleAssembly))
        {
            failures.Add(
                $"{sampleSources.Length} sample source(s) are under Assets/Samples and "
                + "Assembly-CSharp.dll is not in Library/ScriptAssemblies, so they compiled "
                + "into nothing.");
        }

        // 2. R-E10's actual subject: the API compatibility level.
        //
        // A project that compiled the package under .NET Framework would say
        // nothing about the requirement, and Unity's default has changed
        // between versions — so this is read rather than assumed.
        var level = PlayerSettings.GetApiCompatibilityLevel(NamedBuildTarget.Standalone);
        if (level != ApiCompatibilityLevel.NET_Standard)
        {
            failures.Add(
                $"the project's API compatibility level is {level}, not NET_Standard. "
                + "R-E10 is about ApiCompatibilityLevel.NET_Standard, so a run under any "
                + "other level answers a different question.");
        }

        // 3. Every shader the package ships compiles, INCLUDING the variant
        //    R-E12 requires.
        //
        // R-E11 and R-E12 are checked without an editor by
        // `unity/package-gate`, which reads the pragmas. Nothing there compiles
        // a shader, so a `#pragma` that is present and an include path that is
        // wrong both pass it. This is where that is caught.
        //
        // **The import alone is not enough**, and that is the whole reason
        // `CompileVariant` is called below. Unity compiles a shader's variants
        // lazily, so `GetShaderMessages` after an import reports on whatever
        // the editor happened to need — which does not include
        // `DOTS_INSTANCING_ON`, the one variant a BatchRendererGroup actually
        // draws with. A gate that stopped at the import would report that the
        // painter's shaders compile, having never compiled the code path the
        // painter uses.
        //
        // **The whole of `Runtime/`, recursively, and not one directory.**
        // `unity/package-gate` collects for the same reason and over a wider
        // scope — the whole package, so it also sees a shader placed outside
        // `Runtime/`, which this gate would not import anyway:
        // the shaders moved once already — issue #1313 put them under
        // `Runtime/Resources/` so a player build keeps them — and a gate
        // pointed at a directory answers "every shader I found there compiles",
        // which is not the question. A shader this enumeration misses is one
        // nothing in the repository ever compiles, with both emptiness guards
        // below still satisfied by its siblings.
        var shaderDir = Path.Combine("Packages", PackageName, "Runtime");
        var shaderPaths = Directory.Exists(shaderDir)
            ? Directory.GetFiles(shaderDir, "*.shader", SearchOption.AllDirectories)
            : Array.Empty<string>();

        // The non-empty rule R-E11 states, applied to this check too: a gate
        // that compiled no shader would report that every shader compiles.
        if (shaderPaths.Length == 0)
        {
            failures.Add(
                $"no .shader anywhere under {shaderDir}. This check would then report that "
                + "every shader compiles, having compiled none.");
        }

        // The target fleet's two graphics APIs, plus the one this editor runs
        // on. `docs/specification/03-target-hardware-rules.md` names GLES 3.2
        // and Vulkan on Android; Metal is what compiled the import above, and
        // is here so a failure that is specific to the developer's own machine
        // is not mistaken for a target one.
        var platforms = new[]
        {
            (UnityEditor.Rendering.ShaderCompilerPlatform.Vulkan, BuildTarget.Android),
            (UnityEditor.Rendering.ShaderCompilerPlatform.GLES3x, BuildTarget.Android),
            (UnityEditor.Rendering.ShaderCompilerPlatform.Metal, BuildTarget.StandaloneOSX),
        };

        // **A control, because "success with no bytes" needs one.** The check
        // below refuses a variant that reports success and produces nothing, on
        // the reasoning that the API reports success for a variant it declined
        // to build. That reasoning is a hypothesis until a shader KNOWN to
        // compile is measured through the same call: if URP's own unlit shader
        // also produces no bytes for a platform, the emptiness says something
        // about the platform rather than about the shader, and the check must
        // not be stated over it.
        //
        // Measured per (platform, stage) rather than assumed, and the result
        // decides which pairs the assertion below runs on.
        var controlProducesBytes = new Dictionary<string, bool>();
        var control = Shader.Find("Universal Render Pipeline/Unlit");
        if (control == null)
        {
            failures.Add(
                "the control shader 'Universal Render Pipeline/Unlit' was not found, so the "
                + "empty-shader-data check below has nothing to calibrate against.");
        }
        else
        {
            var controlData = ShaderUtil.GetShaderData(control);
            foreach (var (platform, target) in platforms)
            {
                foreach (var stage in Stages)
                {
                    var key = $"{platform}/{stage}";
                    if (controlProducesBytes.ContainsKey(key))
                    {
                        continue;
                    }

                    var info = controlData
                        .GetSubshader(0)
                        .GetPass(0)
                        .CompileVariant(stage, Array.Empty<string>(), platform, target);
                    var bytes = info.Success && info.ShaderData != null && info.ShaderData.Length > 0;
                    controlProducesBytes[key] = bytes;
                    if (!bytes)
                    {
                        Debug.Log(
                            $"[unity-editor] control: URP/Unlit {stage} produces no shader data "
                            + $"for {platform}, so the emptiness check is skipped for that pair.");
                    }
                }
            }
        }

        var variantsCompiled = 0;
        foreach (var path in shaderPaths)
        {
            // The importer path, which is what `AssetDatabase` resolves —
            // `Directory.GetFiles` already returns it in that form here,
            // because the project's working directory is its own root.
            var shader = AssetDatabase.LoadAssetAtPath<Shader>(path.Replace('\\', '/'));
            if (shader == null)
            {
                failures.Add($"{path} did not import as a Shader.");
                continue;
            }

            foreach (var message in ShaderUtil.GetShaderMessages(shader))
            {
                if (message.severity != UnityEditor.Rendering.ShaderCompilerMessageSeverity.Error)
                {
                    continue;
                }
                failures.Add(
                    $"{path}({message.line}): {message.message} {message.messageDetails}");
            }

            var data = ShaderUtil.GetShaderData(shader);
            for (var s = 0; s < data.SubshaderCount; s++)
            {
                var subshader = data.GetSubshader(s);
                for (var p = 0; p < subshader.PassCount; p++)
                {
                    var pass = subshader.GetPass(p);
                    foreach (var (platform, target) in platforms)
                    {
                        foreach (var stage in Stages)
                        {
                            var info = pass.CompileVariant(
                                stage,
                                new[] { "DOTS_INSTANCING_ON" },
                                platform,
                                target);

                            // Every message the compiler produced, whatever
                            // the verdict. A variant that reports success and
                            // produces nothing usually says why in a warning,
                            // and a gate that read `Messages` only on failure
                            // would drop exactly that.
                            var detail = info.Messages == null || info.Messages.Length == 0
                                ? "(no message)"
                                : string.Join(
                                    "; ",
                                    Array.ConvertAll(
                                        info.Messages,
                                        m => $"[{m.severity}] {m.message} {m.messageDetails}"));

                            if (!info.Success)
                            {
                                failures.Add(
                                    $"{path} pass {p} {stage} did not compile for {platform} "
                                    + $"with DOTS_INSTANCING_ON: {detail}");
                                continue;
                            }

                            // **A success with no bytes is not a compile.** The
                            // API reports `Success` for a variant it declined
                            // to build as well as for one it built, and a gate
                            // that read the flag alone would pass over an
                            // unsupported platform having produced nothing.
                            var calibrated = controlProducesBytes.TryGetValue(
                                $"{platform}/{stage}", out var controlBytes) && controlBytes;
                            if (info.ShaderData == null || info.ShaderData.Length == 0)
                            {
                                if (calibrated)
                                {
                                    failures.Add(
                                        $"{path} pass {p} {stage} reported success for {platform} "
                                        + $"and produced no shader data, where URP's own unlit "
                                        + $"shader does produce it: {detail}");
                                }
                                continue;
                            }

                            variantsCompiled++;
                        }
                    }
                }
            }
        }

        // The same non-empty rule one level down: a shader whose passes were
        // all skipped would leave this at zero while every assertion above
        // passed.
        if (shaderPaths.Length > 0 && variantsCompiled == 0)
        {
            failures.Add(
                "no shader variant compiled at all, so R-E12's variant is checked by "
                + "nothing here.");
        }

        // 4. Every native library the package ships is configured for the
        //    platform D3 assigns it.
        //
        // **This reads the importer back, not the `.meta` text.**
        // `unity/package-gate`'s `plugin_meta` test is the textual half and
        // runs on every pull request without an editor. This is the half that
        // asks the engine what it actually parsed, and the two are not
        // redundant: Unity resolves a platform value through an enum converter
        // and, on failure, substitutes the default with a warning rather than
        // an error, so a `.meta` carrying `arm64` where D3 states `ARM64` is
        // plausible as text and wrong here. R-E21 is about that difference.
        //
        // **It verifies and never repairs.** Applying the values here before
        // reading them back would make the assertion pass by construction —
        // the gate would be checking its own write. `WritePluginMeta` is the
        // authoring entry point, and a developer runs it once when a platform
        // is added, then commits what Unity wrote.
        var pluginsChecked = CheckNativePlugins(failures);

        foreach (var failure in failures)
        {
            Debug.LogError($"[unity-editor] {failure}");
        }

        // `level` prints as `NET_Standard_2_0`, which is the obsolete alias
        // for the same enum value as `NET_Standard` — the comparison above is
        // against the current name and passes. Do not "fix" the assertion to
        // match the printed text.
        Debug.Log(
            failures.Count == 0
                ? $"[unity-editor] OK: the package compiled under {level}, "
                  + $"{shaderPaths.Length} shader(s) imported clean, {variantsCompiled} "
                  + "shader variant(s) compiled with DOTS_INSTANCING_ON, and "
                  + $"{pluginsChecked} native plugin(s) carry D3's platform data."
                : $"[unity-editor] FAILED with {failures.Count} problem(s).");

        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }

    /// <summary>
    /// One row of D3's per-platform matrix, for a library this package ships.
    /// </summary>
    ///
    /// `EditorSettings` is empty where D3's row states no editor data — the
    /// Android row is that case, and R-E21 compares over the keys a row
    /// carries rather than over a fixed pair.
    private struct NativePlugin
    {
        public string AssetPath;
        public bool CompatibleWithEditor;
        public (string Key, string Value)[] EditorSettings;
        public BuildTarget Target;
        public (string Key, string Value)[] TargetSettings;
    }

    /// D3's rows for the libraries this branch ships.
    ///
    /// **The Windows, Linux and iOS rows of D3 ship nothing today**, by a
    /// scope decision recorded with story #1334: they have no consumer, and
    /// D4 accepts a committed binary per platform in a public repository's
    /// permanent history. A row added here without a binary beside it fails
    /// the set comparison below rather than passing quietly.
    private static NativePlugin[] ShippedPlugins()
    {
        var root = $"Packages/{PackageName}/Runtime/Plugins";
        return new[]
        {
            new NativePlugin
            {
                AssetPath = $"{root}/macOS/libdashscene_ffi.dylib",
                CompatibleWithEditor = true,
                EditorSettings = new[] { ("OS", "OSX"), ("CPU", "ARM64") },
                Target = BuildTarget.StandaloneOSX,
                TargetSettings = new[] { ("CPU", "ARM64") },
            },
            new NativePlugin
            {
                AssetPath = $"{root}/Android/libdashscene_ffi.so",
                CompatibleWithEditor = false,
                EditorSettings = new (string, string)[0],
                Target = BuildTarget.Android,
                TargetSettings = new[] { ("CPU", "ARM64") },
            },
        };
    }

    /// The extensions Unity treats as a native plugin on the platforms in
    /// scope. `.a` is iOS, which ships nothing yet and is listed so that
    /// adding one is caught by the set comparison rather than ignored.
    private static readonly string[] NativeLibraryExtensions =
    {
        ".dylib", ".so", ".dll", ".a",
    };

    /// <summary>
    /// Reads back the platform data Unity parsed for each shipped library and
    /// compares it against D3, per R-E21.
    /// </summary>
    private static int CheckNativePlugins(List<string> failures)
    {
        var expected = ShippedPlugins();

        // **What is actually in the package, not what this file expects to be
        // there.** A library committed at a path nobody updated this list for
        // would otherwise be invisible here — and being invisible is the
        // failure mode D2 describes, where a plugin with no correct `.meta`
        // is Editor-only and silently absent from every player build.
        var prefix = $"Packages/{PackageName}/Runtime/Plugins/";
        var found = new List<string>();
        foreach (var path in AssetDatabase.GetAllAssetPaths())
        {
            if (!path.StartsWith(prefix, StringComparison.Ordinal))
            {
                continue;
            }

            foreach (var extension in NativeLibraryExtensions)
            {
                if (path.EndsWith(extension, StringComparison.Ordinal))
                {
                    found.Add(path);
                    break;
                }
            }
        }

        // **The set, not the count.** A count would pass a commit that deleted
        // one library and added a second copy of another, leaving the deleted
        // one shipped by nothing. The same reasoning is written out in
        // `unity/ffi-check`'s entry-point check.
        found.Sort(StringComparer.Ordinal);
        var wanted = new List<string>();
        foreach (var plugin in expected)
        {
            wanted.Add(plugin.AssetPath);
        }

        wanted.Sort(StringComparer.Ordinal);
        if (string.Join(", ", found) != string.Join(", ", wanted))
        {
            failures.Add(
                "the native libraries under Runtime/Plugins/ are not the set D3's rows "
                + $"declare. found: [{string.Join(", ", found)}]; declared: "
                + $"[{string.Join(", ", wanted)}]. A library the package ships and this "
                + "list does not name is checked by nothing.");
            return 0;
        }

        foreach (var plugin in expected)
        {
            var importer = AssetImporter.GetAtPath(plugin.AssetPath) as PluginImporter;
            if (importer == null)
            {
                failures.Add(
                    $"{plugin.AssetPath} has no PluginImporter. Unity did not import it as a "
                    + "native plugin at all, so no platform data can be set on it.");
                continue;
            }

            // **`Any` must be off for the rest to mean anything.** A plugin
            // compatible with any platform is included everywhere whatever the
            // per-platform CPU values say, so a wrong CPU would not show as a
            // missing library and this check would not be measuring R-E21.
            if (importer.GetCompatibleWithAnyPlatform())
            {
                failures.Add(
                    $"{plugin.AssetPath} is marked compatible with any platform, so its "
                    + "per-platform CPU settings decide nothing.");
            }

            if (importer.GetCompatibleWithEditor() != plugin.CompatibleWithEditor)
            {
                failures.Add(
                    $"{plugin.AssetPath}: editor compatibility is "
                    + $"{importer.GetCompatibleWithEditor()}, D3's row states "
                    + $"{plugin.CompatibleWithEditor}.");
            }

            foreach (var (key, value) in plugin.EditorSettings)
            {
                var actual = importer.GetEditorData(key);
                if (!string.Equals(actual, value, StringComparison.Ordinal))
                {
                    failures.Add(
                        $"{plugin.AssetPath}: editor data {key} reads '{actual}', D3 states "
                        + $"'{value}'. Casing is the substance of R-E21 — Unity substitutes "
                        + "the default with a warning rather than failing.");
                }
            }

            if (!importer.GetCompatibleWithPlatform(plugin.Target))
            {
                failures.Add(
                    $"{plugin.AssetPath} is not compatible with {plugin.Target}, so it is "
                    + "absent from that platform's player build.");
            }

            foreach (var (key, value) in plugin.TargetSettings)
            {
                var actual = importer.GetPlatformData(plugin.Target, key);
                if (!string.Equals(actual, value, StringComparison.Ordinal))
                {
                    failures.Add(
                        $"{plugin.AssetPath}: {plugin.Target} data {key} reads '{actual}', "
                        + $"D3 states '{value}'.");
                }
            }
        }

        return expected.Length;
    }

    /// <summary>
    /// Writes D3's platform data onto each shipped library and saves it, so
    /// Unity produces the `.meta` a developer then commits.
    /// </summary>
    ///
    /// **A separate entry point from <see cref="Run"/> on purpose.** A check
    /// that writes the values it is about to read cannot fail. This is run by
    /// hand — `just unity-editor WritePluginMeta` — once, when a platform is
    /// added, and the `.meta` it produces is the artifact. R-E2 requires those
    /// files to be generated by an editor rather than written by hand, because
    /// the guid is what an asset reference resolves through and nothing can
    /// mint one later in an immutable package.
    public static void WritePluginMeta()
    {
        var failures = new List<string>();
        foreach (var plugin in ShippedPlugins())
        {
            var importer = AssetImporter.GetAtPath(plugin.AssetPath) as PluginImporter;
            if (importer == null)
            {
                failures.Add(
                    $"{plugin.AssetPath} has no PluginImporter — is the binary committed at "
                    + "that path?");
                continue;
            }

            importer.SetCompatibleWithAnyPlatform(false);
            importer.SetCompatibleWithEditor(plugin.CompatibleWithEditor);

            // **Cleared, not just set.** Setting only the row's own platform
            // leaves anything enabled by hand — an inspector tick, an older
            // authoring pass — enabled, and the arm64 dylib then travels inside
            // every player build for that platform. **`plugin_meta` in
            // `unity/package-gate` is what reports such an entry** — its
            // exclusivity rule fails when a platform the row does not name is
            // enabled. `CheckNativePlugins` below does not: it asks about the
            // row's own platform and about `Any`, and never enumerates the
            // rest. This loop is what removes what that gate reports.
            //
            // Some `BuildTarget` members are obsolete and refuse the call, so
            // each one is attempted on its own rather than assuming the
            // enumeration is uniform.
            foreach (BuildTarget other in Enum.GetValues(typeof(BuildTarget)))
            {
                if (other == plugin.Target)
                {
                    continue;
                }

                try
                {
                    if (importer.GetCompatibleWithPlatform(other))
                    {
                        importer.SetCompatibleWithPlatform(other, false);
                        Debug.Log(
                            $"[unity-editor] cleared {other} from {plugin.AssetPath}");
                    }
                }
                catch (Exception)
                {
                    // An obsolete or unsupported target. It cannot be enabled
                    // either, so there is nothing to clear.
                }
            }
            foreach (var (key, value) in plugin.EditorSettings)
            {
                importer.SetEditorData(key, value);
            }

            importer.SetCompatibleWithPlatform(plugin.Target, true);
            foreach (var (key, value) in plugin.TargetSettings)
            {
                importer.SetPlatformData(plugin.Target, key, value);
            }

            importer.SaveAndReimport();
            Debug.Log($"[unity-editor] wrote platform data for {plugin.AssetPath}");
        }

        foreach (var failure in failures)
        {
            Debug.LogError($"[unity-editor] {failure}");
        }

        AssetDatabase.SaveAssets();
        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }
}
