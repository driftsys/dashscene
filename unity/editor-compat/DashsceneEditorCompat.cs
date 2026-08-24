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
// It answers four questions and refuses to answer any of them vacuously.

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
                  + $"{shaderPaths.Length} shader(s) imported clean, and {variantsCompiled} "
                  + "shader variant(s) compiled with DOTS_INSTANCING_ON."
                : $"[unity-editor] FAILED with {failures.Count} problem(s).");

        EditorApplication.Exit(failures.Count == 0 ? 0 : 1);
    }
}
