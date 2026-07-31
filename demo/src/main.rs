//! `cargo run -p demo` — the showcase host (v0.14, epic #568).
//!
//! This is the first thing in the repository that draws into a window, and
//! since story #572 the first thing that animates one.
//!
//! Scene selection and `.dsb` loading live here. Today there is one scene and
//! it is a placeholder: story #574 authors the showcase scenes under
//! `corpus/showcase/`, and story #575 points this host at a compiled document
//! instead. What story #571 delivered is the [`present`] seam and the Skia
//! implementation behind it; what story #572 adds is the [`shell`] frame loop
//! that drives it.

mod present;
mod shell;

use std::error::Error;
use std::process::ExitCode;

// `Arena` comes from `dashscene-core` rather than through `dashlang`'s
// re-export of it, so the host names one type from one path: `present.rs`
// needs `CommittedScene`, which `dashlang` does not re-export. `dashlang`
// supplies the authoring vocabulary and nothing else.
use dashlang::{Channel, Color, LiveScene, Scene, Spring, node, rgba};
use dashscene_core::Arena;
use dashscene_engine::TaffySolver;

fn main() -> ExitCode {
    match shell::run("dashscene — demo", placeholder_scene, placeholder_pulse) {
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

/// The scene's one signal. Named rather than held in a handle, so
/// [`placeholder_pulse`] can find it in a scene it did not build — which is
/// what lets the host rebuild the scene for a new extent without carrying the
/// signal handles across the rebuild.
const SWEEP: &str = "sweep";

/// A placeholder scene, sized to the drawable in physical pixels, with one
/// signal driving three kinds of animated write.
///
/// Solid fills and absolute placement only — that is the whole of what
/// `dashlang`'s builder exposes without text or images, and it is enough for
/// this story's question, which is whether a frame loop drives motion and then
/// stops. The vocabulary checklist the slice signs off (gradients, strokes,
/// radii, images, MSDF text in Latin and Arabic, clips, group opacity, masks,
/// shadows, backdrop blur, flex, variants, FLIP) belongs to story #574.
///
/// The three writes are deliberately different classes, because they take
/// different paths through `LiveScene::tick`:
///
/// - the rule's **width**, on a childless node, which patches one cached rect
///   and never calls the solver;
/// - each tile's **y**, on a node that has a child, which forces a re-solve;
/// - the veil's **fill alpha**, which is paint-only.
///
/// Every one of them is smoothed through a spring, so the motion is produced
/// by `dashcue`'s scheduler inside `tick` rather than by the host stepping a
/// value itself (P3).
fn placeholder_scene(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
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
    let rule_width = width - 2.0 * margin;

    let mut scene = Scene::new();
    let sweep = scene.signal_named(SWEEP, 0.0);

    let mut root = node("root")
        .size(width, height)
        .child(node("backdrop").size(width, height).fill(ink))
        .child(node("band").size(width, band_height).fill(band))
        .child(
            node("rule")
                .at(margin, band_height - band_height / 6.0)
                .size(rule_width, band_height / 12.0)
                .fill(rule)
                // A childless node, so the write patches one cached rect.
                .bind(
                    Channel::Width,
                    sweep.map_range(0.0, 1.0, rule_width / 8.0, rule_width),
                )
                .smooth(Channel::Width, Spring::critically_damped(0.55)),
        );

    for (index, colour) in tiles.into_iter().enumerate() {
        let left = margin + (tile_width + tile_gap) * index as f32;
        // Each tile rises by a different amount and on its own spring, so the
        // group reads as five animations rather than one block moving.
        let lift = tile_height / 3.0 * (0.4 + 0.15 * index as f32);
        let response = 0.35 + 0.05 * index as f32;
        root = root.child(
            node("tile")
                .at(left, tile_top)
                .size(tile_width, tile_height)
                .fill(colour)
                .bind(
                    Channel::Y,
                    sweep.map_range(0.0, 1.0, tile_top, tile_top - lift),
                )
                .smooth(Channel::Y, Spring::critically_damped(response))
                // Nested, so that a child's placement against its parent's
                // origin rather than the canvas is visible in the picture. It
                // is also what makes the tile's y write force a re-solve
                // rather than patch a single rect.
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
            .fill(veil)
            // Paint-only: an alpha write never reaches the solver.
            .bind(Channel::FillA, sweep.map_range(0.0, 1.0, 0.06, 0.34))
            .smooth(Channel::FillA, Spring::critically_damped(0.7)),
    );

    scene.roots([root]);
    scene.build_live(arena, Box::new(TaffySolver::new()))
}

/// The scripted signal change for pulse `index`: drive `sweep` to one end of
/// its range, then the other.
///
/// The host applies this on the event loop's own thread, either at startup or
/// on a [`shell::Wake`] from the pulse driver, and never inside `tick` (P3).
fn placeholder_pulse(live: &mut LiveScene, index: u64) {
    let Some(sweep) = live.signal_named(SWEEP) else {
        return;
    };
    live.set(sweep, if index.is_multiple_of(2) { 1.0 } else { 0.0 });
}
