// The shader keywords one paint kind set enables, kept where a gate can execute
// the mapping.
//
// `HeapUpload.cs`'s argument, applied again. `BrgPainter.ApplyKindSet` toggles
// these on every drawn frame from `ds_runtime_kind_set`, and a mapping that
// named the wrong keyword for a bit compiles the clip loop out of a document
// that clips — a wrong picture with no diagnostic, because
// `Material.EnableKeyword` on a keyword the shader does not declare selects
// nothing and reports nothing. Left inside `Runtime/Engine/` the mapping would
// be compiled by no CI job and executed by nothing: `unity/ffi-check` compiles
// `Runtime/**` and excludes `Runtime/Engine/**`, whose every file references
// `UnityEngine`, and `unity/render-gate` needs a Unity editor.
//
// Here, `unity/ffi-check` runs it on every pull request, and
// `unity/package-gate`'s `kind_set_keywords.rs` holds these spellings against
// the arms `Runtime/Shaders/DashsceneInstance.hlsl` guards with them.

using System;
using System.Collections.Generic;

namespace Driftsys.Dashscene
{
    /// The `DS_HAS_*` keywords a document's paint kind set enables.
    ///
    /// The set itself is `ds_runtime_kind_set`'s word: bit 0 set when the
    /// document clips, bit 1 when it strokes. It is a census of the tables the
    /// last commit produced, so a painter reads it on every drawn frame rather
    /// than once per load.
    public static class KindSetKeywords
    {
        /// Bit 0 — the document names at least one clip box.
        ///
        /// Undefined, the shading's `DsClipCoverage` returns the unclipped
        /// answer as a literal and its loop, its box loads and its distances
        /// are compiled out.
        public const string Clips = "DS_HAS_CLIPS";

        /// Bit 1 — the document's paint table interned at least one stroke.
        ///
        /// Undefined, the stroke arm and the table read behind it are compiled
        /// out. **Declared by the three node shaders and not by the text
        /// one**: the text arm shades a glyph run and returns before that
        /// branch, so the keyword would remove no code there and R-E6's
        /// `KeepAll` would still compile both halves of it.
        public const string Strokes = "DS_HAS_STROKES";

        /// The bits this build knows.
        ///
        /// **A mask rather than an equality**, which `dashscene.h` asks for by
        /// name: bits above these two are reserved and read as zero today, so
        /// a painter comparing the whole word against a literal would stop
        /// recognising its own set the first time the library sets a third.
        public const uint Known = 0x3u;

        /// The keywords `bits` names, in bit order.
        ///
        /// Empty for a document that neither clips nor strokes, and that is the
        /// fast path rather than a fallback: the general shading is the variant
        /// with every keyword undefined, so an empty set is the one that
        /// removes the most code.
        ///
        /// **In bit order, and that is a property rather than an accident.**
        /// `unity/ffi-check` compares the sequence, so a mapping that returned
        /// the same two names the other way round fails there rather than
        /// leaving two callers to disagree about which is which.
        ///
        /// **`KeywordsFor(Known)` is the whole set**, which is what
        /// `BrgPainter.ApplyKindSet` walks to decide what to turn off. A second
        /// list of every keyword would be a second place for one to be
        /// forgotten.
        ///
        /// Allocates one array per call, and the caller calls it only on a
        /// frame whose bits moved — a document interning its first stroke, or
        /// a replacement. R-T4 bounds what a steady frame spends, and a steady
        /// frame does not reach here.
        public static string[] KeywordsFor(uint bits)
        {
            var masked = bits & Known;
            var count = 0;
            foreach (var pair in ByBit)
            {
                if ((masked & pair.Key) != 0u)
                {
                    count++;
                }
            }

            if (count == 0)
            {
                return Array.Empty<string>();
            }

            var keywords = new string[count];
            var at = 0;
            foreach (var pair in ByBit)
            {
                if ((masked & pair.Key) != 0u)
                {
                    keywords[at++] = pair.Value;
                }
            }

            return keywords;
        }

        /// One row per bit, in bit order.
        ///
        /// **A table rather than a chain of `if`s**, which is
        /// `PackDiagnostics.Describe`'s arrangement over its own flag set and
        /// the reason it reads that way: a third bit is one row here, and the
        /// order the rows are declared in IS the order `KeywordsFor` returns.
        /// Two hand-written chains — one to count, one to fill — would be two
        /// places for a later bit to be added to one of them.
        ///
        /// [`Known`] is the union of these keys and is asserted against them by
        /// [`KeywordsFor`]'s own callers through `unity/ffi-check`, which drives
        /// `KeywordsFor(Known)` as the whole set.
        private static readonly KeyValuePair<uint, string>[] ByBit =
        {
            new KeyValuePair<uint, string>(0x1u, Clips),
            new KeyValuePair<uint, string>(0x2u, Strokes),
        };
    }
}
