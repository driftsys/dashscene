// The thread-cost line's arithmetic, kept where a gate can execute it.
//
// `CommitPacer.cs`'s argument, applied again: no CI job compiles a sample, and
// `unity/package-compat` and `unity/ffi-check` both glob `Runtime/**/*.cs` and
// both exclude `Runtime/Engine/**/*.cs`. So arithmetic left beside the
// recorders would be compiled only by an editor nobody runs in CI and executed
// by nothing at all, while the same arithmetic here is compiled against
// netstandard2.1 on every pull request (R-E10) and run by `just unity-ffi`.
//
// **It is `DashsceneFrameCost`'s arithmetic and not a second version of it.**
// The two instruments report one run side by side — the frame-cost line and the
// thread-cost line describe the same frames — so a percentile taken at a
// different index in one of them is a difference a reader would attribute to
// the measurement. `unity/package-gate`'s `thread_cost_instrument` holds this
// file to the rounding mode for that reason.

namespace Driftsys.Dashscene
{
    /// <summary>Mean, p95 and the two unit conversions the line reports.</summary>
    public static class ThreadCostMath
    {
        /// <summary>The arithmetic mean of a full sample.</summary>
        public static double Mean(double[] values)
        {
            var total = 0.0;
            foreach (var value in values)
            {
                total += value;
            }

            return total / values.Length;
        }

        /// The 95th percentile, by `DashsceneFrameCost.At`'s index arithmetic.
        ///
        /// `values[round((len - 1) * 0.95)]` over a sorted copy, rounding **away
        /// from zero**. C#'s `Math.Round` defaults to banker's rounding, which
        /// differs from this at every midpoint: 31 samples give `30 * 0.95 =
        /// 28.5`, and the default picks 28 where `At` picks 29. Two lines of one
        /// run would then report percentiles taken at different frames.
        ///
        /// **A copy, because the caller's buffer is reused.** The accumulator
        /// fills one array per term and keeps filling it after a report, so
        /// sorting in place would reorder frames that have not been reported
        /// yet against the frames that have.
        public static double P95(double[] values)
        {
            var sorted = (double[])values.Clone();
            System.Array.Sort(sorted);
            var index = (int)System.Math.Round(
                (sorted.Length - 1) * 0.95, System.MidpointRounding.AwayFromZero);
            return sorted[index];
        }

        /// Nanoseconds as milliseconds.
        ///
        /// `ProfilerRecorder.LastValue` on a timing counter is nanoseconds, and
        /// the line reports milliseconds so it can be read beside the
        /// frame-cost line without a conversion in the reader's head.
        public static double NsToMs(long nanoseconds)
        {
            return nanoseconds / 1e6;
        }

        /// <summary>A total spread over the frames it accumulated across.</summary>
        ///
        /// Integer division, because the quantity is bytes: a fractional byte
        /// per frame is a precision the counter does not have.
        public static long PerFrame(long total, int frames)
        {
            return total / frames;
        }
    }
}
