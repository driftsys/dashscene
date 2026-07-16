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
//! Two fixtures carry constructs other stories own (`TEXT` is #160; the
//! `ELLIPSE` shape has no story yet), so the solve tests run on *derived*
//! documents — the captured JSON with exactly the out-of-scope node kind
//! swapped for a fixed-size `FRAME`, sized to the box Figma gave the
//! original, and nothing else changed. The raw captures are pinned
//! separately: each out-of-scope construct is a named diagnostic.

use std::collections::BTreeMap;

use dashc_wasm::figma::{CompileError, lower, rule};
use dashc_wasm::{AxisSizing, LayoutMode, MainAxisAlign, compile_figma};
use dashpaint::{GlyphRunTable, Painter};
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
/// `FRAME`s. Shape nodes are out of this story's scope, and a frame with the
/// same fixed size solves identically, so Figma's captured boxes stay the
/// oracle.
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
fn the_negative_gap_fixtures_ellipses_are_diagnosed_not_dropped() {
    // The raw capture: shape nodes have no story yet, so each is a named
    // diagnostic (P4) and the fixture cannot emit until a story lowers them.
    let (_, diagnostics) = lowered(NEGATIVE_GAP);

    let found = unsupported(&diagnostics);
    assert_eq!(found.len(), 5, "{found:?}");
    for (i, (path, what)) in found.iter().enumerate() {
        assert_eq!(what, "node type ELLIPSE");
        assert_eq!(
            path,
            &format!("/lowering-negative-gap/overlap-{}", i + 1),
            "diagnostics come in document order",
        );
    }
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

    // The root: fixed height 120 and origin hold; the hug width does not.
    // Figma solved it to 264 (5×56 − 4×16 + 2×24), but Taffy 0.12's
    // intrinsic sizing mis-sums children with negative margins, an engine
    // gap filed as debt #236 — the lowering's output is correct (the
    // margins are exactly the #10 margin-equivalent scene), the hug solve
    // of it is not. The wrong value is pinned so the engine fix is loud
    // here: closing #236 means flipping this assertion to 264.
    let root = rects[0];
    assert_eq!((root.x, root.y, root.h), (0.0, 0.0, 120.0));
    assert_eq!(
        root.w, 48.0,
        "debt #236: must become 264 once the engine hug-sizes negative margins"
    );
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
