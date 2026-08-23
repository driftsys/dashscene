// GENERATED FILE — do not edit.
//
// Compiled from
//     crates/dashscene-gpu/src/shaders/sdf.wgsl
// by `naga`, the translator wgpu runs that same file through for the
// lean painter. Regenerate with:
//
//     just sdf-hlsl
//
// `docs/specification/03-target-hardware-rules.md` R-T5 asks for this
// math to be single-sourced into both product painters' shading
// languages. Editing this file breaks that in the one direction no
// review catches: it would still compile, still draw, and no longer
// be the same arithmetic the other painter evaluates. The test in
// `unity/package-gate` re-derives it on every run and fails if it is
// not what the WGSL produces.
//
// Two names differ from the WGSL, and both are naga's namer rather
// than a port: `median3` is emitted as `median3_` because the name
// ends with a digit, and `msdf_coverage`'s `sample` parameter as
// `sample_` because HLSL reserves it. Argument order is untouched.
// The namer has other rules that do not fire on this file —
// docs/design/unity-csharp-host.md carries them.

static const uint MAX_GRADIENT_STOPS = 8u;

float4 clamp_radii(float2 half_size, float4 radii)
{
    float f = 1.0;

    float width_2 = (2.0 * half_size.x);
    float height = (2.0 * half_size.y);
    float top = (radii.x + radii.y);
    float right = (radii.y + radii.z);
    float bottom = (radii.z + radii.w);
    float left = (radii.w + radii.x);
    if ((top > 0.0)) {
        float _e24 = f;
        f = min(_e24, (width_2 / top));
    }
    if ((bottom > 0.0)) {
        float _e29 = f;
        f = min(_e29, (width_2 / bottom));
    }
    if ((right > 0.0)) {
        float _e34 = f;
        f = min(_e34, (height / right));
    }
    if ((left > 0.0)) {
        float _e39 = f;
        f = min(_e39, (height / left));
    }
    float _e42 = f;
    return (radii * min(_e42, 1.0));
}

float rounded_box_sdf(float2 p, float2 half_size_1, float4 radii_in)
{
    const float4 _e3 = clamp_radii(half_size_1, radii_in);
    bool top_1 = (p.y < 0.0);
    bool left_1 = (p.x < 0.0);
    float top_r = (left_1 ? _e3.x : _e3.y);
    float bottom_r = (left_1 ? _e3.w : _e3.z);
    float r = (top_1 ? top_r : bottom_r);
    float2 q = ((abs(p) - half_size_1) + (r).xx);
    return ((min(max(q.x, q.y), 0.0) + length(max(q, (0.0).xx))) - r);
}

float coverage(float d, float width)
{
    if ((width <= 0.0)) {
        return ((d <= 0.0) ? 1.0 : 0.0);
    }
    return clamp((0.5 - (d / width)), 0.0, 1.0);
}

float median3_(float3 v)
{
    return max(min(v.x, v.y), min(max(v.x, v.y), v.z));
}

float msdf_coverage(float3 sample_, float px_range)
{
    const float _e2 = median3_(sample_);
    float signed_distance = (_e2 - 0.5);
    return clamp(((signed_distance * px_range) + 0.5), 0.0, 1.0);
}

float2 gradient_local(float2 p_1, float2 origin, float2 primary, float2 secondary)
{
    float2 u = (primary - origin);
    float2 v_1 = (secondary - origin);
    float det = ((u.x * v_1.y) - (u.y * v_1.x));
    if ((abs(det) <= 1e-20)) {
        return (0.0).xx;
    }
    float2 d_2 = (p_1 - origin);
    return (float2(((d_2.x * v_1.y) - (d_2.y * v_1.x)), ((u.x * d_2.y) - (u.y * d_2.x))) / (det).xx);
}

float gradient_linear_t(float2 p_2, float2 origin_1, float2 primary_1, float2 secondary_1)
{
    const float2 _e4 = gradient_local(p_2, origin_1, primary_1, secondary_1);
    return clamp(_e4.x, 0.0, 1.0);
}

float gradient_radial_t(float2 p_3, float2 origin_2, float2 primary_2, float2 secondary_2)
{
    const float2 _e4 = gradient_local(p_3, origin_2, primary_2, secondary_2);
    return clamp(length(_e4), 0.0, 1.0);
}

float gradient_angular_t(float2 p_4, float2 origin_3, float2 primary_3, float2 secondary_3)
{
    const float2 _e4 = gradient_local(p_4, origin_3, primary_3, secondary_3);
    if ((dot(_e4, _e4) <= 0.0)) {
        return 0.0;
    }
    return frac(((atan2(_e4.y, _e4.x) / 6.2831855) + 1.0));
}

float gradient_diamond_t(float2 p_5, float2 origin_4, float2 primary_4, float2 secondary_4)
{
    const float2 _e4 = gradient_local(p_5, origin_4, primary_4, secondary_4);
    float2 local = abs(_e4);
    return clamp((local.x + local.y), 0.0, 1.0);
}

float gradient_segment_t(float t, float lo, float hi)
{
    if ((hi <= lo)) {
        return 1.0;
    }
    return clamp(((t - lo) / (hi - lo)), 0.0, 1.0);
}

float4 gradient_ramp(float t_1, float offsets[8], float4 colours[8], uint count)
{
    float4 colour = (float4)0;
    uint i = 1u;

    if ((count == 0u)) {
        return (0.0).xxxx;
    }
    colour = colours[0];
    uint2 loop_bound = uint2(4294967295u, 4294967295u);
    bool loop_init = true;
    while(true) {
        if (all(loop_bound == uint2(0u, 0u))) { break; }
        loop_bound -= uint2(loop_bound.y == 0u, 1u);
        if (!loop_init) {
            uint _e33 = i;
            i = (_e33 + 1u);
        }
        loop_init = false;
        uint _e12 = i;
        if ((_e12 < count)) {
        } else {
            break;
        }
        {
            uint _e14 = i;
            if ((t_1 >= offsets[min(uint((_e14 - 1u)), 7u)])) {
                uint _e19 = i;
                uint _e23 = i;
                uint _e25 = i;
                uint _e29 = i;
                const float _e31 = gradient_segment_t(t_1, offsets[min(uint((_e25 - 1u)), 7u)], offsets[min(uint(_e29), 7u)]);
                colour = lerp(colours[min(uint((_e19 - 1u)), 7u)], colours[min(uint(_e23), 7u)], _e31);
            }
        }
    }
    float4 _e36 = colour;
    return _e36;
}

float stroke_coverage(float d_1, float width_1, float align, float aa)
{
    float centre = 0.0;

    if ((align < 0.5)) {
        centre = (-(width_1) * 0.5);
    } else {
        if ((align < 1.5)) {
            centre = 0.0;
        } else {
            centre = (width_1 * 0.5);
        }
    }
    float _e16 = centre;
    float lo_1 = (_e16 - (width_1 * 0.5));
    float _e20 = centre;
    float hi_1 = (_e20 + (width_1 * 0.5));
    const float _e25 = coverage((d_1 - hi_1), aa);
    const float _e27 = coverage((d_1 - lo_1), aa);
    return clamp((_e25 - _e27), 0.0, 1.0);
}

float erf_approx(float x_in)
{
    float x = (clamp(x_in, -4.0, 4.0) * 1.1283792);
    float xx = (x * x);
    float y_2 = (x + ((0.24295 + ((0.03395 + (0.0104 * xx)) * xx)) * (x * xx)));
    return (y_2 / sqrt((1.0 + (y_2 * y_2))));
}

float half_extent_at(float y, float2 half_size_2, float radius)
{
    float over = (abs(y) - (half_size_2.y - radius));
    if ((over <= 0.0)) {
        return half_size_2.x;
    }
    if ((over >= radius)) {
        return (half_size_2.x - radius);
    }
    return ((half_size_2.x - radius) + sqrt(max(((radius * radius) - (over * over)), 0.0)));
}

float blur_row(float y_1, float px, float2 half_size_3, float4 radii_1, float inv)
{
    bool top_2 = (y_1 < 0.0);
    float left_r = (top_2 ? radii_1.x : radii_1.w);
    float right_r = (top_2 ? radii_1.y : radii_1.z);
    const float _e13 = half_extent_at(y_1, half_size_3, left_r);
    float left_2 = -(_e13);
    const float _e15 = half_extent_at(y_1, half_size_3, right_r);
    const float _e18 = erf_approx(((_e15 - px) * inv));
    const float _e21 = erf_approx(((left_2 - px) * inv));
    return (0.5 * (_e18 - _e21));
}

float blurred_rounded_box(float2 p_6, float2 half_size_4, float4 radii_in_1, float sigma)
{
    float total = 0.0;
    int i_1 = int(0);

    const float4 _e4 = clamp_radii(half_size_4, radii_in_1);
    if ((sigma <= 0.0)) {
        const float _e7 = rounded_box_sdf(p_6, half_size_4, radii_in_1);
        return ((_e7 <= 0.0) ? 1.0 : 0.0);
    }
    float inv_1 = (1.0 / (sigma * 1.4142135));
    float box_lo = -(half_size_4.y);
    float box_hi = half_size_4.y;
    float lo_2 = max(box_lo, (p_6.y - (3.0 * sigma)));
    float hi_2 = min(box_hi, (p_6.y + (3.0 * sigma)));
    if ((hi_2 <= lo_2)) {
        return 0.0;
    }
    float step_ = ((hi_2 - lo_2) / 12.0);
    float norm = (0.3989423 / sigma);
    uint2 loop_bound_1 = uint2(4294967295u, 4294967295u);
    bool loop_init_1 = true;
    while(true) {
        if (all(loop_bound_1 == uint2(0u, 0u))) { break; }
        loop_bound_1 -= uint2(loop_bound_1.y == 0u, 1u);
        if (!loop_init_1) {
            int _e64 = i_1;
            i_1 = asint(asuint(_e64) + asuint(int(1)));
        }
        loop_init_1 = false;
        int _e41 = i_1;
        if ((_e41 < int(12))) {
        } else {
            break;
        }
        {
            int _e44 = i_1;
            float y_3 = (lo_2 + (step_ * (float(_e44) + 0.5)));
            float dy = ((y_3 - p_6.y) / sigma);
            float _e53 = total;
            const float _e55 = blur_row(y_3, p_6.x, half_size_4, _e4, inv_1);
            total = (_e53 + (((_e55 * norm) * exp(((-0.5 * dy) * dy))) * step_));
        }
    }
    if ((lo_2 > box_lo)) {
        const float _e71 = erf_approx(((lo_2 - p_6.y) * inv_1));
        const float _e75 = erf_approx(((box_lo - p_6.y) * inv_1));
        float mass = (0.5 * (_e71 - _e75));
        float _e79 = total;
        const float _e81 = blur_row(lo_2, p_6.x, half_size_4, _e4, inv_1);
        total = (_e79 + (_e81 * mass));
    }
    if ((hi_2 < box_hi)) {
        const float _e88 = erf_approx(((box_hi - p_6.y) * inv_1));
        const float _e92 = erf_approx(((hi_2 - p_6.y) * inv_1));
        float mass_1 = (0.5 * (_e88 - _e92));
        float _e96 = total;
        const float _e98 = blur_row(hi_2, p_6.x, half_size_4, _e4, inv_1);
        total = (_e96 + (_e98 * mass_1));
    }
    float _e101 = total;
    return clamp(_e101, 0.0, 1.0);
}

