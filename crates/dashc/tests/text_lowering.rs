//! Figma `TEXT` lowering, end to end (story #160):
//!
//!     Figma TEXT intent → lower → Document text vocabulary → emit
//!         → .dsb → dashscene-core (strings + text-style pools)
//!
//! The document carries authored intent only (P1): the characters and the
//! style (family, em size, CSS-scale weight, fill color) — never Figma's
//! rendered line breaks, glyph positions, or `absoluteRenderBounds`. The
//! vocabulary is the four axes `dashbuf`'s `TextStyle` table carries (story
//! #26); every other authored text feature — a non-default alignment, line
//! height, letter spacing, decoration, case transform, italic, hyperlink,
//! OpenType flag, or multiple style segments — has nothing to lower into and
//! is a named diagnostic (P4), never dropped in silence.
//!
//! `lowering-hug-in-fill.json` compiles raw since this story (its only
//! refusal was the TEXT leaf). `lowering-baseline.json` — the designated text
//! input — is still refused raw by its root's `BASELINE` cross-axis alignment
//! (v0.8, Q-4), so its text is exercised through a declared derivation that
//! lifts only that one refusal (`goldens/dsb/README.md`).

use std::collections::BTreeMap;

use dashc_wasm::figma::lower;
use dashc_wasm::{AxisSizing, compile_figma};
use dashpaint::Color;
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_validator::{Diagnostic, Profile};

mod common;
use common::{derive, node, parse, unsupported};

const HUG_IN_FILL: &str = include_str!("../../../corpus/figma-fixtures/lowering-hug-in-fill.json");
const BASELINE: &str = include_str!("../../../corpus/figma-fixtures/lowering-baseline.json");

fn lowered(json: &str) -> (dashc_wasm::Document, Vec<Diagnostic>) {
    lower(&parse(json), Profile::Core, &BTreeMap::new()).expect("the fixture lowers")
}

/// `lowering-baseline.json` with only its root's `BASELINE` cross-axis
/// alignment lifted to `MIN`, so the subtree — the mixed-size Latin rows and
/// the Arabic RTL run — lowers. `BASELINE` itself is v0.8 layout-fidelity
/// vocabulary (Q-4), out of this story's scope; the text under it is not.
fn baseline_text_derived() -> String {
    derive(
        BASELINE,
        |object| object.get("counterAxisAlignItems").and_then(|v| v.as_str()) == Some("BASELINE"),
        |object| {
            object.insert("counterAxisAlignItems".to_string(), "MIN".into());
        },
    )
}

/// The captures' text ink: `{0.1, 0.1, 0.1, 1.0}`. The `0.1` f32 literal is
/// the exact bit pattern the fixture JSON (`0.10000000149011612`) parses to.
const INK: Color = Color {
    r: 0.1,
    g: 0.1,
    b: 0.1,
    a: 1.0,
};

/// The lowered node in `arena` whose text equals `want`, found by walking the
/// committed tree (the arena exposes roots and children, not a flat list).
fn find_text(arena: &Arena, want: &str) -> NodeId {
    fn search(arena: &Arena, node: NodeId, want: &str) -> Option<NodeId> {
        if arena.text(node) == Some(want) {
            return Some(node);
        }
        arena
            .children(node)
            .iter()
            .find_map(|&child| search(arena, child, want))
    }
    arena
        .roots()
        .iter()
        .find_map(|&root| search(arena, root, want))
        .unwrap_or_else(|| panic!("no arena node carries the text {want:?}"))
}

// ---------------------------------------------------------------------------
// The authored characters and style lower — and only the intent (P1).
// ---------------------------------------------------------------------------

#[test]
fn a_hug_text_leaf_lowers_its_characters_and_style() {
    // The hug-in-fill fixture's leaf is a HUG/HUG text node. Since #160 the
    // raw fixture lowers with no unsupported construct.
    let (doc, diagnostics) = lowered(HUG_IN_FILL);
    assert!(
        unsupported(&diagnostics).is_empty(),
        "{:?}",
        unsupported(&diagnostics),
    );

    let (_, text) = node(&doc, "hug inside fill");
    assert_eq!(text.text.as_deref(), Some("hug inside fill"));
    let style = text.text_style.as_ref().expect("the leaf carries a style");
    assert_eq!(style.family, "Inter");
    assert_eq!(style.size, 14.0);
    assert_eq!(style.weight, 400);
    assert_eq!(
        style.color, INK,
        "the SOLID fill lowered into the glyph color"
    );

    // A text node's fill is its glyph color, not a rect fill: no paint entry.
    assert!(text.paint.is_none(), "text carries no paint entry");

    // HUG on both axes flows to the engine's measure seam (#29); nothing but
    // the sizing intent is lowered, so the box extent stays zero (P1).
    let constraints = text.constraints.expect("the leaf carries its sizing");
    assert_eq!(constraints.sizing_h, AxisSizing::Hug);
    assert_eq!(constraints.sizing_v, AxisSizing::Hug);
    assert_eq!((text.box2d.width, text.box2d.height), (0.0, 0.0));
}

#[test]
fn the_arabic_rtl_run_lowers_with_its_authored_codepoints() {
    // The designated text input carries an Arabic RTL run with Arabic-Indic
    // numerals. The document carries the authored codepoints verbatim — never
    // the shaped forms or a digit substitution (both are the runtime's
    // resolved results, P1).
    let (doc, diagnostics) = lower(
        &parse(&baseline_text_derived()),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the derived fixture lowers");
    assert!(
        unsupported(&diagnostics).is_empty(),
        "with BASELINE lifted the text lowers clean: {:?}",
        unsupported(&diagnostics),
    );

    let (_, arabic) = node(&doc, "arabic-rtl");
    assert_eq!(arabic.text.as_deref(), Some("السرعة ١٢٠ كم/س"));
    // The authored Arabic-Indic digits U+0661 U+0662 U+0660 are carried as
    // authored, not normalised to European.
    assert!(arabic.text.as_deref().unwrap().contains('\u{0661}'));
    let style = arabic.text_style.as_ref().expect("carries a style");
    assert_eq!(style.family, "Noto Sans Arabic");
    assert_eq!(style.size, 24.0);
    assert_eq!(style.weight, 400);

    // The mixed-size Latin rows lower alongside it, each with its own size and
    // the bold row's weight.
    assert_eq!(
        node(&doc, "small").1.text_style.as_ref().unwrap().size,
        12.0
    );
    let medium = node(&doc, "MEDIUM").1.text_style.as_ref().unwrap();
    assert_eq!((medium.size, medium.weight), (24.0, 700));
    assert_eq!(
        node(&doc, "Large").1.text_style.as_ref().unwrap().size,
        40.0
    );
}

#[test]
fn free_standing_text_takes_its_sizing_from_text_auto_resize() {
    // A TEXT node outside auto-layout carries no layoutSizing* (Figma sets it
    // only in an auto-layout context). textAutoResize is then the sizing
    // source: WIDTH_AND_HEIGHT must hug both axes, not fix-size from the
    // resolved box. For a free-standing node the absoluteBoundingBox is
    // authored (the designer placed and sized it), so a Fixed axis reads it as
    // intent (P1); a Hug axis carries no extent.
    for (auto_resize, sh, sv, width_is_fixed, height_is_fixed) in [
        (
            "WIDTH_AND_HEIGHT",
            AxisSizing::Hug,
            AxisSizing::Hug,
            false,
            false,
        ),
        ("HEIGHT", AxisSizing::Fixed, AxisSizing::Hug, true, false),
        ("NONE", AxisSizing::Fixed, AxisSizing::Fixed, true, true),
    ] {
        let mut style = base_style();
        style
            .as_object_mut()
            .unwrap()
            .insert("textAutoResize".to_string(), auto_resize.into());
        // No layoutSizing* fields — free-standing, under a mode-None parent.
        let leaf = serde_json::json!({
            "name": "label", "type": "TEXT", "characters": "hi", "style": style,
            "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 120.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0 } }],
        });
        let json = serde_json::json!({
            "document": { "name": "Document", "type": "DOCUMENT", "children": [{
                "name": "Page 1", "type": "CANVAS", "children": [{
                    "name": "root", "type": "FRAME",
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 100.0 },
                    "children": [leaf],
                }],
            }]},
        })
        .to_string();

        let (doc, diagnostics) = lower(
            &serde_json::from_str(&json).unwrap(),
            Profile::Core,
            &BTreeMap::new(),
        )
        .expect("the free-standing text lowers");
        assert!(
            unsupported(&diagnostics).is_empty(),
            "{auto_resize}: {:?}",
            unsupported(&diagnostics),
        );
        let (_, text) = node(&doc, "label");
        let c = text.constraints.unwrap_or_default();
        assert_eq!(c.sizing_h, sh, "{auto_resize} horizontal sizing");
        assert_eq!(c.sizing_v, sv, "{auto_resize} vertical sizing");
        // A Fixed axis reads the authored box; a Hug axis carries no extent.
        assert_eq!(
            text.box2d.width == 120.0,
            width_is_fixed,
            "{auto_resize} width extent",
        );
        assert_eq!(
            text.box2d.height == 40.0,
            height_is_fixed,
            "{auto_resize} height extent",
        );
    }
}

#[test]
fn layout_sizing_wins_when_it_and_text_auto_resize_disagree() {
    // In an auto-layout context both signals are present; Figma keeps them
    // consistent, so a disagreement is stale input. The modern layoutSizing*
    // pair is authoritative — it is what Figma's layout engine renders (the
    // #140 D1 convention) — so it wins; textAutoResize is consulted only for
    // TRUNCATE and only when layoutSizing is absent. Here layoutSizing hugs
    // both axes while textAutoResize says NONE (fixed): the hug wins.
    let mut text = text_json("t", "hi", 16.0, 400); // sets layoutSizing HUG/HUG
    text["style"]["textAutoResize"] = "NONE".into();
    let json = wrap_single(text);

    let (doc, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("lowers");
    assert!(
        unsupported(&diagnostics).is_empty(),
        "{:?}",
        unsupported(&diagnostics),
    );
    let (_, t) = node(&doc, "t");
    let c = t.constraints.expect("carries the layoutSizing intent");
    assert_eq!(
        c.sizing_h,
        AxisSizing::Hug,
        "layoutSizing HUG wins over NONE"
    );
    assert_eq!(c.sizing_v, AxisSizing::Hug);
}

// ---------------------------------------------------------------------------
// The strings and styles round-trip through dashscene-core (the acceptance
// criterion: "round-trip through dashscene-core").
// ---------------------------------------------------------------------------

#[test]
fn text_and_style_round_trip_through_dashscene_core() {
    let (bytes, report) =
        compile_figma(HUG_IN_FILL, Profile::Core, &BTreeMap::new()).expect("compiles");
    assert!(report.is_empty(), "{report}");
    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");

    let mut arena = Arena::new();
    load_document(&document, &mut arena);

    // The one text node round-trips through the strings and text-style pools
    // into the arena's text accessors — the seam the measure callback reads.
    let leaf = find_text(&arena, "hug inside fill");
    let style = arena.text_style(leaf).expect("the style reached the arena");
    assert_eq!(style.family, "Inter");
    assert_eq!(style.size, 14.0);
    assert_eq!(style.weight, 400);
    assert_eq!(style.color.r, INK.r);
}

#[test]
fn nodes_sharing_text_or_style_dedup_to_one_pool_entry() {
    // Two text nodes with the same string and style share one string and one
    // style pool entry — the producer's dedup job (docs/design/dashbuf.md),
    // proven through the value, not vacuous index equality.
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "row", "type": "FRAME", "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 40.0 },
                "children": [
                    text_json("a", "OK", 16.0, 400),
                    text_json("b", "OK", 16.0, 400),
                ],
            }],
        }]},
    })
    .to_string();

    let (bytes, report) = compile_figma(&json, Profile::Core, &BTreeMap::new()).expect("compiles");
    assert!(report.is_empty(), "{report}");
    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");
    assert_eq!(document.strings().expect("a string pool").len(), 1);
    assert_eq!(document.text_styles().expect("a style pool").len(), 1);
}

#[test]
fn two_styles_differing_only_in_alignment_are_two_pool_entries() {
    // The pool dedup key must include the four widened axes (story #310):
    // two text nodes identical but for alignment must not collapse to one
    // style entry, which would render one of them with the wrong alignment.
    let a = {
        let mut t = text_json("a", "OK", 16.0, 400);
        t["style"]["textAlignHorizontal"] = "CENTER".into();
        t
    };
    let b = {
        let mut t = text_json("b", "OK", 16.0, 400);
        t["style"]["textAlignHorizontal"] = "LEFT".into();
        t
    };
    let json = serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "row", "type": "FRAME", "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 40.0 },
                "children": [a, b],
            }],
        }]},
    })
    .to_string();

    let (bytes, report) = compile_figma(&json, Profile::Core, &BTreeMap::new()).expect("compiles");
    assert!(report.is_empty(), "{report}");
    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");
    assert_eq!(
        document.strings().expect("a string pool").len(),
        1,
        "the shared string still dedups to one entry"
    );
    assert_eq!(
        document.text_styles().expect("a style pool").len(),
        2,
        "distinct alignments must not collapse to one pool entry"
    );
}

// ---------------------------------------------------------------------------
// Out-of-vocabulary text features are named diagnostics, never silent drops
// (P4). Each would render a picture the designer never authored if lowered
// approximately.
// ---------------------------------------------------------------------------

#[test]
fn out_of_vocabulary_text_features_are_named_diagnostics() {
    // One synthetic node per feature: the document's TextStyle carries family,
    // size, weight, and color only, so a non-default value on any other axis
    // is refused by name. Each `style` override sits on an otherwise-clean
    // HUG text node, so the diagnostic named is the feature under test.
    let cases: &[(&str, serde_json::Value, &str)] = &[
        (
            "italic",
            serde_json::json!({ "fontStyle": "Italic" }),
            "italic text",
        ),
        (
            "decoration",
            serde_json::json!({ "textDecoration": "UNDERLINE" }),
            "text decoration",
        ),
        (
            "case",
            serde_json::json!({ "textCase": "UPPER" }),
            "a text case transform",
        ),
        (
            "truncate",
            serde_json::json!({ "textAutoResize": "TRUNCATE" }),
            "text truncation",
        ),
        (
            "hyperlink",
            serde_json::json!({ "hyperlink": { "type": "URL", "url": "https://x" } }),
            "a text hyperlink",
        ),
        (
            "opentype",
            serde_json::json!({ "opentypeFlags": { "smcp": 1 } }),
            "OpenType features",
        ),
    ];

    for (label, style_override, expected) in cases {
        let mut style = base_style();
        let style_map = style.as_object_mut().unwrap();
        for (k, v) in style_override.as_object().unwrap() {
            style_map.insert(k.clone(), v.clone());
        }
        let mut text = text_json("t", "hi", 16.0, 400);
        text["style"] = style;
        let json = wrap_single(text);

        let (_, diagnostics) = lower(
            &serde_json::from_str(&json).unwrap(),
            Profile::Core,
            &BTreeMap::new(),
        )
        .expect("the feature is diagnosed, not fatal");

        assert_eq!(
            unsupported(&diagnostics),
            vec![("/root/t".to_string(), expected.to_string())],
            "{label}",
        );
    }
}

// ---------------------------------------------------------------------------
// The four widened style axes lower (story #310): PIXELS line height, letter
// spacing, horizontal alignment (LEFT/CENTER/RIGHT), and vertical alignment
// (TOP/CENTER/BOTTOM). A percentage line height and JUSTIFIED stay refused.
// ---------------------------------------------------------------------------

#[test]
fn the_four_style_axes_lower_into_the_text_style() {
    use dashc_wasm::{TextAlign, TextAlignV};
    let mut style = base_style();
    let m = style.as_object_mut().unwrap();
    m.insert("lineHeightUnit".into(), "PIXELS".into());
    m.insert("lineHeightPx".into(), 30.0.into());
    m.insert("letterSpacing".into(), 2.5.into());
    m.insert("textAlignHorizontal".into(), "CENTER".into());
    m.insert("textAlignVertical".into(), "BOTTOM".into());
    let mut text = text_json("t", "hi", 16.0, 400);
    text["style"] = style;
    let json = wrap_single(text);

    let (doc, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("the widened axes lower");
    assert!(
        unsupported(&diagnostics).is_empty(),
        "{:?}",
        unsupported(&diagnostics),
    );
    let ts = node(&doc, "t")
        .1
        .text_style
        .as_ref()
        .expect("carries a style");
    assert_eq!(ts.line_height_px, Some(30.0));
    assert_eq!(ts.letter_spacing, 2.5);
    assert_eq!(ts.text_align, TextAlign::Center);
    assert_eq!(ts.text_align_v, TextAlignV::Bottom);
}

#[test]
fn horizontal_and_vertical_alignment_lower_each_value() {
    use dashc_wasm::{TextAlign, TextAlignV};
    for (figma, want) in [
        ("LEFT", TextAlign::Left),
        ("CENTER", TextAlign::Center),
        ("RIGHT", TextAlign::Right),
    ] {
        let mut text = text_json("t", "hi", 16.0, 400);
        text["style"]["textAlignHorizontal"] = figma.into();
        let json = wrap_single(text);
        let (doc, _) = lower(
            &serde_json::from_str(&json).unwrap(),
            Profile::Core,
            &BTreeMap::new(),
        )
        .expect("lowers");
        assert_eq!(
            node(&doc, "t").1.text_style.as_ref().unwrap().text_align,
            want,
            "h-align {figma}"
        );
    }
    for (figma, want) in [
        ("TOP", TextAlignV::Top),
        ("CENTER", TextAlignV::Center),
        ("BOTTOM", TextAlignV::Bottom),
    ] {
        let mut text = text_json("t", "hi", 16.0, 400);
        text["style"]["textAlignVertical"] = figma.into();
        let json = wrap_single(text);
        let (doc, _) = lower(
            &serde_json::from_str(&json).unwrap(),
            Profile::Core,
            &BTreeMap::new(),
        )
        .expect("lowers");
        assert_eq!(
            node(&doc, "t").1.text_style.as_ref().unwrap().text_align_v,
            want,
            "v-align {figma}"
        );
    }
}

#[test]
fn the_default_axes_lower_to_left_top_auto_and_zero() {
    use dashc_wasm::{TextAlign, TextAlignV};
    // The hug-in-fill leaf carries LEFT/TOP, INTRINSIC_% line height, and zero
    // letter spacing — the behavior-preserving defaults.
    let (doc, _) = lowered(HUG_IN_FILL);
    let ts = node(&doc, "hug inside fill").1.text_style.as_ref().unwrap();
    assert_eq!(ts.line_height_px, None);
    assert_eq!(ts.letter_spacing, 0.0);
    assert_eq!(ts.text_align, TextAlign::Left);
    assert_eq!(ts.text_align_v, TextAlignV::Top);
}

#[test]
fn a_percent_line_height_and_justified_alignment_are_still_refused() {
    // Only PIXELS line height lowers; the percentage units and JUSTIFIED have
    // no vocabulary and stay named refusals (P4).
    for (field, value, expected) in [
        ("lineHeightUnit", "FONT_SIZE_%", "a FONT_SIZE_% line height"),
        ("lineHeightUnit", "PERCENT", "a PERCENT line height"),
        (
            "textAlignHorizontal",
            "JUSTIFIED",
            "text alignment JUSTIFIED",
        ),
    ] {
        let mut style = base_style();
        style
            .as_object_mut()
            .unwrap()
            .insert(field.into(), value.into());
        let mut text = text_json("t", "hi", 16.0, 400);
        text["style"] = style;
        let json = wrap_single(text);

        let (_, diagnostics) = lower(
            &serde_json::from_str(&json).unwrap(),
            Profile::Core,
            &BTreeMap::new(),
        )
        .expect("diagnosed, not fatal");
        assert_eq!(
            unsupported(&diagnostics),
            vec![("/root/t".to_string(), expected.to_string())],
            "{value}",
        );
    }
}

#[test]
fn multiple_style_segments_are_diagnosed() {
    // A non-empty styleOverrideTable means the text mixes styles across
    // characters, which the single-style TextStyle cannot express.
    let mut text = text_json("t", "mixed", 16.0, 400);
    text["styleOverrideTable"] = serde_json::json!({ "1": { "fontSize": 24.0 } });
    let json = wrap_single(text);

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("diagnosed, not fatal");
    assert_eq!(
        unsupported(&diagnostics),
        vec![(
            "/root/t".to_string(),
            "multiple text style segments (styleOverrideTable)".to_string(),
        )],
    );
}

#[test]
fn a_text_stroke_outline_is_diagnosed() {
    // A visible text outline (stroke) has no vocabulary — the style carries a
    // fill color only, so an outline is refused rather than dropped. Gate on
    // the strokes array (strokeWeight is present even with no stroke), the
    // same rule stroke_of uses for frames.
    let mut text = text_json("t", "hi", 16.0, 400);
    text["strokes"] = serde_json::json!([
        { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }
    ]);
    let json = wrap_single(text);

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("diagnosed, not fatal");
    assert_eq!(
        unsupported(&diagnostics),
        vec![("/root/t".to_string(), "a text stroke (outline)".to_string())],
    );
}

#[test]
fn a_text_node_with_no_solid_fill_is_diagnosed() {
    // A text node's fill is its glyph color; a gradient fill has no lowering
    // into one color, so it is refused rather than painted an invented color.
    let mut text = text_json("t", "hi", 16.0, 400);
    text["fills"] = serde_json::json!([{
        "type": "GRADIENT_LINEAR",
        "gradientHandlePositions": [{"x":0.0,"y":0.0},{"x":1.0,"y":0.0},{"x":0.0,"y":1.0}],
        "gradientStops": [{"position":0.0,"color":{"r":0.0,"g":0.0,"b":0.0,"a":1.0}}],
    }]);
    let json = wrap_single(text);

    let (_, diagnostics) = lower(
        &serde_json::from_str(&json).unwrap(),
        Profile::Core,
        &BTreeMap::new(),
    )
    .expect("diagnosed, not fatal");
    assert_eq!(
        unsupported(&diagnostics),
        vec![("/root/t".to_string(), "a non-solid text fill".to_string())],
    );
}

// ---------------------------------------------------------------------------
// The emitted bytes are pinned, next to the flex golden .dsbs.
// ---------------------------------------------------------------------------

#[test]
fn the_text_fixtures_emit_their_golden_dsbs() {
    // Same contract as flex_lowering.rs: regenerate with UPDATE_GOLDENS=1,
    // review, commit — a missing golden fails rather than minting its own
    // truth (goldens/README.md). The raw hug-in-fill carries the HUG text
    // leaf; the derived baseline carries the mixed-size Latin rows and the
    // Arabic RTL run.
    for (name, json) in [
        ("v07-text-hug-in-fill.dsb", HUG_IN_FILL.to_string()),
        ("v07-text-baseline-derived.dsb", baseline_text_derived()),
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
                "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test text_lowering` to create it",
                path.display(),
            )
        });
        assert_eq!(
            bytes, golden,
            "{name} drifted. If this is intended, regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
        );
    }
}

// ---------------------------------------------------------------------------
// Synthetic-node builders.
// ---------------------------------------------------------------------------

/// A `TEXT` node with the default (all-mapping) style: LEFT/TOP alignment,
/// intrinsic (auto) line height, zero letter spacing, upright, a single solid
/// ink fill — everything the vocabulary carries, nothing it does not.
fn base_style() -> serde_json::Value {
    serde_json::json!({
        "fontFamily": "Inter",
        "fontStyle": "Regular",
        "fontWeight": 400,
        "fontSize": 16.0,
        "textAlignHorizontal": "LEFT",
        "textAlignVertical": "TOP",
        "letterSpacing": 0.0,
        "lineHeightUnit": "INTRINSIC_%",
    })
}

fn text_json(name: &str, characters: &str, size: f32, weight: u16) -> serde_json::Value {
    let mut style = base_style();
    style["fontSize"] = size.into();
    style["fontWeight"] = weight.into();
    serde_json::json!({
        "name": name,
        "type": "TEXT",
        "characters": characters,
        "style": style,
        "layoutSizingHorizontal": "HUG",
        "layoutSizingVertical": "HUG",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 40.0, "height": 20.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0 } }],
    })
}

/// A one-page document whose root FRAME holds `child` as its only node.
fn wrap_single(child: serde_json::Value) -> String {
    serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [{
                "name": "root", "type": "FRAME", "layoutMode": "HORIZONTAL",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 40.0 },
                "children": [child],
            }],
        }]},
    })
    .to_string()
}
