//! Component lowering (story #242): local `INSTANCE` resolution, `COMPONENT`/
//! `COMPONENT_SET` definitions that resolve but do not paint, the declared
//! multi-root lift, and the #147 root-selection remainder.
//!
//!     Figma REST JSON → lower → Document → emit → validate → .dsb
//!
//! Figma serializes an `INSTANCE` with its resolved subtree baked in — the
//! referenced component's content with the instance's overrides already applied
//! — so an instance lowers like a frame: its fill, its container intent, and its
//! baked children all go through the ordinary walk, and an out-of-vocabulary
//! override on one of them is a named diagnostic (P4) exactly as it would be on
//! any node. A `COMPONENT` or `COMPONENT_SET` is a definition: this story lowers
//! the authored state, so a definition resolves (the walk accepts it) but does
//! not paint as document content — the v0.4 variant table that would carry the
//! alternative members is consumer-side and out of scope
//! (`docs/decisions/figma-component-lowering.md`).

use std::collections::BTreeMap;

use dashc_wasm::figma::rest::FigmaFile;
use dashc_wasm::figma::{CompileError, lower};
use dashc_wasm::{Document, LayoutMode, compile_figma};
use dashscene_validator::{Diagnostic, Profile, Severity};

mod common;
use common::{node, parse, unsupported};

const VARIANT_TOPOLOGY: &str =
    include_str!("../../../corpus/figma-fixtures/lowering-variant-topology.json");

/// A one-page document whose canvas holds `top` (an array of top-level nodes).
fn document_with(top: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "document": {
            "name": "Document",
            "type": "DOCUMENT",
            "children": [{ "name": "Page 1", "type": "CANVAS", "children": top }],
        },
    })
}

/// Lowers a synthetic document, asserting only that the walk did not abort.
fn lower_json(value: serde_json::Value) -> (Document, Vec<Diagnostic>) {
    let file: FigmaFile = serde_json::from_value(value).expect("the synthetic document parses");
    lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers")
}

/// The lowered node names, in document (DFS) order.
fn names(doc: &Document) -> Vec<String> {
    doc.nodes.iter().filter_map(|n| n.name.clone()).collect()
}

#[test]
fn an_instance_lowers_its_baked_subtree_like_a_frame() {
    // Figma bakes the component's content into the instance's children, so an
    // instance lowers like a frame: its fill, its container intent, and its
    // baked children all lower. As a root the instance drops its page position
    // (P1); the fixed width lowers, the hug height lowers as zero (solver-owned).
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([{
        "id": "1:12",
        "name": "chip-instance",
        "type": "INSTANCE",
        "componentId": "1:2",
        "layoutMode": "VERTICAL",
        "layoutSizingHorizontal": "FIXED",
        "layoutSizingVertical": "HUG",
        "absoluteBoundingBox": { "x": 40.0, "y": 200.0, "width": 100.0, "height": 85.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 0.96, "g": 0.96, "b": 0.96, "a": 1.0 } }],
        "children": [{
            "id": "I1:12;1:4",
            "name": "row-1",
            "type": "FRAME",
            "layoutSizingHorizontal": "FIXED",
            "layoutSizingVertical": "FIXED",
            "absoluteBoundingBox": { "x": 56.0, "y": 241.0, "width": 80.0, "height": 28.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.7, "g": 0.75, "b": 0.9, "a": 1.0 } }],
        }],
    }])));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (index, instance) = node(&doc, "chip-instance");
    assert_eq!(index, 0, "the instance is the first rect-table entry");
    assert_eq!(instance.parent, None);
    assert_eq!((instance.box2d.x, instance.box2d.y), (0.0, 0.0));
    assert_eq!(instance.box2d.width, 100.0);
    let container = instance
        .container
        .expect("an instance lowers its container intent");
    assert_eq!(container.mode, LayoutMode::Vertical);
    assert!(instance.paint.is_some(), "the instance fill lowers");

    let (_, row) = node(&doc, "row-1");
    assert_eq!(
        row.parent,
        Some(index),
        "the baked child lowers under the instance",
    );
}

#[test]
fn top_level_component_and_set_definitions_resolve_but_do_not_paint() {
    // A COMPONENT_SET (and its COMPONENT members) is a definition: it resolves
    // — the walk accepts it — but does not paint, and it is not a diagnostic.
    // Its dashed stroke never reaches the paint gate: the definition is skipped
    // whole. Only the instance lowers.
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:11", "name": "chip-set", "type": "COMPONENT_SET",
            "strokeDashes": [10.0, 5.0],
            "children": [
                { "id": "1:2", "name": "state=collapsed", "type": "COMPONENT",
                  "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 85.0 } },
            ],
        },
        {
            "id": "1:12", "name": "chip-instance", "type": "INSTANCE", "componentId": "1:2",
            "absoluteBoundingBox": { "x": 0.0, "y": 200.0, "width": 100.0, "height": 85.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.96, "g": 0.96, "b": 0.96, "a": 1.0 } }],
        },
    ])));

    assert!(
        diagnostics.is_empty(),
        "a definition is not a diagnostic: {diagnostics:?}",
    );
    assert_eq!(names(&doc), vec!["chip-instance"]);
}

#[test]
fn a_definitions_only_document_is_refused_by_name() {
    // A canvas holding only definitions resolves nothing that paints, so the
    // walk produces zero content nodes. That is refused with a named diagnostic
    // (P4), not lowered as a silent zero-node document — a downstream consumer
    // panics loading a scene with no roots.
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([{
        "id": "1:11", "name": "chip-set", "type": "COMPONENT_SET",
        "strokeDashes": [10.0, 5.0],
        "children": [
            { "id": "1:2", "name": "state=collapsed", "type": "COMPONENT",
              "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 85.0 } },
        ],
    }])));

    assert!(doc.nodes.is_empty(), "no content node lowered");
    let found: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.no-content")
        .collect();
    assert_eq!(found.len(), 1, "{diagnostics:?}");
    assert_eq!(found[0].severity, Severity::Error);
    // The message names what was skipped (the definition) and why.
    assert!(
        found[0].message.contains("chip-set"),
        "{}",
        found[0].message
    );
    assert!(
        found[0].message.contains("COMPONENT_SET"),
        "{}",
        found[0].message,
    );
}

#[test]
fn a_definitions_only_document_does_not_emit() {
    // compile_figma must refuse it (R6), never return Ok with a zero-node .dsb.
    let json = document_with(serde_json::json!([{
        "id": "1:11", "name": "chip-set", "type": "COMPONENT_SET",
        "children": [
            { "id": "1:2", "name": "state=collapsed", "type": "COMPONENT",
              "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 85.0 } },
        ],
    }]))
    .to_string();

    let err = compile_figma(&json, Profile::Core, &BTreeMap::new())
        .expect_err("a definitions-only document must not emit");
    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };
    assert!(report.has("figma.no-content"));
}

#[test]
fn a_component_set_nested_in_a_paint_root_is_skipped() {
    // The set can live inside a declared root (the closure keeps it there). It
    // is still a definition: skipped whole, so its members never paint, while
    // the frame and the instance beside it lower.
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([{
        "id": "1:20", "name": "home", "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 200.0 },
        "children": [
            {
                "id": "1:11", "name": "chip-set", "type": "COMPONENT_SET",
                "children": [
                    { "id": "1:2", "name": "state=collapsed", "type": "COMPONENT",
                      "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 85.0 } },
                ],
            },
            {
                "id": "1:21", "name": "chip-instance", "type": "INSTANCE", "componentId": "1:2",
                "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 100.0, "height": 85.0 },
                "fills": [{ "type": "SOLID", "color": { "r": 0.9, "g": 0.9, "b": 0.9, "a": 1.0 } }],
            },
        ],
    }])));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        names(&doc),
        vec!["home", "chip-instance"],
        "the set and its members do not paint",
    );
}

#[test]
fn an_out_of_vocabulary_instance_override_is_a_named_diagnostic() {
    // An instance override the vocabulary cannot carry is baked into the
    // instance's subtree by Figma, so it goes through the ordinary walk and is
    // named there (P4) — here a per-instance rotation override on a baked child.
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([{
        "id": "1:12", "name": "chip-instance", "type": "INSTANCE", "componentId": "1:2",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 85.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 0.96, "g": 0.96, "b": 0.96, "a": 1.0 } }],
        "children": [{
            "id": "I1:12;1:4", "name": "row-1", "type": "FRAME",
            "rotation": 0.25,
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 80.0, "height": 28.0 },
        }],
    }])));

    let found = unsupported(&diagnostics);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, "node rotation");
    assert!(found[0].0.contains("row-1"), "{}", found[0].0);
    assert!(
        !doc.nodes.iter().any(|n| n.name.as_deref() == Some("row-1")),
        "the overridden child is skipped, never lowered as though it were upright",
    );
}

#[test]
fn multiple_top_level_frames_lower_as_independent_roots() {
    // The declared-roots closure computes multi-root exports; the walk lifts
    // them (docs/decisions/figma-component-lowering.md). Two top-level frames
    // become two document roots, each with its page position dropped to its own
    // origin (P1). This is also the #147 remainder: the second frame is no
    // longer silently dropped by a positional first-frame selection.
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:1", "name": "home", "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        },
        {
            "id": "2:1", "name": "settings", "type": "FRAME",
            "absoluteBoundingBox": { "x": 400.0, "y": 0.0, "width": 120.0, "height": 90.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0 } }],
        },
    ])));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (i0, home) = node(&doc, "home");
    let (i1, settings) = node(&doc, "settings");
    assert_eq!((i0, i1), (0, 1), "both frames lower, in document order");
    assert_eq!(home.parent, None, "the first frame is a root");
    assert_eq!(
        settings.parent, None,
        "the second frame is also a root, not dropped",
    );
    assert_eq!(
        (settings.box2d.x, settings.box2d.y),
        (0.0, 0.0),
        "each root drops its own page position",
    );
    assert_eq!((settings.box2d.width, settings.box2d.height), (120.0, 90.0));
}

#[test]
fn an_empty_document_is_still_refused() {
    // The structural refusal survives the multi-root lift: a document with no
    // top-level node under any canvas has nothing to lower.
    let file: FigmaFile = serde_json::from_value(serde_json::json!({
        "document": { "id": "0:0", "name": "Document", "type": "DOCUMENT", "children": [] }
    }))
    .expect("the synthetic document parses");

    assert!(matches!(
        lower(&file, Profile::Core, &BTreeMap::new()),
        Err(CompileError::Unsupported { .. }),
    ));
}

#[test]
fn the_variant_topology_fixture_compiles_clean() {
    // The raw capture: a COMPONENT_SET (with a dashed stroke), two COMPONENT
    // members of different child counts, and one INSTANCE. The set resolves but
    // does not paint, so the dashed stroke never reaches the paint gate; only
    // the instance's baked (collapsed) subtree lowers, and the fixture emits.
    let (bytes, report) = compile_figma(VARIANT_TOPOLOGY, Profile::Core, &BTreeMap::new())
        .expect("the variant-topology fixture compiles since #242");
    assert!(
        report.is_empty(),
        "the component fixture lowers clean: {report}"
    );
    assert!(!bytes.is_empty());

    let (doc, _) = lower(&parse(VARIANT_TOPOLOGY), Profile::Core, &BTreeMap::new()).unwrap();
    assert_eq!(
        names(&doc),
        vec!["instance-collapsed", "state: collapsed", "row-1"],
        "only the instance's authored subtree paints; the set and its members do not",
    );
}

#[test]
fn the_variant_topology_fixture_emits_the_golden_dsb() {
    // The raw component capture pins its own golden .dsb, the same contract the
    // other raw fixtures use: regenerate with UPDATE_GOLDENS=1, review the diff,
    // and commit (goldens/README.md).
    let (bytes, report) = compile_figma(VARIANT_TOPOLOGY, Profile::Core, &BTreeMap::new())
        .expect("the variant-topology fixture compiles");
    assert!(report.is_empty(), "{report}");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens/dsb/v07-variant-topology.dsb");

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden has a parent"))
            .expect("the goldens directory is writable");
        std::fs::write(&path, &bytes).expect("the golden is writable");
        return;
    }

    let golden = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test component_lowering` to create it",
            path.display(),
        )
    });
    assert_eq!(
        bytes, golden,
        "v07-variant-topology.dsb drifted. If this is intended, regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
    );
}
