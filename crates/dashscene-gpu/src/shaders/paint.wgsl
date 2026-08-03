// The render shaders: one instanced quad per row of the instance buffer.
//
// Concatenated after `sdf.wgsl`, so the coverage math here is the same source
// the layer-2 conformance suite evaluates — not a copy of it. That textual
// inclusion is what `docs/decisions/shader-library-and-layer-2.md` D1 chose,
// and it is why nothing below re-derives a distance.

// Mirrors `dashscene_gpu::Instance`. The two four-float vectors come first so
// both sit at a 16-byte offset; the trailing pad is what makes the Rust type
// and this one agree on a 64-byte array stride.
struct Instance {
    bounds: vec4f,
    corners: vec4f,
    kind: u32,
    row: u32,
    shape: u32,
    clip_offset: u32,
    clip_count: u32,
    layer: u32,
    opacity: f32,
    _pad: u32,
}

// `InstanceKind`, as one discriminant. There is no separate tag to read
// without it, which is the point: the two used to be separate fields whose
// values collided, and this shader painted a shadow with a solid-fill row.
const KIND_SHADOW_DROP: u32 = 0u;
const KIND_SHADOW_INNER: u32 = 1u;
const KIND_BACKDROP: u32 = 2u;
const KIND_FILL_SOLID: u32 = 3u;
const KIND_FILL_GRADIENT: u32 = 4u;
const KIND_FILL_IMAGE: u32 = 5u;
const KIND_STROKE: u32 = 6u;

// `StrokeAlign`, as `GpuStroke::align` carries it.
const ALIGN_INSIDE: u32 = 0u;
const ALIGN_CENTER: u32 = 1u;
const ALIGN_OUTSIDE: u32 = 2u;

struct ClipBox {
    x: f32, y: f32, w: f32, h: f32,
    corners: vec4f,
}

// Mirrors `dashscene_gpu::render::GpuStroke`, which is `dashpaint::Stroke` in
// std430 order. `align` is 0 = Inside, 1 = Center, 2 = Outside — the numbers
// `sdf.wgsl`'s `stroke_coverage` is written against, assigned on the Rust side
// by an exhaustive match so a reordered variant is a compile error rather than
// a silently different band.
struct Stroke {
    color: vec4f,
    width: f32,
    align: u32,
    _pad: vec2u,
}

struct Viewport {
    // Drawable size in document units. The painter draws at unit scale, so this
    // is also its pixel size (story #580; a device-pixel ratio is #585's).
    size: vec2f,
    // Distance, in document units, over which an edge ramps. One unit at unit
    // scale. A uniform rather than `fwidth`, for the reason `sdf.wgsl` gives.
    aa: f32,
    _pad: f32,
}

@group(0) @binding(0) var<storage, read> instances: array<Instance>;
@group(0) @binding(1) var<storage, read> solids: array<vec4f>;
@group(0) @binding(2) var<storage, read> clip_boxes: array<ClipBox>;
@group(0) @binding(3) var<uniform> viewport: Viewport;
@group(0) @binding(4) var<storage, read> strokes: array<Stroke>;

struct VertexOut {
    @builtin(position) position: vec4f,
    // `@interpolate(flat)` is stated, not assumed: wgpu 30 stopped defaulting
    // integer shader I/O to flat, and a value that interpolated would index a
    // different row per fragment.
    @location(0) @interpolate(flat) instance: u32,
    // The fragment's position in document space.
    @location(1) local: vec2f,
}

// How far past its own `bounds` an instance draws.
//
// Zero for everything but a stroke. A stroke instance is stated over the node's
// *fill* box — `docs/decisions/instance-buffer-contract.md`, and the packer
// pushes it with `..base` — while an Outside stroke paints a full width beyond
// that box and a Center stroke half of one. The quad is built from `bounds`, so
// without this the outer half of every non-Inside stroke would be clipped away
// by the geometry it is drawn on, which looks exactly like a thinner stroke
// rather than like a bug.
//
// The same three cases as `dashscene-skia`'s `stroke_outset`, which is also
// what the packer grows a drop shadow's silhouette by. Here it is read from the
// stroke row instead, because a stroke instance names one.
//
// Only the *lower* bound of this number is a correctness property, and the
// tests pin only that: a quad too small clips the band, while a quad too large
// shades a few more fragments that the coverage then discards and draws exactly
// the same picture. Returning `width` for a Center stroke survives mutation
// testing for that reason. What it would cost is fill rate, which is R-T2's
// concern and has no instrument in this slice.
fn instance_outset(inst: Instance) -> f32 {
    if inst.kind != KIND_STROKE {
        return 0.0;
    }
    let s = strokes[inst.row];
    if s.align == ALIGN_CENTER {
        return s.width * 0.5;
    }
    if s.align == ALIGN_OUTSIDE {
        return s.width;
    }
    // ALIGN_INSIDE, and nothing else: `stroke_align` on the Rust side maps an
    // exhaustive match over `StrokeAlign`, so no fourth value can arrive here.
    // Stated as the fall-through rather than as a fourth branch, so that this
    // function is total whatever a future variant does before it is taught to
    // this file — an unknown alignment then draws inside the quad it already
    // has, which is a wrong picture rather than a clipped one.
    return 0.0;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> VertexOut {
    let inst = instances[index];
    // The quad, grown by the antialiasing width so the ramp is not clipped by
    // the geometry it belongs to, and by whatever this instance draws beyond
    // its own bounds.
    let margin = viewport.aa + instance_outset(inst);
    let lo = inst.bounds.xy - vec2f(margin);
    let hi = inst.bounds.xy + inst.bounds.zw + vec2f(margin);
    // Two triangles, as a triangle strip of four vertices.
    let corner = vec2f(
        select(lo.x, hi.x, (vertex & 1u) == 1u),
        select(lo.y, hi.y, (vertex & 2u) == 2u),
    );
    // Document space (y down, origin top-left) to clip space.
    let ndc = vec2f(
        corner.x / viewport.size.x * 2.0 - 1.0,
        1.0 - corner.y / viewport.size.y * 2.0,
    );
    var out: VertexOut;
    out.position = vec4f(ndc, 0.0, 1.0);
    out.instance = index;
    out.local = corner;
    return out;
}

// Coverage of the clip region this instance names: the intersection of its
// boxes. An empty range is unclipped, so the loop runs zero times and the
// coverage is one — the property a range has and a sentinel would not.
fn clip_coverage(inst: Instance, p: vec2f) -> f32 {
    var cover = 1.0;
    for (var i = 0u; i < inst.clip_count; i = i + 1u) {
        let b = clip_boxes[inst.clip_offset + i];
        let half_size = vec2f(b.w, b.h) * 0.5;
        let centre = vec2f(b.x, b.y) + half_size;
        let d = rounded_box_sdf(p - centre, half_size, b.corners);
        cover = min(cover, coverage(d, viewport.aa));
    }
    return cover;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let inst = instances[in.instance];
    let half_size = inst.bounds.zw * 0.5;
    let centre = inst.bounds.xy + half_size;
    let d = rounded_box_sdf(in.local - centre, half_size, inst.corners);

    // Coverage is per kind, and cannot be computed before the kind is known: a
    // fill covers the shape's interior, and a stroke covers a band that an
    // Outside stroke puts entirely *outside* it. Taking the fill's ramp first
    // and multiplying would have left every Outside stroke at zero coverage
    // everywhere it draws.
    var shape = 0.0;
    if inst.kind == KIND_STROKE {
        let s = strokes[inst.row];
        shape = stroke_coverage(d, s.width, f32(s.align), viewport.aa);
    } else {
        shape = coverage(d, viewport.aa);
    }
    var cover = shape * clip_coverage(inst, in.local) * inst.opacity;
    if cover <= 0.0 {
        discard;
    }
    // Story #580 drew the solid fill; story #710 added the stroke beside it.
    // Gradients and images are #582's, shadows and backdrop blur #584's.
    //
    // **Both `kind` and `tag`, never `tag` alone.** `tag` means a different
    // enum for each kind — a `PaintTag` for a fill, a `ShadowKind` for a
    // shadow, a `BlurKind` for a backdrop — and their discriminants collide:
    // `PaintTag::Solid`, `ShadowKind::Inner` and `BlurKind::Backdrop` are all
    // 1. Reading `tag` alone made a shadow instance paint `solids[row]` with
    // `row` indexing the *shadow* table, so a node with an inner shadow drew
    // whatever colour happened to sit at that row, over its own fill.
    //
    // A kind this shader cannot draw yet draws *nothing*, and does not fall
    // through to a colour. Painting it black would be loud, but it would also
    // corrupt every node that carries one: an inner shadow is packed after the
    // fill, so a black shadow instance covers the fill it belongs to. Drawing
    // nothing leaves the picture correct for the subset this story implements
    // and absent for the rest.
    //
    // Not a silent drop: the packer emits the instance, the layer-1 golden
    // shows it, and `docs/decisions/pipelines-and-layer-3.md` lists what is and
    // is not drawn. What is refused here is a *wrong* pixel, not a diagnostic.
    var colour = vec4f(0.0);
    if inst.kind == KIND_FILL_SOLID {
        colour = solids[inst.row];
    } else if inst.kind == KIND_STROKE {
        colour = strokes[inst.row].color;
    } else {
        discard;
    }
    // Premultiplied, which is what the blend state below expects.
    let a = colour.a * cover;
    return vec4f(colour.rgb * a, a);
}
