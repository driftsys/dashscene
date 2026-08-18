//! What a frame costs the **GPU**, from the device's own timestamps.
//!
//! Every other number this project has about a frame is wall-clock on the CPU
//! side. `demo-android`'s instrument times `paint` and `submit`; `submit` spans
//! the swapchain acquire and the buffer handoff, and a Perfetto trace of a
//! Pixel 5 showed why that is not GPU time: of about 18 ms inside `present`,
//! `QueueSubmit` — the call that hands work to the device — was **0.12 ms**, and
//! the rest was `AcquireNextImageKHR` and `QueuePresentKHR` waiting on buffers.
//! The GPU runs asynchronously after that and appears in none of it.
//!
//! # Why timestamps rather than counters
//!
//! On a retail Android device there is no other route, which was established by
//! trying all of them on a Pixel 5 (Android 14, user build):
//!
//! - `adb shell perfetto --query` registers **no `gpu.counters`** and no
//!   `gpu.renderstages`. Qualcomm does not ship the Perfetto GPU producer there;
//!   Snapdragon Profiler is the vendor route.
//! - The `kgsl` and `dma_fence` ftrace tracepoints **exist** in
//!   `/sys/kernel/tracing/events` and **will not enable** under `traced_probes`
//!   — a 20 s trace with the painter drawing at 60 fps recorded zero of them,
//!   against 75 000 `sched_switch`. Android's user build allowlists ftrace
//!   events and vendor tracepoints are not on it.
//! - `/sys/class/kgsl/kgsl-3d0/gpu_busy_percentage` is `Permission denied` to
//!   `shell`, and the device has no `su`.
//!
//! What is left is the device's own timestamp queries, and the Adreno 620's
//! Vulkan adapter reports all four query features. `adapter_report` prints them,
//! so an adapter that forecloses this says so before anything is built.
//!
//! # What it measures, and what it does not
//!
//! **Whole-frame GPU execution, offscreen.** The `gpu-timing` feature brackets
//! the frame's command encoder with two timestamps, so the figure covers every
//! pass a frame encodes — the paint pass, one per render-target layer, two more
//! per backdrop blur — without summing passes the device may overlap.
//!
//! It is **not** the on-screen path. There is no swapchain here, so nothing
//! waits on a buffer and nothing is presented; that is the point, since the
//! swapchain is what the CPU-side numbers were already dominated by. What this
//! adds is the term none of them contained.
//!
//! **The device request differs from the painter's by one feature**, and that is
//! the reason `gpu-timing` is off by default. `Cargo.toml` carries the argument.
//!
//! # Running it
//!
//!     cargo run -p dashscene-gpu --features gpu-timing --example gpu_time
//!     just android-gpu-time          # cross-compiled, pushed, run over adb
//!
//! An emulator result describes the host machine's GPU behind a translation
//! layer and is not a device measurement.

use dashpaint::{
    ClipIndex, ClipTable, Color, GlyphRunTable, GroupComposite, ImageTable, PaintTable, Painter,
    RectEntry, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};

/// The extent each frame is drawn at.
///
/// 1280x720 because that is the geometry the target budget is stated over. The
/// figure scales with area for anything fill-bound, so an extent that is not the
/// one being budgeted for would have to be scaled by hand — and the wall-clock
/// measurements already showed that scaling by area is not safe to assume.
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

/// Frames per point, after the warm-up.
///
/// The first frames at a new scene allocate and compile; `layer_cost.rs`
/// measured a first frame an order of magnitude above the rest, which is why
/// both probes discard some.
/// Overridden by `DS_GPU_FRAMES`, because a plain executable under
/// `adb shell` has no host-side timeout: `layer_cost.rs` records the same
/// reasoning, and a device an order of magnitude slower than this one turns a
/// sweep of seconds into a sweep of minutes with no way to shorten it. Six rows
/// at 60 frames plus 10 warm-up is 420 full-target readbacks.
fn frames() -> usize {
    read_usize("DS_GPU_FRAMES", 60)
}

/// Overridden by `DS_GPU_WARMUP`, for the same reason.
fn warmup() -> usize {
    read_usize("DS_GPU_WARMUP", 10)
}

/// One positive environment override, or the default.
///
/// Zero and unparseable both fall back rather than failing: this runs through
/// `adb shell`, where a typo in a variable name is invisible, and a sweep of
/// zero measured frames would report every row absent for a reason that is not
/// the device's.
fn read_usize(name: &str, default: usize) -> usize {
    match std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        Some(value) if value > 0 => value,
        _ => default,
    }
}

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

/// A frame of `rects` plain quads, and `layers` render-target groups over them.
///
/// The same *kind* of construction as `layer_cost.rs` — plain quads plus
/// render-target groups over them — but **not the same scene, and the two are
/// not directly comparable**. That probe runs at 1920x1080 with no content
/// rects, places its group quads differently, and uses 5 warm-up frames and
/// 120 measured; this one runs at 1280x720 with 0, 8 or 32 content rects and
/// 10 warm-up frames. The extent alone differs by 2.25x, which on a
/// fill-rate-bound cost is the dominant term. Read each probe against itself.
fn scene(
    paints: &mut PaintTable,
    rects_wanted: usize,
    layers: usize,
) -> (Vec<RectEntry>, Vec<GroupComposite>) {
    let ink = paints.push_solid(Color {
        r: 0.10,
        g: 0.10,
        b: 0.12,
        a: 1.0,
    });
    let mut rects = vec![rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32, ink)];
    for index in 0..rects_wanted {
        let shade = index as f32 / rects_wanted.max(1) as f32;
        let paint = paints.push_solid(Color {
            r: 0.9 - shade * 0.6,
            g: 0.3 + shade * 0.5,
            b: 0.5,
            a: 1.0,
        });
        let step = (WIDTH as f32 - 320.0) / rects_wanted.max(1) as f32;
        rects.push(rect(
            20.0 + step * index as f32,
            20.0 + step * 0.3 * index as f32,
            300.0,
            300.0,
            paint,
        ));
    }
    let mut groups = Vec::with_capacity(layers);
    for index in 0..layers {
        let paint = paints.push_solid(Color {
            r: 0.2,
            g: 0.6,
            b: 0.9,
            a: 1.0,
        });
        let start = rects.len() as u32;
        let x = 40.0 + 60.0 * index as f32;
        rects.push(rect(x, 40.0, 320.0, 320.0, paint));
        rects.push(rect(x + 120.0, 160.0, 320.0, 320.0, paint));
        groups.push(GroupComposite {
            start,
            end: rects.len() as u32,
            alpha: 0.55,
        });
    }
    (rects, groups)
}

fn statistics(mut values: Vec<f64>) -> (f64, f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
    let min = values[0];
    let p50 = values[values.len() / 2];
    let max = values[values.len() - 1];
    (min, p50, max)
}

const CAVEAT: &str = "\
GPU time only, offscreen. There is no swapchain here, so this excludes the
acquire and the present that dominate a windowed frame's wall-clock — which is
the point: those were already measured and this is the term they did not
contain.

The device request carries two features the shipped painter does not ask for —
TIMESTAMP_QUERY and TIMESTAMP_QUERY_INSIDE_ENCODERS — which is why `gpu-timing`
is off by default. An adapter offering the first without the second reports
every row absent rather than a number.

A dash is an absent reading, never a zero: the feature off, no timestamp
queries on the adapter, a pair that did not read back or did not increase, or
an instrument retired after a failed map.

The adapter is chosen with no compatible surface (issue #890), so a device that
sorts adapters by surface support may pick a different one here than a windowed
host would.

An emulator result describes the host machine's GPU behind a translation layer.
";

fn main() {
    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            println!("gpu_time: no usable device — {error:?}");
            println!();
            println!("{CAVEAT}");
            std::process::exit(1);
        }
    };
    let info = renderer.adapter_info();
    println!(
        "gpu_time: {} | backend {:?} | driver {}",
        info.name, info.backend, info.driver
    );
    let frames = frames();
    let warmup = warmup();
    println!("gpu_time: {WIDTH}x{HEIGHT}, {frames} frames per row after {warmup} discarded");
    println!();
    println!("  rects  layers    gpu min ms   gpu p50 ms   gpu max ms");

    // A sweep in both the things a frame is made of: how much there is to draw,
    // and how many times the target is switched. One row is not a measurement of
    // anything; the differences between rows are.
    let rows: [(usize, usize); 6] = [(0, 0), (8, 0), (32, 0), (32, 1), (32, 4), (32, 8)];
    let mut measured = false;

    for (rects_wanted, layers) in rows {
        let mut paints = PaintTable::new();
        let (rects, groups) = scene(&mut paints, rects_wanted, layers);
        let clips = ClipTable::new();
        let images = ImageTable::new();
        let glyphs = GlyphRunTable::new();
        let mut samples = Vec::with_capacity(frames);

        for frame in 0..warmup + frames {
            let mut painter = GpuPainter::new();
            painter.paint(&rects, &paints, &images, &clips, &groups, &glyphs, None);
            let drawn = renderer.render(
                painter.instances(),
                &paints,
                &images,
                &clips,
                &glyphs,
                WIDTH,
                HEIGHT,
            );
            if let Err(error) = drawn {
                println!();
                println!("gpu_time: {rects_wanted} rects / {layers} layers failed — {error:?}");
                std::process::exit(1);
            }
            if frame >= warmup {
                // An absent reading is skipped rather than pushed as a zero.
                // `Renderer::last_gpu_time` enumerates what absent covers;
                // `measured` is what separates "every frame was absent" from a
                // row of real zeros.
                if let Some(gpu) = renderer.last_gpu_time() {
                    samples.push(gpu.as_secs_f64() * 1000.0);
                    measured = true;
                }
            }
        }

        // A dash under every column, not one dash where the header promises
        // three: a short row reads as a formatting accident, and this is the
        // path an adapter without `TIMESTAMP_QUERY_INSIDE_ENCODERS` takes for
        // every row.
        if samples.is_empty() {
            println!(
                "  {rects_wanted:>5}  {layers:>6}    {:>10}   {:>10}   {:>10}",
                "—", "—", "—"
            );
            continue;
        }
        // A row assembled from fewer frames than were asked for is marked, so
        // it cannot be read beside a complete row as though the two carry the
        // same weight.
        let short = samples.len() < frames;
        let taken = samples.len();
        let (min, p50, max) = statistics(samples);
        print!("  {rects_wanted:>5}  {layers:>6}    {min:>10.3}   {p50:>10.3}   {max:>10.3}");
        if short {
            print!("   ({taken} of {frames} frames read back)");
        }
        println!();
    }

    println!();
    if !measured {
        println!(
            "gpu_time: NO TIMESTAMPS READ BACK. The `gpu-timing` feature is on — this \
             binary could not have been built otherwise — so the adapter offered no \
             timestamp queries, or the pair did not resolve. Nothing here is a \
             measurement of zero."
        );
    }
    println!("{CAVEAT}");
}
