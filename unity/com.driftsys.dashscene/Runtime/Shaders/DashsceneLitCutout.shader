// The lit cutout class — `docs/decisions/unity-painter-uses-brg.md` D1's third.
//
// Writes depth and does not blend, like the opaque class, but discards a
// fragment whose coverage falls below `_DsCutoff`. That is how a shape survives
// into the depth buffer: the silhouette is exact to within one fragment, and
// the edge is hard rather than anti-aliased. A node whose edge must be smooth
// belongs on the overlay class.
Shader "Dashscene/LitCutout"
{
    Properties
    {
        _DsQuad("Node box (x, y, w, h)", Vector) = (0, 0, 0, 0)
        _DsCorners("Corner radii (tl, tr, br, bl)", Vector) = (0, 0, 0, 0)
        _DsShade("Opacity, outset, rotation", Vector) = (1, 0, 0, 0)
        _DsPivot("Rotation pivot", Vector) = (0, 0, 0, 0)
        _DsPaint("Kind, row, clip offset, clip count", Vector) = (0, 0, 0, 0)
        // Per material, not per instance: it is the class's own threshold
        // rather than anything boundary B carries.
        _DsCutoff("Coverage below which a fragment is discarded", Range(0, 1)) = 0.5
    }

    SubShader
    {
        Tags
        {
            "RenderType" = "TransparentCutout"
            "Queue" = "AlphaTest"
            "RenderPipeline" = "UniversalPipeline"
            // Declared as well as pragma'd. Unity reads this tag to decide
            // whether a SubShader is usable at all on the running device, and
            // a BatchRendererGroup needs 4.5 — a pass whose pragma says so on
            // a SubShader that does not declare it is selected and then fails.
            "ShaderModel" = "4.5"
        }

        Pass
        {
            Name "DashsceneLitCutout"
            Tags { "LightMode" = "UniversalForward" }

            ZWrite On
            ZTest LEqual
            Cull Off

            HLSLPROGRAM
            #pragma target 4.5
            #pragma multi_compile _ DOTS_INSTANCING_ON
            // No shadow keywords. `DsLit` takes the main light's colour and
            // direction and samples no shadow map, so a `_MAIN_LIGHT_SHADOWS`
            // variant would compile to the same code and double the variant
            // count for nothing. Add them with the sampling, not before it.
            #pragma vertex DsVertexStage
            #pragma fragment DsFragmentStage

            #define DASHSCENE_CLASS_LIT_CUTOUT
            #include "DashsceneInstance.hlsl"
            #include "DashsceneLighting.hlsl"

            DsVaryings DsVertexStage(DsAttributes input)
            {
                return DsVertex(input);
            }

            float4 DsFragmentStage(DsVaryings input) : SV_Target
            {
                float4 shaded = DsShade(input);
                clip(shaded.a - _DsCutoff);
                return float4(DsLit(shaded.rgb), 1.0);
            }
            ENDHLSL
        }
    }

    Fallback Off
}
