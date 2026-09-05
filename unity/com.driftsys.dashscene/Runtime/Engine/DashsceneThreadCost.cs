// Per-thread frame time, from Unity's own recorders.
//
// **This is the term the two existing instruments exclude by construction**,
// and D3 of `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
// is the ruling that it is needed. The two that exist:
//
// - `demo/src/shell.rs`'s `Timing` brackets `tick` and `present` over a fixed
//   sample of presents, and its `present` is the whole of the drawing — paint
//   plus whatever putting the frame on the window costs.
// - `DashsceneFrameCost.cs` brackets the lease acquire, `BrgPainter.Draw`, the
//   mark and the release, and excludes everything Unity runs after `Update`
//   returns: the culling callback, the render thread's encode, the pipeline's
//   own passes and the swapchain present. Its `draw` is a strict subset of
//   `shell.rs`'s `present`.
//
// So neither of them sees the culling callback, the render thread, or a Canvas
// rebuild. This one does, because it does not bracket anything: it reads the
// counters Unity keeps for its own profiler, which are closed over the whole
// frame including the parts no code in this project executes.
//
// **Both terms include the engine floor**, and that is not a defect to be
// corrected here. The floor is measured rather than modelled: the empty entry
// — the camera, the clear and nothing else — reports its own line in the same
// run, and the renderer's share is the difference. Subtracting a guess would
// produce a number attributable to no reading.
//
// **Pushed from the host loop on drawn frames only, in the same phase as the
// frame-cost line**, so the two lines of a run describe the same frames. A
// recorder's `LastValue` moves when a Unity frame ends, so one push per
// `Update` is one reading per frame; pushing twice in a frame would report one
// frame's value as two.
//
// **A counter this player does not carry is named, never reported as zero.**
// A `ProfilerRecorder` over an unregistered counter is not an error: it stays
// invalid and reports `LastValue` 0 for ever, and a zero Canvas-rebuild term
// reads as a Canvas that rebuilds nothing — which is the finding this whole
// instrument exists to be able to make. So each of the three optional terms
// travels as null and reaches the line as an em dash, and `Unrecorded` names
// them for the host's log.
//
// **Only `Main Thread` is required to arm.** A host that cannot record it has
// no instrument at all, so there is nothing to report. The other four are terms
// a player can legitimately lack, and refusing on them would make the
// instrument unusable in exactly the players that can confirm the rest:
// measured on 6000.3.23f1, macOS/Metal, 2026-09-05, a `-batchmode` player
// carries no `Render Thread`, and one that draws no Canvas carries neither
// Canvas marker — `unity/render-gate` is both.
//
// **Disarmed rather than absent.** `-no-thread-cost` turns it off where a
// command line exists, matching `DashsceneFrameCost`'s `-no-frame-cost`.

using Unity.Profiling;

namespace Driftsys.Dashscene
{
    /// <summary>Reads Unity's thread, Canvas-rebuild and allocation counters.</summary>
    public sealed class DashsceneThreadCost : System.IDisposable
    {
        /// The command-line argument that turns the instrument off.
        public const string OffArgument = "-no-thread-cost";

        /// <summary>True when this instrument is collecting.</summary>
        public bool Armed { get; }

        /// Why it is not collecting, for the log. Null while it is.
        public string Reason { get; }

        /// The counters this player cannot record, comma-separated, or the
        /// empty string when it carries all five.
        ///
        /// **Stated even while armed**, because that is when it matters: the
        /// terms behind these counters report an em dash on every line, and a
        /// host that logs this at launch tells its reader once rather than
        /// leaving them to infer it from a column of dashes.
        public string Unrecorded { get; } = string.Empty;

        private ProfilerRecorder _main;
        private ProfilerRecorder _render;
        private ProfilerRecorder _canvasSend;
        private ProfilerRecorder _canvasBatch;
        private ProfilerRecorder _gcAlloc;

        private readonly ThreadCostAccumulator _accumulator = new ThreadCostAccumulator();

        /// <summary>Starts the five recorders, unless `args` turns them off.</summary>
        ///
        /// `args` is the host's command line. It is passed in rather than read
        /// from `System.Environment` so the render gate can construct an armed
        /// instrument in a player whose own command line says nothing.
        public DashsceneThreadCost(string[] args)
        {
            if (System.Array.IndexOf(args, OffArgument) >= 0)
            {
                Reason = OffArgument;
                return;
            }

            _main = ProfilerRecorder.StartNew(ProfilerCategory.Internal, "Main Thread", 1);
            _render = ProfilerRecorder.StartNew(ProfilerCategory.Internal, "Render Thread", 1);
            _canvasSend = ProfilerRecorder.StartNew(
                ProfilerCategory.Gui, "Canvas.SendWillRenderCanvases", 1);
            _canvasBatch = ProfilerRecorder.StartNew(ProfilerCategory.Gui, "Canvas.BuildBatch", 1);
            _gcAlloc = ProfilerRecorder.StartNew(
                ProfilerCategory.Memory, "GC Allocated In Frame", 1);

            Unrecorded = Missing();
            if (!_main.Valid)
            {
                Reason = "the Main Thread counter is not recordable on this player, so "
                    + "there is no term to report at all. Unrecordable here: " + Unrecorded;
                Dispose();
                return;
            }

            Armed = true;
        }

        /// <summary>Takes one drawn frame, and returns a sample when one is full.</summary>
        ///
        /// Null while the instrument is disarmed, so a host pushes
        /// unconditionally and logs what comes back.
        public ThreadCostSample Push(string entry, int width, int height)
        {
            if (!Armed)
            {
                return null;
            }

            return _accumulator.Push(
                entry,
                width,
                height,
                _main.LastValue,
                Reading(_render),
                // The rebuild is two markers and one term: Unity splits the
                // callback that asks canvases to rebuild from the batch build
                // that follows it, and a reader comparing renderers wants what
                // the rebuild cost, not which half of it. **Both or neither**:
                // one marker's value alone is a part of the rebuild reported as
                // the whole.
                _canvasSend.Valid && _canvasBatch.Valid
                    ? _canvasSend.LastValue + _canvasBatch.LastValue
                    : (long?)null,
                Reading(_gcAlloc));
        }

        /// One counter's last value, or null where this player does not carry it.
        ///
        /// **`Valid` is asked per read and not once at construction**, so a
        /// counter cannot be checked in one place and reported from another.
        /// The invalid case is the whole hazard: `LastValue` answers 0 rather
        /// than failing.
        private static long? Reading(ProfilerRecorder recorder)
        {
            return recorder.Valid ? recorder.LastValue : (long?)null;
        }

        /// Which counters did not start, named for `Unrecorded` and `Reason`.
        ///
        /// **Named rather than counted.** `Canvas.SendWillRenderCanvases` and
        /// `Canvas.BuildBatch` are two a player that draws no Canvas is
        /// missing, and "three counters are invalid" sends a reader to the
        /// wrong repair — the answer to a missing allocation counter is a
        /// player built with `BuildOptions.Development`, and the answer to a
        /// missing Canvas marker is a player that draws a Canvas.
        private string Missing()
        {
            var missing = new System.Collections.Generic.List<string>();
            if (!_main.Valid)
            {
                missing.Add("Main Thread");
            }

            if (!_render.Valid)
            {
                missing.Add("Render Thread");
            }

            if (!_canvasSend.Valid)
            {
                missing.Add("Canvas.SendWillRenderCanvases");
            }

            if (!_canvasBatch.Valid)
            {
                missing.Add("Canvas.BuildBatch");
            }

            if (!_gcAlloc.Valid)
            {
                missing.Add("GC Allocated In Frame");
            }

            return string.Join(", ", missing);
        }

        /// <summary>Releases every recorder that started.</summary>
        ///
        /// **Guarded on `Valid`**, because this runs on the disarming path as
        /// well as the ordinary one: an instrument turned off by
        /// `-no-thread-cost` never started any of them.
        public void Dispose()
        {
            Release(ref _main);
            Release(ref _render);
            Release(ref _canvasSend);
            Release(ref _canvasBatch);
            Release(ref _gcAlloc);
        }

        private static void Release(ref ProfilerRecorder recorder)
        {
            if (recorder.Valid)
            {
                recorder.Dispose();
            }
        }
    }
}
