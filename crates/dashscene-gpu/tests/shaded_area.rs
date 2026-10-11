//! What each showcase scene shades in one frame, pinned with no device.
//!
//! Issue #1296. A GPU cost regression on the target hardware is a fill-rate
//! regression first, and fill rate is shaded pixels per frame — which this
//! painter's own `InstanceBuffer` states exactly, on the CPU, before anything
//! is uploaded. So the reading needs no device and no adapter, and a change
//! that doubles what a scene shades turns this red in the sanity tier rather
//! than being found on a phone.
//!
//! # What is measured
//!
//! Each scene is built and solved the way a host builds it — `(scene.build)`,
//! then `(scene.pulse)` at index 0, then one `tick`, which is
//! `demo/src/shell.rs`'s `Showcase::build` followed by
//! `dashscene_desktop`'s first frame. `GpuPainter::paint` is then called
//! directly on the committed tables, with the commit's occlusion pieces as a
//! host passes them. That runs `pack::pack` and nothing else: no `Renderer`,
//! no adapter, no window, no GPU.
//!
//! Per instance the shaded footprint is the quad the vertex stage builds, then
//! clipped to the extent. For a whole instance that is `bounds` grown by
//! `outset` and the antialiasing margin on every side — `Instance::outset`
//! documents that as exactly what the vertex stage grows the quad by, so a
//! stroke's or a drop shadow's rasterized quad is larger than its `bounds`
//! alone. For an instance cut to an occlusion piece (story #1450) it is the
//! piece's extent, grown by the same amount only on the edges its grow bits
//! name, which is `vs_main` in `src/shaders/paint.wgsl`.
//!
//! # Before and after the occlusion pieces
//!
//! Each scene is packed twice: once with the commit's pieces, which is what a
//! host submits, and once with an empty piece slice, which draws every rect
//! whole and is what this painter submitted before story #1450. Both sums are
//! pinned, so the reduction the pieces make is a reading too, not only the
//! figure left after it.
//!
//! # What this number is, and what it leaves out
//!
//! **It is the paint pipeline's instances and nothing else.** Three things a
//! frame really shades are not in the instance buffer at all, so no change to
//! any of them moves this reading:
//!
//! - **Group composites.** `composite.wgsl`'s `vs_composite` builds a
//!   full-target quad from the vertex index alone, so every group costs the
//!   whole extent — 2.5272 Mpx at 2340x1080, which is 41 % of what `surfaces`
//!   shades here with no pieces and 47 % of what it shades with them. `pack`
//!   emits no instance for a group. The group counts are pinned below beside
//!   the areas for exactly this reason: they are the multiplier this sum does
//!   not carry.
//! - **The backdrop passes.** A backdrop pays a full-target snapshot copy and
//!   a base blit, plus the blur's axis and resolve quads.
//! - **Anything a `shape` mask substitutes.** For a masked instance the vertex
//!   stage uses `bounds.xy + field.plane` rather than `bounds`, and this reads
//!   `bounds`.
//!
//! **Within the instances it does cover it is an upper bound, not a
//! rasterization.** Node-level clip regions and rotation are not applied, so
//! an instance a clip would cut down is counted whole.
//!
//! **One blind spot worth naming, because the fixture causes it.** The sum
//! cannot separate `StrokeAlign::Center` from `StrokeAlign::Outside`: the only
//! two non-inside strokes in the whole showcase are `tile-linear` and
//! `tile-radial`, the same box at the same width, so swapping the two arms of
//! `stroke_outset` is symmetric and moves this total by zero. Issue #1425
//! carries it.
//!
//! So this is a monitor over one number per scene, not a gate over a picture —
//! `goldens/` and `conformance/` own correctness.
//!
//! **The per-kind weight is not here.** Issue #1296's second half asks for a
//! cost per instance kind, which needs a device to calibrate. This half needs
//! none.
//!
//! # Why a test that a scene edit invalidates lives here at all
//!
//! `corpus/showcase/src/lib.rs` rules that "a test that would need updating
//! because a scene was re-authored is a coverage test wearing another name,
//! and belongs in the checklist instead". This file does need updating when a
//! scene is re-authored, and it is not that thing: it asserts no construct is
//! drawn, and a green run here stands in for no coverage claim. What it pins
//! is a **cost**, which has no right answer independent of the scenes — the
//! scenes are the workload the target device is budgeted against
//! (`docs/decisions/the-gpu-frame-on-the-target-device-is-budgeted.md`). It
//! also lives in this crate rather than in `corpus/showcase`, so that rule's
//! subject — the tests in that crate — is unchanged.

use dashpaint::{Painter, Piece};
use dashscene_core::Arena;
use dashscene_gpu::{GpuPainter, Instance};

/// The extent every scene is built and solved at.
///
/// 2340x1080 is the Pixel 5's full landscape extent, which is what both
/// Android showcase hosts draw at
/// (`docs/decisions/the-showcase-hosts-share-one-surface.md`), so this reading
/// is over the same frame the device measurements are taken over.
const WIDTH: u32 = 2340;
const HEIGHT: u32 = 1080;

/// What one scene is expected to shade, and with how many instances.
struct Expected {
    scene: &'static str,
    /// Instances the packer emits with the commit's occlusion pieces, which is
    /// an exact count and not a tolerance: it is an integer the arithmetic
    /// below cannot round.
    instances: usize,
    /// Shaded megapixels with the commit's occlusion pieces, summed over every
    /// instance. The after-column.
    shaded_mpx: f64,
    /// Instances the packer emits with no pieces: every rect whole, as before
    /// story #1450.
    whole_instances: usize,
    /// Shaded megapixels with no pieces. The before-column, and the figure
    /// this table pinned before story #1450, unchanged.
    whole_mpx: f64,
    /// The reduction `probe-1412/rejected-2340x1080.txt` derived for this
    /// scene, in megapixels: an upper bound on `whole_mpx - shaded_mpx`. See
    /// [`the_occlusion_pieces_remove_no_more_than_the_derivation_bounds`].
    derivation_mpx: f64,
    /// Groups the document carries.
    ///
    /// **Pinned because it is the term the megapixel sum does not carry.**
    /// Each group costs a full-target composite quad — 2.5272 Mpx at this
    /// extent — and `pack` emits no instance for one, so a scene that gains a
    /// group shades 2.5 Mpx more per frame and moves `shaded_mpx` by nothing
    /// at all. A count rather than an area, because a group's cost is the
    /// extent and the extent is a constant here.
    groups: usize,
}

/// Measured on 2026-09-04 at 2340x1080 and re-derived by this test on every
/// run.
///
/// **The tolerance is relative and it is 0.01 %, and it has been tightened
/// twice, each time by a mutation the previous band let through.**
///
/// 1 % was the first draft. Zeroing the stroke outset in `pack.rs` — a real
/// packer defect, and the term `Instance::outset` exists to prevent — moves
/// `surfaces` by 0.18 % and the other two scenes not at all, so 1 % passed a
/// mutated painter. 0.1 % was the second. Treating one `StrokeAlign::Outside`
/// as centred is **half** of that mutation, about 0.09 %, and passed; so did
/// trimming `shadow_ink_reach`'s blur support from `3.0 * sigma` to `2.7 *`.
/// 0.01 % fails both.
///
/// It is not zero because the sum is f32 geometry accumulated in f64. It is
/// nearly portable all the same: nothing on the path from a scene to `bounds`
/// and `outset` reaches libm — the fonts and the MSDF sheets are vendored and
/// loaded rather than generated, shaping is IEEE `+ - * /` which Rust does not
/// contract to FMA, and `tick(0.0)` at pulse 0 writes each signal's initial
/// value so no spring integrates. If a Linux runner ever disagrees at 0.01 %,
/// that is a finding about the solver rather than a reason to widen the band.
///
/// The instance counts below are the strict half. A shaping or layout
/// difference large enough to matter changes how many quads are emitted, and
/// that is compared for equality rather than within a band.
///
/// **The after-column was measured on 2026-10-10**, when the lean painter
/// began packing the commit's occlusion pieces (story #1450), at the arena's
/// default 1.0 antialiasing band and `MAX_PIECES_PER_RECT` = 32. The same
/// 0.01 % band holds it. The before-column is the table as it stood, measured
/// with an empty piece slice, and it did not move.
const EXPECTED: &[Expected] = &[
    Expected {
        scene: "surfaces",
        instances: 97,
        shaded_mpx: 5.3653,
        whole_instances: 65,
        whole_mpx: 6.1083,
        derivation_mpx: 1.364,
        groups: 1,
    },
    Expected {
        scene: "typography",
        instances: 393,
        shaded_mpx: 2.9323,
        whole_instances: 381,
        whole_mpx: 3.2100,
        derivation_mpx: 0.450,
        groups: 0,
    },
    Expected {
        scene: "layout",
        instances: 91,
        shaded_mpx: 2.9980,
        whole_instances: 29,
        whole_mpx: 5.4616,
        derivation_mpx: 2.841,
        groups: 0,
    },
];

/// The relative band each scene's shaded area is held to.
const TOLERANCE: f64 = 0.0001;

/// What the vertex stage grows every quad by, on top of the instance's own
/// `outset`.
///
/// `paint.wgsl`'s `vs_main` computes `let margin = globals.aa +
/// instance_outset(inst);`, and `globals.aa` is fed `AA_WIDTH` unconditionally
/// — text included. So the rasterized quad is one pixel larger on every side
/// than `bounds` grown by `outset`, and a reading that left this out
/// understated `typography` by the most: 375 of its 381 instances are glyph
/// quads a few tens of pixels across, where one pixel of margin per side is a
/// large fraction of the area.
///
/// It is duplicated here because `AA_WIDTH` is private to
/// `crates/dashscene-gpu/src/render.rs`, and
/// [`the_painters_antialiasing_margin_is_the_one_this_file_adds`] is what stops
/// the two from drifting apart.
const AA_MARGIN: f32 = 1.0;

/// The quad the vertex stage builds for `instance`, clipped to the extent, in
/// square pixels.
///
/// A whole instance's quad is `bounds` grown on every side by its `outset` and
/// the antialiasing margin. A cut instance's is its piece's extent row, grown
/// by the same amount only on the edges its grow bits name.
fn footprint(instance: &Instance, extents: &[[f32; 4]]) -> f64 {
    let Some(row) = instance.extent_row() else {
        return clipped_area(instance.bounds, instance.outset);
    };
    let [left, top, right, bottom] = extents[row];
    let reach = instance.outset + AA_MARGIN;
    let grow = instance.grow_mask();
    let grown = |edge: f32, bit: u32, sign: f32| {
        if grow & bit != 0 {
            edge + sign * reach
        } else {
            edge
        }
    };
    let x0 = grown(left, Instance::GROW_LEFT, -1.0).max(0.0);
    let y0 = grown(top, Instance::GROW_TOP, -1.0).max(0.0);
    let x1 = grown(right, Instance::GROW_RIGHT, 1.0).min(WIDTH as f32);
    let y1 = grown(bottom, Instance::GROW_BOTTOM, 1.0).min(HEIGHT as f32);
    f64::from((x1 - x0).max(0.0)) * f64::from((y1 - y0).max(0.0))
}

/// `bounds` grown by `outset` and the antialiasing margin on every side, then
/// clipped to the extent, in square pixels.
fn clipped_area(bounds: [f32; 4], outset: f32) -> f64 {
    let [x, y, w, h] = bounds;
    let outset = outset + AA_MARGIN;
    let (x, y, w, h) = (x - outset, y - outset, w + 2.0 * outset, h + 2.0 * outset);
    let x0 = x.max(0.0);
    let y0 = y.max(0.0);
    let x1 = (x + w).min(WIDTH as f32);
    let y1 = (y + h).min(HEIGHT as f32);
    let clipped_w = f64::from((x1 - x0).max(0.0));
    let clipped_h = f64::from((y1 - y0).max(0.0));
    clipped_w * clipped_h
}

/// One scene, packed with its occlusion pieces and packed whole.
struct Measured {
    /// Instances and shaded megapixels with the commit's pieces.
    instances: usize,
    shaded_mpx: f64,
    /// Instances and shaded megapixels with an empty piece slice.
    whole_instances: usize,
    whole_mpx: f64,
    groups: usize,
}

/// Builds, solves and packs one scene, with its pieces and whole.
fn measure(scene: &showcase::Showcase) -> Measured {
    let mut arena = Arena::new();
    let mut live = (scene.build)(&mut arena, WIDTH, HEIGHT);
    (scene.pulse)(&mut live, 0);
    let _generation = live.tick(0.0, &mut arena);

    let committed = arena.committed();
    let pack = |pieces: &[Piece]| {
        let mut painter = GpuPainter::new();
        painter.paint(
            committed.rects(),
            committed.paints(),
            committed.images(),
            committed.clips(),
            committed.groups(),
            committed.glyphs(),
            pieces,
            None,
        );
        let buffer = painter.instances();
        let shaded_px: f64 = buffer
            .instances()
            .iter()
            .map(|instance| footprint(instance, buffer.extents()))
            .sum();
        (buffer.instances().len(), shaded_px / 1_000_000.0)
    };
    let (instances, shaded_mpx) = pack(committed.pieces());
    let (whole_instances, whole_mpx) = pack(&[]);
    Measured {
        instances,
        shaded_mpx,
        whole_instances,
        whole_mpx,
        groups: committed.groups().len(),
    }
}

/// `measured` against `expected` within [`TOLERANCE`], as a failure line.
fn drift(scene: &str, what: &str, measured: f64, expected: f64) -> Option<String> {
    let band = expected * TOLERANCE;
    ((measured - expected).abs() > band).then(|| {
        format!(
            "{scene}: {measured:.4} Mpx {what}, expected {expected:.4} +/- {band:.4} ({:+.2} %)",
            100.0 * (measured - expected) / expected,
        )
    })
}

#[test]
fn every_showcase_scene_shades_what_it_did() {
    let mut failures = Vec::new();

    for expected in EXPECTED {
        let scene = showcase::SCENES
            .iter()
            .find(|candidate| candidate.name == expected.scene)
            .unwrap_or_else(|| panic!("no showcase scene named {}", expected.scene));

        let Measured {
            instances,
            shaded_mpx,
            whole_instances,
            whole_mpx,
            groups,
        } = measure(scene);

        if groups != expected.groups {
            failures.push(format!(
                "{}: {groups} group(s), expected {}. Each one is a \
                 full-target composite this sum does not count.",
                expected.scene, expected.groups
            ));
        }

        if instances != expected.instances {
            failures.push(format!(
                "{}: {instances} instances, expected {}",
                expected.scene, expected.instances
            ));
        }
        if whole_instances != expected.whole_instances {
            failures.push(format!(
                "{}: {whole_instances} instances with no pieces, expected {}",
                expected.scene, expected.whole_instances
            ));
        }

        failures.extend(drift(
            expected.scene,
            "shaded",
            shaded_mpx,
            expected.shaded_mpx,
        ));
        failures.extend(drift(
            expected.scene,
            "shaded with no pieces",
            whole_mpx,
            expected.whole_mpx,
        ));
    }

    assert!(
        failures.is_empty(),
        "the showcase scenes no longer shade what they did:\n  {}\n\n\
         Each figure is what this painter submits for one frame at \
         {WIDTH}x{HEIGHT}. A change here is a fill-rate change on the target \
         device (issue #1296). If it is intended, re-derive the table in this \
         file and say in the pull request what moved and why.",
        failures.join("\n  ")
    );
}

/// The occlusion pieces remove shaded area on every scene, and no more than
/// the derivation's figure for that scene (story #1450).
///
/// **The derivation is an upper bound, not a target.** It inset each core by
/// one pixel and applied no corner radius and no clip, so it removed more than
/// any pass that keeps the antialiased edge intact can: `cores` insets by the
/// largest corner radius plus the band, and intersects with the core's own
/// clip boxes' inset interiors. Its base area was its own reading too, not
/// this file's. So the measured reduction is pinned exactly above, as the two
/// columns of [`EXPECTED`], and this test bounds it on both sides: above zero,
/// so the pieces are doing something, and at most the derivation, so the
/// instrument and the derivation do not contradict each other.
#[test]
fn the_occlusion_pieces_remove_no_more_than_the_derivation_bounds() {
    let mut failures = Vec::new();
    for expected in EXPECTED {
        let scene = showcase::SCENES
            .iter()
            .find(|candidate| candidate.name == expected.scene)
            .unwrap_or_else(|| panic!("no showcase scene named {}", expected.scene));
        let measured = measure(scene);
        let removed = measured.whole_mpx - measured.shaded_mpx;
        if !(removed > 0.0 && removed <= expected.derivation_mpx) {
            failures.push(format!(
                "{}: the pieces removed {removed:.4} Mpx ({:.4} whole, {:.4} cut), \
                 outside (0, {:.3}]",
                expected.scene, measured.whole_mpx, measured.shaded_mpx, expected.derivation_mpx,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "the occlusion pieces' reduction left its bounds:\n  {}",
        failures.join("\n  ")
    );
}

/// `AA_MARGIN` is `render.rs`'s `AA_WIDTH`, and this is what keeps them equal.
///
/// The constant is private to that module, so this file carries its own copy.
/// A copy that can drift silently is worse than no copy: the whole table above
/// would shift by about 1 % and stay green, because every row would shift
/// together. Read as source rather than linked, which is what a private const
/// leaves available.
#[test]
fn the_painters_antialiasing_margin_is_the_one_this_file_adds() {
    let render = include_str!("../src/render.rs");
    let declared = format!("const AA_WIDTH: f32 = {AA_MARGIN:.1};");
    assert!(
        render.contains(&declared),
        "crates/dashscene-gpu/src/render.rs no longer declares `{declared}`, \
         so AA_MARGIN in this file is measuring a margin the painter does not \
         apply. Re-derive the table above with the new value."
    );
}

#[test]
fn the_table_covers_every_scene_the_registry_carries() {
    // **The set of names, not the count.** `len() == len()` passes a table
    // holding one scene twice and another not at all — the realistic
    // copy-paste edit — and the test above iterates the table, so the missing
    // scene would then be measured by nothing while both tests stayed green.
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
