//! What one more mid-frame render-target switch costs on this device (Q-6).
//!
//! `dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` is `8`, and the name
//! says what it is: a stand-in for a number nobody has measured. It is why
//! `paint.render-target-budget` is a **warning** rather than an error, and why
//! `crates/dashscene-validator/tests/scene.rs` builds its fixtures out of the
//! constant rather than out of a figure anyone chose. Issue #1128 is that gap,
//! and it is also issue #851's open question 1, on which "every composition-mode
//! conclusion depends".
//!
//! R-T1 makes one render pass per frame the rule and every mid-frame
//! render-target switch a tile-memory flush and resolve, and **the cost of that
//! flush is a property of the target GPU**. That is why the number cannot come
//! from a desktop, and why this is a device probe rather than a test.
//!
//! # Why a probe and not a scene
//!
//! The workspace already has a scene that forces one layer: `surfaces` in
//! `corpus/showcase` builds `tile-group` at `.opacity(0.55)` with two
//! deliberately overlapping children, which is what makes commit resolve a
//! `GroupComposite` rather than per-rect alpha — measured on 2026-08-17 as
//! exactly one group, against zero for `typography` and `layout`.
//!
//! What no scene offers is a **sweep**: the same frame with N layers and
//! everything else held constant, which is what turns a frame time into a cost
//! per layer. Authoring one would couple this measurement to the showcase
//! registry, to three hosts that enumerate it and to the golden images over it.
//! Sweeping the group table directly costs none of that, and it is the same
//! construction `layer3_render_smoke.rs` already uses to raise and drop a group.
//!
//! # What the number is, and what it is not
//!
//! **The answer is the slope, never the absolute.** `Renderer::render` submits
//! the frame and reads the target back through a staging buffer, and that
//! readback is a full-target copy on every frame — the same cost at every N. So
//! the per-frame figures below carry a large constant term that no device frame
//! pays, and only the **difference between N and N+1** is the layer cost.
//!
//! The readback is also what makes the timing possible at all. Nothing here asks
//! for `Features::TIMESTAMP_QUERY`: it is outside `downlevel_defaults`, so
//! requesting it would change the device request this project's painter makes,
//! and a probe whose device is not the painter's device measures the wrong
//! thing. Mapping the buffer is a fence, so wall-clock per frame is a real
//! serialised measurement rather than a queue-submission time.
//!
//! Two further limits, stated because a reader will otherwise take this for
//! more than it is:
//!
//! - **This is not a swapchain.** `Renderer::new` requests an adapter with
//!   `compatible_surface: None`, so it may not even be the adapter a windowed
//!   host picks — issue #890's caveat, which `adapter_report` carries too. A
//!   layer texture is the same object either way; the target it composites into
//!   is not.
//! - **Every layer here is the full target extent**, which is what the painter
//!   does: `LayerTargets`' own documentation gives the reason, and it is
//!   `dashscene-skia`'s choice as well. A tighter bound would change the cost,
//!   and it is not what the painter allocates.
//!
//! # Running it
//!
//! `just android-layer-cost` cross-compiles this, pushes it to
//! `/data/local/tmp` and runs it, exactly as `just android-probe` does for
//! `adapter_report`. On the host, `cargo run -p dashscene-gpu --example
//! layer_cost` — and running it on both is deliberate, because the two are
//! directly comparable and that is what makes a device figure legible.
//!
//! An emulator result describes the host machine's GPU behind a translation
//! layer. It is not the Q-6 measurement and must not be recorded as one.

use dashpaint::{
    ClipIndex, ClipTable, Color, GlyphRunTable, GroupComposite, ImageTable, PaintTable, Painter,
    RectEntry, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};

/// The extent every frame is drawn at.
///
/// 1920x1080 rather than something small, because the quantity being measured
/// scales with it: a layer is the full target extent, so the flush and resolve
/// this probe exists to price are proportional to the area. A tiny target would
/// measure a cost no product pays. It is also within
/// `downlevel_defaults().max_texture_dimension_2d`, which is 2048 — so this runs
/// on a device that only just meets the painter's own request.
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

/// How many layers to sweep to.
///
/// Past `RENDER_TARGET_BUDGET_PLACEHOLDER`, deliberately: the point is to see
/// whether 8 is anywhere near where the cost turns, and a sweep that stopped at
/// the placeholder could only ever agree with it.
///
/// **Overridable, with [`frames`] below, and that is the only bound this probe
/// has.** Every other step of the bundle takes a timeout — `DS_ATTACH_TIMEOUT`,
/// `DS_GPU_WINDOW`, `DS_FRAME_TIMEOUT` — and this one cannot: it is a plain
/// executable run through `adb shell`, with no host-side loop to bound it. Its
/// duration is `(max + 1) * (WARMUP + frames)` fenced frames, and each of
/// those reads a full target back through a staging buffer, so a device an order
/// of magnitude slower than this host turns seconds into minutes. Lower these
/// before a first run on unknown hardware:
///
///     adb shell 'DS_LAYER_MAX=6 DS_LAYER_FRAMES=30 /data/local/tmp/layer_cost'
///
/// A shorter sweep is a weaker measurement rather than a broken one — fewer
/// points and a larger standard error — and the resolution test below reports
/// that honestly instead of hiding it.
fn max_layers() -> usize {
    read_usize("DS_LAYER_MAX", 12)
}

/// Frames per measured point, after the warm-up below.
///
/// **120 because 30 was measured and was not enough.** On an Apple M3 host on
/// 2026-08-17, 30 frames per point produced a marginal column swinging between
/// -1.29 ms and +1.34 ms with no trend in it — noise an order of magnitude above
/// the quantity being measured, and a column of numbers that would have been
/// recorded as a per-layer cost. On that host the whole sweep is seconds either
/// way, so there is nothing to trade there; [`max_layers`] carries what a slower
/// device changes.
fn frames() -> usize {
    read_usize("DS_LAYER_FRAMES", 120)
}

/// One positive environment override, or the default.
///
/// Zero and unparseable both fall back rather than failing: this runs through
/// `adb shell` where a typo in a variable is invisible, and a sweep of zero frames
/// would divide by zero in `statistics`.
fn read_usize(name: &str, default: usize) -> usize {
    match std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        Some(value) if value > 0 => value,
        _ => default,
    }
}

/// How many standard errors the slope must clear to be reported as a cost.
///
/// Three, which the simulation in [`fit`] puts at a 1.1% false-positive rate
/// against pure noise while still resolving a 0.427 ms slope — one this host has
/// reported — in 99% of sweeps. It is deliberately a named constant rather than a literal in the
/// comparison: the number is the whole difference between this probe answering
/// #1128 and this probe inventing an answer to it.
const SIGMA: f64 = 3.0;

/// Frames discarded before measuring, per point.
///
/// The first frame at a new layer count allocates: a texture, a view, a uniform
/// buffer and a bind group **per layer** (`LayerTargets`), and the first frame
/// overall also builds the pipelines. Measured on the emulator, the first frame
/// of a point is an order of magnitude above the rest, which would put the whole
/// difference between two points into one frame.
const WARMUP: usize = 5;

/// Printed on every run, and the reason it is not optional.
///
/// The rule epic #1107 states is that nothing describes Android as working until
/// #885 is measured on target hardware, and emulator results stay labelled as
/// emulator results. A transcript is usually all that survives a run.
const CAVEAT: &str = "\
Read the SLOPE, never the absolute. Every frame here is read back through a
staging buffer — a full-target copy, the same cost at every layer count — so the
per-frame figures carry a constant term no device frame pays. The marginal
column is the measurement; the mean column is not a frame cost.

This is an offscreen target and not a swapchain, and `Renderer::new` asks for an
adapter with no compatible surface, so it need not be the adapter a windowed host
picks (issue #890).

An emulator result describes the host machine's GPU behind a translation layer.
It is NOT the Q-6 measurement (issue #1128) and must not be recorded as one.
";

fn rect(x: f32, y: f32, w: f32, h: f32, paint: dashpaint::PaintIndex) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }
}

/// The scene for one point of the sweep: `layers` groups of two overlapping
/// members each, plus one ungrouped member so a frame at zero layers still
/// draws.
///
/// **Overlapping is what makes them render-target groups at all.** Commit
/// resolves a non-overlapping group into per-rect `opacity` and produces no
/// `GroupComposite` — the free path — so a sweep built from separated rects would
/// measure nothing and report a flat line, which reads as "layers are free".
/// These are packed the way `dashscene-core`'s commit would pack them: each
/// group's `[start, end)` names a contiguous rect range.
fn scene(
    paints: &mut PaintTable,
    layers: usize,
    max: usize,
) -> (Vec<RectEntry>, Vec<GroupComposite>) {
    let ink = paints.push_solid(Color {
        r: 0.1,
        g: 0.1,
        b: 0.12,
        a: 1.0,
    });
    let mut rects = vec![rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32, ink)];
    let mut groups = Vec::with_capacity(layers);
    for index in 0..layers {
        let shade = index as f32 / max as f32;
        let paint = paints.push_solid(Color {
            r: 0.9 - shade * 0.6,
            g: 0.3 + shade * 0.5,
            b: 0.5,
            a: 1.0,
        });
        // Spread across the target so the groups are not all in one tile: a
        // tiling GPU flushes the tiles a pass touches, and stacking every layer
        // in one place would price the narrowest possible case.
        let step = (WIDTH as f32 - 400.0) / max as f32;
        let x = 40.0 + step * index as f32;
        let y = 40.0 + step * 0.4 * index as f32;
        let start = rects.len() as u32;
        rects.push(rect(x, y, 320.0, 320.0, paint));
        // Offset by less than its own size, so the two overlap.
        rects.push(rect(x + 120.0, y + 120.0, 320.0, 320.0, paint));
        groups.push(GroupComposite {
            start,
            end: rects.len() as u32,
            alpha: 0.55,
        });
    }
    (rects, groups)
}

/// One point of the sweep.
struct Point {
    layers: usize,
    /// **The statistic the slope is fitted over**, and the choice matters more
    /// than anything else here.
    ///
    /// Every source of noise in this measurement — scheduling, another process on
    /// the GPU, a thermal step — makes a frame *slower*. None makes one faster
    /// than the work in it, so the minimum is the only estimator that is not
    /// contaminated by them, and it is the standard choice for a microbenchmark
    /// for exactly that reason. The median moves with the load on the machine;
    /// this does not.
    min: f64,
    p50: f64,
    max: f64,
}

/// Minimum, median and maximum of the frame times at one point, in milliseconds.
fn statistics(layers: usize, mut times: Vec<f64>) -> Point {
    times.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
    Point {
        layers,
        min: times[0],
        p50: times[times.len() / 2],
        max: times[times.len() - 1],
    }
}

/// Least squares over the points' minima: the cost of one layer, in
/// milliseconds, the root-mean-square residual around that line, and **the
/// standard error of the slope itself**.
///
/// **Fitted over every point rather than differenced between adjacent ones.** A
/// difference between two noisy numbers has more noise than either; a slope over
/// thirteen points has less.
///
/// # Why the standard error and not the residual
///
/// The caller has to decide whether the slope means anything, and the residual
/// alone cannot answer that: it is the scatter of the *points*, where the
/// question is about the uncertainty of the *slope*, and the two differ by
/// `sqrt(sum(dx^2))` — a factor of 13.5 over this sweep. Comparing the slope
/// against the residual spread over the sweep's width is a **1.12-sigma** test,
/// and a 1.12-sigma test admits about a third of pure noise: simulated over
/// 20000 trials at a true slope of zero, that form declared 32% of sweeps
/// resolved, at every noise level, because the test is scale-free. This function
/// returns the quantity that makes a real threshold possible.
///
/// The residual is scaled to an estimate of sigma by `sqrt(n / (n - 2))`, since
/// two parameters were fitted and the population RMS is biased low by exactly
/// that.
fn fit(points: &[Point]) -> (f64, f64, f64) {
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.layers as f64).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.min).sum::<f64>() / n;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for point in points {
        let dx = point.layers as f64 - mean_x;
        covariance += dx * (point.min - mean_y);
        variance += dx * dx;
    }
    // A single point cannot have a slope, and the sweep is a constant, so this
    // is a guard against a future edit rather than against an input. `n <= 2`
    // is guarded with it: the sigma correction below divides by `n - 2`.
    if variance == 0.0 || n <= 2.0 {
        return (0.0, 0.0, f64::INFINITY);
    }
    let slope = covariance / variance;
    let intercept = mean_y - slope * mean_x;
    let residual = (points
        .iter()
        .map(|p| {
            let predicted = intercept + slope * p.layers as f64;
            (p.min - predicted).powi(2)
        })
        .sum::<f64>()
        / n)
        .sqrt();
    let sigma = residual * (n / (n - 2.0)).sqrt();
    (slope, residual, sigma / variance.sqrt())
}

fn main() {
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            // The same failure `adapter_report` reports, reached the same way.
            // Non-zero, so this is usable as a gate rather than only as a
            // report.
            println!("layer_cost: no usable device — {error:?}");
            println!();
            println!("{CAVEAT}");
            std::process::exit(1);
        }
    };
    let max = max_layers();
    let frames = frames();
    let info = renderer.adapter_info();
    println!(
        "layer_cost: {} | backend {:?} | device_type {:?} | driver {} {}",
        info.name, info.backend, info.device_type, info.driver, info.driver_info
    );
    println!(
        "layer_cost: {WIDTH}x{HEIGHT}, {frames} frames per point after {WARMUP} \
         discarded, sweeping 0..={max} layers"
    );
    println!();
    println!("layers   min ms   p50 ms   max ms   marginal min ms");

    let mut points = Vec::with_capacity(max + 1);
    let mut previous: Option<f64> = None;
    for layers in 0..=max {
        let mut paints = PaintTable::new();
        let (rects, groups) = scene(&mut paints, layers, max);
        let clips = ClipTable::new();
        let images = ImageTable::new();
        let glyphs = GlyphRunTable::new();

        let mut times = Vec::with_capacity(frames);
        for frame in 0..WARMUP + frames {
            // **Packed inside the loop but outside the timer**, and both halves
            // of that are deliberate.
            //
            // Inside the loop because a buffer packed once would leave `render`
            // uploading an unchanged buffer, which is not the path a host takes.
            //
            // Outside the timer because the pack is CPU work whose own cost
            // **also** grows with the layer count — two more rects and one more
            // group per point — so timing it would add a second term to the slope
            // and Q-6 could not tell the tile-memory flush from the packing. An
            // earlier revision of this comment claimed the pack "belongs in the
            // figure", which the timer's position contradicted; review caught the
            // contradiction, and the timer's position is the half that was right.
            let mut painter = GpuPainter::new();
            painter.paint(&rects, &paints, &images, &clips, &groups, &glyphs, None);
            let started = std::time::Instant::now();
            let drawn = renderer.render(
                painter.instances(),
                &paints,
                &images,
                &clips,
                &glyphs,
                WIDTH,
                HEIGHT,
            );
            let took = started.elapsed();
            match drawn {
                Ok(_) => {}
                Err(error) => {
                    println!();
                    println!("layer_cost: {layers} layer(s) failed to render — {error:?}");
                    println!();
                    println!("{CAVEAT}");
                    std::process::exit(1);
                }
            }
            if frame >= WARMUP {
                times.push(took.as_secs_f64() * 1000.0);
            }
        }

        let point = statistics(layers, times);
        // Shown against the previous point so a divergence from a straight line
        // is visible, and **not** the figure to quote: the fit below is. A
        // difference between two noisy numbers carries more noise than either of
        // them.
        let marginal = match previous {
            Some(before) => format!("{:+.3}", point.min - before),
            None => "—".to_owned(),
        };
        previous = Some(point.min);
        println!(
            "{layers:>6}  {:>7.3}  {:>7.3}  {:>7.3}   {marginal:>9}",
            point.min, point.p50, point.max
        );
        points.push(point);
    }

    let (slope, residual, standard_error) = fit(&points);
    println!();
    // **The comparison that decides whether there is an answer at all**, and it
    // is a test on the slope's own uncertainty rather than on the points'
    // scatter. When the slope does not clear it, this probe has measured that the
    // cost is smaller than it can see, which is a real result and is not a
    // number.
    //
    // **`SIGMA` standard errors, and the first draft of this got it wrong in a
    // way worth recording.** It compared the slope spread over the sweep's width
    // against the residual, which reads as an honest test and is a 1.12-sigma
    // one: the residual is the scatter of the points and the slope's uncertainty
    // is smaller than it by `sqrt(sum(dx^2))`, a factor of 13.5 here. Simulated
    // over 20000 sweeps at a true slope of zero, that form declared **32%** of
    // pure noise resolved — at every noise level, since the test is scale-free —
    // and would have printed a fabricated per-layer cost into #1128 about one run
    // in three. At three standard errors the same simulation gives 1.1%, and the
    // host's measured slope of 0.427 ms is still resolved in 99% of sweeps at
    // more than twice its own noise.
    let threshold = SIGMA * standard_error;
    if slope.abs() > threshold {
        println!(
            "layer_cost: one more render-target layer costs {slope:+.4} ms \
             ± {standard_error:.4} ms (1 s.e., fit over {} points; residual \
             {residual:.4} ms)",
            points.len()
        );
    } else {
        println!(
            "layer_cost: BELOW THIS PROBE'S RESOLUTION on this device. The fitted \
             slope is {slope:+.4} ms per layer with a standard error of \
             {standard_error:.4} ms, so it does not clear the {SIGMA}-sigma \
             threshold of {threshold:.4} ms. The cost of a layer is smaller than \
             what this measurement can separate from noise. Record that, not the \
             slope."
        );
    }
    println!(
        "layer_cost: {max} layers were swept, against \
         dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER = 8"
    );
    println!(
        "layer_cost: allocations across the sweep: {}",
        renderer.allocations()
    );
    println!();
    println!("{CAVEAT}");
}
