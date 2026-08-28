// The names that are NOT per-instance: the buffers and scalars a material
// carries rather than an instance.
//
// Deliberately apart from `PaintProperties.cs`, which holds the per-instance
// set. `unity/package-gate` reads that file to decide what every shader must
// declare, and this one to decide what a shader is ALLOWED to declare beyond
// it — so a name in the wrong file makes the gate demand the wrong thing of
// every shader in the package.

namespace Driftsys.Dashscene
{
    /// The properties one material carries, rather than one instance.
    ///
    /// Set on the material through `Material.SetBuffer`, `Material.SetVector`,
    /// `Material.SetFloat` and `Material.SetTexture`, not through the
    /// BatchRendererGroup metadata. **`BrgPainter` resolves each of these to a
    /// property id once** and calls the `int` overloads, because the `string`
    /// ones hash the name on every call and the heap is bound per material on
    /// every frame.
    ///
    /// **Every one of them is per material, and that is issue #1297's fix.**
    /// The first five were bound with `Shader.SetGlobalBuffer` and
    /// `Shader.SetGlobalVector` until 2026-08-29 — both process-wide, so two
    /// painters in one process shared one paint heap and the last one to draw
    /// supplied the rows every painter's fragments shaded from. A painter now
    /// binds them on the materials it registered itself, so a second painter
    /// reaches nothing the first one draws with.
    ///
    /// `Cutoff` and `Atlas` are per material for reasons of their own, and
    /// those reasons are the ones each member states: nothing on boundary B
    /// varies the cutoff per node, and a texture cannot be bound any other way.
    public static class PaintMaterialProperties
    {
        /// Solid colours and gradient rows, in one buffer at the two bases
        /// [`Scalars`] carries.
        public const string Paints = "_DsPaints";

        /// The flat clip-box array every clip region names a range of.
        public const string ClipBoxes = "_DsClipBoxes";

        /// The stroke table.
        public const string Strokes = "_DsStrokes";

        /// `(aa, solid base, gradient base, unused)`.
        ///
        /// A `UnityPerMaterial` member rather than a bare uniform, which is
        /// what a per-material constant has to be: the SRP Batcher binds that
        /// buffer per material and leaves a uniform outside it to the global
        /// namespace this class no longer uses.
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
        /// **Bound on the text materials alone**, because the shading declares
        /// it under `DASHSCENE_CLASS_TEXT` and no other class can reach a glyph
        /// run.
        public const string Glyphs = "_DsGlyphs";

        /// The coverage below which [`MaterialClass.LitCutout`] discards a
        /// fragment.
        ///
        /// Per material because nothing on boundary B varies it per node and a
        /// per-instance property costs sixteen bytes on every instance whether
        /// or not it differs between them.
        public const string Cutoff = "_DsCutoff";

        /// The MSDF sheet a text material samples.
        ///
        /// A document may name more than one sheet — one per face of the
        /// cascade — and a texture is a per-material binding, so the painter
        /// mints one text material per atlas and sets this with
        /// `Material.SetTexture`.
        ///
        /// The rest of this class is a buffer or a scalar and this one is a
        /// texture, which is a difference the gate reads: `unity/package-gate`
        /// holds this one to a `TEXTURE2D` declaration and its `SAMPLER` in the
        /// shading, which is the form a texture takes and none of the forms it
        /// accepts for the rest.
        public const string Atlas = "_DsAtlas";
    }
}
