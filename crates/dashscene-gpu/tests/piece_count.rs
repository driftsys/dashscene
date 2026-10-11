//! How many pieces the occlusion pass cuts each showcase scene into, and the
//! area those pieces cover, pinned with no device.
//!
//! Story #1450. The commit cuts every rect into the
//! pieces no later opaque core covers (`dashscene_core::occlusion`). Each piece
//! is one more instance to pack, and on the Unity path one more draw command,
//! so the pass is worth what it saves in shaded area only if the piece count
//! stays close to the rect count. This file reads both numbers per scene before
//! any painter packs pieces, so that the trade is measured rather than assumed.
//!
//! # What is measured
//!
//! Each scene is built and solved exactly as `tests/shaded_area.rs` builds it:
//! `(scene.build)` at 2340x1080, `(scene.pulse)` at index 0, then one `tick`.
//! The committed scene's rects and pieces are then read directly. No painter
//! runs.
//!
//! **Only rects that draw something are counted**: a rect whose paint entry
//! has no fill, no extra fill, no stroke, no shadow, no blur and no shape
//! (an unfilled frame, which the packer emits no instance for) is left out,
//! with its pieces. The pass does cut such a rect, and on `surfaces` the
//! unfilled root under the opaque full-screen background loses almost the
//! whole extent, which would be counted as a saving that shades nothing.
//!
//! Area is each rect's box, or each piece, clipped to the 2340x1080 extent —
//! the clip `tests/shaded_area.rs`'s `clipped_area` applies — in square
//! pixels. Unlike that file it adds no `outset` and no antialiasing margin:
//! a rect has no `outset` until a painter packs it, and the margin is the
//! same for a rect and for the pieces that share its box edges. Rotation is
//! not applied, as in that file. The difference between the two sums is the
//! area the pass removes, which is the figure to compare with the derivation's
//! reduction per scene (1.364 Mpx on `surfaces`, 0.450 on `typography`, 2.841
//! on `layout`), an upper bound because the derivation applied no radius and
//! no clip.

use dashscene_core::{Arena, PaintEntry, PaintTable, occlusion};

/// The extent every scene is built and solved at, as in `tests/shaded_area.rs`.
const WIDTH: u32 = 2340;
const HEIGHT: u32 = 1080;

/// One scene's rects and pieces.
struct Expected {
    scene: &'static str,
    /// Rects in the committed table that draw something: an exact count.
    rects: usize,
    /// Pieces the commit cut those rects into: an exact count. A rect a later
    /// core covers completely has no piece, so this can be below `rects`.
    pieces: usize,
    /// Summed rect area, clipped to the extent, in megapixels.
    rect_mpx: f64,
    /// Summed piece area, clipped to the extent, in megapixels.
    piece_mpx: f64,
}

/// Measured on 2026-10-10 at 2340x1080 with the default 1.0 band and
/// `MAX_PIECES_PER_RECT` = 32, and re-derived on every run. The relative
/// tolerance on the areas is `tests/shaded_area.rs`'s, for the reason that
/// file gives: the sums are f32 geometry accumulated in f64.
///
/// **This table is what the cap was set from.** Each scene's opaque
/// full-screen background lies under many later cores. At the first cap of 8
/// it kept its whole quad, and the pass removed 0.0033, 0.0004 and 0.0424 Mpx
/// with 29, 9 and 34 pieces. At a cap of 16, `typography` and `layout` reached
/// the figures below and `surfaces` did not. At 32 and above, and with no cap,
/// every scene measures the figures below, so no rect here reaches the cap.
const EXPECTED: &[Expected] = &[
    Expected {
        scene: "surfaces",
        rects: 27,
        pieces: 59,
        rect_mpx: 4.3758,
        piece_mpx: 3.6330,
    },
    Expected {
        scene: "typography",
        rects: 6,
        pieces: 18,
        rect_mpx: 2.9940,
        piece_mpx: 2.7163,
    },
    Expected {
        scene: "layout",
        rects: 28,
        pieces: 90,
        rect_mpx: 5.4102,
        piece_mpx: 2.9466,
    },
];

const TOLERANCE: f64 = 0.0001;

/// Whether a rect painted from `entry` draws anything of its own.
fn draws(entry: &PaintEntry, paints: &PaintTable) -> bool {
    occlusion::fill_alpha(paints, entry.fill) != occlusion::FillAlpha::None
        || entry.extra_fills.count != 0
        || entry.shape.count != 0
        || paints.stroke(entry).is_some()
        || !paints.shadows(entry).is_empty()
        || !paints.blurs(entry).is_empty()
}

/// A box given by its four edges, clipped to the extent, in square pixels.
///
/// Edges rather than an origin and a size because a piece carries edges: a
/// rect's right edge is `x + w`, the value the pass copies onto every piece
/// that reaches it.
fn clipped_area(left: f32, top: f32, right: f32, bottom: f32) -> f64 {
    let x0 = left.max(0.0);
    let y0 = top.max(0.0);
    let x1 = right.min(WIDTH as f32);
    let y1 = bottom.min(HEIGHT as f32);
    f64::from((x1 - x0).max(0.0)) * f64::from((y1 - y0).max(0.0))
}

/// Builds and solves one scene and returns its rect count, piece count, rect
/// megapixels and piece megapixels.
fn measure(scene: &showcase::Showcase) -> (usize, usize, f64, f64) {
    let mut arena = Arena::new();
    let mut live = (scene.build)(&mut arena, WIDTH, HEIGHT);
    (scene.pulse)(&mut live, 0);
    let _generation = live.tick(0.0, &mut arena);

    let committed = arena.committed();
    let rects = committed.rects();
    let pieces = committed.pieces();

    // The pieces the commit holds are the pass run over this commit's own
    // tables: a commit that reused stale pieces would differ here.
    let cores = occlusion::cores(
        rects,
        committed.paints(),
        committed.clips(),
        committed.groups(),
        arena.aa_band(),
    );
    let barriers = occlusion::barriers(rects, committed.paints());
    let paints = committed.paints();
    let fresh = occlusion::pieces(rects, &cores, &barriers, |index| {
        occlusion::occludable(paints.resolve(rects[index].paint), paints)
    });
    assert_eq!(
        pieces, fresh,
        "{}: the committed pieces are not this commit's",
        scene.name
    );

    let drawn: Vec<bool> = rects
        .iter()
        .map(|rect| draws(paints.resolve(rect.paint), paints))
        .collect();
    let rect_px: f64 = rects
        .iter()
        .zip(&drawn)
        .filter(|(_, drawn)| **drawn)
        .map(|(rect, _)| clipped_area(rect.x, rect.y, rect.x + rect.w, rect.y + rect.h))
        .sum();
    let drawn_pieces: Vec<_> = pieces
        .iter()
        .filter(|piece| drawn[piece.rect as usize])
        .collect();
    let piece_px: f64 = drawn_pieces
        .iter()
        .map(|piece| clipped_area(piece.left, piece.top, piece.right, piece.bottom))
        .sum();
    (
        drawn.iter().filter(|drawn| **drawn).count(),
        drawn_pieces.len(),
        rect_px / 1_000_000.0,
        piece_px / 1_000_000.0,
    )
}

#[test]
fn every_showcase_scene_cuts_into_the_pieces_it_did() {
    let mut failures = Vec::new();

    for expected in EXPECTED {
        let scene = showcase::SCENES
            .iter()
            .find(|candidate| candidate.name == expected.scene)
            .unwrap_or_else(|| panic!("no showcase scene named {}", expected.scene));

        let (rects, pieces, rect_mpx, piece_mpx) = measure(scene);
        eprintln!(
            "{}: {rects} rects, {pieces} pieces, {rect_mpx:.4} Mpx of rects, \
             {piece_mpx:.4} Mpx of pieces, {:.4} Mpx removed",
            expected.scene,
            rect_mpx - piece_mpx
        );

        if rects != expected.rects {
            failures.push(format!(
                "{}: {rects} rects, expected {}",
                expected.scene, expected.rects
            ));
        }
        if pieces != expected.pieces {
            failures.push(format!(
                "{}: {pieces} pieces, expected {}",
                expected.scene, expected.pieces
            ));
        }
        for (what, measured, pinned) in [
            ("rect", rect_mpx, expected.rect_mpx),
            ("piece", piece_mpx, expected.piece_mpx),
        ] {
            let band = pinned * TOLERANCE;
            if (measured - pinned).abs() > band {
                failures.push(format!(
                    "{}: {measured:.4} Mpx of {what}s, expected {pinned:.4} +/- {band:.4}",
                    expected.scene
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "the showcase scenes no longer cut into the pieces they did:\n  {}\n\n\
         Each piece is one more instance to pack, and the area removed is \
         what the occlusion pass saves at {WIDTH}x{HEIGHT} (story #1450). If \
         the change is intended, re-derive the table in this file and say in \
         the pull request what moved and why.",
        failures.join("\n  ")
    );
}

#[test]
fn the_table_covers_every_scene_the_registry_carries() {
    // The set of names, not the count, for the reason
    // `tests/shaded_area.rs`'s test of the same name gives.
    let mut pinned: Vec<&str> = EXPECTED.iter().map(|row| row.scene).collect();
    pinned.sort_unstable();
    let before = pinned.len();
    pinned.dedup();
    assert_eq!(before, pinned.len(), "a scene is pinned twice: {pinned:?}");

    let mut registered: Vec<&str> = showcase::SCENES.iter().map(|s| s.name).collect();
    registered.sort_unstable();

    assert_eq!(
        pinned, registered,
        "every registry scene has a measured row and every row names a \
         registry scene; add the new scene's measured row to EXPECTED"
    );
}
