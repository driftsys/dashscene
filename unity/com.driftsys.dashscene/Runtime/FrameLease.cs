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
        ///
        /// **Refused once disposed.** Every pointer in the frame is invalid the
        /// moment the lease ends, and the next commit replaces the tables they
        /// point into — so handing them out afterwards is the unreadable
        /// failure the lease exists to convert into a reportable one. This is
        /// the guard `DashsceneRuntime.Handle` already applies to a freed
        /// runtime; a stray read from a worker that outlived the dispose is the
        /// case this type's own remarks warn about.
        public DsFrame Frame
        {
            get
            {
                ThrowIfReleased();
                return _frame;
            }
        }

        /// The commit this frame is. Compare only within one document.
        public ulong Generation
        {
            get
            {
                ThrowIfReleased();
                return _frame.Generation;
            }
        }

        /// Discard every cached per-rect thing you hold: this frame's rect
        /// indices do not name what the last one's did.
        public bool DocumentReplaced
        {
            get
            {
                ThrowIfReleased();
                return _frame.DocumentReplacedFlag;
            }
        }

        private void ThrowIfReleased()
        {
            if (_released)
            {
                throw new ObjectDisposedException(
                    nameof(FrameLease),
                    "the lease has ended, so every pointer in this frame is invalid. Acquire "
                    + "another frame rather than holding one past its release.");
            }
        }

        /// Say that this frame was painted, so the commit is marked shown and a
        /// settled scene stops reporting `advanced`.
        ///
        /// Leave it unset if you took the frame and did not paint it — read its
        /// generation, decided nothing was visible, ran out of budget — and it
        /// stays worth drawing. Releasing cannot mean "I consumed this frame"
        /// on its own, because releasing is mandatory where painting is not.
        public void MarkDrawn()
        {
            // Refused after release rather than ignored: the release has already
            // told the library whether the frame was drawn, so a late call
            // cannot take effect and silently dropping it would leave a settled
            // scene reporting that it still has something worth drawing.
            ThrowIfReleased();
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
        // from the member names.** Two of them do not hold the type their name
        // suggests: `extra_fills` holds `PaintKind` and `shapes` holds
        // `VectorField`. The seven `*Range` types are rows of no array here —
        // five are index ranges inside `PaintEntry`, and `StopRange` and
        // `GlyphRange` sit inside `Gradient` and `GlyphRun`.
        //
        // **Row sizes collide**, so this table's ORDER is load-bearing and the
        // length check below cannot see a permutation: rects/shapes/glyph_runs
        // are all 40 bytes, gradients/image_fills/shadows 36,
        // extra_fills/blurs/clip_regions 8, groups/glyph_quads 12,
        // gradient_stops/image_entries 20, gradients/clip_boxes 32. Two
        // same-sized arrays exchanged here match every stride and read the
        // wrong rows.
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
