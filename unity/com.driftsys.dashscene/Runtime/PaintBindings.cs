// The names that are NOT per-instance: the global buffers, and the one property
// a material carries rather than an instance.
//
// Deliberately apart from `PaintProperties.cs`, which holds the per-instance
// set. `unity/package-gate` reads that file to decide what every shader must
// declare, and this one to decide what a shader is ALLOWED to declare beyond
// it — so a name in the wrong file makes the gate demand the wrong thing of
// every shader in the package.

namespace Driftsys.Dashscene
{
    /// The global buffers and scalars the shaders read, by name.
    public static class PaintGlobals
    {
        /// Solid colours and gradient rows, in one buffer at the two bases
        /// [`Scalars`] carries.
        public const string Paints = "_DsPaints";

        /// The flat clip-box array every clip region names a range of.
        public const string ClipBoxes = "_DsClipBoxes";

        /// The stroke table.
        public const string Strokes = "_DsStrokes";

        /// `(aa, solid base, gradient base, unused)`.
        public const string Scalars = "_DsGlobals";
    }

    /// A property one material carries, rather than one instance.
    ///
    /// Set on the material through `Material.SetFloat`, not through the
    /// BatchRendererGroup metadata: nothing on boundary B varies it per node,
    /// and a per-instance property costs sixteen bytes on every instance
    /// whether or not it differs between them.
    public static class PaintMaterialProperties
    {
        /// The coverage below which [`MaterialClass.LitCutout`] discards a
        /// fragment.
        public const string Cutoff = "_DsCutoff";
    }

}
