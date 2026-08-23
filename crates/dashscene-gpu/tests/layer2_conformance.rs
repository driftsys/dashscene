//! Layer 2 of epic #569's verification net: the shader library's arithmetic,
//! evaluated on a real device by compute shader and checked against a
//! committed table of expectations.
//!
//! # The expectations are a file, not this suite
//!
//! `conformance/layer2-probes.json` holds the inputs, the expected values and
//! the tolerances for every function `shaders/conformance.wgsl` has a probe
//! entry point for. **Nothing recomputes the table's expectations to judge the
//! shader against** — that is what the file is for, and it is what issue #828
//! asks for. The property tests at the end do derive their own expectations,
//! from what a claim *means* rather than from the table:
//! `a_stroke_band_sits_where_its_alignment_puts_it` takes the band from
//! `reference_stroke_coverage`, and two others use derived constants. The
//! distinction is which claim is being made, not whether arithmetic happens.
//! For the table: a second painter implementing the same math has
//! something to run, rather than a description of a suite written in Rust.
//!
//! The references below still run, in the `the_recorded_*` tests, and they run
//! against the same file rather than against the shader. That is the point of
//! them and it is not the same thing as computing an expectation at judging
//! time.
//!
//! **Five checks hold the file**, the same list
//! `docs/design/dashscene-gpu.md` gives:
//!
//! - [`the_shader_matches_the_committed_probe_table`] runs the WGSL against
//!   the file. This is the test a second painter ports.
//! - One `the_recorded_*` test per function runs the Rust references below
//!   against the same file, on no device at all. This is what says the file is
//!   right, and it is why the references stay in the tree after the file was
//!   recorded from them.
//! - [`the_committed_table_matches_the_specs_that_recorded_it`] holds the
//!   file's inputs and metadata against `CASE_SPECS` and [`fixture_args`], so a
//!   change made in the source and never re-recorded fails rather than gating
//!   nothing.
//! - [`every_probe_entry_point_is_reached_by_the_table`] refuses a case that
//!   stopped running.
//! - [`every_function_in_the_shader_library_is_probed_or_named_as_a_helper`]
//!   refuses a function added to the shader library and never probed.
//!
//! [`record_the_probe_table`] is **not** one of them: it writes the file and
//! checks nothing. It is `#[ignore]`d and no tier runs it, which is what stops
//! a consumer regenerating the data from its own implementation and then
//! testing that implementation against itself.
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
//! the shader compiler, not the math. Each expectation in the file is derived
//! independently where that is possible: the rounded-box distance from
//! brute-force sampling of the outline, the median from sorting, the error
//! function from integrating its own definition, the blurred box from a
//! 512-row quadrature.
//!
//! Not all of them are, and saying so is the point. `coverage` and the four
//! gradient parameterizations have expectations transliterated from the same
//! expression the shader uses, because those functions *are* their definitions
//! and there is no second derivation to write. They still catch a WGSL-side
//! error, which mutation testing confirms, but they check the transliteration
//! and not the mathematics, and a reader should not think otherwise. Every case
//! in the file carries a `reference` field saying which it is.
//!
//! # What is a table and what stays a property
//!
//! Not everything here is a table, and the rule for which is which is stated
//! in `docs/design/dashscene-gpu.md` under "The probe table, and what stays a
//! property". The short form: a row belongs in the file when the function has
//! one right answer at that input; the claim stays a property when it
//! quantifies over inputs the file does not enumerate, or when the assertion
//! depends on something other than a value at a point. The property tests are
//! at the end of this file, each with the claim it makes in its name.
//!
//! # This suite is shared with the Unity painter
//!
//! R-T5 asks for the SDF math to be single-sourced into both product painters'
//! shading languages. The functions under test come from
//! `dashscene_gpu::SDF_WGSL`, the one file the render pipelines also include,
//! so a second painter porting that file has a table to port with it rather
//! than a review promise.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// The committed probe table
// ---------------------------------------------------------------------------

/// The table's path from the repository root, for messages.
const TABLE_PATH: &str = "conformance/layer2-probes.json";

/// The table itself.
///
/// `include_str!` resolves against **this source file**, so the path here has
/// one more `..` than [`table_file`]'s, which resolves against the crate
/// manifest. Baked in rather than read at run time so that a missing file is a
/// compile error and an edited file cannot be read by a stale binary.
const TABLE_JSON: &str = include_str!("../../../conformance/layer2-probes.json");

/// The same file, as a path the recorder can write.
fn table_file() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/layer2-probes.json")
}

/// The whole table.
#[derive(Debug, Serialize, Deserialize)]
struct ProbeTable {
    format: u32,
    about: String,
    shader: String,
    properties: String,
    recorded_by: String,
    functions: Vec<FunctionCase>,
}

/// One function of the shader library, and every probe of it.
#[derive(Debug, Serialize, Deserialize)]
struct FunctionCase {
    /// The function's name in `sdf.wgsl`. Also the key this suite dispatches
    /// on, which is why an unknown one is a failure rather than a skip.
    name: String,
    signature: String,
    /// The names of the positional entries in each probe's `args`, in order.
    arguments: Vec<String>,
    /// `f32` or `vec4f` — how many components each `expected` carries.
    result: String,
    tolerance: f64,
    /// Whether this function's expectation was derived independently of the
    /// shader, or transliterated from the same expression.
    reference: String,
    probes: Vec<FileProbe>,
}

/// One evaluation: its arguments, positionally, and what the function owes.
#[derive(Debug, Serialize, Deserialize)]
struct FileProbe {
    args: Vec<Value>,
    expected: Value,
}

/// Parses the committed table.
fn table() -> ProbeTable {
    let parsed: ProbeTable = serde_json::from_str(TABLE_JSON)
        .unwrap_or_else(|error| panic!("{TABLE_PATH} does not parse: {error}"));
    assert_eq!(
        parsed.format, TABLE_FORMAT,
        "{TABLE_PATH} is format {} and this suite reads format {TABLE_FORMAT}",
        parsed.format
    );
    assert!(
        !parsed.functions.is_empty(),
        "{TABLE_PATH} names no function, so a green run would establish nothing"
    );
    parsed
}

impl FunctionCase {
    /// How many floats one probe of this function produces.
    fn components(&self) -> usize {
        match self.result.as_str() {
            "f32" => 1,
            "vec4f" => 4,
            other => panic!("{}: unknown result type {other}", self.name),
        }
    }

    /// Every expected value, flattened in probe order.
    fn expected(&self) -> Vec<f64> {
        let components = self.components();
        let mut out = Vec::with_capacity(self.probes.len() * components);
        for (index, probe) in self.probes.iter().enumerate() {
            match (&probe.expected, components) {
                (Value::Number(_), 1) => out.push(number(&probe.expected)),
                (Value::Array(values), 4) if values.len() == 4 => {
                    out.extend(values.iter().map(number));
                }
                _ => panic!(
                    "{} probe {index}: expected {} component(s), got {}",
                    self.name, components, probe.expected
                ),
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Reading one probe's arguments
// ---------------------------------------------------------------------------

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} is not a number"))
}

fn scalar(args: &[Value], index: usize) -> f32 {
    number(&args[index]) as f32
}

fn floats(args: &[Value], index: usize, count: usize) -> Vec<f32> {
    let Value::Array(values) = &args[index] else {
        panic!("argument {index} is {}, not an array", args[index]);
    };
    assert_eq!(
        values.len(),
        count,
        "argument {index} has {} entries, not {count}",
        values.len()
    );
    values.iter().map(|v| number(v) as f32).collect()
}

fn vec2(args: &[Value], index: usize) -> [f32; 2] {
    let v = floats(args, index, 2);
    [v[0], v[1]]
}

fn vec3(args: &[Value], index: usize) -> [f32; 3] {
    let v = floats(args, index, 3);
    [v[0], v[1], v[2]]
}

fn vec4(args: &[Value], index: usize) -> [f32; 4] {
    let v = floats(args, index, 4);
    [v[0], v[1], v[2], v[3]]
}

/// An argument that is itself an array of four-float colours.
fn vec4_list(args: &[Value], index: usize, count: usize) -> Vec<[f32; 4]> {
    let Value::Array(values) = &args[index] else {
        panic!("argument {index} is {}, not an array", args[index]);
    };
    assert_eq!(
        values.len(),
        count,
        "argument {index} has {} entries, not {count}",
        values.len()
    );
    values
        .iter()
        .map(|row| {
            let Value::Array(row) = row else {
                panic!("{row} is not a four-float colour");
            };
            assert_eq!(row.len(), 4, "{row:?} is not a four-float colour");
            [
                number(&row[0]) as f32,
                number(&row[1]) as f32,
                number(&row[2]) as f32,
                number(&row[3]) as f32,
            ]
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The compute harness
// ---------------------------------------------------------------------------

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
        // Seeded with a sentinel rather than left zero-initialised, so that a
        // probe the dispatch never reached is a failure rather than a zero.
        //
        // wgpu zero-fills a new buffer, and zero is a legitimate answer for
        // most of these functions — a clear sample's coverage, a gradient at
        // its origin, a median of three zeroes. So a wrong workgroup count, or
        // an entry point whose bounds test rejected the tail, would leave the
        // trailing results reading as correct. A quiet NaN with a recognisable
        // payload cannot be produced by any of this arithmetic and cannot be
        // confused with one the shader computed itself, which the check after
        // the readback relies on.
        let result_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("results"),
                contents: bytemuck::cast_slice(&vec![UNWRITTEN; probes.len()]),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
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

        // Every probe was reached. Compared by bit pattern, so a NaN the shader
        // computed for itself is reported by the caller's own comparison rather
        // than as a probe that did not run.
        let unwritten: Vec<usize> = out
            .iter()
            .enumerate()
            .filter(|(_, value)| value.to_bits() == UNWRITTEN.to_bits())
            .map(|(index, _)| index)
            .collect();
        assert!(
            unwritten.is_empty(),
            "{entry} left {} of {} result(s) unwritten — the dispatch did not reach \
             probe(s) {:?}{}",
            unwritten.len(),
            out.len(),
            &unwritten[..unwritten.len().min(8)],
            if unwritten.len() > 8 { " …" } else { "" }
        );
        out
    }
}

/// What a result slot holds before the shader writes it.
///
/// A quiet NaN carrying `0xbeef`, so that "the dispatch never reached this
/// probe" is distinguishable from every value the shader library can produce,
/// including a NaN of its own.
const UNWRITTEN: f32 = f32::from_bits(0x7fc0_beef);

// ---------------------------------------------------------------------------
// Independent references
//
// These are what recorded `conformance/layer2-probes.json`, and they stay in
// the tree because deleting them after recording would leave nothing able to
// say the file is right. Every one of them is run against the file by its own
// `the_recorded_*` test, on no device at all.
// ---------------------------------------------------------------------------

/// Corner radii scaled down until no edge is over-subscribed.
///
/// The rule Skia applies internally, restated: for each edge, the two radii
/// that meet it may not exceed its length; take the worst ratio and scale all
/// four by it.
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
    // `blurred_rounded_box` clamps before it integrates, and this has to as
    // well. Harmless for the radii recorded today — none over-subscribes a
    // 60x40 half-box — and wrong the moment a blurred pill is recorded, which
    // is the case story #579 existed for and which `rounded_box_sdf`'s own
    // fixtures already carry.
    let radii = reference_clamp_radii(half, radii);
    // A zero blur is the unblurred shape, which is what `blurred_rounded_box`
    // answers. Without this the quadrature below sees `high <= low` and returns
    // 0 for every point, including the inside of the shape.
    if sigma <= 0.0 {
        return if reference_rounded_box_sdf(p, half, radii) <= 0.0 {
            1.0
        } else {
            0.0
        };
    }
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

/// Antialiased coverage for a signed distance — transliterated, because this
/// function is its own definition and there is no second derivation to write.
fn reference_coverage(d: f32, width: f32) -> f32 {
    if width <= 0.0 {
        if d <= 0.0 { 1.0 } else { 0.0 }
    } else {
        (0.5 - d / width).clamp(0.0, 1.0)
    }
}

/// Coverage of a stroke band, from the band's overlap with the pixel footprint.
///
/// Derived from the geometry rather than from the shader's arrangement: the
/// alignment puts the band at a known interval of signed distance, the
/// antialiasing width `aa` makes the sample a footprint of that width centred
/// on `d`, and the coverage is how much of the footprint the band holds. The
/// shader instead takes the difference of the two edge ramps. The two agree
/// identically for a linear ramp, which is the point of writing this one — the
/// arrangement `stroke_coverage` had before story #579 (one ramp of a folded
/// distance) does not, and it is what this catches.
///
/// A zero `aa` is a hard edge, and the band is then **half-open**, `(lo, hi]`:
/// the shader answers 0 at the lower edge and 1 at the upper, because its two
/// ramps are each half-open in the same direction. Measured by dispatching all
/// six endpoints, not inferred. This function spelled the interval closed for a
/// while and the sweep skipped the endpoints rather than choosing, which left a
/// port free to answer 1 where this shader answers 0; both are checked now.
fn reference_stroke_coverage(d: f32, width: f32, align: f32, aa: f32) -> f32 {
    let centre = if align < 0.5 {
        -width * 0.5
    } else if align < 1.5 {
        0.0
    } else {
        width * 0.5
    };
    let (lo, hi) = (centre - width * 0.5, centre + width * 0.5);
    if aa <= 0.0 {
        // Half-open, `(lo, hi]`, because that is what the shader does and it is
        // not symmetric: its two ramps are half-open in the same direction, so
        // a hard edge answers 0 at the lower band edge and 1 at the upper.
        // Measured by dispatching all six endpoints. This read `[lo, hi]` and
        // was called a tie in the prose above it; a second painter porting the
        // closed form would answer 1 where this shader answers 0.
        return if d > lo && d <= hi { 1.0 } else { 0.0 };
    }
    let (near, far) = (d - aa * 0.5, d + aa * 0.5);
    let overlap = (hi.min(far) - lo.max(near)).max(0.0);
    (overlap / aa).clamp(0.0, 1.0)
}

/// The stop list one `gradient_ramp` probe carries, as `reference_ramp` reads
/// it: the first `count` of the eight offsets, paired with their colours.
fn ramp_stops(args: &[Value]) -> Vec<Stop> {
    let offsets = floats(args, 1, MAX_GRADIENT_STOPS);
    let colours = vec4_list(args, 2, MAX_GRADIENT_STOPS);
    let count = scalar(args, 3) as usize;
    assert!(
        count <= MAX_GRADIENT_STOPS,
        "a gradient carries at most {MAX_GRADIENT_STOPS} stops, not {count}"
    );
    (0..count).map(|i| (offsets[i], colours[i])).collect()
}

/// `dashpaint::MAX_GRADIENT_STOPS`, which `sdf.wgsl` declares its own copy of.
const MAX_GRADIENT_STOPS: usize = dashpaint::MAX_GRADIENT_STOPS;

/// What the references say one probe of `function` owes, in the order the
/// table's `expected` carries it.
///
/// The one place the table's expectations are computed. Both the recorder and
/// [`check_against_the_references`] call it, so a reference and the file cannot
/// disagree about which derivation produced a number.
fn reference_value(function: &str, args: &[Value]) -> Vec<f64> {
    match function {
        "clamp_radii" => reference_clamp_radii(vec2(args, 0), vec4(args, 1))
            .iter()
            .map(|&v| v as f64)
            .collect(),
        "rounded_box_sdf" => {
            vec![reference_rounded_box_sdf(vec2(args, 0), vec2(args, 1), vec4(args, 2)) as f64]
        }
        "coverage" => vec![reference_coverage(scalar(args, 0), scalar(args, 1)) as f64],
        "median3" => vec![reference_median3(vec3(args, 0)) as f64],
        "msdf_coverage" => {
            let median = reference_median3(vec3(args, 0));
            let range = scalar(args, 1);
            vec![((median - 0.5) * range + 0.5).clamp(0.0, 1.0) as f64]
        }
        "gradient_linear_t" | "gradient_radial_t" | "gradient_angular_t" | "gradient_diamond_t" => {
            let local = gradient_local(vec2(args, 0), vec2(args, 1), vec2(args, 2), vec2(args, 3));
            let t = match function {
                "gradient_linear_t" => local[0].clamp(0.0, 1.0),
                "gradient_radial_t" => (local[0] * local[0] + local[1] * local[1])
                    .sqrt()
                    .clamp(0.0, 1.0),
                "gradient_angular_t" => {
                    if local[0] == 0.0 && local[1] == 0.0 {
                        0.0
                    } else {
                        (local[1].atan2(local[0]) / std::f32::consts::TAU + 1.0).fract()
                    }
                }
                _ => (local[0].abs() + local[1].abs()).clamp(0.0, 1.0),
            };
            vec![t as f64]
        }
        "gradient_ramp" => {
            let stops = ramp_stops(args);
            // A gradient with no stops has no colour to clamp to, and drawing
            // nothing is the one answer that cannot paint a wrong one.
            // `reference_ramp` has no first colour to return for it, so the
            // case is stated here rather than inside it.
            if stops.is_empty() {
                return vec![0.0; 4];
            }
            reference_ramp(scalar(args, 0), &stops)
                .iter()
                .map(|&v| v as f64)
                .collect()
        }
        "stroke_coverage" => vec![reference_stroke_coverage(
            scalar(args, 0),
            scalar(args, 1),
            scalar(args, 2),
            scalar(args, 3),
        ) as f64],
        // Evaluated at the argument the *shader* sees, not at the decimal the
        // file spells: the claim is that `erf_approx` is within 1e-3 of the
        // error function at its f32 input, so the reference rounds to f32
        // first, as every other arm here does through `scalar`.
        "erf_approx" => vec![reference_erf(scalar(args, 0) as f64)],
        "blurred_rounded_box" => vec![reference_blurred_rounded_box(
            vec2(args, 0),
            vec2(args, 1),
            vec4(args, 2),
            scalar(args, 3),
        ) as f64],
        other => panic!("{TABLE_PATH} names {other}, which no reference in this file computes"),
    }
}

// ---------------------------------------------------------------------------
// Feeding the table's probes to the shader
// ---------------------------------------------------------------------------

/// Names [`PROBED_FUNCTIONS`] and generates one reference test per function.
///
/// A macro rather than one copy per function, for two reasons and the second is
/// the load-bearing one. It keeps the reference check to one body, and it makes
/// the list of functions and the set of tests over them **the same list**, so
/// a function cannot be probed without being checked, or checked without being
/// probed, and [`every_probe_entry_point_is_reached_by_the_table`] holds that
/// one list against the file.
///
/// One test each rather than one test over all of them because the references
/// are where this suite spends its time: the blurred box integrates 512 rows at
/// each of its probes, and the rounded-box distance walks four corner arcs and
/// four edges at 4096 steps each at every one of its own. Run as one test they
/// add up on the sanity tier's critical path; run one per function, nextest
/// overlaps them and the slowest one is the cost.
macro_rules! probed_functions {
    ($($test:ident => $name:literal),* $(,)?) => {
        /// The functions this suite probes, checked against the file both ways
        /// so that a case cannot be lost from either side without a failure.
        const PROBED_FUNCTIONS: &[&str] = &[$($name),*];

        $(
            /// One function's committed expectations, against the reference
            /// that recorded them.
            ///
            /// **This is what says the file is right**, and it is why those
            /// references stay in the tree. It touches no device: a table that
            /// has drifted from the mathematics is a defect whether or not a
            /// GPU is present.
            #[test]
            fn $test() {
                check_against_the_references($name);
            }
        )*
    };
}

probed_functions! {
    the_recorded_clamp_radii_match_the_reference => "clamp_radii",
    the_recorded_rounded_box_distances_match_the_reference => "rounded_box_sdf",
    the_recorded_coverage_matches_the_reference => "coverage",
    the_recorded_medians_match_the_reference => "median3",
    the_recorded_msdf_coverage_matches_the_reference => "msdf_coverage",
    the_recorded_linear_gradients_match_the_reference => "gradient_linear_t",
    the_recorded_radial_gradients_match_the_reference => "gradient_radial_t",
    the_recorded_angular_gradients_match_the_reference => "gradient_angular_t",
    the_recorded_diamond_gradients_match_the_reference => "gradient_diamond_t",
    the_recorded_stop_ramps_match_the_reference => "gradient_ramp",
    the_recorded_stroke_coverage_matches_the_reference => "stroke_coverage",
    the_recorded_error_function_matches_the_reference => "erf_approx",
    the_recorded_blurred_boxes_match_the_reference => "blurred_rounded_box",
}

/// The compute entry point that evaluates one function.
///
/// Split out of [`dispatch`] so that
/// [`every_probe_entry_point_is_reached_by_the_table`] can ask for a name
/// without building every probe in the table and dropping it. `dispatch`
/// asserts its own answer against this one, so the two cannot drift.
fn entry_point_for(function: &str) -> &'static str {
    match function {
        "clamp_radii" => "probe_clamp_radii",
        "rounded_box_sdf" => "probe_rounded_box_sdf",
        "coverage" => "probe_coverage",
        "median3" => "probe_median3",
        "msdf_coverage" => "probe_msdf_coverage",
        "gradient_linear_t" => "probe_gradient_linear",
        "gradient_radial_t" => "probe_gradient_radial",
        "gradient_angular_t" => "probe_gradient_angular",
        "gradient_diamond_t" => "probe_gradient_diamond",
        "gradient_ramp" => "probe_gradient_ramp",
        "stroke_coverage" => "probe_stroke_coverage",
        "erf_approx" => "probe_erf",
        "blurred_rounded_box" => "probe_blurred_rounded_box",
        other => panic!("{TABLE_PATH} names {other}, which this suite cannot dispatch"),
    }
}

/// The compute entry point that evaluates one function, the probes that carry
/// the table's arguments into it, and the stop-colour table it reads.
///
/// One [`Probe`] per expected component: `clamp_radii` and `gradient_ramp`
/// return four floats and their entry points write one, selected by an index
/// the probe carries, so a four-component row becomes four dispatched probes.
fn dispatch(case: &FunctionCase) -> (&'static str, Vec<Probe>, Vec<[f32; 4]>) {
    let mut probes = Vec::with_capacity(case.probes.len() * case.components());
    let mut colours: Vec<[f32; 4]> = Vec::new();
    let entry = match case.name.as_str() {
        "clamp_radii" => {
            for probe in &case.probes {
                for which in 0..4u32 {
                    probes.push(Probe {
                        v0: vec4(&probe.args, 1),
                        v1: [which as f32, 0.0, 0.0, 0.0],
                        q: vec2(&probe.args, 0),
                        ..Probe::default()
                    });
                }
            }
            "probe_clamp_radii"
        }
        "rounded_box_sdf" => {
            for probe in &case.probes {
                probes.push(Probe {
                    v0: vec4(&probe.args, 2),
                    p: vec2(&probe.args, 0),
                    q: vec2(&probe.args, 1),
                    ..Probe::default()
                });
            }
            "probe_rounded_box_sdf"
        }
        "coverage" => {
            for probe in &case.probes {
                probes.push(Probe {
                    v1: [scalar(&probe.args, 0), scalar(&probe.args, 1), 0.0, 0.0],
                    ..Probe::default()
                });
            }
            "probe_coverage"
        }
        "median3" => {
            for probe in &case.probes {
                let s = vec3(&probe.args, 0);
                probes.push(Probe {
                    v0: [s[0], s[1], s[2], 0.0],
                    ..Probe::default()
                });
            }
            "probe_median3"
        }
        "msdf_coverage" => {
            for probe in &case.probes {
                let s = vec3(&probe.args, 0);
                probes.push(Probe {
                    v0: [s[0], s[1], s[2], scalar(&probe.args, 1)],
                    ..Probe::default()
                });
            }
            "probe_msdf_coverage"
        }
        "gradient_linear_t" | "gradient_radial_t" | "gradient_angular_t" | "gradient_diamond_t" => {
            for probe in &case.probes {
                let primary = vec2(&probe.args, 2);
                let secondary = vec2(&probe.args, 3);
                probes.push(Probe {
                    v0: [primary[0], primary[1], secondary[0], secondary[1]],
                    p: vec2(&probe.args, 0),
                    q: vec2(&probe.args, 1),
                    ..Probe::default()
                });
            }
            match case.name.as_str() {
                "gradient_linear_t" => "probe_gradient_linear",
                "gradient_radial_t" => "probe_gradient_radial",
                "gradient_angular_t" => "probe_gradient_angular",
                _ => "probe_gradient_diamond",
            }
        }
        "gradient_ramp" => {
            for probe in &case.probes {
                let offsets = floats(&probe.args, 1, MAX_GRADIENT_STOPS);
                // Each probe's eight colours are appended rather than shared,
                // so one row of the file is one self-contained fixture.
                let base = colours.len() as f32;
                colours.extend(vec4_list(&probe.args, 2, MAX_GRADIENT_STOPS));
                for which in 0..4u32 {
                    probes.push(Probe {
                        v0: [offsets[0], offsets[1], offsets[2], offsets[3]],
                        v1: [offsets[4], offsets[5], offsets[6], offsets[7]],
                        p: [scalar(&probe.args, 0), scalar(&probe.args, 3)],
                        q: [which as f32, base],
                    });
                }
            }
            "probe_gradient_ramp"
        }
        "stroke_coverage" => {
            for probe in &case.probes {
                probes.push(Probe {
                    v1: [
                        scalar(&probe.args, 0),
                        scalar(&probe.args, 1),
                        scalar(&probe.args, 2),
                        scalar(&probe.args, 3),
                    ],
                    ..Probe::default()
                });
            }
            "probe_stroke_coverage"
        }
        "erf_approx" => {
            for probe in &case.probes {
                probes.push(Probe {
                    v1: [scalar(&probe.args, 0), 0.0, 0.0, 0.0],
                    ..Probe::default()
                });
            }
            "probe_erf"
        }
        "blurred_rounded_box" => {
            for probe in &case.probes {
                probes.push(Probe {
                    v0: vec4(&probe.args, 2),
                    v1: [scalar(&probe.args, 3), 0.0, 0.0, 0.0],
                    p: vec2(&probe.args, 0),
                    q: vec2(&probe.args, 1),
                });
            }
            "probe_blurred_rounded_box"
        }
        other => panic!("{TABLE_PATH} names {other}, which this suite cannot dispatch"),
    };
    assert_eq!(
        entry,
        entry_point_for(&case.name),
        "the two spellings of {}'s entry point disagree",
        case.name
    );
    (entry, probes, colours)
}

// ---------------------------------------------------------------------------
// The tests that hold the table
// ---------------------------------------------------------------------------

/// Which of `measured` differ from what `case` records, and by how much.
///
/// The whole comparison, so that a test can drive **this** with a NaN rather
/// than driving the predicate underneath it. An earlier round extracted only
/// `error <= tolerance` and tested that, which pinned the definition and left
/// both call sites free to be re-inlined in the NaN-blind `>` form with every
/// test green.
///
/// **This is a mitigation, not a pin, and the difference is worth stating.**
/// [`a_nan_reaches_the_failure_list`] drives this function; it cannot detect a
/// caller that stops calling it, and a mutation confirmed that re-inlining the
/// loop at the call site still passes. Nothing short of a probe that actually
/// produces a NaN would catch that, and none does — the table records finite
/// expectations for finite arguments. What the extraction buys is that
/// re-inlining is now a visible several-line change deleting a call to a tested
/// function, rather than flipping one character.
fn compare(case: &FunctionCase, measured: &[f64]) -> (Vec<String>, usize) {
    let expected = case.expected();
    let components = case.components();
    let bad = outside(measured, &expected, |_| case.tolerance);
    let mut named = Vec::new();
    for &index in bad.iter().take(REPORTED_FAILURES) {
        let (got, want) = (measured[index], expected[index]);
        let probe = &case.probes[index / components];
        named.push(format!(
            "{} probe {} component {}: shader gave {got}, the table says {want} \
             (difference {:.3e}, tolerance {}) at {}",
            case.name,
            index / components,
            index % components,
            (got - want).abs(),
            case.tolerance,
            describe(case, probe),
        ));
    }
    (named, bad.len())
}

/// A NaN reaches the failure list, through the comparison the suite runs.
///
/// Driving [`compare`] rather than [`outside`]: this is the call the two table
/// tests make, so re-inlining a `>` loop at either of them makes this fail.
#[test]
fn a_nan_reaches_the_failure_list() {
    let case = FunctionCase {
        name: "synthetic".to_owned(),
        signature: "synthetic(x: f32) -> f32".to_owned(),
        arguments: vec!["x".to_owned()],
        result: "f32".to_owned(),
        tolerance: 1e6,
        reference: "a fixture for this test alone".to_owned(),
        probes: (0..3)
            .map(|index| FileProbe {
                args: vec![serde_json::json!(index as f64)],
                expected: serde_json::json!(0.0),
            })
            .collect(),
    };
    let (named, differing) = compare(&case, &[0.0, f64::NAN, 0.0]);
    assert_eq!(
        differing, 1,
        "a NaN measurement is outside a tolerance of 1e6, and `error > tolerance` \
         reports none — which is how a shader returning one passed the whole suite"
    );
    assert_eq!(named.len(), 1, "and it is named");
    assert!(named[0].contains("probe 1"), "by its probe: {}", named[0]);
}

/// The most differing probes one failure names before it stops listing.
const REPORTED_FAILURES: usize = 12;

/// How many `fn` tokens a WGSL source declares, ignoring comments.
///
/// Counting tokens rather than line prefixes is what makes the two scans over
/// this repository's WGSL **refuse** an unreadable declaration rather than skip
/// it: `fn` and the identifier on separate lines is legal, and a name missing
/// from both sides of a set comparison is invisible to it. One function rather
/// than two copies, because the first fix wrote the count into one scan and
/// left the sibling open — which is the same duplication, one round earlier.
fn fn_tokens(source: &str) -> usize {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .flat_map(|code| code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')))
        .filter(|token| *token == "fn")
        .count()
}

/// The indices of `measured` that are not within `tolerance` of `expected`.
///
/// **The comparison loop, not just its predicate.** An earlier fix extracted
/// only `error <= tolerance` and tested that, which pinned nothing: the two
/// call sites could both be reverted to `error > tolerance` with the predicate
/// and its test untouched, and a review measured that the whole suite still
/// passed. Reverting now means re-inlining a loop that exists, which is a
/// deliberate act rather than a one-character one.
///
/// The whole of the difference is NaN. `NaN > tolerance` and
/// `NaN <= tolerance` are both false, so `>` silently accepts every NaN a
/// shader or a reference produces, and `<=` refuses it. No probe in the table
/// produces one, so no fixture can hold this —
/// [`a_nan_is_outside_every_tolerance`] drives this function directly instead.
fn outside(measured: &[f64], expected: &[f64], tolerance: impl Fn(usize) -> f64) -> Vec<usize> {
    assert_eq!(
        measured.len(),
        expected.len(),
        "one measurement per expectation"
    );
    measured
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(index, (got, want))| {
            // Spelled as a three-way match rather than a negated `<=`, both
            // because clippy refuses the latter on a partially ordered type and
            // because the arm that matters is the one with no ordering at all.
            // A NaN compares neither less, equal nor greater, and it is the
            // value this whole function exists to refuse.
            !matches!(
                ((*got - *want).abs()).partial_cmp(&tolerance(*index)),
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            )
        })
        .map(|(index, _)| index)
        .collect()
}

/// A NaN is outside every tolerance.
///
/// The suite's headline comparison is `within`, and a shader returning a quiet
/// NaN passed the whole suite before it. `conformance/README.md` now makes this
/// a promise to every consumer, naming this suite as the worked example, so it
/// is asserted here rather than left to a probe that happens to produce one.
#[test]
fn a_nan_is_outside_every_tolerance() {
    let want = [0.0, 1.0, 2.0, 3.0, 4.0];
    let exact = [0.0, 1.0, 2.0, 3.0, 4.0];
    assert!(
        outside(&exact, &want, |_| 1e-6).is_empty(),
        "exact matches are inside"
    );

    // Exactly the tolerance, so the `Ordering::Equal` arm is reached. The
    // previous pair used 1.000_001, whose f64 difference from 1.0 is
    // 9.99999999e-7 — strictly inside — so dropping `Equal` from the predicate
    // passed. `conformance/README.md` hands a second painter `<= tolerance`, so
    // the inclusive end is part of the contract and not a detail.
    assert!(
        outside(&[1e-6], &[0.0], |_| 1e-6).is_empty(),
        "an error exactly equal to the tolerance is inside"
    );
    let edge = [0.0, 1.000_001, 2.0, 3.000_002, 4.0];
    assert_eq!(
        outside(&edge, &want, |_| 1e-6),
        vec![3],
        "just past the tolerance is outside"
    );

    // The whole point. `error > tolerance` reports an empty list here, which is
    // how a shader returning a quiet NaN passed the suite over every value.
    let nan = [0.0, f64::NAN, 2.0, f64::INFINITY, f64::NEG_INFINITY];
    assert_eq!(
        outside(&nan, &want, |_| 1e6),
        vec![1, 3, 4],
        "a NaN and an infinity are outside every tolerance, however wide"
    );
}

/// `text`, cut to `limit` characters with an ellipsis if it is longer.
///
/// Cut on a character boundary rather than a byte one: these strings carry the
/// argument names, and a `String::truncate` inside a multi-byte character
/// panics.
fn ellipsize(mut text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.truncate(cut);
    text.push('…');
    text
}

/// One probe's arguments, as the file writes them, for a failure message.
fn describe(case: &FunctionCase, probe: &FileProbe) -> String {
    case.arguments
        .iter()
        .zip(&probe.args)
        .map(|(name, value)| format!("{name} = {value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The shader library answers what `conformance/layer2-probes.json` says it
/// owes, on whatever adapter this machine offers.
///
/// **This is the test a second painter ports.** What it needs from its own
/// language is a way to evaluate each function the file names over the
/// arguments it records; the expectations and the tolerances come from the
/// file, and no part of this suite computes them while it runs.
#[test]
fn the_shader_matches_the_committed_probe_table() {
    let table = table();
    let gpu = Gpu::new();
    let mut failures: Vec<String> = Vec::new();
    let mut differing = 0usize;
    let mut evaluated = 0usize;

    for case in &table.functions {
        let (entry, probes, colours) = dispatch(case);
        let expected = case.expected();
        assert_eq!(
            probes.len(),
            expected.len(),
            "{}: {} probe(s) dispatched against {} expected value(s)",
            case.name,
            probes.len(),
            expected.len()
        );
        let measured = gpu.run_with(entry, &probes, &colours);
        assert_eq!(
            measured.len(),
            probes.len(),
            "{} ran {} probe(s) and returned {} result(s)",
            entry,
            probes.len(),
            measured.len()
        );
        evaluated += measured.len();

        let components = case.components();
        // Capped per case, not across the run: `clamp_radii` is the first case
        // and a total regression in it would otherwise fill the list before
        // `rounded_box_sdf` was dispatched, so the message could not tell one
        // broken function from thirteen.
        let widened: Vec<f64> = measured.iter().map(|&got| got as f64).collect();
        let (named, bad) = compare(case, &widened);
        differing += bad;
        failures.extend(named);
        let mut worst = 0.0f64;
        let mut worst_at = 0usize;
        let mut total = 0.0f64;
        for (index, (&got, &want)) in widened.iter().zip(&expected).enumerate() {
            let error = (got - want).abs();
            total += error;
            if error > worst {
                worst = error;
                worst_at = index / components;
            }
        }
        // The measurement, not just the verdict: `docs/decisions/
        // shader-library-and-layer-2.md` D4 and D5 are budgets, and a budget
        // with no number beside it cannot be compared against the next run's.
        // The mean and the worst probe are both in D4's table, so both are
        // printed rather than only the verdict's own number.
        println!(
            "{:<22} {:>5} probe(s), worst {worst:.3e} of {:.3e}, mean {:.3e}",
            case.name,
            case.probes.len(),
            case.tolerance,
            total / measured.len() as f64,
        );
        if worst > 0.0 {
            // Cut, because one `gradient_ramp` probe carries eight offsets and
            // eight colours and its whole argument list is four screens wide.
            // A failure prints it in full; this line is a measurement's label.
            println!(
                "  worst at {}",
                ellipsize(describe(case, &case.probes[worst_at]), 120)
            );
        }
    }

    // No `evaluated == declared` assertion here. It was one, and it was a
    // tautology: the two `assert_eq!`s in the loop above already force every
    // case's `measured.len()` to equal `probes.len() * components()`, which is
    // how `declared` is computed, so it could not fire and it read as an
    // independent check. What actually holds the two halves is
    // `Gpu::run_with`'s unwritten-slot sentinel, and
    // `the_committed_table_matches_the_specs_that_recorded_it` for the count of
    // rows the file carries.
    assert!(
        failures.is_empty(),
        "{differing} of {evaluated} value(s) differ from {TABLE_PATH}:\n{}{}",
        failures.join("\n"),
        if differing > failures.len() {
            format!("\n… and {} more", differing - failures.len())
        } else {
            String::new()
        }
    );
}

/// One function's committed expectations, against the reference that recorded
/// them. The body [`probed_functions`] generates a test around, once per
/// function.
///
/// The tolerance is `1e-6` relative, three orders or more inside every
/// tolerance the file carries. Not bit-exact: both sides call `sqrt`, `exp`,
/// `atan2` and `cos`, whose last bit is a libm's business and differs between a
/// developer's machine and a runner, and an `as f32` near a rounding boundary
/// turns that last bit into a visible difference. What this asserts is that the
/// numbers in the file are the mathematics, not that two libms agree.
fn check_against_the_references(function: &str) {
    let table = table();
    let case = table
        .functions
        .iter()
        .find(|case| case.name == function)
        .unwrap_or_else(|| panic!("{TABLE_PATH} carries no case for {function}"));
    let expected = case.expected();
    let components = case.components();
    assert!(
        !case.probes.is_empty(),
        "{function} carries no probe, so this test would establish nothing"
    );

    // Every reference value first, then one comparison over the whole vector —
    // the same shape the shader side uses, and with the per-index tolerance the
    // closure exists for. Comparing one element at a time made re-inlining the
    // NaN-blind form a one-line change here.
    let mut computed: Vec<f64> = Vec::with_capacity(expected.len());
    for (index, probe) in case.probes.iter().enumerate() {
        let values = reference_value(function, &probe.args);
        assert_eq!(
            values.len(),
            components,
            "{function} probe {index}: the reference gives {} value(s) and the file's \
             `result` says {components}",
            values.len()
        );
        computed.extend(values);
    }
    assert_eq!(
        computed.len(),
        expected.len(),
        "{function}: {} reference value(s) against {} recorded",
        computed.len(),
        expected.len()
    );

    let bad = outside(&computed, &expected, |index| {
        1e-6 * expected[index].abs().max(1.0)
    });
    let differing = bad.len();
    let failures: Vec<String> = bad
        .iter()
        .take(REPORTED_FAILURES)
        .map(|&index| {
            let probe = &case.probes[index / components];
            format!(
                "{function} probe {} component {}: the reference gives {}, the file says \
                 {} at {}",
                index / components,
                index % components,
                computed[index],
                expected[index],
                describe(case, probe),
            )
        })
        .collect();

    assert!(
        failures.is_empty(),
        "{differing} of {} value(s) recorded for {function} disagree with the reference \
         that recorded them:\n{}",
        expected.len(),
        failures.join("\n")
    );
}

/// The committed file is what `CASE_SPECS` and `fixture_args` describe.
///
/// **This closes the symmetric half of the "a probe stopped running" hole.**
/// Three mechanisms already refuse a case *deleted from the file*. Nothing
/// refused a change made in the *source* and never re-recorded — `CASE_SPECS`
/// and `fixture_args` are read only by the `#[ignore]`d recorder, so tightening
/// a tolerance there, or adding a probe, compiled and passed every tier while
/// gating nothing. Both this file's Rust and
/// `docs/decisions/shader-library-and-layer-2.md` D7 read as though they did.
///
/// It also pins the columns a second painter binds against. `tolerance` is the
/// one that matters most: the shader check reads it out of the file it is
/// validating, so with nothing holding it, setting every tolerance to `1e6`
/// made that test unable to fail. `arguments` and `signature` are next — the
/// README tells a portable consumer to key its harness on them.
///
/// This does not regenerate anything and could not: it compares the recorded
/// **inputs and metadata**, never the expected values. Those stay the
/// references' word, checked by the `the_recorded_*` tests.
#[test]
fn the_committed_table_matches_the_specs_that_recorded_it() {
    let table = table();
    assert_eq!(table.about, TABLE_ABOUT, "{TABLE_PATH}: about");
    assert_eq!(table.shader, TABLE_SHADER, "{TABLE_PATH}: shader");
    assert_eq!(
        table.properties, TABLE_PROPERTIES,
        "{TABLE_PATH}: properties"
    );
    assert_eq!(
        table.recorded_by, TABLE_RECORDED_BY,
        "{TABLE_PATH}: recorded_by"
    );
    let shader = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(TABLE_SHADER);
    let advertised = std::fs::read_to_string(&shader).unwrap_or_else(|error| {
        panic!("the file advertises {TABLE_SHADER} as where the maths lives: {error}")
    });
    assert_eq!(
        advertised,
        dashscene_gpu::SDF_WGSL,
        "{TABLE_SHADER} is not the library this suite compiled. Existence alone was \
         the check here and it passed with the field pointed at paint.wgsl, a real \
         file carrying none of this mathematics"
    );
    assert_eq!(
        table.functions.len(),
        CASE_SPECS.len(),
        "{TABLE_PATH} carries {} case(s) and CASE_SPECS declares {}",
        table.functions.len(),
        CASE_SPECS.len()
    );

    for (case, spec) in table.functions.iter().zip(CASE_SPECS) {
        let where_ = format!("{} in {TABLE_PATH}", spec.name);
        assert_eq!(case.name, spec.name, "case order differs from CASE_SPECS");
        assert_eq!(case.signature, spec.signature, "{where_}: signature");
        assert_eq!(case.result, spec.result, "{where_}: result");
        assert_eq!(case.reference, spec.reference, "{where_}: reference");
        assert_eq!(
            case.tolerance, spec.tolerance,
            "{where_}: tolerance. The shader check reads this out of the file, so a \
             widened one here cannot fail and cannot be noticed"
        );
        let names: Vec<&str> = case.arguments.iter().map(String::as_str).collect();
        assert_eq!(names.as_slice(), spec.arguments, "{where_}: arguments");

        let recorded = fixture_args(spec.name);
        assert_eq!(
            case.probes.len(),
            recorded.len(),
            "{where_}: {} probe(s) recorded against {} in fixture_args. A file with \
             fewer rows than its fixture is a re-record that did not happen",
            case.probes.len(),
            recorded.len()
        );
        for (index, (probe, expected_args)) in case.probes.iter().zip(&recorded).enumerate() {
            assert_eq!(
                probe.args.len(),
                case.arguments.len(),
                "{where_} probe {index}: {} argument(s) against {} name(s). `describe` \
                 zips the two and would truncate silently",
                probe.args.len(),
                case.arguments.len()
            );
            assert_eq!(
                &probe.args, expected_args,
                "{where_} probe {index}: arguments"
            );
        }
    }

    // The file is named twice — `include_str!` resolves against this source
    // file and `table_file()` against the crate manifest — and nothing else
    // says the two land on the same bytes. A recorder writing somewhere else
    // would look entirely successful.
    let on_disk = std::fs::read_to_string(table_file())
        .unwrap_or_else(|error| panic!("{} is not readable: {error}", table_file().display()));
    assert_eq!(
        on_disk, TABLE_JSON,
        "the path `include_str!` reads and the path the recorder writes are not the \
         same file"
    );
}

/// Every function `sdf.wgsl` declares is either probed or named below as one
/// reached only through its caller.
///
/// The gap this closes is the one a suite of this shape is most likely to have:
/// every other assertion here is keyed to the probe entry points, so a function
/// **added to the shader library** and never probed would be invisible to all
/// of them. The design record's claim that four functions have no probe of
/// their own is prose today and this is what makes it checked.
///
/// Adding a function to `sdf.wgsl` therefore fails this test until someone
/// decides which it is — a probe entry point plus a case in the table, or a
/// helper whose whole output reaches its caller's return value and a line here.
/// Both are deliberate acts, which is the point.
#[test]
fn every_function_in_the_shader_library_is_probed_or_named_as_a_helper() {
    // The exported string the render pipelines include, not a second copy.
    let lines: Vec<&str> = dashscene_gpu::SDF_WGSL
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("fn "))
        .collect();
    let declared: std::collections::BTreeSet<String> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("fn "))
        .filter_map(|rest| rest.split('(').next())
        .map(str::to_owned)
        .collect();
    // The parse refuses rather than skips, for the same reason the entry-point
    // parse does: a declaration it could not read would be missing from both
    // sides of the comparison, and set equality cannot see that.
    //
    // Counting `fn` as a **token** rather than as a line prefix is what makes
    // that true. `lines` is already filtered on `starts_with("fn ")`, so
    // comparing against its length catches only a repeated name — a
    // declaration written with `fn` and the identifier on separate lines, which
    // WGSL permits, is absent from `lines` as well and the comparison cannot
    // see it. The token scan reads the whole source and does not care about
    // line breaks; comments are stripped first because this file's prose says
    // "fn" often.
    let tokens = fn_tokens(dashscene_gpu::SDF_WGSL);
    assert_eq!(
        tokens,
        declared.len(),
        "sdf.wgsl carries {tokens} `fn` token(s) and {} parsed declaration(s). A \
         declaration this test cannot read is missing from both sides of the set \
         comparison below, which is why it counts tokens rather than trusting the \
         line scan",
        declared.len()
    );
    assert_eq!(
        declared.len(),
        lines.len(),
        "sdf.wgsl declares {} function(s) and {} distinct name(s) were parsed, so a \
         declaration was not read or a name is repeated",
        lines.len(),
        declared.len()
    );
    assert!(
        !declared.is_empty(),
        "no function was found in dashscene_gpu::SDF_WGSL, so this test would pass over \
         an empty library"
    );

    let accounted: std::collections::BTreeSet<String> = PROBED_FUNCTIONS
        .iter()
        .chain(UNPROBED_HELPERS)
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        declared, accounted,
        "sdf.wgsl and this suite disagree about which functions exist. A function here \
         and not there needs either a probe entry point in shaders/conformance.wgsl with \
         a case in {TABLE_PATH}, or a line in UNPROBED_HELPERS saying it is reached \
         through its caller"
    );
}

/// The functions of `sdf.wgsl` that have no probe of their own.
///
/// Each is reached through its callers, and adequately: the whole of its output
/// reaches their return values. `gradient_local` through the four
/// `gradient_*_t` parameterizations, `gradient_segment_t` through
/// `gradient_ramp`, `half_extent_at` and `blur_row` through
/// `blurred_rounded_box`.
///
/// This is a list of deliberate exceptions, not a backlog. A function belongs
/// here only when probing it directly would add nothing its caller's probes do
/// not already carry — and `gradient_segment_t` is the one to think about
/// before trusting that: its zero-width answer is observable only when the
/// zero-width segment is the **last** one `gradient_ramp` visits, which is why
/// the recorded stop lists put a repeated offset in that position.
const UNPROBED_HELPERS: &[&str] = &[
    "gradient_local",
    "gradient_segment_t",
    "half_extent_at",
    "blur_row",
];

/// Every probe entry point is reached by the table, and every function the
/// table names has one.
///
/// A suite reading its expectations from a file can pass over a probe that
/// silently stopped running: delete a case and the loop simply has less to do.
/// This is what refuses that. The entry points are read out of
/// `shaders/conformance.wgsl` rather than listed again here, so adding one
/// without a case in the file fails rather than passing quietly.
#[test]
fn every_probe_entry_point_is_reached_by_the_table() {
    let table = table();
    let named: std::collections::BTreeSet<&str> = table
        .functions
        .iter()
        .map(|case| case.name.as_str())
        .collect();
    let known: std::collections::BTreeSet<&str> = PROBED_FUNCTIONS.iter().copied().collect();
    assert_eq!(
        named, known,
        "{TABLE_PATH} names a different set of functions than this suite dispatches"
    );

    // Read out of the shader rather than listed again here, so an entry point
    // added without a case in the file fails rather than passing quietly.
    let source = include_str!("shaders/conformance.wgsl");
    let declared: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("fn "))
        .collect();
    let entries: std::collections::BTreeSet<String> = declared
        .iter()
        .filter_map(|line| line.strip_prefix("fn "))
        .filter_map(|rest| rest.split('(').next())
        .map(str::to_owned)
        .collect();

    // The parse refuses rather than skips. A `filter_map` that quietly drops a
    // declaration it cannot read would leave an unreachable entry point out of
    // **both** sides of the comparison below, and set equality cannot see a row
    // missing from both. Two counts close that: every function in this file is
    // a probe, and every probe carries exactly one `@compute`.
    let odd: Vec<&&str> = declared
        .iter()
        .filter(|line| !line.starts_with("fn probe_"))
        .collect();
    assert!(
        odd.is_empty(),
        "shaders/conformance.wgsl declares a function that is not a probe, which this \
         test cannot account for: {odd:?}"
    );
    let compute = source
        .lines()
        .filter(|line| line.trim_start().starts_with("@compute"))
        .count();
    assert_eq!(
        entries.len(),
        compute,
        "shaders/conformance.wgsl has {compute} @compute annotation(s) and {} parsed \
         entry point name(s), so a declaration was not read",
        entries.len()
    );
    // The same token count the shader-library census uses, for the same reason:
    // `starts_with("fn ")` misses a declaration split across lines, which is
    // legal WGSL, and a name missing from both sides of a set comparison is
    // invisible to it. Fixing only the `sdf.wgsl` sibling left this one open.
    let fn_tokens_here = fn_tokens(source);
    assert_eq!(
        fn_tokens_here,
        declared.len(),
        "shaders/conformance.wgsl carries {fn_tokens_here} `fn` token(s) and {} parsed \
         declaration(s), so one was not read",
        declared.len()
    );
    assert!(
        !entries.is_empty(),
        "no probe entry point was found in shaders/conformance.wgsl, so this test would \
         pass over an empty table"
    );
    let reached: std::collections::BTreeSet<String> = table
        .functions
        .iter()
        .map(|case| entry_point_for(&case.name).to_owned())
        .collect();
    assert_eq!(
        entries, reached,
        "the probe entry points in shaders/conformance.wgsl and the ones {TABLE_PATH} \
         reaches are not the same set"
    );
}

// ---------------------------------------------------------------------------
// Recording the table
// ---------------------------------------------------------------------------

/// The envelope `record_the_probe_table` writes and
/// `the_committed_table_matches_the_specs_that_recorded_it` holds the file to.
///
/// Constants rather than literals inside the recorder, so that the file's copy
/// can be compared against them. `conformance/README.md` tells a consumer that
/// `format` is a version handshake and that `shader` names the file the maths
/// lives in; both were previously written once and checked never.
const TABLE_FORMAT: u32 = 1;
const TABLE_ABOUT: &str = "Layer-2 conformance probes for dashscene's SDF shader library: \
     the inputs, the expected values and the tolerances, so a painter in any shading \
     language can check the same mathematics. See conformance/README.md.";
const TABLE_SHADER: &str = "crates/dashscene-gpu/src/shaders/sdf.wgsl";
const TABLE_PROPERTIES: &str = "docs/design/dashscene-gpu.md — 'The probe table, and what \
     stays a property'. This file carries only the claims that are one value at one input; \
     the rest a painter must implement as properties.";
const TABLE_RECORDED_BY: &str = "crates/dashscene-gpu/tests/layer2_conformance.rs, \
     record_the_probe_table (#[ignore]d)";

/// One function's row in the table, without its probes.
struct CaseSpec {
    name: &'static str,
    signature: &'static str,
    arguments: &'static [&'static str],
    result: &'static str,
    tolerance: f64,
    reference: &'static str,
}

/// Every case the recorder writes, in the order the file carries them.
///
/// The tolerances are the ones this suite has held since story #579, moved out
/// of the assertions and into the data. Each is stated where it comes from:
/// a sampling bound, a float's own resolution, or a budget a decision record
/// carries.
const CASE_SPECS: &[CaseSpec] = &[
    CaseSpec {
        name: "clamp_radii",
        signature: "clamp_radii(half_size: vec2f, radii: vec4f) -> vec4f",
        arguments: &["half_size", "radii"],
        result: "vec4f",
        tolerance: 1e-5,
        reference: "independent — the over-subscription rule, restated per edge",
    },
    CaseSpec {
        name: "rounded_box_sdf",
        signature: "rounded_box_sdf(p: vec2f, half_size: vec2f, radii: vec4f) -> f32",
        arguments: &["p", "half_size", "radii"],
        result: "f32",
        tolerance: 0.02,
        reference: "independent — brute-force sampling of the outline; the tolerance is \
                    1.6 times the 0.0122 sample spacing, which is the bound that always holds",
    },
    CaseSpec {
        name: "coverage",
        signature: "coverage(d: f32, width: f32) -> f32",
        arguments: &["d", "width"],
        result: "f32",
        tolerance: 1e-6,
        reference: "transliterated — the ramp is its own definition, so this checks the \
                    transliteration and not the mathematics",
    },
    CaseSpec {
        name: "median3",
        signature: "median3(v: vec3f) -> f32",
        arguments: &["v"],
        result: "f32",
        tolerance: 1e-7,
        reference: "independent — a sort, rather than the shader's min/max lattice",
    },
    CaseSpec {
        name: "msdf_coverage",
        signature: "msdf_coverage(sample: vec3f, px_range: f32) -> f32",
        arguments: &["sample", "px_range"],
        result: "f32",
        tolerance: 1e-6,
        reference: "independent for the median, transliterated for the ramp it is \
                    composed with",
    },
    CaseSpec {
        name: "gradient_linear_t",
        signature: "gradient_linear_t(p: vec2f, origin: vec2f, primary: vec2f, \
                    secondary: vec2f) -> f32",
        arguments: &["p", "origin", "primary", "secondary"],
        result: "f32",
        tolerance: 1e-5,
        reference: "transliterated — the parameterization is its own definition",
    },
    CaseSpec {
        name: "gradient_radial_t",
        signature: "gradient_radial_t(p: vec2f, origin: vec2f, primary: vec2f, \
                    secondary: vec2f) -> f32",
        arguments: &["p", "origin", "primary", "secondary"],
        result: "f32",
        tolerance: 1e-5,
        reference: "transliterated — the parameterization is its own definition",
    },
    CaseSpec {
        name: "gradient_angular_t",
        signature: "gradient_angular_t(p: vec2f, origin: vec2f, primary: vec2f, \
                    secondary: vec2f) -> f32",
        arguments: &["p", "origin", "primary", "secondary"],
        result: "f32",
        tolerance: 1e-5,
        reference: "transliterated — the parameterization is its own definition",
    },
    CaseSpec {
        name: "gradient_diamond_t",
        signature: "gradient_diamond_t(p: vec2f, origin: vec2f, primary: vec2f, \
                    secondary: vec2f) -> f32",
        arguments: &["p", "origin", "primary", "secondary"],
        result: "f32",
        tolerance: 1e-5,
        reference: "transliterated — the parameterization is its own definition",
    },
    CaseSpec {
        name: "gradient_ramp",
        signature: "gradient_ramp(t: f32, offsets: array<f32, 8>, colours: array<vec4f, 8>, \
                    count: u32) -> vec4f",
        arguments: &["t", "offsets", "colours", "count"],
        result: "vec4f",
        tolerance: 1e-6,
        reference: "independent — the first stop past t and the segment before it, rather \
                    than the shader's overwriting walk",
    },
    CaseSpec {
        name: "stroke_coverage",
        signature: "stroke_coverage(d: f32, width: f32, align: f32, aa: f32) -> f32",
        arguments: &["d", "width", "align", "aa"],
        result: "f32",
        tolerance: 1e-5,
        reference: "independent in form — the band's overlap with the pixel footprint, \
                    rather than the shader's difference of two edge ramps. The two are \
                    algebraically equal for a linear ramp, so this checks the arrangement \
                    rather than adding a second derivation: the folded single ramp the \
                    shader used before story #579 fails it, a faithful port does not",
    },
    CaseSpec {
        name: "erf_approx",
        signature: "erf_approx(x: f32) -> f32",
        arguments: &["x"],
        result: "f32",
        tolerance: 1e-3,
        reference: "independent — erf's own definition, integrated by Simpson's rule in \
                    double precision; the tolerance is the accuracy the fitted form is \
                    trusted within (docs/decisions/shader-library-and-layer-2.md)",
    },
    CaseSpec {
        name: "blurred_rounded_box",
        signature: "blurred_rounded_box(p: vec2f, half_size: vec2f, radii: vec4f, \
                    sigma: f32) -> f32",
        arguments: &["p", "half_size", "radii", "sigma"],
        result: "f32",
        tolerance: 1.0 / 255.0,
        reference: "independent — a 512-row quadrature with an integrated erf, against the \
                    shader's twelve rows and a fitted one, except at sigma = 0 where both \
                    answer the unblurred shape and neither integrates; the tolerance is \
                    D5's budget of one code point of an eight-bit channel",
    },
];

/// Six decimal places, so a generated fixture value reads as a number rather
/// than as a float's shortest round-trip spelling.
fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

/// The arguments of every probe of one function.
///
/// The recorder's input, and the only place the probe inputs are constructed.
/// Nothing else in this suite calls it: the tests read
/// `conformance/layer2-probes.json`, which is what this produced.
fn fixture_args(function: &str) -> Vec<Vec<Value>> {
    use serde_json::json;

    match function {
        "clamp_radii" => vec![
            // A pill's 9999, a radius larger than a square's half-extent, and
            // a set that already fits and must come back unchanged.
            vec![json!([50.0, 30.0]), json!([9999.0, 9999.0, 9999.0, 9999.0])],
            vec![json!([20.0, 20.0]), json!([30.0, 30.0, 30.0, 30.0])],
            vec![json!([50.0, 30.0]), json!([10.0, 4.0, 16.0, 8.0])],
            // Unequal radii **and** an over-subscribed edge. Without a row of
            // this shape the rule is unpinned: the three above have either four
            // equal radii, where every pairing of the per-edge sums gives the
            // same number, or no clamping at all. A shader pairing diagonal
            // corners rather than the two that meet an edge passed all three.
            vec![json!([50.0, 30.0]), json!([70.0, 60.0, 0.0, 0.0])],
            vec![json!([50.0, 30.0]), json!([0.0, 50.0, 40.0, 0.0])],
        ],
        "rounded_box_sdf" => {
            // A spread of points and boxes: inside, outside, on an edge, in a
            // corner arc, and past each corner. Every box has four different
            // radii, and one has a sharp corner beside a round one, so a
            // shader that picked the wrong corner lands on a different value.
            let boxes: [([f64; 2], [f64; 4]); 4] = [
                ([50.0, 30.0], [0.0, 0.0, 0.0, 0.0]),     // a sharp rectangle
                ([50.0, 30.0], [10.0, 4.0, 16.0, 8.0]),   // four different radii
                ([50.0, 30.0], [30.0, 0.0, 0.0, 12.0]),   // a sharp corner beside a round one
                ([20.0, 20.0], [20.0, 20.0, 20.0, 20.0]), // a circle
            ];
            let points: [[f64; 2]; 13] = [
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
            let mut rows = Vec::new();
            for (half, radii) in boxes {
                for p in points {
                    rows.push(vec![json!(p), json!(half), json!(radii)]);
                }
            }
            // A pill, whose radii clamp before the distance means anything.
            // Nothing clamped them before story #579's fix, and a 50x30
            // half-box with radii 9999 read about 4085 units *outside* at
            // every point.
            for p in [[0.0f64, 0.0], [40.0, 0.0], [0.0, 25.0], [-40.0, 0.0]] {
                rows.push(vec![
                    json!(p),
                    json!([50.0, 30.0]),
                    json!([9999.0, 9999.0, 9999.0, 9999.0]),
                ]);
            }
            rows
        }
        "coverage" => {
            let widths = [0.0f64, 0.5, 1.0, 2.0];
            let distances = [-4.0f64, -1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0, 4.0];
            widths
                .iter()
                .flat_map(|&width| distances.iter().map(move |&d| vec![json!(d), json!(width)]))
                .collect()
        }
        "median3" => msdf_samples().iter().map(|s| vec![json!(s)]).collect(),
        "msdf_coverage" => {
            // Every sample at three different ranges. A single range lets a
            // shader that ignored the argument and hardcoded it pass — the
            // uniform-fixture defect one level down, in the arguments.
            let ranges = [2.0f64, 4.0, 9.0];
            msdf_samples()
                .iter()
                .flat_map(|s| ranges.iter().map(move |&r| vec![json!(s), json!(r)]))
                .collect()
        }
        "gradient_linear_t" | "gradient_radial_t" | "gradient_angular_t" | "gradient_diamond_t" => {
            let points: [[f64; 2]; 11] = [
                [0.0, 0.0],
                [0.25, 0.25],
                [0.5, 0.5],
                [0.75, 0.5],
                [1.0, 1.0],
                [0.1, 0.9],
                [0.9, 0.1],
                [0.5, 0.25],
                [0.25, 0.75],
                // Straddling `gradient_angular_t`'s seam. The angle is lifted
                // by one turn before `fract`, so `t` runs to 1 and wraps to 0;
                // without a probe either side of that the discontinuity is
                // unmeasured, and the recorded values otherwise stop at 0.992.
                // These sit a whisker off the primary axis of the first frame.
                [0.4999, 0.3752],
                [0.5001, 0.3748],
            ];
            // Two frames, not one, and they carry different properties
            // because two different wrong painters need catching. Measured
            // from these literals:
            //
            //   frame 1  |u| / |v| = 2.500, angle 90.00 degrees
            //   frame 2  |u| / |v| = 1.466, angle 61.56 degrees
            //
            // Frame 1's unequal handles are what fail a painter that drops
            // the secondary handle from a radial. They are **not** enough for
            // the linear and angular parameterizations: for perpendicular
            // handles, `gradient_local`'s x agrees identically with a
            // projection onto the primary axis alone — measured at 0.000000
            // over frame 1's own probes, and at 0.460 over frame 2's. So the
            // oblique frame is what fails that painter. A single origin and
            // primary handle would also let a shader that ignored its
            // arguments and used the fixture's literals pass, which is the
            // uniform-fixture rule one level down.
            let frames: [([f64; 2], [f64; 2], [f64; 2]); 2] = [
                ([0.25, 0.25], [0.75, 0.5], [0.15, 0.45]),
                ([0.6, 0.1], [0.2, 0.9], [0.95, 0.6]),
            ];
            frames
                .iter()
                .flat_map(|&(origin, primary, secondary)| {
                    points.iter().map(move |&p| {
                        vec![json!(p), json!(origin), json!(primary), json!(secondary)]
                    })
                })
                .collect()
        }
        "gradient_ramp" => {
            let mut rows = Vec::new();
            for (stops, samples) in ramp_cases() {
                let (offsets, colours) = ramp_slots(&stops);
                for t in samples {
                    // Widened to f64 explicitly, and this is a readability
                    // change rather than a fix: `json!` on an `f32` routes
                    // through `Number::from_f32`, which stores `f as f64`, so
                    // the two spell the same number and the recorded file did
                    // not move. An earlier version of this comment claimed
                    // otherwise and was wrong — the defect
                    // `the_committed_table_matches_the_specs_that_recorded_it`
                    // found was in the **parser**, not the writer, and the fix
                    // for it is serde_json's `float_roundtrip` feature. The
                    // cast stays because it says at the call site what type
                    // reaches the file.
                    rows.push(vec![
                        json!(t as f64),
                        json!(offsets.map(|offset| offset as f64)),
                        json!(colours.map(|colour| colour.map(|c| c as f64))),
                        json!(stops.len()),
                    ]);
                }
            }
            // A gradient with no stops at all. `PaintTable::intern_fill` hands
            // out a `StopRange` of count zero — it leaves the "at least one
            // stop" rule to `dashscene-validator`, which reports it as
            // `paint.gradient.no-stops` (P4) — so the value is reachable from
            // an unvalidated document. The answer is transparent: there is no
            // colour to clamp to, and drawing nothing is the one answer that
            // cannot paint a wrong one. `dashscene-skia` panics on the same
            // input, which this records rather than hides.
            //
            // The slots hold the fixture's poison, so a walk that ran once
            // regardless of the count reads 9.0 rather than 0.0.
            let (offsets, colours) = ramp_slots(&[]);
            rows.push(vec![
                json!(0.5),
                json!(offsets.map(|offset| offset as f64)),
                json!(colours.map(|colour| colour.map(|c| c as f64))),
                json!(0),
            ]);
            rows
        }
        "stroke_coverage" => {
            let mut rows = Vec::new();
            // A soft edge, where the band's two ramps do not tie at its
            // endpoints. The hard edge is a property test rather than a table
            // for exactly that reason — see `reference_stroke_coverage`.
            let aa = 1.0f64;
            for align in [0.0f64, 1.0, 2.0] {
                for width in [4.0f64, 0.5] {
                    for step in -16..=16 {
                        let d = step as f64 * 0.5;
                        rows.push(vec![json!(d), json!(width), json!(align), json!(aa)]);
                    }
                }
            }
            // A zero-width stroke covers nothing, whatever else it is given.
            // The shader has no guard for it — the band form makes both edges
            // coincide — and that is a claim with one right answer at one
            // input, so the rule this file follows puts it in the table rather
            // than leaving it to a property test.
            for align in [0.0f64, 1.0, 2.0] {
                for d in [-2.0f64, 0.0, 2.0] {
                    rows.push(vec![json!(d), json!(0.0), json!(align), json!(aa)]);
                }
            }
            // A stroke narrower than the antialiasing width, on the outline.
            // Folding the distance and taking one ramp saturates as soon as
            // the fold passes the ramp's centre: a 0.25-unit Center stroke
            // measured 0.625 where 0.25 is correct.
            for width in [0.125f64, 0.25, 0.5, 1.0, 2.0, 4.0] {
                rows.push(vec![json!(0.0), json!(width), json!(1.0), json!(aa)]);
            }
            rows
        }
        "erf_approx" => (-400..=400)
            .map(|i| vec![json!(round6(i as f64 / 100.0))])
            .collect(),
        "blurred_rounded_box" => {
            let half = [60.0f64, 40.0];
            let cases: [([f64; 4], f64); 5] = [
                ([0.0, 0.0, 0.0, 0.0], 8.0),      // a sharp rectangle
                ([16.0, 16.0, 16.0, 16.0], 8.0),  // a uniform corner
                ([30.0, 4.0, 12.0, 0.0], 8.0),    // four different radii
                ([16.0, 16.0, 16.0, 16.0], 24.0), // a blur wider than the corner
                // A blurred pill. Every row above leaves the corner radii
                // untouched, so `clamp_radii` was the identity on all of them
                // and could be deleted from the shader **and** from the
                // reference with nothing failing — measured. This is the case
                // story #579 existed for, and `rounded_box_sdf` already carries
                // its unblurred twin.
                ([9999.0, 9999.0, 9999.0, 9999.0], 8.0),
            ];
            let mut rows = Vec::new();
            // A zero blur is the unblurred shape. `sdf.wgsl` guards `sigma <=
            // 0` before dividing by it, so a painter conformant against this
            // file alone would never evaluate that branch and could ship a
            // division by zero there. Inside, outside, and on each side of an
            // edge.
            for radii in [[0.0f64, 0.0, 0.0, 0.0], [16.0, 16.0, 16.0, 16.0]] {
                for p in [[0.0f64, 0.0], [59.0, 0.0], [61.0, 0.0], [0.0, 200.0]] {
                    rows.push(vec![json!(p), json!(half), json!(radii), json!(0.0)]);
                }
            }
            for (radii, sigma) in cases {
                // A grid across the box, its edges, its corners and well
                // outside it.
                for iy in -6..=6 {
                    for ix in -8..=8 {
                        let p = [ix as f64 * 10.0, iy as f64 * 10.0];
                        rows.push(vec![json!(p), json!(half), json!(radii), json!(sigma)]);
                    }
                }
            }
            rows
        }
        other => panic!("no fixture is recorded for {other}"),
    }
}

/// The MSDF samples `median3` and `msdf_coverage` share.
fn msdf_samples() -> [[f64; 3]; 8] {
    [
        [0.1, 0.5, 0.9],
        [0.9, 0.5, 0.1],
        [0.5, 0.1, 0.9],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 0.0],
        [0.25, 0.25, 0.25],
        [0.49, 0.51, 0.50],
        [0.75, 0.20, 0.60],
    ]
}

/// The stop lists `gradient_ramp` is probed over, and the `t` values probed
/// against each.
///
/// Every colour is distinct and no two share a channel pattern, so a ramp that
/// mixed the right pair in the wrong channels fails. No two lists agree in
/// their count, their offsets or their colours — the uniform-fixture rule,
/// applied to the argument that is an array. Issue #715 names the middle three
/// by hand:
///
/// - **A range that starts at 0 and ends at 1**, the ordinary case.
/// - **A range that starts above 0 and ends below 1**, which a producer authors
///   whenever it moves a handle instead of a stop. Both clamps are reachable
///   inside it.
/// - **Three uneven stops**, so the segment widths differ and a walk that
///   assumed even spacing is wrong everywhere but the ends.
/// - **One stop**, where there is no segment at all and every `t` gives the
///   same colour.
/// - **A hard stop in the middle** — two stops at one offset, which Figma
///   produces for a banded gradient.
/// - **Eight stops**, the vocabulary's ceiling
///   (`dashpaint::MAX_GRADIENT_STOPS`), so the walk's upper bound is exercised
///   rather than assumed.
/// - **A hard stop on the last pair.** The middle one above has the segment
///   after it overwrite whatever the zero-width one produced, so nothing there
///   depends on `gradient_segment_t`'s answer for a zero-width segment, and
///   mutation testing is what said so. Here the zero-width segment is the last
///   one the walk visits, and its answer is the result.
/// - **Both stops at one offset**, a ramp with no width anywhere — the
///   smallest thing a hard stop can be, with no interior segment to reach the
///   right-continuity through.
fn ramp_cases() -> Vec<RampCase> {
    let eight: Vec<Stop> = (0..8)
        .map(|i| {
            let f = i as f64;
            (
                round6(f / 7.0) as f32,
                [
                    round6(f / 7.0) as f32,
                    round6(1.0 - f / 9.0) as f32,
                    round6((f * 0.13) % 1.0) as f32,
                    round6(0.1 + f / 10.0) as f32,
                ],
            )
        })
        .collect();
    vec![
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
        (eight, vec![0.0, 0.05, 0.3, 0.5, 0.6, 0.85, 1.0]),
        (
            vec![
                (0.0, [0.15, 0.85, 0.35, 1.0]),
                (0.5, [0.95, 0.45, 0.05, 0.9]),
                (1.0, [0.25, 0.15, 0.75, 0.6]),
                (1.0, [0.65, 0.35, 0.95, 0.3]),
            ],
            vec![0.0, 0.5, 0.99, 1.0],
        ),
        (
            vec![
                (0.4, [0.05, 0.55, 0.95, 0.45]),
                (0.4, [0.85, 0.25, 0.15, 0.95]),
            ],
            vec![0.0, 0.39, 0.4, 0.41, 1.0],
        ),
    ]
}

/// Writes `conformance/layer2-probes.json` from the fixtures above and the
/// references in this file.
///
/// **`#[ignore]`d, and it must stay that way.** No test tier runs it, so no
/// consumer regenerates the data from its own implementation and then tests
/// that implementation against itself. Run it deliberately:
///
/// ```text
/// cargo test -p dashscene-gpu --test layer2_conformance -- \
///     --ignored record_the_probe_table
/// prim fmt --no-primignore conformance/layer2-probes.json
/// just test
/// ```
///
/// The `just test` at the end is not optional: after a re-record, the shader is
/// the only independent word on what was written, because the references that
/// wrote the file are also what the `the_recorded_*` tests check it with. Read
/// the diff before committing — this file is reviewed truth, the same rule
/// `goldens/README.md` states for a golden image.
#[test]
#[ignore = "rewrites conformance/layer2-probes.json; run it by name and review the diff"]
fn record_the_probe_table() {
    // The recorder's own list and the tests' list are separate declarations and
    // this is what keeps them one set. Without it a case added to only one of
    // them is caught by `every_probe_entry_point_is_reached_by_the_table` —
    // but only after the file has been written and committed, which is a worse
    // place to find it than here.
    let specified: Vec<&str> = CASE_SPECS.iter().map(|spec| spec.name).collect();
    assert_eq!(
        specified, PROBED_FUNCTIONS,
        "CASE_SPECS and PROBED_FUNCTIONS name different functions, so this would record \
         a file the suite cannot dispatch"
    );

    let functions: Vec<FunctionCase> = CASE_SPECS
        .iter()
        .map(|spec| {
            let probes = fixture_args(spec.name)
                .into_iter()
                .map(|args| {
                    let computed = reference_value(spec.name, &args);
                    let expected = match spec.result {
                        "f32" => serde_json::to_value(computed[0]),
                        _ => serde_json::to_value(&computed),
                    }
                    .expect("a float is representable in JSON");
                    FileProbe { args, expected }
                })
                .collect::<Vec<_>>();
            assert!(!probes.is_empty(), "{} records no probe", spec.name);
            FunctionCase {
                name: spec.name.to_owned(),
                signature: spec.signature.to_owned(),
                arguments: spec.arguments.iter().map(|&a| a.to_owned()).collect(),
                result: spec.result.to_owned(),
                tolerance: spec.tolerance,
                reference: spec.reference.to_owned(),
                probes,
            }
        })
        .collect();

    let table = ProbeTable {
        format: TABLE_FORMAT,
        about: TABLE_ABOUT.to_owned(),
        shader: TABLE_SHADER.to_owned(),
        properties: TABLE_PROPERTIES.to_owned(),
        recorded_by: TABLE_RECORDED_BY.to_owned(),
        functions,
    };

    // Minified, and `prim fmt` is what lays it out. prim keeps an array on one
    // line where it fits inside the line length and breaks it where it does
    // not, but it also *preserves* a break the author already made — so a
    // pretty-printed emission here would commit one number per line, measured
    // at about four times the lines and twice the bytes. Handing the formatter
    // one line lets it choose.
    let path = table_file();
    let json = serde_json::to_string(&table).expect("the table serialises");
    std::fs::write(&path, json + "\n").unwrap_or_else(|error| {
        panic!("{} is not writable: {error}", path.display());
    });
    println!(
        "recorded {} probe(s) over {} function(s) into {}",
        table
            .functions
            .iter()
            .map(|case| case.probes.len())
            .sum::<usize>(),
        table.functions.len(),
        path.display(),
    );
    println!("now run `prim fmt --no-primignore {TABLE_PATH}`, then `just test`, then read");
    println!("(the override is issue #1284: inside a .claude/worktrees/ worktree every prim");
    println!(" pass matches `.claude/` and skips every file, `just prim` included)");
}

// ---------------------------------------------------------------------------
// The properties
//
// What is here rather than in the table, and why, is stated in
// `docs/design/dashscene-gpu.md` under "The probe table, and what stays a
// property". A second painter implements these; it cannot load them.
// ---------------------------------------------------------------------------

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
    // A zero-width stroke is not re-covered here: it is nine rows of the
    // committed table now, which is where a claim with one right answer at one
    // input belongs. This block dispatched three of those nine a second time.
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
        // Through `reference_stroke_coverage`, not a second expression here.
        // Its `aa <= 0` branch is reachable from nowhere else — every recorded
        // probe carries `aa = 1`, because a hard edge ties at both band
        // endpoints and a table row cannot say "either answer is right" — so
        // computing the expectation twice left that branch unable to fail. The
        // interval below is still what says the band is *where the alignment
        // puts it*; this is the same claim, evaluated by the reference a second
        // painter would port.
        let want = reference_stroke_coverage(d, width, align, aa);
        // Half-open, `(lo, hi]`. The band's placement is the claim; which side
        // owns each endpoint is the shader's convention, measured rather than
        // assumed, and pinned here so that a port cannot quietly pick the other
        // one. Both endpoints are checked — an earlier version skipped them and
        // called them a tie, which left the reference free to disagree with the
        // shader at the lower edge, and it did.
        assert_eq!(
            want,
            if d > lo && d <= hi { 1.0 } else { 0.0 },
            "the hard-edge reference disagrees with the half-open band ({lo}, {hi}] \
             at {d}"
        );
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

/// A sample on the edge is half covered.
///
/// The property the antialiasing ramp exists for, stated separately from the
/// formula the table checks: the ramp is 1 well inside, 0 well outside, and
/// exactly a half on the edge, for every width.
#[test]
fn a_sample_on_the_edge_is_half_covered() {
    let gpu = Gpu::new();
    let widths = [0.25f32, 0.5, 1.0, 2.0, 16.0];
    let probes: Vec<Probe> = widths
        .iter()
        .map(|&width| Probe {
            v1: [0.0, width, 0.0, 0.0],
            ..Probe::default()
        })
        .collect();
    for (&width, &got) in widths.iter().zip(&gpu.run("probe_coverage", &probes)) {
        assert!(
            (got - 0.5).abs() < 1e-6,
            "a sample on the edge of a {width}-wide ramp is half covered, got {got}"
        );
    }
}

/// Every point inside a pill is inside the shape.
///
/// The consequence of clamping, stated as the property rather than as the four
/// distances the table already carries. Nothing clamped the radii before story
/// #579: Figma authors a pill as `cornerRadius: 9999`, `dashc` passes it
/// through, and the Inigo Quilez form has no meaning above half the box — a
/// 50x30 half-box with radii 9999 read about 4085 units *outside* at every
/// point, so the painter drew nothing where the reference draws a pill.
#[test]
fn every_point_inside_a_pill_is_inside_the_shape() {
    let gpu = Gpu::new();
    let half = [50.0f32, 30.0];
    let points = [[0.0f32, 0.0], [40.0, 0.0], [0.0, 25.0], [-40.0, 0.0]];
    let probes: Vec<Probe> = points
        .iter()
        .map(|&p| Probe {
            v0: [9999.0, 9999.0, 9999.0, 9999.0],
            p,
            q: half,
            ..Probe::default()
        })
        .collect();
    for (&p, &d) in points
        .iter()
        .zip(&gpu.run("probe_rounded_box_sdf", &probes))
    {
        assert!(
            d < 0.0,
            "a point inside a pill is inside it, got {d} at {p:?}"
        );
    }
}

/// The gradient frames in the table can tell a wrong painter from a right one.
///
/// A fixture-validity property, over the committed file rather than over a
/// literal here, and it needs **two** statements because the two ways a painter
/// gets the frame wrong are caught by different frames:
///
/// - **A frame that is a similarity** — equal handle lengths and a right angle
///   between them — cannot tell an elliptical radial from a circular one, so
///   `gradient_radial_t` and `gradient_diamond_t` need a frame whose handles
///   differ in length.
/// - **An orthogonal frame cannot tell `gradient_local` from a projection onto
///   the primary axis alone**: for perpendicular handles the two agree
///   identically in x. So `gradient_linear_t` and `gradient_angular_t` need a
///   frame whose handles are oblique.
///
/// The first frame recorded is perpendicular (90.00 degrees) and 2.500 times
/// unequal, which covers the first; the second is oblique (61.56 degrees),
/// which covers the second. Measured from the committed file rather than
/// asserted: over frame 1's probes a primary-axis-only projection agrees with
/// the full frame to 0.000000, and over frame 2's it is out by 0.460.
#[test]
fn the_gradient_frames_in_the_file_can_fail_a_wrong_painter() {
    let table = table();
    // All four parameterizations, not one. They are four independent entries in
    // the file, and the two properties are assigned to different pairs of them:
    // a re-record that gave the radial its own frames would not be seen by a
    // guard that reads only the linear case.
    let cases: Vec<&FunctionCase> = table
        .functions
        .iter()
        .filter(|case| case.name.starts_with("gradient_") && case.name.ends_with("_t"))
        .collect();
    assert_eq!(
        cases.len(),
        4,
        "the table carries {} gradient parameterization(s), not the four this guard \
         is written over",
        cases.len()
    );

    for case in cases {
        // Two frames, pinned. Frame 2 sets both flags on its own, so deleting
        // frame 1 and re-recording left this guard green while halving the
        // probe count and the coverage.
        let mut frames: Vec<&[Value]> = Vec::new();
        for probe in &case.probes {
            let frame = &probe.args[1..4];
            if !frames.contains(&frame) {
                frames.push(frame);
            }
        }
        assert!(
            frames.len() >= 2,
            "{} carries {} distinct handle frame(s); the two properties below are \
             assigned to different ones and need both. Not an exact count — a \
             re-record that adds a third frame is more coverage, not less",
            case.name,
            frames.len()
        );
        let mut unequal = false;
        let mut oblique = false;
        for probe in &case.probes {
            let origin = vec2(&probe.args, 1);
            let primary = vec2(&probe.args, 2);
            let secondary = vec2(&probe.args, 3);
            let u = [primary[0] - origin[0], primary[1] - origin[1]];
            let v = [secondary[0] - origin[0], secondary[1] - origin[1]];
            let (len_u, len_v) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
            assert!(
                len_u > 1e-6 && len_v > 1e-6,
                "a handle of zero length makes the frame degenerate: {u:?}, {v:?}"
            );
            if (len_u / len_v - 1.0).abs() > 0.1 {
                unequal = true;
            }
            if (u[0] * v[0] + u[1] * v[1]).abs() / (len_u * len_v) > 0.1 {
                oblique = true;
            }
        }
        assert!(
            unequal,
            "every frame recorded for {} has handles of the same length, so dropping the \
         secondary handle from a radial would pass",
            case.name
        );
        assert!(
            oblique,
            "every frame recorded for {} is orthogonal, so projecting onto the primary \
         axis alone would pass",
            case.name
        );
    }
}

/// The stop ramp's recorded probes can fail.
///
/// Three fixture-validity properties over the committed file, each of which a
/// re-record could quietly remove:
///
/// - **The ramp must vary.** A function returning its first colour for every
///   `t` would pass a probe set whose samples were all clamped.
/// - **A zero-width **final** segment must be observed.** Two stops at one
///   offset is what Figma produces for a banded gradient, and it is the case a
///   division would have made a NaN of — but only where the walk ends there.
///   A repeat in the middle of a list is overwritten by the segment after it,
///   so this guard skips those and the mid-list banded case is a fixture choice
///   with nothing holding it.
/// - **The slots past `count` must be poison.** A slot past the count is one
///   the function must not read; an offset of -100 compares true against every
///   `t` and a colour of nines leaves the tolerance by three orders of
///   magnitude, so an over-running walk is loud. Zeroes there would be
///   indistinguishable from a black stop.
#[test]
fn the_stop_ramps_in_the_file_can_fail_a_wrong_painter() {
    let table = table();
    let case = table
        .functions
        .iter()
        .find(|case| case.name == "gradient_ramp")
        .expect("the table carries a stop-ramp case");
    let expected = case.expected();

    // **Within each stop list**, not across them, and this took two rounds to
    // get right.
    //
    // The first form flattened every component and compared against
    // `expected[0]` — probe 0's red, 1.0. Probe 0's own green channel is 0.0, a
    // whole colour away, so the guard was satisfied by a single probe and a
    // ramp answering one colour for every `t` passed it.
    //
    // Comparing whole colours across probes fixed that and left a second hole.
    // Every colour was still measured against probe 0's, and the recorded stop
    // lists have different first colours, so the guard only established that
    // the fixture uses more than one list. Seven of the eight could be
    // collapsed to a single colour each with it still green. Grouped by the
    // arguments after `t`, which is exactly the stop list.
    let components = case.components();
    let mut varying = 0usize;
    for (index, probe) in case.probes.iter().enumerate() {
        let first = case
            .probes
            .iter()
            .position(|other| other.args[1..] == probe.args[1..])
            .expect("a probe matches itself");
        if first == index {
            continue;
        }
        let here = &expected[index * components..(index + 1) * components];
        let base = &expected[first * components..(first + 1) * components];
        if here.iter().zip(base).any(|(a, b)| (a - b).abs() > 0.25) {
            varying += 1;
        }
    }
    // Every list with more than one distinct `t`, not merely one of them. As an
    // existence check this passed with seven of the eight lists collapsed to a
    // single colour, which is the fixture degrading exactly as the guard is
    // meant to prevent.
    let mut lists: Vec<&[Value]> = Vec::new();
    for probe in &case.probes {
        if !lists.contains(&&probe.args[1..]) {
            lists.push(&probe.args[1..]);
        }
    }
    for list in &lists {
        let rows: Vec<usize> = case
            .probes
            .iter()
            .enumerate()
            .filter(|(_, probe)| &probe.args[1..] == *list)
            .map(|(index, _)| index)
            .collect();
        let ts: std::collections::BTreeSet<String> = rows
            .iter()
            .map(|&index| format!("{}", scalar(&case.probes[index].args, 0)))
            .collect();
        if ts.len() < 2 {
            continue;
        }
        // A one-stop ramp answers the same colour at every `t` by definition —
        // there is no segment to interpolate — so it is exempt rather than a
        // failure. Found by this assertion when it was first written strictly,
        // which is the fixture behaving as its own comment says it should.
        if scalar(&case.probes[rows[0]].args, 3) < 2.0 {
            continue;
        }
        let base = &expected[rows[0] * components..(rows[0] + 1) * components];
        let varies_here = rows.iter().any(|&index| {
            expected[index * components..(index + 1) * components]
                .iter()
                .zip(base)
                .any(|(a, b)| (a - b).abs() > 0.25)
        });
        assert!(
            varies_here,
            "a recorded stop list is probed at {} distinct `t` values and answers one \
             colour at all of them, so a ramp returning its first colour would pass",
            ts.len()
        );
    }
    assert!(
        varying > 0,
        "no recorded stop list reaches more than one colour across its own `t` values"
    );

    // `gradient_segment_t`'s zero-width answer, observed directly.
    //
    // The guard here compared any two nearby `t` values of one list and took a
    // step as evidence. That accepted the **middle** hard stop, which
    // `sdf.wgsl`'s own comment says cannot observe the branch — the segment
    // after it overwrites whatever the zero-width one produced — so the
    // fixture's two observing cases could be deleted with the guard still
    // reporting the fixture adequate. Measured: deleting them and mutating
    // `gradient_segment_t` to return 0.0 passed all 26 tests.
    //
    // What observes it is a list whose **last** segment is zero-width, and the
    // observable is right-continuity: at that offset the answer is the later
    // stop's colour, which is what `gradient_segment_t` returning 1.0 produces
    // and returning 0.0 does not.
    let mut observed = 0usize;
    for (index, probe) in case.probes.iter().enumerate() {
        let offsets = floats(&probe.args, 1, MAX_GRADIENT_STOPS);
        let colours = vec4_list(&probe.args, 2, MAX_GRADIENT_STOPS);
        let count = scalar(&probe.args, 3) as usize;
        if count < 2 || offsets[count - 1] > offsets[count - 2] {
            continue;
        }
        if (scalar(&probe.args, 0) - offsets[count - 1]).abs() > 1e-6 {
            continue;
        }
        let answer = &expected[index * components..(index + 1) * components];
        let later: Vec<f64> = colours[count - 1].iter().map(|&v| v as f64).collect();
        let earlier: Vec<f64> = colours[count - 2].iter().map(|&v| v as f64).collect();
        assert!(
            answer
                .iter()
                .zip(&later)
                .all(|(a, b)| (a - b).abs() <= case.tolerance),
            "probe {index} sits at a zero-width final segment, where the ramp is \
             right-continuous and owes the later stop's colour {later:?}, not {answer:?}"
        );
        assert!(
            later
                .iter()
                .zip(&earlier)
                .any(|(a, b)| (a - b).abs() > 0.25),
            "probe {index}'s zero-width final segment joins two colours too alike to \
             tell the branch's two answers apart"
        );
        observed += 1;
    }
    assert!(
        observed > 0,
        "no recorded probe sits at the offset of a zero-width **final** segment, so \
         `gradient_segment_t`'s answer for one is unobserved. A repeated offset in the \
         middle of a list does not do it: the segment after it overwrites the result"
    );

    for (index, probe) in case.probes.iter().enumerate() {
        let offsets = floats(&probe.args, 1, MAX_GRADIENT_STOPS);
        let colours = vec4_list(&probe.args, 2, MAX_GRADIENT_STOPS);
        let count = scalar(&probe.args, 3) as usize;
        for slot in count..MAX_GRADIENT_STOPS {
            assert_eq!(
                offsets[slot], -100.0,
                "gradient_ramp probe {index} slot {slot} is past its count of {count} and \
                 must be poison, not {}",
                offsets[slot]
            );
            assert_eq!(
                colours[slot], [9.0; 4],
                "gradient_ramp probe {index} colour {slot} is past its count of {count} and \
                 must be poison"
            );
        }
    }
}
