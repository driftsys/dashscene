// The per-instance property names, and nothing else.
//
// **One file per binding kind, and `unity/package-gate` is why.** A
// BatchRendererGroup binds a per-instance property by name through
// `Shader.PropertyToID`, and a name that exists on one side and not the other
// is neither a compile error nor a run-time error: the shader reads the
// property's default and draws a plausible wrong picture. The gate holds every
// shader's `Properties` block to THIS file's set — so a per-material name
// living here would be demanded of every shader as an INSTANCED property, and
// a per-instance name living elsewhere would be demanded of none.

namespace Driftsys.Dashscene
{
    /// The per-instance property names.
    ///
    /// **Every name here appears in every shader's `Properties` block**, and
    /// `unity/package-gate` asserts the two sets are equal in both directions. A
    /// BatchRendererGroup binds a property by name through
    /// `Shader.PropertyToID`, so a name that exists on one side and not the
    /// other is not a compile error and not a run-time error: the shader reads
    /// the property's default and draws a plausible wrong picture.
    public static class PaintProperties
    {
        /// The node's box in document space: `(x, y, w, h)`.
        public const string Quad = "_DsQuad";

        /// Corner radii `(top_left, top_right, bottom_right, bottom_left)`.
        public const string Corners = "_DsCorners";

        /// `(opacity, outset, rotation, unused)`.
        public const string Shade = "_DsShade";

        /// `(pivot.x, pivot.y, unused, unused)`, in document space.
        public const string Pivot = "_DsPivot";

        /// `(kind, row, clip offset, clip count)`.
        public const string Paint = "_DsPaint";

        /// Every per-instance property, in the order the painter lays them out.
        public static readonly string[] All = { Quad, Corners, Shade, Pivot, Paint };
    }
}
