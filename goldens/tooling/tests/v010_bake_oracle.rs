//! Story B1 (#340) — the bake oracle (B1.5,
//! docs/wip/2026-07-19-B1-vector-msdf-design.md): the vector-fidelity gate.
//!
//! The generator's own weld test (`crates/dashc/tests/vector_field_weld.rs`)
//! proves `fdsm`'s field equals pinned `msdfgen` output — us-vs-msdfgen. This
//! oracle proves the other half: the baked field, rendered as an MSDF quad
//! through the painter, equals the **actual shape**. Ground truth is the shape
//! filled directly by Skia's exact path rasterizer (not msdfgen), so the diff
//! measures the whole bake-and-reconstruct path — winding, corner fidelity,
//! atlas packing, and the painter's median-of-3 coverage resolve — against the
//! geometry the shape was baked from.
//!
//! Per shape, at a common device size:
//!   - TRUTH: `skia_safe` fills the parsed vector path (winding rule honored).
//!   - BAKE:  `dashc`'s `vector_field` bakes the path to an MSDF atlas tile,
//!     hand-built into a `.dsb` and rendered through the Skia reference
//!     painter's field path (`draw_vector_field` / the `FIELD_MASK_SKSL`
//!     median-of-3 resolve — the same code the real import renders vectors
//!     with, story B1.3).
//!
//! Both render the shape at unit device scale (`device = origin + shape`), so a
//! shape coordinate lands at the same device pixel in both renders regardless of
//! `px_per_em`; only the field's reconstruction fidelity varies with `px_per_em`.
//!
//! This is a self-comparison (path vs field, us-vs-us), so it sets its own bake
//! tolerance — tighter than the Figma-vs-us E7 bands (`goldens::oracle`) and
//! entirely independent of them; it neither reads nor retunes a frozen band.
//!
//! Escalation and refusal are executable **here, in this oracle** — they are
//! not in the production lowering. v0.10 ships a fixed-`DEFAULT_PX_PER_EM` (48)
//! bake that never re-bakes (the census found zero shapes needing more); wiring
//! the ladder and the ceiling refusal into the lowering is deferred (debt #357).
//! In this test a shape over tolerance at the default `px_per_em` is re-baked up
//! the ladder until it passes or the ceiling is reached; a shape still over
//! tolerance at the ceiling is **unfieldable** — the bake-side cause of the
//! lowering's `figma.unsupported` refusal (P4, B1.4; there is no dedicated
//! `figma.vector-unfieldable` code). The census found zero unfieldable nodes in
//! either live target, so the refusal arm is defensive; it is exercised here by
//! a synthetic sub-texel barcode (detail finer than the atlas grid at every
//! rung). A correctly framed field reconstructs the census shapes — and even a
//! plain thin bar — within tolerance at the fixed default 48 px/em, so none of
//! them escalates; the ladder is exercised only by the barcode refusal.

use dashbuf::{
    AtlasRect, Color, Document, DocumentArgs, Fill, FixedSizeLayout, Image, ImageArgs, ImageFormat,
    Node, NodeArgs, Paint, PaintArgs, PlaneBounds, SolidFill, SolidFillArgs, VectorAtlas,
    VectorAtlasArgs, VectorShape, VectorShapeArgs, root_as_document,
};
use dashc_wasm::figma::vector_field::{
    DEFAULT_DISTANCE_RANGE, DEFAULT_PX_PER_EM, VectorAtlasBaker, VectorPath, WindingRule,
};
use dashpaint::{GlyphRunTable, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_skia::SkiaPainter;
use flatbuffers::FlatBufferBuilder;
use skia_safe::{
    AlphaType, Color as SkColor, ColorType, ImageInfo, Paint as SkPaint, Path, PathFillType,
    surfaces,
};

/// The per-pixel coverage threshold (0..=255): a device pixel counts as
/// differing only when the field's coverage and the exact fill's coverage
/// disagree by more than this. At ~19 % of full coverage it filters the shape
/// difference between an anti-aliased path edge (a ~1-px linear ramp) and an
/// MSDF reconstruction (a smoothstep over the screen-pixel range), while still
/// counting a genuine inside/outside flip. The E7 bands are unrelated and
/// untouched; this is the bake oracle's own, tighter self-comparison threshold.
const BAKE_CHANNEL_DELTA: u8 = 48;

/// The bake pass ceiling: the fraction of a shape's own footprint (0.0..=1.0)
/// allowed to exceed [`BAKE_CHANNEL_DELTA`]. Set empirically and principled, not
/// to merely pass:
///   - A pipeline-clean bake (the axis-aligned `square-with-hole`, all
///     right-angle corners) measures 0.000 % here with a max delta under 10 —
///     the anchor that the framing carries no systematic error.
///   - The inherent MSDF residual for curved and slanted edges at a
///     well-resolved `px_per_em` is at most ~2.2 % of the shape footprint
///     (`organic-blob`), concentrated in the 1–2 px edge band.
///   - 3 % sits just above that inherent residual: a wrong-winding bake
///     (≈100 % footprint flip), a lost corner (a large contiguous region), or a
///     mis-packed atlas (a shifted or garbage tile) each vastly exceed it, while
///     the sub-pixel edge disagreement between an exact fill and an MSDF field
///     does not.
const BAKE_TOLERANCE: f64 = 0.03;

/// The `px_per_em` escalation ladder, starting at the generator default. A
/// shape over tolerance at one rung is re-baked at the next; the last entry is
/// the ceiling (the arabic-atlas spike found diminishing returns past ~48 px/em
/// for small content, so the ceiling is finite — `docs/technotes/
/// msdf-arabic-atlas-spike.md`). A shape still over tolerance at the ceiling is
/// unfieldable.
const PX_PER_EM_LADDER: [f64; 5] = [DEFAULT_PX_PER_EM, 64.0, 96.0, 128.0, 192.0];

/// A pixel is inside a render's footprint when either render covers it by more
/// than this (of 255). Below it the pixel is background in both, and comparing
/// two transparent pixels measures nothing — so the footprint, not the whole
/// canvas, is the diff denominator: the disagreement is graded over the shape's
/// own area, undiluted by empty margin.
const FOOTPRINT_EPSILON: u8 = 8;

/// The shared canvas; every shape is authored within a ~150-unit box and drawn
/// at [`ORIGIN`], so the padded field quad (which extends `distance_range /
/// scale` beyond the geometry, largest at the lowest `px_per_em`) stays on
/// canvas.
const CANVAS: i32 = 220;
const ORIGIN: f32 = 24.0;

fn sk_fill_type(winding: WindingRule) -> PathFillType {
    match winding {
        WindingRule::NonZero => PathFillType::Winding,
        WindingRule::EvenOdd => PathFillType::EvenOdd,
    }
}

/// TRUTH: Skia fills the vector path directly at unit scale, offset by
/// [`ORIGIN`]. Returns unpremultiplied RGBA8888 rows (the golden comparison
/// space). The alpha channel carries the exact anti-aliased coverage.
fn render_truth(path: &str, winding: WindingRule) -> Vec<u8> {
    let mut surface =
        surfaces::raster_n32_premul((CANVAS, CANVAS)).expect("truth raster surface allocates");
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);
    let mut skia_path = Path::from_svg(path).expect("the census path parses in Skia");
    skia_path.set_fill_type(sk_fill_type(winding));
    let mut paint = SkPaint::default();
    paint.set_color(SkColor::BLACK);
    paint.set_anti_alias(true);
    canvas.save();
    canvas.translate((ORIGIN, ORIGIN));
    canvas.draw_path(&skia_path, &paint);
    canvas.restore();

    let info = ImageInfo::new(
        (CANVAS, CANVAS),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = CANVAS as usize * 4;
    let mut pixels = vec![0u8; row_bytes * CANVAS as usize];
    assert!(
        surface.read_pixels(&info, &mut pixels, row_bytes, (0, 0)),
        "the truth surface reads back"
    );
    pixels
}

/// BAKE: bakes the path at `px_per_em`, packs the one-shape atlas, hand-builds a
/// `.dsb` whose single node carries a `Field` paint entry masking an opaque
/// black fill, and renders it through the Skia reference painter — the real
/// median-of-3 field resolve (B1.3). Returns unpremultiplied RGBA8888 rows; the
/// alpha channel carries the field coverage. A `bake_single`-style refusal
/// (unsupported command, degenerate geometry) would panic here — the census
/// shapes never trigger one; that boundary is the generator's own test.
fn render_bake(path: &str, winding: WindingRule, px_per_em: f64) -> Vec<u8> {
    let mut baker = VectorAtlasBaker::with_resolution(px_per_em, DEFAULT_DISTANCE_RANGE);
    let index = baker
        .add(&VectorPath { path, winding })
        .expect("the census path bakes");
    let out = baker.finish().expect("the one-shape atlas packs");
    let placement = &out.shapes[index as usize];
    let r = placement.atlas_rect;
    let pb = placement.plane_bounds;

    let mut b = FlatBufferBuilder::new();
    let bytes = b.create_vector(&out.image_png);
    let image = Image::create(
        &mut b,
        &ImageArgs {
            format: ImageFormat::Png,
            bytes: Some(bytes),
        },
    );
    let atlas = VectorAtlas::create(
        &mut b,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: out.px_per_em as f32,
            distance_range: out.distance_range as f32,
        },
    );
    let shape = VectorShape::create(
        &mut b,
        &VectorShapeArgs {
            atlas: 0,
            atlas_rect: Some(&AtlasRect::new(r.x, r.y, r.width, r.height)),
            plane_bounds: Some(&PlaneBounds::new(
                pb.left as f32,
                pb.top as f32,
                pb.right as f32,
                pb.bottom as f32,
            )),
        },
    );
    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(0.0, 0.0, 0.0, 1.0)),
        },
    );
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            shape_field: 0,
            ..Default::default()
        },
    );
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&FixedSizeLayout::new(ORIGIN, ORIGIN, 150.0, 150.0)),
            paint_entry: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let images = b.create_vector(&[image]);
    let paints = b.create_vector(&[paint]);
    let vector_atlases = b.create_vector(&[atlas]);
    let vector_shapes = b.create_vector(&[shape]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            images: Some(images),
            paints: Some(paints),
            vector_atlases: Some(vector_atlases),
            vector_shapes: Some(vector_shapes),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let dsb = b.finished_data().to_vec();

    let document = root_as_document(&dsb).expect("the hand-built .dsb is valid");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    let scene = arena.committed();
    let mut painter = SkiaPainter::new(CANVAS, CANVAS);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

/// The measured coverage difference over a shape's footprint.
struct BakeDiff {
    footprint: usize,
    differing: usize,
    max_delta: u8,
}

impl BakeDiff {
    fn fraction(&self) -> f64 {
        if self.footprint == 0 {
            0.0
        } else {
            self.differing as f64 / self.footprint as f64
        }
    }
}

/// Footprint-relative alpha (coverage) diff of a bake render against a truth
/// render. Both carry coverage in alpha; comparing alpha alone measures
/// coverage without the undefined-RGB noise of a fully transparent pixel.
fn footprint_diff(truth: &[u8], bake: &[u8]) -> BakeDiff {
    let mut footprint = 0usize;
    let mut differing = 0usize;
    let mut max_delta = 0u8;
    for (t, k) in truth.chunks_exact(4).zip(bake.chunks_exact(4)) {
        let (truth_cov, bake_cov) = (t[3], k[3]);
        if truth_cov.max(bake_cov) <= FOOTPRINT_EPSILON {
            continue;
        }
        footprint += 1;
        let delta = truth_cov.abs_diff(bake_cov);
        max_delta = max_delta.max(delta);
        if delta > BAKE_CHANNEL_DELTA {
            differing += 1;
        }
    }
    BakeDiff {
        footprint,
        differing,
        max_delta,
    }
}

/// One rung of the escalation ladder: the resolution tried and what it measured.
struct Rung {
    px_per_em: f64,
    fraction: f64,
    max_delta: u8,
}

/// The escalation outcome for one shape.
enum BakeOutcome {
    /// Passed at `px_per_em` (the first rung within tolerance) at `fraction`.
    Fielded { px_per_em: f64, fraction: f64 },
    /// Over tolerance at every rung including the ceiling — unfieldable (the
    /// bake-side cause of a `figma.unsupported` refusal, P4).
    Unfieldable,
}

/// Bakes `path` up the [`PX_PER_EM_LADDER`], stopping at the first rung within
/// [`BAKE_TOLERANCE`]. Records every rung tried (for the report) and returns the
/// outcome. Escalate-until-first-pass is deliberate: it is monotone in effort,
/// so a shape whose residual is non-monotone in `px_per_em` (sharp corners and
/// sub-`distance_range` features reconstruct differently at different texel
/// grids) still lands at the lowest resolution that meets the bar.
fn field_or_refuse(path: &str, winding: WindingRule) -> (BakeOutcome, Vec<Rung>) {
    let truth = render_truth(path, winding);
    let mut rungs = Vec::new();
    for &px_per_em in &PX_PER_EM_LADDER {
        let bake = render_bake(path, winding, px_per_em);
        let diff = footprint_diff(&truth, &bake);
        let fraction = diff.fraction();
        rungs.push(Rung {
            px_per_em,
            fraction,
            max_delta: diff.max_delta,
        });
        if fraction <= BAKE_TOLERANCE {
            return (
                BakeOutcome::Fielded {
                    px_per_em,
                    fraction,
                },
                rungs,
            );
        }
    }
    (BakeOutcome::Unfieldable, rungs)
}

/// A 5-point star, y-down, centred at (75, 75): outer radius 70, inner radius
/// 28, first outer point straight up. Ten straight segments, NONZERO — sharp
/// convex points and reflex inner corners, the MSDF-corner stress case.
fn star_path() -> String {
    let (cx, cy) = (75.0_f64, 75.0_f64);
    let (outer, inner) = (70.0_f64, 28.0_f64);
    let mut path = String::new();
    for k in 0..10 {
        let angle = -std::f64::consts::FRAC_PI_2 + (k as f64) * std::f64::consts::PI / 5.0;
        let radius = if k % 2 == 0 { outer } else { inner };
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();
        let cmd = if k == 0 { 'M' } else { 'L' };
        path.push_str(&format!("{cmd} {x:.3} {y:.3} "));
    }
    path.push('Z');
    path
}

/// The representative census vocabulary (extends the fixture shapes,
/// docs/wip/2026-07-19-B1-vector-msdf-design.md): a star, a filled arrow, a
/// cubic-Bézier organic blob, an EVENODD square-with-a-hole, and a stroke-like
/// thin bar. Every one must bake within tolerance at or before the ceiling.
fn census_shapes() -> Vec<(&'static str, String, WindingRule)> {
    vec![
        ("star-5-point", star_path(), WindingRule::NonZero),
        (
            "arrow",
            "M 0 45 L 90 45 L 90 20 L 145 75 L 90 130 L 90 105 L 0 105 Z".to_string(),
            WindingRule::NonZero,
        ),
        (
            "organic-blob",
            "M 30 75 C 30 25, 120 25, 130 70 C 140 120, 70 145, 40 120 C 15 100, 30 100, 30 75 Z"
                .to_string(),
            WindingRule::NonZero,
        ),
        (
            "square-with-hole",
            "M 5 5 L 145 5 L 145 145 L 5 145 Z M 45 45 L 45 105 L 105 105 L 105 45 Z".to_string(),
            WindingRule::EvenOdd,
        ),
        (
            "thin-stroke",
            "M 5 72 L 155 72 L 155 78 L 5 78 Z".to_string(),
            WindingRule::NonZero,
        ),
    ]
}

#[test]
fn census_shapes_bake_within_tolerance_at_the_fixed_default() {
    let mut escalated = 0usize;
    let mut lines = Vec::new();
    let mut failures = Vec::new();

    for (name, path, winding) in census_shapes() {
        let (outcome, rungs) = field_or_refuse(&path, winding);
        let ladder = rungs
            .iter()
            .map(|r| format!("{:.0}:{:.3}%", r.px_per_em, r.fraction * 100.0))
            .collect::<Vec<_>>()
            .join(" -> ");
        match outcome {
            BakeOutcome::Fielded {
                px_per_em,
                fraction,
            } => {
                if px_per_em > DEFAULT_PX_PER_EM {
                    escalated += 1;
                }
                lines.push(format!(
                    "  {name:16} fielded @ {px_per_em:.0} px/em ({:.3}% of footprint) [{ladder}]",
                    fraction * 100.0
                ));
            }
            BakeOutcome::Unfieldable => {
                failures.push(format!(
                    "{name} exceeded the {:.0}% bake tolerance at every rung [{ladder}] — a \
                     census shape must field",
                    BAKE_TOLERANCE * 100.0
                ));
            }
        }
    }

    eprintln!(
        "BAKE ORACLE (B1.5): {} census shapes, tolerance {:.0}% of footprint at max channel \
         delta {BAKE_CHANNEL_DELTA}",
        census_shapes().len(),
        BAKE_TOLERANCE * 100.0
    );
    for line in &lines {
        eprintln!("{line}");
    }

    assert!(
        failures.is_empty(),
        "bake-fidelity failures:\n{}",
        failures.join("\n")
    );
    // Every census shape fields at the fixed default `px_per_em` (48) — none
    // needs escalation. This is the documented census finding ("zero shapes need
    // escalation"), and it is what lets v0.10 ship a fixed-48 bake with the
    // escalation ladder deferred to production (#357). It is also the oracle-side
    // guard on the plane_bounds framing: before that fix the mis-sized field
    // pushed star/arrow/blob up the ladder (48 px/em ran 5–6% of footprint over
    // tolerance), so a regression that reintroduced the anisotropic error would
    // make a shape escalate again and fail this assertion. (The escalation
    // ladder itself is still walked end-to-end by the refusal test below.)
    assert_eq!(
        escalated, 0,
        "a census shape escalated above {DEFAULT_PX_PER_EM:.0} px/em — with correct plane-bounds \
         geometry every census shape fields at the fixed default; escalation is deferred (#357)"
    );
}

/// A "barcode": 80 vertical teeth, each 1 unit wide on a 2-unit period, filling
/// a 160-unit span 100 tall (each tooth a separate NONZERO contour). The detail
/// is far finer than the atlas texel grid at every ladder rung — at the 192
/// px/em ceiling a tooth is ~1.2 texels wide with a ~1.2-texel gap, well under
/// the 4-texel MSDF distance range — so the field blurs the teeth toward gray
/// while the exact fill keeps 80 sharp bars. This is a genuine reconstruction
/// pathology (high-frequency detail below the resolvable grid), unlike a plain
/// thin bar, which a correct field reconstructs faithfully.
fn barcode_path() -> String {
    let tooth = 1.0_f64;
    let period = 2.0 * tooth;
    let n = (160.0 / period) as usize;
    let mut path = String::with_capacity(n * 40);
    for i in 0..n {
        let x0 = 8.0 + i as f64 * period;
        let x1 = x0 + tooth;
        path.push_str(&format!("M {x0} 10 L {x1} 10 L {x1} 110 L {x0} 110 Z "));
    }
    path
}

#[test]
fn a_sub_texel_barcode_is_refused_at_the_ceiling() {
    // Detail finer than the atlas texel grid cannot be reconstructed at any
    // rung, so the disagreement stays over tolerance up to the ceiling. This is
    // the executable refusal boundary: the bake-side cause of a
    // `figma.unsupported` refusal (P4; the lowering has no dedicated
    // `figma.vector-unfieldable` code). The census has zero such nodes in either
    // live target — the refusal arm is defensive — so a synthetic barcode
    // exercises it; a plain thin bar does not, because a correctly framed field
    // reconstructs a straight-edged bar within tolerance at 48 px/em (measured).
    // This test also walks the full escalation ladder end to end (48 -> 192).
    // Both the ladder and the refusal live only in this oracle — production bakes
    // at fixed 48 and never escalates or refuses on reconstruction (#357).
    let (outcome, rungs) = field_or_refuse(&barcode_path(), WindingRule::NonZero);
    let ladder = rungs
        .iter()
        .map(|r| {
            format!(
                "{:.0}:{:.3}%(maxΔ{})",
                r.px_per_em,
                r.fraction * 100.0,
                r.max_delta
            )
        })
        .collect::<Vec<_>>()
        .join(" -> ");
    eprintln!("BAKE ORACLE refusal arm: barcode [{ladder}]");
    assert!(
        matches!(outcome, BakeOutcome::Unfieldable),
        "the sub-texel barcode must be refused at the ceiling, not fielded [{ladder}]"
    );
    // Every rung must genuinely exceed tolerance — the refusal is earned at the
    // ceiling, not an early-exit artifact.
    assert!(
        rungs.iter().all(|r| r.fraction > BAKE_TOLERANCE),
        "the barcode must exceed the {:.0}% tolerance at every rung [{ladder}]",
        BAKE_TOLERANCE * 100.0
    );
    assert_eq!(
        rungs.len(),
        PX_PER_EM_LADDER.len(),
        "a refusal must have tried the whole ladder up to the ceiling"
    );
}
