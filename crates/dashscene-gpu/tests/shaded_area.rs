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
//! directly on the committed tables. That runs `pack::pack` and nothing else:
//! no `Renderer`, no adapter, no window, no GPU.
//!
//! Per instance the shaded footprint is `bounds` grown by `outset` on every
//! side — `Instance::outset` documents that as exactly what the vertex stage
//! grows the quad by, so a stroke's or a drop shadow's rasterized quad is
//! larger than its `bounds` alone — then clipped to the extent.
//!
//! # What it is not
//!
//! **It is an upper bound on shaded area, not a rasterization.** Node-level
//! clip regions are not applied and rotation is not applied, so an instance
//! that a clip would cut down is counted whole. That is deliberate: the
//! quantity being pinned is what the painter *submits*, and a change to what
//! it submits is the regression this catches. It is a monitor over one number
//! per scene, not a gate over a picture — `goldens/` and `conformance/` own
//! correctness.
//!
//! **The per-kind weight is not here.** Issue #1296's second half asks for a
//! cost per instance kind, which needs a device to calibrate. This half needs
//! none.

use dashpaint::Painter;
use dashscene_core::Arena;
use dashscene_gpu::GpuPainter;

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
    /// Instances the packer emits, which is an exact count and not a tolerance:
    /// it is an integer the arithmetic below cannot round.
    instances: usize,
    /// Shaded megapixels, summed over every instance.
    shaded_mpx: f64,
}

/// Measured on 2026-09-04 at 2340x1080 and re-derived by this test on every
/// run.
///
/// **The tolerance is relative and it is 0.1 %, and that number is measured
/// rather than chosen.** It is not zero, because the sum is f32 geometry
/// accumulated in f64 and the solver reaches libm, so the last places are not
/// portable across architectures. It was 1 % in a first draft and 1 % had no
/// teeth: zeroing the stroke outset in `pack.rs` — a real packer defect, and
/// the term `Instance::outset` exists for — moves `surfaces` by **0.18 %** and
/// the other two scenes not at all, so a 1 % band passed a mutated painter.
/// 0.1 % fails it, and still leaves about four orders of magnitude over f32
/// last-place drift, which moves a 6 Mpx sum by parts in ten million.
///
/// The instance counts below are the strict half. A shaping or layout
/// difference large enough to matter changes how many quads are emitted, and
/// that is compared for equality rather than within a band.
const EXPECTED: &[Expected] = &[
    Expected {
        scene: "surfaces",
        instances: 65,
        shaded_mpx: 6.0601,
    },
    Expected {
        scene: "typography",
        instances: 381,
        shaded_mpx: 3.1669,
    },
    Expected {
        scene: "layout",
        instances: 29,
        shaded_mpx: 5.4260,
    },
];

/// The relative band each scene's shaded area is held to.
const TOLERANCE: f64 = 0.001;

/// `bounds` grown by `outset` on every side, then clipped to the extent, in
/// square pixels.
fn clipped_area(bounds: [f32; 4], outset: f32) -> f64 {
    let [x, y, w, h] = bounds;
    let (x, y, w, h) = (x - outset, y - outset, w + 2.0 * outset, h + 2.0 * outset);
    let x0 = x.max(0.0);
    let y0 = y.max(0.0);
    let x1 = (x + w).min(WIDTH as f32);
    let y1 = (y + h).min(HEIGHT as f32);
    let clipped_w = f64::from((x1 - x0).max(0.0));
    let clipped_h = f64::from((y1 - y0).max(0.0));
    clipped_w * clipped_h
}

/// Builds, solves and packs one scene, and returns its instance count and
/// shaded megapixels.
fn measure(scene: &showcase::Showcase) -> (usize, f64) {
    let mut arena = Arena::new();
    let mut live = (scene.build)(&mut arena, WIDTH, HEIGHT);
    (scene.pulse)(&mut live, 0);
    let _generation = live.tick(0.0, &mut arena);

    let committed = arena.committed();
    let mut painter = GpuPainter::new();
    painter.paint(
        committed.rects(),
        committed.paints(),
        committed.images(),
        committed.clips(),
        committed.groups(),
        committed.glyphs(),
        None,
    );

    let instances = painter.instances().instances();
    let shaded_px: f64 = instances
        .iter()
        .map(|instance| clipped_area(instance.bounds, instance.outset))
        .sum();
    (instances.len(), shaded_px / 1_000_000.0)
}

#[test]
fn every_showcase_scene_shades_what_it_did() {
    let mut failures = Vec::new();

    for expected in EXPECTED {
        let scene = showcase::SCENES
            .iter()
            .find(|candidate| candidate.name == expected.scene)
            .unwrap_or_else(|| panic!("no showcase scene named {}", expected.scene));

        let (instances, shaded_mpx) = measure(scene);

        if instances != expected.instances {
            failures.push(format!(
                "{}: {instances} instances, expected {}",
                expected.scene, expected.instances
            ));
        }

        let band = expected.shaded_mpx * TOLERANCE;
        let drift = (shaded_mpx - expected.shaded_mpx).abs();
        if drift > band {
            failures.push(format!(
                "{}: {shaded_mpx:.4} Mpx shaded, expected {:.4} +/- {band:.4} \
                 ({:+.2} %)",
                expected.scene,
                expected.shaded_mpx,
                100.0 * (shaded_mpx - expected.shaded_mpx) / expected.shaded_mpx,
            ));
        }
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

#[test]
fn the_table_covers_every_scene_the_registry_carries() {
    // A scene added to the registry and not to the table would be measured by
    // nothing, and the test above would still pass — it iterates the table.
    assert_eq!(
        showcase::SCENES.len(),
        EXPECTED.len(),
        "the registry carries {} scenes and this file pins {}; \
         add the new scene's measured row to EXPECTED",
        showcase::SCENES.len(),
        EXPECTED.len()
    );
}
