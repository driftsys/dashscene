// The thread-cost line's sampling, kept beside its arithmetic and for the same
// reason: `unity/ffi-check` executes what sits here and excludes what sits
// under `Runtime/Engine/`. See `ThreadCostMath.cs`.

namespace Driftsys.Dashscene
{
    /// <summary>One reported sample of the host's per-thread frame time.</summary>
    ///
    /// **Returned rather than printed**, as `FrameCostSample` is: the
    /// arithmetic is then separable from where the line goes, which on Android
    /// is logcat.
    public sealed class ThreadCostSample
    {
        /// What was drawn — the host's own label for the entry.
        public string Entry;

        /// The extent it was drawn at, which issue #1236 makes part of the row
        /// rather than a property of the run.
        public int Width;

        /// <summary>The extent's height. See <see cref="Width"/>.</summary>
        public int Height;

        /// <summary>How many drawn frames this sample covers.</summary>
        public int Frames;

        /// The mean of Unity's `Main Thread` counter over the sample, in
        /// milliseconds. It carries the engine floor as well as the renderer's
        /// work; the empty entry's line is what that floor is subtracted from.
        public double MainMean;

        /// <summary>The sample's median main-thread frame time, in milliseconds.</summary>
        public double MainP50;

        /// <summary>The sample's 95th percentile main-thread frame time, in milliseconds.</summary>
        public double MainP95;

        /// <summary>The sample's slowest main-thread frame, in milliseconds.</summary>
        ///
        /// **Four statistics and not two**, because D1 of
        /// `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
        /// states its at-rest test against the floor's own p95 and requires the
        /// reading to state mean, p50, p95 and max for both renderers — the
        /// same four `FrameCostSample` reports, so the two lines of one run are
        /// read in the same units and the same shape.
        public double MainMax;

        /// The mean of Unity's `Render Thread` counter, in milliseconds, or
        /// null where this player does not carry that counter.
        ///
        /// **Null rather than zero, and that is the whole rule.** A
        /// `ProfilerRecorder` over a counter Unity has not registered is not an
        /// error: it stays invalid and reports `LastValue` 0 for ever. A zero
        /// here is indistinguishable from a render thread that costs nothing,
        /// and a zero in <see cref="CanvasRebuildMean"/> reads as a Canvas that
        /// rebuilds nothing — which is the finding this instrument exists to be
        /// able to make. `Line` prints an em dash for a null, so no row of any
        /// table carries a measurement that was not taken.
        ///
        /// Measured on 6000.3.23f1, macOS/Metal, 2026-09-05: a `-batchmode`
        /// player carries no `Render Thread` counter, and one that draws no
        /// Canvas carries no `Canvas.SendWillRenderCanvases`.
        public double? RenderMean;

        /// <summary>The sample's median render-thread frame time, in milliseconds.</summary>
        /// Null where the counter is not recordable. See <see cref="RenderMean"/>.
        public double? RenderP50;

        /// <summary>The sample's 95th percentile render-thread frame time, in milliseconds.</summary>
        /// Null where the counter is not recordable. See <see cref="RenderMean"/>.
        public double? RenderP95;

        /// <summary>The sample's slowest render-thread frame, in milliseconds.</summary>
        /// Null where the counter is not recordable. See <see cref="RenderMean"/>.
        public double? RenderMax;

        /// The mean of `Canvas.SendWillRenderCanvases` plus `Canvas.BuildBatch`,
        /// in milliseconds — the Canvas rebuild the frame-cost line cannot see
        /// at all, and which D2's rule 5 predicts is zero at rest. Null where
        /// this player carries neither marker. See <see cref="RenderMean"/>.
        ///
        /// **A mean and no percentiles**, unlike the two thread terms. D1's
        /// four-statistic rule is stated over the main-thread cost, which is
        /// what its at-rest test compares; the rebuild term answers a different
        /// question — whether the Canvas rebuilds at all — and D2's rule 5 is
        /// met or not met by a mean of zero.
        public double? CanvasRebuildMean;

        /// Managed bytes allocated per drawn frame, from `GC Allocated In
        /// Frame`. D3's allocation rule is stated at zero for a steady frame,
        /// which is exactly why an unrecorded counter must not report zero
        /// here: it would answer the rule. Null says not recorded. See
        /// <see cref="RenderMean"/>.
        public long? GcAllocBytesPerFrame;

        /// The em dash a term whose counter this player does not carry reports.
        ///
        /// **The same character `measure/android/frame-table.py` prints for a
        /// cell it has no reading for**, so both halves of this apparatus say
        /// "not measured" the same way, and that parser accepts it.
        public const string Unrecorded = "—";

        /// <summary>The line, in the shape the record quotes.</summary>
        ///
        /// An em dash where a term's counter is not recordable on this player,
        /// never a zero.
        public string Line()
        {
            return string.Format(
                System.Globalization.CultureInfo.InvariantCulture,
                "{0} at {1}x{2} over {3} frames — "
                + "main mean {4:F2} p50 {5:F2} p95 {6:F2} max {7:F2} ms, "
                + "render mean {8} p50 {9} p95 {10} max {11} ms, "
                + "canvas {12} ms, gc {13} B/frame",
                Entry, Width, Height, Frames,
                MainMean, MainP50, MainP95, MainMax,
                Milliseconds(RenderMean), Milliseconds(RenderP50),
                Milliseconds(RenderP95), Milliseconds(RenderMax),
                Milliseconds(CanvasRebuildMean),
                GcAllocBytesPerFrame.HasValue
                    ? GcAllocBytesPerFrame.Value.ToString(
                        System.Globalization.CultureInfo.InvariantCulture)
                    : Unrecorded);
        }

        private static string Milliseconds(double? value)
        {
            return value.HasValue
                ? value.Value.ToString("F2", System.Globalization.CultureInfo.InvariantCulture)
                : Unrecorded;
        }
    }

    /// <summary>Collects per-thread frame times until a full sample is in hand.</summary>
    public sealed class ThreadCostAccumulator
    {
        /// How many drawn frames one report covers.
        ///
        /// **The same 240 as `DashsceneFrameCost.TimingSample`**, so the two
        /// lines of one run cover the same frames and can be read together.
        public const int Sample = 240;

        /// How many frames after a key change are discarded.
        ///
        /// **Not a tidiness measure.** An entry's first frames carry its load
        /// and, on the Canvas side, its mesh bakes — and this instrument's terms
        /// are exactly the ones those land in: a Canvas rebuild is
        /// `Canvas.BuildBatch`, and a bake allocates. `DashsceneFrameCost` needs
        /// no equivalent because its own header says every published row is a
        /// first sample carrying warm-up, and a reader drops it; a line whose
        /// subject is the rebuild term cannot leave that to the reader.
        public const int WarmUp = 60;

        private readonly double[] _main = new double[Sample];
        private readonly double[] _render = new double[Sample];
        private readonly double[] _canvas = new double[Sample];

        /// Bytes allocated across the frames collected so far. Summed rather
        /// than averaged per frame, so the reported figure divides one total.
        private long _gc;

        /// How many frames of the current sample are collected.
        private int _n;

        /// How many warm-up frames are still to be discarded.
        private int _skip;

        /// What the sample in hand is a sample *of* — the entry and the extent
        /// together, for `DashsceneFrameCost`'s reason: either changing makes
        /// the samples either side of it describe different work.
        ///
        /// **Three fields and not one composed string.** `DashsceneFrameCost`
        /// composes `entry + "@" + width + "x" + height` per frame, which costs
        /// two `int.ToString()` allocations and a concatenation. Measured with
        /// `GC.GetAllocatedBytesForCurrentThread`: **192 managed bytes per push**
        /// for the render gate's key and 200 for the device's longer entry
        /// name. That is affordable in a sample; it is
        /// not affordable here, because `unity/render-gate` pushes this
        /// instrument from its own `Update`, and D3 states the zero-allocation
        /// rule as "a steady frame shall allocate zero managed bytes on the
        /// main thread, measured by `GC.GetAllocatedBytesForCurrentThread`
        /// around the host's `Update`" — in that gate, which is story #1445's
        /// measurement. An instrument that allocated would be measuring itself.
        private string _entry;
        private int _width;
        private int _height;

        /// False until the first push, so a first entry of `null` at 0x0 — a
        /// host that has not resolved its label yet — is still a key change.
        private bool _keyed;

        /// Whether each optional term was recorded on every frame collected.
        ///
        /// A recorder's validity is decided when it is started and does not
        /// change, so in practice these are constant across a window — tracked
        /// rather than assumed, so a term is reported only when every frame of
        /// the sample carried it.
        private bool _renderRecorded = true;
        private bool _canvasRecorded = true;
        private bool _gcRecorded = true;

        /// <summary>Takes one drawn frame, and returns a sample when one is full.</summary>
        ///
        /// The four counter arguments are Unity's own readings for the frame
        /// that has just ended, in nanoseconds and bytes. They are taken as
        /// arguments rather than read here so this class compiles — and is
        /// executed by `unity/ffi-check` — outside Unity.
        ///
        /// **Three of them are nullable and `mainNs` is not.** A player that
        /// cannot record the main-thread counter has no instrument at all, and
        /// `DashsceneThreadCost` refuses to arm on one; the other three are
        /// terms a player can legitimately lack. Null travels through to an em
        /// dash on the line rather than to a zero.
        public ThreadCostSample Push(
            string entry,
            int width,
            int height,
            long mainNs,
            long? renderNs,
            long? canvasNs,
            long? gcBytes)
        {
            if (!_keyed || width != _width || height != _height || entry != _entry)
            {
                _keyed = true;
                _entry = entry;
                _width = width;
                _height = height;
                _n = 0;
                _gc = 0;
                _skip = WarmUp;
                _renderRecorded = true;
                _canvasRecorded = true;
                _gcRecorded = true;
            }

            if (_skip > 0)
            {
                _skip--;
                return null;
            }

            _main[_n] = ThreadCostMath.NsToMs(mainNs);
            // **`&=`, so one frame without a term disqualifies the whole
            // sample.** A mean over a window that carried the counter for part
            // of it describes neither part.
            _renderRecorded &= renderNs.HasValue;
            _canvasRecorded &= canvasNs.HasValue;
            _gcRecorded &= gcBytes.HasValue;
            _render[_n] = ThreadCostMath.NsToMs(renderNs ?? 0);
            _canvas[_n] = ThreadCostMath.NsToMs(canvasNs ?? 0);
            _gc += gcBytes ?? 0;
            _n++;
            if (_n < Sample)
            {
                return null;
            }

            var sample = new ThreadCostSample
            {
                Entry = entry,
                Width = width,
                Height = height,
                Frames = Sample,
                MainMean = ThreadCostMath.Mean(_main),
                MainP50 = ThreadCostMath.P50(_main),
                MainP95 = ThreadCostMath.P95(_main),
                MainMax = ThreadCostMath.Max(_main),
                RenderMean = _renderRecorded ? ThreadCostMath.Mean(_render) : (double?)null,
                RenderP50 = _renderRecorded ? ThreadCostMath.P50(_render) : (double?)null,
                RenderP95 = _renderRecorded ? ThreadCostMath.P95(_render) : (double?)null,
                RenderMax = _renderRecorded ? ThreadCostMath.Max(_render) : (double?)null,
                CanvasRebuildMean =
                    _canvasRecorded ? ThreadCostMath.Mean(_canvas) : (double?)null,
                GcAllocBytesPerFrame =
                    _gcRecorded ? ThreadCostMath.PerFrame(_gc, Sample) : (long?)null,
            };

            // **The warm-up is not re-applied here**, and that is the
            // difference between a key change and a full sample: the next 240
            // frames are the same entry at the same extent, already warm.
            _n = 0;
            _gc = 0;
            _renderRecorded = true;
            _canvasRecorded = true;
            _gcRecorded = true;
            return sample;
        }
    }
}
