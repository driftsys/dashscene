//! Renders one committed frame through both painters, offscreen, and reports
//! where they disagree.
//!
//! # What question this answers
//!
//! Watching the showcase host swap painters shows that the two pictures are not
//! the same, but it cannot say *why*, because the two paths differ in more than
//! the painter: `dashscene-skia` rasters to CPU memory and posts it through
//! `softbuffer` (a CoreGraphics image on macOS, tagged `DeviceRGB`), while
//! `dashscene-gpu` presents a `Bgra8Unorm` swapchain the window system colour
//! matches on its own terms. A difference seen on screen could come from either
//! end, and the two have opposite remedies.
//!
//! This takes both windowing paths out of the comparison. Both painters draw the
//! same [`dashscene_core::CommittedScene`] into offscreen memory, and both
//! readbacks are unpremultiplied RGBA8888 — `SkiaPainter::rgba_bytes` and
//! `Renderer::render`, which `crates/dashscene-gpu/src/render.rs` documents as
//! returning "unpremultiplied RGBA8, the space `goldens/README.md` compares in".
//! What is left is the painters.
//!
//! # The discriminator: a flat neighbourhood
//!
//! The two painters are expected to disagree along every edge. They antialias
//! differently — analytic SDF coverage against Skia's scan conversion — so a
//! partially covered pixel gets a different alpha, and after compositing that
//! reads as a different colour. Counting differing pixels therefore says almost
//! nothing on its own.
//!
//! So each differing pixel is also classified. A pixel is **flat** when its
//! whole 3x3 neighbourhood is byte-identical to it in *both* images: both
//! painters agree the region is uniform, and they still disagree about what
//! colour it is. That cannot be an antialiasing, sampling or geometry artifact
//! — there is no edge there for either painter to resolve differently. A
//! population of flat differing pixels with a consistent signed delta is the
//! signature of a colour-pipeline defect; its absence says the pipeline agrees
//! and every difference on screen is content at an edge.
//!
//! # The control scene
//!
//! The showcase scenes cannot answer the pipeline question by themselves.
//! Almost every pixel in them that differs is an edge — a glyph outline, a
//! corner radius, a circle, a blur falloff — and a difference at an edge says
//! nothing about whether the two painters agree about a colour. A scene can
//! also carry a construct one painter does not draw at all, which moves a whole
//! region for a reason that has nothing to do with colour; that was true of
//! `surfaces` and its backdrop blur until story #733 landed, and it will be
//! true again of the next construct the lean painter reaches last.
//!
//! `control` is therefore built here rather than taken from the corpus: large
//! solid rectangles and nothing else — no text, no image, no blur, no stroke.
//! Three of its four bands are opaque and one is half alpha over an opaque
//! backdrop, so the fill path and the blend path are both covered. Every
//! interior pixel of it is flat. If the opaque bands differ, the colour
//! pipeline is wrong; if they are byte-identical, it is not.
//!
//! # This is a measurement, not a gate
//!
//! An example rather than a test, for the reason `corpus/showcase/examples/
//! still.rs` is one and for the reason story #586 states: fidelity needs real
//! hardware, CI has none, and a band tuned on a software adapter would drift
//! with the runner image while saying nothing about a real driver. Nothing here
//! asserts. It prints numbers and writes pictures, with the adapter named beside
//! them, and a person reads them.
//!
//! ```text
//! cargo run -p goldens --example painter-diff
//! cargo run -p goldens --example painter-diff -- 1920 1200 target/painter-diff
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dashpaint::{Color, Painter};
use dashscene_core::{Arena, CommittedScene, Prop};
use dashscene_gpu::{GpuPainter, Renderer};
use dashscene_skia::SkiaPainter;

/// The fixed step the scenes are advanced with. A wall clock is deliberately not
/// read, for the reason the `still` example gives: the same arguments must
/// produce the same frame on any machine, or a number measured here cannot be
/// compared against the same number measured later.
const STEP: f32 = 1.0 / 60.0;

/// How long each showcase scene runs before its frame is taken, and which
/// scripted phase it runs towards — the `still` example's own defaults, so the
/// frame compared here is the frame that example already renders.
const SECONDS: f32 = 1.2;
const PHASE: u64 = 1;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let width: u32 = parse(args.first(), 1440).max(1);
    let height: u32 = parse(args.get(1), 900).max(1);
    let out: PathBuf = args
        .get(2)
        .map_or_else(|| PathBuf::from("target/painter-diff"), PathBuf::from);

    let mut renderer = match Renderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("painter-diff: no device: {error}");
            std::process::exit(1);
        }
    };
    let info = renderer.adapter_info();
    println!("painter-diff: {} ({:?})", info.name, info.backend);
    println!("painter-diff: driver {} {}", info.driver, info.driver_info);
    println!(
        "painter-diff: {width}x{height}, output in {}",
        out.display()
    );
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("painter-diff: creating {}: {error}", out.display());
        std::process::exit(1);
    }

    // The control first: it is the one scene whose answer is unambiguous, and
    // reading it before the showcase numbers is what tells you how to read them.
    let mut arena = Arena::new();
    control(&mut arena, width, height);
    measure(
        "control",
        arena.committed(),
        width,
        height,
        &mut renderer,
        &out,
    );

    for scene in showcase::SCENES {
        let mut arena = Arena::new();
        let mut live = (scene.build)(&mut arena, width, height);
        (scene.pulse)(&mut live, PHASE);
        let steps = (SECONDS / STEP).round().max(0.0) as u32;
        for _ in 0..steps {
            live.tick(STEP, &mut arena);
        }
        measure(
            scene.name,
            arena.committed(),
            width,
            height,
            &mut renderer,
            &out,
        );
    }
}

/// Draws `committed` with both painters and reports the comparison.
///
/// # Why the renderer is told to forget first
///
/// One [`Renderer`] draws every scene here, and each scene is a **separate
/// arena**. That is exactly the case `Renderer::forget_uploaded` exists for:
/// its own documentation calls it the "these commits come from a different
/// chain" call, and it is what clears residency. Residency is keyed by the
/// image table's own row, and a fresh arena starts that table again from zero,
/// so the same key can name a different picture across two scenes.
///
/// Getting this wrong would be worse here than almost anywhere else. A resident
/// slot is returned without reading the payload's bytes — the digest that would
/// catch a collision is `#[cfg(debug_assertions)]`, so a release run is silent —
/// and a collision draws one scene's texels under another scene's key. In this
/// tool that surfaces as a **flat** disagreement over a whole region, which is
/// precisely the signal the flat classification exists to report as a
/// colour-pipeline defect. The instrument would accuse the painters of its own
/// bug.
fn measure(
    name: &str,
    committed: &CommittedScene,
    width: u32,
    height: u32,
    renderer: &mut Renderer,
    out: &Path,
) {
    renderer.forget_uploaded();

    let mut skia_painter = SkiaPainter::new(width as i32, height as i32);
    skia_painter.paint(
        committed.rects(),
        committed.paints(),
        committed.images(),
        committed.clips(),
        committed.groups(),
        committed.glyphs(),
        None,
    );
    let skia = skia_painter.rgba_bytes();

    let mut gpu_painter = GpuPainter::new();
    gpu_painter.paint(
        committed.rects(),
        committed.paints(),
        committed.images(),
        committed.clips(),
        committed.groups(),
        committed.glyphs(),
        None,
    );
    // Checked rather than left to `render`, which **panics** on an empty frame
    // rather than returning `Err` — so the graceful arm below could not catch
    // it, and one empty scene would abort the run before the later scenes were
    // measured. No current scene is empty; the guard costs one comparison.
    if gpu_painter.instances().instances().is_empty() {
        println!("\n{name}: the lean painter packed no instances, so there is nothing to compare");
        return;
    }
    let gpu = match renderer.render(
        gpu_painter.instances(),
        committed.paints(),
        committed.images(),
        committed.clips(),
        committed.glyphs(),
        width,
        height,
    ) {
        Ok(pixels) => pixels,
        Err(error) => {
            println!("\n{name}: the lean painter drew nothing: {error}");
            return;
        }
    };

    println!(
        "\n{name} — {} rects, {} paints, {} glyph runs, {} images, {} groups",
        committed.rects().len(),
        committed.paints().len(),
        committed.glyphs().runs().len(),
        committed.images().len(),
        committed.groups().len(),
    );
    if skia.len() != gpu.len() {
        println!(
            "  the two readbacks are different sizes: {} and {}",
            skia.len(),
            gpu.len()
        );
        return;
    }

    let report = compare(&skia, &gpu, width, height);
    report.print();

    write_png(&out.join(format!("{name}-skia.png")), &skia, width, height);
    write_png(&out.join(format!("{name}-gpu.png")), &gpu, width, height);
    write_png(
        &out.join(format!("{name}-delta.png")),
        &delta_image(&skia, &gpu, width, height),
        width,
        height,
    );
}

/// What the comparison found.
struct Report {
    total: usize,
    differing: usize,
    max_delta: u8,
    /// Differing pixels by how far apart they are: 1-2, 3-8, 9-32, 33+ code
    /// points on the worst channel. The first bucket is where a rounding
    /// difference lands and the last is where a missing construct does.
    histogram: [usize; 4],
    /// Differing pixels whose 3x3 neighbourhood is uniform in both images —
    /// see the module documentation for why this is the number that matters.
    flat: usize,
    /// The same buckets as [`Report::histogram`], over the flat pixels alone.
    /// This is the distribution that separates "the painters round the last bit
    /// differently" from "the painters disagree about a colour": the first is
    /// entirely in the 1-2 bucket and the second is not.
    flat_histogram: [usize; 4],
    /// Of those, the ones both painters drew fully opaque. Unpremultiplying
    /// amplifies a small absolute error at low alpha, so the opaque subset is
    /// the one whose delta means what it reads as.
    flat_opaque: usize,
    /// Summed signed delta over the flat opaque pixels, per channel. A
    /// systematic shift has a mean far from zero; rounding noise averages out.
    signed: [i64; 4],
    /// The flat pixel the two painters disagree about most.
    worst_flat: Option<(u32, u32, [u8; 4], [u8; 4])>,
    /// How often each distinct flat disagreement occurs, keyed by the pair of
    /// colours. A flat region is one colour by definition, so a tile the two
    /// painters draw in different shades appears here as one entry with its
    /// area as the count — which is what attributes a flat population to a
    /// construct without a region-labelling pass.
    pairs: HashMap<([u8; 4], [u8; 4]), usize>,
}

impl Report {
    fn print(&self) {
        let percent = |n: usize| 100.0 * n as f64 / self.total as f64;
        println!(
            "  differing pixels     {:>9} of {} ({:.3} %)",
            self.differing,
            self.total,
            percent(self.differing)
        );
        println!("  worst channel delta  {:>9}", self.max_delta);
        println!(
            "  by delta             1-2 {}, 3-8 {}, 9-32 {}, 33+ {}",
            self.histogram[0], self.histogram[1], self.histogram[2], self.histogram[3]
        );
        println!(
            "  FLAT differing       {:>9} ({:.4} %) — no edge for either painter to resolve",
            self.flat,
            percent(self.flat)
        );
        println!(
            "  ... by delta         1-2 {}, 3-8 {}, 9-32 {}, 33+ {}",
            self.flat_histogram[0],
            self.flat_histogram[1],
            self.flat_histogram[2],
            self.flat_histogram[3]
        );
        println!(
            "  ... of them opaque   {:>9} ({:.4} %)",
            self.flat_opaque,
            percent(self.flat_opaque)
        );
        if self.flat_opaque > 0 {
            let mean = |c: usize| self.signed[c] as f64 / self.flat_opaque as f64;
            println!(
                "  mean signed delta    r {:+.3}  g {:+.3}  b {:+.3}  a {:+.3}",
                mean(0),
                mean(1),
                mean(2),
                mean(3)
            );
        }
        match self.worst_flat {
            Some((x, y, skia, gpu)) => {
                println!("  worst flat pixel     ({x}, {y}) skia {skia:?} gpu {gpu:?}")
            }
            None => {
                println!("  worst flat pixel     none — every uniform region agrees byte for byte")
            }
        }
        if self.pairs.is_empty() {
            return;
        }
        let mut ranked: Vec<_> = self.pairs.iter().collect();
        // By area, then by the colours themselves so a tie does not reorder
        // between runs — a report a person compares against an earlier one must
        // not move for reasons that are not the picture.
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        println!("  the flat disagreements, largest area first:");
        for ((skia, gpu), count) in ranked.into_iter().take(6) {
            let delta = (0..4).map(|c| skia[c].abs_diff(gpu[c])).max().unwrap_or(0);
            println!("    {count:>8} px  skia {skia:?} -> gpu {gpu:?}  (delta {delta})");
        }
    }
}

/// Compares two unpremultiplied RGBA8888 readbacks of the same extent.
fn compare(skia: &[u8], gpu: &[u8], width: u32, height: u32) -> Report {
    let mut report = Report {
        total: (width as usize) * (height as usize),
        differing: 0,
        max_delta: 0,
        histogram: [0; 4],
        flat: 0,
        flat_histogram: [0; 4],
        flat_opaque: 0,
        signed: [0; 4],
        worst_flat: None,
        pairs: HashMap::new(),
    };
    let mut worst_flat_delta = 0u8;

    for y in 0..height {
        for x in 0..width {
            let a = pixel(skia, width, x, y);
            let b = pixel(gpu, width, x, y);
            if a == b {
                continue;
            }
            report.differing += 1;
            let delta = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            report.max_delta = report.max_delta.max(delta);
            report.histogram[bucket(delta)] += 1;

            if !flat(skia, width, height, x, y) || !flat(gpu, width, height, x, y) {
                continue;
            }
            report.flat += 1;
            report.flat_histogram[bucket(delta)] += 1;
            *report.pairs.entry((a, b)).or_insert(0) += 1;
            if a[3] == 255 && b[3] == 255 {
                report.flat_opaque += 1;
                for c in 0..4 {
                    report.signed[c] += i64::from(b[c]) - i64::from(a[c]);
                }
            }
            if delta > worst_flat_delta {
                worst_flat_delta = delta;
                report.worst_flat = Some((x, y, a, b));
            }
        }
    }
    report
}

/// Which histogram bucket a per-pixel worst-channel delta falls in.
fn bucket(delta: u8) -> usize {
    match delta {
        0..=2 => 0,
        3..=8 => 1,
        9..=32 => 2,
        _ => 3,
    }
}

/// Whether `(x, y)`'s whole 3x3 neighbourhood holds the same pixel it does.
///
/// A border pixel is never flat: it has no full neighbourhood, and treating a
/// truncated one as uniform would count the frame's own edge as a colour
/// disagreement.
fn flat(buffer: &[u8], width: u32, height: u32, x: u32, y: u32) -> bool {
    if x == 0 || y == 0 || x + 1 >= width || y + 1 >= height {
        return false;
    }
    let centre = pixel(buffer, width, x, y);
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let nx = (x as i32 + dx) as u32;
            let ny = (y as i32 + dy) as u32;
            if pixel(buffer, width, nx, ny) != centre {
                return false;
            }
        }
    }
    true
}

fn pixel(buffer: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y as usize) * (width as usize) + (x as usize)) * 4;
    [buffer[i], buffer[i + 1], buffer[i + 2], buffer[i + 3]]
}

/// The difference, as a picture: grey scaled 8x so a small delta is visible,
/// and **red wherever the differing pixel is flat in both images** — the pixels
/// no antialiasing difference can explain, which is what a person looking at
/// this image is looking for.
fn delta_image(skia: &[u8], gpu: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0u8; skia.len()];
    for y in 0..height {
        for x in 0..width {
            let a = pixel(skia, width, x, y);
            let b = pixel(gpu, width, x, y);
            let delta = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            let i = ((y as usize) * (width as usize) + (x as usize)) * 4;
            let lit = u8::try_from(u32::from(delta) * 8).unwrap_or(u8::MAX);
            let flat_here =
                delta > 0 && flat(skia, width, height, x, y) && flat(gpu, width, height, x, y);
            out[i] = if flat_here { 255 } else { lit };
            out[i + 1] = if flat_here { 0 } else { lit };
            out[i + 2] = if flat_here { 0 } else { lit };
            out[i + 3] = 255;
        }
    }
    out
}

fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    let file = match std::fs::File::create(path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("painter-diff: writing {}: {error}", path.display());
            return;
        }
    };
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    match encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
    {
        Ok(()) => {}
        Err(error) => eprintln!("painter-diff: encoding {}: {error}", path.display()),
    }
}

/// The control scene: large solid rectangles, and no text, image, blur or
/// stroke.
///
/// Built here rather than taken from `corpus/showcase/` deliberately — see the
/// module documentation. Every swatch is far larger than any antialiasing
/// footprint, so its interior is flat by construction and a difference there
/// cannot be an edge.
///
/// Four bands, three of them opaque and one not:
///
/// 1. A **grey ramp**, because a linear-vs-encoded blending error is largest in
///    the mid-tones and would show as a curve across it.
/// 2. A **saturated sweep**, because a gamut or colour-matching difference
///    moves saturated colours most and neutrals least.
/// 3. The same sweep at **half alpha** over an opaque white backdrop. This is
///    the one band where the blend arithmetic runs rather than just the fill,
///    and it earns its place: it is the only band of the four that has ever
///    disagreed, by one code point, where the three opaque bands are
///    byte-identical.
/// 4. The sweep **dimmed to a mid-tone**, opaque, because an encoding error is
///    largest here and smallest at the extremes the other bands sit at.
fn control(arena: &mut Arena, width: u32, height: u32) {
    /// The saturated sweep, and the grey ramp's endpoints. Ordinary sRGB
    /// values a producer would author, not corner cases: the question is
    /// whether the everyday path agrees.
    const SWEEP: &[(f32, f32, f32)] = &[
        (1.0, 0.0, 0.0),
        (1.0, 0.5, 0.0),
        (1.0, 1.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 1.0, 1.0),
        (0.0, 0.0, 1.0),
        (0.5, 0.0, 1.0),
        (1.0, 0.0, 1.0),
    ];
    const COLUMNS: u32 = 8;

    let w = width as f32;
    let h = height as f32;
    let cell_w = w / COLUMNS as f32;
    let band_h = h / 4.0;

    let mut txn = arena.open();
    let root = txn.add_node(None, Some("control"));
    txn.set_prop(root, Prop::Width(w));
    txn.set_prop(root, Prop::Height(h));
    // Opaque white, so the half-alpha band below has a backdrop to blend over
    // and the blend is not the trivial one over black.
    txn.set_prop(
        root,
        Prop::Fill(Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }),
    );

    let swatch = |txn: &mut dashscene_core::Txn<'_>, name, x: f32, y: f32, colour: Color| {
        let node = txn.add_node(Some(root), Some(name));
        txn.set_prop(node, Prop::X(x));
        txn.set_prop(node, Prop::Y(y));
        txn.set_prop(node, Prop::Width(cell_w));
        txn.set_prop(node, Prop::Height(band_h));
        txn.set_prop(node, Prop::Fill(colour));
    };

    for i in 0..COLUMNS {
        let x = i as f32 * cell_w;
        // Band 0: the grey ramp, black through white in eight even steps.
        let level = i as f32 / (COLUMNS - 1) as f32;
        swatch(
            &mut txn,
            "grey",
            x,
            0.0,
            Color {
                r: level,
                g: level,
                b: level,
                a: 1.0,
            },
        );

        let (r, g, b) = SWEEP[i as usize];
        // Band 1: the saturated sweep, opaque.
        swatch(&mut txn, "sweep", x, band_h, Color { r, g, b, a: 1.0 });
        // Band 2: the same sweep at half alpha over the white root.
        swatch(
            &mut txn,
            "blend",
            x,
            band_h * 2.0,
            Color { r, g, b, a: 0.5 },
        );
        // Band 3: the sweep dimmed to a mid-tone, opaque. A linear-vs-encoded
        // error is largest here and smallest at the extremes the other bands
        // sit at.
        swatch(
            &mut txn,
            "mid",
            x,
            band_h * 3.0,
            Color {
                r: r * 0.5,
                g: g * 0.5,
                b: b * 0.5,
                a: 1.0,
            },
        );
    }
    txn.commit();
}

fn parse<T: std::str::FromStr>(arg: Option<&String>, fallback: T) -> T {
    arg.and_then(|value| value.parse().ok()).unwrap_or(fallback)
}
