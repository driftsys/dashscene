//! The render gate's order fixture, re-derived from its source.
//!
//! `unity/render-gate/order.json` is the document `just unity-render` draws to
//! ask whether the Unity painter composites in the painter's order — issue
//! #1402, and the fixture `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
//! asks for: a full-bleed backdrop, a glyph run over an opaque fill, a
//! half-alpha node over both, and a second run in a second atlas packed after
//! it, so the painter's order gives a composite no other permutation gives.
//! `order.dsb` beside it is what the gate loads, and a committed binary nobody
//! can re-derive is a golden with no explanation. This test is the derivation:
//! the same `compile_figma` that pins `goldens/dsb/`, over the same profile.
//!
//! Regenerate with `UPDATE_GOLDENS=1`, review the diff, commit — the contract
//! `crates/dashc/tests/text_lowering.rs` states for the text goldens.

use std::collections::BTreeMap;

const SOURCE: &str = "unity/render-gate/order.json";
const COMPILED: &str = "unity/render-gate/order.dsb";

/// `hint` is what a missing file means: the compiled `.dsb` can be created,
/// the hand-written source cannot.
fn read(relative: &str, hint: &str) -> Vec<u8> {
    let path = package_gate::root().join(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}\n{hint}", path.display()))
}

const SOURCE_HINT: &str = "order.json is hand-written; nothing regenerates it";
const COMPILED_HINT: &str =
    "run `UPDATE_GOLDENS=1 cargo test -p package-gate --test order_fixture` to create it";

/// The committed `.dsb` is what the source compiles to, with no diagnostic.
#[test]
fn the_order_fixture_is_what_its_source_compiles_to() {
    let json = String::from_utf8(read(SOURCE, SOURCE_HINT)).expect("order.json is UTF-8");
    let (bytes, report) =
        dashc_wasm::compile_figma(&json, dashscene_validator::Profile::Core, &BTreeMap::new())
            .unwrap_or_else(|e| panic!("{SOURCE} does not compile: {e:?}"));
    assert!(
        report.diagnostics().is_empty(),
        "{SOURCE} compiles with diagnostics, and the render gate cannot tell a \
         refused node from a drawn one:\n{report}"
    );

    let path = package_gate::root().join(COMPILED);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&path, &bytes).expect("order.dsb is writable");
        return;
    }
    let committed = read(COMPILED, COMPILED_HINT);
    assert!(
        bytes == committed,
        "{COMPILED} is not what {SOURCE} compiles to. If the change is meant, \
         regenerate with UPDATE_GOLDENS=1, review the diff, and commit."
    );
}

/// The source carries the five nodes, in the paint order the gate's probes
/// are written against, with the colours those probes discriminate on.
///
/// **Held here as well as in the gate**, because the gate reads the packed
/// instances back and a document that drifted would move its probes with it —
/// a fixture whose veil became opaque would leave every probe satisfied by a
/// different order, and a veil whose left edge moved off x = 420 would put the
/// gate's classifier and probe points on the wrong pixels. The gate's C# is
/// compiled by no CI job; this is, so the boxes and the glyph strings the
/// C# hard-codes against are pinned here too.
#[test]
fn the_order_fixture_keeps_its_five_nodes_and_their_colours() {
    let json: serde_json::Value =
        serde_json::from_slice(&read(SOURCE, SOURCE_HINT)).expect("order.json parses");
    let root = &json["document"]["children"][0]["children"][0];
    assert_eq!(root["name"], "order");
    let box_ = &root["absoluteBoundingBox"];
    assert_eq!(
        (
            box_["x"].as_f64(),
            box_["y"].as_f64(),
            box_["width"].as_f64(),
            box_["height"].as_f64()
        ),
        (Some(0.0), Some(0.0), Some(960.0), Some(680.0)),
        "the backdrop must be the 960x680 extent at the origin the render gate's camera frames and its classifier matches"
    );

    let children = root["children"].as_array().expect("the root has children");
    let names: Vec<&str> = children
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["fill", "regular", "veil", "bold"],
        "the paint order is the probe order: an opaque fill, black glyphs over \
         it, a half-alpha veil over both, white bold glyphs over the veil"
    );

    fn colour(node: &serde_json::Value) -> (f64, f64, f64, f64) {
        let c = &node["fills"][0]["color"];
        (
            c["r"].as_f64().unwrap(),
            c["g"].as_f64().unwrap(),
            c["b"].as_f64().unwrap(),
            c["a"].as_f64().unwrap(),
        )
    }
    // Every opaque channel is 0 or 1, so the gate's predicates hold under any
    // monotone colour transfer that fixes both ends — the gate does not model
    // the pipeline's colour handling and must not have to.
    assert_eq!(colour(root), (0.0, 0.0, 1.0, 1.0), "backdrop: pure blue");
    assert_eq!(
        colour(&children[0]),
        (1.0, 1.0, 0.0, 1.0),
        "fill: pure yellow"
    );
    assert_eq!(
        colour(&children[1]),
        (0.0, 0.0, 0.0, 1.0),
        "regular glyphs: black"
    );
    assert_eq!(
        colour(&children[2]),
        (1.0, 0.0, 0.0, 0.5),
        "veil: red at half alpha"
    );
    assert_eq!(
        colour(&children[3]),
        (1.0, 1.0, 1.0, 1.0),
        "bold glyphs: white"
    );
    fn bbox(node: &serde_json::Value) -> (f64, f64, f64, f64) {
        let b = &node["absoluteBoundingBox"];
        (
            b["x"].as_f64().unwrap(),
            b["y"].as_f64().unwrap(),
            b["width"].as_f64().unwrap(),
            b["height"].as_f64().unwrap(),
        )
    }
    // The gate classifies the fill by this box, splits the two runs at
    // x = 690, judges the veil's left edge at x = 420, and reads its four fixed
    // probes at (80, 600), (180, 400), (800, 400) and (560, 400) — every one of
    // them written against these boxes.
    assert_eq!(
        bbox(&children[0]),
        (160.0, 160.0, 480.0, 260.0),
        "fill: the box the gate classifies by"
    );
    assert_eq!(
        bbox(&children[1]),
        (200.0, 200.0, 400.0, 146.0),
        "regular run: left of x = 690, straddling the veil's edge"
    );
    assert_eq!(
        bbox(&children[2]),
        (420.0, 60.0, 480.0, 440.0),
        "veil: its left edge at x = 420 is what the gate splits the regular glyphs on"
    );
    assert_eq!(
        bbox(&children[3]),
        (700.0, 150.0, 80.0, 146.0),
        "bold run: right of x = 690, over the veil"
    );
    assert_eq!(
        children[1]["characters"], "IIIIIIIIIIII",
        "twelve regular glyphs, so one straddles the veil's edge and the rest probe"
    );
    assert_eq!(children[3]["characters"], "II", "two bold glyphs");
    assert_eq!(children[1]["style"]["fontWeight"], 400);
    assert_eq!(
        children[3]["style"]["fontWeight"], 700,
        "the second run is in a second atlas"
    );
}
