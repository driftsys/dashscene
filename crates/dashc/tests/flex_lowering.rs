//! The auto-layout lowering, end to end (story #140):
//!
//!     Figma auto-layout intent → lower → Document flex vocabulary → emit
//!         → .dsb → dashscene-core → TaffySolver → Skia painter
//!
//! The captured fixtures are both the input and the oracle. Figma's own
//! solver wrote every `absoluteBoundingBox` in a capture, so a lowering
//! that carries the *intent* correctly must, when solved by the runtime,
//! land on the same boxes Figma rendered — that comparison is the fidelity
//! check debt #105 asked for. P1 still holds inside the lowering itself:
//! the solved boxes are never written into the document, only asserted
//! against in these tests.
//!
//! The `hug-in-fill` solve test runs on a *derived* document — its `TEXT`
//! leaf swapped for a fixed-size `FRAME` — because solving text needs the
//! typesetter this binary does not wire, and a fixed frame of the shaped size
//! gives the hug chain the same content extent. The `negative-gap` derived
//! document (its five `ELLIPSE`s retyped to frames) predates #239 and is kept
//! only because the Deno suite byte-compares the same derived bytes for
//! cross-language ABI parity (`goldens/dsb/README.md`), and a frame and a
//! circle solve to one box. Since #239 the raw `negative-gap` capture also
//! emits — its ellipses lower to circles (corner radius = half the extent,
//! `docs/decisions/figma-ellipse-as-circle.md`) — and is pinned directly.

use std::collections::BTreeMap;

use dashc_wasm::figma::{CompileError, lower, rule};
use dashc_wasm::{AxisSizing, LayoutMode, MainAxisAlign, compile_figma};
use dashpaint::{CornerRadii, GlyphRunTable, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_validator::{Diagnostic, Location, Profile};

mod common;
use common::{derive, kind_is, node, parse, unsupported};

const HUG_IN_FILL: &str = include_str!("../../../corpus/figma-fixtures/lowering-hug-in-fill.json");
const NEGATIVE_GAP: &str =
    include_str!("../../../corpus/figma-fixtures/lowering-negative-gap.json");
const WRAP: &str = include_str!("../../../corpus/figma-fixtures/lowering-wrap.json");
const BASELINE: &str = include_str!("../../../corpus/figma-fixtures/lowering-baseline.json");
const GRID_BASIC: &str = include_str!("../../../corpus/figma-fixtures/grid-basic.json");
const VARIABLES_BOUND: &str = include_str!("../../../corpus/figma-fixtures/variables-bound.json");

fn lowered(json: &str) -> (dashc_wasm::Document, Vec<Diagnostic>) {
    lower(&parse(json), Profile::Core, &BTreeMap::new()).expect("the fixture lowers")
}

/// `lowering-negative-gap.json` with its five `ELLIPSE` children retyped as
/// `FRAME`s. Since #239 the ellipses lower directly (as circles), so this
/// derivation is no longer needed to make the fixture emit; it is kept because
/// a frame and a circle of the same fixed size solve to one box, and the Deno
/// suite byte-compares the same derived bytes for cross-language ABI parity
/// (`goldens/dsb/README.md`). Figma's captured boxes stay the oracle.
fn negative_gap_derived() -> String {
    derive(
        NEGATIVE_GAP,
        |object| kind_is(object, "ELLIPSE"),
        |object| {
            object.insert("type".to_string(), "FRAME".into());
        },
    )
}

/// `lowering-hug-in-fill.json` with its `TEXT` leaf swapped for a `FRAME`
/// fixed at the box Figma gave the text (87×17). Text lowering is story
/// #160; a fixed frame of the same size gives the hug chain the same
/// content extent, so Figma's captured boxes stay the oracle.
fn hug_in_fill_derived() -> String {
    derive(
        HUG_IN_FILL,
        |object| kind_is(object, "TEXT"),
        |object| {
            object.insert("type".to_string(), "FRAME".into());
            object.insert("layoutSizingHorizontal".to_string(), "FIXED".into());
            object.insert("layoutSizingVertical".to_string(), "FIXED".into());
        },
    )
}

/// Compile, load, and solve through the engine; returns the committed rects.
fn solved_rects(json: &str) -> Vec<dashpaint::RectEntry> {
    let (bytes, _) = compile_figma(json, Profile::Core, &BTreeMap::new())
        .expect("the derived document compiles");
    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");

    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    // `load_document` commits with the fixed solver; the flex intent needs
    // the engine. An empty transaction re-committed through a fresh
    // `TaffySolver` performs a full first solve.
    arena.open().commit_with(&mut TaffySolver::new());

    arena.committed().rects().to_vec()
}

// ---------------------------------------------------------------------------
// The authored intent lowers — and only the intent (P1).
// ---------------------------------------------------------------------------

#[test]
fn the_hug_in_fill_fixture_lowers_its_authored_flex_intent() {
    let (doc, diagnostics) = lowered(HUG_IN_FILL);

    // The root: a fixed-width, hug-height vertical stack.
    let (_, root) = node(&doc, "lowering-hug-in-fill");
    let container = root.container.expect("the root is an auto-layout frame");
    assert_eq!(container.mode, LayoutMode::Vertical);
    assert_eq!(container.gap, 12.0);
    assert_eq!(
        (
            container.padding.left,
            container.padding.top,
            container.padding.right,
            container.padding.bottom,
        ),
        (16.0, 16.0, 16.0, 16.0),
    );
    let constraints = root.constraints.expect("the root hugs one axis");
    assert_eq!(constraints.sizing_h, AxisSizing::Fixed);
    assert_eq!(constraints.sizing_v, AxisSizing::Hug);
    // Per-axis intent: the fixed width is authored (480); the hug height is
    // Figma's solver output and must not be baked in (P1).
    assert_eq!((root.box2d.width, root.box2d.height), (480.0, 0.0));

    // The FILL child of the fixed-width root.
    let (_, fill) = node(&doc, "fill-container");
    let constraints = fill.constraints.expect("fill-container fills");
    assert_eq!(constraints.sizing_h, AxisSizing::Fill);
    assert_eq!(constraints.sizing_v, AxisSizing::Hug);
    // A flex child's position and its non-fixed extents are solver output:
    // nothing but zeros may be written (P1).
    let b = fill.box2d;
    assert_eq!((b.x, b.y, b.width, b.height), (0.0, 0.0, 0.0, 0.0));

    // The HUG frame inside it.
    let (_, hug) = node(&doc, "hug-inside");
    let constraints = hug.constraints.expect("hug-inside hugs");
    assert_eq!(constraints.sizing_h, AxisSizing::Hug);
    assert_eq!(constraints.sizing_v, AxisSizing::Hug);
    let padding = hug.container.expect("hug-inside is a row").padding;
    assert_eq!((padding.left, padding.top), (10.0, 6.0));

    // Since story #160 the TEXT leaf lowers too, so the whole fixture is
    // clean — no unsupported construct remains. Its characters and style are
    // pinned in tests/text_lowering.rs; here it is enough that the leaf is
    // present and the flex chain around it lowered.
    assert!(
        unsupported(&diagnostics).is_empty(),
        "{:?}",
        unsupported(&diagnostics),
    );
    assert!(!node_missing(&doc, "hug inside fill"));
}

fn node_missing(doc: &dashc_wasm::Document, name: &str) -> bool {
    !doc.nodes.iter().any(|n| n.name.as_deref() == Some(name))
}

#[test]
fn a_negative_gap_lowers_to_leading_margins_before_emission() {
    // The document must never carry a negative gap
    // (docs/decisions/negative-gap-lowering.md): dashc lowers it at the
    // walk, the same rewrite core's Txn::lower_negative_gaps applies —
    // gap to zero, the gap onto the leading main-axis margin of every
    // child after the first.
    let (doc, diagnostics) = lower(
        &parse(&negative_gap_derived()),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the derived fixture lowers");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (_, root) = node(&doc, "lowering-negative-gap");
    let container = root.container.expect("the root is an auto-layout frame");
    assert_eq!(container.mode, LayoutMode::Horizontal);
    assert_eq!(container.gap, 0.0, "the authored -16 is gone");

    // The first in-flow child absorbs nothing; every later child pulls
    // itself 16 left, over its predecessor.
    let (_, first) = node(&doc, "overlap-1");
    assert_eq!(first.constraints, None, "fully default constraints");
    for name in ["overlap-2", "overlap-3", "overlap-4", "overlap-5"] {
        let (_, n) = node(&doc, name);
        let margin = n.constraints.expect("carries the lowered margin").margin;
        assert_eq!(margin.left, -16.0, "{name}");
        assert_eq!((margin.top, margin.right, margin.bottom), (0.0, 0.0, 0.0));
    }
}

#[test]
fn the_negative_gap_fixtures_ellipses_lower_to_circles() {
    // A full ELLIPSE with equal, fixed extents lowers to a rounded rect with
    // corner radius = half the extent — a circle, the only ellipse the
    // rounded-rect vocabulary expresses exactly
    // (docs/decisions/figma-ellipse-as-circle.md). The five 56x56 circles
    // lower clean, so the raw capture now emits (no derivation needed).
    let (doc, diagnostics) = lowered(NEGATIVE_GAP);
    assert!(
        unsupported(&diagnostics).is_empty(),
        "{:?}",
        unsupported(&diagnostics),
    );

    for name in [
        "overlap-1",
        "overlap-2",
        "overlap-3",
        "overlap-4",
        "overlap-5",
    ] {
        let (_, ellipse) = node(&doc, name);
        assert_eq!(
            ellipse
                .paint
                .as_ref()
                .expect("a filled circle")
                .entry
                .corners,
            CornerRadii {
                top_left: 28.0,
                top_right: 28.0,
                bottom_right: 28.0,
                bottom_left: 28.0,
            },
            "{name}: corner radius is half the 56px extent",
        );
        // A leaf: no container. The fixed 56x56 box stands (P1 permits a
        // Fixed axis's authored extent).
        assert!(
            ellipse.container.is_none(),
            "{name} is a leaf, not a container"
        );
        assert_eq!(
            (ellipse.box2d.width, ellipse.box2d.height),
            (56.0, 56.0),
            "{name}",
        );
    }

    // And under R6 the whole raw capture emits.
    let (bytes, report) = compile_figma(NEGATIVE_GAP, Profile::Core, &BTreeMap::new())
        .expect("the raw capture compiles now that its ellipses lower");
    assert!(report.is_empty(), "{report}");
    assert!(!bytes.is_empty());
}

/// A one-page document: a horizontal root frame holding one `ELLIPSE` named
/// `circle`, built from the shapes `lowering-negative-gap.json` pins. The
/// arc, extents, and per-axis sizing are the test's variables.
fn ellipse_doc(
    arc: serde_json::Value,
    width: f32,
    height: f32,
    sizing_h: &str,
    sizing_v: &str,
) -> String {
    serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "root", "type": "FRAME", "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 100.0 },
                "children": [{
                    "name": "circle", "type": "ELLIPSE",
                    "layoutSizingHorizontal": sizing_h,
                    "layoutSizingVertical": sizing_v,
                    "fills": [{ "type": "SOLID", "color": { "r": 0.3, "g": 0.5, "b": 0.9, "a": 1.0 } }],
                    "arcData": arc,
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": width, "height": height },
                }],
            }],
        }]},
    })
    .to_string()
}

/// A full ellipse's `arcData`: a `2π` sweep from `0`, no inner radius.
fn full_arc() -> serde_json::Value {
    serde_json::json!({
        "startingAngle": 0.0, "endingAngle": std::f64::consts::TAU, "innerRadius": 0.0,
    })
}

fn diagnosed(json: &str) -> Vec<(String, String)> {
    let (_, diagnostics) = lower(
        &serde_json::from_str(json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the ellipse findings are diagnosed, not fatal");
    unsupported(&diagnostics)
}

#[test]
fn an_elliptical_arc_and_ring_are_diagnosed_not_lowered() {
    // A partial sweep is a pie and a non-zero inner radius is a ring; neither
    // has a rounded-rect lowering (docs/decisions/figma-ellipse-as-circle.md).
    // One node, two findings, both in one pass (debt #149).
    let json = ellipse_doc(
        // A half sweep (π) and a 0.5 inner radius: a pie that is also a ring.
        serde_json::json!({
            "startingAngle": 0.0, "endingAngle": std::f64::consts::PI, "innerRadius": 0.5,
        }),
        56.0,
        56.0,
        "FIXED",
        "FIXED",
    );
    assert_eq!(
        diagnosed(&json),
        vec![
            (
                "/root/circle".to_string(),
                "an elliptical arc (partial arcData sweep)".to_string(),
            ),
            (
                "/root/circle".to_string(),
                "a ring (arcData innerRadius)".to_string(),
            ),
        ],
    );
}

#[test]
fn the_circle_gate_tolerates_capture_noise_but_still_refuses_a_real_ellipse() {
    // Both sides of the tolerance boundary. A real capture composes transforms
    // up the tree and reports decimal extents, a sweep of 2π minus a rounding
    // bit, and float noise on innerRadius — the real-file shape #37 targets —
    // so an exact gate would refuse genuine full circles. The toleranced gate
    // lowers a 56.0 × 55.99998 circle whose sweep and inner radius carry noise,
    // and still refuses a 56 × 50 ellipse
    // (docs/decisions/figma-ellipse-as-circle.md).
    let noisy_circle = ellipse_doc(
        serde_json::json!({
            "startingAngle": 0.0,
            "endingAngle": std::f64::consts::TAU - 1e-5,
            "innerRadius": 1.2e-6,
        }),
        56.0,
        55.99998,
        "FIXED",
        "FIXED",
    );
    assert!(
        diagnosed(&noisy_circle).is_empty(),
        "a noisy circle lowers clean: {:?}",
        diagnosed(&noisy_circle),
    );
    let (doc, _) = lower(
        &serde_json::from_str(&noisy_circle).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .unwrap();
    let (_, circle) = node(&doc, "circle");
    assert_eq!(
        circle
            .paint
            .as_ref()
            .expect("a filled circle")
            .entry
            .corners
            .top_left,
        28.0,
        "half the larger 56px extent",
    );

    // 56 × 50 is a genuine ellipse — an 11% relative difference, far past the
    // 0.1% tolerance — so it is refused, not lowered to a stadium.
    let real_ellipse = ellipse_doc(full_arc(), 56.0, 50.0, "FIXED", "FIXED");
    assert_eq!(
        diagnosed(&real_ellipse),
        vec![(
            "/root/circle".to_string(),
            "a non-circular ellipse (unequal extents)".to_string(),
        )],
    );
}

#[test]
fn a_non_fixed_size_ellipse_is_diagnosed() {
    // A FILL/HUG extent is solver output; a static corner radius could not
    // track it (P1), so a non-fixed ellipse is refused. The parent does not
    // hug, so the fill-in-hug refusal does not also fire — the non-fixed
    // finding is the only one.
    let json = ellipse_doc(full_arc(), 56.0, 56.0, "FILL", "FIXED");
    assert_eq!(
        diagnosed(&json),
        vec![(
            "/root/circle".to_string(),
            "a non-fixed-size ellipse".to_string(),
        )],
    );
}

#[test]
fn other_shape_kinds_are_each_diagnosed_by_name() {
    // LINE/VECTOR/STAR/POLYGON/REGULAR_POLYGON have no lowering; each is its
    // own named node-type diagnostic (P4), none silently dropped. Lowering
    // them stays out of scope (docs/decisions/figma-ellipse-as-circle.md).
    let children: Vec<serde_json::Value> = ["LINE", "VECTOR", "STAR", "POLYGON", "REGULAR_POLYGON"]
        .iter()
        .map(|kind| {
            serde_json::json!({
                "name": kind, "type": kind,
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            })
        })
        .collect();
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "shapes", "type": "FRAME",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
                "children": children,
            }],
        }]},
    })
    .to_string();

    assert_eq!(
        diagnosed(&json),
        vec![
            ("/shapes/LINE".to_string(), "node type LINE".to_string()),
            ("/shapes/VECTOR".to_string(), "node type VECTOR".to_string()),
            ("/shapes/STAR".to_string(), "node type STAR".to_string()),
            (
                "/shapes/POLYGON".to_string(),
                "node type POLYGON".to_string()
            ),
            (
                "/shapes/REGULAR_POLYGON".to_string(),
                "node type REGULAR_POLYGON".to_string(),
            ),
        ],
    );
}

#[test]
fn space_between_zeroes_the_authored_gap() {
    // Under SPACE_BETWEEN Figma ignores itemSpacing — the solver owns the
    // spacing — while CSS gap would add to it. The authored value lowers
    // to zero so both solvers agree. Synthetic: no capture carries
    // SPACE_BETWEEN (shape per Figma's REST enum).
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "spread", "type": "FRAME",
                "layoutMode": "HORIZONTAL",
                "primaryAxisAlignItems": "SPACE_BETWEEN",
                "itemSpacing": 40.0,
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 300.0, "height": 50.0 },
                "children": [
                    { "name": "a", "type": "FRAME",
                      "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 50.0, "height": 50.0 } },
                    { "name": "b", "type": "FRAME",
                      "absoluteBoundingBox": { "x": 250.0, "y": 0.0, "width": 50.0, "height": 50.0 } },
                ],
            }],
        }]},
    })
    .to_string();

    let (doc, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the synthetic document lowers");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (_, root) = node(&doc, "spread");
    let container = root.container.expect("an auto-layout frame");
    assert_eq!(container.main_align, MainAxisAlign::SpaceBetween);
    assert_eq!(container.gap, 0.0);
    let (_, b) = node(&doc, "b");
    assert_eq!(b.constraints, None, "no margins under SPACE_BETWEEN");
}

#[test]
fn min_max_clamps_lower_onto_the_constraints() {
    // The clamp fields' shape is pinned by grid-basic.json (`fill-minmax`),
    // but that subtree is grid — refused until v0.8 — so the H/V case is
    // synthetic from the same shape.
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "row", "type": "FRAME",
                "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 300.0, "height": 50.0 },
                "children": [{
                    "name": "clamped", "type": "FRAME",
                    "layoutSizingHorizontal": "FILL",
                    "minWidth": 120.0, "maxWidth": 400.0,
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 50.0 },
                }],
            }],
        }]},
    })
    .to_string();

    let (doc, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the synthetic document lowers");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (_, clamped) = node(&doc, "clamped");
    let constraints = clamped.constraints.expect("carries the clamps");
    assert_eq!(constraints.min_width, Some(120.0));
    assert_eq!(constraints.max_width, Some(400.0));
    assert_eq!(constraints.min_height, None, "absent means unconstrained");
    assert_eq!(constraints.max_height, None);
    assert_eq!(constraints.sizing_h, AxisSizing::Fill);
}

// ---------------------------------------------------------------------------
// Fidelity: the lowered intent, solved by the runtime, lands on the boxes
// Figma's own solver produced (debt #105).
// ---------------------------------------------------------------------------

#[test]
fn the_negative_gap_fixture_solves_to_figmas_captured_rects() {
    // Fixed-size children under a negative gap: the captured
    // absoluteBoundingBox of every child is the oracle. Exact equality —
    // the fixture is integer-dimensioned, per the v0.2 flex-golden rule
    // (docs/decisions/v02-flex-goldens-per-construct.md).
    let rects = solved_rects(&negative_gap_derived());

    let children: [(f32, f32, f32, f32); 5] = [
        (24.0, 24.0, 56.0, 56.0),
        (64.0, 24.0, 56.0, 56.0), // 24 + 56 − 16: the overlap
        (104.0, 24.0, 56.0, 56.0),
        (144.0, 24.0, 56.0, 56.0),
        (184.0, 24.0, 56.0, 56.0),
    ];
    assert_eq!(rects.len(), 1 + children.len());
    for (i, (rect, (x, y, w, h))) in rects[1..].iter().zip(children).enumerate() {
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (x, y, w, h), "child {i}");
    }

    // The root: fixed height 120 and origin hold, and the hug width is
    // Figma's own 264 (5×56 − 4×16 + 2×24). Taffy 0.12's intrinsic
    // sizing mis-sums children with negative margins (debt #236); the
    // engine rebates the negative margin into the flex basis
    // (docs/decisions/negative-margin-hug-rebate.md), so the hug solve
    // of the lowering's margin output lands on the captured value.
    let root = rects[0];
    assert_eq!((root.x, root.y, root.h), (0.0, 0.0, 120.0));
    assert_eq!(root.w, 264.0, "the Figma-captured hug width (debt #236)");
}

#[test]
fn the_hug_in_fill_fixture_solves_to_figmas_captured_rects() {
    // FILL inside fixed, HUG inside FILL, fixed content inside HUG: the
    // whole Figma→CSS sizing lowering in one chain, solved and compared
    // against Figma's own boxes.
    let rects = solved_rects(&hug_in_fill_derived());

    let expected: [(f32, f32, f32, f32); 4] = [
        (0.0, 0.0, 480.0, 85.0),   // root: fixed 480, hug 53+2×16
        (16.0, 16.0, 448.0, 53.0), // fill: 480−2×16 wide, hug 29+2×12
        (28.0, 28.0, 107.0, 29.0), // hug: 87+2×10 by 17+2×6
        (38.0, 34.0, 87.0, 17.0),  // the fixed stand-in for the text
    ];
    assert_eq!(rects.len(), expected.len());
    for (i, (rect, (x, y, w, h))) in rects.iter().zip(expected).enumerate() {
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (x, y, w, h), "rect {i}");
    }
}

#[test]
fn the_solved_flex_fixtures_render_through_the_skia_painter() {
    // The story's acceptance criterion names the painter, so the chain is
    // driven one link further: solved rects in, pixels out.
    for json in [negative_gap_derived(), hug_in_fill_derived()] {
        let (bytes, _) = compile_figma(&json, Profile::Core, &BTreeMap::new()).expect("compiles");
        let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");
        let mut arena = Arena::new();
        load_document(&document, &mut arena);
        arena.open().commit_with(&mut TaffySolver::new());

        let scene = arena.committed();
        let mut painter = SkiaPainter::new(480, 120);
        painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            &GlyphRunTable::new(),
            None,
        );
        assert_eq!(&painter.png_bytes()[1..4], b"PNG");
    }
}

// ---------------------------------------------------------------------------
// What the runtime cannot solve until v0.8 is refused by name, never
// flattened (P4) — wrap, grid, baseline. The roadmap places all three in
// the v0.8 layout-fidelity slice; the schema's enums append there.
// ---------------------------------------------------------------------------

#[test]
fn the_wrap_fixture_is_diagnosed_not_flattened_onto_one_line() {
    let (_, diagnostics) = lowered(WRAP);

    assert_eq!(
        unsupported(&diagnostics),
        vec![(
            "/lowering-wrap".to_string(),
            "wrapping auto-layout (WRAP)".to_string(),
        )],
        "one diagnostic: the subtree under an unlowerable container is skipped",
    );
}

#[test]
fn the_grid_fixture_is_diagnosed_not_flattened() {
    let (doc, diagnostics) = lowered(GRID_BASIC);

    assert_eq!(
        unsupported(&diagnostics),
        vec![(
            "/grid-basic".to_string(),
            "grid auto-layout (GRID)".to_string(),
        )],
    );
    assert!(
        doc.nodes.is_empty(),
        "a grid root lowers nothing: every child box is grid-solver output (P1)",
    );

    // And under R6 it can never emit.
    let err = compile_figma(GRID_BASIC, Profile::Core, &BTreeMap::new())
        .expect_err("a grid document is blocked");
    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };
    assert!(report.has(rule::UNSUPPORTED));
}

#[test]
fn the_baseline_fixture_is_diagnosed_not_realigned() {
    let (_, diagnostics) = lowered(BASELINE);

    assert_eq!(
        unsupported(&diagnostics),
        vec![(
            "/lowering-baseline".to_string(),
            "cross-axis alignment BASELINE".to_string(),
        )],
    );
}

#[test]
fn a_fill_child_on_its_parents_hug_axis_is_diagnosed() {
    // Figma resolves the fill-in-hug cycle from the child's stored size —
    // solver state P1 forbids reading — where a CSS solve derives the hug
    // from content. The two disagree, so the construct is refused rather
    // than solved to a picture Figma never rendered. Pinned by
    // variables-bound.json: both cards are FILL in a hug-width root.
    let (doc, diagnostics) = lowered(VARIABLES_BOUND);

    assert_eq!(
        unsupported(&diagnostics),
        vec![
            (
                "/variables-bound/card-inherits-mode".to_string(),
                "a Fill child on its parent's hug axis (horizontal)".to_string(),
            ),
            (
                "/variables-bound/card-explicit-dark".to_string(),
                "a Fill child on its parent's hug axis (horizontal)".to_string(),
            ),
        ],
    );
    let (_, root) = node(&doc, "variables-bound");
    assert!(root.container.is_some(), "the root itself lowers");
}

#[test]
fn a_fill_child_on_both_hug_axes_reports_each_axis() {
    // One child, two findings — without the axis in the message the two
    // diagnostics would be byte-identical and read as one. Synthetic: no
    // capture doubly hugs over a doubly filling child.
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "hug-both", "type": "FRAME",
                "layoutMode": "HORIZONTAL",
                "layoutSizingHorizontal": "HUG",
                "layoutSizingVertical": "HUG",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
                "children": [{
                    "name": "fill-both", "type": "FRAME",
                    "layoutSizingHorizontal": "FILL",
                    "layoutSizingVertical": "FILL",
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
                }],
            }],
        }]},
    })
    .to_string();

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the findings are diagnosed, not fatal");

    assert_eq!(
        unsupported(&diagnostics),
        vec![
            (
                "/hug-both/fill-both".to_string(),
                "a Fill child on its parent's hug axis (horizontal)".to_string(),
            ),
            (
                "/hug-both/fill-both".to_string(),
                "a Fill child on its parent's hug axis (vertical)".to_string(),
            ),
        ],
    );
}

#[test]
fn an_absolutely_positioned_child_is_diagnosed() {
    // layoutPositioning: ABSOLUTE takes the child out of flow; treating it
    // as in-flow would reflow every sibling after it. Synthetic — no
    // capture carries it (see the REST-shapes technote).
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "row", "type": "FRAME",
                "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
                "children": [{
                    "name": "badge", "type": "FRAME",
                    "layoutPositioning": "ABSOLUTE",
                    "absoluteBoundingBox": { "x": 80.0, "y": -10.0, "width": 30.0, "height": 30.0 },
                }],
            }],
        }]},
    })
    .to_string();

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the synthetic document lowers");

    assert_eq!(
        unsupported(&diagnostics),
        vec![(
            "/row/badge".to_string(),
            "absolute positioning inside auto-layout".to_string(),
        )],
    );
}

// ---------------------------------------------------------------------------
// The walk restructure: every finding survives one pass (debt #149), a
// path names its exact node (debt #150), and depth is a stated limit, not
// serde's (debt #148).
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_survive_an_unsupported_sibling() {
    // The debt #149 scenario: a REJECT-band effect on one node, an
    // unsupported construct on a later sibling. Before the restructure the
    // second finding erased the first; now one pass reports both.
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "root", "type": "FRAME",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
                "children": [
                    { "name": "noisy", "type": "FRAME",
                      "effects": [{ "type": "NOISE", "visible": true }],
                      "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 50.0, "height": 50.0 } },
                    { "name": "label", "type": "TEXT",
                      "absoluteBoundingBox": { "x": 50.0, "y": 0.0, "width": 50.0, "height": 50.0 } },
                ],
            }],
        }]},
    })
    .to_string();

    let err = compile_figma(&json, Profile::Core, &BTreeMap::new())
        .expect_err("both findings are errors");
    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };

    assert!(report.has("profile.noise-or-texture-effect"));
    assert!(report.has(rule::UNSUPPORTED));
    let paths: Vec<&str> = report
        .diagnostics()
        .iter()
        .map(|d| match &d.at {
            Location::Node(at) => at.path.as_str(),
            other => panic!("expected a node location, got {other:?}"),
        })
        .collect();
    assert_eq!(paths, ["/root/noisy", "/root/label"], "document order");
}

#[test]
fn a_node_carrying_two_gaps_reports_both() {
    // One node, two unsupported constructs. Reporting only the first would
    // re-create the fix-one-recompile-find-the-next loop per property.
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "tilted-dashed", "type": "FRAME",
                "rotation": 0.25,
                "complexStrokeProperties": { "strokeType": "BASIC" },
                "strokeDashes": [10.0, 5.0],
                "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            }],
        }]},
    })
    .to_string();

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the findings are diagnosed, not fatal");

    let found = unsupported(&diagnostics);
    assert_eq!(
        found,
        vec![
            ("/tilted-dashed".to_string(), "node rotation".to_string()),
            ("/tilted-dashed".to_string(), "a dashed stroke".to_string()),
        ],
    );
}

#[test]
fn duplicate_sibling_names_get_distinct_paths() {
    // Figma permits duplicate sibling names (debt #150). The path suffixes
    // the Figma node id — the stable, URL-pastable one — or the child
    // position when a synthetic node carries no id. `VECTOR` is a node kind
    // with no lowering, so each child is one "node type" diagnostic — the
    // path is what the test pins (a `TEXT` node would lower since #160).
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "root", "type": "FRAME",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
                "children": [
                    { "id": "1:2", "name": "Frame 1", "type": "VECTOR" },
                    { "id": "1:3", "name": "Frame 1", "type": "VECTOR" },
                    { "name": "Frame 1", "type": "VECTOR" },
                    { "id": "1:5", "name": "unique", "type": "VECTOR" },
                ],
            }],
        }]},
    })
    .to_string();

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the unsupported nodes are diagnosed, not fatal");

    let paths: Vec<String> = unsupported(&diagnostics)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        paths,
        [
            "/root/Frame 1 (1:2)",
            "/root/Frame 1 (1:3)",
            "/root/Frame 1 (#2)",
            "/root/unique", // a unique name stays a bare name
        ],
    );
}

/// A one-page document of `depth` nested frames, as raw JSON text.
fn nested_frames(depth: usize) -> String {
    let mut json = String::from(
        r#"{"document":{"name":"Document","type":"DOCUMENT","children":[{"name":"Page 1","type":"CANVAS","children":["#,
    );
    for i in 0..depth {
        json.push_str(&format!(
            r#"{{"name":"level-{i}","type":"FRAME","absoluteBoundingBox":{{"x":0.0,"y":0.0,"width":100.0,"height":100.0}},"children":["#,
        ));
    }
    json.push_str(&"]}".repeat(depth));
    json.push_str("]}]}}");
    json
}

#[test]
fn nesting_beyond_the_old_serde_limit_compiles() {
    // serde_json's default recursion limit refused the 62nd nested frame
    // (debt #148). 120 is nearly double that and still inside
    // MAX_JSON_DEPTH. A debug-build parse costs roughly 15 KiB of stack per
    // JSON level (see MAX_JSON_DEPTH's doc), so the depth this test needs
    // is spelled out as its own thread rather than borrowed from the test
    // harness's 2 MiB default.
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let (bytes, report) =
                compile_figma(&nested_frames(120), Profile::Core, &BTreeMap::new())
                    .expect("120 nested frames compile");
            assert!(report.is_empty());
            assert!(!bytes.is_empty());
        })
        .expect("the thread spawns")
        .join()
        .expect("the deep compile neither panics nor overflows");
}

#[test]
fn nesting_beyond_the_documented_limit_is_a_named_refusal() {
    // Past MAX_JSON_DEPTH the file is refused by the pre-scan — before any
    // recursive code sees it, so no big stack is needed — with an error
    // that names both depths, instead of serde's opaque "recursion limit
    // exceeded".
    let err = compile_figma(&nested_frames(200), Profile::Core, &BTreeMap::new())
        .expect_err("400-plus JSON levels are past the limit");

    let CompileError::Parse(e) = err else {
        panic!("expected a parse-stage refusal, got {err:?}");
    };
    let message = e.to_string();
    assert!(
        message.contains("the limit is 256") && message.contains("JSON levels"),
        "the error names the limit: {message}",
    );
}

// ---------------------------------------------------------------------------
// The emitted bytes are pinned, next to goldens/dsb/v03-paint.dsb.
// ---------------------------------------------------------------------------

#[test]
fn the_derived_flex_fixtures_emit_their_golden_dsbs() {
    // Same contract as `the_fixture_emits_the_golden_dsb` in
    // figma_lowering.rs: regenerate with UPDATE_GOLDENS=1, review, commit —
    // a missing golden fails rather than minting its own truth
    // (goldens/README.md).
    for (name, json) in [
        ("v07-negative-gap-derived.dsb", negative_gap_derived()),
        ("v07-hug-in-fill-derived.dsb", hug_in_fill_derived()),
    ] {
        let (bytes, report) =
            compile_figma(&json, Profile::Core, &BTreeMap::new()).expect("compiles");
        assert!(report.is_empty(), "{name}: {report}");

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../goldens/dsb")
            .join(name);

        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::create_dir_all(path.parent().expect("the golden has a parent"))
                .expect("the goldens directory is writable");
            std::fs::write(&path, &bytes).expect("the golden is writable");
            continue;
        }

        let golden = std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test flex_lowering` to create it",
                path.display(),
            )
        });
        assert_eq!(
            bytes, golden,
            "{name} drifted. If this is intended, regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
        );
    }
}

#[test]
fn the_negative_gap_fixture_emits_its_golden_dsb() {
    // Since #239 the raw capture emits — its five ellipses lower to circles
    // (corners = 28) — so it is pinned directly, distinct from the derived
    // (frames) golden the Deno suite byte-compares. Same golden contract:
    // regenerate with UPDATE_GOLDENS=1, review, commit (goldens/README.md).
    let (bytes, report) = compile_figma(NEGATIVE_GAP, Profile::Core, &BTreeMap::new())
        .expect("the raw capture compiles");
    assert!(report.is_empty(), "{report}");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens/dsb/v07-negative-gap.dsb");

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden has a parent"))
            .expect("the goldens directory is writable");
        std::fs::write(&path, &bytes).expect("the golden is writable");
        return;
    }

    let golden = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test flex_lowering` to create it",
            path.display(),
        )
    });
    assert_eq!(
        bytes, golden,
        "v07-negative-gap.dsb drifted. If this is intended, regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
    );
}
