// Where a row sits in the instance buffer, in words.
//
// **The layout is `BrgPainter.FillStaging`'s and it is stated here**, outside
// `Runtime/Engine/`, so `unity/ffi-check` executes it. Nothing in this file
// references Unity: a word index is arithmetic over three numbers the painter
// already holds — the batch stride, the per-batch capacity, and the row — and
// the painter composes these members rather than restating the arithmetic where
// no gate can reach it.

using System;
using System.Collections.Generic;

namespace Driftsys.Dashscene
{
    /// Which of the two paths an instance upload took.
    public enum UploadKind
    {
        /// Nothing was sent — a frame with no instances in it.
        None,

        /// The whole staging array, every batch and the capacity past the live
        /// rows included.
        Whole,

        /// Only the rows the commit's dirty rects name.
        Ranges,
    }

    /// What one frame sent to the instance buffer.
    ///
    /// **A reading, not a claim, and [`Words`] is the reading.** The painter
    /// counts the words it actually passed to `GraphicsBuffer.SetData` and
    /// reports that; [`Uploads`] counts the calls. So a version that chose the
    /// ranged path and then sent the whole array reports the whole array's
    /// words, which is what lets a gate fail on it.
    public readonly struct InstanceUpload
    {
        public InstanceUpload(UploadKind kind, int uploads, int words, int rows)
        {
            Kind = kind;
            Uploads = uploads;
            Words = words;
            Rows = rows;
        }

        /// Which path this frame took.
        public UploadKind Kind { get; }

        /// How many `SetData` calls it made.
        ///
        /// **Not the number of dirty ranges.** One range is cut at batch
        /// boundaries and each piece is five stream writes, so a single
        /// contiguous range of rows is at least five calls.
        public int Uploads { get; }

        /// How many 4-byte words went to the device, counted as they were sent.
        ///
        /// **The one member that is purely a reading**, on both paths, which is
        /// why a gate should judge this rather than [`Rows`] when it is asking
        /// whether the transfer changed size.
        public int Words { get; }

        /// How many instance rows the frame is about.
        ///
        /// **The two kinds answer different questions, and only one of them is
        /// derived.** On [`UploadKind.Ranges`] this is [`RowsIn`] of [`Words`]
        /// — the rows actually written. On [`UploadKind.Whole`] it is the live
        /// instance count, which is NOT what the transfer carried: that array
        /// also holds every batch's head and the capacity past the live rows,
        /// so its word count divided by twenty is not a row count at all.
        /// Comparing the two kinds' `Rows` compares the picture's size against
        /// the delta's, which is the comparison R-T4 is about; comparing the
        /// transfers is what [`Words`] is for.
        public int Rows { get; }

        /// The instance rows `words` words carry, across an instance's five
        /// streams.
        public static int RowsIn(int words)
        {
            return words / (StreamLayout.Streams * StreamLayout.WordsPerRow);
        }

        /// Whether a frame may send ranges rather than the whole array.
        ///
        /// **Five conditions, and stated here so that a gate can drive them.**
        /// The painter that asks this question lives in `Runtime/Engine/`,
        /// which nothing compiles and nothing runs — so a version of this
        /// predicate written there could only be scanned as text, and a scan
        /// that looks for three clause spellings passes just as happily when
        /// the `&amp;&amp;` between them becomes `||`. Here `unity/ffi-check`
        /// executes it.
        ///
        /// - `packWasPartial` — the packer rewrote only the dirty rects' rows.
        ///   Its arrays hold the whole commit either way, so a `false` here
        ///   costs a whole upload and nothing else.
        /// - `uploadedBefore` — this painter has sent something. Without it a
        ///   `packed` of 1 against an `uploaded` left at 0 satisfies the
        ///   arithmetic below on a device that received nothing.
        /// - `uploaded + 1 == packed` — the ranges are a delta against the
        ///   commit the device holds, not against one it never received. A
        ///   `Draw` that throws between the pack and the upload is what opens
        ///   that gap.
        /// - `sameBuffer` — the `GraphicsBuffer` was not reallocated. A fresh
        ///   one holds nothing this painter wrote, and a zeroed staging array
        ///   would leave every row outside the ranges empty.
        /// - `sameHead` — the batch heads hold the transform the painter is
        ///   drawing with. **Not a property of the commit at all**, which is
        ///   why it is a condition rather than an assumption: the document's
        ///   object-to-world is set by the HOST, on its own schedule, and a
        ///   ranged upload rewrites instance rows and never a batch head. A
        ///   host that moved the document and then drew a commit the other four
        ///   conditions admit would keep drawing at the previous transform
        ///   until something unrelated forced a whole upload.
        public static bool CanSendRanges(
            bool packWasPartial,
            bool uploadedBefore,
            ulong uploaded,
            ulong packed,
            bool sameBuffer,
            bool sameHead)
        {
            return packWasPartial
                   && uploadedBefore
                   && uploaded + 1 == packed
                   && sameBuffer
                   && sameHead;
        }

        public override string ToString()
        {
            return $"{Kind} ({Uploads} call(s), {Words} word(s), {Rows} row(s))";
        }

    }

    /// The instance buffer's layout: batches of a shared head and five streams.
    ///
    /// Each batch window opens with [`HeadWords`] words of data every instance
    /// in it shares, then five streams of `capacity` rows each — `Quad`,
    /// `Corners`, `Shade`, `Pivot`, `Paint`, in that order. So one instance's
    /// five properties sit at five DISJOINT offsets, which is what makes a row
    /// range five uploads rather than one, and it is
    /// `ComputeDOTSInstanceDataAddress`'s own layout rather than a choice made
    /// here.
    ///
    /// **A span can straddle a batch boundary.** `FramePacker` never sees
    /// `_instancesPerBatch` — it packs a document, not a buffer — so a rect's
    /// rows can begin in one window and end in the next, and every global row
    /// range has to be cut at those boundaries before it becomes an offset.
    public static class StreamLayout
    {
        /// The words each batch window opens with, before the first stream.
        ///
        /// The zero `float4` a metadata value of 0 resolves to, then
        /// `unity_ObjectToWorld` and `unity_WorldToObject` as `float3x4` —
        /// 16 + 48 + 48 bytes. `BrgPainter.HeadBytes` is the same quantity in
        /// bytes and `unity/package-gate` holds the two to each other.
        public const int HeadWords = 28;

        /// `Quad`, `Corners`, `Shade`, `Pivot`, `Paint`.
        public const int Streams = 5;

        /// One row of one stream is a `float4`.
        public const int WordsPerRow = 4;

        /// Cut a global row range at batch boundaries, into `into`.
        ///
        /// Each piece is the batch it lands in, the first row WITHIN that batch,
        /// and how many rows of the range that batch holds. `into` is the
        /// caller's own list, cleared and refilled, so a steady frame allocates
        /// nothing — which is why this is not an iterator method.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when `capacity` is not positive, when
        /// `first` is negative, or when `count` is negative. A zero `count`
        /// leaves `into` empty.
        public static void Cut(
            int capacity,
            int first,
            int count,
            List<(int Batch, int Row, int Rows)> into)
        {
            if (capacity <= 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(capacity), capacity, "a batch holds at least one row.");
            }

            if (first < 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(first), first, "a row range starts at a row.");
            }

            if (count < 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(count), count, "a row range holds no negative number of rows.");
            }

            if (into == null)
            {
                throw new ArgumentNullException(nameof(into));
            }

            into.Clear();
            while (count > 0)
            {
                var batch = first / capacity;
                var row = first % capacity;
                var take = Math.Min(count, capacity - row);
                into.Add((batch, row, take));
                first += take;
                count -= take;
            }
        }

        /// The five word ranges holding rows `[row, row + rows)` of batch
        /// `batch`, into `into`.
        ///
        /// **Absolute, not relative to the batch.** The batch head
        /// (`batch * strideWords`) is added here rather than by the caller,
        /// because that addition is the one piece of this arithmetic a scan
        /// cannot check and an executed gate can: a version that dropped it
        /// would upload every batch's rows over batch 0 and draw a picture no
        /// golden covers.
        ///
        /// `strideWords` is the painter's `_batchStrideBytes / 4` and is NOT
        /// derivable from `capacity`: under the `ConstantBuffer` rung the
        /// stride is rounded up to the device's own alignment, so a window can
        /// be wider than the head and its five streams (R-E15).
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when the rows named do not fit in one
        /// batch of `capacity`, or when `strideWords` is narrower than a batch
        /// of `capacity` needs.
        public static void Ranges(
            int capacity,
            int strideWords,
            int batch,
            int row,
            int rows,
            List<(int First, int Count)> into)
        {
            if (capacity <= 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(capacity), capacity, "a batch holds at least one row.");
            }

            if (batch < 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(batch), batch, "a batch is named by its index.");
            }

            if (row < 0 || rows < 0 || row + rows > capacity)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(rows),
                    rows,
                    $"rows [{row}, {row + rows}) do not fit a batch of {capacity}. `Cut` is what "
                    + "divides a global range into pieces one batch holds.");
            }

            if (strideWords < HeadWords + Streams * capacity * WordsPerRow)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(strideWords),
                    strideWords,
                    $"a batch of {capacity} rows needs "
                    + $"{HeadWords + Streams * capacity * WordsPerRow} words, so this stride "
                    + "would make one batch's streams overlap the next batch's head.");
            }

            if (into == null)
            {
                throw new ArgumentNullException(nameof(into));
            }

            into.Clear();
            var head = batch * strideWords + HeadWords;
            var words = rows * WordsPerRow;
            for (var stream = 0; stream < Streams; stream++)
            {
                into.Add((head + stream * capacity * WordsPerRow + row * WordsPerRow, words));
            }
        }
    }
}
