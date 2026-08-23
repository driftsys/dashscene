// The unlit overlay class — `docs/decisions/unity-painter-uses-brg.md` D1's
// first, and the one the bulk of a UI takes.
//
// Alpha-blended, writing no depth and testing none, so a document draws over
// whatever the engine drew before it. Coverage IS the alpha here, which is what
// lets a rounded corner, a stroke and a clip edge all be anti-aliased; the two
// lit classes below it cannot express partial coverage that way and say so.
Shader "Dashscene/UnlitOverlay"
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
    }

    SubShader
    {
        Tags
        {
            "RenderType" = "Transparent"
            "Queue" = "Transparent"
            "RenderPipeline" = "UniversalPipeline"
            // Declared as well as pragma'd. Unity reads this tag to decide
            // whether a SubShader is usable at all on the running device, and
            // a BatchRendererGroup needs 4.5 — a pass whose pragma says so on
            // a SubShader that does not declare it is selected and then fails.
            "ShaderModel" = "4.5"
        }

        Pass
        {
            Name "DashsceneUnlitOverlay"
            Tags { "LightMode" = "UniversalForward" }

            Blend SrcAlpha OneMinusSrcAlpha
            ZWrite Off
            ZTest Always
            Cull Off

            HLSLPROGRAM
            // R-E11. 4.5 is Unity's spelling of Shader Model 5.0, which is what
            // `BatchRendererGroup` needs and what GLES 3.1 and above satisfy.
            #pragma target 4.5
            // R-E12. Unity refuses a BRG pass without the variant, naming it.
            #pragma multi_compile _ DOTS_INSTANCING_ON
            #pragma vertex DsVertexStage
            #pragma fragment DsFragmentStage

            #define DASHSCENE_CLASS_UNLIT_OVERLAY
            #include "Packages/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl"

            DsVaryings DsVertexStage(DsAttributes input)
            {
                return DsVertex(input);
            }

            float4 DsFragmentStage(DsVaryings input) : SV_Target
            {
                float4 shaded = DsShade(input);
                // Straight alpha, not premultiplied: the blend state above is
                // `SrcAlpha OneMinusSrcAlpha`, and
                // `docs/decisions/blur-blends-in-srgb-encoded-space.md` makes
                // the space a term of boundary B rather than a painter's
                // choice.
                //
                // **The lean painter differs here, in the destination alpha
                // only.** It uses `PREMULTIPLIED_ALPHA_BLENDING` and
                // premultiplies in the shader, so the two agree on every colour
                // channel and disagree on what accumulates in the target's
                // alpha — `src.a` there against `src.a * src.a` here.
                // Irrelevant against an opaque backbuffer, and not irrelevant
                // the moment a host draws the document into a render texture
                // whose alpha it then consumes.
                return shaded;
            }
            ENDHLSL
        }
    }

    Fallback Off
}
