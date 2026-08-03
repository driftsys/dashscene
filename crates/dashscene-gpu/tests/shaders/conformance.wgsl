// Layer-2 conformance entry points: one per function of the shader library,
// each evaluating it over a buffer of probes and writing one float per probe.
//
// Concatenated after `dashscene_gpu::SDF_WGSL`, so the functions under test are
// the ones the render pipelines will include — not a copy of them.
//
// Every entry point reads the same generic probe and writes the same output
// buffer. The fields are named for their shape rather than their meaning,
// because what a probe means differs per function; each entry point documents
// how it reads one.

struct Probe {
    // Two four-float slots and two two-float slots, in that order, so the
    // struct is 48 bytes with nothing padded under std430 and the Rust side
    // can be plain `#[repr(C)]`.
    v0: vec4f,
    v1: vec4f,
    p: vec2f,
    q: vec2f,
}

@group(0) @binding(0) var<storage, read> probes: array<Probe>;
@group(0) @binding(1) var<storage, read_write> results: array<f32>;

// `rounded_box_sdf(p, half_size, radii)` — p is `p`, half_size is `q`, radii is
// `v0` as (top_left, top_right, bottom_right, bottom_left).
@compute @workgroup_size(64)
fn probe_rounded_box_sdf(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = rounded_box_sdf(probe.p, probe.q, probe.v0);
}

// `coverage(d, width)` — d is `v1.x`, width is `v1.y`.
@compute @workgroup_size(64)
fn probe_coverage(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = coverage(probe.v1.x, probe.v1.y);
}

// `median3(v)` — the three channels are `v0.xyz`.
@compute @workgroup_size(64)
fn probe_median3(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    results[i] = median3(probes[i].v0.xyz);
}

// `msdf_coverage(sample, px_range)` — the sample is `v0.xyz`, px_range `v0.w`.
@compute @workgroup_size(64)
fn probe_msdf_coverage(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = msdf_coverage(probe.v0.xyz, probe.v0.w);
}

// The four gradient parameterizations — the point is `p`, the gradient origin
// is `q`, the primary handle is `v0.xy` and the secondary is `v0.zw`.
@compute @workgroup_size(64)
fn probe_gradient_linear(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = gradient_linear_t(probe.p, probe.q, probe.v0.xy, probe.v0.zw);
}

@compute @workgroup_size(64)
fn probe_gradient_radial(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = gradient_radial_t(probe.p, probe.q, probe.v0.xy, probe.v0.zw);
}

@compute @workgroup_size(64)
fn probe_gradient_angular(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = gradient_angular_t(probe.p, probe.q, probe.v0.xy, probe.v0.zw);
}

@compute @workgroup_size(64)
fn probe_gradient_diamond(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = gradient_diamond_t(probe.p, probe.q, probe.v0.xy, probe.v0.zw);
}

// `stroke_coverage(d, width, align, aa)` — d is `v1.x`, width `v1.y`, align
// `v1.z`, aa `v1.w`.
@compute @workgroup_size(64)
fn probe_stroke_coverage(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = stroke_coverage(probe.v1.x, probe.v1.y, probe.v1.z, probe.v1.w);
}

// `erf_approx(x)` — x is `v1.x`.
@compute @workgroup_size(64)
fn probe_erf(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    results[i] = erf_approx(probes[i].v1.x);
}

// `blurred_rounded_box(p, half_size, radii, sigma)` — p is `p`, half_size is
// `q`, radii is `v0`, sigma is `v1.x`.
@compute @workgroup_size(64)
fn probe_blurred_rounded_box(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    results[i] = blurred_rounded_box(probe.p, probe.q, probe.v0, probe.v1.x);
}

// `clamp_radii(half_size, radii)` — half_size is `q`, radii is `v0`; the
// result's component selected by `v1.x` (0..3) is written out, so one entry
// point can check all four.
@compute @workgroup_size(64)
fn probe_clamp_radii(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= arrayLength(&probes) { return; }
    let probe = probes[i];
    let r = clamp_radii(probe.q, probe.v0);
    let which = u32(probe.v1.x);
    if which == 0u { results[i] = r.x; }
    else if which == 1u { results[i] = r.y; }
    else if which == 2u { results[i] = r.z; }
    else { results[i] = r.w; }
}
