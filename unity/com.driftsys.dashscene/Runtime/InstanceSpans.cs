// Which instance rows each rect packed to, and the ranges a dirty set names.
//
// **Unity-free on purpose, like `FramePacker` beside it**: `unity/ffi-check`
// compiles `Runtime/` without any Unity reference assembly, so every line here
// is executed by a gate. What sits in `Runtime/Engine/` is compiled by no CI
// job and can only be scanned — which is why the arithmetic lives here and the
// painter composes it rather than restating it.

using System;
using System.Collections.Generic;

namespace Driftsys.Dashscene
{
    /// Which instance rows each rect packed to on the last full pack.
    ///
    /// **A rect is not a row.** It packs to a fill, to stacked fills and a
    /// stroke, to a glyph per drawn quad of every run anchored to it, or to
    /// nothing at all when the packer refuses it — `FramePacker`'s own header
    /// counts fourteen rects to sixteen instances on the document
    /// `just unity-render` draws. So a dirty rect index names a SPAN, and a
    /// commit that leaves every dirty rect's span count where it was can be
    /// applied in place.
    ///
    /// The shape `dashscene-gpu`'s `InstanceSpan` and `dirty_ranges` already
    /// have, ported rather than reinvented so the two painters agree on which
    /// rows a dirty set stands for.
    public sealed class InstanceSpans
    {
        /// One rect's rows: where they start and how many there are.
        public struct Span
        {
            public int Offset;
            public int Count;
        }

        private Span[] _spans = new Span[64];

        /// How many rects the last full pack recorded spans for.
        ///
        /// **Not `_spans.Length`.** The array grows by doubling and never
        /// shrinks, on the same rule every array in `FramePacker` follows, so
        /// what sits past this is whatever an earlier, larger document left.
        public int RectCount { get; private set; }

        /// The rows rect `rect` packed to.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when `rect` is not a rect of the
        /// last full pack.
        public Span Of(int rect)
        {
            if (rect < 0 || rect >= RectCount)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(rect),
                    rect,
                    $"the last full pack recorded {RectCount} rect(s), so this index names no "
                    + "span. A dirty set and a rect table that disagree are two views of one "
                    + "frame that cannot both be right.");
            }

            return _spans[rect];
        }

        /// Begin recording a full pack of `rects` rects.
        internal void Begin(int rects)
        {
            if (_spans.Length < rects)
            {
                Array.Resize(ref _spans, Math.Max(rects, _spans.Length * 2));
            }

            RectCount = rects;
        }

        internal void Set(int rect, int offset, int count)
        {
            _spans[rect] = new Span { Offset = offset, Count = count };
        }

        /// The instance ranges `dirty` names, adjacent ones merged, written into
        /// `into`.
        ///
        /// `dashscene-gpu`'s `dirty_ranges`, with its three rules:
        ///
        /// - **the dirty set's own order**, not a sorted one. The committed set
        ///   is sorted already and this does not require it to be; an unsorted
        ///   set merges less and names the same rows.
        /// - **a span merged into the previous emitted range when adjacent**,
        ///   so a run of changed rects is one upload rather than one each.
        /// - **a zero-count span skipped** — a refused rect, or a layout-only
        ///   container. Its span still records where the next rect begins, so
        ///   it must neither emit an empty range nor break the merge of its
        ///   neighbours.
        ///
        /// `into` is the caller's own list, cleared and refilled, so a steady
        /// frame allocates nothing here. That is R-T4's allocation half, and it
        /// is why this writes into a list rather than returning one.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when a dirty index names no span.
        /// The packer checks the whole set before it chooses this path, so
        /// reaching this from there is not possible; a caller driving it
        /// directly gets the bound rather than another rect's rows.
        public void Coalesce(ReadOnlySpan<uint> dirty, List<(int Offset, int Count)> into)
        {
            if (into == null)
            {
                throw new ArgumentNullException(nameof(into));
            }

            into.Clear();
            for (var d = 0; d < dirty.Length; d++)
            {
                var span = Of((int)dirty[d]);
                if (span.Count == 0)
                {
                    continue;
                }

                var last = into.Count - 1;
                if (last >= 0 && into[last].Offset + into[last].Count == span.Offset)
                {
                    into[last] = (into[last].Offset, into[last].Count + span.Count);
                }
                else
                {
                    into.Add((span.Offset, span.Count));
                }
            }
        }

        /// The spans of a pack that has already happened, for a caller that is
        /// checking [`Coalesce`] rather than packing.
        ///
        /// **A test seam and named as one.** `FramePacker` fills these through
        /// `Begin` and `Set` as it walks; nothing in the package calls this.
        public static InstanceSpans From(params (int Offset, int Count)[] spans)
        {
            if (spans == null)
            {
                throw new ArgumentNullException(nameof(spans));
            }

            var made = new InstanceSpans();
            made.Begin(spans.Length);
            for (var i = 0; i < spans.Length; i++)
            {
                made.Set(i, spans[i].Offset, spans[i].Count);
            }

            return made;
        }
    }
}
