//! `cargo run -p demo` — the showcase host (v0.14, epic #568).
//!
//! This is the first thing in the repository that draws into a window.
//! Everything before it rastered offscreen and compared the result against a
//! PNG.
//!
//! Scene selection and `.dsb` loading live here. Today there is one scene and
//! it is a placeholder: story #574 authors the showcase scenes under
//! `corpus/showcase/`, and story #575 points this host at a compiled document
//! instead. What story #571 delivers is the [`present`] seam and the Skia
//! implementation behind it, plus the smallest host that proves pixels reach a
//! screen.

mod present;
mod shell;

use std::error::Error;
use std::process::ExitCode;

// `Arena` comes from `dashscene-core` rather than through `dashlang`'s
// re-export of it, so the host names one type from one path: `present.rs`
// needs `CommittedScene`, which `dashlang` does not re-export. `dashlang`
// supplies the authoring vocabulary and nothing else.
use dashlang::{Color, anon, node, rgba, scene};
use dashscene_core::Arena;

fn main() -> ExitCode {
    match shell::run("dashscene — demo", placeholder_scene) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(error.as_ref());
            ExitCode::FAILURE
        }
    }
}

/// Prints the failure and every cause behind it, so a windowing-system error
/// arrives with the context that produced it rather than as one bare line.
fn report(error: &dyn Error) {
    eprintln!("demo: {error}");
    let mut cause = error.source();
    while let Some(next) = cause {
        eprintln!("demo:   caused by: {next}");
        cause = next.source();
    }
}

/// A placeholder scene, sized to the drawable in physical pixels.
///
/// Solid fills and absolute placement only — that is the whole of what
/// `dashlang`'s builder exposes without a layout solver, and it is enough for
/// this story's question, which is whether pixels reach a window at all. The
/// vocabulary checklist the slice signs off (gradients, strokes, radii,
/// images, MSDF text in Latin and Arabic, clips, group opacity, masks,
/// shadows, backdrop blur, flex, variants, FLIP, springs) belongs to story
/// #574, not here.
fn placeholder_scene(arena: &mut Arena, width: u32, height: u32) {
    let width = width as f32;
    let height = height as f32;

    let ink = rgba(0.05, 0.07, 0.12, 1.0);
    let band = rgba(0.13, 0.17, 0.27, 1.0);
    let rule = rgba(0.36, 0.71, 0.94, 1.0);
    let veil = rgba(1.0, 1.0, 1.0, 0.18);
    let tiles: [Color; 5] = [
        rgba(0.91, 0.30, 0.24, 1.0),
        rgba(0.95, 0.61, 0.07, 1.0),
        rgba(0.18, 0.72, 0.42, 1.0),
        rgba(0.20, 0.51, 0.89, 1.0),
        rgba(0.61, 0.35, 0.85, 1.0),
    ];

    let margin = width / 16.0;
    let band_height = height / 6.0;
    let tile_gap = margin / 2.0;
    let tile_count = tiles.len() as f32;
    let tile_width = (width - 2.0 * margin - tile_gap * (tile_count - 1.0)) / tile_count;
    let tile_height = height / 3.0;
    let tile_top = band_height + margin;

    let mut root = anon()
        .size(width, height)
        .child(node("backdrop").size(width, height).fill(ink))
        .child(node("band").size(width, band_height).fill(band))
        .child(
            node("rule")
                .at(margin, band_height - band_height / 6.0)
                .size(width - 2.0 * margin, band_height / 12.0)
                .fill(rule),
        );

    for (index, colour) in tiles.into_iter().enumerate() {
        let left = margin + (tile_width + tile_gap) * index as f32;
        root = root.child(
            node("tile")
                .at(left, tile_top)
                .size(tile_width, tile_height)
                .fill(colour)
                // Nested, so that a child's placement against its parent's
                // origin rather than the canvas is visible in the picture.
                .child(
                    node("badge")
                        .at(tile_width / 8.0, tile_height / 8.0)
                        .size(tile_width / 4.0, tile_width / 4.0)
                        .fill(ink),
                ),
        );
    }

    // Translucent, and overlapping the tiles: the only part of the picture
    // that exercises the presenter's compositing rather than a plain copy.
    root = root.child(
        node("veil")
            .at(margin, tile_top + tile_height / 2.0)
            .size(width - 2.0 * margin, tile_height)
            .fill(veil),
    );

    scene([root]).build(arena);
}
