// The names that are NOT per-instance: the global buffers, and the properties a
// material carries rather than an instance.
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

        /// One row per glyph run: the run's fill, then its atlas scale, its
        /// screen-pixel MSDF range and whether the row was resolved.
        ///
        /// **Per run and not per glyph.** A run's fill and its `px_range` are
        /// one value for every glyph it places, and the per-glyph half — which
        /// texels this quad samples — rides on the instance's own
        /// `_DsCorners`, which is what the lean painter does with the same two
        /// halves.
        ///
        /// Bound globally like the other three, so the collision issue #1297
        /// names applies to it: the last painter to draw supplies the runs
        /// every painter shades from.
        public const string Glyphs = "_DsGlyphs";

    }

    /// The properties one material carries, rather than one instance.
    ///
    /// Set on the material through `Material.SetFloat` and
    /// `Material.SetTexture`, not through the BatchRendererGroup metadata.
    /// `Cutoff` is per material because nothing on boundary B varies it per
    /// node and a per-instance property costs sixteen bytes on every instance
    /// whether or not it differs between them; `Atlas` is per material because
    /// a texture cannot be anything else.
    public static class PaintMaterialProperties
    {
        /// The coverage below which [`MaterialClass.LitCutout`] discards a
        /// fragment.
        public const string Cutoff = "_DsCutoff";

        /// The MSDF sheet a text material samples.
        ///
        /// **Per material and not global, which is why it is in this class**:
        /// a document may name more than one sheet — one per face of the
        /// cascade — and a texture is a per-material binding, so the painter
        /// mints one text material per atlas and sets this with
        /// `Material.SetTexture`.
        ///
        /// The other member here is a scalar and this one is a texture, so the
        /// class name is about how a value is bound rather than about what it
        /// is. `unity/package-gate` holds this one to a `TEXTURE2D` declaration
        /// and its `SAMPLER` in the shading, which is the form a texture takes
        /// and none of the three forms it accepts for the rest.
        public const string Atlas = "_DsAtlas";
    }

}
