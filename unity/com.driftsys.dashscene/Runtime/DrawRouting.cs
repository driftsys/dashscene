// Which draw path each instance takes, and the runs the ordered path draws
// (story #1448).
//
// **Unity-free on purpose, like `InstanceSpans` beside it.** `unity/ffi-check`
// compiles `Runtime/` without any Unity reference assembly, so every line here
// is executed by a gate. These decisions say what is drawn and in which order,
// which is the picture; `BrgPainter` composes them and keeps only the engine
// calls — the material a run's key names, the buffers and the draw commands.
// The ordered buffer's capacity arithmetic (`OrderedCapacity`) and the row
// interleave (`OrderedRowWords`, `InterleaveRows`) live here for the same
// reason.

using System;
using System.Collections.Generic;

namespace Driftsys.Dashscene
{
    /// Which of the two draw paths an instance takes (story #1448).
    ///
    /// [`BrgPainter.PathFor`] answers it for a material class and
    /// [`BrgPainter.TextPath`] for a glyph.
    public enum DrawPath
    {
        /// BatchRendererGroup: one draw command per instance, ordered by the
        /// sorting keys `OnPerformCulling` writes. The two lit classes, which
        /// write and test depth.
        Brg,

        /// `OrderedDrawPass`: one `DrawProcedural` per contiguous run of
        /// instances that share a material, in instance order, with no key.
        /// The overlay class and text, which write no depth and test none, so
        /// the order of the draws is the picture.
        Ordered,
    }

    /// One contiguous run of instances that draw with one material on the
    /// ordered path, the material named by its integer key.
    ///
    /// `Key` is [`DrawRouting.ClassMaterial`] for the painter's class material
    /// and an atlas index for that atlas's text material. `Offset` is the
    /// run's first instance and `Count` the number of instances.
    internal readonly struct KeyedRun
    {
        internal KeyedRun(int key, int offset, int count)
        {
            Key = key;
            Offset = offset;
            Count = count;
        }

        internal int Key { get; }

        internal int Offset { get; }

        internal int Count { get; }
    }

    /// The routing and run-cutting decisions of the two draw paths, over the
    /// packer's arrays.
    ///
    /// `paint` is `FramePacker.Paint`, four words per instance with the kind
    /// tag first, and `atlas` is `FramePacker.InstanceAtlas`, one index per
    /// instance.
    internal static class DrawRouting
    {
        /// The path a glyph takes in every painter, whatever its class.
        ///
        /// **Text is not a `MaterialClass` and is always blended**: the text
        /// shader writes no depth and tests none, so only the order of its
        /// draws decides what covers what. In a lit painter that order puts a
        /// run after every lit node, which is where the transparent queue put
        /// it before story #1448 as well.
        internal const DrawPath TextPath = DrawPath.Ordered;

        /// The key of the painter's class material.
        ///
        /// **-1, the atlas index a node carries.** Every key at or above zero
        /// names an atlas's text material.
        internal const int ClassMaterial = -1;

        /// The key of an instance that the ordered path does not draw.
        internal const int NotOrdered = -2;

        /// The path a painter of class `c` draws its node instances on.
        ///
        /// **One arm per class, and an unknown class throws** rather than being
        /// routed by a default nobody chose. The overlay class writes no depth
        /// and tests none, so draw order is its picture and it takes the
        /// ordered path; the two lit classes write and test depth and stay on
        /// BatchRendererGroup.
        internal static DrawPath PathFor(MaterialClass c)
        {
            switch (c)
            {
                case MaterialClass.LitOpaque:
                    return DrawPath.Brg;
                case MaterialClass.LitCutout:
                    return DrawPath.Brg;
                case MaterialClass.UnlitOverlay:
                    return DrawPath.Ordered;
                default:
                    throw new ArgumentOutOfRangeException(
                        nameof(c), c, "a material class with no draw path");
            }
        }

        /// Whether instance `at` is a glyph.
        ///
        /// **By its Paint kind tag, not by its atlas.** `InstanceAtlas` is -1
        /// for a node and could be -1 for a glyph too, so it cannot say which
        /// of the two an instance is; the kind tag is what the shading branches
        /// on, so it is the one reading that decides how the instance is drawn.
        internal static bool IsGlyph(uint[] paint, int at)
        {
            return paint[at * 4] == (uint)PaintKindTag.Text;
        }

        /// The path instance `at` takes in a painter of class `c`: a glyph's
        /// is [`TextPath`], every other instance's is its painter's class's.
        internal static DrawPath PathOf(uint[] paint, int at, MaterialClass c)
        {
            return IsGlyph(paint, at) ? TextPath : PathFor(c);
        }

        /// How many of the instances `[first, limit)` take the
        /// BatchRendererGroup path in a painter of class `c`.
        ///
        /// **The culling callback's own predicate**, [`PathOf`], so a count
        /// taken here is exactly what that callback's emission loop emits.
        internal static int BrgPathInstances(uint[] paint, int first, int limit, MaterialClass c)
        {
            var count = 0;
            for (var at = first; at < limit; at++)
            {
                if (PathOf(paint, at, c) == DrawPath.Brg)
                {
                    count++;
                }
            }
            return count;
        }

        /// The key of the material instance `at` draws with on the ordered
        /// path, or [`NotOrdered`] if it is not drawn there.
        ///
        /// A glyph whose atlas is one of the `textMaterials` this painter holds
        /// draws with that atlas's material; every other ordered instance with
        /// the class material. **Bounded rather than trusted**: the packer
        /// only writes an atlas it resolved against the set the painter was
        /// given, so an index past the material array means the two went out
        /// of step — and following it would read a material out of a managed
        /// array's end. What a glyph drawn with the class material shows is
        /// not predictable: the node shading reads its atlas texels as corner
        /// radii. The bound makes it a wrong picture rather than an
        /// out-of-range read.
        ///
        /// **One more [`NotOrdered`]**: a glyph of a lit painter whose atlas
        /// fell out of step would map to the lit class material, which has no
        /// procedural variant and would be drawn without a mesh through the
        /// mesh variant. It is drawn on neither path — unreachable while the
        /// packer resolves atlases against the set the painter holds, and a
        /// missing glyph rather than a garbled quad if that ever stops holding.
        internal static int OrderedKeyOf(
            uint[] paint, int[] atlas, int at, int textMaterials, MaterialClass c)
        {
            if (PathOf(paint, at, c) != DrawPath.Ordered)
            {
                return NotOrdered;
            }

            // Only a glyph reads its atlas; a node takes the class material.
            if (!IsGlyph(paint, at))
            {
                return ClassMaterial;
            }

            var index = atlas[at];
            if (index >= 0 && index < textMaterials)
            {
                return index;
            }
            return PathFor(c) == DrawPath.Ordered ? ClassMaterial : NotOrdered;
        }

        /// Words one instance occupies in the `_DsInstances` buffer: the five
        /// streams interleaved, Quad, Corners, Shade, Pivot and Paint, four
        /// words each — the 80-byte `DsInstance` the shading reads.
        internal const int OrderedRowWords = 20;

        /// The capacity the ordered buffer must be reallocated to so that it
        /// holds `wanted` rows, or `0` if it already does.
        ///
        /// `current` is the buffer's capacity in rows, `0` if there is none. A
        /// missing buffer is always allocated. Capacity doubles from at least
        /// 64 and never shrinks, as the batch buffer's does.
        internal static int OrderedCapacity(int current, int wanted)
        {
            if (current > 0 && current >= wanted)
            {
                return 0;
            }

            var capacity = Math.Max(current, 64);
            while (capacity < wanted)
            {
                capacity *= 2;
            }
            return capacity;
        }

        /// Write rows `[first, first + count)` of the packer's five streams
        /// into `staging`, one interleaved row of [`OrderedRowWords`] words per
        /// instance.
        ///
        /// **In `DsInstance`'s field order**: Quad, Corners, Shade, Pivot,
        /// Paint. A float is written as its bits, not converted.
        internal static void InterleaveRows(
            float[] quad,
            float[] corners,
            float[] shade,
            float[] pivot,
            uint[] paint,
            uint[] staging,
            int first,
            int count)
        {
            for (var i = first; i < first + count; i++)
            {
                var row = i * OrderedRowWords;
                var at = i * 4;
                for (var k = 0; k < 4; k++)
                {
                    staging[row + k] = Bits(quad[at + k]);
                    staging[row + 4 + k] = Bits(corners[at + k]);
                    staging[row + 8 + k] = Bits(shade[at + k]);
                    staging[row + 12 + k] = Bits(pivot[at + k]);
                    staging[row + 16 + k] = paint[at + k];
                }
            }
        }

        private static uint Bits(float value)
        {
            return unchecked((uint)BitConverter.SingleToInt32Bits(value));
        }

        /// Cut the first `count` instances into the runs the ordered path
        /// draws, in instance order, replacing what `into` held.
        ///
        /// **A run is contiguous in instance index**, and it is cut where the
        /// material key changes from one instance to the next — comparing
        /// instance `i` with `i - 1`, on [`OrderedKeyOf`]'s mapping. An
        /// instance not drawn on this path has the key [`NotOrdered`], so a
        /// run never spans a lit node and a lit painter's glyph runs are cut
        /// around every node between them. **The resolved material, not the
        /// atlas**: a node and an unresolved glyph of an overlay painter both
        /// resolve to the class material, which is one draw, not two.
        ///
        /// **Never sorted.** The order of the runs is the order of the draws,
        /// and the order of the draws is the picture.
        internal static void CutRuns(
            uint[] paint,
            int[] atlas,
            int count,
            int textMaterials,
            MaterialClass c,
            List<KeyedRun> into)
        {
            into.Clear();
            var start = 0;
            for (var i = 1; i <= count; i++)
            {
                if (i < count
                    && OrderedKeyOf(paint, atlas, i, textMaterials, c)
                        == OrderedKeyOf(paint, atlas, i - 1, textMaterials, c))
                {
                    continue;
                }

                var key = OrderedKeyOf(paint, atlas, i - 1, textMaterials, c);
                if (key != NotOrdered)
                {
                    into.Add(new KeyedRun(key, start, i - start));
                }
                start = i;
            }
        }
    }
}
