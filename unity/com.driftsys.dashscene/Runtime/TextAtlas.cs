// The glyph atlases a document's runs sample, copied out of the library.
//
// **Engine-independent on purpose**, like `FramePacker.cs` beside it: no
// `UnityEngine` type appears here, so `unity/package-compat` compiles it
// against `netstandard.dll` 2.1.0 and a check with no editor can execute the
// glyph lookup against a real document. Turning a sheet into a texture is
// `Runtime/Engine/`'s job.
//
// **Copied rather than borrowed.** `ds_runtime_atlas` hands out pointers valid
// until the next load, which is longer than a frame lease and still not the
// life of a painter — a host that kept them would read freed memory the moment
// it loaded a second document. The tables are small (an ASCII sheet places
// about a hundred glyphs) and are read once per load, so copying costs nothing
// the frame path pays for.

using System;
using Driftsys.Dashscene.BoundaryB;

namespace Driftsys.Dashscene
{
    /// One MSDF glyph atlas: the sheet, the scalars a painter shades with, and
    /// the placement of every glyph that paints.
    public sealed class TextAtlas
    {
        private readonly AtlasGlyph[] _glyphs;

        internal TextAtlas(
            int width,
            int height,
            int pxPerEm,
            float distanceRangePx,
            byte[] png,
            AtlasGlyph[] glyphs,
            long nativeGlyphRows)
        {
            Width = width;
            Height = height;
            PxPerEm = pxPerEm;
            DistanceRangePx = distanceRangePx;
            Png = png;
            _glyphs = glyphs;
            NativeGlyphRows = nativeGlyphRows;
        }

        /// The sheet's width in texels. Never zero.
        public int Width { get; }

        /// The sheet's height in texels. Never zero.
        public int Height { get; }

        /// The size, in texels per em, the sheet was rendered at. Never zero.
        public int PxPerEm { get; }

        /// The MSDF distance range in atlas texels. Always finite and above
        /// zero.
        public float DistanceRangePx { get; }

        /// The encoded sheet — a PNG, and the bytes the library holds rather
        /// than the ones a host happens to have on hand.
        ///
        /// **Which is the whole point of it crossing.** An atlas index is the
        /// typesetter's font slot, not the index of the `TextFontFace` a host
        /// passed: the cascade groups faces by family before flattening
        /// family-major, so a host that listed one family's faces
        /// non-contiguously and paired by array index would upload another
        /// face's sheet and sample the wrong glyphs rather than fail.
        public byte[] Png { get; }

        /// How many glyphs this sheet places.
        public int GlyphCount => _glyphs.Length;

        /// How many rows the library reported for this sheet.
        ///
        /// **Not `GlyphCount`, and the difference is the point.** That is the
        /// length of the copy this package made; this is what the library said
        /// there were. They must agree, and nothing else can say so: every
        /// question asked of the copy — how many rows it holds, which ids
        /// `TryGlyph` reaches — is answered by the copy, so a copy that dropped
        /// rows is self-consistent. `unity/ffi-check` compares the two.
        public long NativeGlyphRows { get; }

        /// The screen-pixel MSDF range for a run drawn at `size` document
        /// units per em.
        ///
        /// `dashpaint::Atlas`'s own formula, which every painter computes:
        /// `distance_range_px * size / px_per_em`. `PlaneEm` and `AtlasPx` bake
        /// the range into the bounds, so this scales the sharpness of the edge
        /// and not the size.
        public float PixelRange(float size)
        {
            return DistanceRangePx * size / PxPerEm;
        }

        /// The placement for `glyphId`, or `false` when this sheet has no quad
        /// for it.
        ///
        /// **An absent glyph draws nothing, and that is not an error.** An
        /// empty outline such as a space has no quad at all, and a codepoint
        /// outside the sheet's charset is a build-time coverage gap the atlas
        /// closure owns — there is no runtime atlas rebuild to ask for
        /// (`docs/decisions/glyph-coverage-is-declared-at-build-time.md`).
        /// `dashpaint::Atlas::glyph` answers `None` for both and both painters
        /// skip the quad, which is what this mirrors.
        ///
        /// Binary search over a table the library promises is sorted and unique
        /// by `GlyphId`, which is the same search `Atlas::glyph` performs.
        public bool TryGlyph(uint glyphId, out AtlasGlyph glyph)
        {
            var lo = 0;
            var hi = _glyphs.Length - 1;
            while (lo <= hi)
            {
                // `lo + (hi - lo) / 2`, not `(lo + hi) / 2`: the second
                // overflows for a table past int.MaxValue/2 rows. Unreachable
                // at any sheet size this pipeline bakes, and written the safe
                // way because the unsafe way is not shorter.
                var mid = lo + ((hi - lo) / 2);
                var id = _glyphs[mid].GlyphId;
                if (id == glyphId)
                {
                    glyph = _glyphs[mid];
                    return true;
                }
                if (id < glyphId)
                {
                    lo = mid + 1;
                }
                else
                {
                    hi = mid - 1;
                }
            }
            glyph = default;
            return false;
        }
    }

    /// Every atlas a loaded document's runs can name, indexed by a
    /// `GlyphRun.Atlas`.
    ///
    /// **Read it once per load, not once per frame.** The set is installed by a
    /// load and replaced only by another, so a host reads it when a frame
    /// reports `DocumentReplaced` and keeps its textures until the next one.
    public sealed class TextAtlasSet
    {
        /// An empty set — what a document loaded without faces has, and what a
        /// painter draws no text from.
        ///
        /// A value rather than `null`, so a caller has one shape to handle:
        /// "this document stages no runs" and "nobody has read the atlases
        /// yet" are different facts, and only the second is a `null`.
        public static readonly TextAtlasSet Empty = new TextAtlasSet(Array.Empty<TextAtlas>());

        private readonly TextAtlas[] _atlases;

        internal TextAtlasSet(TextAtlas[] atlases)
        {
            _atlases = atlases;
        }

        /// How many atlases this document's runs sample.
        public int Count => _atlases.Length;

        /// The atlas at `index`, which is a `GlyphRun.Atlas`.
        ///
        /// # Exceptions
        ///
        /// `IndexOutOfRangeException` for an index this set does not hold. Not
        /// a clamp: the nearest atlas is a different face's sheet, and
        /// sampling it draws the wrong glyphs rather than failing. A caller
        /// walking a committed table asks `TryGet` instead, because a corrupt
        /// row is a diagnostic there rather than an exception.
        public TextAtlas this[int index] => _atlases[index];

        /// The atlas at `index`, or `false` when this set does not hold one.
        public bool TryGet(uint index, out TextAtlas atlas)
        {
            if (index >= (uint)_atlases.Length)
            {
                atlas = null;
                return false;
            }
            atlas = _atlases[index];
            return true;
        }
    }
}
