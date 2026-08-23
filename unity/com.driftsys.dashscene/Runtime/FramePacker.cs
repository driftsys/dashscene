// The committed tables, turned into instances and a paint heap.
//
// **This is the half of the painter that decides what the picture is**, and it
// is engine-independent on purpose: no `UnityEngine` type appears here, so
// `unity/package-compat` compiles it against `netstandard.dll` 2.1.0 and a
// check with no editor can execute it against a real document. The
// `BatchRendererGroup` half in `Runtime/Engine/` uploads what this produces and
// decides nothing about it.
//
// **The order instances are emitted in is the lean painter's**, and matching it
// is the point rather than a convenience: `crates/dashscene-gpu/src/pack.rs`
// emits, per rect, the backdrop, then drop shadows, then the fill, the stacked
// fills and the stroke, then inner shadows. This painter emits the middle group
// and reports the rest by name — so the instances it does emit are in the same
// relative order the other painter puts them in.
//
// **Arrays are reused across frames, and that is only half of R-T4.** That rule
// asks for a CPU frame cost of "dirty-range instance-buffer upload from the rect
// table + submission, nothing else". The arrays here grow by doubling and never
// shrink, so a steady frame allocates nothing — but this packer walks EVERY rect
// and rebuilds the whole heap on every commit, and `DsFrame.Dirty`'s ROWS are
// read by nothing — `FrameLease` reads its stride for R-E17, and no consumer
// reads the indices it carries. **The transfer R-T4 actually bounds is unbounded**; issue #1306
// carries it, and issue #708 is the same gap in the lean painter, where the
// design that would serve both belongs.

using System;
using Driftsys.Dashscene.BoundaryB;

namespace Driftsys.Dashscene
{
    /// How many rows each table a `RectEntry` or its `PaintEntry` can name
    /// actually holds.
    ///
    /// **Most of these bound a raw-pointer read, and three bound a row index
    /// the SHADER follows instead.** `ClipBoxes` bounds a range this packer
    /// never dereferences and the fragment stage walks without a bound of its
    /// own; `Solids` and `Gradients` bound a `fill.Index` that reaches the
    /// shader as `_DsPaint.y`. Following either past its table is a wrong
    /// picture rather than a crash, which is why they are checked here where
    /// the count is known. The committed
    /// tables are borrowed native memory, so following a row index past its
    /// table is an out-of-bounds read of the host process rather than an
    /// exception — which is why they travel together rather than being fetched
    /// where each is used, and why a first version that carried two of them and
    /// followed five ranges was a defect rather than an omission.
    internal readonly struct TableBounds
    {
        internal TableBounds(
            int entries,
            int regions,
            int clipBoxes,
            int strokes,
            int blurs,
            int extraFills,
            int solids,
            int gradients)
        {
            Entries = entries;
            Regions = regions;
            ClipBoxes = clipBoxes;
            Strokes = strokes;
            Blurs = blurs;
            ExtraFills = extraFills;
            Solids = solids;
            Gradients = gradients;
        }

        internal int Entries { get; }

        internal int Regions { get; }

        internal int ClipBoxes { get; }

        internal int Strokes { get; }

        internal int Blurs { get; }

        internal int ExtraFills { get; }

        internal int Solids { get; }

        internal int Gradients { get; }
    }

    /// Packs one committed frame into the arrays a painter uploads.
    ///
    /// Not thread-safe, and deliberately not: one painter owns one packer and
    /// calls it on the runtime's own thread, where the frame lease is acquired.
    public sealed class FramePacker
    {
        private const int Float4 = 4;

        private float[] _quad = new float[Float4 * 64];
        private float[] _corners = new float[Float4 * 64];
        private float[] _shade = new float[Float4 * 64];
        private float[] _pivot = new float[Float4 * 64];
        private uint[] _paint = new uint[Float4 * 64];

        private float[] _paints = new float[Float4 * 64];

        /// Whether each solid / gradient row is fully opaque.
        ///
        /// Filled by the heap walk, which already visits every row, so the
        /// opaque material class can refuse a translucent fill without a second
        /// pass over the tables. **Only that class reads them**; the overlay
        /// class expresses any alpha and the cutout class thresholds it.
        private bool[] _solidOpaque = new bool[64];
        private bool[] _gradientOpaque = new bool[64];
        private float[] _clipBoxes = new float[Float4 * 64];
        private float[] _strokes = new float[Float4 * 64];

        private PackDiagnostic _flags;
        private int _affectedRects;
        private int _firstAffectedRect = -1;

        /// The last rect [`Affect`] was called for, so a rect that skips two
        /// things is counted once. Rects are visited in order, so comparing
        /// against the previous one is enough.
        private int _lastAffectedRect = -1;

        /// How many instances the last [`Pack`] produced.
        public int InstanceCount { get; private set; }

        /// `(x, y, w, h)` per instance, four floats each.
        public float[] Quad => _quad;

        /// `(top_left, top_right, bottom_right, bottom_left)` per instance.
        public float[] Corners => _corners;

        /// `(opacity, outset, rotation, 0)` per instance.
        public float[] Shade => _shade;

        /// `(pivot.x, pivot.y, 0, 0)` per instance, in document space.
        public float[] Pivot => _pivot;

        /// `(kind, row, clip offset, clip count)` per instance.
        public uint[] Paint => _paint;

        /// The paint heap: solid colours then gradient rows, as floats.
        public float[] Paints => _paints;

        /// How many floats of [`Paints`] the last pack filled.
        public int PaintFloats { get; private set; }

        /// The `float4` index the solid colours start at. Always zero; named so
        /// the shader's `_DsGlobals.y` has one source rather than a literal.
        public int SolidBase => 0;

        /// The `float4` index the gradient rows start at.
        public int GradientBase { get; private set; }

        /// The flat clip-box array, two `float4`s per box.
        public float[] ClipBoxes => _clipBoxes;

        /// How many floats of [`ClipBoxes`] the last pack filled.
        public int ClipFloats { get; private set; }

        /// The stroke table, two `float4`s per stroke.
        public float[] Strokes => _strokes;

        /// How many floats of [`Strokes`] the last pack filled.
        public int StrokeFloats { get; private set; }

        /// What the last pack was handed and did not draw.
        public PackDiagnostics Diagnostics { get; private set; }

        /// Pack one committed frame.
        ///
        /// The frame's arrays are borrowed from the runtime and are valid only
        /// while its lease is outstanding, which is why this copies rather than
        /// retaining: what it produces outlives the lease, and the lease is
        /// what the caller releases once its workers are done.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when a table is longer than
        /// `int.MaxValue` rows. Not a condition any document reaches — it is
        /// asserted rather than handled because the alternative is a silent
        /// truncation from a `long` to an `int`.
        public unsafe void Pack(DsFrame frame, MaterialClass materialClass)
        {
            _flags = PackDiagnostic.None;
            _affectedRects = 0;
            _firstAffectedRect = -1;
            _lastAffectedRect = -1;
            InstanceCount = 0;

            PackHeap(frame);
            PackClipBoxes(frame);
            PackStrokes(frame);

            if (Rows(frame.Groups) > 0)
            {
                // Not per rect: a group is a property of the document, and the
                // rect it would be reported against is the range's first, which
                // is not the node a reader would look for.
                _flags |= PackDiagnostic.RenderTargetGroup;
            }

            if (Rows(frame.GlyphRuns) > 0)
            {
                _flags |= PackDiagnostic.GlyphRun;
            }

            var rectCount = Rows(frame.Rects);
            var entryCount = Rows(frame.PaintEntries);
            var regionCount = Rows(frame.ClipRegions);
            // **Every table a row can name is counted, not just the two that
            // used to be.** `PackRect` follows five ranges through raw
            // pointers; a first version bounds-checked two of them and
            // explained in a comment why it must — which made the other three
            // out-of-bounds reads of the host process rather than diagnostics.
            var boxCount = Rows(frame.ClipBoxes);
            var strokeCount = Rows(frame.Strokes);
            var blurCount = Rows(frame.Blurs);
            var extraFillCount = Rows(frame.ExtraFills);
            var solidCount = Rows(frame.Solids);
            var gradientCount = Rows(frame.Gradients);
            var rects = (RectEntry*)frame.Rects.Ptr;
            var entries = (PaintEntry*)frame.PaintEntries.Ptr;
            var regions = (ClipRegion*)frame.ClipRegions.Ptr;
            var extraFills = (PaintKind*)frame.ExtraFills.Ptr;
            var strokes = (Stroke*)frame.Strokes.Ptr;
            var blurs = (Blur*)frame.Blurs.Ptr;

            for (var i = 0; i < rectCount; i++)
            {
                PackRect(
                    i,
                    rects[i],
                    entries,
                    regions,
                    extraFills,
                    strokes,
                    blurs,
                    new TableBounds(
                        entryCount,
                        regionCount,
                        boxCount,
                        strokeCount,
                        blurCount,
                        extraFillCount,
                        solidCount,
                        gradientCount),
                    materialClass);
            }

            Diagnostics = new PackDiagnostics(_flags, _affectedRects, _firstAffectedRect);
        }

        private unsafe void PackRect(
            int index,
            in RectEntry rect,
            PaintEntry* entries,
            ClipRegion* regions,
            PaintKind* extraFills,
            Stroke* strokes,
            Blur* blurs,
            in TableBounds bounds,
            MaterialClass materialClass)
        {
            // A rect naming a row outside its table is a corrupt frame rather
            // than an empty one. Refused by drawing nothing and reporting,
            // never by indexing: the table is a raw pointer, so an out-of-range
            // read is undefined behaviour and not an exception.
            if (rect.Paint >= (uint)bounds.Entries)
            {
                Affect(index, PackDiagnostic.CorruptRow);
                return;
            }

            var entry = entries[rect.Paint];

            // **An out-of-range clip index is a corrupt row, not "unclipped".**
            // `ClipIndex::UNCLIPPED` is 0 and `dashpaint` documents it as a
            // real entry rather than a sentinel, so a value past the table
            // carries no "no clip" meaning to fall back on — and falling back
            // is the more dangerous of the two answers, because ink the
            // document said must be cut then covers the whole node with
            // `Diagnostics.IsClean` reporting the frame as fully drawn.
            if (rect.Clip >= (uint)bounds.Regions)
            {
                Affect(index, PackDiagnostic.CorruptRow);
                return;
            }

            var region = regions[rect.Clip];
            var clipOffset = region.Offset;
            var clipCount = region.Count;

            // The region's own range into the flat box array, which the shader
            // walks without a bound of its own.
            if (clipCount > 0
                && (clipOffset > (uint)bounds.ClipBoxes
                    || clipCount > (uint)bounds.ClipBoxes - clipOffset))
            {
                Affect(index, PackDiagnostic.CorruptRow);
                return;
            }

            // Reported before anything is emitted, so a node that is skipped
            // entirely still says why.
            var skipped = PackDiagnostic.None;
            if (entry.Shadows.Count > 0)
            {
                skipped |= PackDiagnostic.Shadow;
            }

            if (entry.Blurs.Count > 0
                && (entry.Blurs.Offset > (uint)bounds.Blurs
                    || entry.Blurs.Count > (uint)bounds.Blurs - entry.Blurs.Offset))
            {
                // `skipped |`, like every other early return here. A first
                // version dropped it, so a node with a shadow AND a corrupt
                // blur range reported the corruption and lost the shadow —
                // against this block's own rule that a node skipped entirely
                // still says why.
                Affect(index, skipped | PackDiagnostic.CorruptRow);
                return;
            }

            for (var b = 0u; b < entry.Blurs.Count; b++)
            {
                var blur = blurs[entry.Blurs.Offset + b];
                skipped |= blur.Kind == BlurKind.Backdrop
                    ? PackDiagnostic.BackdropBlur
                    : PackDiagnostic.LayerBlur;
            }

            if (entry.Shape.Count > 0)
            {
                // A baked vector carries its outline in the coverage field,
                // so the parametric rounded box shaded here is not its shape.
                // Drawing the box would be a plausible wrong picture.
                Affect(index, skipped | PackDiagnostic.VectorField);
                return;
            }

            // `dashpaint::PaintTable::stroke` asserts the range has arity one
            // and panics above it — "the vocabulary is single-stroke". Taking
            // the first and dropping the rest would be the silent answer to a
            // table that check did not run over.
            if (entry.Stroke.Count > 1)
            {
                Affect(index, skipped | PackDiagnostic.CorruptRow);
                return;
            }

            var strokeRow = entry.Stroke.Count > 0 ? entry.Stroke.Offset : uint.MaxValue;
            var hasStroke = strokeRow != uint.MaxValue;
            if (hasStroke && strokeRow >= (uint)bounds.Strokes)
            {
                Affect(index, skipped | PackDiagnostic.CorruptRow);
                return;
            }

            // The row index also crosses to the shader as `_DsPaint.y` and
            // indexes `_DsStrokes` there, so refusing it here is what keeps a
            // corrupt row out of the fragment stage as well.
            var outset = hasStroke ? StrokeOutset(strokes[strokeRow]) : 0.0f;

            if (materialClass == MaterialClass.LitOpaque
                && NeedsCoverage(entry, clipCount, hasStroke, rect.Opacity))
            {
                Affect(index, skipped | PackDiagnostic.CoverageNotExpressible);
                return;
            }

            if (skipped != PackDiagnostic.None)
            {
                Affect(index, skipped);
            }

            // The node's ink, in the lean painter's order: the fill, then the
            // stacked fills, then the stroke.
            if (entry.Fill.Tag != PaintTag.None)
            {
                EmitFill(index, rect, entry, entry.Fill, clipOffset, clipCount, bounds,
                         materialClass);
            }

            if (entry.ExtraFills.Count > 0
                && (entry.ExtraFills.Offset > (uint)bounds.ExtraFills
                    || entry.ExtraFills.Count > (uint)bounds.ExtraFills - entry.ExtraFills.Offset))
            {
                Affect(index, PackDiagnostic.CorruptRow);
            }
            else
            {
                for (var f = 0u; f < entry.ExtraFills.Count; f++)
                {
                    EmitFill(index, rect, entry, extraFills[entry.ExtraFills.Offset + f],
                             clipOffset, clipCount, bounds, materialClass);
                }
            }

            if (hasStroke)
            {
                Emit(rect, entry, PaintKindTag.Stroke, strokeRow, clipOffset, clipCount, outset);
            }
        }

        /// Whether this node needs coverage or alpha the opaque class cannot
        /// express.
        ///
        /// A corner radius, a clip, a stroke — **or a per-node opacity below
        /// one**. The opaque class does not blend and its fragment stage
        /// returns `1.0` for alpha, so a node authored at 40 % would draw fully
        /// opaque; that is the same silent drop as a square corner, applied to
        /// a different term of the same product, and a first version of this
        /// function considered only the geometric half. The fill's own alpha is
        /// checked separately, where the row index is known.
        private static bool NeedsCoverage(
            in PaintEntry entry,
            uint clipCount,
            bool hasStroke,
            float opacity)
        {
            return hasStroke
                   || opacity < 1.0f
                   || clipCount > 0
                   || entry.Corners.TopLeft > 0.0f
                   || entry.Corners.TopRight > 0.0f
                   || entry.Corners.BottomRight > 0.0f
                   || entry.Corners.BottomLeft > 0.0f;
        }

        private void EmitFill(
            int index,
            in RectEntry rect,
            in PaintEntry entry,
            PaintKind fill,
            uint clipOffset,
            uint clipCount,
            in TableBounds bounds,
            MaterialClass materialClass)
        {
            switch (fill.Tag)
            {
                case PaintTag.Solid:
                    // The row index reaches the shader as `_DsPaint.y` and
                    // indexes the paint heap there, so an out-of-range value
                    // would shade from another table's bytes rather than fail.
                    if (fill.Index >= (uint)bounds.Solids)
                    {
                        Affect(index, PackDiagnostic.CorruptRow);
                        break;
                    }
                    if (materialClass == MaterialClass.LitOpaque && !_solidOpaque[fill.Index])
                    {
                        Affect(index, PackDiagnostic.CoverageNotExpressible);
                        break;
                    }
                    Emit(rect, entry, PaintKindTag.FillSolid, fill.Index,
                         clipOffset, clipCount, 0.0f);
                    break;
                case PaintTag.Gradient:
                    if (fill.Index >= (uint)bounds.Gradients)
                    {
                        Affect(index, PackDiagnostic.CorruptRow);
                        break;
                    }
                    if (materialClass == MaterialClass.LitOpaque && !_gradientOpaque[fill.Index])
                    {
                        Affect(index, PackDiagnostic.CoverageNotExpressible);
                        break;
                    }
                    Emit(rect, entry, PaintKindTag.FillGradient, fill.Index,
                         clipOffset, clipCount, 0.0f);
                    break;
                case PaintTag.Image:
                    Affect(index, PackDiagnostic.ImageFill);
                    break;
                case PaintTag.None:
                    // A stacked layer naming no fill is a corrupt list rather
                    // than an empty one — `dashpaint::PaintTable::check_fills`
                    // refuses it by name upstream, so reaching it here means
                    // the table was not the one that check ran over.
                    Affect(index, PackDiagnostic.CorruptRow);
                    break;
                default:
                    Affect(index, PackDiagnostic.CorruptRow);
                    break;
            }
        }

        private void Emit(
            in RectEntry rect,
            in PaintEntry entry,
            PaintKindTag kind,
            uint row,
            uint clipOffset,
            uint clipCount,
            float outset)
        {
            var at = InstanceCount;
            Grow(at + 1);

            var f = at * Float4;
            _quad[f] = rect.X;
            _quad[f + 1] = rect.Y;
            _quad[f + 2] = rect.W;
            _quad[f + 3] = rect.H;

            _corners[f] = entry.Corners.TopLeft;
            _corners[f + 1] = entry.Corners.TopRight;
            _corners[f + 2] = entry.Corners.BottomRight;
            _corners[f + 3] = entry.Corners.BottomLeft;

            _shade[f] = rect.Opacity;
            _shade[f + 1] = outset;
            _shade[f + 2] = rect.Rotation;
            _shade[f + 3] = 0.0f;

            // Document space, not node-relative: `RectEntry.RotationAnchor` is
            // `(0, 0)` at the node's top-left, and the vertex stage turns a
            // point that is already in document space. The lean painter's
            // packer resolves it the same way and for the same reason.
            _pivot[f] = rect.X + rect.RotationAnchor.X;
            _pivot[f + 1] = rect.Y + rect.RotationAnchor.Y;
            _pivot[f + 2] = 0.0f;
            _pivot[f + 3] = 0.0f;

            _paint[f] = (uint)kind;
            _paint[f + 1] = row;
            _paint[f + 2] = clipOffset;
            _paint[f + 3] = clipCount;

            InstanceCount = at + 1;
        }

        /// The distance a stroke's band reaches past the node's fill box.
        ///
        /// `dashscene-gpu`'s `stroke_outset`, and the same three cases: an
        /// inside stroke sits within the box, a centred one straddles it by
        /// half its width, an outside one by its whole width.
        private static float StrokeOutset(in Stroke stroke)
        {
            switch (stroke.Align)
            {
                case StrokeAlign.Inside:
                    return 0.0f;
                case StrokeAlign.Center:
                    return stroke.Width / 2.0f;
                case StrokeAlign.Outside:
                    return stroke.Width;
                default:
                    // An alignment outside the declared three came across the
                    // ABI as a byte. Zero is the value that draws inside the
                    // node's own box, so an unknown alignment cannot make ink
                    // appear outside a node that never asked for any.
                    return 0.0f;
            }
        }

        private unsafe void PackHeap(DsFrame frame)
        {
            var solids = (Color*)frame.Solids.Ptr;
            var solidCount = Rows(frame.Solids);
            var gradients = (Gradient*)frame.Gradients.Ptr;
            var gradientCount = Rows(frame.Gradients);
            var stops = (GradientStop*)frame.GradientStops.Ptr;
            var stopCount = Rows(frame.GradientStops);

            var words = solidCount * PaintHeap.SolidWords
                        + gradientCount * PaintHeap.GradientWords;
            EnsureFloats(ref _paints, words * Float4);
            GradientBase = solidCount * PaintHeap.SolidWords;

            EnsureFlags(ref _solidOpaque, solidCount);
            EnsureFlags(ref _gradientOpaque, gradientCount);

            var at = 0;
            for (var i = 0; i < solidCount; i++)
            {
                _solidOpaque[i] = solids[i].A >= 1.0f;
                _paints[at++] = solids[i].R;
                _paints[at++] = solids[i].G;
                _paints[at++] = solids[i].B;
                _paints[at++] = solids[i].A;
            }

            for (var i = 0; i < gradientCount; i++)
            {
                var g = gradients[i];
                var count = (int)Math.Min(g.Stops.Count, (uint)PaintHeap.MaxGradientStops);
                if (g.Stops.Count > (uint)PaintHeap.MaxGradientStops)
                {
                    _flags |= PackDiagnostic.GradientStopsTruncated;
                }

                // Word 0: the primary handles, normalised to the node's box —
                // which is what makes one gradient row shareable between nodes
                // of different sizes, and why the shader multiplies by the
                // bounds rather than this packer.
                _paints[at++] = g.HandleOrigin.X;
                _paints[at++] = g.HandleOrigin.Y;
                _paints[at++] = g.HandlePrimary.X;
                _paints[at++] = g.HandlePrimary.Y;

                // Word 1: the secondary handle, the kind and the stop count.
                _paints[at++] = g.HandleSecondary.X;
                _paints[at++] = g.HandleSecondary.Y;
                _paints[at++] = (float)g.Kind;
                _paints[at++] = count;

                // Words 2 and 3: eight offset slots, read unconditionally by
                // the shader because they are two whole words. Slots past the
                // count hold zero, which `gradient_ramp` never reads.
                var offsetsAt = at;
                for (var s = 0; s < 8; s++)
                {
                    _paints[at++] = 0.0f;
                }

                // Words 4..12: eight colour slots.
                var coloursAt = at;
                for (var s = 0; s < 8 * Float4; s++)
                {
                    _paints[at++] = 0.0f;
                }

                // A gradient is opaque only if every stop it carries is. A
                // ramp that fades to transparent is exactly what the opaque
                // class cannot draw.
                _gradientOpaque[i] = true;

                for (var s = 0; s < count; s++)
                {
                    var index = g.Stops.Offset + (uint)s;
                    if (index >= (uint)stopCount)
                    {
                        // A stop range reaching past its table is a corrupt
                        // frame. Stopping leaves the remaining slots zero,
                        // which draws the ramp the stops that DID arrive
                        // describe rather than reading another gradient's.
                        //
                        // **Reported, and the row loses its opaque claim.** A
                        // first version broke silently and left
                        // `_gradientOpaque[i]` true, so the opaque class drew
                        // the row — whose unread colour slots are zero — as a
                        // solid black node, with `Diagnostics.IsClean`
                        // reporting the frame fully drawn. That is the silent
                        // drop P4 forbids, wearing the worst possible colour.
                        _flags |= PackDiagnostic.CorruptRow;
                        _gradientOpaque[i] = false;
                        break;
                    }

                    var stop = stops[index];
                    if (stop.Color.A < 1.0f)
                    {
                        _gradientOpaque[i] = false;
                    }
                    _paints[offsetsAt + s] = stop.Offset;
                    _paints[coloursAt + s * Float4] = stop.Color.R;
                    _paints[coloursAt + s * Float4 + 1] = stop.Color.G;
                    _paints[coloursAt + s * Float4 + 2] = stop.Color.B;
                    _paints[coloursAt + s * Float4 + 3] = stop.Color.A;
                }
            }

            PaintFloats = at;
        }

        private unsafe void PackClipBoxes(DsFrame frame)
        {
            var boxes = (ClipBox*)frame.ClipBoxes.Ptr;
            var count = Rows(frame.ClipBoxes);
            EnsureFloats(ref _clipBoxes, count * PaintHeap.ClipWords * Float4);

            var at = 0;
            for (var i = 0; i < count; i++)
            {
                var box = boxes[i];
                _clipBoxes[at++] = box.X;
                _clipBoxes[at++] = box.Y;
                _clipBoxes[at++] = box.W;
                _clipBoxes[at++] = box.H;
                _clipBoxes[at++] = box.Corners.TopLeft;
                _clipBoxes[at++] = box.Corners.TopRight;
                _clipBoxes[at++] = box.Corners.BottomRight;
                _clipBoxes[at++] = box.Corners.BottomLeft;
            }

            ClipFloats = at;
        }

        private unsafe void PackStrokes(DsFrame frame)
        {
            var strokes = (Stroke*)frame.Strokes.Ptr;
            var count = Rows(frame.Strokes);
            EnsureFloats(ref _strokes, count * PaintHeap.StrokeWords * Float4);

            var at = 0;
            for (var i = 0; i < count; i++)
            {
                var stroke = strokes[i];
                // **Colour first**, matching `paint.wgsl`'s
                // `struct Stroke { color: vec4f, width: f32, align: u32, _pad }`.
                // A first version wrote the two words the other way round —
                // internally consistent with its own shader, and not the row
                // the lean painter reads, which is what this file's "word for
                // word" claim has to mean if issue #828's suite is to be stated
                // over both.
                _strokes[at++] = stroke.Color.R;
                _strokes[at++] = stroke.Color.G;
                _strokes[at++] = stroke.Color.B;
                _strokes[at++] = stroke.Color.A;
                _strokes[at++] = stroke.Width;
                // **The alignment crosses as a float here and as a `u32` in the
                // WGSL row.** That is the one word of the heap that is not
                // byte-identical, and it is forced: this heap is a
                // `StructuredBuffer<float4>`, and `stroke_coverage` takes
                // `align` as a `float` in both languages — `paint.wgsl`
                // converts on read, and this converts on write.
                _strokes[at++] = (float)stroke.Align;
                _strokes[at++] = 0.0f;
                _strokes[at++] = 0.0f;
            }

            StrokeFloats = at;
        }

        /// Record that `rect` contributed a skip.
        ///
        /// **Counts each rect once**, however many of its fills or effects were
        /// skipped: `AffectedRects` says how many nodes drew less than they
        /// asked for, and a node with two image fills is one node. A first
        /// version incremented per call and reported two.
        private void Affect(int rect, PackDiagnostic flags)
        {
            _flags |= flags;
            if (rect != _lastAffectedRect)
            {
                _affectedRects++;
                _lastAffectedRect = rect;
            }
            if (_firstAffectedRect < 0)
            {
                _firstAffectedRect = rect;
            }
        }

        /// Make room for one more instance, **keeping the ones already
        /// written**.
        ///
        /// This is called from inside the emission loop, so a growth that
        /// discarded the buffer would silently zero every instance packed
        /// before it — a document that crossed the capacity would draw its tail
        /// and nothing else. The heap arrays below grow the other way, because
        /// they are sized before anything is written into them.
        private void Grow(int instances)
        {
            var floats = instances * Float4;
            KeepFloats(ref _quad, floats);
            KeepFloats(ref _corners, floats);
            KeepFloats(ref _shade, floats);
            KeepFloats(ref _pivot, floats);
            KeepUints(ref _paint, floats);
        }

        /// Grows by doubling, never shrinks, and copies what was there.
        private static void KeepFloats(ref float[] array, int floats)
        {
            if (array.Length >= floats)
            {
                return;
            }
            Array.Resize(ref array, Capacity(array.Length, floats));
        }

        private static void KeepUints(ref uint[] array, int words)
        {
            if (array.Length >= words)
            {
                return;
            }
            Array.Resize(ref array, Capacity(array.Length, words));
        }

        /// Grows by doubling, never shrinks, and **preserves nothing**.
        ///
        /// Only for the heap arrays, which are sized once and then written from
        /// index zero — so a copy would be work the frame path pays for
        /// nothing. Never for the instance arrays: [`Grow`] above says why.
        private static void EnsureFloats(ref float[] array, int floats)
        {
            if (array.Length >= floats)
            {
                return;
            }
            array = new float[Capacity(array.Length, floats)];
        }

        /// Grows a flag array, preserving nothing — it is rewritten per pack.
        private static void EnsureFlags(ref bool[] array, int wanted)
        {
            if (array.Length >= wanted)
            {
                return;
            }
            array = new bool[Capacity(array.Length, wanted)];
        }

        /// The next power-of-two multiple of `have` that reaches `want`.
        ///
        /// Saturating rather than doubling past `int.MaxValue`, where the
        /// multiply would wrap to a negative and `new float[size]` would throw
        /// something unrelated to the cause. Not reachable from any document
        /// this repository can build — `Rows` already refuses a table longer
        /// than `int.MaxValue` — which is why it is a clamp and not a
        /// diagnostic.
        private static int Capacity(int have, int want)
        {
            var size = have == 0 ? 1 : have;
            while (size < want)
            {
                if (size > int.MaxValue / 2)
                {
                    return want;
                }
                size *= 2;
            }
            return size;
        }

        /// A slice's row count as an `int`.
        ///
        /// Refused rather than truncated. `DsSlice.Count` is pointer-width and
        /// every consumer here indexes with an `int`, so a silent narrowing
        /// would read the wrong rows rather than fail.
        private static int Rows(DsSlice slice)
        {
            var count = slice.CountAsLong;
            if (count > int.MaxValue)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(slice),
                    count,
                    "a committed table longer than int.MaxValue rows cannot be packed here.");
            }
            return (int)count;
        }
    }
}
