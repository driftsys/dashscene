// One font face and the sheet its shaped glyphs sample, as a host supplies it.
//
// The managed side of `DsFontFace`. It exists because
// `ds_runtime_load_document_with_text` is the only loader that takes a font
// cascade, and until this story nothing wrapped it: the package exposed the
// loaders that need none, so a C# host could open a document whose text shaped
// to nothing and had no way to say otherwise.
//
// **Engine-independent**, like everything else outside `Runtime/Engine/`: a
// face is bytes and a family name, and where those bytes came from — a
// `StreamingAssets` read, a `TextAsset`, an `AssetBundle` — is the host's
// question and not this type's.

using System;

namespace Driftsys.Dashscene
{
    /// One face of one family, with the atlas its glyphs sample.
    ///
    /// **The sheet and its metrics travel together or not at all.** Both null
    /// is the measure-only cascade, where text is shaped and measured and no
    /// glyph is drawn; exactly one null is `DsStatus.Atlas` from the library,
    /// and so is a mixed set across faces where some carry a sheet and some do
    /// not. That rule is the library's and is not repeated as a check here —
    /// `crates/dashscene-ffi/include/dashscene.h` states it on `DsFontFace`,
    /// and a second copy would be a second thing to keep true.
    public sealed class TextFontFace
    {
        /// The family this face belongs to, as the document's text styles name
        /// it.
        ///
        /// **Matched case-insensitively by the cascade**, which is also what
        /// decides the atlas order: faces are grouped by family before the
        /// cascade is flattened, so "Inter" and "inter" are one family and the
        /// atlas a run samples is indexed by the flattened slot rather than by
        /// this face's position in the argument.
        public string Family { get; set; }

        /// The CSS weight, 1..=1000. The library refuses anything outside that
        /// range with `DsStatus.FontFace`, naming the face and the value —
        /// including 0, which is what an uninitialised descriptor carries.
        public ushort Weight { get; set; } = 400;

        /// The index within a font collection; 0 for a file holding one face.
        public uint FaceIndex { get; set; }

        /// The font file's bytes.
        public byte[] FontBytes { get; set; }

        /// The committed MSDF sheet, as a PNG — or null for measure-only.
        public byte[] AtlasPng { get; set; }

        /// The postcard metrics blob beside the sheet — or null for
        /// measure-only.
        ///
        /// **Opaque to a host, and deliberately.** It is an internal
        /// serialization of the glyph table, and nothing here decodes it: the
        /// table a painter needs comes back through
        /// <see cref="DashsceneRuntime.ReadAtlases"/> as boundary-B rows, which
        /// is a gated layout rather than a format a second decoder would have
        /// to track.
        public byte[] AtlasMetrics { get; set; }

        /// Throws unless this face is one the loader could use at all.
        ///
        /// **Only what a null pointer would make undiagnosable.** Everything a
        /// status can name — an empty family, a weight outside the CSS range,
        /// bytes that are not a font, a half-described atlas — is left to the
        /// library, which reports it with the face's index. Checking it twice
        /// is how the two copies come to disagree.
        internal void ThrowIfUnusable(int index)
        {
            if (Family == null)
            {
                throw new ArgumentException(
                    $"face {index} names no family. The library would receive a null pointer "
                    + "and report DS_FONT_FACE without being able to say which field was "
                    + "missing.",
                    nameof(Family));
            }
            if (FontBytes == null)
            {
                throw new ArgumentException(
                    $"face {index} ({Family}) carries no font bytes.",
                    nameof(FontBytes));
            }
        }
    }
}
