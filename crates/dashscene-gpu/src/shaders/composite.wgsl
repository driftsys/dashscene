// Blends one render-target group's layer into the target around it, at the
// group's alpha (story #583).
//
// The device-aligned counterpart of `dashscene-skia`'s `blend_layer`: one
// source-over draw of the layer at the origin, modulated by the same alpha.
// It is a 1:1 pixel copy — there is no geometry to smooth and no scale to
// filter — so it samples by `textureLoad` and declares no sampler at all,
// which is also why it needs none of `sdf.wgsl`.
//
// Its own module rather than another entry point in `paint.wgsl`: a pipeline
// owns `@group(0)`, and binding 0 there is the instance array. Two
// declarations of one binding slot in one module is a conflict, not a choice.
// `shader-library-and-layer-2.md` D1 anticipates this — the library is the
// math, and each pipeline concatenates its own entry points.

struct Composite {
    // The group's composite alpha, in [0, 1].
    alpha: f32,
    // Declared, and always zero. A uniform buffer's binding size rounds up to
    // 16 bytes whatever its members add to, so the three words exist either
    // way; naming them as scalars is what keeps this struct 16 bytes and the
    // Rust type the same size by construction. A `vec3f` here would *not* —
    // it aligns to 16, which puts it at offset 16 and makes the struct 32.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var layer: texture_2d<f32>;
@group(0) @binding(1) var<uniform> composite: Composite;

// The full-target quad, from the vertex index alone. Four vertices as a
// triangle strip, indexed the way `paint.wgsl`'s `vs_main` indexes its corners
// — bit 0 selects x and bit 1 selects y, in document space, with y flipped on
// the way to clip space. Nothing depends on the winding (neither pipeline
// culls, and `fs_composite` reads the fragment's own position rather than an
// interpolated coordinate), but two quads in one painter disagreeing about
// which bit is which is a trap with no upside.
@vertex
fn vs_composite(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let x = f32(vertex & 1u) * 2.0 - 1.0;
    let y = 1.0 - f32((vertex >> 1u) & 1u) * 2.0;
    return vec4f(x, y, 0.0, 1.0);
}

@fragment
fn fs_composite(@builtin(position) position: vec4f) -> @location(0) vec4f {
    // The layer is the target's own extent, so the fragment's pixel coordinate
    // is the texel — no transform, and no sampling outside the texture.
    let texel = textureLoad(layer, vec2i(position.xy), 0);
    // The layer was drawn with premultiplied blending, so its texels are
    // premultiplied. Scaling a premultiplied colour by the group's alpha is
    // exactly "draw this layer at that alpha", and the pipeline's own blend
    // state is the source-over that puts it down.
    return texel * composite.alpha;
}
