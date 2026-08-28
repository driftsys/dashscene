// The heap layout, the material classes, and the shader each one draws with.
//
// The property NAMES are next door in `PaintProperties.cs` and
// `PaintBindings.cs`, one file per binding kind — per-instance in the first,
// per-material in the second. That split is not tidiness:
// `unity/package-gate` reads each file for its own set and holds the shaders to
// them, and a single file would make it guess which name is which.
//
// **Engine-independent on purpose.** Nothing here references `UnityEngine`, so
// `unity/package-compat` compiles it against `netstandard.dll` 2.1.0 and
// `unity/package-gate` holds these numbers and names to the shader sources that
// carry the other copy. The BatchRendererGroup half that consumes them lives in
// `Runtime/Engine/`, which R-E10's second check covers — see
// `docs/decisions/r-e10-is-checked-in-two-halves.md`.
//
// **The layout is the lean painter's, word for word, with one stated
// exception.** A gradient row is the twelve `float4`s
// `crates/dashscene-gpu/src/shaders/paint.wgsl`'s `gradient_colour` reads, in
// that order; a clip box is two; a stroke is two, colour first. The exception
// is a stroke's `align`, which crosses as an `f32` here and a `u32` there —
// forced by this heap being a `StructuredBuffer<float4>`, and harmless because
// both painters hand it to `stroke_coverage` as a float.
// That is what gives two painters written in different languages a chance of
// drawing the same picture, and it is what issue #828's portable conformance
// suite will be stated over.

namespace Driftsys.Dashscene
{
    /// The heap layout the painter packs into and the shaders read out of.
    public static class PaintHeap
    {
        /// `float4`s per gradient row. `paint.wgsl`'s `GRADIENT_WORDS`.
        ///
        /// Two words of handles and kind, two of stop offsets, eight of stop
        /// colours.
        public const int GradientWords = 12;

        /// `float4`s per solid colour: the colour itself.
        public const int SolidWords = 1;

        /// `float4`s per clip box: `(x, y, w, h)` then the four radii.
        public const int ClipWords = 2;

        /// `float4`s per stroke: **the colour**, then `(width, align, 0, 0)`.
        ///
        /// Colour first, matching `paint.wgsl`'s
        /// `struct Stroke { color, width, align, _pad }`. This comment said the
        /// opposite while the code said this — the one place a reader would look
        /// for the row order, describing the layout a first version had and the
        /// review removed.
        public const int StrokeWords = 2;

        /// `float4`s per glyph-run row: **the run's fill**, then
        /// `(1/atlas width, 1/atlas height, px range, resolved)`.
        ///
        /// **Not the lean painter's `GpuGlyphRun` word for word, and the
        /// divergence is stated rather than tidied.** That row carries the
        /// atlas payload's own rectangle inside a shared residency texture —
        /// `uv = (u, v, sx, sy)` — because `dashscene-gpu` packs every sheet
        /// into one atlas. This painter gives each sheet its own `Texture2D`
        /// and its own material, so that origin is structurally `(0, 0)` and
        /// only the scale survives. Carrying a constant zero to look alike
        /// would tell a reader a residency atlas exists here, which it does
        /// not. The half-texel inset `msdf_sample` clamps with is `0.5 *`
        /// this scale and is derived in the shader rather than stored, for the
        /// same reason.
        ///
        /// `resolved` is `0` for a row nothing wrote, and the shader draws
        /// nothing for it — the gate `paint.wgsl` carries on both its MSDF
        /// arms, because a zeroed row has a zero `px_range` and
        /// `msdf_coverage` then answers `0.5` whatever the sample was, which
        /// paints the run's colour over the whole quad.
        public const int GlyphWords = 2;

        /// The stop slots a gradient row has, and the hard bound on what one
        /// gradient can carry.
        ///
        /// `MAX_GRADIENT_STOPS` in the shader library, which is generated — so
        /// this number and the shader's come from two places and
        /// `unity/package-gate` is what holds them together. A gradient with
        /// more stops than this is a named diagnostic
        /// ([`PackDiagnostic.GradientStopsTruncated`]) rather than a silent
        /// truncation, which is P4.
        public const int MaxGradientStops = 8;
    }

    /// The shader each material class draws with, by its `Shader` name.
    ///
    /// **The name is also the path `Resources.Load` resolves.** The painter
    /// loads `Dashscene/UnlitOverlay` from
    /// `Runtime/Resources/Dashscene/UnlitOverlay.shader`, so one constant
    /// serves both and neither can drift from the other. `Shader.Find` was
    /// what resolved these until issue #1313: a player build strips a shader
    /// no scene and no material references, and a `Resources` folder is
    /// included whether or not anything references it.
    ///
    /// `unity/package-gate` asserts that every name here is declared by a
    /// `.shader` the package ships, that every such shader is named here, and
    /// that each sits at the path its name implies — in all three directions,
    /// because a shader nothing registers passes a pragma check while the
    /// painter registers something else, and a shader in the wrong place
    /// passes every text check and returns null in a player.
    public static class PaintShaders
    {
        /// [`MaterialClass.UnlitOverlay`]'s shader.
        public const string UnlitOverlay = "Dashscene/UnlitOverlay";

        /// [`MaterialClass.LitOpaque`]'s shader.
        public const string LitOpaque = "Dashscene/LitOpaque";

        /// [`MaterialClass.LitCutout`]'s shader.
        public const string LitCutout = "Dashscene/LitCutout";

        /// The text shader, which every material class draws its glyph runs
        /// with.
        ///
        /// **Not a [`MaterialClass`], and that is the design rather than an
        /// omission.** A class decides how a NODE is drawn — blended, opaque,
        /// or thresholded — and MSDF coverage is partial coverage by
        /// construction, so a glyph cannot be drawn by the opaque class at all.
        /// Text is therefore always blended and always drawn through this one
        /// shader, whichever class the painter was built with, and
        /// [`For`](PaintShaders.For) does not answer it because no class
        /// selects it.
        ///
        /// It carries its own material per atlas rather than per class: a
        /// sheet is a texture, and a texture is a per-material binding.
        public const string Text = "Dashscene/Text";

        /// The shader one material class draws with.
        ///
        /// A `switch` with no default arm reaching a throw, rather than an
        /// array indexed by the enum: a value cast in from outside the declared
        /// three would index out of range with a message naming neither the
        /// value nor what it was for.
        public static string For(MaterialClass materialClass)
        {
            switch (materialClass)
            {
                case MaterialClass.UnlitOverlay:
                    return UnlitOverlay;
                case MaterialClass.LitOpaque:
                    return LitOpaque;
                case MaterialClass.LitCutout:
                    return LitCutout;
                default:
                    throw new System.ArgumentOutOfRangeException(
                        nameof(materialClass),
                        materialClass,
                        "not one of the three material classes "
                        + "docs/decisions/unity-painter-uses-brg.md D1 names.");
            }
        }

        /// Every shader this package ships, for a host that wants to enumerate
        /// them — the three material classes and the text shader, which is not
        /// one of them.
        ///
        /// **Nothing in this repository reads it, and its order is not a
        /// contract.** `unity/package-gate` derives the registered set from the
        /// `const string` declarations above rather than from this array, so a
        /// shader added there and forgotten here is caught by nothing. Said
        /// out loud because the alternative is a reader taking the order for a
        /// meaning it does not carry.
        public static readonly string[] All = { UnlitOverlay, LitOpaque, LitCutout, Text };
    }

    /// One instance's kind, as the shader branches on it.
    ///
    /// A subset of `paint.wgsl`'s eight, renumbered. The five this painter does
    /// not emit are absent rather than reserved: an instance carrying one could
    /// not be produced, and a constant that means something different in each
    /// painter is worse than one that exists in only one of them.
    public enum PaintKindTag : uint
    {
        /// A fill whose colour is one row of the solid table.
        FillSolid = 0,

        /// A fill whose colour is one gradient row evaluated at the fragment.
        FillGradient = 1,

        /// A stroke band around the node's rounded box.
        Stroke = 2,

        /// One glyph of one run: an MSDF quad sampled from its run's atlas.
        ///
        /// **Renumbered, like the three above.** `paint.wgsl` numbers text `7`
        /// among eight kinds; this painter emits four, and a constant that
        /// means something different in each painter is worse than one that
        /// exists in only one of them.
        Text = 3,
    }

    /// Which material class a painter draws a document with.
    ///
    /// `docs/decisions/unity-painter-uses-brg.md` D1's three, and a **host**
    /// setting rather than a document property: nothing on boundary B says
    /// which node is lit, and inventing a per-node flag would be discovering
    /// vocabulary rather than validating it, which P4 forbids.
    public enum MaterialClass
    {
        /// Alpha-blended, writing no depth and testing none. Coverage is the
        /// alpha, so corners, strokes and clip edges are anti-aliased. The
        /// class the bulk of a UI takes.
        UnlitOverlay = 0,

        /// Writes depth, does not blend, takes the engine's main light.
        ///
        /// **Cannot express partial coverage.** Every fragment of the quad is
        /// opaque, so a rounded corner would be a square one. The painter
        /// refuses a node that needs coverage on this class with a named
        /// diagnostic rather than drawing the wrong silhouette.
        LitOpaque = 1,

        /// Writes depth, does not blend, discards a fragment whose coverage is
        /// below the material's cutoff. The silhouette survives; the edge is
        /// hard.
        LitCutout = 2,
    }
}
