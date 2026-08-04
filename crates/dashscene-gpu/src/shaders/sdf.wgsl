// The lean painter's signed-distance math — the one source it is written in.
//
// R-T5 (`docs/specification/03-target-hardware-rules.md`) asks for this math to
// be single-sourced into both product painters' shading languages, so that
// "same picture" is a property of one implementation rather than a promise two
// implementations make separately. This file is that source. The render
// pipelines include it (story #580) and so does the layer-2 conformance harness
// (story #579), which evaluates every function below by compute shader and
// compares it against an independent implementation in Rust.
//
// Nothing here samples a texture, reads a derivative, or touches a uniform. It
// is float arithmetic over its arguments and nothing else, which is what makes
// it evaluable in a compute shader with no rasteriser, no antialiasing resolve,
// no blend stage and no sampler in the loop — the reason epic #569 trusts a
// software adapter for layer 2 and does not trust one for layer 4.

// Corner radii scaled down until no edge is over-subscribed, which is the rule
// Skia applies internally and this project therefore inherits.
//
// Figma authors a pill as `cornerRadius: 9999`, and `dashc` passes that through
// unchanged (`crates/dashc/src/figma/mod.rs`, `corners_of`); `dashscene-skia`
// relies on Skia clamping it. Nothing clamped it on this side, and the
// Inigo Quilez form has no meaning for a radius larger than the box: a 50x30
// half-box with radii 9999 reported every point roughly 4085 units *outside*
// the shape, so the painter drew nothing where the reference painter draws a
// pill.
//
// The rule: for each edge, the two radii that meet it may not exceed its
// length; take the worst ratio and scale all four by it.
fn clamp_radii(half_size: vec2f, radii: vec4f) -> vec4f {
    let width = 2.0 * half_size.x;
    let height = 2.0 * half_size.y;
    var f = 1.0;
    let top = radii.x + radii.y;      // top_left + top_right
    let right = radii.y + radii.z;    // top_right + bottom_right
    let bottom = radii.z + radii.w;   // bottom_right + bottom_left
    let left = radii.w + radii.x;     // bottom_left + top_left
    if top > 0.0 { f = min(f, width / top); }
    if bottom > 0.0 { f = min(f, width / bottom); }
    if right > 0.0 { f = min(f, height / right); }
    if left > 0.0 { f = min(f, height / left); }
    return radii * min(f, 1.0);
}

// The signed distance from `p` to a rounded box centred on the origin, negative
// inside. `half_size` is half the box's extent; `radii` is (top_left,
// top_right, bottom_right, bottom_left), matching `dashpaint::CornerRadii`.
//
// y is down, as it is everywhere in this project's document space, so "top" is
// the negative-y side.
fn rounded_box_sdf(p: vec2f, half_size: vec2f, radii_in: vec4f) -> f32 {
    let radii = clamp_radii(half_size, radii_in);
    // The radius of the corner `p` is nearest to. Written as two selects on the
    // sign of each axis rather than as a swizzle chain, because the swizzle
    // form encodes the corner order implicitly and this file is the place that
    // order is defined.
    let top = p.y < 0.0;
    let left = p.x < 0.0;
    let top_r = select(radii.y, radii.x, left);      // top_right, top_left
    let bottom_r = select(radii.z, radii.w, left);   // bottom_right, bottom_left
    let r = select(bottom_r, top_r, top);

    // Inigo Quilez's rounded-box distance: shrink the box by the corner radius,
    // take the distance to that box, and grow it back.
    let q = abs(p) - half_size + vec2f(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2f(0.0))) - r;
}

// Antialiased coverage for a signed distance, in [0, 1].
//
// `width` is the distance, in the same units as `d`, over which the edge
// ramps — one device pixel for a screen-space edge. Passed in rather than taken
// from `fwidth`, for the reason the MSDF resolve below gives, and because a
// compute shader has no derivatives to take.
fn coverage(d: f32, width: f32) -> f32 {
    // A zero width is a hard edge: inside is covered, outside is not. Guarding
    // it here keeps every caller from having to, and keeps the function total —
    // `d / 0.0` is an infinity that propagates a NaN through `clamp` when `d`
    // is also zero, which is exactly the sample sitting on the edge.
    if width <= 0.0 {
        return select(0.0, 1.0, d <= 0.0);
    }
    return clamp(0.5 - d / width, 0.0, 1.0);
}

// The median of three channels — the MSDF resolve.
//
// A multi-channel distance field stores three distances whose median
// reconstructs a sharp corner that a single channel would round off. This is
// the whole of why the atlas is MSDF rather than SDF.
fn median3(v: vec3f) -> f32 {
    return max(min(v.r, v.g), min(max(v.r, v.g), v.b));
}

// Coverage for an MSDF sample.
//
// `px_range` is the field's range measured in *screen* pixels — the atlas's
// `distance_range_px` scaled by the ratio of render size to the size the atlas
// was baked at (`dashpaint::Atlas::distance_range_px` documents that scaling).
// It arrives as a uniform rather than from `fwidth`: Chlumsky recommends the
// uniform form for 2D text, and the compact derivative form has a documented
// failure where `fwidth` returns zero and the division produces a NaN that
// paints a hole. A compute shader has no derivatives at all, so the uniform
// form is also what makes this function conformance-testable.
fn msdf_coverage(sample: vec3f, px_range: f32) -> f32 {
    let signed_distance = median3(sample) - 0.5;
    return clamp(signed_distance * px_range + 0.5, 0.0, 1.0);
}

// A point in the gradient's own coordinate frame.
//
// `dashpaint::Gradient` carries three normalized handles, and they define an
// affine frame: the origin maps to (0, 0), the primary handle to (1, 0) and the
// secondary handle to (0, 1). `dashscene-skia`'s `gradient_frame` builds
// exactly that matrix, so a painter that projected onto the primary axis alone
// would disagree with the reference painter for every gradient whose frame is
// not a similarity — a radial with an elliptical frame, or a linear whose
// secondary handle is not perpendicular to its primary. Both are ordinary in
// Figma, and R-T5's "same picture" is the claim this whole file exists to
// support.
//
// Returns (0, 0) for a degenerate frame, which is what a gradient with no area
// can mean: every sample takes the first stop.
fn gradient_local(p: vec2f, origin: vec2f, primary: vec2f, secondary: vec2f) -> vec2f {
    let u = primary - origin;
    let v = secondary - origin;
    let det = u.x * v.y - u.y * v.x;
    if abs(det) <= 1e-20 {
        return vec2f(0.0);
    }
    let d = p - origin;
    // The inverse of the 2x2 frame [u v], applied to `d`.
    return vec2f(d.x * v.y - d.y * v.x, u.x * d.y - u.y * d.x) / det;
}

// Linear: how far along the primary axis, in the handle frame.
fn gradient_linear_t(p: vec2f, origin: vec2f, primary: vec2f, secondary: vec2f) -> f32 {
    return clamp(gradient_local(p, origin, primary, secondary).x, 0.0, 1.0);
}

// Radial: the distance from the origin in the handle frame, so an
// unequal-length or non-perpendicular frame gives an ellipse rather than a
// circle.
fn gradient_radial_t(p: vec2f, origin: vec2f, primary: vec2f, secondary: vec2f) -> f32 {
    return clamp(length(gradient_local(p, origin, primary, secondary)), 0.0, 1.0);
}

// Angular (a conic sweep): the angle around the origin in the handle frame,
// measured from the primary handle, normalized to [0, 1) clockwise in a y-down
// space.
fn gradient_angular_t(p: vec2f, origin: vec2f, primary: vec2f, secondary: vec2f) -> f32 {
    let local = gradient_local(p, origin, primary, secondary);
    if dot(local, local) <= 0.0 {
        return 0.0;
    }
    let tau = 6.2831853071795865;
    // `atan2` is (-pi, pi]; the `+ 1.0` lifts it positive before `fract`, which
    // is floor-based in WGSL and truncation-based in Rust. The reference in
    // `layer2_conformance.rs` relies on the two agreeing, which they do only
    // while the argument is positive — so the lift is load-bearing on both
    // sides, not decoration.
    return fract(atan2(local.y, local.x) / tau + 1.0);
}

// Diamond: the L1 distance in the handle frame, which is what makes the
// iso-lines diamonds rather than ellipses.
fn gradient_diamond_t(p: vec2f, origin: vec2f, primary: vec2f, secondary: vec2f) -> f32 {
    let local = abs(gradient_local(p, origin, primary, secondary));
    return clamp(local.x + local.y, 0.0, 1.0);
}

// The most stops one gradient carries — `dashpaint::MAX_GRADIENT_STOPS`.
//
// Boundary B fixes the ceiling rather than each painter, so that the validator
// that rejects an over-long gradient upstream (P4) and the painters that assume
// the bound are held to one number. The Rust side asserts this copy against
// that one.
const MAX_GRADIENT_STOPS: u32 = 8u;

// Where `t` sits within the stop segment `lo`..`hi`, in [0, 1].
//
// `hi <= lo` is a **hard stop**: two stops authored at the same offset, which
// Figma produces for a banded gradient. That segment has no width, so there is
// no position within it, and one is the answer that makes the ramp
// right-continuous — the colour *at* a hard stop's offset is the later stop's,
// which is what `TileMode::Clamp` gives in the reference painter. Returning
// zero instead would make the earlier colour win at the offset, and dividing
// would produce a NaN that `mix` propagates into the picture.
//
// **The branch is only observable when the zero-width segment is the last one
// the walk visits**, and that is worth knowing before writing a fixture for it.
// `gradient_ramp` overwrites as it goes, so a repeated offset in the middle of
// a stop list has its result replaced by the segment after it — the answer is
// the same whichever value comes back here. Mutation testing is what showed it:
// a fixture with a repeated offset at 0.5 of four stops could not tell one from
// zero. What can is two stops sharing the *final* offset, and a two-stop ramp
// whose stops share their only offset.
fn gradient_segment_t(t: f32, lo: f32, hi: f32) -> f32 {
    if hi <= lo {
        return 1.0;
    }
    return clamp((t - lo) / (hi - lo), 0.0, 1.0);
}

// A gradient's colour at normalized position `t`, from its stop ramp.
//
// `offsets` and `colours` are index-aligned and only the first `count` entries
// of each are read. The offsets are non-decreasing, which is the same
// precondition `dashscene-skia` inherits from Skia's own gradient shaders —
// this function does not sort them, and an out-of-order stop makes it disagree
// with the reference painter rather than fail.
//
// **Clamped at both ends, not repeated.** Below the first stop the first
// colour, above the last stop the last colour: `TileMode::Clamp`, which is what
// every gradient in `dashscene-skia` is built with. A stop range that does not
// start at 0 or end at 1 is therefore an ordinary case rather than a degenerate
// one, and it is the case a producer authors whenever it moves a handle instead
// of a stop.
//
// **Interpolation is a plain `mix` of the stored components**, which is
// sRGB-encoded space — `docs/decisions/blur-blends-in-srgb-encoded-space.md`
// makes that a term of the boundary-B contract rather than a per-painter
// choice, and the reference painter interpolates its stops unpremultiplied in
// the same space.
//
// The walk keeps the *last* segment `t` has entered rather than stopping at the
// first match. Both find the same segment, and this form has no early exit and
// no separate "past the end" branch: `gradient_segment_t` saturates, so a `t`
// above the final stop leaves the final colour in place by the same arithmetic
// that interpolates an interior one.
fn gradient_ramp(
    t: f32,
    offsets: array<f32, MAX_GRADIENT_STOPS>,
    colours: array<vec4f, MAX_GRADIENT_STOPS>,
    count: u32,
) -> vec4f {
    // A gradient the paint table holds carries at least one stop, so this is a
    // row no interned gradient produces. Transparent rather than the first
    // colour, because there is no first colour to take: drawing nothing is the
    // one answer that cannot paint a wrong one.
    if count == 0u {
        return vec4f(0.0);
    }
    var colour = colours[0];
    for (var i = 1u; i < count; i = i + 1u) {
        if t >= offsets[i - 1u] {
            colour = mix(
                colours[i - 1u],
                colours[i],
                gradient_segment_t(t, offsets[i - 1u], offsets[i]),
            );
        }
    }
    return colour;
}

// Coverage of a stroke of width `width` centred on the outline `d` describes.
//
// `align` shifts the band: 0 = Inside, 1 = Center, 2 = Outside, matching
// `dashpaint::StrokeAlign`. An Inside stroke sits entirely within the shape, an
// Outside stroke entirely without, and a Center stroke straddles it — the same
// geometry `dashscene-skia`'s `stroke_outset` derives its shadow silhouette
// from.
fn stroke_coverage(d: f32, width: f32, align: f32, aa: f32) -> f32 {
    // No zero-width guard. There was one, and the band form below made it dead:
    // a zero width puts both edges at the same place, so the two ramps are
    // identical and their difference is zero for every `d`. Mutation testing is
    // what showed it — deleting the guard changed no result — and an
    // unreachable branch that reads as load-bearing is worse than none.
    //
    // The band's centre line, as a signed distance from the outline.
    var centre = 0.0;
    if align < 0.5 {
        centre = -width * 0.5;        // Inside
    } else if align < 1.5 {
        centre = 0.0;                 // Center
    } else {
        centre = width * 0.5;         // Outside
    }
    // The band's two edges, and the coverage between them as the difference of
    // their ramps.
    //
    // Not `coverage(abs(d - centre) - width/2, aa)`: that is one ramp of a
    // folded distance, and it saturates as soon as the fold passes the ramp's
    // centre, so a stroke narrower than the antialiasing width paints far too
    // opaque. Measured: a 0.25-unit stroke at `aa = 1` reported 0.625 coverage
    // where 0.25 is correct. The difference of the two edge ramps is exact for
    // a linear ramp and identical to the old form for `width >= aa`.
    let lo = centre - width * 0.5;
    let hi = centre + width * 0.5;
    return clamp(coverage(d - hi, aa) - coverage(d - lo, aa), 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// The blurred rounded rectangle — a drop shadow's coverage
// ---------------------------------------------------------------------------

// An approximation of the Gauss error function, accurate to about 1e-4 over
// the range a blur uses.
//
// Abramowitz-and-Stegun-style rational form, in the compact arrangement Raph
// Levien uses for blurred rounded rectangles. The constants are empirically
// fitted rather than derived, which is exactly why story #579 says to measure
// this before trusting it — `layer2_conformance.rs` does, against this
// function's own definition integrated by Simpson's rule in double precision,
// and records the measured error.
fn erf_approx(x_in: f32) -> f32 {
    // Clamped before the polynomial, not after. The fitted form grows as
    // `0.0104 * t^7`, so `y * y` overflows f32 at |x| around 962 and
    // `y / sqrt(1 + y*y)` collapses to `y / inf` = 0 — the error function
    // reading zero where it should read one. Past |x| = 4 the true erf is 1 to
    // within 1.5e-8, which is four orders below anything this shader is held
    // to, so clamping there is exact for every purpose here and removes the
    // overflow entirely.
    //
    // Not hypothetical: `blurred_rounded_box` divides by sigma, so a hairline
    // blur on a wide element reaches this. A half-width of 720 — a full-width
    // element on the 1440 hero canvas — with a Figma blur radius of 1
    // (sigma 0.4375) made the centre of the shape report zero coverage.
    let x = clamp(x_in, -4.0, 4.0) * 1.1283791671;  // 2 / sqrt(pi)
    let xx = x * x;
    let y = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
    return y / sqrt(1.0 + y * y);
}

// Half the rounded box's horizontal extent at height `y`, on the side `radius`
// belongs to.
//
// Above the corner centre the edge is straight and the extent is the full half
// width; within the corner it follows the arc. Written per side rather than
// once, because this project's boxes carry four independent corner radii and a
// shadow that assumed one would round the wrong corners.
fn half_extent_at(y: f32, half_size: vec2f, radius: f32) -> f32 {
    // How far past the corner's centre this height reaches.
    let over = abs(y) - (half_size.y - radius);
    if over <= 0.0 {
        return half_size.x;
    }
    if over >= radius {
        return half_size.x - radius;
    }
    return half_size.x - radius + sqrt(max(radius * radius - over * over, 0.0));
}

// The coverage of a Gaussian-blurred rounded box at `p`, in [0, 1].
//
// `p` is relative to the box's centre, `half_size` is half its extent, `radii`
// is (top_left, top_right, bottom_right, bottom_left), and `sigma` is the
// Gaussian's standard deviation — `0.4375 * radius` for this project's blurs
// (`docs/decisions/blur-sigma-is-figmas-mapping.md`).
//
// The x integral is exact: at any height the box's cross-section is one
// interval, and a Gaussian's integral over an interval is a difference of two
// error functions. Only the y integral is approximated, by midpoint quadrature
// over the blur's support.
//
// **Twelve samples, measured rather than chosen.** Story #579 says to validate
// this before trusting it, and `layer2_conformance.rs` does, against a
// 512-row quadrature with a real erf. Wallace's four samples — the
// construction the story names as the one with production mileage — come out
// at 5.10 code points of 255 at the worst probe, which is visible. The error
// roughly halves as the count doubles:
//
//     samples   4      6      8      12     16
//     worst     5.10   2.51   1.58   0.83   0.54   code points of 255
//
// Twelve is the first that fits inside one code point, which is the budget
// because a shadow within one code point of the truth cannot be told from it
// in an eight-bit output. The worst probe throughout is a corner whose radius
// is most of the box's half-height, where the cross-section varies fastest.
// Story #584 owns the shadow's cost and may revisit the count against a frame
// budget; the table above is what that decision needs.
fn blur_row(y: f32, px: f32, half_size: vec2f, radii: vec4f, inv: f32) -> f32 {
    // The corner radii that apply at this height, left and right.
    let top = y < 0.0;
    let left_r = select(radii.w, radii.x, top);   // bottom_left, top_left
    let right_r = select(radii.z, radii.y, top);  // bottom_right, top_right
    let left = -half_extent_at(y, half_size, left_r);
    let right = half_extent_at(y, half_size, right_r);
    return 0.5 * (erf_approx((right - px) * inv) - erf_approx((left - px) * inv));
}

fn blurred_rounded_box(p: vec2f, half_size: vec2f, radii_in: vec4f, sigma: f32) -> f32 {
    let radii = clamp_radii(half_size, radii_in);
    // A zero blur is the unblurred shape. Guarding it keeps the reciprocal
    // below finite and makes the function total at the boundary of its domain.
    if sigma <= 0.0 {
        return select(0.0, 1.0, rounded_box_sdf(p, half_size, radii_in) <= 0.0);
    }
    let inv = 1.0 / (sigma * 1.4142135624);  // 1 / (sigma * sqrt(2))
    let box_lo = -half_size.y;
    let box_hi = half_size.y;
    // Quadrature only where it is needed: within three sigma of the sample,
    // and within the box, since outside the box the cross-section is empty.
    let lo = max(box_lo, p.y - 3.0 * sigma);
    let hi = min(box_hi, p.y + 3.0 * sigma);
    if hi <= lo {
        return 0.0;
    }
    let step = (hi - lo) / 12.0;
    let norm = 0.3989422804 / sigma;  // 1 / (sigma * sqrt(2 pi))

    var total = 0.0;
    for (var i = 0; i < 12; i = i + 1) {
        let y = lo + step * (f32(i) + 0.5);
        let dy = (y - p.y) / sigma;
        total = total + blur_row(y, p.x, half_size, radii, inv) * norm * exp(-0.5 * dy * dy) * step;
    }

    // The tails. Where the window was cut by three sigma rather than by the
    // box, the rows beyond it are still inside the shape and still contribute:
    // treating them as empty is what made a sample at the centre of a tall box
    // read 0.997 instead of 1. Each tail is the Gaussian mass out there times
    // the row at the window's edge, which is exact wherever that edge has
    // cleared the corner band and is the straight side.
    if lo > box_lo {
        let mass = 0.5 * (erf_approx((lo - p.y) * inv) - erf_approx((box_lo - p.y) * inv));
        total = total + blur_row(lo, p.x, half_size, radii, inv) * mass;
    }
    if hi < box_hi {
        let mass = 0.5 * (erf_approx((box_hi - p.y) * inv) - erf_approx((hi - p.y) * inv));
        total = total + blur_row(hi, p.x, half_size, radii, inv) * mass;
    }
    return clamp(total, 0.0, 1.0);
}
