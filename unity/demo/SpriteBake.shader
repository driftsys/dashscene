// The faithful Canvas's sprite baker: one rounded-box shape, rasterised once at
// load through the SAME distance function the painter evaluates.
//
// Story #1444, and the fairness rules' rule 2
// (`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
// D2): a solid fill, its corners and a static stroke are one 9-sliced sprite per
// distinct shape, rasterised once at load and tinted through `Image.color`. This
// is the rasteriser. Nothing here runs per frame.
//
// **Why a shader rather than C#.** The rounded-box distance exists once, in
// `crates/dashscene-gpu/src/shaders/sdf.wgsl`, and `just sdf-hlsl` compiles it
// to the package's `Runtime/Shaders/Sdf.hlsl` (R-T5). A third copy written in C#
// would drift with no gate watching, and it would be the copy the epic's own
// comparison is judged against — so the baseline's shape would differ from the
// painter's for a reason the reading could not see. This includes the generated
// file instead, at the path a consumer of the package includes it by.
//
// **Why it lives outside the package.** `unity/package-gate` holds every
// `.shader` INSIDE `unity/com.driftsys.dashscene` to being a painter material
// class registered by `PaintShaders` and sitting at
// `Runtime/Resources/<name>.shader` — four assertions in `shader_pragmas.rs`,
// and `shader_sources`' own remarks say a shader elsewhere in the package is
// reported rather than tolerated. This one registers with no `BatchRendererGroup`
// and belongs to no material class, so it sits beside the demo host and the two
// `unity-demo*` recipes stage it into the throwaway project's `Assets/Resources/`
// — which is also what keeps a player build from stripping it, since
// `CanvasSprites` resolves it with `Shader.Find`. `unity/hlsl-conformance/
// SdfConformance.compute` is the same arrangement for the same reason.
//
// **What the baked texture is.** A square of `2 * border + 1` texels with a
// one-texel stretchable centre, so a 9-slice reproduces the shape at any node
// size. The distance is evaluated against a box inset from the texture's edge by
// `pad` — the stroke's outset past the node box plus the anti-aliasing band — so
// the sprite carries the outset and the `Image`'s `RectTransform` is the node
// box grown by `pad` on every side. `CanvasSprites` computes both and states the
// arithmetic there.
Shader "Dashscene/Samples/SpriteBake"
{
    Properties
    {
        _DsCorners("Corner radii (tl, tr, br, bl)", Vector) = (0, 0, 0, 0)

        // width, align (0 inside, 1 centre, 2 outside), ring, anti-aliasing band
        _DsStroke("Stroke (width, align, ring, aa)", Vector) = (0, 0, 0, 1)

        // The box's half-extent, then the texture's extent, both in texels.
        _DsBox("Box (half x, half y, size x, size y)", Vector) = (1, 1, 3, 3)

        // Rule 3's gradient bake. The handles are normalised to the node's box,
        // which is what `DashsceneInstance.hlsl`'s `DsGradientColour` reads them
        // as, so this pass works in unit space and needs no node extent.
        _DsHandles("Gradient (origin x, origin y, primary x, primary y)", Vector) = (0, 0, 1, 0)
        _DsFrame("Gradient (secondary x, secondary y, kind, stop count)", Vector) = (0, 1, 0, 0)
        _DsOffsetsLo("Stop offsets 0-3", Vector) = (0, 0, 0, 0)
        _DsOffsetsHi("Stop offsets 4-7", Vector) = (0, 0, 0, 0)

        // The node's own box inside the baked texture: its extent in texels,
        // then the padding the texture carries on every side.
        _DsUnit("Node (width, height, pad, unused)", Vector) = (1, 1, 1, 0)
    }

    SubShader
    {
        Tags { "RenderPipeline" = "UniversalPipeline" }

        Pass
        {
            Name "DashsceneSpriteBake"

            // A bake writes coverage, it does not composite: the target is
            // cleared by the blit and every texel is written exactly once.
            Blend Off
            ZWrite Off
            ZTest Always
            Cull Off

            HLSLPROGRAM
            // The same target the package's four shaders declare. `Sdf.hlsl` is
            // generated for it — `gradient_ramp` uses `uint`, `uint2` and an
            // unbounded `while` — and Unity's default of 2.5 does not carry
            // them on the ES subtargets an Android player build compiles.
            #pragma target 4.5
            #pragma vertex DsBakeVertex
            #pragma fragment DsBakeFragment

            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            #include "Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl"

            float4 _DsCorners;
            float4 _DsStroke;
            float4 _DsBox;

            struct DsBakeAttributes
            {
                float4 positionOS : POSITION;
                float2 uv : TEXCOORD0;
            };

            struct DsBakeVaryings
            {
                float4 positionCS : SV_POSITION;
                float2 uv : TEXCOORD0;
            };

            DsBakeVaryings DsBakeVertex(DsBakeAttributes input)
            {
                DsBakeVaryings output;
                output.positionCS = TransformObjectToHClip(input.positionOS.xyz);
                output.uv = input.uv;
                return output;
            }

            float4 DsBakeFragment(DsBakeVaryings input) : SV_Target
            {
                // **`0.5 - uv.y`, not `uv.y - 0.5`.** The document's y runs
                // down and `rounded_box_sdf` reads a negative y as the top, so
                // it takes `_DsCorners.x` and `.y` there. A texture's v runs up
                // and a `Sprite` maps v = 1 to the top of its `RectTransform`,
                // so without this flip a top-left radius would be baked into
                // the bottom-left corner of every sprite.
                float2 p = float2(input.uv.x - 0.5, 0.5 - input.uv.y) * _DsBox.zw;
                float d = rounded_box_sdf(p, _DsBox.xy, _DsCorners);

                float aa = _DsStroke.w;
                float ring = stroke_coverage(d, _DsStroke.x, _DsStroke.y, aa);
                float fill = coverage(d, aa);

                // Straight alpha over white, because `Image.color` tints a
                // sprite by multiplying every channel: a premultiplied texel
                // would darken with the tint's own alpha a second time.
                return float4(1.0, 1.0, 1.0, _DsStroke.z > 0.5 ? ring : fill);
            }
            ENDHLSL
        }

        // Rule 3: a gradient fill, baked once at load. The colour is the
        // generated `gradient_ramp` over the generated parameter, exactly as
        // `DashsceneInstance.hlsl` computes it per fragment — the same
        // arithmetic, evaluated once instead of every frame.
        //
        // **The alpha carries the box coverage as well as the ramp's**, so a
        // gradient on a rounded node keeps its corners. A team would reach for a
        // mask; a mask costs a stencil pass per node, which would be an expense
        // this baseline invented rather than one the design implies.
        Pass
        {
            Name "DashsceneGradientBake"

            Blend Off
            ZWrite Off
            ZTest Always
            Cull Off

            HLSLPROGRAM
            // 4.5, for the coverage pass's reason.
            #pragma target 4.5
            #pragma vertex DsBakeVertex
            #pragma fragment DsGradientFragment

            #include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
            #include "Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl"

            float4 _DsCorners;
            float4 _DsStroke;
            float4 _DsBox;
            float4 _DsHandles;
            float4 _DsFrame;
            float4 _DsOffsetsLo;
            float4 _DsOffsetsHi;
            float4 _DsUnit;
            float4 _DsStops[8];

            struct DsBakeAttributes
            {
                float4 positionOS : POSITION;
                float2 uv : TEXCOORD0;
            };

            struct DsBakeVaryings
            {
                float4 positionCS : SV_POSITION;
                float2 uv : TEXCOORD0;
            };

            DsBakeVaryings DsBakeVertex(DsBakeAttributes input)
            {
                DsBakeVaryings output;
                output.positionCS = TransformObjectToHClip(input.positionOS.xyz);
                output.uv = input.uv;
                return output;
            }

            float4 DsGradientFragment(DsBakeVaryings input) : SV_Target
            {
                // The document's y runs down, a texture's v runs up: the same
                // flip the coverage pass makes, for the same reason. `texel` is
                // in the baked texture's own space with y down, and `unit` is
                // the same point normalised to the NODE's box — which is the
                // space the handles are stored in, and which the texture is not,
                // because the texture carries the anti-aliasing pad on each
                // side.
                float2 texel = float2(input.uv.x, 1.0 - input.uv.y) * _DsBox.zw;
                float2 unit = (texel - _DsUnit.zz) / max(_DsUnit.xy, 1e-6);

                float2 origin = _DsHandles.xy;
                float2 primary = _DsHandles.zw;
                float2 secondary = _DsFrame.xy;
                uint kind = (uint)_DsFrame.z;
                uint count = min((uint)_DsFrame.w, MAX_GRADIENT_STOPS);

                float t;
                if (kind == 1u)
                {
                    t = gradient_radial_t(unit, origin, primary, secondary);
                }
                else if (kind == 2u)
                {
                    t = gradient_angular_t(unit, origin, primary, secondary);
                }
                else if (kind == 3u)
                {
                    t = gradient_diamond_t(unit, origin, primary, secondary);
                }
                else
                {
                    // Linear as the fall-through, which is what `paint.wgsl` and
                    // its HLSL twin both do: an unknown kind draws a wrong
                    // picture rather than an undefined one.
                    t = gradient_linear_t(unit, origin, primary, secondary);
                }

                float offsets[8] = {
                    _DsOffsetsLo.x, _DsOffsetsLo.y, _DsOffsetsLo.z, _DsOffsetsLo.w,
                    _DsOffsetsHi.x, _DsOffsetsHi.y, _DsOffsetsHi.z, _DsOffsetsHi.w,
                };
                float4 colours[8] = {
                    _DsStops[0], _DsStops[1], _DsStops[2], _DsStops[3],
                    _DsStops[4], _DsStops[5], _DsStops[6], _DsStops[7],
                };
                float4 ramp = gradient_ramp(t, offsets, colours, count);

                float d = rounded_box_sdf(texel - _DsBox.zw * 0.5, _DsBox.xy, _DsCorners);
                float box = coverage(d, _DsStroke.w);

                return float4(ramp.rgb, ramp.a * box);
            }
            ENDHLSL
        }
    }

    Fallback Off
}
