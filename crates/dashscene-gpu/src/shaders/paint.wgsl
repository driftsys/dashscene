// The render shaders: one instanced quad per row of the instance buffer.
//
// Concatenated after `sdf.wgsl`, so the coverage math here is the same source
// the layer-2 conformance suite evaluates — not a copy of it. That textual
// inclusion is what `docs/decisions/shader-library-and-layer-2.md` D1 chose,
// and it is why nothing below re-derives a distance.

// Mirrors `dashscene_gpu::Instance`. The two four-float vectors come first so
// both sit at a 16-byte offset, and the trailing pad word is what makes the
// Rust type and this one agree on an 80-byte array stride. The stride was 64
// until story #832 added the rotation; `outset` occupied the pad word before
// story #584 gave it a meaning.
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
    // How far past `bounds` this instance's ink reaches, resolved by the packer.
    // It was the trailing pad word until story #584 — see `instance_outset`.
    outset: f32,
    // The point this instance turns about, in document space, and the angle it
    // turns by, in radians (story #832). Zero is unrotated.
    //
    // `rotation_pivot` before `rotation`, and the order is load-bearing: WGSL
    // aligns a `vec2f` to eight bytes, so the other order would place it at 72
    // here while the Rust type packs it at 68, and every row after the first
    // would be read from the wrong offset. The trap `GlyphRun` above documents
    // against its own `half_uv`.
    rotation_pivot: vec2f,
    rotation: f32,
    // Padding to an 80-byte stride, which `array<Instance>` rounds to anyway
    // because `bounds` aligns this struct to 16. The Rust type declares the
    // same word, so both sides agree on where element `n` begins. The next
    // vertex-side scalar goes here, as `outset` did before story #584.
    _pad: f32,
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
const KIND_TEXT: u32 = 7u;

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

// Mirrors `dashscene_gpu::render::Globals` — what every fragment of the frame
// shares.
struct Globals {
    // Drawable size in document units. The painter draws at unit scale, so this
    // is also its pixel size (story #580; a device-pixel ratio is #585's).
    size: vec2f,
    // Distance, in document units, over which an edge ramps. One unit at unit
    // scale. A uniform rather than `fwidth`, for the reason `sdf.wgsl` gives.
    aa: f32,
    // The first word of the paint heap's gradient region. It moves with the
    // number of solid fills, which is why it is a frame value rather than a
    // constant.
    gradient_base: u32,
    // The first word of the paint heap's shadow region, after the gradients.
    // A frame value for the same reason.
    shadow_base: u32,
    // Thirty-two bytes, not twenty. A uniform-address-space struct's size
    // rounds up to a multiple of 16, so the three words below are what the
    // fifth member costs, and the Rust type declares them too — a struct that
    // agreed on five members and disagreed on its size would read every value
    // correctly and still bind at the wrong length.
    //
    // Three scalars rather than one `vec3u`: a three-component vector aligns to
    // **16**, so it would sit at offset 32 rather than 20 and take this struct
    // to 48. Story #583 met that exact trap with a `vec3f` in `GpuComposite`,
    // where wgpu reported "bound with size 16 where the shader expects 32".
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// Mirrors `dashscene_gpu::render::GpuImage` — an image fill's parameters with
// its residency slot resolved into them. `uv` is where the payload sits in the
// atlas bound for this draw, normalised; `size` is the payload's own extent in
// texels, which is what every scale mode but Tile is stated over.
//
// The atlas rectangle is on the row rather than on the instance because an
// image instance still needs `Instance::corners` for its own rounded box. Story
// #582's glyph instances took the other route — `corners` is meaningless for a
// glyph, so a glyph quad carries its texel rectangle there.
struct Image {
    uv: vec4f,
    // `Mat23`'s linear part, row-major: (a, b, c, d).
    transform: vec4f,
    // `Mat23`'s translation.
    translate: vec2f,
    // The payload's extent in texels.
    size: vec2f,
    scale_mode: u32,
    tile_scale: f32,
    _pad: vec2u,
}

// `ScaleMode`, mapped on the Rust side by an exhaustive match for the reason
// `Stroke::align` is.
const SCALE_FILL: u32 = 0u;
const SCALE_FIT: u32 = 1u;
const SCALE_CROP: u32 = 2u;
const SCALE_TILE: u32 = 3u;

// `GradientKind`, mapped on the Rust side by an exhaustive match for the same
// reason.
const GRADIENT_LINEAR: u32 = 0u;
const GRADIENT_RADIAL: u32 = 1u;
const GRADIENT_ANGULAR: u32 = 2u;
const GRADIENT_DIAMOND: u32 = 3u;

// How many words of the paint heap one gradient occupies —
// `dashscene_gpu::render::GRADIENT_WORDS`, which documents the layout. A row's
// words are `globals.gradient_base + row * GRADIENT_WORDS`.
//
// The two copies are held together by
// `the_gradient_kinds_are_distinct_and_match_the_shader`, which reads this
// file's own source and asserts the literal. A stride that disagreed would read
// another gradient's handles — a plausible picture rather than an absent one,
// which no coverage assertion catches.
const GRADIENT_WORDS: u32 = 12u;

// How many words of the paint heap one shadow occupies —
// `dashscene_gpu::render::SHADOW_WORDS`. A row's words are
// `globals.shadow_base + row * SHADOW_WORDS`, and the two copies of this
// literal are held together by the same source-text test the stride above and
// the gradient kinds use.
//
// Two words: the geometry the painter resolves per instance, then the colour.
//
//     +0   (offset.x, offset.y, sigma, spread)
//     +1   the shadow's colour
//
// `sigma`, not the authored blur radius: the mapping from one to the other is
// `dashpaint::BLUR_SIGMA_PER_RADIUS`, applied once on the Rust side
// (`pack::blur_sigma`) so that this painter and the reference painter cannot
// drift on a measured number.
const SHADOW_WORDS: u32 = 2u;

// Mirrors `dashscene_gpu::render::GpuGlyphRun` — one positioned run's shared
// parameters, with its atlas's residency slot resolved into them. `uv` maps a
// texel of the run's *source* atlas into the residency texture: texel `t` sits
// at `uv.xy + t * uv.zw`.
//
// `half_uv` before `px_range`, not after: WGSL aligns a `vec2f` to eight bytes,
// so the other order would put `half_uv` at offset 40 and round this struct to
// 64, while the Rust type packs it at 36 and is 48. Every row after the first
// would then be read from the wrong offset.
struct GlyphRun {
    color: vec4f,
    uv: vec4f,
    half_uv: vec2f,
    px_range: f32,
    // Non-zero when the four members above describe an atlas this frame made
    // resident. `render::GpuGlyphRun::resolved` is where that is derived and
    // what it is for; the short version is the one `Shape::resolved` gives —
    // a zeroed row is not an atlas, and reading one as though it were produces
    // exactly half coverage rather than none.
    resolved: u32,
}

// Mirrors `dashscene_gpu::render::GpuShape` — one baked-vector coverage mask.
// `plane` is the padded field quad in shape space, node-box-relative and
// y-down: `[left, top, right, bottom]`. `uv` is the field's own sub-rect in the
// residency texture. Member order for the reason `GlyphRun` gives.
struct Shape {
    plane: vec4f,
    uv: vec4f,
    half_uv: vec2f,
    px_range: f32,
    // Non-zero when the four members above describe a field this frame made
    // resident. `render::GpuShape::resolved` is where that is derived and what
    // it is for; the short version is that a zeroed row is not a field, and
    // reading one as though it were produces exactly half coverage rather than
    // none.
    resolved: u32,
}

@group(0) @binding(0) var<storage, read> instances: array<Instance>;
// The paint-parameter heap: the fill and effect parameters the fragment stage
// reads, as one array of words. The solid colours come first, so a solid fill's
// colour is still `paints[row]`; the gradient rows follow at
// `globals.gradient_base`, and the shadow rows after them at
// `globals.shadow_base`.
//
// One binding for three tables because there was no second one to take: the
// fragment stage reads the four storage buffers `downlevel_defaults` allows,
// and a gradient's stop array is indexed by a value this stage computes, so it
// cannot cross as a varying the way story #582's tables do. A shadow's row
// arrived under the same constraint and took the same route.
// `dashscene_gpu::render::paint_heap` writes the layout and
// `docs/decisions/the-paint-parameter-heap.md` is the reasoning.
@group(0) @binding(1) var<storage, read> paints: array<vec4f>;
@group(0) @binding(2) var<storage, read> clip_boxes: array<ClipBox>;
@group(0) @binding(3) var<uniform> globals: Globals;
@group(0) @binding(4) var<storage, read> strokes: array<Stroke>;
@group(0) @binding(5) var<storage, read> images: array<Image>;
@group(0) @binding(6) var atlas: texture_2d<f32>;
@group(0) @binding(7) var atlas_sampler: sampler;
// Story #582's two tables. **Vertex only** — the fragment stage already reads
// the four storage buffers `wgpu::Limits::downlevel_defaults` allows, and both
// of these carry values that are constant across an instance, so the stage that
// runs four times per quad reads them and hands the values across. See
// `docs/decisions/tables-the-vertex-stage-reads.md`.
@group(0) @binding(8) var<storage, read> glyph_runs: array<GlyphRun>;
@group(0) @binding(9) var<storage, read> shapes: array<Shape>;
// The sampler an MSDF payload is read through: linear, because a distance field
// is not a colour and nearest quantises the edge ramp to the atlas's texel
// grid. Image fills keep `atlas_sampler`. `render.rs` builds both, and says why
// the clamp in `msdf_sample` is what makes filtering safe without a gutter.
@group(0) @binding(10) var msdf_sampler: sampler;

// What the fragment stage needs of an instance, carried through the rasteriser
// rather than re-read from the instance array.
//
// # Why this is not just the instance index
//
// It was, until story #581. `wgpu::Limits::downlevel_defaults` allows four
// storage buffers per shader stage, and the image table is a fifth thing the
// fragment stage reads. Passing the instance's values instead of its index
// takes `instances` out of that stage's count entirely: the vertex stage reads
// two storage buffers and the fragment stage four.
//
// It is also the more ordinary shape for an instanced renderer. A fragment
// needs the instance's *values*, not the instance *array*, and every value here
// is constant across the quad — which is what `@interpolate(flat)` says for the
// integers, and what makes `bounds` and `corners` interpolate to themselves.
//
// `@interpolate(flat)` is stated, not assumed: wgpu 30 stopped defaulting
// integer shader I/O to flat, and a value that interpolated would name a
// different row per fragment.
struct VertexOut {
    @builtin(position) position: vec4f,
    // The fragment's position in document space.
    @location(0) local: vec2f,
    // The node's own box, always — never the field quad a masked instance is
    // drawn over. A gradient's frame is stated over the node box even when a
    // coverage mask confines where it lands, which is what `dashscene-skia`
    // does and what `gradient_colour` resolves its handles against. A masked
    // gradient is the instance where the two differ, and it draws correctly
    // only because this varying is the box rather than the quad.
    @location(1) bounds: vec4f,
    @location(2) corners: vec4f,
    // `Instance`'s four index members, packed so that they cost one variable
    // rather than four: kind, row, clip_offset, clip_count.
    @location(3) @interpolate(flat) rows: vec4u,
    @location(4) @interpolate(flat) opacity: f32,
    // `Instance::shape` — the coverage-mask row plus one, or zero. Carried
    // because the fragment stage decides its coverage by it and cannot read the
    // instance array.
    @location(5) @interpolate(flat) shape: u32,
    // Story #582's three parameter slots, written by the vertex stage from a
    // table the fragment stage does not bind. Each is a different thing per
    // kind, and every kind that reads one also wrote it:
    //
    //     text          params0 = the run's colour
    //                   params1 = the glyph's sub-rect in the atlas texture
    //                   params2 = (half_u, half_v, px_range, resolved)
    //     masked        params0 = the field's device quad, [x, y, w, h]
    //                   params1 = the field's sub-rect in the atlas texture
    //                   params2 = (half_u, half_v, px_range, resolved)
    //     everything    zero, and nothing reads them
    //
    // **`params2.w` is spare on neither path** (issues #972 and #993). It
    // carries `Shape::resolved` on the one and `GlyphRun::resolved` on the
    // other, and the fragment stage's arm draws nothing when it is zero — a
    // payload the frame could not make resident, where sampling the zeroed row
    // would report half coverage rather than none. A fourth component taken
    // here for something else would silently paint every refused mask and every
    // refused glyph.
    //
    // Flat, and stated: these are per-instance constants, and a flat varying is
    // exact where an interpolated one is only exact because every vertex agreed.
    @location(6) @interpolate(flat) params0: vec4f,
    @location(7) @interpolate(flat) params1: vec4f,
    @location(8) @interpolate(flat) params2: vec4f,
    // The fragment's position in document space **after** this instance's
    // rotation — where it actually lands on the canvas (story #832).
    //
    // Equal to `local` for an unrotated instance, which is every instance
    // before that story, so nothing that read `local` changed meaning.
    //
    // It exists because a clip region does not turn with the node. `local` is
    // the node's own frame, which is what every SDF, gradient and image fill
    // wants; a clip box is stated in document space by an *ancestor*, and an
    // ancestor is not rotating. Testing the clip against `local` would rotate
    // the clip along with the node — the reference painter is explicit about
    // this, applying the clip first and the rotation inside it.
    //
    // Ten of the fifteen inter-stage variables `downlevel_defaults` allows.
    @location(9) placed: vec2f,
}

// How far past its own `bounds` an instance draws, as the packer resolved it.
//
// Non-zero for the two kinds whose ink does not coincide with the box their
// instance is stated over: a **stroke**, which is stated over the node's fill
// box while an Outside stroke paints a full width beyond it, and a **drop
// shadow**, which is drawn spread, displaced and blurred away from the
// silhouette. The quad is built from `bounds`, so without this the outer half
// of every non-Inside stroke and the whole of a shadow's falloff would be
// clipped away by the geometry they are drawn on — which looks like a thinner
// stroke and a cropped shadow rather than like a bug.
//
// **It was computed here until story #584**, from the stroke row. A shadow's
// parameters are in the paint heap, which is bound to the fragment stage only,
// and both stages already read the four storage buffers
// `wgpu::Limits::downlevel_defaults` allows — so this stage could not have read
// a shadow row at all. `pack::shadow_ink_reach` and `pack::stroke_outset`
// resolve both instead, and this stage no longer reads the stroke table, which
// takes it to three storage buffers of four.
//
// Only the *lower* bound of this number is a correctness property, and the
// tests pin only that: a quad too small clips the ink, while a quad too large
// shades a few more fragments that the coverage then discards and draws exactly
// the same picture. What it would cost is fill rate, which is R-T2's concern
// and has no instrument in this slice.
fn instance_outset(inst: Instance) -> f32 {
    return inst.outset;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> VertexOut {
    let inst = instances[index];

    var out: VertexOut;
    out.params0 = vec4f(0.0);
    out.params1 = vec4f(0.0);
    out.params2 = vec4f(0.0);

    // The rectangle this instance's ink lives in. The node's own box for
    // everything but a masked instance, whose ink is confined to the coverage
    // field's padded quad — `plane` is relative to the node's origin, at unit
    // scale, which is what `docs/decisions/baked-vector-msdf-field.md` fixes.
    //
    // A masked instance whose field could not be resolved has a zeroed row, and
    // `field.resolved` is what says so — carried into `params2.w` for the
    // fragment stage, which is where it draws nothing (issue #972). **The
    // zeroed quad does not do that on its own**: it has no area, but the margin
    // below then grows it into a square two antialiasing widths across, and the
    // fragment stage was shading that square at exactly half coverage.
    //
    // A glyph whose atlas could not be resolved carries `run.resolved` through
    // the same component for the same reason (issue #993). Its quad is the
    // glyph's own rectangle and is not zeroed at all, so there was never
    // anything geometric to rely on: the fragment stage is the whole of it.
    var quad = inst.bounds;
    if inst.shape != 0u {
        let field = shapes[inst.shape - 1u];
        let lo = inst.bounds.xy + field.plane.xy;
        let hi = inst.bounds.xy + field.plane.zw;
        quad = vec4f(lo, hi - lo);
        out.params0 = quad;
        out.params1 = field.uv;
        out.params2 = vec4f(field.half_uv, field.px_range, f32(field.resolved));
    } else if inst.kind == KIND_TEXT {
        let run = glyph_runs[inst.row];
        out.params0 = run.color;
        // The glyph's own rectangle, from source texels into the atlas texture.
        // `corners` carries it in source texels because the packer has no
        // device and cannot know where residency put the atlas.
        out.params1 = vec4f(
            run.uv.xy + inst.corners.xy * run.uv.zw,
            inst.corners.zw * run.uv.zw,
        );
        out.params2 = vec4f(run.half_uv, run.px_range, f32(run.resolved));
    }

    // Grown by the antialiasing width so the ramp is not clipped by the
    // geometry it belongs to, and by whatever this instance draws beyond its
    // own bounds. An MSDF quad needs neither — the field's own padding is its
    // antialiasing — and the margin is harmless there because `msdf_sample`
    // clamps every sample back inside the payload.
    let margin = globals.aa + instance_outset(inst);
    let lo = quad.xy - vec2f(margin);
    let hi = quad.xy + quad.zw + vec2f(margin);
    // Two triangles, as a triangle strip of four vertices.
    let corner = vec2f(
        select(lo.x, hi.x, (vertex & 1u) == 1u),
        select(lo.y, hi.y, (vertex & 2u) == 2u),
    );
    // The node's rotation, applied to the quad's corners and to nothing else
    // (story #832).
    //
    // This is the whole of the change: `placed` feeds the clip-space position
    // below, and `out.local` keeps the *unrotated* `corner`. The fragment stage
    // evaluates every SDF against the interpolated `out.local`, in the node's
    // own axis-aligned frame, so it receives exactly the coordinates it
    // received before this story — no new per-pixel arithmetic, no branch, and
    // a rounded rect stays a true rounded rect rather than becoming an
    // axis-aligned approximation of one, which is what rotating the SDF's input
    // instead would produce.
    //
    // y-down and clockwise-positive, matching `dashscene-skia`'s
    // `canvas.rotate` and `Prop::Rotation`'s own convention
    // (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
    var placed = corner;
    if inst.rotation != 0.0 {
        let s = sin(inst.rotation);
        let c = cos(inst.rotation);
        let d = corner - inst.rotation_pivot;
        placed = inst.rotation_pivot + vec2f(d.x * c - d.y * s, d.x * s + d.y * c);
    }
    // Document space (y down, origin top-left) to clip space.
    let ndc = vec2f(
        placed.x / globals.size.x * 2.0 - 1.0,
        1.0 - placed.y / globals.size.y * 2.0,
    );
    out.position = vec4f(ndc, 0.0, 1.0);
    out.local = corner;
    out.placed = placed;
    out.bounds = inst.bounds;
    out.corners = inst.corners;
    out.rows = vec4u(inst.kind, inst.row, inst.clip_offset, inst.clip_count);
    out.opacity = inst.opacity;
    out.shape = inst.shape;
    return out;
}

// Coverage of the clip region this instance names: the intersection of its
// boxes. An empty range is unclipped, so the loop runs zero times and the
// coverage is one — the property a range has and a sentinel would not.
fn clip_coverage(offset: u32, count: u32, p: vec2f) -> f32 {
    var cover = 1.0;
    for (var i = 0u; i < count; i = i + 1u) {
        let b = clip_boxes[offset + i];
        let half_size = vec2f(b.w, b.h) * 0.5;
        let centre = vec2f(b.x, b.y) + half_size;
        let d = rounded_box_sdf(p - centre, half_size, b.corners);
        cover = min(cover, coverage(d, globals.aa));
    }
    return cover;
}

// One image fill's colour at document-space point `p`, or a fully transparent
// value where the fill paints nothing.
//
// The four scale modes are `dashscene-skia`'s, resolved here rather than
// re-derived: the reference painter is the specification for what each one
// means, the same posture the packer takes for draw order.
//
// - Fill and Fit both centre the image in the node's box at a uniform scale,
//   covering it (max) or contained by it (min). Under Fit the image does not
//   reach the box's edges, and what is outside it is *not painted* — the
//   reference draws the image into a destination rectangle rather than filling
//   the box, so the alpha there is zero rather than a clamped edge texel.
// - Crop maps the box's normalised coordinate through the fill's transform,
//   which is `uv_image = T · uv_box`, and clamps.
// - Tile repeats the image every `size * tile_scale` document units from the
//   box's own origin.
fn image_colour(fill: Image, bounds: vec4f, p: vec2f) -> vec4f {
    // A payload with no extent draws nothing.
    //
    // Boundary B stores an asset whose binding supplied no bytes at 0 x 0
    // rather than refusing it, because `dashscene-validator`'s image.no-bytes
    // rule is what names that case. Every branch below divides by `fill.size`,
    // and the Fill/Fit range check cannot catch the result: a NaN compares
    // false against everything, so the guard would pass it straight through to
    // the sampler with non-finite coordinates. Refused here instead, where
    // "no bytes" and "draws nothing" agree.
    if fill.size.x <= 0.0 || fill.size.y <= 0.0 {
        return vec4f(0.0);
    }
    let box_uv = (p - bounds.xy) / bounds.zw;
    var uv = vec2f(0.0);
    if fill.scale_mode == SCALE_TILE {
        uv = fract(box_uv * bounds.zw / (fill.size * fill.tile_scale));
    } else if fill.scale_mode == SCALE_CROP {
        uv = vec2f(
            fill.transform.x * box_uv.x + fill.transform.y * box_uv.y + fill.translate.x,
            fill.transform.z * box_uv.x + fill.transform.w * box_uv.y + fill.translate.y,
        );
        uv = clamp(uv, vec2f(0.0), vec2f(1.0));
    } else {
        let ratio = bounds.zw / fill.size;
        var scale = max(ratio.x, ratio.y);
        if fill.scale_mode == SCALE_FIT {
            scale = min(ratio.x, ratio.y);
        }
        let drawn = fill.size * scale;
        let origin = bounds.xy + (bounds.zw - drawn) * 0.5;
        uv = (p - origin) / drawn;
        // Outside the destination rectangle the reference paints nothing. Under
        // Fill the destination covers the box and this is unreachable; under Fit
        // it is the whole of the letterboxing.
        if any(uv < vec2f(0.0)) || any(uv > vec2f(1.0)) {
            return vec4f(0.0);
        }
    }
    // To the centre of a texel, and never past the payload's own last one: the
    // allocation beside this one in the atlas is a different image, and there is
    // no padding between them. Sampling is nearest, so this is the whole of what
    // keeps a neighbour out of the picture.
    let half_texel = vec2f(0.5) / fill.size;
    let inside = clamp(uv, half_texel, vec2f(1.0) - half_texel);
    return textureSampleLevel(atlas, atlas_sampler, fill.uv.xy + inside * fill.uv.zw, 0.0);
}

// One gradient fill's colour at document-space point `p`.
//
// # The frame is resolved here, not on the CPU
//
// `dashpaint::Gradient` carries three handles normalized to the node's box —
// Figma's `gradientHandlePositions` — and a gradient row is *interned*, so one
// row is shared by every node that authored the same gradient. The box is per
// instance, so the box-to-document mapping cannot live on the row. Resolving it
// here is also what P1 asks for: the document carries the intent and the
// painter resolves the geometry.
//
// `bounds` is the node's own box even for a masked instance, whose quad is the
// coverage field's plane instead — a gradient's frame is stated over the node
// box however the mask confines where it lands, which is what `dashscene-skia`
// does. `VertexOut.bounds` is documented against exactly this.
//
// # A degenerate frame takes the first stop
//
// `gradient_local` returns (0, 0) when the three handles enclose no area, so
// every kind reports t = 0 and the ramp clamps to the first stop. That is the
// reference painter's own fallback for a frame it cannot invert, reached here
// by the same arithmetic rather than by a second branch.
fn gradient_colour(row: u32, bounds: vec4f, p: vec2f) -> vec4f {
    let base = globals.gradient_base + row * GRADIENT_WORDS;
    let handles = paints[base];
    let frame = paints[base + 1u];
    let origin = bounds.xy + handles.xy * bounds.zw;
    let primary = bounds.xy + handles.zw * bounds.zw;
    let secondary = bounds.xy + frame.xy * bounds.zw;

    let kind = u32(frame.z);
    // Clamped to the heap row's own slot count. The Rust side asserts the same
    // bound before it writes the row, so this can only differ if the two ever
    // disagree — and a loop that walked past the row would read the *next*
    // gradient's handles as stops.
    let count = min(u32(frame.w), MAX_GRADIENT_STOPS);

    var t = 0.0;
    if kind == GRADIENT_RADIAL {
        t = gradient_radial_t(p, origin, primary, secondary);
    } else if kind == GRADIENT_ANGULAR {
        t = gradient_angular_t(p, origin, primary, secondary);
    } else if kind == GRADIENT_DIAMOND {
        t = gradient_diamond_t(p, origin, primary, secondary);
    } else {
        // GRADIENT_LINEAR, and nothing else: `gradient_kind` on the Rust side
        // maps an exhaustive match over `GradientKind`, so no fifth value can
        // arrive here. Stated as the fall-through rather than as a fourth
        // branch so that this function stays total whatever a future kind does
        // before it is taught to this file — an unknown kind then draws the
        // linear ramp, which is a wrong picture rather than an undefined one.
        t = gradient_linear_t(p, origin, primary, secondary);
    }

    // The eight offset slots are two whole words, so they are read
    // unconditionally; the colours are read only as far as the count, because
    // that is where the loop can be bounded without a branch per slot.
    let lo = paints[base + 2u];
    let hi = paints[base + 3u];
    let offsets = array<f32, MAX_GRADIENT_STOPS>(
        lo.x, lo.y, lo.z, lo.w,
        hi.x, hi.y, hi.z, hi.w,
    );
    var colours: array<vec4f, MAX_GRADIENT_STOPS>;
    for (var i = 0u; i < count; i = i + 1u) {
        colours[i] = paints[base + 4u + i];
    }
    return gradient_ramp(t, offsets, colours, count);
}

// One shadow's parameters, as the paint heap carries them.
struct Shadow {
    offset: vec2f,
    sigma: f32,
    spread: f32,
    color: vec4f,
}

// Shadow row `row` of the heap's shadow region.
fn shadow_row(row: u32) -> Shadow {
    let base = globals.shadow_base + row * SHADOW_WORDS;
    let geometry = paints[base];
    var out: Shadow;
    out.offset = geometry.xy;
    out.sigma = geometry.z;
    out.spread = geometry.w;
    out.color = paints[base + 1u];
    return out;
}

// Per-corner radii adjusted by a spread delta: a corner grows with a positive
// spread (a drop shadow) and shrinks with a negative one (an inner shadow's lit
// hole), floored at zero. **A sharp corner stays sharp** — CSS's spread rule,
// and `dashscene-skia`'s `spread_corners`, which this is the transliteration
// of.
//
// In `paint.wgsl` rather than in the shared `sdf.wgsl`: the library is the
// distance math a second painter ports and layer 2 evaluates, and this is the
// parameter arithmetic that decides which box to hand it. `blurred_rounded_box`
// is the part that had to be single-sourced, and it is.
fn spread_corners(corners: vec4f, delta: f32) -> vec4f {
    return select(
        vec4f(0.0),
        max(corners + vec4f(delta), vec4f(0.0)),
        corners > vec4f(0.0),
    );
}

// A drop shadow's coverage at `p`: the silhouette grown by the spread, moved by
// the offset, and blurred.
//
// `bounds` is the instance's own box, which the packer already grew by the
// stroke outset — a drop shadow casts from what the node draws, and the stroke
// row is the one term of that geometry no shadow row carries. The spread and
// the offset are on the row and resolved here, which is the split
// `Instance::bounds` documents.
//
// Built as an origin and a size rather than as a centre and a half-extent, so
// that a spread negative enough to collapse the box collapses it exactly where
// `dashscene-skia` does: it clamps the *size* at zero and keeps the origin,
// where clamping a half-extent would leave the box centred somewhere else.
fn shadow_drop_coverage(s: Shadow, bounds: vec4f, corners: vec4f, p: vec2f) -> f32 {
    let size = max(bounds.zw + vec2f(2.0 * s.spread), vec2f(0.0));
    let origin = bounds.xy - vec2f(s.spread) + s.offset;
    let half_size = size * 0.5;
    return blurred_rounded_box(
        p - (origin + half_size),
        half_size,
        spread_corners(corners, s.spread),
        s.sigma,
    );
}

// An inner shadow's coverage at `p`: the node's own shape, minus a blurred hole
// inset by the spread and moved by the offset, so the blur bleeds inward from
// the shape's edge.
//
// `d` is the signed distance to the node's own rounded box, which `fs_main` has
// already computed — an inner shadow takes no stroke outset, so the instance's
// bounds are the node's box exactly and that distance is the shape this shadow
// is clipped to.
//
// **The complement is the whole of the effect.** `dashscene-skia` fills an
// even-odd path — an outer rectangle minus the hole — under a blur mask and
// clips it to the shape, and it sizes that rectangle to clear the blur's reach
// so the shadow saturates at the shape's edge rather than fading in from the
// rectangle's own boundary. `1 - blurred(hole)` is that construction with the
// rectangle taken to infinity, which is what its margin approximates.
fn shadow_inner_coverage(s: Shadow, bounds: vec4f, corners: vec4f, d: f32, p: vec2f) -> f32 {
    let size = max(bounds.zw - vec2f(2.0 * s.spread), vec2f(0.0));
    let origin = bounds.xy + vec2f(s.spread) + s.offset;
    let half_size = size * 0.5;
    let hole = blurred_rounded_box(
        p - (origin + half_size),
        half_size,
        spread_corners(corners, -s.spread),
        s.sigma,
    );
    return coverage(d, globals.aa) * (1.0 - hole);
}

// The MSDF channels under `p`, for a quad that maps onto one sub-rect of the
// bound atlas.
//
// `rect` is that sub-rect, normalised: `[u0, v0, du, dv]`. `quad` is the device
// rectangle it covers, `[x, y, w, h]`. `half` is half a source texel in the
// same normalised units.
//
// **The clamp is what keeps the sample inside this payload**, and it is doing
// two jobs. The allocation beside this one in the residency atlas is a
// different picture with no padding between them, so a sample that walked out
// of the sub-rect would read it — and the quad is deliberately grown by the
// antialiasing width, so without the clamp every glyph would read its
// neighbour along a one-unit fringe. Half a texel in is also exactly what makes
// *filtering* safe: a bilinear footprint taken from a texel's own centre
// weights that texel alone at the payload's edge, so no gutter is needed.
//
// `min`/`max` around the bounds rather than the bounds themselves: a sub-rect
// under two texels wide has `lo` past `hi`, and `clamp` with a reversed range
// is not defined to do anything sensible.
fn msdf_sample(rect: vec4f, quad: vec4f, half: vec2f, p: vec2f) -> vec3f {
    let t = (p - quad.xy) / quad.zw;
    let lo = rect.xy + half;
    let hi = rect.xy + rect.zw - half;
    let uv = clamp(rect.xy + t * rect.zw, min(lo, hi), max(lo, hi));
    return textureSampleLevel(atlas, msdf_sampler, uv, 0.0).rgb;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    let kind = in.rows.x;
    let row = in.rows.y;
    let half_size = in.bounds.zw * 0.5;
    let centre = in.bounds.xy + half_size;
    let d = rounded_box_sdf(in.local - centre, half_size, in.corners);

    // Read once, for the two kinds that need it, rather than in both of the
    // chains below — the coverage and the colour of a shadow come from the same
    // row, and every other kind would pay for a load it does not read.
    var shadow: Shadow;
    let is_shadow = kind == KIND_SHADOW_DROP || kind == KIND_SHADOW_INNER;
    if is_shadow {
        shadow = shadow_row(row);
    }

    // Coverage is per kind, and cannot be computed before the kind is known: a
    // fill covers the shape's interior, and a stroke covers a band that an
    // Outside stroke puts entirely *outside* it. Taking the fill's ramp first
    // and multiplying would have left every Outside stroke at zero coverage
    // everywhere it draws.
    var shape = 0.0;
    if in.shape != 0u {
        // A baked-vector node carries its outline in the baked geometry, so the
        // parametric rounded box and its corners do not apply at all — the
        // field's coverage *is* the node's silhouette. `dashscene-skia` says
        // the same thing by skipping the whole parametric branch for a masked
        // entry, and the packer already emits neither a stroke nor a stacked
        // layer for one.
        //
        // Checked before the kind, because a mask applies to whatever it is
        // masking. Its own colour still comes from the kind below.
        //
        // A field this frame did not make resident leaves the coverage at zero,
        // so the `discard` below takes this fragment (issue #972). Sampling it
        // would not: the row is zeroed, so `px_range` is zero, and
        // `msdf_coverage` is then `0.5` whatever the sample was — the node's
        // own ink at half alpha, over the margin, in a picture that is meant to
        // be empty.
        if in.params2.w != 0.0 {
            shape = msdf_coverage(
                msdf_sample(in.params1, in.params0, in.params2.xy, in.local),
                in.params2.z,
            );
        }
    } else if kind == KIND_TEXT {
        // A glyph whose atlas this frame did not make resident leaves the
        // coverage at zero, so the `discard` below takes this fragment (issue
        // #993). The same gate as the masked arm above, and it is needed for
        // the same reason: the row is zeroed, so `px_range` is zero and
        // `msdf_coverage` is then `0.5` whatever the sample was.
        //
        // **It is the gate and not the colour that draws nothing here.** The
        // row's own zero alpha did that before this existed, which made an
        // empty frame the agreement of two defaults in two files rather than a
        // decision anything states — and unlike `KIND_FILL_IMAGE`, the colour
        // arm below has no `discard` of its own behind it.
        if in.params2.w != 0.0 {
            // The glyph's quad is its own bounds — a glyph has no rounded box,
            // and `corners` carried its atlas rectangle instead.
            shape = msdf_coverage(
                msdf_sample(in.params1, in.bounds, in.params2.xy, in.local),
                in.params2.z,
            );
        }
    } else if kind == KIND_STROKE {
        let s = strokes[row];
        shape = stroke_coverage(d, s.width, f32(s.align), globals.aa);
    } else if kind == KIND_SHADOW_DROP {
        shape = shadow_drop_coverage(shadow, in.bounds, in.corners, in.local);
    } else if kind == KIND_SHADOW_INNER {
        shape = shadow_inner_coverage(shadow, in.bounds, in.corners, d, in.local);
    } else {
        shape = coverage(d, globals.aa);
    }
    // `placed`, not `local`: a clip box belongs to an ancestor and stays
    // axis-aligned in document space while this node turns (story #832). The
    // two are the same value for an unrotated instance.
    var cover = shape * clip_coverage(in.rows.z, in.rows.w, in.placed) * in.opacity;
    if cover <= 0.0 {
        discard;
    }
    // Story #580 drew the solid fill; story #710 added the stroke beside it,
    // story #581 the image fill, story #582 text and the coverage mask above,
    // issue #715 the gradient, and story #584 the two shadow kinds. Group
    // opacity is #583's and composites elsewhere. **The backdrop blur reaches
    // the `discard` below and is drawn anyway**: story #733 gave it its own two
    // pipelines, because it has to read what is already in the render target
    // and no binding here can do that. `composite::plan` keeps it out of every
    // instance range, so in a correct frame this shader never sees one — and
    // the `discard` is what keeps a frame that somehow does from painting it.
    //
    // A masked *gradient* fill now draws too, and it is the one combination
    // that needed both halves: story #582 resolved the mask and its coverage,
    // and this is the colour it modulates.
    //
    // **Both `kind` and `tag`, never `tag` alone.** `tag` means a different
    // enum for each kind — a `PaintTag` for a fill, a `ShadowKind` for a
    // shadow, a `BlurKind` for a backdrop — and their discriminants collide:
    // `PaintTag::Solid`, `ShadowKind::Inner` and `BlurKind::Backdrop` are all
    // 1. Reading `tag` alone made a shadow instance paint the solid table's
    // `row` with `row` indexing the *shadow* table, so a node with an inner
    // shadow drew whatever colour happened to sit at that row, over its own
    // fill.
    //
    // A kind this shader does not draw draws *nothing*, and does not fall
    // through to a colour. Painting it black would be loud, but it would also
    // corrupt every node that carries one: an inner shadow is packed after the
    // fill, so a black shadow instance covers the fill it belongs to. Drawing
    // nothing leaves the picture correct for the subset this shader draws and
    // absent for the rest.
    //
    // Since story #733 the only kind that reaches it is the backdrop, which is
    // drawn by another pipeline entirely and is kept out of every instance
    // range by `composite::plan`. The arm stays because "packed but not drawn"
    // was only ever safe while this fall-through discarded: a masked node drew
    // as a plain rounded rectangle from story #578 until #582 because the
    // fragment stage ignored a field the packer had set.
    //
    // Not a silent drop: the packer emits the instance, the layer-1 golden
    // shows it, and `docs/decisions/pipelines-and-layer-3.md` lists what is and
    // is not drawn. What is refused here is a *wrong* pixel, not a diagnostic.
    var colour = vec4f(0.0);
    if is_shadow {
        // Both kinds take the row's colour; the coverage above is what makes
        // one fall behind the node and the other sit inside it.
        colour = shadow.color;
    } else if kind == KIND_FILL_SOLID {
        // The solid region is at the head of the heap, so a solid fill's row is
        // still its word — this path pays nothing for the gradient region
        // behind it.
        colour = paints[row];
    } else if kind == KIND_FILL_GRADIENT {
        colour = gradient_colour(row, in.bounds, in.local);
    } else if kind == KIND_STROKE {
        colour = strokes[row].color;
    } else if kind == KIND_TEXT {
        // The run's fill, which the MSDF coverage above modulates. The run's
        // own free-path alpha is on `opacity` and is already in `cover`.
        colour = in.params0;
    } else if kind == KIND_FILL_IMAGE {
        colour = image_colour(images[row], in.bounds, in.local);
        // A payload's own transparency, and the letterboxing Fit leaves. Both
        // reach here as a zero alpha, and discarding keeps an image fill from
        // writing transparent black over what it was composited onto.
        if colour.a <= 0.0 {
            discard;
        }
    } else {
        discard;
    }
    // Premultiplied, which is what the blend state below expects.
    let a = colour.a * cover;
    return vec4f(colour.rgb * a, a);
}
