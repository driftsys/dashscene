// One MSDF sheet, as a texture the painter samples.
//
// **Everything under `Runtime/Engine/` references UnityEngine**, which is what
// separates it from the rest of `Runtime/`: the sheet arrives as PNG bytes on a
// `TextAtlas`, which is engine-independent and compiles against netstandard2.1,
// and turning those bytes into a `Texture2D` is a thing only an engine can do.
//
// R-E10's netstandard2.1 check cannot compile this file; `just unity-editor`
// is what compiles it — `docs/decisions/r-e10-is-checked-in-two-halves.md`.

using System;
using UnityEngine;
using UnityEngine.Experimental.Rendering;

namespace Driftsys.Dashscene
{
    /// Decodes a committed MSDF sheet into the texture a text material samples.
    internal static class AtlasTexture
    {
        /// The sheet as a linear, unmipped, bilinear `Texture2D`.
        ///
        /// **Linear and not sRGB, which is the first of story #851's six rules
        /// and the one that is silent when broken.** The three channels are
        /// signed distances, not colour; sampled through an sRGB decode every
        /// distance is transformed by a curve, and `msdf_coverage`'s
        /// `median3(sample) - 0.5` then crosses zero somewhere other than the
        /// glyph's outline. The result is text with thin or bloated stems
        /// rather than an obvious failure, so the colour space is **read back**
        /// below rather than assumed from the constructor argument.
        ///
        /// **Bilinear, clamped, and no mips.** The first two are set again
        /// after the decode, because `LoadImage` reinitialises the texture and
        /// a read-back alone cannot tell a preserved value from a default that
        /// matches. The third cannot be set after the fact and is read back
        /// instead. `DsMsdfSample` clamps every sample half a texel inside the
        /// glyph's own rectangle, which is what makes a bilinear footprint safe
        /// without a gutter between glyphs; a mip level would average distances
        /// across those boundaries, and nothing selects a level for a 2D quad
        /// anyway.
        ///
        /// # Exceptions
        ///
        /// `DashscenePainterException` when the payload does not decode, when
        /// the decoded extent disagrees with the one the library reported, when
        /// the decoded texture carries a mip chain, or when it came back in an
        /// sRGB format.
        internal static Texture2D Decode(TextAtlas atlas, int index)
        {
            if (atlas == null)
            {
                throw new ArgumentNullException(nameof(atlas));
            }

            // The extent is replaced by `LoadImage`, which reinitialises the
            // texture from the payload's own header — so the two here are a
            // placeholder and the format and the flags are what matter. The
            // `linear` argument is the third positional `bool`, and it is the
            // one this whole function exists for.
            var texture = new Texture2D(2, 2, TextureFormat.RGBA32, mipChain: false, linear: true)
            {
                name = $"Dashscene MSDF Atlas {index}",
                hideFlags = HideFlags.HideAndDontSave,
                // Requests, not results: `LoadImage` below reinitialises the
                // texture and both are set again afterwards. They are here so
                // the object is never briefly in a state this painter did not
                // ask for.
                wrapMode = TextureWrapMode.Clamp,
                filterMode = FilterMode.Bilinear,
            };

            // `markNonReadable` — nothing reads a texel back, so the CPU copy
            // is freed at upload. A readable texture would double the sheet's
            // memory for the life of the document.
            if (!ImageConversion.LoadImage(texture, atlas.Png, markNonReadable: true))
            {
                UnityEngine.Object.DestroyImmediate(texture);
                throw new DashscenePainterException(
                    $"atlas {index}: the {atlas.Png.Length}-byte sheet did not decode. The "
                    + "library admitted it on its PNG header alone — a truncated or "
                    + "CRC-corrupt IDAT passes that and fails here.");
            }

            // **The extent, against the one the metrics declared.** The library
            // checks the same pair at load, against the PNG's IHDR; this checks
            // it against what the DECODER produced, which is a different claim.
            // A disagreement samples the wrong texels rather than failing,
            // because every UV this painter computes normalises by the reported
            // extent and not by the texture's.
            if (texture.width != atlas.Width || texture.height != atlas.Height)
            {
                var decoded = $"{texture.width} x {texture.height}";
                UnityEngine.Object.DestroyImmediate(texture);
                throw new DashscenePainterException(
                    $"atlas {index}: the sheet decoded to {decoded} and its metrics declare "
                    + $"{atlas.Width} x {atlas.Height}. Every glyph rectangle is normalised by "
                    + "the declared extent, so a disagreement samples the wrong texels rather "
                    + "than failing.");
            }

            // **The sampler state, re-applied after the decode and then read
            // back.** `LoadImage` reinitialises the texture from the payload,
            // so `mipChain: false`, `FilterMode.Bilinear` and
            // `TextureWrapMode.Clamp` were requests made of a texture that no
            // longer exists — and reading them back without setting them again
            // cannot tell "preserved" from "reset to a default that happens to
            // match". Setting them is what makes the values true; reading them
            // back is what catches the one that cannot be set, the mip chain.
            //
            // `DsMsdfSample`'s half-texel clamp is calibrated for a bilinear
            // sample of level 0 with no neighbour bleeding in: a mip chain
            // averages distances across glyphs, point filtering steps between
            // them, and a wrapping sampler resolves the opposite edge of the
            // sheet — the first two as soft or ragged stems, the third as
            // another glyph's ink, and none of the three as a failure.
            texture.filterMode = FilterMode.Bilinear;
            texture.wrapMode = TextureWrapMode.Clamp;
            if (texture.mipmapCount != 1)
            {
                var levels = texture.mipmapCount;
                UnityEngine.Object.DestroyImmediate(texture);
                throw new DashscenePainterException(
                    $"atlas {index}: the decoded sheet came back with {levels} mip levels, "
                    + "and an MSDF sheet must be sampled from level 0 alone — a mip level "
                    + "averages distances across glyph boundaries, which thins or bloats "
                    + "stems rather than failing. Unlike the filter and wrap modes, this is "
                    + "not something the texture can be told after the decode.");
            }

            // **Read back rather than assumed**, which is the same posture the
            // painter takes to `BatchRendererGroup.BufferTarget` (R-E14): a
            // constructor argument is a request, and what decides the sample is
            // the format the texture ended up in. `LoadImage` reinitialises the
            // texture from the payload, and a project whose colour space or
            // whose platform format support differs is exactly where a request
            // and a result come apart.
            if (GraphicsFormatUtility.IsSRGBFormat(texture.graphicsFormat))
            {
                var format = texture.graphicsFormat;
                UnityEngine.Object.DestroyImmediate(texture);
                throw new DashscenePainterException(
                    $"atlas {index}: the sheet decoded into {format}, which is an sRGB format. "
                    + "MSDF channels are distances and must be sampled linearly — an sRGB "
                    + "decode moves where median3(sample) - 0.5 crosses zero, which thins or "
                    + "bloats every stem rather than failing.");
            }

            return texture;
        }
    }
}
