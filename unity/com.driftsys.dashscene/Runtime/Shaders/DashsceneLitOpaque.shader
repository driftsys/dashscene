// The lit opaque class — `docs/decisions/unity-painter-uses-brg.md` D1's
// second.
//
// Writes depth and does not blend, so a document takes part in the engine's
// depth buffer and its main directional light. **It cannot express partial
// coverage**: with no blending, every fragment of the quad is opaque, so a
// rounded corner would be a square one and a clipped edge would not be cut.
// The painter refuses to route a node with a corner radius, a clip or a stroke
// to this class with a named diagnostic — drawing a square where a pill was
// authored is exactly the silent drop P4 forbids.
Shader "Dashscene/LitOpaque"
{
    Properties
    {
        _DsQuad("Node box (x, y, w, h)", Vector) = (0, 0, 0, 0)
        _DsCorners("Corner radii (tl, tr, br, bl)", Vector) = (0, 0, 0, 0)
        _DsShade("Opacity, outset, rotation", Vector) = (1, 0, 0, 0)
        _DsPivot("Rotation pivot", Vector) = (0, 0, 0, 0)
        _DsPaint("Kind, row, clip offset, clip count", Vector) = (0, 0, 0, 0)
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
            Name "DashsceneLitOpaque"
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

            #define DASHSCENE_CLASS_LIT_OPAQUE
            #include "DashsceneInstance.hlsl"
            #include "DashsceneLighting.hlsl"

            DsVaryings DsVertexStage(DsAttributes input)
            {
                return DsVertex(input);
            }

            float4 DsFragmentStage(DsVaryings input) : SV_Target
            {
                float4 shaded = DsShade(input);
                return float4(DsLit(shaded.rgb), 1.0);
            }
            ENDHLSL
        }
    }

    Fallback Off
}
