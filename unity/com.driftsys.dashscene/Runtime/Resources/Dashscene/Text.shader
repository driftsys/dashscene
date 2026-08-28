// The text class — one glyph of one run, as an MSDF quad sampled from its run's
// atlas.
//
// **Not a fourth `MaterialClass`, and the difference matters.** A class decides
// how a NODE is drawn: blended, opaque, or thresholded. MSDF coverage is
// partial coverage by construction — that is what the field is for — so a glyph
// cannot be drawn by the non-blending opaque class at all. Text is therefore
// always blended and always drawn through this shader, whichever class the
// painter draws its nodes with, and `PaintShaders.For` does not answer it
// because no class selects it.
//
// **One material per atlas, not one per class.** A sheet is a texture and a
// texture is a per-material binding, so a document naming two faces mints two
// materials over this one shader and the painter emits a draw command per
// contiguous run of instances that share one.
//
// It sits in `Runtime/Resources/Dashscene/` with the three class shaders and is
// loaded the same way, for the reason issue #1313 measured: a player build
// strips a shader that no scene and no material references, and an editor
// strips nothing — so `Shader.Find` resolved in every gate this repository had
// and returned null in the one configuration a customer ships. The file's name
// is the second half of the shader's declared name, which is what makes the
// name double as the path.
//
// The blend state is the overlay class's: coverage IS the alpha, which is what
// anti-aliases a glyph's edge. `ZTest Always` as well, so a document is a flat
// sheet drawn in submission order whichever class its nodes took — text drawn
// after a lit-opaque node is not occluded by it.
Shader "Dashscene/Text"
{
    Properties
    {
        // The five per-instance names, declared for the reason the other three
        // shaders declare them: so a tool can enumerate the surface and so
        // `unity/package-gate` can hold this shader and the C# packer to one
        // list. Every one is overridden per instance from the
        // BatchRendererGroup buffer.
        //
        // **Two of them mean something different here**, and neither is a
        // reinterpretation this shader invented: `_DsCorners` carries the
        // glyph's rectangle in atlas texels rather than four corner radii, and
        // `_DsPaint.y` indexes `_DsGlyphs` rather than the paint heap. The lean
        // painter spends `Instance::corners` the same way on the same kind, and
        // `_DsPaint.y` has always meant "a row in whichever table this kind
        // reads".
        _DsQuad("Glyph box (x, y, w, h)", Vector) = (0, 0, 0, 0)
        _DsCorners("Atlas rect in texels (x, y, w, h)", Vector) = (0, 0, 0, 0)
        _DsShade("Opacity, outset, rotation", Vector) = (1, 0, 0, 0)
        _DsPivot("Rotation pivot", Vector) = (0, 0, 0, 0)
        _DsPaint("Kind, run row, clip offset, clip count", Vector) = (0, 0, 0, 0)

        // Per material, not per instance, and declared here because a
        // `UnityPerMaterial` member the property section does not declare
        // makes the pass SRP-Batcher-incompatible — which a
        // BatchRendererGroup draw refuses outright.
        // `Runtime/Shaders/DashsceneInstance.hlsl` carries the rule, the run
        // that measured it, and why no default here can be an obvious absence.
        // The painter writes the value with `Material.SetVector` every frame.
        _DsGlobals("Edge width, solid base, gradient base", Vector) = (1, 0, 0, 0)

        // Declared by a class that never reads it, for the reason above:
        // every `UnityPerMaterial` member is declared by every shader.
        _DsCutoff("Coverage below which a fragment is discarded", Range(0, 1)) = 0.5

        // The sheet this material's runs sample. Set with
        // `Material.SetTexture`, one material per atlas.
        //
        // **"black" and not "white" as the default.** An unbound sampler reads
        // the default, and a black sample resolves through `msdf_coverage` to
        // zero coverage — nothing drawn. "white" would resolve to full
        // coverage and paint every glyph's whole quad as a solid box of the
        // run's colour, which is a plausible wrong picture rather than an
        // obvious absence.
        _DsAtlas("MSDF atlas (linear)", 2D) = "black" {}
    }

    SubShader
    {
        Tags
        {
            "RenderType" = "Transparent"
            "Queue" = "Transparent"
            "RenderPipeline" = "UniversalPipeline"
            // Declared as well as pragma'd, for the reason the other shaders
            // give: Unity reads this tag to decide whether a SubShader is
            // usable on the running device at all.
            "ShaderModel" = "4.5"
        }

        Pass
        {
            Name "DashsceneText"
            Tags { "LightMode" = "UniversalForward" }

            Blend SrcAlpha OneMinusSrcAlpha
            ZWrite Off
            ZTest Always
            Cull Off

            HLSLPROGRAM
            // R-E11. 4.5 is Unity's spelling of Shader Model 5.0, which is what
            // `BatchRendererGroup` needs and what GLES 3.1 and above satisfy.
            #pragma target 4.5
            // R-E12. Unity refuses a BRG pass without the variant, naming it —
            // and a BRG draw whose shader lacks it packs, submits and draws
            // NOTHING, with the reason only in the log.
            #pragma multi_compile _ DOTS_INSTANCING_ON
            #pragma vertex DsVertexStage
            #pragma fragment DsFragmentStage

            #define DASHSCENE_CLASS_TEXT
            #include "Packages/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl"

            DsVaryings DsVertexStage(DsAttributes input)
            {
                return DsVertex(input);
            }

            float4 DsFragmentStage(DsVaryings input) : SV_Target
            {
                // Straight alpha, not premultiplied, matching the blend state
                // above and the overlay class beside it.
                return DsShade(input);
            }
            ENDHLSL
        }
    }

    Fallback Off
}
