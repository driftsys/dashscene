// Whether a heap table's rows are already in the buffer, kept where a gate can
// execute it.
//
// `CommitPacer.cs`'s argument, applied a fourth time. This comparison decides
// whether `BrgPainter.Upload` sends a table at all, so getting it wrong in the
// "already there" direction freezes that table's contents for the life of the
// painter — a document whose paint changes then draws the colours of the commit
// before it, with a correct-looking first frame and no diagnostic. Left inside
// `Runtime/Engine/` it would be compiled by no CI job and executed by nothing:
// `unity/render-gate` draws one static document, so a run of it cannot tell
// "skip when unchanged" from "skip always".
//
// Here, `unity/ffi-check` runs it on every pull request.

using System;

namespace Driftsys.Dashscene
{
    /// The upload skip, as arithmetic over two arrays.
    public static class HeapUpload
    {
        /// Whether `source`'s live floats are already what was last uploaded.
        ///
        /// `uploaded` is how many live floats the last upload carried, or a
        /// negative number where the buffer behind them was (re)created and
        /// holds nothing this painter wrote. `staging` is that upload's own
        /// copy, so comparing against it is comparing against the buffer.
        ///
        /// **The length as well as the contents.** The staging array is longer
        /// than what is live, and what sits past the live floats is whatever an
        /// earlier frame left there. A table that grew into those floats would
        /// compare equal over its new length while the rows past its old one
        /// were never sent, so a changed length is a change whatever the
        /// contents say.
        ///
        /// **Spans, so the comparison is vectorised.** The caller has already
        /// paid one linear pass over these floats to build them and pays
        /// another to copy them, and a hand-written scalar loop here would add
        /// a third that is several times slower than either — on every frame
        /// whose table did change, which is every frame of a transition.
        /// `MemoryExtensions.SequenceEqual` is the one the runtime vectorises.
        ///
        /// It compares floats through `IEquatable&lt;float&gt;`, so `NaN` equals
        /// `NaN` and `-0` equals `0`. Both readings are the right ones here:
        /// the question is whether the buffer already holds what would be sent,
        /// and a `NaN` that was uploaded is the `NaN` the source still carries.
        public static bool AlreadyUploaded(float[] staging, float[] source, int uploaded, int floats)
        {
            if (uploaded != floats)
            {
                return false;
            }

            var live = Math.Min(floats, source.Length);
            return staging.AsSpan(0, live).SequenceEqual(source.AsSpan(0, live));
        }
    }
}
