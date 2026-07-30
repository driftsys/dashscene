//! The v0.11 backdrop-blur render (story #393, stage B-3). Each scene is
//! authored through `dashscene-core`'s producer API (`Prop::Blurs`),
//! committed, and rendered by the reference painter — the whole path from
//! backdrop-blur intent to pixels.
//!
//! `docs/decisions/backdrop-blur-is-core-vocabulary.md`. A backdrop blur is
//! the first effect that reads what is already composited beneath a node, so
//! beyond the golden this file pins the three properties that reading has,
//! each of which a plausible implementation gets wrong in a way the golden's
//! area tolerance would not catch:
//!
//! - the sample reads **past the node's own box**, so the blur is built from
//!   the real backdrop rather than from a copy truncated at the node's clip;
//! - inside a render-target [`GroupComposite`] the sample reads **that
//!   group's layer**, not the canvas beneath it, and outside one it reads the
//!   canvas — the question boundary B left open, settled in the decision
//!   record;
//! - a baked-vector node's blur is confined to the **field's coverage**, not
//!   to its box. That is the shape the live hero's own frosted panel has.

use dashc_wasm::figma::vector_field::{VectorAtlasBaker, VectorPath, WindingRule};
use dashpaint::{
    Blur, BlurKind, Color, GlyphRunTable, ImageAsset, ImageFormat, ImageTable, Painter, VectorField,
};
use dashscene_core::{Arena, NodeId, Prop, Txn};
use dashscene_skia::SkiaPainter;

const SIZE: usize = 64;
const TOLERANCE: f64 = 0.02;

/// The 2% tolerance is ~82 pixels on the 64×64 canvas. The sensitivity guard
/// requires the blur to move the render well past that — the same 250 px
/// floor the shadow goldens use, and for the same reason: a tolerance budget
/// alone cannot prove a golden pins the effect it is named for. Measured,
/// removing the blur changes 856 px, so the floor sits ≈3× above the
/// tolerance budget and well below the blur's actual footprint.
const SENSITIVITY_FLOOR: usize = 250;

const NAVY: Color = Color {
    r: 0.06,
    g: 0.08,
    b: 0.16,
    a: 1.0,
};
const AMBER: Color = Color {
    r: 0.98,
    g: 0.78,
    b: 0.20,
    a: 1.0,
};
const GREEN: Color = Color {
    r: 0.10,
    g: 0.70,
    b: 0.30,
    a: 1.0,
};
const WHITE: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};
const BLACK: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
/// The frosted panel's own fill: white at 0.2 alpha, the fixture's value
/// (`corpus/figma-fixtures/backdrop-blur.json`).
const FROST: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 0.2,
};

fn boxed(txn: &mut Txn<'_>, parent: Option<NodeId>, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    let node = txn.add_node(parent, None);
    txn.set_prop(node, Prop::X(x));
    txn.set_prop(node, Prop::Y(y));
    txn.set_prop(node, Prop::Width(w));
    txn.set_prop(node, Prop::Height(h));
    node
}

fn backdrop_blur(radius: f32) -> Prop {
    Prop::Blurs(vec![Blur {
        kind: BlurKind::Backdrop,
        radius,
    }])
}

fn render_sized(arena: &Arena, painter: &mut SkiaPainter, images: &ImageTable) -> Vec<u8> {
    let scene = arena.committed();
    painter.paint(
        scene.rects(),
        scene.paints(),
        images,
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
    painter.rgba_bytes()
}

fn render(arena: &Arena, painter: &mut SkiaPainter) -> Vec<u8> {
    render_sized(arena, painter, &ImageTable::new())
}

fn quantized(c: Color) -> [u8; 4] {
    let q = |v: f32| (v * 255.0).round() as u8;
    [q(c.r), q(c.g), q(c.b), q(c.a)]
}

/// The count of differing pixels between two RGBA8888 buffers — the
/// sensitivity guard's measure, inlined so this golden stays self-contained
/// (the same helper `v08_shadows.rs` carries).
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

/// A frosted panel over a hard colour seam — the shape the committed fixture
/// has, at golden scale.
///
///   bg navy 64×64
///     └── band amber (0,0) 32×64        — a hard seam down x = 32
///     └── panel (16,16) 32×32 rounded r=8, white α=0.2,
///         backdrop blur radius 12
///
/// The panel straddles the seam, so a correct blur carries amber rightward
/// and navy leftward *inside the panel only*, while the seam stays hard
/// everywhere the panel does not cover.
fn frosted_scene(arena: &mut Arena, with_blur: bool) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
    txn.set_prop(band, Prop::Fill(AMBER));

    let panel = boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
    txn.set_prop(
        panel,
        Prop::Corners {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    );
    txn.set_prop(panel, Prop::Fill(FROST));
    if with_blur {
        txn.set_prop(panel, backdrop_blur(12.0));
    }
    txn.commit();
}

#[test]
fn the_frosted_panel_scene_matches_its_golden() {
    let mut arena = Arena::new();
    frosted_scene(&mut arena, true);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // Above the panel the seam is untouched: the last amber column and the
    // first navy column keep their exact colours. A blur that leaked past
    // the node's shape would soften them.
    assert_eq!(
        probe(31, 8),
        quantized(AMBER),
        "the seam stays hard above the panel"
    );
    assert_eq!(
        probe(32, 8),
        quantized(NAVY),
        "the seam stays hard above the panel"
    );

    // Inside the panel, on the seam, the two bands have mixed. The seam is
    // the panel's own centre line, so a symmetric Gaussian weights the two
    // sides almost equally and the pixel lands within a few code points of
    // their midpoint on every channel — a much tighter claim than "somewhere
    // between", and one only an actual average of the neighbourhood
    // satisfies. Without the blur this pixel is frosted navy exactly.
    let seam = probe(32, 32);
    let amber_side = probe(20, 32);
    let navy_side = probe(44, 32);
    for channel in 0..3 {
        let (lo, hi) = (
            i32::from(navy_side[channel]),
            i32::from(amber_side[channel]),
        );
        let got = i32::from(seam[channel]);
        assert!(
            got > lo && got < hi,
            "the blur mixes the bands inside the panel: seam {seam:?} channel {channel} \
             must sit between the navy side {navy_side:?} and the amber side {amber_side:?}"
        );
        assert!(
            (got - (lo + hi) / 2).abs() <= 10,
            "the mix is an even average at the panel's centre line: seam {seam:?} channel \
             {channel} is {got}, halfway between {lo} and {hi} is {}",
            (lo + hi) / 2
        );
    }

    // Well away from the seam the blur of a uniform region is that region,
    // so each side reads as its own band seen through the 0.2-alpha frost:
    // 0.2·white + 0.8·band. This pins the frost composite, and nothing more:
    // over a uniform backdrop a truncated backdrop copy is byte-identical
    // here, because whatever alpha the copy loses composites back over the
    // sharp original, which is the same colour. Truncation is caught by
    // `the_backdrop_blur_reads_past_the_node_box`, which is the only place
    // that claim is measured.
    let frosted = |band: Color| {
        let mix = |v: f32| ((0.2 + 0.8 * v) * 255.0).round() as i32;
        [mix(band.r), mix(band.g), mix(band.b)]
    };
    for (label, px, band) in [
        ("the amber side", amber_side, AMBER),
        ("the navy side", navy_side, NAVY),
    ] {
        let want = frosted(band);
        for (channel, (got, expect)) in px.iter().take(3).zip(want).enumerate() {
            assert!(
                (i32::from(*got) - expect).abs() <= 8,
                "{label} away from the seam is the band seen through the frost: \
                 channel {channel} is {got}, expected about {expect} ({px:?})"
            );
        }
    }

    // Sensitivity: the same scene with no blur differs from the good render
    // by far more than the 2% tolerance budget (~82 px), so a regression that
    // drops the blur cannot pass the golden.
    let mut broken_arena = Arena::new();
    frosted_scene(&mut broken_arena, false);
    let mut broken_painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let broken = render(&broken_arena, &mut broken_painter);
    let diff = differing(&bytes, &broken);
    assert!(
        diff > SENSITIVITY_FLOOR,
        "the backdrop-blur golden must pin the blur: a no-blur render differs by only {diff} px \
         (floor {SENSITIVITY_FLOOR})"
    );

    goldens::assert_matches_golden_within("v011-backdrop-blur", &painter.png_bytes(), TOLERANCE);
}

/// A fill-less panel whose left edge sits exactly on a black/white seam.
///
///   bg white 64×64
///     └── band black (0,0) 32×64
///     └── panel (32,16) 24×32, no fill, backdrop blur radius 12
///
/// Everything beneath the panel is white. The only way a pixel just inside
/// the panel can darken is if the blur read the black band, which lies
/// entirely *outside* the panel's box.
///
/// **These probes bound the blur's shape, not its width** (issue #409). The
/// edge probe's band has no upper bound on sigma — as sigma grows that pixel
/// tends to 127.5, which stays inside it — and the deep probe passes on a
/// render where the blur does nothing at all, the no-blur value there being
/// 255; it is a falloff guard only because the edge probe is paired with it.
/// Neither is evidence about the radius-to-sigma mapping.
/// `the_backdrop_blur_spreads_at_the_mapped_sigma` is the only place in this
/// tree that measures the mapping outside the golden image.
#[test]
fn the_backdrop_blur_reads_past_the_node_box() {
    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
        txn.set_prop(bg, Prop::Fill(WHITE));
        let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
        txn.set_prop(band, Prop::Fill(BLACK));
        let panel = boxed(&mut txn, Some(bg), 32.0, 16.0, 24.0, 32.0);
        txn.set_prop(panel, backdrop_blur(12.0));
        txn.commit();
    }
    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    // One pixel inside the panel's left edge. A blur whose input stopped at
    // the node's box would see only white here and leave this at 255; the
    // real backdrop is ~40% black at this distance from the seam.
    let edge = probe(33, 32);
    assert!(
        (60..200).contains(&i32::from(edge[0])),
        "the blur must pull the black band in from outside the node's box: the pixel just \
         inside the panel edge is {edge:?}, which is what a backdrop truncated at the box \
         would leave white"
    );
    // Deep inside the panel, ~4 sigma from the seam, the backdrop is white
    // again — the blur has a falloff, it does not flood the node.
    let deep = probe(55, 32);
    assert!(
        deep[0] > 245,
        "the blur falls off: deep inside the panel the backdrop is white again, got {deep:?}"
    );
    // Outside the panel the seam is untouched.
    assert_eq!(
        probe(31, 4),
        quantized(BLACK),
        "the band outside the panel is unblurred"
    );
    assert_eq!(
        probe(32, 4),
        quantized(WHITE),
        "the seam outside the panel is hard"
    );
}

/// The canvas the sigma measurement renders, and the panel and row it reads.
const SIGMA_SURFACE: i32 = 192;
const SIGMA_HEIGHT: i32 = 64;
/// The band's right edge, which the panel is centred on.
const SIGMA_SEAM: f64 = 96.0;
const SIGMA_PANEL_X: usize = 32;
const SIGMA_PANEL_W: usize = 128;
/// The panel's vertical middle, far from its top and bottom edges.
const SIGMA_ROW: usize = 32;

/// The standard deviation, in device pixels, of the blur kernel the painter
/// actually applied at `radius`.
///
///   bg white 192×64
///     └── band black (0,0) 96×64            — a hard seam down x = 96
///     └── panel (32,8) 128×48, no fill, backdrop blur radius `radius`
///
/// The panel is centred on the seam and carries no fill, so along
/// [`SIGMA_ROW`] the render *is* the blurred backdrop. Both panel edges sit
/// 64 px from the seam, well past four sigma at the radii measured, so the row
/// starts at pure black and ends at pure white and the profile is complete.
/// Where the kernel reaches past the canvas the painter's `TileMode::Clamp`
/// extends the edge column, which is the band on the left and the background
/// on the right — the same values an unbounded canvas would supply, so the
/// canvas edges contribute no error.
///
/// A blurred step edge is the kernel's cumulative distribution scaled to the
/// step's height, so the differences between neighbouring columns are the
/// kernel itself and their second moment about the seam is its variance. That
/// makes the measurement independent of the kernel's *shape*: Skia
/// approximates a Gaussian with three box passes, and a fit against an erf
/// profile would charge that approximation to sigma, while a second moment
/// does not.
fn measured_blur_sigma(radius: f32) -> f64 {
    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        let bg = boxed(
            &mut txn,
            None,
            0.0,
            0.0,
            SIGMA_SURFACE as f32,
            SIGMA_HEIGHT as f32,
        );
        txn.set_prop(bg, Prop::Fill(WHITE));
        let band = boxed(
            &mut txn,
            Some(bg),
            0.0,
            0.0,
            SIGMA_SEAM as f32,
            SIGMA_HEIGHT as f32,
        );
        txn.set_prop(band, Prop::Fill(BLACK));
        let panel = boxed(
            &mut txn,
            Some(bg),
            SIGMA_PANEL_X as f32,
            8.0,
            SIGMA_PANEL_W as f32,
            48.0,
        );
        txn.set_prop(panel, backdrop_blur(radius));
        txn.commit();
    }
    let mut painter = SkiaPainter::new(SIGMA_SURFACE, SIGMA_HEIGHT);
    let bytes = render(&arena, &mut painter);
    let row: Vec<f64> = (SIGMA_PANEL_X..SIGMA_PANEL_X + SIGMA_PANEL_W)
        .map(|x| f64::from(goldens::pixel(&bytes, SIGMA_SURFACE as usize, x, SIGMA_ROW)[0]))
        .collect();

    // The profile must be complete inside the panel, or the moments below
    // are taken over a truncated kernel and read low.
    assert_eq!(
        (row[0], row[row.len() - 1]),
        (0.0, 255.0),
        "the blurred edge must reach both flats inside the panel at radius {radius}, or the \
         second moment is measured over a truncated kernel: {row:?}"
    );

    let (mut mass, mut first, mut second) = (0.0f64, 0.0f64, 0.0f64);
    for (i, pair) in row.windows(2).enumerate() {
        let weight = pair[1] - pair[0];
        // The difference between columns x and x+1 belongs at their shared
        // boundary, x + 1, measured from the seam.
        let u = (SIGMA_PANEL_X + i + 1) as f64 - SIGMA_SEAM;
        mass += weight;
        first += u * weight;
        second += u * u * weight;
    }
    let mean = first / mass;
    // A symmetric kernel leaves the edge where it found it. An asymmetric one
    // would move it, and would also make the variance below mean less.
    assert!(
        mean.abs() <= 0.05,
        "the blur must be centred on the seam it blurs: the kernel's mean is {mean} px off at \
         radius {radius}"
    );
    (second / mass - mean * mean).sqrt()
}

/// The blur the painter applies is the width `blur_sigma` asks for.
///
/// Everything else in this file measures the blur's *shape* — that it reads
/// past the node's box, that it stops at a group, that it follows a field's
/// coverage. None of it constrains how wide the blur is, so before this test
/// the `sigma = radius / 2` mapping was pinned by the `v011-backdrop-blur`
/// golden image alone, and the non-golden probes admitted roughly any sigma in
/// (1.9, 13.3) against a true 6 (issue #409).
///
/// Measured here instead, as the second moment of the rendered step edge at
/// two radii. The two are not redundant: each pins the mapping to the interval
/// of sigmas that Skia quantises onto the same box-blur window, and the two
/// intervals are different, so their intersection is much narrower than
/// either.
///
/// | authored radius | `radius * 0.4375` | rendered sigma | mapping constants that render the same |
/// | --------------- | ----------------- | -------------- | -------------------------------------- |
/// | 12              | 5.25              | 5.1373         | 0.4212 … 0.4654                        |
/// | 24              | 10.5              | 10.1869        | 0.4322 … 0.4543                        |
///
/// **What stays unpinned: the mapping constant anywhere in 0.4322 … 0.4543**,
/// about −1.2 % / +3.8 % around the shipped 0.4375. That floor is Skia's, not
/// this test's: the raster blur converts sigma to an integer box-blur window,
/// so every constant in one of those intervals renders byte-identical pixels
/// and no pixel measurement can separate them. Narrowing the window further
/// needs more radii, each contributing its own interval.
///
/// The intersection is wider than the one this test carried while the constant
/// was 0.5 (0.4988 … 0.5092, about ±1 %), because the box-blur windows these
/// two radii land on at the lower constant are themselves wider relative to
/// it. The test is correspondingly weaker as a pin, and saying so is the point:
/// it is a *measured* upper bound on precision, not a claim of accuracy.
///
/// The rendered sigma runs below the nominal for the usual reason — the window
/// Skia selects is the one below what the requested sigma names — so the
/// expectations are the measured values rather than the nominal ones. Pinning
/// to the nominal 5.25 and 10.5 would need a tolerance wide enough to readmit
/// the neighbouring windows, which is the upper bound this test exists to
/// supply.
///
/// **This test previously held the mapping to 0.5 so that the refit could not
/// land quietly.** It did its job: issue #412's refit is the change that
/// re-recorded it, and the numbers above are that re-recording
/// (`docs/decisions/blur-sigma-is-figmas-mapping.md`). It now holds the
/// mapping to Figma's measured value, and `radius / 2` fails it — which is the
/// same guard pointing the other way, so a silent revert to the CSS convention
/// is caught too.
#[test]
fn the_backdrop_blur_spreads_at_the_mapped_sigma() {
    /// Well under half a box-blur window at these radii: wide enough that
    /// 8-bit rounding cannot reach it, narrow enough that the neighbouring
    /// windows (4.4502 and 5.4937; 9.4549 and 10.4986) stay out — the tighter
    /// side is radius 24's upper neighbour at +0.3117, so 0.15 clears it with
    /// room to spare but not by much. Named apart
    /// from this file's golden-area `TOLERANCE`, which it has nothing to do
    /// with.
    const SIGMA_TOLERANCE: f64 = 0.15;

    for (radius, expected) in [(12.0f32, 5.1373f64), (24.0, 10.1869)] {
        let got = measured_blur_sigma(radius);
        assert!(
            (got - expected).abs() <= SIGMA_TOLERANCE,
            "a backdrop blur of radius {radius} must render at sigma {expected} \
             ± {SIGMA_TOLERANCE}, the width `sigma = radius * 0.4375` asks for once Skia \
             has quantised it onto a box-blur window; got {got}"
        );
    }
}

/// The scene the [`GroupComposite`] decision is pinned on.
///
///   bg white 64×64
///     └── band `band_fill` (0,0) 24×64      — a sibling of the group
///     └── group (0,0) 64×64, no fill, opacity `alpha`
///           ├── cover green (24,16) 32×32
///           └── panel (28,20) 24×24, white α=0.2, backdrop blur radius 12
///
/// `cover` and `panel` overlap, so an `alpha` below 1 takes the
/// render-target path and the pair becomes a `GroupComposite`
/// (`docs/decisions/masks-and-group-opacity.md`); at `alpha` 1.0 no group is
/// emitted at all. The band is painted before the group and is not in it, so
/// it can only reach a pixel inside the panel through the blur.
///
/// [`GroupComposite`]: dashpaint::GroupComposite
fn grouped_scene(arena: &mut Arena, alpha: f32, band_fill: Color) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(WHITE));

    let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 24.0, 64.0);
    txn.set_prop(band, Prop::Fill(band_fill));

    let group = boxed(&mut txn, Some(bg), 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(group, Prop::Opacity(alpha));

    let cover = boxed(&mut txn, Some(group), 24.0, 16.0, 32.0, 32.0);
    txn.set_prop(cover, Prop::Fill(GREEN));

    let panel = boxed(&mut txn, Some(group), 28.0, 20.0, 24.0, 24.0);
    txn.set_prop(panel, Prop::Fill(FROST));
    txn.set_prop(panel, backdrop_blur(12.0));
    txn.commit();
}

fn grouped_render(alpha: f32, band_fill: Color) -> Vec<u8> {
    let mut arena = Arena::new();
    grouped_scene(&mut arena, alpha, band_fill);
    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    render(&arena, &mut painter)
}

/// A render-target group is a backdrop root: a backdrop-sampling node inside
/// one reads that group's layer, never the canvas beneath it.
///
/// Boundary B left this open — its guarantee fixes iteration order alone —
/// and `docs/decisions/backdrop-blur-is-core-vocabulary.md` settles it here,
/// because sampling through the group would composite the backdrop twice:
/// once directly, and once again inside the group's own alpha.
///
/// The pair of assertions is what makes this a measurement rather than a
/// restatement. Changing a band that only the blur could reach leaves the
/// pixel inside the panel byte-identical while the group isolates it, and
/// changes that same pixel once the group is gone — so the test would fail
/// if the sampling silently stopped happening, not only if it started
/// reading through.
#[test]
fn a_render_target_group_is_a_backdrop_root() {
    let inside_black = grouped_render(0.5, BLACK);
    let inside_white = grouped_render(0.5, WHITE);
    let probe_at = |bytes: &[u8], x, y| goldens::pixel(bytes, SIZE, x, y);

    // The control: the band itself changed, so the render did change.
    assert_ne!(
        probe_at(&inside_black, 10, 32),
        probe_at(&inside_white, 10, 32),
        "the two renders must differ where the band is, or this test measures nothing"
    );

    // Inside the isolating group, the panel's blur cannot see the band.
    assert_eq!(
        probe_at(&inside_black, 29, 32),
        probe_at(&inside_white, 29, 32),
        "a backdrop sample inside a render-target group reads that group's layer, so a band \
         outside the group cannot reach a pixel inside the panel"
    );

    // Without the isolating layer — the same scene at full group opacity, so
    // `dashscene-core` emits no `GroupComposite` — the same band does reach
    // the same pixel. This is the sampling being live, not absent.
    let free_black = grouped_render(1.0, BLACK);
    let free_white = grouped_render(1.0, WHITE);
    assert_ne!(
        probe_at(&free_black, 29, 32),
        probe_at(&free_white, 29, 32),
        "with no render-target group in the way, the same backdrop sample does read the band — \
         so the equality above is isolation, not a missing blur"
    );
}

/// A 40×40 square with a centred 20×20 hole, filled EVENODD so the hole
/// reads through — the same path `v010_vector_render.rs` bakes.
const SQUARE_WITH_HOLE: &str = "M 0 0 L 40 0 L 40 40 L 0 40 Z M 10 10 L 10 30 L 30 30 L 30 10 Z";

/// A baked-vector node's backdrop blur is confined to the field's coverage,
/// not to its box.
///
/// This is the shape the live hero's frosted panel actually has — a Figma
/// VECTOR carrying `BACKGROUND_BLUR` — so the vector path is the one the
/// story's fidelity number depends on. The hole makes the claim measurable:
/// a box-shaped blur would frost the hole too, and the hole is where the
/// backdrop must stay sharp.
/// The vector scenes' canvas, and the node box the field renders into: the
/// shape occupies device (20,20)–(60,60), and its padded field quad is
/// `ORIGIN + plane_bounds`, a little larger.
const VEC_SURFACE: i32 = 100;
const VEC_ORIGIN: f32 = 20.0;
const VEC_BOX: f32 = 40.0;

/// Bakes [`SQUARE_WITH_HOLE`] with the real generator and returns the image
/// table holding its atlas plus the resolved [`VectorField`] — the same
/// values `dashc` produces for a Figma VECTOR.
fn baked_square_with_hole() -> (ImageTable, VectorField) {
    let mut baker = VectorAtlasBaker::new();
    let shape = baker
        .add(&VectorPath {
            path: SQUARE_WITH_HOLE,
            winding: WindingRule::EvenOdd,
        })
        .expect("the path bakes");
    let baked = baker.finish().expect("the atlas packs");
    let placement = &baked.shapes[shape as usize];
    let mut images = ImageTable::new();
    let image = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: baked.image_png.clone(),
    });
    let field = VectorField {
        image,
        atlas_rect: [
            placement.atlas_rect.x,
            placement.atlas_rect.y,
            placement.atlas_rect.width,
            placement.atlas_rect.height,
        ],
        plane_bounds: [
            placement.plane_bounds.left as f32,
            placement.plane_bounds.top as f32,
            placement.plane_bounds.right as f32,
            placement.plane_bounds.bottom as f32,
        ],
        distance_range: baked.distance_range as f32,
    };
    (images, field)
}

/// A baked-vector node over a black band that ends at `seam`, with or without
/// its backdrop blur.
///
///   bg white 100×100
///     └── band black (0,0) `seam`×100
///     └── vector (20,20) 40×40, the square-with-hole field, blur radius 12
fn vector_scene(field: VectorField, seam: f32, with_blur: bool) -> Arena {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let bg = boxed(
        &mut txn,
        None,
        0.0,
        0.0,
        VEC_SURFACE as f32,
        VEC_SURFACE as f32,
    );
    txn.set_prop(bg, Prop::Fill(WHITE));
    let band = boxed(&mut txn, Some(bg), 0.0, 0.0, seam, VEC_SURFACE as f32);
    txn.set_prop(band, Prop::Fill(BLACK));
    let vector = boxed(&mut txn, Some(bg), VEC_ORIGIN, VEC_ORIGIN, VEC_BOX, VEC_BOX);
    txn.set_prop(vector, Prop::ShapeField(field));
    if with_blur {
        txn.set_prop(vector, backdrop_blur(12.0));
    }
    txn.commit();
    arena
}

fn vector_render(arena: &Arena, images: &ImageTable) -> Vec<u8> {
    let mut painter = SkiaPainter::new(VEC_SURFACE, VEC_SURFACE);
    render_sized(arena, &mut painter, images)
}

#[test]
fn a_baked_vector_blurs_only_inside_its_field() {
    /// The seam runs down the middle of the shape, crossing both the solid
    /// body and the hole.
    const SEAM: f32 = 40.0;

    let (images, field) = baked_square_with_hole();
    let arena = vector_scene(field, SEAM, true);
    let bytes = vector_render(&arena, &images);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, VEC_SURFACE as usize, x, y);

    // The hole spans device (30,30)–(50,50). On the seam inside it, the
    // backdrop must still be hard: pure black one side, pure white the other.
    assert_eq!(
        probe(38, 40),
        quantized(BLACK),
        "the hole is outside the field's coverage, so its backdrop stays sharp"
    );
    assert_eq!(
        probe(42, 40),
        quantized(WHITE),
        "the hole is outside the field's coverage, so its backdrop stays sharp"
    );

    // The solid body above the hole (y = 25 sits between the shape's top
    // edge at 20 and the hole's at 30) is blurred across the same seam.
    //
    // The band is a coverage claim — the body blurs where the hole does not —
    // and not a sigma one: it has no upper bound, since a wider blur only
    // carries these two pixels further toward the 127.5 they meet at. The
    // mapping is measured in `the_backdrop_blur_spreads_at_the_mapped_sigma`
    // (issue #409).
    for x in [38usize, 42] {
        let body = probe(x, 25);
        assert!(
            (40..215).contains(&i32::from(body[0])),
            "the field's solid body blurs the seam: pixel at x={x} is {body:?}, which a \
             coverage-masked blur must leave between black and white"
        );
    }
}

/// A baked-vector node's backdrop blur changes **nothing outside the field's
/// padded quad**.
///
/// This is a separate test from the coverage claim above because the two fail
/// separately, and this one failed. `SaveLayerRec::bounds` is a hint to Skia,
/// not a guarantee: with a backdrop filter the layer is allocated over the
/// device clip, so before `draw_backdrop_blur_field` clipped to the quad,
/// every layer pixel the `DstIn` rect did not cover kept a full-opacity
/// blurred backdrop and composited on restore — a single baked-vector node
/// blurred the whole frame wherever the backdrop had contrast within about
/// 14 px of it, including the canvas's own top row.
///
/// The probes of `a_baked_vector_blurs_only_inside_its_field` all sit inside
/// the quad, and its seam is vertical and full-height, so the leak was
/// loudest exactly where nothing was probed. Asserting over the whole canvas
/// rather than at chosen points is what makes that unrepeatable.
///
/// **Since debt #503 this no longer pins the rect clip.** The coverage now
/// enters as a clip shader rather than as a `DstIn` mask inside the layer, and
/// an uncovered pixel resolves to `lerp(dst, blurred, 0)` — the destination
/// unchanged — whether or not the rect clip bounds the layer. Deleting
/// `clip_rect` from `draw_backdrop_blur_field` leaves this test green, which
/// was measured rather than assumed. The rect stays as a bound on how much
/// Skia allocates and blurs, which is a cost this test cannot see; the claim
/// asserted below is confinement itself, and that is still real and still
/// worth pinning against a coverage or clip-shader regression.
#[test]
fn the_baked_vector_blur_is_confined_to_its_quad() {
    const SEAM: f32 = 40.0;

    let (images, field) = baked_square_with_hole();
    let blurred = vector_render(&vector_scene(field, SEAM, true), &images);
    let plain = vector_render(&vector_scene(field, SEAM, false), &images);

    // The padded field quad in device space, as the painter computes it —
    // `device = node origin + plane_bounds` — widened by one pixel so a
    // partially covered boundary pixel is not counted as a leak.
    let [left, top, right, bottom] = field.plane_bounds;
    let quad = (
        (VEC_ORIGIN + left) as i32 - 1,
        (VEC_ORIGIN + top) as i32 - 1,
        (VEC_ORIGIN + right).ceil() as i32 + 1,
        (VEC_ORIGIN + bottom).ceil() as i32 + 1,
    );

    let mut leaked = Vec::new();
    for y in 0..VEC_SURFACE {
        for x in 0..VEC_SURFACE {
            let (a, b) = (
                goldens::pixel(&blurred, VEC_SURFACE as usize, x as usize, y as usize),
                goldens::pixel(&plain, VEC_SURFACE as usize, x as usize, y as usize),
            );
            let inside = x >= quad.0 && x < quad.2 && y >= quad.1 && y < quad.3;
            if a != b && !inside {
                leaked.push((x, y, a, b));
            }
        }
    }
    assert!(
        leaked.is_empty(),
        "a baked-vector backdrop blur must change nothing outside its padded quad \
         {quad:?}, but {} pixel(s) outside it moved; first four: {:?}",
        leaked.len(),
        &leaked[..leaked.len().min(4)]
    );

    // The control: inside the quad it does change plenty, so the emptiness
    // above is confinement and not an absent blur.
    let changed = differing(&blurred, &plain);
    assert!(
        changed > 200,
        "the blur must still do its work inside the quad: only {changed} px changed"
    );
}

/// The field path reads its backdrop from **outside** the quad it is clipped
/// to — the baked-vector twin of `the_backdrop_blur_reads_past_the_node_box`.
///
/// The two paths confine the blur by different mechanisms (`clip_rrect` on a
/// box, `clip_rect` plus a coverage mask on a field), so evidence that one
/// reads its halo correctly does not transfer to the other. Here the black
/// band ends at x = 16, which is left of the padded quad entirely, so the
/// only way a pixel inside the shape can darken is a backdrop read from
/// beyond the clip.
#[test]
fn the_baked_vector_blur_reads_past_its_quad() {
    /// Left of the padded quad, which starts at `VEC_ORIGIN + plane_bounds[0]`
    /// (about 16.7). The band therefore lies wholly outside the clip.
    const SEAM: f32 = 16.0;

    let (images, field) = baked_square_with_hole();
    assert!(
        VEC_ORIGIN + field.plane_bounds[0] > SEAM,
        "the band must end left of the padded quad for this test to mean anything"
    );
    let bytes = vector_render(&vector_scene(field, SEAM, true), &images);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, VEC_SURFACE as usize, x, y);

    // Inside the shape's solid body, near its left edge at x = 20 and above
    // the hole. With the halo read the band pulls this down well below white;
    // with the backdrop truncated at the quad it stays 255.
    let near = probe(22, 25);
    assert!(
        near[0] < 245,
        "the field blur must read the band from outside its quad: the pixel just inside the \
         shape's left edge is {near:?}, which a quad-truncated backdrop would leave white"
    );
    // The hole still shows the sharp backdrop, which at x = 40 is white.
    assert_eq!(
        probe(40, 40),
        quantized(WHITE),
        "the hole keeps the sharp backdrop"
    );
}

/// A baked-vector node over a **transparent** canvas, with an opaque band
/// covering everything left of `TRANSPARENT_SEAM` — the scene
/// `a_baked_vector_blur_over_a_transparent_backdrop_softens_its_alpha_edge`
/// reads.
///
///   canvas 100×100, cleared to transparent
///     └── root, no fill
///           ├── band black opaque (0,0) 40×100
///           └── vector (20,20) 40×40, the square-with-hole field, no fill,
///               backdrop blur radius 12
///
/// [`vector_scene`] cannot serve here: its white background makes the backdrop
/// opaque everywhere, which is exactly the case that hides the defect.
fn transparent_vector_scene(field: VectorField) -> Arena {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = boxed(
        &mut txn,
        None,
        0.0,
        0.0,
        VEC_SURFACE as f32,
        VEC_SURFACE as f32,
    );
    let band = boxed(
        &mut txn,
        Some(root),
        0.0,
        0.0,
        TRANSPARENT_SEAM,
        VEC_SURFACE as f32,
    );
    txn.set_prop(band, Prop::Fill(BLACK));
    let vector = boxed(
        &mut txn,
        Some(root),
        VEC_ORIGIN,
        VEC_ORIGIN,
        VEC_BOX,
        VEC_BOX,
    );
    txn.set_prop(vector, Prop::ShapeField(field));
    txn.set_prop(vector, backdrop_blur(12.0));
    txn.commit();
    arena
}

/// The band's right edge, run down the middle of the shape so the seam
/// crosses both the solid body and the hole.
const TRANSPARENT_SEAM: f32 = 40.0;

/// A baked-vector backdrop blur **replaces** the region its field covers
/// rather than compositing over it (debt #503) — the baked-vector twin of
/// `a_backdrop_blur_over_a_transparent_backdrop_softens_its_alpha_edge` in
/// `crates/dashscene-skia/tests/painter.rs`, which pins the parametric path.
///
/// The defect is invisible over an opaque backdrop, because an opaque blurred
/// copy hides the original, and every other scene in this file has one. Over a
/// partially transparent backdrop the blurred copy is also partially
/// transparent, so compositing it `SrcOver` leaves the sharp original showing
/// through underneath: the blur's alpha falloff is lost and the alpha edge
/// stays hard at a flat 255. RGB is correct either way; only alpha moves.
///
/// The second half of the test is the trap the naive fix springs. Setting
/// `BlendMode::Src` on the layer paint — which is what fixed the parametric
/// path — replaces the whole clip, and this path's clip is the field's padded
/// bounding box rather than its outline. That writes transparent wherever the
/// coverage is zero and erases the real backdrop in the box around the shape,
/// which the hole probes here catch. Both halves must hold at once.
#[test]
fn a_baked_vector_blur_over_a_transparent_backdrop_softens_its_alpha_edge() {
    let (images, field) = baked_square_with_hole();
    let bytes = vector_render(&transparent_vector_scene(field), &images);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, VEC_SURFACE as usize, x, y);
    let alpha_at = |x: usize| probe(x, BODY_ROW)[3];

    /// Five pixels below the shape's top edge at y = 20 and five above the
    /// hole's at y = 30, so the field covers this row fully and the only
    /// structure the blur sees along it is the seam.
    const BODY_ROW: usize = 25;

    // Deep inside the band the blur has not yet pulled in any transparency.
    assert!(
        alpha_at(22) >= 250,
        "deep inside the band the blur should stay near-opaque, got {}",
        alpha_at(22),
    );

    // Approaching the seam at x = 40, the blurred copy must carry
    // progressively less alpha. Compositing over the sharp original instead
    // pins every one of these at 255.
    let (a32, a36, a39) = (alpha_at(32), alpha_at(36), alpha_at(39));
    assert!(
        a32 < 250 && a36 < a32 && a39 < a36,
        "the alpha edge must soften across the blur: got {a32} at x=32, {a36} at x=36, \
         {a39} at x=39 — a flat 255 means the layer composited over the sharp backdrop \
         instead of replacing it",
    );

    // The hole spans device (30,30)–(50,50) and is outside the field's
    // coverage, so the backdrop inside it must survive exactly: opaque black
    // left of the seam, fully transparent right of it. Replacing the whole
    // clip instead of the coverage erases both.
    assert_eq!(
        probe(34, 40),
        quantized(BLACK),
        "the hole is outside the field's coverage, so the band beneath it must survive \
         the replacement untouched"
    );
    assert_eq!(
        probe(46, 40),
        [0, 0, 0, 0],
        "the hole is outside the field's coverage, so the transparent canvas beneath it \
         must survive the replacement untouched"
    );
}

/// A non-finite blur radius renders exactly as no blur.
///
/// The document load path refuses one (`paint.blur.invalid-radius`), but the
/// producer API stores `Prop::Blurs` unchecked, so the painter is the last
/// place a NaN can be caught. It is caught by the guard being written as
/// `radius > 0.0`, which a NaN fails, rather than as a `<= 0.0` rejection,
/// which a NaN passes — and passing one to Skia has no defined result.
#[test]
fn a_non_finite_blur_radius_renders_as_no_blur() {
    for radius in [f32::NAN, f32::NEG_INFINITY, -4.0, 0.0] {
        let mut arena = Arena::new();
        {
            let mut txn = arena.open();
            let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
            txn.set_prop(bg, Prop::Fill(WHITE));
            let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
            txn.set_prop(band, Prop::Fill(BLACK));
            let panel = boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
            txn.set_prop(
                panel,
                Prop::Blurs(vec![Blur {
                    kind: BlurKind::Backdrop,
                    radius,
                }]),
            );
            txn.commit();
        }
        let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
        let got = render(&arena, &mut painter);

        let mut plain = Arena::new();
        {
            let mut txn = plain.open();
            let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
            txn.set_prop(bg, Prop::Fill(WHITE));
            let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
            txn.set_prop(band, Prop::Fill(BLACK));
            boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
            txn.commit();
        }
        let mut plain_painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
        let want = render(&plain, &mut plain_painter);

        assert_eq!(
            differing(&got, &want),
            0,
            "a blur radius of {radius} must render exactly as no blur"
        );
    }
}

/// The blurred panel's box. Named because the scenes build the panel from it
/// and one assertion picks that rect back out of the scene's five.
///
/// Picking it positionally would not fail if it picked a mark instead: every
/// rect the group holds carries the same free-path alpha, so the opacity
/// assertion below passes on any of them. Matching on geometry is what makes
/// the rect being asserted about the one the assertion names. The assertion's
/// own teeth are elsewhere — it fails when the group takes the render-target
/// path, or when the alpha is not folded at all.
const PANEL: (f32, f32, f32, f32) = (32.0, 8.0, 24.0, 40.0);

/// A band, a fill-less panel carrying `blurs`, and a partial-opacity ancestor
/// holding the panel plus two marks that do not overlap each other.
///
///   bg white 64×64
///     └── band black (0,0) 32×64
///     └── group (0,0) 64×64, opacity `alpha`
///           ├── mark green (0,56) 8×8
///           ├── mark green (56,56) 8×8
///           └── panel (32,8) 24×40, no fill, `blurs`
///
/// The two marks are what make the group's painted subtree real: they are the
/// pair `subtree_overlaps` tests, and they are disjoint, so the group takes the
/// free path and `alpha` folds into every subtree rect's `RectEntry::opacity`
/// (`docs/decisions/masks-and-group-opacity.md`). Were they to overlap, the
/// group would become a `GroupComposite` instead and
/// `the_backdrop_layer_carries_the_free_path_group_alpha` would say so.
///
/// The panel carries no fill, so the only ink it contributes is the blurred
/// backdrop itself. That is what lets the alpha claim be measured as
/// arithmetic on one composite rather than inferred through a frost fill.
fn dimmed_scene(alpha: f32, blurs: &[f32]) -> Arena {
    let (px, py, pw, ph) = PANEL;
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(WHITE));

    let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
    txn.set_prop(band, Prop::Fill(BLACK));

    let group = boxed(&mut txn, Some(bg), 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(group, Prop::Opacity(alpha));
    for x in [0.0, 56.0] {
        let mark = boxed(&mut txn, Some(group), x, 56.0, 8.0, 8.0);
        txn.set_prop(mark, Prop::Fill(GREEN));
    }

    let panel = boxed(&mut txn, Some(group), px, py, pw, ph);
    txn.set_prop(
        panel,
        Prop::Blurs(
            blurs
                .iter()
                .map(|&radius| Blur {
                    kind: BlurKind::Backdrop,
                    radius,
                })
                .collect(),
        ),
    );
    txn.commit();
    arena
}

/// The row the blur is measured along: y = 28 is the panel's vertical middle,
/// far from its top and bottom edges and from the marks, so every pixel in it
/// moves for one reason only — the backdrop read across the seam at x = 32.
const PROBE_ROW: usize = 28;

/// The panel's own columns, x = 32 through 55.
fn probe_row(bytes: &[u8]) -> Vec<u8> {
    (32..56)
        .map(|x| goldens::pixel(bytes, SIZE, x, PROBE_ROW)[0])
        .collect()
}

fn dimmed_render(alpha: f32, blurs: &[f32]) -> Vec<u8> {
    let arena = dimmed_scene(alpha, blurs);
    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    render(&arena, &mut painter)
}

/// The blurred-backdrop layer composites at the rect's **free-path group
/// alpha**, not at full opacity.
///
/// `backdrop_layer_paint` folds `RectEntry::opacity` into the paint the layer
/// composites through, and every scene above it in this file has
/// `rect.opacity == 1.0` — the grouped scene's panel included, because it sits
/// inside a render-target group and `dashscene-core` resets the rect's own
/// opacity to 1.0 there, the alpha being carried by the `GroupComposite`
/// instead. Deleting the fold therefore left all seven of them green (debt
/// #406).
///
/// The claim measured here is the CSS model `backdrop_layer_paint` documents:
/// below `alpha = 1.0` the filtered backdrop composites **at the element's
/// alpha over the unfiltered one**. So each pixel of the dimmed render must be
/// the alpha-weighted mix of the same pixel in the full-opacity blurred render
/// and in the unblurred one — an equality with no free parameter, which a
/// layer composited at any other opacity fails.
#[test]
fn the_backdrop_layer_carries_the_free_path_group_alpha() {
    const ALPHA: f32 = 0.5;
    const BLUR: [f32; 1] = [12.0];

    // The free path, asserted rather than assumed: no render-target group, and
    // the alpha landed on the panel's own rect. The panel is the last rect the
    // scene emits.
    let dimmed_arena = dimmed_scene(ALPHA, &BLUR);
    let scene = dimmed_arena.committed();
    assert!(
        scene.groups().is_empty(),
        "the group's painted subtree does not overlap, so it must take the free path rather \
         than becoming a GroupComposite — this test measures the free path only"
    );
    let panel_rect = scene
        .rects()
        .iter()
        .find(|r| (r.x, r.y, r.w, r.h) == PANEL)
        .expect("the blurred panel's own rect, matched on its box");
    assert_eq!(
        panel_rect.opacity, ALPHA,
        "the free path folds the ancestor's alpha into the blurred panel's own rect"
    );

    let dimmed = probe_row(&dimmed_render(ALPHA, &BLUR));
    let opaque = probe_row(&dimmed_render(1.0, &BLUR));
    let sharp = probe_row(&dimmed_render(1.0, &[]));

    // The control: at full opacity the blur moves this row a long way, so the
    // mix below is being checked over pixels that actually differ.
    let moved = opaque.iter().zip(&sharp).filter(|(a, b)| a != b).count();
    assert!(
        moved >= 10,
        "the blur must move a large part of the probe row at full opacity, or the mix proves \
         nothing: only {moved} of {} columns moved",
        opaque.len()
    );

    for (x, ((&got, &blurred), &plain)) in dimmed.iter().zip(&opaque).zip(&sharp).enumerate() {
        let want = (ALPHA * f32::from(blurred) + (1.0 - ALPHA) * f32::from(plain)).round() as i32;
        assert!(
            (i32::from(got) - want).abs() <= 2,
            "the blurred backdrop composites at the rect's free-path alpha: column x={} is \
             {got}, but {ALPHA} of the blurred value {blurred} over the unblurred {plain} is \
             {want}. Full row: dimmed {dimmed:?} opaque {opaque:?}",
            x + 32
        );
    }
}

/// Several backdrop blurs on one node apply **in list order, each over the
/// result of the last** — the posture the painter's loop states in a comment
/// and the shadow loops share for Figma's back-to-front `effects` array.
///
/// No scene above puts two backdrop blurs on one node, so `.take(1)` on the
/// blur loop left all seven green (debt #407). Three claims separate the real
/// behaviour from the ways a loop can go wrong:
///
/// - the compounded render is **darker than either radius alone** at every
///   column, because two passes spread the band further than one. A loop that
///   keeps only the first entry, or lets the last win, matches a single-radius
///   render exactly;
/// - the two orders **render differently**, so the list order is load-bearing
///   rather than incidental;
/// - **which** order is darker at the panel's left edge follows from "each
///   over the result of the last": both orders read the same sharp band
///   through their final pass, and what differs is the already-blurred inside
///   content that pass mixes in. After a radius-12 first pass the inside is
///   darker than after a radius-4 one, so applying the larger radius first
///   stays darker. A reversed loop inverts this.
#[test]
fn several_backdrop_blurs_compound_in_list_order() {
    const BIG: f32 = 12.0;
    const SMALL: f32 = 4.0;

    let big_first = probe_row(&dimmed_render(1.0, &[BIG, SMALL]));
    let small_first = probe_row(&dimmed_render(1.0, &[SMALL, BIG]));
    let big_only = probe_row(&dimmed_render(1.0, &[BIG]));
    let small_only = probe_row(&dimmed_render(1.0, &[SMALL]));

    // Compounding: never lighter than either single blur, and strictly darker
    // across the columns the second pass can still reach. `SMALL` alone has
    // fallen back to white by column 6, so the strict window stops there.
    const STRICT_COLUMNS: usize = 6;
    for (label, single) in [
        ("the larger radius", &big_only),
        ("the smaller", &small_only),
    ] {
        for (x, (&got, &alone)) in big_first.iter().zip(single).enumerate() {
            assert!(
                got <= alone,
                "two backdrop blurs must spread the band at least as far as one: column x={} \
                 is {got} compounded but {alone} under {label} alone. \
                 compounded {big_first:?} single {single:?}",
                x + 32
            );
            assert!(
                x >= STRICT_COLUMNS || got < alone,
                "two backdrop blurs must spread the band strictly further than one near the \
                 seam: column x={} is {got} both compounded and under {label} alone. \
                 compounded {big_first:?} single {single:?}",
                x + 32
            );
        }
    }

    // List order is load-bearing: the same two radii in the other order do not
    // render the same pixels.
    assert_ne!(
        big_first, small_first,
        "applying the two radii in the other order must not render identically, or nothing \
         distinguishes 'in list order' from 'in any order'"
    );

    // And it is the documented order that is applied. The left edge is where
    // the two differ most, both having the same sharp band outside it.
    assert!(
        big_first[0] + 8 < small_first[0],
        "each blur applies over the result of the last, so the larger radius applied first \
         leaves the panel's left edge darker: {} with {BIG} first, {} with {SMALL} first",
        big_first[0],
        small_first[0]
    );
}

/// A `BlurKind::Layer` blur renders **nothing**, and does not disturb a
/// backdrop blur sharing the list with it.
///
/// The painter's blur loop filters on `BlurKind::Backdrop`, so a layer blur
/// draws nothing at all. That is deliberate — layer blur is node-local,
/// budgeted at v1, and
/// `docs/decisions/backdrop-blur-is-core-vocabulary.md` records why it does not
/// ride along — and nothing in this tree emits one, because `dashc` lowers only
/// `BACKGROUND_BLUR`. It was still an unasserted claim (debt #408).
///
/// Pinning it keeps the gap a named gap: implementing layer blur has to update
/// this test deliberately, rather than changing the render unnoticed. The
/// mixed-list cases are what make the assertion about the **filter** rather
/// than about the first entry winning — dropping the filter would turn a layer
/// blur into a second backdrop blur, which
/// `several_backdrop_blurs_compound_in_list_order` shows is plainly visible.
#[test]
fn a_layer_blur_renders_as_no_blur() {
    const RADIUS: f32 = 12.0;

    // One `(kind, radius)` list per render, rather than two parallel slices: a
    // zip of two slices truncates to the shorter one, so a mismatched pair
    // would quietly render fewer blurs than the call site asked for.
    let render_blurs = |blurs: &[(BlurKind, f32)]| -> Vec<u8> {
        let (px, py, pw, ph) = PANEL;
        let mut arena = Arena::new();
        {
            let mut txn = arena.open();
            let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
            txn.set_prop(bg, Prop::Fill(WHITE));
            let band = boxed(&mut txn, Some(bg), 0.0, 0.0, 32.0, 64.0);
            txn.set_prop(band, Prop::Fill(BLACK));
            let panel = boxed(&mut txn, Some(bg), px, py, pw, ph);
            txn.set_prop(
                panel,
                Prop::Blurs(
                    blurs
                        .iter()
                        .map(|&(kind, radius)| Blur { kind, radius })
                        .collect(),
                ),
            );
            txn.commit();
        }
        let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
        render(&arena, &mut painter)
    };

    let none = render_blurs(&[]);
    let layer = render_blurs(&[(BlurKind::Layer, RADIUS)]);
    let backdrop = render_blurs(&[(BlurKind::Backdrop, RADIUS)]);

    assert_eq!(
        differing(&layer, &none),
        0,
        "a BlurKind::Layer blur renders nothing: the reference painter implements backdrop \
         blur only, and layer blur is a named gap"
    );

    // The control: the same radius as a backdrop blur moves the render a long
    // way, so the equality above is the kind filter and not a scene that
    // cannot show a blur at all.
    let moved = differing(&backdrop, &none);
    assert!(
        moved > SENSITIVITY_FLOOR,
        "the same radius as a backdrop blur must move the render, or the equality above proves \
         nothing: only {moved} px differ (floor {SENSITIVITY_FLOOR})"
    );

    // A layer blur beside a backdrop blur changes neither the backdrop blur's
    // result nor its position in the list. Both orders, because a filter that
    // was really a `first()` would pass one of them.
    for mixed_list in [
        [(BlurKind::Layer, RADIUS), (BlurKind::Backdrop, RADIUS)],
        [(BlurKind::Backdrop, RADIUS), (BlurKind::Layer, RADIUS)],
    ] {
        let mixed = render_blurs(&mixed_list);
        assert_eq!(
            differing(&mixed, &backdrop),
            0,
            "a BlurKind::Layer entry beside a backdrop blur is skipped, not applied and not \
             counted: {mixed_list:?} must render exactly as the backdrop blur alone"
        );
    }
}
