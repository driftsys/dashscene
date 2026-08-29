// The showcase's frame-cost instrument.
//
// **Stated against `demo/src/shell.rs`, which is the deliverable and not a
// courtesy.** Issue #1329's third limb asks for "a per-frame figure whose
// definition is stated against the instrument in `demo/src/shell.rs`", and
// issue #1347 says why in terms: a Unity figure taken from `Time.deltaTime` or
// from the profiler measures the engine's frame rather than the painter's work,
// so a comparison built on one is between two harnesses and not between two
// painters.
//
// **What the two instruments share, exactly.**
//
// - `tick` is the same quantity in both. `shell.rs` brackets
//   `live.tick(dt, &mut arena)`; this brackets `DashsceneRuntime.Tick(dt)`,
//   which is `ds_runtime_tick` across the same C ABI onto the same solver. It
//   is the one term where the two numbers may be subtracted from each other.
// - The sample size is the same 240 presents, and `unity/package-gate`'s
//   `frame_cost_instrument` re-derives that from `demo/src/shell.rs` rather
//   than trusting this file.
// - A sample is discarded when what it is a sample OF changes part-way
//   through, for `shell.rs`'s reason: a mean taken across that boundary
//   describes neither side of it.
// - The reported terms are mean, p50, p95 and max over the sample, in
//   milliseconds, plus the rate the measured work alone would allow.
//
// **What they do not share, which is the whole of the trap.** `shell.rs`'s
// `present` is "the whole of the drawing: `paint` plus whatever putting the
// frame on the window costs". Nothing in this package can measure that, because
// Unity owns it:
//
// - INCLUDED in `draw` — acquiring the frame lease, `BrgPainter.Draw` (the
//   instance packing and the buffer upload), marking the frame drawn, and
//   releasing the lease. That is every part of the frame this project executes.
// - EXCLUDED from `draw` — the GPU's execution of the batches, the render
//   pipeline's own passes, culling, and the swapchain present. Unity runs all
//   of them after `Update` returns, in a loop this project neither calls nor
//   can bracket. `BatchRendererGroup`'s culling callback is Unity's too.
// - EXCLUDED from both — everything else in Unity's frame: scripts, physics,
//   the render pipeline's setup, and whatever the editor or the player adds.
//
// So `draw` is a strict SUBSET of `shell.rs`'s `present`, and a number from
// here is a floor on the Unity painter's per-frame cost rather than the whole
// of it. It is deliberately not named `present`: that word already spans paint
// as well on the desktop host, which is why `demo-android` renamed its own to
// `submit` — one word must not name two quantities, and this is a third.
//
// **Armed by default, unlike `shell.rs`'s.** That instrument reads
// `DASHSCENE_FRAME_TIMING` so an ordinary run pays nothing. An Android player
// has no environment to read: `am start` sets none without root, and there is
// no equivalent of the `--args` a desktop player takes. The cost here is two
// `Stopwatch.GetTimestamp()` reads per frame against a frame that packs and
// uploads a whole instance table, so the deviation buys a figure obtainable on
// any device and costs nothing measurable. `-no-frame-cost` on the command line
// turns it off where a command line exists.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;

namespace Driftsys.Dashscene.Samples
{
    /// <summary>One reported sample of the showcase's frame cost.</summary>
    ///
    /// **Returned rather than printed**, for the reason
    /// `demo-android/src/timing.rs` gives about its own: the arithmetic is then
    /// separable from where the line goes, which on Android is logcat.
    public sealed class FrameCostSample
    {
        /// What was drawn — a scene name or a document label.
        public string Entry;

        /// The extent it was drawn at, which issue #1236 makes part of the row
        /// rather than a property of the run. Orientation changes the workload
        /// and not only the pixel count, and a Unity player rotates for exactly
        /// the reason issue #1346 exercises.
        public int Width;

        /// <summary>The extent's height. See <see cref="Width"/>.</summary>
        public int Height;

        /// How many drawn frames this sample covers.
        public int Frames;

        /// The mean of `DashsceneRuntime.Tick` over the sample, in
        /// milliseconds. The one term comparable with a Rust host's directly.
        public double TickMean;

        /// The mean of the drawing this project executes, in milliseconds. See
        /// this file's header for what it excludes.
        public double DrawMean;

        /// <summary>The sample's median draw cost, in milliseconds.</summary>
        public double DrawP50;

        /// <summary>The sample's 95th percentile draw cost, in milliseconds.</summary>
        public double DrawP95;

        /// <summary>The sample's slowest draw, in milliseconds.</summary>
        public double DrawMax;

        /// What the frame rate would be if nothing paced it.
        ///
        /// **Not the frame rate.** Unity paces the loop, so the observed rate
        /// is the display's until the work exceeds the budget. Both measured
        /// terms are in it, and neither of the excluded ones is — so it is an
        /// upper bound on this player's rate and not a prediction of it.
        public double FpsIfUnpaced;

        /// <summary>The line, in the shape the record quotes.</summary>
        public string Line()
        {
            return string.Format(
                CultureInfo.InvariantCulture,
                "{0} at {1}x{2} over {3} frames — tick {4:F2} ms, "
                + "draw mean {5:F2} p50 {6:F2} p95 {7:F2} max {8:F2} ms "
                + "({9:F1} fps if unpaced)",
                Entry, Width, Height, Frames,
                TickMean, DrawMean, DrawP50, DrawP95, DrawMax, FpsIfUnpaced);
        }
    }

    /// <summary>Collects tick and draw costs until a full sample is in hand.</summary>
    public sealed class DashsceneFrameCost
    {
        /// How many drawn frames one report covers.
        ///
        /// **The same 240 as `demo/src/shell.rs` and
        /// `demo-android/src/timing.rs`**, which is the sample size
        /// `docs/technotes/frame-budget.md` states for its own measurement, so
        /// all four are read in the same units.
        /// `unity/package-gate`'s `frame_cost_instrument` re-derives this from
        /// `demo/src/shell.rs` rather than trusting the comment.
        public const int TimingSample = 240;

        /// The command-line argument that turns the instrument off.
        public const string OffArgument = "-no-frame-cost";

        private readonly List<double> _tick = new List<double>(TimingSample);
        private readonly List<double> _draw = new List<double>(TimingSample);

        /// What the sample in hand is a sample *of*.
        ///
        /// The entry and the extent together, because either changing makes the
        /// samples either side of it describe different work — the scene
        /// changes on a key and the extent changes on a rotation.
        private string _of;

        /// <summary>True when this instrument is collecting.</summary>
        public bool Armed { get; private set; }

        public DashsceneFrameCost()
        {
            Armed = Array.IndexOf(Environment.GetCommandLineArgs(), OffArgument) < 0;
        }

        /// <summary>Times one frame, and returns a sample when one is full.</summary>
        ///
        /// `tickTicks` and `drawTicks` are `Stopwatch` ticks, converted here so
        /// the caller does not carry the frequency.
        public FrameCostSample Push(
            string entry, int width, int height, long tickTicks, long drawTicks)
        {
            if (!Armed)
            {
                return null;
            }

            var now = entry + "@" + width + "x" + height;
            if (_of != now)
            {
                _tick.Clear();
                _draw.Clear();
                _of = now;
            }

            _tick.Add(Milliseconds(tickTicks));
            _draw.Add(Milliseconds(drawTicks));
            if (_draw.Count < TimingSample)
            {
                return null;
            }

            var sample = Report(entry, width, height);
            _tick.Clear();
            _draw.Clear();
            return sample;
        }

        /// <summary>Stopwatch ticks as milliseconds.</summary>
        ///
        /// `Stopwatch.Frequency` rather than `TimeSpan.TicksPerSecond`: the two
        /// differ on platforms where the high-resolution timer is not 100 ns,
        /// and a figure scaled by the wrong one is wrong by a constant factor
        /// that reads as a plausible measurement.
        private static double Milliseconds(long ticks)
        {
            return ticks * 1000.0 / Stopwatch.Frequency;
        }

        private FrameCostSample Report(string entry, int width, int height)
        {
            var draw = new List<double>(_draw);
            draw.Sort();
            var tickMean = Mean(_tick);
            var drawMean = Mean(draw);
            return new FrameCostSample
            {
                Entry = entry,
                Width = width,
                Height = height,
                Frames = draw.Count,
                TickMean = tickMean,
                DrawMean = drawMean,
                DrawP50 = At(draw, 0.5),
                DrawP95 = At(draw, 0.95),
                DrawMax = At(draw, 1.0),
                // Both measured terms, as `shell.rs` divides by tick plus
                // present rather than by the drawing alone.
                FpsIfUnpaced = tickMean + drawMean <= 0.0
                    ? 0.0
                    : 1000.0 / (tickMean + drawMean),
            };
        }

        private static double Mean(List<double> values)
        {
            if (values.Count == 0)
            {
                return 0.0;
            }

            var total = 0.0;
            foreach (var value in values)
            {
                total += value;
            }

            return total / values.Count;
        }

        /// The value at a percentile of an already-sorted list.
        ///
        /// **`shell.rs`'s index arithmetic, not a re-derivation of it**:
        /// `values[round((len - 1) * p)]`. A nearest-rank or an interpolating
        /// percentile would put the two hosts' p95 columns a frame apart on the
        /// same data, which is the kind of difference that is read as a finding.
        private static double At(List<double> sorted, double percentile)
        {
            if (sorted.Count == 0)
            {
                return 0.0;
            }

            var index = (int)Math.Round(
                (sorted.Count - 1) * percentile, MidpointRounding.AwayFromZero);
            return sorted[index];
        }
    }
}
