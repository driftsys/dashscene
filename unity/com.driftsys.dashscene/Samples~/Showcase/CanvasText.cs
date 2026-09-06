// Rule 4: the faithful Canvas's text, through TextMeshPro's own typesetting.
//
// Story #1444, fairness rule 4 of
// `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
// D2: text is TextMeshPro on the same font files, one text object per glyph run,
// with TextMeshPro's own typesetting. The record is explicit that the showcase is
// not Latin only — `typography` carries three Noto Sans Arabic runs — so this
// takes both families and TextMeshPro's own bidi, joining and mark placement for
// the Arabic ones. That makes `typography`'s reading a comparison of two
// typesetters as well as two renderers, which the record calls the honest shape
// of it.
//
// **Boundary B carries no text, and that is why this file exists.**
// `GlyphQuad.GlyphId` is the OpenType id the shaper produced and `Atlas` maps it
// to placement geometry; no member of `DsFrame` or `DsAtlas` carries a codepoint
// or a family. Reversing the ids is not a route either: an Arabic run's ids are
// positional forms and ligatures with no `cmap` preimage, in visual rather than
// logical order. So the string and the face come from two `ds_demo_*` entry
// points — the library a customer does not install — rather than from a widened
// shipped C ABI, which is the trade `unity/demo-producer`'s own header argues
// for every entry point it carries.
//
// **What this file resolves, and what it refuses.** A face is resolved by the
// key the producer answers for a run's atlas slot, against a `TMP_FontAsset` the
// `DemoFonts` editor step built from the same font file the corpus shapes with.
// A run whose text or whose face cannot be obtained is counted and drawn by
// nothing — never guessed at, because a wrong string is a picture that looks
// right and compares wrong.

using System;
using System.Collections.Generic;
using TMPro;
using UnityEngine;

namespace Driftsys.Dashscene.Samples
{
    /// The font assets and the run text the Canvas's text objects are built
    /// from.
    public sealed class CanvasText
    {
        /// What `DemoFonts` names each generated asset, under `Assets/Resources`.
        ///
        /// The suffix is the producer's own face key, so the two sides agree
        /// without either holding a list: `unity/demo/DemoFonts.cs` builds one
        /// asset per staged font file and names it by the same key.
        public const string ResourcePrefix = "DashsceneFont-";

        private readonly Func<int, string> _faceKey;
        private readonly Func<int, string> _runText;
        private readonly Dictionary<int, TMP_FontAsset> _fonts =
            new Dictionary<int, TMP_FontAsset>();

        /// How many glyph runs were left undrawn, because their text or their
        /// face could not be obtained.
        ///
        /// Reported beside the `drew` line. A run counted here is a run the
        /// painter drew and the Canvas did not, so it is a term of the
        /// comparison rather than a detail of it.
        public int Refused { get; private set; }

        /// `faceKey` answers the producer's key for a cascade slot; `runText`
        /// answers a glyph run's source text. Either may be null, which is a
        /// build carrying no producer — every run is then refused and counted.
        public CanvasText(Func<int, string> faceKey, Func<int, string> runText)
        {
            _faceKey = faceKey;
            _runText = runText;
        }

        /// The text of glyph run `run`, or null when it cannot be obtained.
        public string TextOf(int run)
        {
            if (_runText == null)
            {
                Refused++;
                return null;
            }

            var text = _runText(run);
            if (string.IsNullOrEmpty(text))
            {
                Refused++;
                return null;
            }
            return text;
        }

        /// The font asset for a run's atlas slot, or null when there is none.
        ///
        /// **Cached by slot rather than by key**, because the slot is what a run
        /// carries and the key is one call further away; the two are the same
        /// mapping and this is the direction every caller reads it in.
        public TMP_FontAsset FontFor(int atlas)
        {
            if (_fonts.TryGetValue(atlas, out var cached))
            {
                if (cached == null)
                {
                    Refused++;
                }
                return cached;
            }

            var key = _faceKey?.Invoke(atlas);
            var font = string.IsNullOrEmpty(key)
                ? null
                : Resources.Load<TMP_FontAsset>(ResourcePrefix + key);
            _fonts[atlas] = font;

            if (font == null)
            {
                Refused++;
                Debug.LogWarning(
                    $"[showcase] no {ResourcePrefix}{key} in Resources, so every run "
                    + $"shaped by cascade slot {atlas} is left undrawn. The unity-demo "
                    + "recipes stage the corpus fonts and DemoFonts.Create builds one "
                    + "TMP_FontAsset per staged file.");
            }
            return font;
        }
    }
}
