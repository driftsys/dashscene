// A committed frame's row arrays, as spans, for a caller that cannot write
// `unsafe`.
//
// **This exists for the samples, and the asymmetry is the whole point.** The
// package's own assembly definition sets `allowUnsafeCode`, so `FramePacker`
// reads `DsSlice.Ptr` as a raw pointer directly. A file under `Samples~/` is
// copied into a consumer's `Assets/` and compiles into `Assembly-CSharp`, which
// does not allow unsafe code and which no file in this repository configures —
// so a sample that wanted to read the tables had no way to, short of a
// `Marshal.PtrToStructure` per row. This is that seam: one `unsafe` member
// inside the package, and none in the sample.
//
// **The lease's rules are unchanged and are not restated by this type.** The
// span borrows the runtime's own memory: it is valid until
// `ds_runtime_release_frame` returns, it is never freed by the reader, and it
// must not outlive the `FrameLease` it came from. `FrameLease`'s own remarks are
// the contract; a span makes the bytes reachable, not durable.

using System;

namespace Driftsys.Dashscene
{
    /// One committed frame's arrays, as `ReadOnlySpan<T>` over the runtime's
    /// own rows.
    public static class FrameRows
    {
        /// The rows of `slice`, read as `T`.
        ///
        /// **The stride is compared against `T` before the span is made**, which
        /// is what turns the one mistake this signature admits — naming the
        /// wrong row type for an array — into an exception rather than into
        /// geometry read from the middle of a row. `FrameLease.ValidateStrides`
        /// already holds every array to this package's declaration at acquire
        /// (R-E17); this holds the reader to the array.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentException`] when the slice's stride is not `sizeof(T)`, and
        /// [`ArgumentOutOfRangeException`] when it is longer than `int.MaxValue`
        /// rows — a bound `ReadOnlySpan<T>` cannot express, and one no document
        /// reaches.
        public static unsafe ReadOnlySpan<T> Of<T>(DsSlice slice)
            where T : unmanaged
        {
            var stride = slice.StrideAsLong;
            if (stride != sizeof(T))
            {
                throw new ArgumentException(
                    $"this array holds {stride}-byte rows and {typeof(T).Name} is "
                    + $"{sizeof(T)} bytes. Reading it as {typeof(T).Name} would take each "
                    + "row from a different offset than the one it starts at.",
                    nameof(slice));
            }

            var rows = slice.CountAsLong;
            if (rows > int.MaxValue)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(slice),
                    rows,
                    "a span cannot address more than int.MaxValue rows.");
            }

            // `Ptr` is null exactly when `Count` is 0, which `DsSlice` documents,
            // so an empty table needs no branch of its own here: the span
            // constructor accepts a null pointer with a zero length.
            return new ReadOnlySpan<T>((void*)slice.Ptr, (int)rows);
        }
    }
}
