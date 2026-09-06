// The BRG painter's shading, shared by its three material classes and by the
// text shader beside them.
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
// Four shaders include this file, each defining exactly one of
// `DASHSCENE_CLASS_UNLIT_OVERLAY`, `DASHSCENE_CLASS_LIT_OPAQUE`,
// `DASHSCENE_CLASS_LIT_CUTOUT` or `DASHSCENE_CLASS_TEXT` before it. The first
// three are `docs/decisions/unity-painter-uses-brg.md` D1's material classes;
// the fourth is **not** a class — MSDF coverage is partial coverage by
// construction, so a glyph cannot be drawn by the non-blending opaque class at
// all, and text is always blended whichever class the NODES take.

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
#if !defined(DASHSCENE_CLASS_UNLIT_OVERLAY) && !defined(DASHSCENE_CLASS_LIT_OPAQUE) && !defined(DASHSCENE_CLASS_LIT_CUTOUT) && !defined(DASHSCENE_CLASS_TEXT)
#error "define one of DASHSCENE_CLASS_UNLIT_OVERLAY, DASHSCENE_CLASS_LIT_OPAQUE, DASHSCENE_CLASS_LIT_CUTOUT or DASHSCENE_CLASS_TEXT before including DashsceneInstance.hlsl"
#endif
// **Two is as wrong as none**, and a first version guarded only the second: a
// shader defining two compiled, and `unity/package-gate` then read whichever
// appeared first, so a shader could declare the right class and the wrong one
// together and pass.
#if defined(DASHSCENE_CLASS_UNLIT_OVERLAY) && (defined(DASHSCENE_CLASS_LIT_OPAQUE) || defined(DASHSCENE_CLASS_LIT_CUTOUT) || defined(DASHSCENE_CLASS_TEXT))
#error "exactly one DASHSCENE_CLASS_* may be defined"
#endif
#if defined(DASHSCENE_CLASS_LIT_OPAQUE) && (defined(DASHSCENE_CLASS_LIT_CUTOUT) || defined(DASHSCENE_CLASS_TEXT))
#error "exactly one DASHSCENE_CLASS_* may be defined"
#endif
#if defined(DASHSCENE_CLASS_LIT_CUTOUT) && defined(DASHSCENE_CLASS_TEXT)
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
// One glyph of one run. Only the text class emits or shades it: MSDF coverage
// is partial coverage by construction, so a glyph cannot be drawn by the
// non-blending opaque class at all, and text is therefore always drawn through
// `Dashscene/Text` whichever class the painter draws its NODES with.
#define DS_KIND_TEXT          3u

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
#define DS_GLYPH_WORDS     2u

// The paint heap: solid colours and gradient rows in one buffer, at the two
// bases `_DsGlobals` carries. One buffer rather than two because the lean
// painter uses one, and because a `StructuredBuffer` costs a `t` register
// whether or not it is full.
StructuredBuffer<float4> _DsPaints;
StructuredBuffer<float4> _DsClipBoxes;
StructuredBuffer<float4> _DsStrokes;

// The baked gradient strip, and the sampler it is read through (story #1449).
// One row per gradient row of the paint heap, 256 texels wide, RGBA8 —
// `dashpaint::gradient_strip` bakes it, `ds_runtime_gradient_strip` hands the
// rows out, and `BrgPainter` uploads them into a `Texture2D`.
//
// **LINEAR, never sRGB.** A stop's components are sRGB-encoded and this project
// blends in that space, so the sampler must hand back the stored code points
// unconverted — exactly what the stop loop this replaced read out of the heap.
// `BrgPainter` constructs the texture with `linear: true` for that reason and
// `paint.wgsl` binds `Rgba8Unorm` for the same one.
//
// **Bilinear and clamped, and that is the whole mechanism.** Between two texel
// centres a filtered read is the linear interpolation the ramp is already made
// of, so this is not an approximation of the ramp along `t` but the same
// piecewise-linear function evaluated by the sampler instead of by a loop; the
// clamp at both ends is what makes the two ends the clamped colours, which is
// the ramp's own `TileMode::Clamp` rule.
//
// **Declared by every class, including text.** It costs the text class one `t`
// register for a texture no glyph fragment samples — a run's fill is a solid
// colour in `_DsGlyphs` and never a gradient — and it is declared there anyway
// because `BindHeapTo` is the one path every material goes through, and a
// material whose shader declares a sampler nothing bound reads an unbound one.
TEXTURE2D(_DsGradientStrip);
SAMPLER(sampler_DsGradientStrip);

#ifdef DASHSCENE_CLASS_TEXT
// One row per glyph run: `(r, g, b, a)` then `(1/atlas width, 1/atlas height,
// px range, resolved)`. `Runtime/PaintHeap.cs` carries the other copy of the
// layout, and `unity/package-gate` holds the two together.
//
// **Declared for the text class alone**, so the three node classes compile
// exactly as they did: a `StructuredBuffer` costs a `t` register whether or not
// anything reads it, and none of them can reach `DS_KIND_TEXT`.
StructuredBuffer<float4> _DsGlyphs;

// The MSDF sheet this material's runs sample.
//
// **A texture cannot be bound any other way**, which is one of the reasons
// `PaintMaterialProperties` holds only per-material names. A
// document may name more than one sheet — one per face of the cascade — and a
// texture is a per-material binding, so the painter mints one text material per
// atlas and the draw commands for a run name the material its sheet is on.
//
// The texture is LINEAR and not sRGB: the three channels are distances, not
// colour. Bilinear, no mips. That is the painter's to set up, and
// `Runtime/Engine/AtlasTexture.cs` reads the format back rather than assuming
// it.
TEXTURE2D(_DsAtlas);
SAMPLER(sampler_DsAtlas);
#endif

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
// **And the converse holds for any member a pass actually reads, which is
// measured rather than reasoned.** `_DsGlobals` was moved into this buffer for
// issue #1297 and left out of the four `Properties` blocks; every player draw
// then failed with `A BatchDrawCommand is using a pass from the shader
// "Dashscene/UnlitOverlay" that is not SRP Batcher compatible. Reason:
// "UnityPerMaterial var is not declared in shader property section"`, and the
// frame was blank at all thirteen sampled node centres — `just unity-render`,
// 6000.3.23f1, macOS/Metal, Apple M3, 2026-08-29. Declaring it in the four
// blocks is what fixed it. `_DsCutoff` sat in this buffer and in one
// `Properties` block, and the three shaders that did not declare it drew all
// the same — the explanation the two runs support is that a uniform no pass
// statement reads does not survive that pass's compile, and neither run
// measured that directly. Rather than rest on it, all four shaders now declare
// every member of this block, and `unity/package-gate`'s
// `every_per_material_member_is_declared_by_every_shader` holds them there.
//
// **The five per-instance members are the non-instanced fallback and nothing
// draws with them**: under `DOTS_INSTANCING_ON` the `#define`s below replace
// each name with a metadata load, and a `BatchRendererGroup` always draws with
// that variant. `_DsCutoff` is NOT in that group — it is a real per-material
// value the cutout class reads on the instanced variant too, because the
// painter writes no metadata for it, and `_DsGlobals` is a second such value,
// which every class reads.
//
// **`_DsPaint` is `uint4` here and a `Vector` in the `Properties` block**,
// which is float4: the same sixteen bytes with a different meaning. That
// mismatch is unreachable while the instanced accessor is what reads it — it
// loads raw words and reinterprets them as `uint4`, the layout the C# packer
// writes — but it becomes a real defect the moment something draws this
// material through a MeshRenderer, so it is written down rather than left to be
// rediscovered.
//
// **`_DsCutoff` DOES resolve under `DOTS_INSTANCING_ON`, measured rather than
// reasoned.** URP's own Lit declares its `_Cutoff` as a DOTS instanced property
// with a default rather than relying on this buffer, which was evidence that a
// BatchRendererGroup draw might not read `UnityPerMaterial` at all — so this
// block carried an open question until `just unity-render` drew the cutout
// class twice at two thresholds. Unity 6000.3.22f1, macOS/Metal, Apple M3,
// 2026-08-23: at a cutoff of 0.5 the class inked all 13 sampled node centres,
// and at a cutoff of 2 — above any coverage a fragment can have, so `clip`
// must discard every one — it inked none, with 601144 of 786432 pixels
// differing between the two frames. A value that did not reach the stage would
// have drawn the same picture both times, whatever the stage read instead.
// Issue #1307.
//
// **That is one graphics API.** Metal is a translation of this HLSL, and issue
// #1195 is a measured case of a translation differing from what the source
// says; GLES 3.2 and Vulkan on the target fleet are untested here.
CBUFFER_START(UnityPerMaterial)
    float4 _DsQuad;
    float4 _DsCorners;
    float4 _DsShade;
    float4 _DsPivot;
    uint4  _DsPaint;
    // (aa, solid base, gradient base, gradient strip rows).
    //
    // The fourth component is the height of the bound `_DsGradientStrip`, which
    // is the divisor that turns a gradient row index into a v coordinate.
    // **At least one**, because a frame with no gradient still uploads a
    // one-row placeholder and a zero here would divide by nothing.
    //
    // `aa` is the width in document units over which an edge ramps — one
    // device pixel, resolved by the painter from the document-to-screen scale,
    // because a fragment shader that took it from `fwidth` would disagree with
    // the layer-2 conformance harness, which has no derivatives at all.
    //
    // **In this buffer rather than at global scope, which is issue #1297's
    // half of the fix on the shading side.** A uniform declared outside a
    // CBUFFER lands in `$Globals`, which is one namespace for the process — so
    // two painters shaded from one set of bases, and only a global setter could
    // write it. Here the painter writes it with `Material.SetVector`, and that
    // the value written reaches the fragment stage is measured rather than
    // reasoned: with the solid base written one row high, `just unity-render`
    // inked 11 of 13 sampled node centres instead of 13, its smallest distance
    // from the clear colour fell from 0.514 to 0.420 and the per-instance
    // colour advantage from 0.599 to -0.109 — 6000.3.23f1, macOS/Metal, Apple
    // M3, 2026-08-29. A value that did not reach the stage would have drawn the
    // same picture both times, whatever the stage read instead.
    float4 _DsGlobals;
    // Per material rather than per instance: nothing on boundary B varies it
    // per node, and a per-instance property costs sixteen bytes on every
    // instance whether or not it differs between them. Declared here for all
    // four shaders, and in all four `Properties` blocks, though only the cutout
    // shader reads it — see the note above this block for why the second half
    // is not optional.
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
//
// **The body is compiled out for a document that clips nothing**, which is
// `DS_HAS_CLIPS`. The keyword is toggled per material from
// `ds_runtime_kind_set` on every drawn frame, and the set is a census of the
// tables the commit this frame draws produced — so no instance can name a clip
// range in a variant built without the loop. The unclipped answer either way;
// what changes is whether the loop, the box loads and the distances exist.
// `paint.wgsl` removes the same code through an `override` constant, which is
// what a pipeline-specialising painter has where a Unity one has a keyword.
float DsClipCoverage(uint offset, uint count, float2 p)
{
#if DS_HAS_CLIPS
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
#else
    return 1.0;
#endif
}

// One gradient row's colour at document point `p`.
//
// Reads the first two of the twelve words in the order `paint.wgsl`'s
// `gradient_colour` writes them, and reads the ramp itself out of the baked
// strip. The handles are stored normalised to the node's box, which is what
// makes a gradient row shareable between nodes of different sizes.
//
// **The ten stop words are still in the heap and are no longer read here**
// (story #1449). The row's layout is boundary B's and is what the lean painter
// packs, so this reads fewer of its words rather than the packer writing fewer.
float4 DsGradientColour(uint row, float4 bounds, float2 p)
{
    uint base = (uint)_DsGlobals.z + row * DS_GRADIENT_WORDS;
    float4 handles = _DsPaints[base];
    float4 frame = _DsPaints[base + 1u];
    float2 origin = bounds.xy + handles.xy * bounds.zw;
    float2 primary = bounds.xy + handles.zw * bounds.zw;
    float2 secondary = bounds.xy + frame.xy * bounds.zw;

    uint kind = (uint)frame.z;

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

    // One sample, in place of the walk over eight offsets and up to eight
    // colours this used to do per fragment (story #1449). `dashpaint`'s
    // `bake_row` evaluated `gradient_ramp`'s own rules at 256 positions on the
    // CPU, so this is the same ramp read back rather than a second one.
    //
    // `t` addresses the row's 256 texels directly: the strip is baked at texel
    // centres and the sampler is filtering, so a `t` between two centres reads
    // the linear interpolation between them, which is what the ramp is there.
    //
    // The v coordinate is this gradient's own row centre. `_DsGlobals.w` is the
    // strip texture's height and is never zero — a frame with no gradient
    // uploads a one-row placeholder that no instance names, for the same reason
    // `paint.wgsl` binds one.
    return SAMPLE_TEXTURE2D_LOD(
        _DsGradientStrip,
        sampler_DsGradientStrip,
        float2(t, ((float)row + 0.5) / _DsGlobals.w),
        0);
}

#ifdef DASHSCENE_CLASS_TEXT
// One MSDF sample for the fragment at document point `p`.
//
// **This is composition and not arithmetic, which is why it is here.** R-T5
// single-sources the RESOLVE — `median3` and `msdf_coverage` — into `Sdf.hlsl`,
// generated from the WGSL, and this file calls it. A texture sample cannot be
// generated from WGSL at all, because its binding is not portable, so
// `paint.wgsl`'s `msdf_sample` has no generated twin and its mapping is
// rewritten here. What is rewritten is the mapping and never the resolve.
//
// `rect` is the glyph's rectangle in NORMALISED texture coordinates,
// `[u, v, du, dv]`, all four positive. `quad` is the document-space rectangle
// it covers, `[x, y, w, h]`, y-DOWN. `half` is half a source texel in the same
// normalised units, and it is the parameter named `half_texel` below —
// `half` is an HLSL type keyword.
//
// **The v axis runs opposite to `t.y` and nothing is flipped to make it do
// so.** `dashpaint::AtlasGlyph::atlas_px` has a bottom-left origin and so does
// a Unity texture coordinate, so the rectangle crosses unchanged and `rect.y`
// is the glyph's BOTTOM edge; document space is y-down, so `t.y` is 0 at the
// glyph's top, which is `rect.y + rect.w`. `dashscene-skia` subtracts from the
// sheet's height instead, because Skia's images are top-left — copying that
// line here would flip twice and draw every glyph upside down in a way that
// looks like a transform bug.
//
// **The clamp is what keeps the sample inside this glyph.** The quad is
// deliberately grown by the antialiasing width, so without it every glyph would
// read its neighbour along a one-unit fringe. Half a texel in is also exactly
// what makes filtering safe: a bilinear footprint taken from a texel's own
// centre weights that texel alone at the edge, so no gutter is needed.
// `min`/`max` around the bounds rather than the bounds themselves, as
// `paint.wgsl` does: a rectangle under two texels wide has `lo` past `hi`, and
// `clamp` with a reversed range is not defined to do anything sensible.
float3 DsMsdfSample(float4 rect, float4 quad, float2 half_texel, float2 p)
{
    float2 t = (p - quad.xy) / quad.zw;
    float2 uv = float2(rect.x + t.x * rect.z, rect.y + rect.w - t.y * rect.w);
    float2 lo = rect.xy + half_texel;
    float2 hi = rect.xy + rect.zw - half_texel;
    uv = clamp(uv, min(lo, hi), max(lo, hi));
    return SAMPLE_TEXTURE2D_LOD(_DsAtlas, sampler_DsAtlas, uv, 0).rgb;
}
#endif

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

    uint kind = paint.x;
    uint row = paint.y;

    float4 colour;
    float shape;
#ifdef DASHSCENE_CLASS_TEXT
    if (kind == DS_KIND_TEXT)
    {
        // Word 0 is the run's fill, which the coverage below modulates. Word 1
        // is `(1/atlas width, 1/atlas height, px range, resolved)`.
        colour = _DsGlyphs[row * DS_GLYPH_WORDS];
        float4 msdf = _DsGlyphs[row * DS_GLYPH_WORDS + 1u];

        // The glyph's own rectangle, in texels on `_DsCorners` and scaled here
        // into normalised coordinates. `_DsCorners` carries the atlas rectangle
        // on a text instance because a glyph has no rounded box — the same
        // member the lean painter's `Instance::corners` spends the same way.
        // Hoisted, as `quad` and `paint` are above: under
        // `DOTS_INSTANCING_ON` the name is a macro expanding to a metadata
        // load, so two swizzles of it are two loads of the same sixteen bytes,
        // per fragment, on the one hot path this story adds.
        float4 corners = _DsCorners;
        float2 scale = msdf.xy;
        float4 rect = float4(corners.xy * scale, corners.zw * scale);

        // **A row nothing resolved draws nothing, and it is the gate rather
        // than the colour that says so.** A zeroed row has a zero `px_range`,
        // and `msdf_coverage` is then 0.5 whatever the sample was — the run's
        // colour at half alpha over the whole quad, in a picture that is meant
        // to be empty. `paint.wgsl` carries the same gate on both its MSDF
        // arms.
        shape = 0.0;
        if (msdf.w != 0.0)
        {
            shape = msdf_coverage(
                DsMsdfSample(rect, quad, 0.5 * scale, input.local),
                msdf.z);
        }
    }
    else
#endif
    {
    // The plain fill: a solid colour on a sharp box that nothing clips. The
    // commonest instance in every scene measured for epic #1441, and the one
    // the general path below charges the most for relative to what it draws —
    // a stroke branch it does not take, a gradient branch it does not take, and
    // a clip call whose loop runs zero times.
    //
    // **It is the general path's own arithmetic, not an approximation of it.**
    // With no clip range `DsClipCoverage` returns exactly 1.0, so the general
    // path computes `coverage(d, aa) * 1.0 * _DsShade.x` and this computes
    // `coverage(d, aa) * _DsShade.x` — the same product, since multiplying by
    // one is exact. The distance is computed from the same centre with the same
    // half-size, and the alpha is bound in the same order: floating-point
    // multiplication does not associate, so a different grouping would move the
    // last bit of a channel and the goldens with it.
    //
    // What it saves is what a literal zero radius lets the compiler fold:
    // `rounded_box_sdf`'s corner select and its radius arithmetic reduce before
    // the shader runs, which is why the distance is recomputed here rather than
    // the one below being reused.
    //
    // **No baked-vector condition, unlike `paint.wgsl`'s twin.** That shader
    // tests `in.shape == 0u` because a baked vector's silhouette IS its
    // coverage mask and its box says nothing about where it inks; this painter
    // refuses such a node outright — `FramePacker` reports `VectorField` and
    // emits no instance — so no fragment here can carry one.
    //
    // **No `discard`, also unlike that twin.** Returning early is what saves
    // the work; discarding a zero-coverage fragment as well would change what
    // the opaque and cutout classes do with it, and those two decide that in
    // their own fragment stages.
    if (kind == DS_KIND_FILL_SOLID && paint.w == 0u && all(_DsCorners == 0.0))
    {
        float2 plainHalf = quad.zw * 0.5;
        float plainD = rounded_box_sdf(
            input.local - (quad.xy + plainHalf), plainHalf, float4(0.0, 0.0, 0.0, 0.0));
        float plainCover = coverage(plainD, aa) * _DsShade.x;
        float4 plainColour = _DsPaints[(uint)_DsGlobals.y + row];
        return float4(plainColour.rgb, plainColour.a * plainCover);
    }

    // **The rounded box, computed only where a rounded box is what is drawn.**
    // `_DsCorners` carries the glyph's rectangle in ATLAS TEXELS on a text
    // instance, so on the text arm above this would evaluate `rounded_box_sdf`
    // against radii of a few hundred texels for a box a few document units
    // across — per fragment, in the one hot path text adds, for a value that
    // arm never reads. `paint.wgsl` computes it unconditionally because one
    // shader there serves every kind; here the text class serves exactly one.
    float2 halfSize = quad.zw * 0.5;
    float2 centre = quad.xy + halfSize;
    float d = rounded_box_sdf(input.local - centre, halfSize, _DsCorners);

    // **Compiled out for a document whose paint table interned no stroke**,
    // which is `DS_HAS_STROKES`. The set is a census of that table, taken at
    // the commit this frame draws, so no instance can name a stroke row in a
    // variant built without the arm.
    //
    // **What a frame that somehow held one would draw, and it is NOT what
    // `paint.wgsl` draws.** Here the instance falls into the fill arm below and
    // reads `_DsPaints[solid base + row]` — but `row` on a stroke instance
    // names a row of the STROKE table, so this is another table's row read as a
    // colour, which is the failure class
    // `a_kind_this_shader_cannot_draw_is_black_not_another_tables_row` exists
    // for on the other painter. `paint.wgsl` reaches its final `else` and
    // discards instead, which
    // `crates/dashscene-gpu/tests/kind_set.rs::a_stroke_under_a_stroke_free_pipeline_draws_nothing`
    // pins. The difference is left standing rather than closed: matching it
    // needs a `kind == DS_KIND_STROKE` compare on the variant this story exists
    // to make cheap, for a state the kind set makes unreachable, and this
    // painter has no `force_kind_set` seam to reach it from a test either.
    //
    // **Both halves go together, and here they already are one branch**: this
    // arm carries the stroke table's colour AND its coverage, so gating it gates
    // the read as well.
#if DS_HAS_STROKES
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
#endif
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
    }

    float cover = shape * DsClipCoverage(paint.z, paint.w, input.placed) * _DsShade.x;
    return float4(colour.rgb, colour.a * cover);
}

#endif // DASHSCENE_INSTANCE_INCLUDED
