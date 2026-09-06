// The TextMeshPro font assets story #1444's faithful Canvas typesets with, built
// at player-build time from the same font files the corpus shapes with.
//
// **Its own `-executeMethod` step, before `DemoBuild.Build`.** A `TMP_FontAsset`
// is an asset rather than a run-time object: `TMP_FontAsset.CreateFontAsset`
// needs an imported `UnityEngine.Font`, and a player build keeps only what a
// scene or a `Resources` folder references. So the assets are created in the
// editor, written under `Assets/Resources/`, and loaded by name at run time.
// Placing this in a separate file rather than inside `DemoBuild.cs` keeps that
// file — another lane's — untouched.
//
// **The name is the producer's own face key**, `<family without spaces>-<weight>`:
// `ds_demo_face_key` answers it for a glyph run's cascade slot and
// `Samples~/Showcase/CanvasText.cs` loads `DashsceneFont-<key>` with it. The two
// sides agree without either holding a list of faces, and a font the recipe
// staged under a name no run asks for simply goes unused — while a face a run
// asks for and no asset answers is a warning and a counted refusal, never a
// silently wrong glyph.
//
// **`AddObjectToAsset` for the atlas texture and the material.** Both are
// in-memory sub-objects of a freshly created font asset until they are added, and
// an asset saved without them loads with neither — which is a `TMP_Text` that
// renders nothing, with no error. TextMeshPro's own creation window adds both.

using System.Collections.Generic;
using System.IO;
using TMPro;
using UnityEditor;
using UnityEngine;

// `GlyphRenderMode` is TextCore's rather than TextMeshPro's own: TMP is a client
// of the font engine here, and the enum sits with it.
using UnityEngine.TextCore.LowLevel;

/// <summary>Builds one TMP font asset per font file the recipe staged.</summary>
public static class DemoFonts
{
    /// <summary>Where the recipe stages the corpus fonts.</summary>
    public const string FontDirectory = "Assets/Fonts";

    /// <summary>Where the created assets go, so a player build keeps them.</summary>
    public const string ResourceDirectory = "Assets/Resources";

    /// <summary>The prefix `CanvasText` loads them by.</summary>
    public const string Prefix = "DashsceneFont-";

    /// <summary>
    /// The sampling point size every asset is rasterised at.
    /// </summary>
    /// <remarks>
    /// Large enough that the showcase's headings are not upsampled and small
    /// enough that three faces fit their atlases. TextMeshPro's own default for a
    /// dynamic asset is 90; this is below it because the atlas is filled on
    /// demand and the showcase's runs are between 12 and 48 document units.
    /// </remarks>
    private const int PointSize = 64;

    private const int Padding = 6;
    private const int AtlasWidth = 1024;
    private const int AtlasHeight = 1024;

    /// <summary>
    /// Creates one asset per staged font, or exits non-zero naming what failed.
    /// </summary>
    public static void Create()
    {
        var failures = new List<string>();
        CreateAssets(failures);
        if (failures.Count > 0)
        {
            Debug.LogError("[demo-fonts] " + string.Join("\n[demo-fonts] ", failures));
            EditorApplication.Exit(1);
            return;
        }
        Debug.Log("[demo-fonts] OK");
    }

    /// <summary>
    /// The half that reports rather than exits, so a caller can run it inside a
    /// larger step.
    /// </summary>
    public static void CreateAssets(List<string> failures)
    {
        if (!Directory.Exists(FontDirectory))
        {
            // **A failure, not a skip.** A player built with no font asset draws
            // a Canvas with no text at all, and that is a comparison that reads
            // as a large difference rather than as a missing input.
            failures.Add(
                $"{FontDirectory} does not exist. The unity-demo recipes stage the "
                + "corpus fonts there before this step runs.");
            return;
        }

        if (!AssetDatabase.IsValidFolder(ResourceDirectory))
        {
            AssetDatabase.CreateFolder("Assets", "Resources");
        }

        if (TMP_Settings.instance == null)
        {
            failures.Add(
                "TextMeshPro's essential resources are not in this project, so "
                + "TMP_Settings has no instance and TMP_FontAsset.CreateFontAsset "
                + "throws inside the editor's own code. `just _tmp-essentials` is the "
                + "step that extracts them, and it runs before this one.");
            return;
        }

        var staged = 0;
        foreach (var path in Directory.GetFiles(FontDirectory))
        {
            var extension = Path.GetExtension(path).ToLowerInvariant();
            if (extension != ".ttf" && extension != ".otf")
            {
                continue;
            }

            staged++;
            var assetPath = $"{FontDirectory}/{Path.GetFileName(path)}";
            var font = AssetDatabase.LoadAssetAtPath<Font>(assetPath);
            if (font == null)
            {
                failures.Add($"{assetPath} did not import as a Font.");
                continue;
            }

            var created = TMP_FontAsset.CreateFontAsset(
                font, PointSize, Padding, GlyphRenderMode.SDFAA, AtlasWidth, AtlasHeight);
            if (created == null)
            {
                failures.Add($"{assetPath}: TMP_FontAsset.CreateFontAsset answered null.");
                continue;
            }

            var key = Path.GetFileNameWithoutExtension(path);
            var target = $"{ResourceDirectory}/{Prefix}{key}.asset";
            AssetDatabase.CreateAsset(created, target);

            // Both are in-memory sub-objects until added, and an asset saved
            // without them loads with neither — see the file header.
            if (created.atlasTextures != null && created.atlasTextures.Length > 0)
            {
                AssetDatabase.AddObjectToAsset(created.atlasTextures[0], created);
            }
            else
            {
                failures.Add($"{target}: the created asset carries no atlas texture.");
            }

            if (created.material != null)
            {
                AssetDatabase.AddObjectToAsset(created.material, created);
            }
            else
            {
                failures.Add($"{target}: the created asset carries no material.");
            }

            Debug.Log($"[demo-fonts] {target} from {assetPath}");
        }

        if (staged == 0)
        {
            failures.Add(
                $"{FontDirectory} holds no .ttf or .otf, so no font asset was created and "
                + "the Canvas would draw no text at all.");
        }

        AssetDatabase.SaveAssets();
    }
}
