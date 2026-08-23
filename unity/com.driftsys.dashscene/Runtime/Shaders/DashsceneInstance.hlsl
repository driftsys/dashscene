// The BRG painter's shading, shared by its three material classes.
//
// **The arithmetic is not here.** Every signed-distance, coverage and gradient
// function this file calls comes from `Sdf.hlsl`, which is generated from
// `crates/dashscene-gpu/src/shaders/sdf.wgsl` by the compiler wgpu already runs
// that file through — `docs/specification/03-target-hardware-rules.md` R-T5.
// What is here is the composition: which row an instance names, in what order
// coverage multiplies, and how a clip combines. Adding a function that computes
// a distance or a coverage ramp *here* is the thing R-T5 forbids; add it to the
// WGSL and regenerate.
//
// **The heap layout mirrors the lean painter's, word for word.** A gradient row
// is twelve `float4`s in the order `paint.wgsl`'s `gradient_colour` reads them,
// a clip box is two, a stroke is two. That is not a coincidence to be tidied
// away later: it is what gives two painters written in different languages a
// chance of drawing the same picture, and it is what issue #828's portable
// conformance suite will be stated over.
//
// Three material classes include this file, each defining exactly one of
// `DASHSCENE_CLASS_UNLIT_OVERLAY`, `DASHSCENE_CLASS_LIT_OPAQUE` or
// `DASHSCENE_CLASS_LIT_CUTOUT` before it —
// `docs/decisions/unity-painter-uses-brg.md` D1's three.

#ifndef DASHSCENE_INSTANCE_INCLUDED
#define DASHSCENE_INSTANCE_INCLUDED

// URP's `Core.hlsl` is the only include a BatchRendererGroup pass needs, and
// that is checked rather than assumed: it reaches `Input.hlsl`, which reaches
// `UniversalDOTSInstancing.hlsl` — the file that declares
// `BuiltinPropertyMetadata` and so `unity_ObjectToWorld` as an instanced
// property. URP's own `Hidden/Universal Render Pipeline/BRGPicking` includes
// exactly this and nothing else.
//
// **A draft of this file also included
// `…/universal/ShaderLibrary/UnityInstancing.hlsl`, which does not exist** —
// that file is in `com.unity.render-pipelines.core`. Every variant failed to
// compile, and the editor gate reported all three shaders clean anyway, because
// an import compiles variants lazily and never built the one a
// BatchRendererGroup draws with. `just unity-editor` compiles
// `DOTS_INSTANCING_ON` explicitly for exactly that reason.
#include "Packages/com.unity.render-pipelines.universal/ShaderLibrary/Core.hlsl"
#include "Sdf.hlsl"

// Exactly one class, and the shader that includes this says which. A file
// reached with none defined would compile — silently, as an overlay — which is
// the failure mode `#error` exists for.
#if !defined(DASHSCENE_CLASS_UNLIT_OVERLAY) && !defined(DASHSCENE_CLASS_LIT_OPAQUE) && !defined(DASHSCENE_CLASS_LIT_CUTOUT)
#error "define one of DASHSCENE_CLASS_UNLIT_OVERLAY, DASHSCENE_CLASS_LIT_OPAQUE or DASHSCENE_CLASS_LIT_CUTOUT before including DashsceneInstance.hlsl"
#endif
// **Two is as wrong as none**, and a first version guarded only the second: a
// shader defining two compiled, and `unity/package-gate` then read whichever
// appeared first, so a shader could declare the right class and the wrong one
// together and pass.
#if defined(DASHSCENE_CLASS_UNLIT_OVERLAY) && (defined(DASHSCENE_CLASS_LIT_OPAQUE) || defined(DASHSCENE_CLASS_LIT_CUTOUT))
#error "exactly one DASHSCENE_CLASS_* may be defined"
#endif
#if defined(DASHSCENE_CLASS_LIT_OPAQUE) && defined(DASHSCENE_CLASS_LIT_CUTOUT)
#error "exactly one DASHSCENE_CLASS_* may be defined"
#endif

// What one instance draws. The same partition `paint.wgsl` uses, renumbered to
// the three this painter emits — a kind it does not emit is not declared here,
// so an instance carrying one would take the fall-through below rather than
// matching a constant that means something else.
//
// The five `paint.wgsl` kinds this painter does not emit — the two shadow
// passes, the backdrop, the image fill and text — are refused on the C# side
// with a named diagnostic per node, which is P4. They are absent here because
// nothing can reach them.
#define DS_KIND_FILL_SOLID    0u
#define DS_KIND_FILL_GRADIENT 1u
#define DS_KIND_STROKE        2u

#define DS_GRADIENT_LINEAR  0u
#define DS_GRADIENT_RADIAL  1u
#define DS_GRADIENT_ANGULAR 2u
#define DS_GRADIENT_DIAMOND 3u

// `float4`s per row. Read by the C# packer through the constants in
// `Runtime/PaintHeap.cs`, which carry the same numbers and are what
// `unity/package-gate` holds these to.
#define DS_GRADIENT_WORDS 12u
#define DS_CLIP_WORDS      2u
#define DS_STROKE_WORDS    2u

// The paint heap: solid colours and gradient rows in one buffer, at the two
// bases `_DsGlobals` carries. One buffer rather than two because the lean
// painter uses one, and because a `StructuredBuffer` costs a `t` register
// whether or not it is full.
StructuredBuffer<float4> _DsPaints;
StructuredBuffer<float4> _DsClipBoxes;
StructuredBuffer<float4> _DsStrokes;

// (aa, solid base, gradient base, unused).
//
// `aa` is the width in document units over which an edge ramps — one device
// pixel, resolved by the painter from the document-to-screen scale, because a
// fragment shader that took it from `fwidth` would disagree with the layer-2
// conformance harness, which has no derivatives at all.
float4 _DsGlobals;

struct DsAttributes
{
    float4 positionOS : POSITION;
    UNITY_VERTEX_INPUT_INSTANCE_ID
};

struct DsVaryings
{
    float4 positionCS : SV_POSITION;
    // The point in DOCUMENT space, unrotated. Every SDF below evaluates in the
    // node's own axis-aligned frame, so a rounded rect stays a true rounded
    // rect at an angle rather than an axis-aligned approximation of one. The
    // lean painter's `out.local` is the same value for the same reason.
    float2 local      : TEXCOORD0;

    // The same point ROTATED — where the fragment actually is in document
    // space. **The clip is evaluated here and not at `local`.** A clip box is
    // stated in document space by an *ancestor*, and an ancestor is not
    // rotating; testing the clip against `local` rotates the clip along with
    // the node it clips. `paint.wgsl` carries this as `out.placed` for exactly
    // this reason and passes it to `clip_coverage`, and a first version of this
    // file passed `local` — so every rotated clipped node was cut along the
    // wrong rectangle, and the two painters could not agree on such a document.
    float2 placed     : TEXCOORD1;
    UNITY_VERTEX_INPUT_INSTANCE_ID
};

// The material's own constant buffer.
//
// A `Properties` block entry must appear in `UnityPerMaterial` or the SRP
// Batcher refuses the shader, so every one of them is declared here.
//
// **The five per-instance members are the non-instanced fallback and nothing
// draws with them**: under `DOTS_INSTANCING_ON` the `#define`s below replace
// each name with a metadata load, and a `BatchRendererGroup` always draws with
// that variant. `_DsCutoff` is NOT in that group — it is a real per-material
// value the cutout class reads on the instanced variant too, because the
// painter writes no metadata for it.
//
// **`_DsPaint` is `uint4` here and a `Vector` in the `Properties` block**,
// which is float4: the same sixteen bytes with a different meaning. That
// mismatch is unreachable while the instanced accessor is what reads it — it
// loads raw words and reinterprets them as `uint4`, the layout the C# packer
// writes — but it becomes a real defect the moment something draws this
// material through a MeshRenderer, so it is written down rather than left to be
// rediscovered.
//
// **Whether `_DsCutoff` resolves under `DOTS_INSTANCING_ON` is unverified.**
// URP's own Lit declares its `_Cutoff` as a DOTS instanced property with a
// default rather than relying on this buffer, which is evidence that the
// fallback path may not be what a BRG draw reads. Nothing here can tell: `just
// unity-editor` compiles and does not evaluate, and no frame has been drawn.
// Issue #1307 carries it.
CBUFFER_START(UnityPerMaterial)
    float4 _DsQuad;
    float4 _DsCorners;
    float4 _DsShade;
    float4 _DsPivot;
    uint4  _DsPaint;
    // Per material rather than per instance: nothing on boundary B varies it
    // per node, and a per-instance property costs sixteen bytes on every
    // instance whether or not it differs between them. Declared for all three
    // classes because the CBUFFER layout must match the material's property
    // block; only the cutout shader reads it.
    float  _DsCutoff;
CBUFFER_END

#ifdef UNITY_DOTS_INSTANCING_ENABLED
    UNITY_DOTS_INSTANCING_START(MaterialPropertyMetadata)
        // The node's box in document space: (x, y, w, h).
        UNITY_DOTS_INSTANCED_PROP(float4, _DsQuad)
        // Corner radii: (top_left, top_right, bottom_right, bottom_left), the
        // order `dashpaint::CornerRadii` and `rounded_box_sdf` both use.
        UNITY_DOTS_INSTANCED_PROP(float4, _DsCorners)
        // (opacity, outset, rotation, unused). `outset` is how far past the box
        // this instance's ink reaches, which the vertex stage grows the quad
        // by; only its lower bound is a correctness property.
        UNITY_DOTS_INSTANCED_PROP(float4, _DsShade)
        // (pivot.x, pivot.y, unused, unused) in document space.
        UNITY_DOTS_INSTANCED_PROP(float4, _DsPivot)
        // (kind, row, clip offset, clip count).
        UNITY_DOTS_INSTANCED_PROP(uint4, _DsPaint)
    UNITY_DOTS_INSTANCING_END(MaterialPropertyMetadata)

    #define _DsQuad    UNITY_ACCESS_DOTS_INSTANCED_PROP(float4, _DsQuad)
    #define _DsCorners UNITY_ACCESS_DOTS_INSTANCED_PROP(float4, _DsCorners)
    #define _DsShade   UNITY_ACCESS_DOTS_INSTANCED_PROP(float4, _DsShade)
    #define _DsPivot   UNITY_ACCESS_DOTS_INSTANCED_PROP(float4, _DsPivot)
    #define _DsPaint   UNITY_ACCESS_DOTS_INSTANCED_PROP(uint4, _DsPaint)
#endif

// Coverage of the clip region `[offset, offset + count)` at document point `p`.
//
// **`min` across boxes, multiplied into the shape's coverage by the caller.**
// `docs/decisions/clip-edge-semantics.md` fixes the second half — a clip
// contributes anti-aliased coverage and it multiplies into the shape's own —
// and leaves how overlapping boxes combine open.
//
// **The `min` is an interim default, not a ruling, and issue #1281 is the
// reason it is written down as one.** That issue observes that
// `dashscene-gpu` takes the minimum while `dashscene-skia` pushes one
// anti-aliased clip per box and lets Skia's clip stack combine them; the two
// agree wherever at most one box covers a pixel fractionally, and can differ
// where two clip edges cross the same pixel. No fixture in this repository
// covers that case — `v03-clips` looks like it does and does not, because
// every box in it is integer- and axis-aligned. This painter takes `min`
// because agreeing with a shipped painter beats inventing a third behaviour,
// and #1281 exists so that choice does not harden into the rule by being the
// thing that shipped.
float DsClipCoverage(uint offset, uint count, float2 p)
{
    float cover = 1.0;
    for (uint i = 0u; i < count; i = i + 1u)
    {
        float4 box = _DsClipBoxes[(offset + i) * DS_CLIP_WORDS];
        float4 corners = _DsClipBoxes[(offset + i) * DS_CLIP_WORDS + 1u];
        float2 halfSize = box.zw * 0.5;
        float2 centre = box.xy + halfSize;
        float d = rounded_box_sdf(p - centre, halfSize, corners);
        cover = min(cover, coverage(d, _DsGlobals.x));
    }
    return cover;
}

// One gradient row's colour at document point `p`.
//
// Reads the twelve words in the order `paint.wgsl`'s `gradient_colour` writes
// them, and hands the stops to the generated `gradient_ramp`. The handles are
// stored normalised to the node's box, which is what makes a gradient row
// shareable between nodes of different sizes.
float4 DsGradientColour(uint row, float4 bounds, float2 p)
{
    uint base = (uint)_DsGlobals.z + row * DS_GRADIENT_WORDS;
    float4 handles = _DsPaints[base];
    float4 frame = _DsPaints[base + 1u];
    float2 origin = bounds.xy + handles.xy * bounds.zw;
    float2 primary = bounds.xy + handles.zw * bounds.zw;
    float2 secondary = bounds.xy + frame.xy * bounds.zw;

    uint kind = (uint)frame.z;
    // Clamped to the row's own slot count, as the lean painter clamps it: a
    // loop that walked past the row would read the NEXT gradient's handles as
    // stops. The C# packer asserts the same bound before it writes the row.
    uint count = min((uint)frame.w, MAX_GRADIENT_STOPS);

    float t;
    if (kind == DS_GRADIENT_RADIAL)
    {
        t = gradient_radial_t(p, origin, primary, secondary);
    }
    else if (kind == DS_GRADIENT_ANGULAR)
    {
        t = gradient_angular_t(p, origin, primary, secondary);
    }
    else if (kind == DS_GRADIENT_DIAMOND)
    {
        t = gradient_diamond_t(p, origin, primary, secondary);
    }
    else
    {
        // Linear, and the fall-through rather than a fourth branch, so an
        // unknown kind draws a wrong picture rather than an undefined one —
        // the posture `paint.wgsl` takes at the same point.
        t = gradient_linear_t(p, origin, primary, secondary);
    }

    float4 lo = _DsPaints[base + 2u];
    float4 hi = _DsPaints[base + 3u];
    float offsets[8] = { lo.x, lo.y, lo.z, lo.w, hi.x, hi.y, hi.z, hi.w };
    float4 colours[8] = {
        _DsPaints[base + 4u], _DsPaints[base + 5u],
        _DsPaints[base + 6u], _DsPaints[base + 7u],
        _DsPaints[base + 8u], _DsPaints[base + 9u],
        _DsPaints[base + 10u], _DsPaints[base + 11u],
    };
    return gradient_ramp(t, offsets, colours, count);
}

DsVaryings DsVertex(DsAttributes input)
{
    UNITY_SETUP_INSTANCE_ID(input);
    DsVaryings output = (DsVaryings)0;
    UNITY_TRANSFER_INSTANCE_ID(input, output);

    float4 quad = _DsQuad;
    float4 shade = _DsShade;
    float outset = shade.y;

    // The mesh is the unit quad [0, 1] x [0, 1], so a corner is the box's
    // origin plus its extent, grown on every side by the outset. The outset is
    // resolved by the packer rather than here, as it is in the lean painter:
    // a stroke's alignment and a shadow's reach both live in rows the vertex
    // stage has no buffer for.
    //
    // **The margin is the outset PLUS the antialiasing width**, and leaving the
    // second term out is a defect in every edge the painter draws.
    // `coverage(d, aa)` reaches zero at `d = +aa/2` — half the ramp lies
    // OUTSIDE the box — so a quad that stops at the box never rasterises those
    // fragments and every fill, corner, stroke and clip edge steps from half
    // coverage straight to nothing instead of ramping. `paint.wgsl` grows by
    // `globals.aa + instance_outset(inst)` for exactly this reason and says so.
    float margin = outset + _DsGlobals.x;
    float2 corner = input.positionOS.xy;
    float2 local = quad.xy - margin.xx + corner * (quad.zw + 2.0 * margin.xx);

    // Rotation about the document-space pivot. `local` keeps the UNROTATED
    // point, so every SDF below evaluates in the node's own frame; only the
    // position handed to the rasteriser turns.
    float angle = shade.z;
    float2 pivot = _DsPivot.xy;
    float s, c;
    sincos(angle, s, c);
    float2 d = local - pivot;
    float2 placed = pivot + float2(d.x * c - d.y * s, d.x * s + d.y * c);

    output.local = local;
    output.placed = placed;
    // URP's own transform rather than `mul(UNITY_MATRIX_VP, mul(UNITY_MATRIX_M,
    // …))`: it resolves the DOTS instanced object-to-world where one is bound
    // and the per-draw matrix where one is not, so the shader draws the same
    // whether or not `DOTS_INSTANCING_ON` is set.
    output.positionCS = TransformObjectToHClip(float3(placed, 0.0));
    return output;
}

// The shaded colour, premultiplication left to the class that includes this.
//
// Coverage multiplies in one order and it is the reference painter's: the
// shape's own coverage, then the clip's, then the node's opacity, then the
// paint's own alpha.
float4 DsShade(DsVaryings input)
{
    UNITY_SETUP_INSTANCE_ID(input);

    float4 quad = _DsQuad;
    uint4 paint = _DsPaint;
    float aa = _DsGlobals.x;

    float2 halfSize = quad.zw * 0.5;
    float2 centre = quad.xy + halfSize;
    float d = rounded_box_sdf(input.local - centre, halfSize, _DsCorners);

    uint kind = paint.x;
    uint row = paint.y;

    float4 colour;
    float shape;
    if (kind == DS_KIND_STROKE)
    {
        // Colour first, then `(width, align, 0, 0)` — the order
        // `paint.wgsl`'s `struct Stroke { color, width, align, _pad }` declares.
        // A first version had the two words the other way round: internally
        // consistent, and not the row the other painter reads, which is what
        // the "word for word" claim in this file's header has to mean.
        colour = _DsStrokes[row * DS_STROKE_WORDS];
        float4 params = _DsStrokes[row * DS_STROKE_WORDS + 1u];
        shape = stroke_coverage(d, params.x, params.y, aa);
    }
    else
    {
        shape = coverage(d, aa);
        if (kind == DS_KIND_FILL_GRADIENT)
        {
            colour = DsGradientColour(row, quad, input.local);
        }
        else
        {
            colour = _DsPaints[(uint)_DsGlobals.y + row];
        }
    }

    float cover = shape * DsClipCoverage(paint.z, paint.w, input.placed) * _DsShade.x;
    return float4(colour.rgb, colour.a * cover);
}

#endif // DASHSCENE_INSTANCE_INCLUDED
