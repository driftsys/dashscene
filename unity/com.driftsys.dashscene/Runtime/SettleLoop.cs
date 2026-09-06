// The one decision a host loop makes per frame: draw, or skip everything after
// the tick.
//
// `CommitPacer.cs`'s argument, applied a third time: no CI job compiles a
// sample, and `unity/package-compat` and `unity/ffi-check` both glob
// `Runtime/**/*.cs` and both exclude `Runtime/Engine/**/*.cs`. A decision left
// in `Samples~/` would be written three times — the frame loop, the showcase
// and the Canvas baseline each make it — and executed by nothing. Here it is
// one decision, compiled against netstandard2.1 on every pull request (R-E10)
// and run by `just unity-ffi`.
//
// **The reasons a redraw is forced are the ones the runtime cannot report.**
// `DashsceneRuntime.Tick` already answers for the document: it returns true
// while the committed generation has not been marked shown, which covers a
// fresh load, a replaced document and the atlas set that changes with one —
// `LiveScene::advanced` is true before the first `mark_shown`, so the first
// tick after a load advances. What is left is host-side: the drawable's extent,
// and a surface or device the host rebuilt under a document that did not
// change.
//
// **Of those two, only the extent is wired.** No sample calls `ForceRedraw`,
// because none of them has an event to call it from: a Unity host hands
// dashscene no surface — `Native.cs` names `ds_runtime_attach_surface` among
// the four entry points it does not call — so the C ABI's "first frame after
// re-attaching must be drawn whatever the tick says" is not this host's
// obligation, and Unity raises no managed callback for a graphics device it
// rebuilt underneath one. Issue #1467 carries what that leaves exposed. The
// method is here because the decision belongs in one class whether or not this
// package's samples have a trigger for it.

namespace Driftsys.Dashscene
{
    /// Draw, or skip the acquire, the pack, the upload and the bind.
    ///
    /// **A skip needs both halves.** The tick reported no advance AND no forced
    /// redraw is pending; either one alone draws. A forced redraw is consumed
    /// by the draw it forces, so one rebuilt surface costs one frame rather
    /// than every frame after it.
    ///
    /// **A class, not a struct**, for [`CommitPacer`]'s reason: `ShouldDraw`
    /// and `NoteExtent` mutate through instance methods, so a value type held
    /// in a `readonly` field would be defensively copied at every call and the
    /// pending bit would be discarded as it was set.
    ///
    /// No `UnityEngine` types, so `unity/ffi-check` executes it and
    /// `unity/package-compat` compiles it against netstandard2.1.
    public sealed class SettleLoop
    {
        /// A forced redraw waiting to be consumed.
        ///
        /// **Raised at construction**, so a host draws its first frame whatever
        /// the tick and the extent say. Leaving it to the first
        /// [`NoteExtent`] would rest that guarantee on the sentinel below never
        /// equalling an extent a host reports, which nothing in Unity's API
        /// promises.
        private bool _pending = true;

        /// The drawable extent the last [`NoteExtent`] reported.
        ///
        /// **Negative until the first call**, so the first frame of any host
        /// forces one draw whatever the tick says. A host that started settled
        /// would otherwise present whatever the surface was cleared to.
        private int _width = -1;

        /// The drawable height. See [`_width`].
        private int _height = -1;

        /// Whether a forced redraw is waiting.
        public bool Pending
        {
            get { return _pending; }
        }

        /// How many frames this loop has skipped.
        public int FramesSkipped { get; private set; }

        /// How many frames this loop has drawn.
        public int FramesDrawn { get; private set; }

        /// Force one redraw: a rebuilt surface, a recreated device, an
        /// embedder's request.
        ///
        /// A reason the runtime cannot report, because none of them changes the
        /// document. Consumed by the next [`ShouldDraw`] that returns true.
        public void ForceRedraw()
        {
            _pending = true;
        }

        /// Report the drawable's extent, forcing a redraw when it changed.
        ///
        /// Called every frame rather than from a resize event: Unity reports
        /// `Screen.width` and `Screen.height` and raises nothing, and a host
        /// polling two integers costs less than the frame it would otherwise
        /// draw at the wrong size.
        public void NoteExtent(int width, int height)
        {
            if (width == _width && height == _height)
            {
                return;
            }

            _width = width;
            _height = height;
            _pending = true;
        }

        /// Whether this frame draws, given what the tick reported.
        ///
        /// **The counters move here and nowhere else**, so `FramesDrawn` over
        /// `FramesDrawn + FramesSkipped` is the ratio the render gate judges
        /// and no caller can report a frame it did not decide.
        ///
        /// **The pending bit is consumed by the decision, not by the drawing.**
        /// A host whose frame then throws has spent a redraw it never made, and
        /// on a static document nothing raises the bit again — so a host that
        /// catches and carries on calls [`ForceRedraw`] from that catch. One
        /// that stops has nothing to put back.
        public bool ShouldDraw(bool advanced)
        {
            if (!advanced && !_pending)
            {
                FramesSkipped++;
                return false;
            }

            _pending = false;
            FramesDrawn++;
            return true;
        }
    }
}
