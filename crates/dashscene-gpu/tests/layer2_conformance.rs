//! Layer 2 of epic #569's verification net: the shader library's arithmetic,
//! evaluated on a real device by compute shader and checked against an
//! independent implementation.
//!
//! # Why compute, and why this is trustworthy on a software adapter
//!
//! Evaluating the math in a compute shader removes the rasteriser, the
//! antialiasing resolve, the blend stage and the sampler from the loop. What
//! is left is float arithmetic over the function's arguments. That is stable
//! across drivers and Mesa versions in a way coverage and blending are not,
//! which is why epic #569 trusts lavapipe here and does not trust it for
//! layer 4. The same property is what makes the suite meaningful on whatever
//! adapter a developer's machine offers.
//!
//! # The reference is not a copy of the shader
//!
//! A conformance suite whose expectation restates the implementation checks
//! the shader compiler, not the math. Each expectation below is derived
//! independently where that is possible: the rounded-box distance from
//! brute-force sampling of the outline, the median from sorting, the error
//! function from integrating its own definition.
//!
//! Two of them are not, and saying so is the point. `coverage` and the four
//! gradients have expectations transliterated from the same expression the
//! shader uses, because those functions *are* their definitions — there is no
//! second derivation to write. They still catch a WGSL-side error, which
//! mutation testing confirms, but they check the transliteration and not the
//! mathematics, and a reader should not think otherwise.
//!
//! # This suite is shared with the Unity painter
//!
//! R-T5 asks for the SDF math to be single-sourced into both product painters'
//! shading languages. The functions under test come from
//! `dashscene_gpu::SDF_WGSL`, the one file the render pipelines also include,
//! so a second painter porting that file has a suite to port with it rather
//! than a review promise.

use bytemuck::{Pod, Zeroable};

/// One evaluation's arguments. The fields are named for their shape rather
/// than their meaning: what a probe means differs per function, and each entry
/// point in `shaders/conformance.wgsl` documents how it reads one.
///
/// Forty-eight bytes, `#[repr(C)]`, laid out four-float slots first so that
/// nothing is padded under std430 and the Rust and WGSL declarations agree
/// without a rule about either.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct Probe {
    v0: [f32; 4],
    v1: [f32; 4],
    p: [f32; 2],
    q: [f32; 2],
}

/// A device and the queue that feeds it.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    module: wgpu::ShaderModule,
}

impl Gpu {
    /// Acquires an adapter and compiles the shader library plus the probe
    /// entry points.
    ///
    /// Panics when no adapter is available rather than skipping. A conformance
    /// suite that quietly passes on a machine with no device is a green result
    /// that establishes nothing, which is the shape v0.13 spent a slice
    /// removing (`docs/decisions/t2-check-has-no-teeth.md`). CI installs a
    /// software adapter for exactly this reason.
    fn new() -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .expect(
            "layer-2 conformance needs a wgpu adapter and found none. On a runner this means the \
             software adapter is not installed; the CI job installs mesa-vulkan-drivers for it.",
        );
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("layer-2 conformance"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .expect("the adapter provides a device at downlevel defaults");

        // The library and the entry points, concatenated: the functions under
        // test are the ones a render pipeline includes, not a copy.
        let source = format!(
            "{}\n{}",
            dashscene_gpu::SDF_WGSL,
            include_str!("shaders/conformance.wgsl")
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf + conformance probes"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        Self {
            device,
            queue,
            module,
        }
    }

    /// Evaluates `entry` over `probes` and returns one result per probe.
    fn run(&self, entry: &str, probes: &[Probe]) -> Vec<f32> {
        self.run_with(entry, probes, &[])
    }

    /// The same, for an entry point that also reads a table at binding 2.
    ///
    /// One function has a table and it is the stop ramp, whose input is an
    /// *array* rather than the twelve floats a [`Probe`] carries. Bound rather
    /// than synthesised in WGSL, so that the colours the shader mixes are the
    /// same values the Rust reference mixes and neither side invented them.
    ///
    /// The binding is added only when there is a table, because `layout: None`
    /// reflects each pipeline's layout from the bindings its own entry point
    /// uses — every other entry point declares two, and naming a third would be
    /// a bind group that does not match its layout.
    fn run_with(&self, entry: &str, probes: &[Probe], table: &[[f32; 4]]) -> Vec<f32> {
        use wgpu::util::DeviceExt as _;

        assert!(
            !probes.is_empty(),
            "a probe set with no probes proves nothing"
        );
        let results_size = (probes.len() * size_of::<f32>()) as wgpu::BufferAddress;

        let probe_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("probes"),
                contents: bytemuck::cast_slice(probes),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let result_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("results"),
            size: results_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("results readback"),
            size: results_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &self.module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        let table_buffer = (!table.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("table"),
                    contents: bytemuck::cast_slice(table),
                    usage: wgpu::BufferUsages::STORAGE,
                })
        });
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: probe_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: result_buffer.as_entire_binding(),
            },
        ];
        if let Some(buffer) = &table_buffer {
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: buffer.as_entire_binding(),
            });
        }
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(entry),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(entry) });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(entry),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(probes.len().div_ceil(64) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&result_buffer, 0, &staging, 0, results_size);
        self.queue.submit([encoder.finish()]);

        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| {
            r.expect("the readback buffer maps");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device completes the submission");
        let data = slice
            .get_mapped_range()
            .expect("the mapped range is readable");
        let out = bytemuck::cast_slice::<u8, f32>(&data[..]).to_vec();
        drop(data);
        staging.unmap();
        out
    }
}

/// Compares a run against its reference, naming the probe that differs.
fn assert_matches(entry: &str, probes: &[Probe], measured: &[f32], expected: &[f32], tol: f32) {
    assert_eq!(measured.len(), expected.len());
    for (index, (&got, &want)) in measured.iter().zip(expected).enumerate() {
        assert!(
            (got - want).abs() <= tol,
            "{entry} probe {index} {:?}: shader gave {got}, the reference gives {want} \
             (difference {}, tolerance {tol})",
            probes[index],
            (got - want).abs()
        );
    }
}

// ---------------------------------------------------------------------------
// Independent references
// ---------------------------------------------------------------------------

/// The signed distance to a rounded box, derived by sampling the outline
/// rather than by restating the shader's closed form.
///
/// The outline is walked as four straight edges and four corner arcs, and the
/// distance is the smallest to any sampled point; the sign comes from a
/// separate inside test. Nothing here shares an expression with
/// `rounded_box_sdf`, which is the point — a suite whose expectation is the
/// implementation checks the compiler and not the math.
///
/// Accurate to about half the sample spacing: 4096 samples along a 100-unit
/// edge is 0.0122, so the tolerance the tests use (0.02) is 1.6 times it. The
/// measured worst difference over the probe set is 7.6e-6, three orders inside
/// that, because the nearest sample is almost never the worst case — but the
/// tolerance is set by the spacing, which is the bound that always holds.
fn reference_clamp_radii(half: [f32; 2], radii: [f32; 4]) -> [f32; 4] {
    let (w, h) = (2.0 * half[0], 2.0 * half[1]);
    let pairs = [
        (radii[0] + radii[1], w), // top
        (radii[1] + radii[2], h), // right
        (radii[2] + radii[3], w), // bottom
        (radii[3] + radii[0], h), // left
    ];
    let mut f = 1.0f32;
    for (sum, extent) in pairs {
        if sum > 0.0 {
            f = f.min(extent / sum);
        }
    }
    let f = f.min(1.0);
    [radii[0] * f, radii[1] * f, radii[2] * f, radii[3] * f]
}

fn reference_rounded_box_sdf(p: [f32; 2], half: [f32; 2], radii: [f32; 4]) -> f32 {
    let radii = reference_clamp_radii(half, radii);
    let (px, py) = (p[0] as f64, p[1] as f64);
    let (hx, hy) = (half[0] as f64, half[1] as f64);
    // (top_left, top_right, bottom_right, bottom_left), y down.
    let r: Vec<f64> = radii.iter().map(|&v| v as f64).collect();
    let (tl, tr, br, bl) = (r[0], r[1], r[2], r[3]);

    const N: usize = 4096;
    let mut best = f64::INFINITY;
    let mut consider = |x: f64, y: f64| {
        let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
        if d < best {
            best = d;
        }
    };

    // The four corner centres, in the same order as the radii, each with the
    // direction that is "outward" for that corner.
    //
    // The direction is carried rather than inferred from the sign of the
    // centre's coordinate. When a radius equals its half-extent the centre
    // lands exactly on an axis — a top corner's centre at y = 0 — and a sign
    // test then reads it as the wrong side. That is not hypothetical: it is
    // what this reference got wrong on its first run, and the shader was
    // right.
    let pi = std::f64::consts::PI;
    let centres = [
        (-hx + tl, -hy + tl, tl, pi, 1.5 * pi, -1.0, -1.0),
        (hx - tr, -hy + tr, tr, 1.5 * pi, 2.0 * pi, 1.0, -1.0),
        (hx - br, hy - br, br, 0.0, 0.5 * pi, 1.0, 1.0),
        (-hx + bl, hy - bl, bl, 0.5 * pi, pi, -1.0, 1.0),
    ];
    for &(cx, cy, radius, a0, a1, _, _) in &centres {
        if radius <= 0.0 {
            consider(cx, cy);
            continue;
        }
        for i in 0..=N {
            let a = a0 + (a1 - a0) * (i as f64 / N as f64);
            consider(cx + radius * a.cos(), cy + radius * a.sin());
        }
    }
    // The four straight edges, between the corner tangent points.
    let edges = [
        ((-hx + tl, -hy), (hx - tr, -hy)), // top
        ((hx, -hy + tr), (hx, hy - br)),   // right
        ((hx - br, hy), (-hx + bl, hy)),   // bottom
        ((-hx, hy - bl), (-hx, -hy + tl)), // left
    ];
    for &((x0, y0), (x1, y1)) in &edges {
        for i in 0..=N {
            let t = i as f64 / N as f64;
            consider(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        }
    }

    // Inside test, independent of the distance: outside the box's own extent is
    // outside; within a corner's quadrant, inside iff within that corner's arc.
    let inside = if px.abs() > hx || py.abs() > hy {
        false
    } else {
        let mut inside = true;
        for &(cx, cy, radius, _, _, dir_x, dir_y) in &centres {
            if radius <= 0.0 {
                continue;
            }
            let beyond_x = if dir_x < 0.0 { px < cx } else { px > cx };
            let beyond_y = if dir_y < 0.0 { py < cy } else { py > cy };
            if beyond_x && beyond_y {
                inside = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() <= radius;
                break;
            }
        }
        inside
    };
    (if inside { -best } else { best }) as f32
}

/// The median of three, by sorting.
fn reference_median3(v: [f32; 3]) -> f32 {
    let mut s = v;
    s.sort_by(|a, b| a.partial_cmp(b).expect("the probes are finite"));
    s[1]
}

// ---------------------------------------------------------------------------
// The suite
// ---------------------------------------------------------------------------

/// A spread of points and boxes: inside, outside, on an edge, in a corner arc,
/// and past each corner. Every box has four different radii, and one has a
/// sharp corner beside a round one, so a shader that picked the wrong corner
/// would land on a different value.
fn rounded_box_probes() -> Vec<Probe> {
    let boxes: [([f32; 2], [f32; 4]); 4] = [
        ([50.0, 30.0], [0.0, 0.0, 0.0, 0.0]),     // a sharp rectangle
        ([50.0, 30.0], [10.0, 4.0, 16.0, 8.0]),   // four different radii
        ([50.0, 30.0], [30.0, 0.0, 0.0, 12.0]),   // a sharp corner beside a round one
        ([20.0, 20.0], [20.0, 20.0, 20.0, 20.0]), // a circle
    ];
    let points: [[f32; 2]; 13] = [
        [0.0, 0.0],
        [10.0, 5.0],
        [49.0, 29.0],
        [-49.0, -29.0],
        [50.0, 30.0],
        [-50.0, -30.0],
        [55.0, 0.0],
        [0.0, 35.0],
        [-60.0, -40.0],
        [45.0, -25.0],
        [-45.0, 25.0],
        [47.0, 27.0],
        [-47.0, 27.0],
    ];
    let mut probes = Vec::new();
    for (half, radii) in boxes {
        for p in points {
            probes.push(Probe {
                v0: radii,
                p,
                q: half,
                ..Probe::default()
            });
        }
    }
    probes
}

#[test]
fn the_rounded_box_distance_matches_an_independently_sampled_outline() {
    let gpu = Gpu::new();
    let probes = rounded_box_probes();
    let measured = gpu.run("probe_rounded_box_sdf", &probes);
    let expected: Vec<f32> = probes
        .iter()
        .map(|probe| reference_rounded_box_sdf(probe.p, probe.q, probe.v0))
        .collect();
    // An order of magnitude above the sampling error of the reference.
    assert_matches("rounded_box_sdf", &probes, &measured, &expected, 0.02);
}

#[test]
fn the_coverage_ramp_is_linear_between_the_half_widths_and_clamps_outside() {
    let gpu = Gpu::new();
    let widths = [0.0f32, 0.5, 1.0, 2.0];
    let distances = [-4.0f32, -1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0, 4.0];
    let mut probes = Vec::new();
    for width in widths {
        for d in distances {
            probes.push(Probe {
                v1: [d, width, 0.0, 0.0],
                ..Probe::default()
            });
        }
    }
    let measured = gpu.run("probe_coverage", &probes);
    let expected: Vec<f32> = probes
        .iter()
        .map(|probe| {
            let (d, width) = (probe.v1[0], probe.v1[1]);
            if width <= 0.0 {
                if d <= 0.0 { 1.0 } else { 0.0 }
            } else {
                (0.5 - d / width).clamp(0.0, 1.0)
            }
        })
        .collect();
    assert_matches("coverage", &probes, &measured, &expected, 1e-6);

    // The property the ramp exists for, stated separately from the formula: it
    // is 1 well inside, 0 well outside, and exactly a half on the edge.
    for (index, probe) in probes.iter().enumerate() {
        if probe.v1[0] == 0.0 && probe.v1[1] > 0.0 {
            assert!(
                (measured[index] - 0.5).abs() < 1e-6,
                "a sample on the edge is half covered, got {}",
                measured[index]
            );
        }
    }
}

#[test]
fn the_msdf_resolve_takes_the_median_of_three_channels() {
    let gpu = Gpu::new();
    let samples: [[f32; 3]; 8] = [
        [0.1, 0.5, 0.9],
        [0.9, 0.5, 0.1],
        [0.5, 0.1, 0.9],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 0.0],
        [0.25, 0.25, 0.25],
        [0.49, 0.51, 0.50],
        [0.75, 0.20, 0.60],
    ];
    // Every sample at three different ranges. A single range lets a shader
    // that ignored the argument and hardcoded it pass — the same uniform-
    // fixture defect as a range that always starts at offset zero, one level
    // down, in the arguments rather than the data.
    let ranges = [2.0f32, 4.0, 9.0];
    let probes: Vec<Probe> = samples
        .iter()
        .flat_map(|s| {
            ranges.iter().map(move |&r| Probe {
                v0: [s[0], s[1], s[2], r],
                ..Probe::default()
            })
        })
        .collect();

    let measured = gpu.run("probe_median3", &probes);
    let expected: Vec<f32> = probes
        .iter()
        .map(|probe| reference_median3([probe.v0[0], probe.v0[1], probe.v0[2]]))
        .collect();
    assert_matches("median3", &probes, &measured, &expected, 1e-7);

    // The resolve is the median mapped through the coverage ramp at
    // `px_range`. Stated as the composition it is, over the two functions
    // already checked above, rather than as a third copy of the arithmetic.
    let resolved = gpu.run("probe_msdf_coverage", &probes);
    let expected: Vec<f32> = probes
        .iter()
        .map(|probe| {
            let median = reference_median3([probe.v0[0], probe.v0[1], probe.v0[2]]);
            ((median - 0.5) * probe.v0[3] + 0.5).clamp(0.0, 1.0)
        })
        .collect();
    assert_matches("msdf_coverage", &probes, &resolved, &expected, 1e-6);
}

/// The gradient frame, in Rust: the affine map taking the origin to (0, 0), the
/// primary handle to (1, 0) and the secondary to (0, 1).
///
/// The same frame `dashscene-skia`'s `gradient_frame` builds, which is why the
/// shader must use all three handles: a painter projecting onto the primary
/// axis alone disagrees with the reference for every frame that is not a
/// similarity, and both an elliptical radial and an oblique linear are ordinary
/// in Figma.
fn gradient_local(
    p: [f32; 2],
    origin: [f32; 2],
    primary: [f32; 2],
    secondary: [f32; 2],
) -> [f32; 2] {
    let u = [primary[0] - origin[0], primary[1] - origin[1]];
    let v = [secondary[0] - origin[0], secondary[1] - origin[1]];
    let det = u[0] * v[1] - u[1] * v[0];
    if det.abs() <= 1e-20 {
        return [0.0, 0.0];
    }
    let d = [p[0] - origin[0], p[1] - origin[1]];
    [
        (d[0] * v[1] - d[1] * v[0]) / det,
        (u[0] * d[1] - u[1] * d[0]) / det,
    ]
}

#[test]
fn each_gradient_parameterization_uses_the_whole_handle_frame() {
    let gpu = Gpu::new();
    // Deliberately not a similarity: the secondary handle is neither
    // perpendicular to the primary nor the same length. A shader using only the
    // primary axis gives a different answer for every one of these.
    let origin = [0.25f32, 0.25];
    let primary = [0.75f32, 0.5];
    // 2.5 times shorter than the primary and 100 degrees from it, so the frame
    // is a clear ellipse rather than a near-circle. The first handles tried
    // here were 78.7 degrees apart with lengths 0.559 and 0.570 — close enough
    // to a similarity that the guard at the end of this test caught them, which
    // is what that guard is for.
    let secondary = [0.15f32, 0.45];
    let points: [[f32; 2]; 9] = [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.5],
        [1.0, 1.0],
        [0.1, 0.9],
        [0.9, 0.1],
        [0.5, 0.25],
        [0.25, 0.75],
    ];
    // Two frames, not one. With a single origin and primary handle a shader
    // that ignored its arguments and used the fixture's literals would pass —
    // the uniform-fixture defect one level down, in the arguments.
    let frames = [
        (origin, primary, secondary),
        ([0.6f32, 0.1], [0.2f32, 0.9], [0.95f32, 0.6]),
    ];
    let probes: Vec<Probe> = frames
        .iter()
        .flat_map(|&(o, pr, se)| {
            points.iter().map(move |&p| Probe {
                v0: [pr[0], pr[1], se[0], se[1]],
                p,
                q: o,
                ..Probe::default()
            })
        })
        .collect();
    let local: Vec<[f32; 2]> = probes
        .iter()
        .map(|probe| {
            gradient_local(
                probe.p,
                probe.q,
                [probe.v0[0], probe.v0[1]],
                [probe.v0[2], probe.v0[3]],
            )
        })
        .collect();

    let linear: Vec<f32> = local.iter().map(|l| l[0].clamp(0.0, 1.0)).collect();
    assert_matches(
        "gradient_linear_t",
        &probes,
        &gpu.run("probe_gradient_linear", &probes),
        &linear,
        1e-5,
    );

    let radial: Vec<f32> = local
        .iter()
        .map(|l| (l[0] * l[0] + l[1] * l[1]).sqrt().clamp(0.0, 1.0))
        .collect();
    assert_matches(
        "gradient_radial_t",
        &probes,
        &gpu.run("probe_gradient_radial", &probes),
        &radial,
        1e-5,
    );

    let angular: Vec<f32> = local
        .iter()
        .map(|l| {
            if l[0] == 0.0 && l[1] == 0.0 {
                return 0.0;
            }
            (l[1].atan2(l[0]) / std::f32::consts::TAU + 1.0).fract()
        })
        .collect();
    assert_matches(
        "gradient_angular_t",
        &probes,
        &gpu.run("probe_gradient_angular", &probes),
        &angular,
        1e-5,
    );

    let diamond: Vec<f32> = local
        .iter()
        .map(|l| (l[0].abs() + l[1].abs()).clamp(0.0, 1.0))
        .collect();
    assert_matches(
        "gradient_diamond_t",
        &probes,
        &gpu.run("probe_gradient_diamond", &probes),
        &diamond,
        1e-5,
    );

    // The frame is load-bearing, stated as a property rather than left to the
    // comparisons above: with this secondary handle a radial is an ellipse, so
    // two points equidistant from the origin must give different values.
    let radial_at = |p: [f32; 2]| {
        let l = gradient_local(p, origin, primary, secondary);
        (l[0] * l[0] + l[1] * l[1]).sqrt()
    };
    let a = radial_at([0.55, 0.25]);
    let b = radial_at([0.25, 0.55]);
    assert!(
        (a - b).abs() > 0.1,
        "the fixture's frame must not be a similarity, or dropping the secondary \
         handle would pass: {a} vs {b}"
    );
}

/// One gradient stop, as the ramp fixture authors it: an offset and a colour.
type Stop = (f32, [f32; 4]);

/// One stop list and the `t` values probed against it.
type RampCase = (Vec<Stop>, Vec<f32>);

/// The stop ramp, derived independently of the shader's walk.
///
/// The shader keeps the *last* segment `t` has entered, overwriting as it goes.
/// This finds the *first* stop past `t` and interpolates the segment before it,
/// with the two clamped ends stated as their own cases. Both reach the same
/// answer for every ordered stop list, including one with a repeated offset —
/// which is the point of writing the second one rather than transliterating.
///
/// `dashscene-skia` builds every gradient with `TileMode::Clamp`, which is what
/// the two end cases are.
///
/// # The two ends are not symmetric, and that is the ramp's own rule
///
/// The lower clamp is **strict** and the upper one is not. Both sides of a
/// hard stop are reachable at the same `t`, and the ramp is right-continuous:
/// the colour *at* a repeated offset is the later stop's. So a `t` equal to the
/// first offset must not short-circuit to the first colour — with the first two
/// stops repeated, the answer is the second colour, and `<=` here would have
/// disagreed with the shader at exactly that point.
///
/// The upper clamp stays inclusive for the same reason read the other way: at a
/// `t` equal to the last offset, the last colour is the later of whatever pair
/// meets there.
fn reference_ramp(t: f32, stops: &[Stop]) -> [f32; 4] {
    let (first_offset, first_colour) = stops[0];
    let (last_offset, last_colour) = stops[stops.len() - 1];
    if t < first_offset {
        return first_colour;
    }
    if t >= last_offset {
        return last_colour;
    }
    let above = stops
        .iter()
        .position(|&(offset, _)| offset > t)
        .expect("t is below the last stop, so some stop is above it");
    let (lo, lo_colour) = stops[above - 1];
    let (hi, hi_colour) = stops[above];
    // `above` is the first stop past `t` and `t` is past `lo`, so this segment
    // has width — a repeated offset is never the divisor here, which is the
    // shape difference from the shader's form.
    let u = (t - lo) / (hi - lo);
    let mut out = [0.0; 4];
    for channel in 0..4 {
        out[channel] = lo_colour[channel] + (hi_colour[channel] - lo_colour[channel]) * u;
    }
    out
}

/// The eight-slot arrays one probe hands `gradient_ramp`, with everything past
/// the stop count set to a value that cannot be mistaken for a colour.
///
/// A slot past the count is one the function must not read. Filling the tail
/// with a colour of nines and an offset of -100 makes reading one *loud*: the
/// offset compares true against every `t`, so an over-running walk mixes the
/// nines in and the measurement leaves the tolerance by three orders of
/// magnitude. Zeroes there would have been indistinguishable from a black stop.
fn ramp_slots(stops: &[Stop]) -> ([f32; 8], [[f32; 4]; 8]) {
    let mut offsets = [-100.0f32; 8];
    let mut colours = [[9.0f32; 4]; 8];
    for (slot, &(offset, colour)) in stops.iter().enumerate() {
        offsets[slot] = offset;
        colours[slot] = colour;
    }
    (offsets, colours)
}

/// The stop ramp gives the authored colour at every stop, interpolates between
/// them, and clamps outside them.
///
/// # The cases, and why each is here
///
/// Six stop lists, no two of which agree in their count, their offsets or their
/// colours — the uniform-fixture rule, applied to the argument that is an
/// array. Issue #715 names the last three by hand:
///
/// - **A range that starts at 0 and ends at 1**, the ordinary case.
/// - **A range that starts above 0 and ends below 1**, which a producer authors
///   whenever it moves a handle instead of a stop. Both clamps are reachable
///   inside it.
/// - **Three uneven stops**, so the segment widths differ and a walk that
///   assumed even spacing is wrong everywhere but the ends.
/// - **One stop**, where there is no segment at all and every `t` gives the
///   same colour.
/// - **A hard stop** — two stops at one offset, which Figma produces for a
///   banded gradient. `gradient_segment_t` has a zero-width segment to answer
///   for, and the probes straddle it at 0.49, 0.50 and 0.51.
/// - **Eight stops**, the vocabulary's ceiling
///   (`dashpaint::MAX_GRADIENT_STOPS`), so the walk's upper bound is exercised
///   rather than assumed.
#[test]
fn the_stop_ramp_interpolates_between_stops_and_clamps_outside_them() {
    let gpu = Gpu::new();

    // Every colour below is distinct and no two share a channel pattern, so a
    // ramp that mixed the right pair in the wrong channels fails.
    let cases: Vec<RampCase> = vec![
        (
            vec![(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.25, 1.0, 0.5])],
            vec![0.0, 0.25, 0.5, 0.75, 1.0],
        ),
        (
            vec![(0.25, [0.0, 0.8, 0.2, 1.0]), (0.75, [0.9, 0.1, 0.7, 0.25])],
            vec![0.0, 0.1, 0.25, 0.4, 0.5, 0.75, 0.9, 1.0],
        ),
        (
            vec![
                (0.1, [0.3, 0.05, 0.95, 0.6]),
                (0.4, [0.7, 0.55, 0.15, 0.9]),
                (0.95, [0.05, 0.65, 0.45, 0.2]),
            ],
            vec![0.0, 0.1, 0.25, 0.4, 0.7, 0.95, 1.0],
        ),
        (
            vec![(0.6, [0.42, 0.17, 0.83, 0.35])],
            vec![0.0, 0.3, 0.6, 0.9, 1.0],
        ),
        (
            vec![
                (0.0, [1.0, 1.0, 0.0, 1.0]),
                (0.5, [0.0, 1.0, 1.0, 0.75]),
                (0.5, [1.0, 0.0, 1.0, 0.5]),
                (1.0, [0.2, 0.4, 0.6, 0.8]),
            ],
            vec![0.0, 0.25, 0.49, 0.5, 0.51, 0.75, 1.0],
        ),
        (
            (0..8)
                .map(|i| {
                    let f = i as f32;
                    (
                        f / 7.0,
                        [f / 7.0, 1.0 - f / 9.0, (f * 0.13) % 1.0, 0.1 + f / 10.0],
                    )
                })
                .collect(),
            vec![0.0, 0.05, 0.3, 0.5, 0.6, 0.85, 1.0],
        ),
        // A hard stop on the **last** pair. The case above puts its repeated
        // offset in the middle, where the segment after it overwrites whatever
        // the zero-width one produced — so nothing there depends on
        // `gradient_segment_t`'s answer for a zero-width segment, and mutation
        // testing is what said so. Here the zero-width segment is the last one
        // the walk visits, and its answer is the result.
        (
            vec![
                (0.0, [0.15, 0.85, 0.35, 1.0]),
                (0.5, [0.95, 0.45, 0.05, 0.9]),
                (1.0, [0.25, 0.15, 0.75, 0.6]),
                (1.0, [0.65, 0.35, 0.95, 0.3]),
            ],
            vec![0.0, 0.5, 0.99, 1.0],
        ),
        // Both stops at one offset: a ramp with no width anywhere, which is the
        // smallest thing a hard stop can be. Below the offset the first colour,
        // at it and above it the second — the right-continuity the ramp is
        // stated on, with no interior segment to reach it through.
        (
            vec![
                (0.4, [0.05, 0.55, 0.95, 0.45]),
                (0.4, [0.85, 0.25, 0.15, 0.95]),
            ],
            vec![0.0, 0.39, 0.4, 0.41, 1.0],
        ),
    ];

    let mut probes = Vec::new();
    let mut table: Vec<[f32; 4]> = Vec::new();
    let mut expected: Vec<f32> = Vec::new();
    for (stops, samples) in &cases {
        let (offsets, colours) = ramp_slots(stops);
        let base = table.len() as f32;
        table.extend_from_slice(&colours);
        for &t in samples {
            let want = reference_ramp(t, stops);
            for channel in 0..4u32 {
                probes.push(Probe {
                    v0: [offsets[0], offsets[1], offsets[2], offsets[3]],
                    v1: [offsets[4], offsets[5], offsets[6], offsets[7]],
                    p: [t, stops.len() as f32],
                    q: [channel as f32, base],
                });
                expected.push(want[channel as usize]);
            }
        }
    }

    // A gradient with no stops at all, stated as its own case because
    // `reference_ramp` has no first colour to return for it.
    //
    // `PaintTable::intern_fill` will hand out a `StopRange` of count zero — it
    // leaves the "at least one stop" rule to `dashscene-validator`, which
    // reports it as `paint.gradient.no-stops` (P4). So the value is reachable
    // from an unvalidated document, and the answer is transparent: there is no
    // colour to clamp to, and drawing nothing is the one answer that cannot
    // paint a wrong one. `dashscene-skia` panics on the same input, which is a
    // divergence this records rather than hides.
    //
    // The probe's table slots hold the fixture's poison nines, so a walk that
    // ran once regardless of the count reads 9.0 rather than 0.0 and this case
    // is what fails.
    let empty_base = table.len() as f32;
    table.extend_from_slice(&[[9.0; 4]; 8]);
    for channel in 0..4u32 {
        probes.push(Probe {
            v0: [-100.0; 4],
            v1: [-100.0; 4],
            p: [0.5, 0.0],
            q: [channel as f32, empty_base],
        });
        expected.push(0.0);
    }

    let measured = gpu.run_with("probe_gradient_ramp", &probes, &table);
    assert_matches("gradient_ramp", &probes, &measured, &expected, 1e-6);

    // The fixture has to be able to fail, and two properties say it can. The
    // ramp must actually vary — a function returning its first colour for every
    // `t` would pass a fixture whose probes were all clamped — and the hard stop
    // must be a step, which is the case a division would have made a NaN of.
    let varies = expected
        .iter()
        .any(|value| (value - expected[0]).abs() > 0.25);
    assert!(varies, "the probe set must reach more than one colour");
    let hard = &cases[4];
    let below = reference_ramp(0.49, &hard.0);
    let above = reference_ramp(0.51, &hard.0);
    assert!(
        (below[0] - above[0]).abs() > 0.5,
        "the hard-stop fixture must step across its repeated offset: {below:?} to {above:?}"
    );
}

#[test]
fn a_stroke_band_sits_where_its_alignment_puts_it() {
    let gpu = Gpu::new();
    let width = 4.0f32;
    let aa = 0.0f32; // a hard edge, so the band's extent is exact
    let distances: Vec<f32> = (-80..=80).map(|i| i as f32 / 10.0).collect();
    let mut probes = Vec::new();
    for align in [0.0f32, 1.0, 2.0] {
        for &d in &distances {
            probes.push(Probe {
                v1: [d, width, align, aa],
                ..Probe::default()
            });
        }
    }
    // A zero-width stroke covers nothing, whatever else it is given. Without
    // this the guard that says so can be deleted and the suite stays green.
    let zero_width: Vec<Probe> = [-2.0f32, 0.0, 2.0]
        .iter()
        .map(|&d| Probe {
            v1: [d, 0.0, 1.0, 1.0],
            ..Probe::default()
        })
        .collect();
    for (probe, c) in zero_width
        .iter()
        .zip(gpu.run("probe_stroke_coverage", &zero_width))
    {
        assert_eq!(
            c, 0.0,
            "a zero-width stroke covers nothing, got {c} at {probe:?}"
        );
    }
    let measured = gpu.run("probe_stroke_coverage", &probes);

    // The band each alignment covers, as an interval of signed distance,
    // derived from what the alignment means rather than from the shader: an
    // Inside stroke lies within the shape, an Outside stroke without, a Center
    // stroke straddles the outline.
    let band = |align: f32| -> (f32, f32) {
        match align as i32 {
            0 => (-width, 0.0),
            1 => (-width / 2.0, width / 2.0),
            _ => (0.0, width),
        }
    };
    for (index, probe) in probes.iter().enumerate() {
        let (d, align) = (probe.v1[0], probe.v1[2]);
        let (lo, hi) = band(align);
        let want = if d >= lo && d <= hi { 1.0 } else { 0.0 };
        // Skip the two exact endpoints, where a hard edge is a tie.
        if (d - lo).abs() < 1e-6 || (d - hi).abs() < 1e-6 {
            continue;
        }
        assert!(
            (measured[index] - want).abs() < 1e-6,
            "stroke align {align} at distance {d}: got {}, expected {want} (band {lo}..{hi})",
            measured[index]
        );
    }

    // An inside and an outside stroke of the same width cover disjoint sides
    // of the outline — the property that makes `stroke_outset` non-zero for
    // one and zero for the other.
    let inside: Vec<f32> = measured[..distances.len()].to_vec();
    let outside: Vec<f32> = measured[2 * distances.len()..].to_vec();
    for (i, &d) in distances.iter().enumerate() {
        if d.abs() > 1e-6 {
            assert!(
                inside[i] * outside[i] == 0.0,
                "inside and outside strokes overlap at {d}"
            );
        }
    }
}

/// The device the suite ran on, recorded on stdout.
///
/// Layer 2's claim is that this arithmetic is stable across adapters, and a
/// number with no adapter beside it cannot be compared against the next run's.
/// Printed rather than asserted: the suite must pass on every adapter, so
/// pinning one would be a different, weaker claim.
#[test]
fn the_adapter_is_recorded() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect(
        "layer-2 conformance needs a wgpu adapter and found none. On a runner this means no \
         software device is available; the test job installs mesa-vulkan-drivers.",
    );
    let info = adapter.get_info();
    println!(
        "layer-2 adapter: {} | backend {:?} | device_type {:?} | driver {} {}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );
}

// ---------------------------------------------------------------------------
// The blurred rounded box, and the measurement story #579 asks for
// ---------------------------------------------------------------------------

/// The Gauss error function in double precision, by Simpson integration of its
/// own definition.
///
/// The reference `erf_approx` is measured against. Deliberately the definition
/// rather than another approximation: `erf(x) = 2/sqrt(pi) * integral of
/// exp(-t^2) from 0 to x`, integrated with 256 intervals. Simpson's error is of
/// order `h^4`: over the range these tests use that is below 1e-9, six orders
/// under the 1e-3 the shader's approximation is held to. The integrand is
/// positive and bounded, so nothing cancels. A rational approximation would
/// have been faster and would have made the comparison one fit against
/// another.
fn reference_erf(x: f64) -> f64 {
    if x < 0.0 {
        return -reference_erf(-x);
    }
    if x == 0.0 {
        return 0.0;
    }
    const N: usize = 256; // even, as Simpson requires
    let h = x / N as f64;
    let f = |t: f64| (-t * t).exp();
    let mut sum = f(0.0) + f(x);
    for i in 1..N {
        let t = h * i as f64;
        sum += f(t) * if i % 2 == 0 { 2.0 } else { 4.0 };
    }
    (sum * h / 3.0) * 2.0 / std::f64::consts::PI.sqrt()
}

/// Half the rounded box's horizontal extent at height `y` on the side `radius`
/// belongs to — the same geometry the shader's `half_extent_at` computes, in
/// double precision, for the quadrature below.
fn half_extent_at(y: f64, half: [f64; 2], radius: f64) -> f64 {
    let over = y.abs() - (half[1] - radius);
    if over <= 0.0 {
        half[0]
    } else if over >= radius {
        half[0] - radius
    } else {
        half[0] - radius + (radius * radius - over * over).max(0.0).sqrt()
    }
}

/// The true coverage of a Gaussian-blurred rounded box, to quadrature
/// accuracy.
///
/// Exact in x — at any height the box's cross-section is one interval and a
/// Gaussian's integral over an interval is a difference of error functions —
/// and integrated in y with 512 midpoint steps over the blur's whole support,
/// against the shader's twelve. That is the same decomposition the shader
/// uses, with forty times the samples in y and an integrated erf rather than a
/// fitted one, so what the comparison measures is precisely the cost of the
/// shader's quadrature and its approximation.
///
/// The step counts are set by what the comparison needs, not by what is
/// impressive: at 4000 steps and 2000 Simpson intervals the suite took 159
/// seconds, which no tier should carry, and the measured numbers were identical
/// to three significant figures.
fn reference_blurred_rounded_box(p: [f32; 2], half: [f32; 2], radii: [f32; 4], sigma: f32) -> f32 {
    let (px, py) = (p[0] as f64, p[1] as f64);
    let half = [half[0] as f64, half[1] as f64];
    let r: Vec<f64> = radii.iter().map(|&v| v as f64).collect();
    let sigma = sigma as f64;
    let inv = 1.0 / (sigma * std::f64::consts::SQRT_2);

    const N: usize = 512;
    let low = (py - 6.0 * sigma).max(-half[1]);
    let high = (py + 6.0 * sigma).min(half[1]);
    if high <= low {
        return 0.0;
    }
    let step = (high - low) / N as f64;
    let norm = 1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    let mut total = 0.0;
    for i in 0..N {
        let y = low + step * (i as f64 + 0.5);
        let top = y < 0.0;
        let left_r = if top { r[0] } else { r[3] };
        let right_r = if top { r[1] } else { r[2] };
        let left = -half_extent_at(y, half, left_r);
        let right = half_extent_at(y, half, right_r);
        let across = 0.5 * (reference_erf((right - px) * inv) - reference_erf((left - px) * inv));
        let dy = y - py;
        let w = norm * (-0.5 * (dy / sigma).powi(2)).exp();
        total += across * w * step;
    }
    total.clamp(0.0, 1.0) as f32
}

#[test]
fn the_error_function_approximation_is_within_its_stated_accuracy() {
    let gpu = Gpu::new();
    let xs: Vec<f32> = (-400..=400).map(|i| i as f32 / 100.0).collect();
    let probes: Vec<Probe> = xs
        .iter()
        .map(|&x| Probe {
            v1: [x, 0.0, 0.0, 0.0],
            ..Probe::default()
        })
        .collect();
    let measured = gpu.run("probe_erf", &probes);

    let mut worst = 0.0f64;
    let mut at = 0.0f32;
    for (&x, &got) in xs.iter().zip(&measured) {
        let want = reference_erf(x as f64);
        let err = (got as f64 - want).abs();
        if err > worst {
            worst = err;
            at = x;
        }
    }
    println!("erf_approx: worst absolute error {worst:.3e} at x = {at}");
    // The constants are fitted, not derived, so this is a measurement with a
    // budget rather than an identity. 1e-3 is an order of magnitude above what
    // it measures, and far below the 1/255 an eight-bit channel can express.
    assert!(
        worst < 1e-3,
        "erf_approx is off by {worst:.3e} at x = {at}, past the 1e-3 this shader is trusted within"
    );
}

/// The twelve-row quadrature costs less than an eight-bit channel can show.
///
/// This is the measurement story #579 asks for before the closed form is
/// trusted: the shader's twelve midpoint rows and its fitted erf, against a
/// 512-row quadrature with an integrated erf over the same decomposition.
/// `docs/decisions/shader-library-and-layer-2.md` D4 carries the table behind
/// the choice of twelve.
/// The budget is 1/255 — one code point of an eight-bit channel — because a
/// shadow that is within one code point of the truth cannot be told from it in
/// the output the goldens compare.
#[test]
fn the_blurred_rounded_box_is_within_one_code_point_of_a_fine_quadrature() {
    let gpu = Gpu::new();
    let half = [60.0f32, 40.0];
    let cases: [([f32; 4], f32); 4] = [
        ([0.0, 0.0, 0.0, 0.0], 8.0),      // a sharp rectangle
        ([16.0, 16.0, 16.0, 16.0], 8.0),  // a uniform corner
        ([30.0, 4.0, 12.0, 0.0], 8.0),    // four different radii
        ([16.0, 16.0, 16.0, 16.0], 24.0), // a blur wider than the corner
    ];
    let mut probes = Vec::new();
    for (radii, sigma) in cases {
        // A grid across the box, its edges, its corners and well outside it.
        for iy in -6..=6 {
            for ix in -8..=8 {
                probes.push(Probe {
                    v0: radii,
                    v1: [sigma, 0.0, 0.0, 0.0],
                    p: [ix as f32 * 10.0, iy as f32 * 10.0],
                    q: half,
                });
            }
        }
    }
    let measured = gpu.run("probe_blurred_rounded_box", &probes);

    let mut worst = 0.0f32;
    let mut worst_probe = probes[0];
    let mut sum = 0.0f64;
    for (probe, &got) in probes.iter().zip(&measured) {
        let want = reference_blurred_rounded_box(probe.p, probe.q, probe.v0, probe.v1[0]);
        let err = (got - want).abs();
        sum += err as f64;
        if err > worst {
            worst = err;
            worst_probe = *probe;
        }
    }
    let mean = sum / probes.len() as f64;
    println!(
        "blurred_rounded_box over {} probes: worst {worst:.5} ({:.2} code points), mean {mean:.5}",
        probes.len(),
        worst * 255.0
    );
    println!("  worst at {worst_probe:?}");
    assert!(
        worst < 1.0 / 255.0,
        "the twelve-row quadrature is off by {worst:.5} ({:.2} code points of 255) at \
         {worst_probe:?}; story #579 adopts it only while it stays inside one code point",
        worst * 255.0
    );
}

/// Well inside the shape the blurred coverage is one, and well outside it is
/// zero — the property a shadow has to have before its accuracy matters.
#[test]
fn a_blurred_box_saturates_inside_and_vanishes_outside() {
    let gpu = Gpu::new();
    let half = [60.0f32, 40.0];
    let radii = [16.0f32, 16.0, 16.0, 16.0];
    let sigma = 6.0f32;
    let probes = vec![
        Probe {
            v0: radii,
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [0.0, 0.0],
            q: half,
        },
        Probe {
            v0: radii,
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [200.0, 0.0],
            q: half,
        },
        Probe {
            v0: radii,
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [0.0, 200.0],
            q: half,
        },
        Probe {
            v0: radii,
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [-60.0, 0.0],
            q: half,
        },
    ];
    let measured = gpu.run("probe_blurred_rounded_box", &probes);
    // Tight enough to notice one missing Gaussian tail. The support is clipped
    // to three sigma either side, and the rows beyond it are still inside a box
    // this tall, so dropping either tail correction costs about 0.0014 here —
    // above this threshold and below a looser one. Mutation-tested: removing
    // the lower tail from the shader fails this line and nothing else.
    assert!(
        measured[0] > 0.9995,
        "the centre is fully covered, got {}",
        measured[0]
    );
    assert!(
        measured[1] < 1e-4,
        "far to the right is clear, got {}",
        measured[1]
    );
    assert!(
        measured[2] < 1e-4,
        "far below is clear, got {}",
        measured[2]
    );
    assert!(
        (measured[3] - 0.5).abs() < 0.05,
        "the middle of a straight edge is about half covered, got {}",
        measured[3]
    );
}

/// The fitted error function saturates instead of overflowing.
///
/// Its polynomial grows as `t^7`, so `y * y` overflowed f32 near |x| = 962 and
/// `y / sqrt(1 + y*y)` collapsed to zero — and then to NaN. The argument is
/// clamped to the range where the true erf is 1 to within 1.5e-8. Measured on
/// the GPU, not reasoned about: the values below came back as 1, 0, 0 and NaN
/// before the clamp.
#[test]
fn the_error_function_saturates_rather_than_overflowing() {
    let gpu = Gpu::new();
    let xs = [4.0f32, 100.0, 900.0, 962.0, 1164.0, 1.0e6, -1.0e6, 3.0e38];
    let probes: Vec<Probe> = xs
        .iter()
        .map(|&x| Probe {
            v1: [x, 0.0, 0.0, 0.0],
            ..Probe::default()
        })
        .collect();
    for (&x, &got) in xs.iter().zip(&gpu.run("probe_erf", &probes)) {
        assert!(got.is_finite(), "erf_approx({x}) is {got}");
        let want = if x < 0.0 { -1.0 } else { 1.0 };
        assert!(
            (got - want).abs() < 1e-3,
            "erf_approx({x}) is {got}, and erf saturates at {want}"
        );
    }
}

/// A hairline blur on a full-width element keeps its shadow.
///
/// The consequence of the overflow above, and the reason it mattered: the erf
/// argument is `(edge - x) / (sigma * sqrt 2)`, so a wide box with a small
/// sigma pushed it past the breakdown and both erfs returned zero. The centre
/// of the shape then reported zero coverage. Half-width 720 is a full-width
/// element on the 1440 hero canvas and sigma 0.4375 is a Figma blur radius of 1
/// (`docs/decisions/blur-sigma-is-figmas-mapping.md`).
#[test]
fn a_hairline_blur_on_a_wide_element_still_covers_it() {
    let gpu = Gpu::new();
    let half = [720.0f32, 20.0];
    let sigma = 0.4375f32;
    let probes = vec![
        Probe {
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [0.0, 0.0],
            q: half,
            ..Probe::default()
        },
        Probe {
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [-700.0, 0.0],
            q: half,
            ..Probe::default()
        },
        Probe {
            v1: [sigma, 0.0, 0.0, 0.0],
            p: [900.0, 0.0],
            q: half,
            ..Probe::default()
        },
    ];
    // A zero blur is the unblurred shape. Without this the guard saying so can
    // be deleted and the suite stays green — it was, until this was added.
    let sharp: Vec<Probe> = [[0.0f32, 0.0], [1000.0, 0.0]]
        .iter()
        .map(|&pt| Probe {
            v1: [0.0, 0.0, 0.0, 0.0],
            p: pt,
            q: half,
            ..Probe::default()
        })
        .collect();
    let got_sharp = gpu.run("probe_blurred_rounded_box", &sharp);
    assert_eq!(got_sharp[0], 1.0, "a zero blur covers the inside exactly");
    assert_eq!(got_sharp[1], 0.0, "a zero blur covers nothing outside");

    let got = gpu.run("probe_blurred_rounded_box", &probes);
    assert!(
        got[0] > 0.99,
        "the centre of a wide thin box is covered, got {}",
        got[0]
    );
    assert!(
        got[1] > 0.99,
        "well inside the left end is covered, got {}",
        got[1]
    );
    assert!(got[2] < 1e-4, "well outside is clear, got {}", got[2]);
}

/// An oversized corner radius is scaled to fit, as Skia scales it.
///
/// Figma authors a pill as `cornerRadius: 9999` and `dashc` passes it through;
/// `dashscene-skia` relies on Skia clamping. Nothing clamped it here, and the
/// rounded-box form has no meaning above half the box: a 50x30 half-box with
/// radii 9999 read about 4085 units *outside* at every point, so the painter
/// drew nothing where the reference draws a pill.
#[test]
fn an_oversized_corner_radius_is_scaled_to_fit() {
    let gpu = Gpu::new();

    // The scaling itself, component by component.
    let mut probes = Vec::new();
    let cases: [([f32; 2], [f32; 4]); 3] = [
        ([50.0, 30.0], [9999.0, 9999.0, 9999.0, 9999.0]),
        ([20.0, 20.0], [30.0, 30.0, 30.0, 30.0]),
        ([50.0, 30.0], [10.0, 4.0, 16.0, 8.0]), // already fits: unchanged
    ];
    for (half, radii) in cases {
        for which in 0..4u32 {
            probes.push(Probe {
                v0: radii,
                v1: [which as f32, 0.0, 0.0, 0.0],
                q: half,
                ..Probe::default()
            });
        }
    }
    let got = gpu.run("probe_clamp_radii", &probes);
    let expected: Vec<f32> = probes
        .iter()
        .map(|probe| reference_clamp_radii(probe.q, probe.v0)[probe.v1[0] as usize])
        .collect();
    assert_matches("clamp_radii", &probes, &got, &expected, 1e-5);

    // A pill: every point inside the box is inside the shape, and the distance
    // agrees with the independently sampled outline of the clamped shape.
    let half = [50.0f32, 30.0];
    let pill: Vec<Probe> = [[0.0f32, 0.0], [40.0, 0.0], [0.0, 25.0], [-40.0, 0.0]]
        .iter()
        .map(|&p| Probe {
            v0: [9999.0, 9999.0, 9999.0, 9999.0],
            p,
            q: half,
            ..Probe::default()
        })
        .collect();
    let got = gpu.run("probe_rounded_box_sdf", &pill);
    for (probe, &d) in pill.iter().zip(&got) {
        assert!(
            d < 0.0,
            "a point inside a pill is inside it, got {d} at {:?}",
            probe.p
        );
        let want = reference_rounded_box_sdf(probe.p, probe.q, probe.v0);
        assert!(
            (d - want).abs() < 0.02,
            "pill distance at {:?}: shader {d}, reference {want}",
            probe.p
        );
    }
}

/// A stroke narrower than the antialiasing width covers what it should.
///
/// Folding the distance and taking one ramp saturates as soon as the fold
/// passes the ramp's centre, so a hairline painted far too opaque: a 0.25-unit
/// Center stroke on the outline measured 0.625 where 0.25 is correct. The band
/// is the difference of its two edge ramps.
#[test]
fn a_stroke_narrower_than_the_antialiasing_width_is_not_overdrawn() {
    let gpu = Gpu::new();
    let aa = 1.0f32;
    let widths = [0.125f32, 0.25, 0.5, 1.0, 2.0, 4.0];
    let probes: Vec<Probe> = widths
        .iter()
        .map(|&w| Probe {
            v1: [0.0, w, 1.0, aa], // on the outline, Center aligned
            ..Probe::default()
        })
        .collect();
    let got = gpu.run("probe_stroke_coverage", &probes);
    for (&w, &c) in widths.iter().zip(&got) {
        // A band of width w under a linear ramp of width aa, centred on the
        // sample, covers min(w / aa, 1).
        let want = (w / aa).min(1.0);
        assert!(
            (c - want).abs() < 1e-5,
            "a {w}-wide Center stroke at aa {aa} covers {want}, got {c}"
        );
    }
}
