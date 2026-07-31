//! Renders one showcase scene to a PNG — the still the entry-path
//! documentation shows (story #576 consumes it).
//!
//! This is **not** a golden and never becomes one. It asserts nothing, it is
//! not run by CI, and no committed image is compared against it. Its whole job
//! is to produce a picture a person puts in a document, which is why it is an
//! example rather than a test.
//!
//! ```text
//! cargo run -p showcase --example still -- surfaces docs/images/showcase-surfaces.png
//! ```
//!
//! Optional third and fourth arguments set the extent in physical pixels, a
//! fifth sets how many seconds of scripted animation to run before the frame
//! is taken — so the still catches the scene mid-motion rather than at rest —
//! and a sixth picks which scripted phase to run towards.

use dashpaint::Painter;
use dashscene_core::Arena;
use dashscene_skia::SkiaPainter;

/// The fixed step the still is advanced with. A wall clock is deliberately not
/// read: the same arguments must produce the same picture, on any machine, so
/// the frame the documentation shows can be regenerated.
const STEP: f32 = 1.0 / 60.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = args
        .first()
        .map(String::as_str)
        .unwrap_or(showcase::DEFAULT);
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("{name}.png"));
    // Clamped away from zero: `SkiaPainter::new` refuses a non-positive extent,
    // and a typo in an argument should not arrive as a panic from inside the
    // painter.
    let width: u32 = parse(args.get(2), 1920).max(1);
    let height: u32 = parse(args.get(3), 1200).max(1);
    let seconds: f32 = parse(args.get(4), 1.2);
    let phase: u64 = parse(args.get(5), 1);

    let Some(scene) = showcase::by_name(name) else {
        eprintln!("still: no scene named {name:?}");
        eprintln!("still: scenes are:");
        for scene in showcase::SCENES {
            eprintln!("still:   {:<12} {}", scene.name, scene.summary);
        }
        std::process::exit(2);
    };

    let mut arena = Arena::new();
    let mut live = (scene.build)(&mut arena, width, height);
    // Phase 1 by default rather than 0, so the still is taken while the scene
    // is running towards its far end rather than sitting at its starting
    // values.
    (scene.pulse)(&mut live, phase);
    let steps = (seconds / STEP).round().max(0.0) as u32;
    for _ in 0..steps {
        live.tick(STEP, &mut arena);
    }

    let committed = arena.committed();
    let mut painter = SkiaPainter::new(width as i32, height as i32);
    painter.paint(
        committed.rects(),
        committed.paints(),
        committed.images(),
        committed.clips(),
        committed.groups(),
        committed.glyphs(),
        None,
    );
    std::fs::write(&out, painter.png_bytes()).unwrap_or_else(|error| {
        eprintln!("still: writing {out}: {error}");
        std::process::exit(1);
    });
    eprintln!(
        "still: {name} at {width}x{height}, phase {phase}, after {seconds} s — {} rects, {} paints, {} glyph \
         runs, {} images, {} groups -> {out}",
        committed.rects().len(),
        committed.paints().len(),
        committed.glyphs().runs().len(),
        committed.images().len(),
        committed.groups().len(),
    );
    let painted = committed
        .rects()
        .iter()
        .filter(|entry| entry.w > 0.0 && entry.h > 0.0)
        .count();
    eprintln!("still: {painted} of them have a non-empty box");
}

fn parse<T: std::str::FromStr>(arg: Option<&String>, fallback: T) -> T {
    arg.and_then(|value| value.parse().ok()).unwrap_or(fallback)
}
