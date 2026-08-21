// The committed frame, borrowed under a lease.
//
// `ds_runtime_acquire_frame` hands out pointers into the runtime's committed
// tables, and a commit is the only thing that replaces them — so while a lease
// is outstanding the library refuses every call that would commit. That refusal
// is what makes the borrowed views safe rather than merely documented.
//
// **A forgotten release refuses every later tick.** That is the intended
// failure mode: it is diagnosable, where reading a freed table is not. This
// type exists so the release is a `using` rather than a thing to remember.

using System;
using System.Runtime.InteropServices;
using Driftsys.Dashscene.BoundaryB;

namespace Driftsys.Dashscene
{
    /// A row array whose stride did not match this package's declaration.
    ///
    /// `docs/specification/07-embedding-and-distribution.md` R-E17. The library
    /// and the C# package came from different commits, and reading the array
    /// would draw geometry from a row layout that is not the one it holds.
    public class DashsceneStrideMismatchException : Exception
    {
        /// The array whose stride disagreed, by its name on `DsFrame`.
        public string Array { get; }

        /// The row size this package declares.
        public long Expected { get; }

        /// The row size the library build reports.
        public long Actual { get; }

        internal DashsceneStrideMismatchException(string array, long expected, long actual)
            : base($"dashscene row layout mismatch on {array}: this package declares a "
                   + $"{expected}-byte row and the native library reports {actual}. "
                   + "The library and the C# package must come from one commit.")
        {
            Array = array;
            Expected = expected;
            Actual = actual;
        }
    }

    /// A borrowed committed frame. Dispose ends the lease.
    ///
    /// **Release after your readers finish, not when the call that dispatched
    /// them returns.** If you hand these pointers to worker threads, dispose
    /// once those workers have completed — for a Unity host that means after
    /// Unity completes the `JobHandle`, not on return from `OnPerformCulling`.
    /// The workers make no call into this library; they read memory, so nothing
    /// here is thread-affine for them. The acquire and the release are the only
    /// calls, and both are on the runtime's own thread.
    public sealed class FrameLease : IDisposable
    {
        private readonly DashsceneRuntime _runtime;
        private DsFrame _frame;
        private int _drawn;
        private bool _released;

        internal FrameLease(DashsceneRuntime runtime, DsFrame frame)
        {
            _runtime = runtime;
            _frame = frame;
        }

        /// The arrays. Valid until this lease is disposed.
        public DsFrame Frame => _frame;

        /// The commit this frame is. Compare only within one document.
        public ulong Generation => _frame.Generation;

        /// Discard every cached per-rect thing you hold: this frame's rect
        /// indices do not name what the last one's did.
        public bool DocumentReplaced => _frame.DocumentReplacedFlag;

        /// Say that this frame was painted, so the commit is marked shown and a
        /// settled scene stops reporting `advanced`.
        ///
        /// Leave it unset if you took the frame and did not paint it — read its
        /// generation, decided nothing was visible, ran out of budget — and it
        /// stays worth drawing. Releasing cannot mean "I consumed this frame"
        /// on its own, because releasing is mandatory where painting is not.
        public void MarkDrawn()
        {
            _drawn = 1;
        }

        /// Ends the lease. Every pointer in the frame is invalid once this
        /// returns.
        public void Dispose()
        {
            if (_released)
            {
                return;
            }

            _released = true;
            _runtime.ReleaseLease(_drawn);
        }

        // The nineteen arrays and the row each one holds.
        //
        // **Derived from `frame_of` in `crates/dashscene-ffi/src/lib.rs`, not
        // from the member names.** Five of them do not hold the type their name
        // suggests: `extra_fills` holds `PaintKind`, `strokes` holds `Stroke`,
        // `shapes` holds `VectorField`, `shadows` holds `Shadow` and `blurs`
        // holds `Blur`. The `*Range` types are index ranges inside `PaintEntry`
        // and are not rows of any array here.
        //
        // `Dirty` and `ImagePayload` hold primitives rather than boundary-B
        // rows — `uint` rect indices and the raw payload pool.
        private static readonly (string Name, int Size)[] RowSizes =
        {
            ("rects", Marshal.SizeOf<RectEntry>()),
            ("groups", Marshal.SizeOf<GroupComposite>()),
            ("dirty", sizeof(uint)),
            ("paint_entries", Marshal.SizeOf<PaintEntry>()),
            ("extra_fills", Marshal.SizeOf<PaintKind>()),
            ("strokes", Marshal.SizeOf<Stroke>()),
            ("shapes", Marshal.SizeOf<VectorField>()),
            ("solids", Marshal.SizeOf<Color>()),
            ("gradients", Marshal.SizeOf<Gradient>()),
            ("gradient_stops", Marshal.SizeOf<GradientStop>()),
            ("image_fills", Marshal.SizeOf<ImageFill>()),
            ("shadows", Marshal.SizeOf<Shadow>()),
            ("blurs", Marshal.SizeOf<Blur>()),
            ("clip_regions", Marshal.SizeOf<ClipRegion>()),
            ("clip_boxes", Marshal.SizeOf<ClipBox>()),
            ("image_entries", Marshal.SizeOf<ImageEntry>()),
            ("image_payload", sizeof(byte)),
            ("glyph_runs", Marshal.SizeOf<GlyphRun>()),
            ("glyph_quads", Marshal.SizeOf<GlyphQuad>()),
        };

        /// Every array's stride, in `RowSizes` order.
        ///
        /// Written as one expression beside `RowSizes` so the two orders cannot
        /// drift apart silently: a member added to one and not the other is a
        /// length mismatch the check below reports.
        private static long[] StridesOf(DsFrame frame)
        {
            return new[]
            {
                frame.Rects.StrideAsLong,
                frame.Groups.StrideAsLong,
                frame.Dirty.StrideAsLong,
                frame.PaintEntries.StrideAsLong,
                frame.ExtraFills.StrideAsLong,
                frame.Strokes.StrideAsLong,
                frame.Shapes.StrideAsLong,
                frame.Solids.StrideAsLong,
                frame.Gradients.StrideAsLong,
                frame.GradientStops.StrideAsLong,
                frame.ImageFills.StrideAsLong,
                frame.Shadows.StrideAsLong,
                frame.Blurs.StrideAsLong,
                frame.ClipRegions.StrideAsLong,
                frame.ClipBoxes.StrideAsLong,
                frame.ImageEntries.StrideAsLong,
                frame.ImagePayload.StrideAsLong,
                frame.GlyphRuns.StrideAsLong,
                frame.GlyphQuads.StrideAsLong,
            };
        }

        /// Compares every array's stride against this package's row size.
        ///
        /// **R-E17, and it covers the empty arrays too.** The library reports a
        /// stride for an array with no rows, precisely so a host can validate
        /// all of them at the top of the frame — most documents leave several
        /// empty, and a scene with no gradients, no images and no blurs leaves
        /// most of them empty. Checking only the populated ones would let a
        /// layout change ride in on the first document that uses the table.
        internal static void ValidateStrides(DsFrame frame)
        {
            var strides = StridesOf(frame);
            if (strides.Length != RowSizes.Length)
            {
                throw new InvalidOperationException(
                    $"FrameLease has {RowSizes.Length} row sizes and {strides.Length} strides. "
                    + "A DsFrame member was added to one list and not the other.");
            }

            for (var i = 0; i < strides.Length; i++)
            {
                if (strides[i] != RowSizes[i].Size)
                {
                    throw new DashsceneStrideMismatchException(
                        RowSizes[i].Name, RowSizes[i].Size, strides[i]);
                }
            }
        }
    }
}
