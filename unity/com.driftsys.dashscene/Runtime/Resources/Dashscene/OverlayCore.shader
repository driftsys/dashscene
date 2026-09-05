// The overlay class's opaque-core pass — R-T2 in this painter, story #1412.
//
// **One pass, drawn once per fully opaque fill before the blended instance.**
// It writes depth in the geometry queue and keeps only the fragments the
// node's shape and its clip cover completely, so a later-painted node's core
// rejects the pixels under it and the blended pass's own interior — at the
// same depth, under `ZTest Less` — is never shaded again. What is left to the
// blended pass is the antialiasing band, where this pass discarded. The
// picture is the one the overlay class drew before: an interior fragment reads
// the fill colour either way.
//
// Not a `MaterialClass`, for the reason `Dashscene/Text` is not: a host
// chooses the overlay class, and this is how that class draws opaque fills.
// Unlit, deliberately — `DsLit` would shade a core black in a scene with no
// light, which is what the showcase host builds.
Shader "Dashscene/OverlayCore"
{
    Properties
    {
        // Declared so a tool can enumerate the per-instance surface, and so
        // `unity/package-gate` can hold this shader and the C# packer to one
        // list of names. The values are never read from here: every one of
        // them is overridden per instance from the BatchRendererGroup buffer.
        _DsQuad("Node box (x, y, w, h)", Vector) = (0, 0, 0, 0)
        _DsCorners("Corner radii (tl, tr, br, bl)", Vector) = (0, 0, 0, 0)
        _DsShade("Opacity, outset, rotation", Vector) = (1, 0, 0, 0)
        _DsPivot("Rotation pivot", Vector) = (0, 0, 0, 0)
        _DsPaint("Kind, row, clip offset, clip count", Vector) = (0, 0, 0, 0)

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
    }

    SubShader
    {
        Tags
        {
            "RenderType" = "Opaque"
            "Queue" = "Geometry"
            "RenderPipeline" = "UniversalPipeline"
            // Declared as well as pragma'd. Unity reads this tag to decide
            // whether a SubShader is usable at all on the running device, and
            // a BatchRendererGroup needs 4.5 — a pass whose pragma says so on
            // a SubShader that does not declare it is selected and then fails.
            "ShaderModel" = "4.5"
        }

        Pass
        {
            Name "DashsceneOverlayCore"
            Tags { "LightMode" = "UniversalForward" }

            ZWrite On
            ZTest LEqual
            Cull Off

            HLSLPROGRAM
            // R-E11. 4.5 is Unity's spelling of Shader Model 5.0, which is what
            // `BatchRendererGroup` needs and what GLES 3.1 and above satisfy.
            #pragma target 4.5
            // R-E12. Unity refuses a BRG pass without the variant, naming it.
            #pragma multi_compile _ DOTS_INSTANCING_ON
            #pragma vertex DsVertexStage
            #pragma fragment DsFragmentStage

            #define DASHSCENE_CLASS_OVERLAY_CORE
            #include "Packages/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl"

            DsVaryings DsVertexStage(DsAttributes input)
            {
                return DsVertex(input);
            }

            float4 DsFragmentStage(DsVaryings input) : SV_Target
            {
                float4 shaded = DsShade(input);
                // **Only full coverage survives.** `shaded.a` is the shape's
                // coverage times the clip's times the node's opacity times the
                // paint's alpha, and the packer emits a core only where the
                // last two are one — so anything under `DS_CORE_FLOOR` is the
                // antialiasing ramp of the shape or of a clip edge, and the
                // blended pass draws it.
                clip(shaded.a - DS_CORE_FLOOR);
                return float4(shaded.rgb, 1.0);
            }
            ENDHLSL
        }
    }

    Fallback Off
}
