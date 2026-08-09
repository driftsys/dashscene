// The backdrop blur: replaces the region a node covers with a blurred copy of
// everything already composited beneath it (story #733).
//
// The device-aligned counterpart of `dashscene-skia`'s `draw_backdrop_blur_box`
// and `draw_backdrop_blur_field`. Those get the blur from Skia natively, as a
// `SaveLayerRec` carrying a backdrop `ImageFilter`; there is no such thing here,
// so the kernel is written out.
//
// # Why this is its own module, and its own two pipelines
//
// A texture cannot be a render attachment and a sampled binding in the same
// pass, so a backdrop cannot be drawn by the paint pipeline at all: it has to
// read the target it is drawing into. `composite::plan` therefore ends the
// pass at a backdrop instance, and the renderer snapshots the target between
// the two passes. This module samples that snapshot.
//
// A pipeline owns its `@group(0)`, so a separate module costs the paint
// pipeline nothing — the route story #583 established for the composite and
// `docs/decisions/pipelines-and-layer-3.md` D1 anticipates. It is also the only
// route available: the blend state below is per-pipeline, and it is not the
// paint pipeline's.
//
// # The blend is a replacement, not a source-over
//
// `dashscene-skia`'s `backdrop_layer_paint` composites the blurred layer with
// `BlendMode::Src` at opacity 1 — the blurred copy *replaces* the region rather
// than being blended over it, which is what a backdrop filter means. Source-over
// is indistinguishable from replacement only where the backdrop is opaque, and
// wrong everywhere else: the blur's alpha falloff is lost and its alpha edge
// stays hard (debt #405).
//
// A replacement cannot be expressed in the blend state alone. The lerp the
// coverage needs is `dst * (1 - cover) + blurred * cover`, and the colour half
// of that is reachable — source factor one, destination factor
// one-minus-source-alpha, with `cover` written to alpha — but the alpha half
// then needs `src.a` to be two different values at once, `cover` for the
// destination factor and `blurred.a * cover` for its own contribution. So the
// resolve pass samples the sharp destination itself, does the whole arithmetic
// here, and the pipeline writes the answer with no blending at all.
//
// # The kernel is separable, and clamped
//
// A Gaussian is separable, so the two-dimensional kernel is two one-dimensional
// passes: 2n+1 taps each rather than (2n+1)². At the showcase's own frost panel
// — `backdrop_blur(24.0 * unit)`, so a sigma of 10.5 and a support of 32 — that
// is 130 taps against 4225.
//
// Taps clamp to the target's edge, which is `TileMode::Clamp` in
// `backdrop_blur_filter` and the rule CSS's `backdrop-filter` specifies for the
// same reason: past the edge there is no backdrop to read, and extending the
// edge pixel is what keeps a frosted node at the frame's border from darkening.
//
// **The clamp is written out even though one backend already does it**, and
// that is deliberate rather than redundant. `wgpu-hal`'s Metal backend sets
// naga's `image_load` bounds-check policy to `Restrict` whenever runtime checks
// are on, which clamps the coordinate for us — so on that backend, and only
// there, deleting the `clamp` below changes no pixel and no test in this crate
// can tell. It is not portable: the policy is `Unchecked` with runtime checks
// off, and the GLES backend can choose `ReadZeroSkipWrite`, which returns
// transparent black and darkens exactly the edge this exists to protect. The
// tap range is this shader's own contract, not a wgpu configuration's.
//
// # And it averages sRGB-encoded values
//
// `dashpaint::Blur` makes this a term of boundary B rather than a per-painter
// choice: two painters blurring in different spaces disagree by roughly 50 code
// points across a saturated seam. It needs no code here — `TARGET_FORMAT` is
// `Rgba8Unorm` and `surface.rs` refuses any surface format that sRGB-converts
// on write, so a texel *is* the encoded value and averaging texels averages in
// the encoded space. The same one allocation that keeps MSDF distance channels
// sampling raw.

struct Blur {
    // The node's silhouette in document space: x, y, w, h. Document space is
    // pixel space — the painter draws at unit scale — so this is also its texel
    // rectangle.
    bounds: vec4f,
    // The rounded-box radii, [top_left, top_right, bottom_right, bottom_left].
    corners: vec4f,
    // The quad this pass draws, in the same space: x, y, w, h. Not derived from
    // `bounds` in here, because the two passes want different ones — the
    // horizontal pass has to cover every texel the vertical pass will read, so
    // it is dilated by the support **in y alone**, and the vertical pass writes
    // the node's own silhouette.
    //
    // In y alone, and not on both axes: the horizontal pass reads the snapshot,
    // which is the whole target, so its own taps need no margin. What needs one
    // is the vertical pass reading the horizontal pass's *output* over ± the
    // support, since a row that pass never wrote is transparent.
    // `render::clamped_quad` takes the pad per axis for exactly this.
    quad: vec4f,
    // The coverage mask that confines this backdrop, when `masked` is set —
    // `plane`, `uv`, `half_uv` and `px_range` together.
    //
    // A baked-vector node's blur follows the field's outline rather than a box.
    // `dashscene-skia`'s `draw_backdrop_blur_field` is the same case, and the
    // hero's frosted panel is exactly that node: a Figma VECTOR carrying
    // `BACKGROUND_BLUR`. All four are `render::GpuShape`'s members, which is
    // where they are derived and documented; they arrive on this uniform rather
    // than through a table because a backdrop is one draw, so a table would be
    // a binding to read one row of.
    plane: vec4f,
    uv: vec4f,
    // The target's extent in texels, for the clip-space map and the tap clamp.
    size: vec2f,
    // One tap's step: (1, 0) horizontally, (0, 1) vertically. A vector rather
    // than a flag so the loop below has no branch in it.
    step: vec2f,
    half_uv: vec2f,
    // The Gaussian sigma in texels — `radius * dashpaint::BLUR_SIGMA_PER_RADIUS`,
    // applied once on the Rust side by `pack::blur_sigma`, the way every other
    // blur in this painter takes it.
    sigma: f32,
    // Half the kernel's width in texels: `ceil(3 * sigma)`, which is where a
    // Gaussian's weight has fallen to about 1.1 % of its peak.
    support: f32,
    // The node's free-path alpha — `dashpaint::RectEntry::opacity`, carried
    // through unchanged, and the value that decides which of
    // `backdrop_layer_paint`'s two blend modes this fragment reproduces.
    opacity: f32,
    // The antialiasing width, the same `globals.aa` the paint pipeline ramps
    // its edges over. Stated here rather than shared, because this pipeline
    // binds no `Globals`.
    aa: f32,
    px_range: f32,
    // Non-zero when the four mask members describe one. Not inferred from any
    // of them: a zero `px_range` is a degenerate field rather than an absent
    // one, and inferring absence from a value a real field could take is how a
    // sentinel goes wrong.
    masked: u32,
    // The clip region this backdrop is confined to, as a range into
    // `clip_boxes`. A backdrop is clipped exactly as the node's fill is: it is
    // the node's own ink, drawn beneath it.
    clip_offset: u32,
    clip_count: u32,
    // The point the node turns about, in document space, and the angle it turns
    // by, in radians (story #832). Zero is unrotated.
    //
    // The pivot is first and lands at an eight-aligned offset, which is what
    // makes WGSL's `vec2f` alignment agree with the Rust type — the trap
    // `Instance` documents against its own `rotation_pivot`.
    rotation_pivot: vec2f,
    rotation: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// The member order above is `render::GpuBlur`'s, exactly, and it has to be:
// the two are one layout declared twice in two languages, and nothing in
// either one holds them together. `render.rs` asserts every offset for that
// reason — a size assertion alone does not pin a layout, and this struct is
// the proof. Reordering the mask members here without reordering them there
// left both at 144 bytes and moved eight of the eighteen offsets, which made
// every backdrop read its sigma out of a texture coordinate.

struct ClipBox {
    x: f32, y: f32, w: f32, h: f32,
    corners: vec4f,
}

// The texture this pass's taps read. The snapshot of the target for the
// horizontal pass; the horizontal pass's own output for the vertical one.
@group(0) @binding(0) var source: texture_2d<f32>;
// The snapshot, always — the destination as it stood before this backdrop, read
// at the fragment's own texel and at no other. Bound for both passes because
// one bind group layout serves both; only `fs_blur_resolve` reads it.
@group(0) @binding(1) var sharp: texture_2d<f32>;
@group(0) @binding(2) var<uniform> blur: Blur;
@group(0) @binding(3) var<storage, read> clip_boxes: array<ClipBox>;
// The residency atlas the coverage mask lives in, and the sampler a distance
// field is read through — linear, for the reason `render.rs` gives where it
// builds it. Bound for every backdrop, and a frame whose backdrop carries no
// mask names the placeholder, exactly as the paint pipeline does.
@group(0) @binding(4) var atlas: texture_2d<f32>;
@group(0) @binding(5) var msdf_sampler: sampler;

// The pass's quad, from the vertex index alone, indexed the way `paint.wgsl`'s
// `vs_main` and `composite.wgsl`'s `vs_composite` index theirs — bit 0 selects
// x and bit 1 selects y, in document space, with y flipped on the way to clip
// space.
@vertex
fn vs_blur(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let corner = vec2f(
        f32(vertex & 1u),
        f32((vertex >> 1u) & 1u),
    );
    let p = blur.quad.xy + corner * blur.quad.zw;
    let ndc = vec2f(
        p.x / blur.size.x * 2.0 - 1.0,
        1.0 - p.y / blur.size.y * 2.0,
    );
    return vec4f(ndc, 0.0, 1.0);
}

// One axis of the separable Gaussian at `texel`, normalised by the weight it
// actually summed rather than by the continuous integral `sigma * sqrt(2 pi)`.
//
// **Not because of the clamp.** Every fragment sums exactly `2 * support + 1`
// taps whatever it sits on — the clamp moves a tap's coordinate and never drops
// it — so the summed weight is one number per sigma rather than a per-position
// one. The reason is that the number is not the integral: a Gaussian sampled at
// integers and truncated at three sigma sums to the integral only while sigma is
// large enough for the two to converge. Below about half a texel they part
// company — at the sigma a radius of 1 maps to, by 4.6 % — and a kernel that
// does not sum to one scales everything it touches, alpha included.
//
// That scaling is invisible against an opaque backdrop, because the readback
// divides the colour by the alpha and both moved by the same factor. Against a
// partially transparent one it is not, which is what
// `blurring_a_uniform_backdrop_changes_nothing_even_at_the_targets_edge` states
// it over.
fn gaussian(texel: vec2i) -> vec4f {
    let support = i32(blur.support);
    // Negated and folded here so the loop multiplies rather than divides.
    let scale = -0.5 / (blur.sigma * blur.sigma);
    let step = vec2i(blur.step);
    let limit = vec2i(blur.size) - vec2i(1, 1);
    var sum = vec4f(0.0);
    var total = 0.0;
    for (var i = -support; i <= support; i = i + 1) {
        let weight = exp(f32(i * i) * scale);
        let at = clamp(texel + step * i, vec2i(0, 0), limit);
        sum = sum + textureLoad(source, at, 0) * weight;
        total = total + weight;
    }
    return sum / total;
}

// The first pass: the horizontal half of the kernel, over the dilated quad.
@fragment
fn fs_blur_axis(@builtin(position) position: vec4f) -> @location(0) vec4f {
    return gaussian(vec2i(position.xy));
}

// The second pass: the vertical half, and the composite that puts it down.
//
// Writes the finished destination — the pipeline blends nothing — so everything
// this fragment does not mean to change it has to write back unchanged. Outside
// the node's shape the coverage is zero and the answer is the sharp texel it
// just read, which is what makes writing the whole quad harmless.
@fragment
fn fs_blur_resolve(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let texel = vec2i(position.xy);
    let blurred = gaussian(texel);
    // Document space is pixel space, so the fragment's own position is the
    // point the node's shape is stated over — once it is turned back into the
    // node's own frame.
    //
    // The node's shape is stated unrotated, in `bounds`/`plane`, exactly as it
    // is for the node's fill; turning the fragment back by the node's own angle
    // is what makes a rotated frosted region follow the node (story #832). It
    // is the inverse of what `paint.wgsl`'s vertex stage does to the quad, and
    // for the same reason: the mask turns, the sampling does not.
    //
    // `p` alone, and never `texel`: the Gaussian below reads the neighbourhood
    // in *screen* space, which does not turn. So does the clip, which belongs
    // to an ancestor.
    let screen = position.xy;
    var p = screen;
    if blur.rotation != 0.0 {
        let s = sin(-blur.rotation);
        let c = cos(-blur.rotation);
        let d = screen - blur.rotation_pivot;
        p = blur.rotation_pivot + vec2f(d.x * c - d.y * s, d.x * s + d.y * c);
    }
    let sharp_texel = textureLoad(sharp, texel, 0);

    // A baked-vector node carries its outline in the baked geometry, so the
    // parametric rounded box and its corners do not apply at all — the field's
    // coverage *is* the node's silhouette. `paint.wgsl`'s `fs_main` says the
    // same thing for the node's fill, and this is the same node.
    var shape = 0.0;
    if blur.masked != 0u {
        // The field's device quad is the node's origin plus its padded plane,
        // at unit scale — `GpuShape::plane` is node-relative and y-down, and
        // this is where it becomes absolute.
        let quad = vec4f(
            blur.bounds.xy + blur.plane.xy,
            blur.plane.zw - blur.plane.xy,
        );
        shape = msdf_coverage(msdf_sample(blur.uv, quad, blur.half_uv, p), blur.px_range);
    } else {
        let half_size = blur.bounds.zw * 0.5;
        let centre = blur.bounds.xy + half_size;
        let d = rounded_box_sdf(p - centre, half_size, blur.corners);
        shape = coverage(d, blur.aa);
    }
    // The clip is stated by an ancestor and does not turn, so it is tested
    // against the unturned screen position rather than against `p`.
    let cover = shape * clip_coverage(screen);

    // `backdrop_layer_paint`'s two modes, and the discontinuity between them is
    // the reference painter's own: at full opacity the blurred copy replaces
    // the region, and below it the copy composites over the sharp original so a
    // dimmed node frosts proportionally. Those disagree wherever the backdrop
    // is not opaque, which is why the branch exists there and here.
    var over = blurred;
    if blur.opacity < 1.0 {
        // Premultiplied, so scaling by the alpha is the whole of "draw this at
        // that alpha", and the source-over that puts it down is written out
        // because this pipeline does not blend.
        let src = blurred * blur.opacity;
        over = src + sharp_texel * (1.0 - src.a);
    }
    return mix(sharp_texel, over, cover);
}

// The MSDF channels under `p` — `paint.wgsl`'s `msdf_sample`, over this
// pipeline's own atlas binding, and stated identically because the two are
// sampling the same payload for the same node. See that file for what the
// clamp is doing and why filtering needs no gutter behind it.
fn msdf_sample(rect: vec4f, quad: vec4f, half: vec2f, p: vec2f) -> vec3f {
    let t = (p - quad.xy) / quad.zw;
    let lo = rect.xy + half;
    let hi = rect.xy + rect.zw - half;
    let uv = clamp(rect.xy + t * rect.zw, min(lo, hi), max(lo, hi));
    return textureSampleLevel(atlas, msdf_sampler, uv, 0.0).rgb;
}

// Coverage of the clip region this backdrop names — `paint.wgsl`'s
// `clip_coverage` over this pipeline's own uniform. An empty range is
// unclipped, so the loop runs zero times and the coverage is one.
fn clip_coverage(p: vec2f) -> f32 {
    var cover = 1.0;
    for (var i = 0u; i < blur.clip_count; i = i + 1u) {
        let b = clip_boxes[blur.clip_offset + i];
        let half_size = vec2f(b.w, b.h) * 0.5;
        let centre = vec2f(b.x, b.y) + half_size;
        let d = rounded_box_sdf(p - centre, half_size, b.corners);
        cover = min(cover, coverage(d, blur.aa));
    }
    return cover;
}
