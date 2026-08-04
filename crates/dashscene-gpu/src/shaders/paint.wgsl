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
    _pad: f32,
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
    _pad: f32,
}

@group(0) @binding(0) var<storage, read> instances: array<Instance>;
// The paint-parameter heap: the fill parameters the fragment stage reads, as
// one array of words. The solid colours come first, so a solid fill's colour is
// still `paints[row]`; the gradient rows follow at `globals.gradient_base`.
//
// One binding for two tables because there was no second one to take: the
// fragment stage reads the four storage buffers `downlevel_defaults` allows,
// and a gradient's stop array is indexed by a value this stage computes, so it
// cannot cross as a varying the way story #582's tables do.
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
    //     masked        params0 = the field's device quad, [x, y, w, h]
    //                   params1 = the field's sub-rect in the atlas texture
    //     everything    zero, and nothing reads them
    //
    // `params2` is `(half_u, half_v, px_range, 0)` for both.
    //
    // Flat, and stated: these are per-instance constants, and a flat varying is
    // exact where an interpolated one is only exact because every vertex agreed.
    @location(6) @interpolate(flat) params0: vec4f,
    @location(7) @interpolate(flat) params1: vec4f,
    @location(8) @interpolate(flat) params2: vec4f,
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

    var out: VertexOut;
    out.params0 = vec4f(0.0);
    out.params1 = vec4f(0.0);
    out.params2 = vec4f(0.0);

    // The rectangle this instance's ink lives in. The node's own box for
    // everything but a masked instance, whose ink is confined to the coverage
    // field's padded quad — `plane` is relative to the node's origin, at unit
    // scale, which is what `docs/decisions/baked-vector-msdf-field.md` fixes.
    //
    // A masked instance whose field could not be resolved has a zeroed row, so
    // its quad has no area and it draws nothing. That is the reference
    // painter's own answer to a degenerate field, reached here without a
    // second branch.
    var quad = inst.bounds;
    if inst.shape != 0u {
        let field = shapes[inst.shape - 1u];
        let lo = inst.bounds.xy + field.plane.xy;
        let hi = inst.bounds.xy + field.plane.zw;
        quad = vec4f(lo, hi - lo);
        out.params0 = quad;
        out.params1 = field.uv;
        out.params2 = vec4f(field.half_uv, field.px_range, 0.0);
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
        out.params2 = vec4f(run.half_uv, run.px_range, 0.0);
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
    // Document space (y down, origin top-left) to clip space.
    let ndc = vec2f(
        corner.x / globals.size.x * 2.0 - 1.0,
        1.0 - corner.y / globals.size.y * 2.0,
    );
    out.position = vec4f(ndc, 0.0, 1.0);
    out.local = corner;
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
        shape = msdf_coverage(
            msdf_sample(in.params1, in.params0, in.params2.xy, in.local),
            in.params2.z,
        );
    } else if kind == KIND_TEXT {
        // The glyph's quad is its own bounds — a glyph has no rounded box, and
        // `corners` carried its atlas rectangle instead.
        shape = msdf_coverage(
            msdf_sample(in.params1, in.bounds, in.params2.xy, in.local),
            in.params2.z,
        );
    } else if kind == KIND_STROKE {
        let s = strokes[row];
        shape = stroke_coverage(d, s.width, f32(s.align), globals.aa);
    } else {
        shape = coverage(d, globals.aa);
    }
    var cover = shape * clip_coverage(in.rows.z, in.rows.w, in.local) * in.opacity;
    if cover <= 0.0 {
        discard;
    }
    // Story #580 drew the solid fill; story #710 added the stroke beside it,
    // story #581 the image fill, story #582 text and the coverage mask above,
    // and issue #715 the gradient. Shadows and backdrop blur are #584's, group
    // opacity #583's.
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
    if kind == KIND_FILL_SOLID {
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
