//! Story #167 acceptance: bindings authored in Figma reach the runtime.
//!
//!     capture + joined rows → compile → .dsb → load_document → arena
//!     tables → attach_live → live signals drive the committed scene
//!
//! The capture is `corpus/figma-fixtures/variables-bound.json`, derived:
//! the fixture's root hugs its width around two `Fill` cards, which the
//! flex lowering refuses by design (`docs/decisions/figma-flex-lowering.md`
//! D5), so the test pins the root's width sizing to `FIXED` — the
//! documented fixture derivation pattern — and compiles the rest as
//! captured. The joined rows are hand-built to exactly the rows the Deno
//! join produces for this capture and its committed vartable;
//! `importers/figma/src/bindings_test.ts` pins that side, so the contract
//! is checked from both ends.

use std::collections::BTreeMap;

use dashc_wasm::figma::{BoundValue, BoundVariable};
use dashc_wasm::{compile_figma_with_bindings, rule};
use dashscene_core::{Arena, Channel, ScalarTransform, load_document};
use dashscene_validator::{Profile, Severity};

const VARIABLES_BOUND: &str = include_str!("../../../corpus/figma-fixtures/variables-bound.json");

/// The capture with the root's hug width pinned to FIXED, so the `Fill`
/// cards inside it lower instead of being refused (D5).
fn derived_capture() -> String {
    let mut file: serde_json::Value =
        serde_json::from_str(VARIABLES_BOUND).expect("the fixture parses");
    fn patch(node: &mut serde_json::Value) {
        if node["name"] == "variables-bound" {
            node["layoutSizingHorizontal"] = "FIXED".into();
            return;
        }
        if let Some(children) = node["children"].as_array_mut() {
            for child in children {
                patch(child);
            }
        }
    }
    patch(&mut file["document"]);
    file.to_string()
}

fn float(node_id: &str, property: &str, signal: &str, value: f32) -> BoundVariable {
    BoundVariable {
        node_id: node_id.to_string(),
        property: property.to_string(),
        signal: signal.to_string(),
        value: BoundValue::Float(value),
    }
}

fn color(node_id: &str, property: &str, signal: &str, rgba: [f32; 4]) -> BoundVariable {
    BoundVariable {
        node_id: node_id.to_string(),
        property: property.to_string(),
        signal: signal.to_string(),
        value: BoundValue::Color {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        },
    }
}

/// The rows the Deno join derives from the fixture capture plus its
/// committed vartable (`variables-bound.vartable.json`), in sidecar
/// (document) order. The light card inherits the collection's default
/// mode; the dark card pins `explicitVariableModes` to dark, so its
/// signals are mode-qualified and carry the dark values.
fn joined_rows() -> Vec<BoundVariable> {
    const BG_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    const BG_DARK: [f32; 4] = [0.08, 0.09, 0.11, 1.0];
    const ACCENT_LIGHT: [f32; 4] = [0.13, 0.45, 0.9, 1.0];
    const ACCENT_DARK: [f32; 4] = [0.4, 0.65, 1.0, 1.0];
    const CORNERS: [&str; 4] = [
        "rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS",
        "rectangleCornerRadii.RECTANGLE_TOP_RIGHT_CORNER_RADIUS",
        "rectangleCornerRadii.RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS",
        "rectangleCornerRadii.RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS",
    ];
    let mut rows = Vec::new();
    // card-inherits-mode (1:8) and its chip (1:9) — default (light) mode.
    rows.push(float("1:8", "itemSpacing", "size/gap", 16.0));
    for corner in CORNERS {
        rows.push(float("1:8", corner, "size/radius", 8.0));
    }
    rows.push(color("1:8", "fills[0].color", "color/bg", BG_LIGHT));
    rows.push(color("1:9", "fills[0].color", "color/accent", ACCENT_LIGHT));
    // card-explicit-dark (1:11) and its chip (1:12) — dark-pinned.
    rows.push(float("1:11", "itemSpacing", "size/gap@dark", 24.0));
    for corner in CORNERS {
        rows.push(float("1:11", corner, "size/radius@dark", 2.0));
    }
    rows.push(color("1:11", "fills[0].color", "color/bg@dark", BG_DARK));
    rows.push(color(
        "1:12",
        "fills[0].color",
        "color/accent@dark",
        ACCENT_DARK,
    ));
    rows
}

/// Compiles the derived capture with the joined rows and loads it.
fn compile_and_load() -> (Arena, dashscene_validator::Report) {
    let (bytes, report) = compile_figma_with_bindings(
        &derived_capture(),
        Profile::Core,
        &BTreeMap::new(),
        &joined_rows(),
    )
    .expect("the derived capture compiles");

    let doc = dashbuf::root_as_document(&bytes).expect("valid .dsb");
    let gate = dashscene_validator::validate_document(&doc);
    assert!(!gate.has_errors(), "the load gate passes:\n{gate}");
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);
    (arena, report)
}

#[test]
fn figma_authored_bindings_land_in_the_arena_tables() {
    let (arena, report) = compile_and_load();

    // The unsupported sites (a corner radius has no binding channel yet;
    // four corners per card) are named warnings, never a silent drop
    // (P4) and never a block — the resolved literals ship.
    let unsupported: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == "figma.bindings.unsupported-property")
        .collect();
    assert_eq!(unsupported.len(), 8, "report was:\n{report}");
    assert!(
        unsupported
            .iter()
            .all(|d| d.severity == Severity::Warning && d.message.contains("rectangleCornerRadii"))
    );

    // One signal per (variable, mode, channel), interned in row order.
    let names: Vec<Option<&str>> = arena.signals().iter().map(|s| s.name.as_deref()).collect();
    assert_eq!(
        names,
        [
            Some("size/gap"),
            Some("color/bg.r"),
            Some("color/bg.g"),
            Some("color/bg.b"),
            Some("color/bg.a"),
            Some("color/accent.r"),
            Some("color/accent.g"),
            Some("color/accent.b"),
            Some("color/accent.a"),
            Some("size/gap@dark"),
            Some("color/bg@dark.r"),
            Some("color/bg@dark.g"),
            Some("color/bg@dark.b"),
            Some("color/bg@dark.a"),
            Some("color/accent@dark.r"),
            Some("color/accent@dark.g"),
            Some("color/accent@dark.b"),
            Some("color/accent@dark.a"),
        ]
    );
    assert_eq!(arena.signals()[0].initial, 16.0);
    assert_eq!(arena.signals()[9].initial, 24.0, "dark gap is 24");
    assert_eq!(arena.signals()[14].initial, 0.4, "dark accent red");

    // 2 gap rows + 4 fill sites x 4 channels.
    let rows = arena.bindings();
    assert_eq!(rows.len(), 18);
    assert_eq!(rows[0].channel, Channel::Gap);
    assert!(
        rows.iter()
            .all(|r| r.transform == ScalarTransform::Identity)
    );

    // The two gap rows target the two cards (document DFS: root 0,
    // card 1, chip 2, text 3, card 4, chip 5, text 6).
    assert_eq!(rows[0].node.index(), 1);
    let dark_gap = &rows[9];
    assert_eq!(dark_gap.channel, Channel::Gap);
    assert_eq!(dark_gap.node.index(), 4);
    assert_eq!(
        arena.signals()[dark_gap.signal.index()].name.as_deref(),
        Some("size/gap@dark")
    );
}

/// The story's headline: an imported document's Figma-variable bindings
/// become dashlang reactive bindings — set a variable's signal by name,
/// tick, and the committed scene follows.
#[test]
fn a_loaded_figma_document_drives_through_attach_live() {
    use dashscene_core::PaintKind;
    use dashscene_engine::TaffySolver;

    let (mut arena, _) = compile_and_load();
    let mut live = dashlang::attach_live(&mut arena, Box::new(TaffySolver::new()));

    // The dark card's accent chip follows its mode-qualified signal.
    let accent_dark_r = live
        .signal_named("color/accent@dark.r")
        .expect("the dark accent signal is declared");
    live.set(accent_dark_r, 0.9);
    live.tick(0.016, &mut arena);

    let dark_chip = arena.committed().node_of(5);
    match arena.fill(dark_chip) {
        Some(PaintKind::Solid { color }) => {
            assert_eq!(color.r, 0.9, "the bound component tracks the signal");
            assert_eq!(
                color.g, 0.65,
                "unbound components keep the mode-resolved literal"
            );
        }
        other => panic!("expected a solid fill, got {other:?}"),
    }

    // The light card's gap follows its unqualified signal and reflows
    // the card's children.
    let chip_y_before = arena.committed().rects()[3].y;
    let gap = live.signal_named("size/gap").expect("size/gap is declared");
    live.set(gap, 40.0);
    live.tick(0.016, &mut arena);
    let chip_y_after = arena.committed().rects()[3].y;
    assert_eq!(
        chip_y_after - chip_y_before,
        24.0,
        "gap 16 -> 40 moves the second child down by 24"
    );
}

/// The acceptance criterion proper: the same scene authored in dashlang
/// emits the same binding rows — same signal declarations, same
/// (node, channel, transform) rows, node for node.
#[test]
fn a_dashlang_authored_scene_emits_the_same_binding_rows() {
    use dashlang::{Channel as LangChannel, LayoutMode, Scene, anon, node, rgba};
    use dashscene_engine::TaffySolver;

    let (figma_arena, _) = compile_and_load();

    // The same topology (root, two cards each holding a chip and a text
    // box), the same signals in the same order, the same bindings in the
    // same order. Values mirror the vartable's two modes.
    let mut scene = Scene::new();
    let gap = scene.signal_named("size/gap", 16.0);
    let bg = [
        scene.signal_named("color/bg.r", 1.0),
        scene.signal_named("color/bg.g", 1.0),
        scene.signal_named("color/bg.b", 1.0),
        scene.signal_named("color/bg.a", 1.0),
    ];
    let accent = [
        scene.signal_named("color/accent.r", 0.13),
        scene.signal_named("color/accent.g", 0.45),
        scene.signal_named("color/accent.b", 0.9),
        scene.signal_named("color/accent.a", 1.0),
    ];
    let gap_dark = scene.signal_named("size/gap@dark", 24.0);
    let bg_dark = [
        scene.signal_named("color/bg@dark.r", 0.08),
        scene.signal_named("color/bg@dark.g", 0.09),
        scene.signal_named("color/bg@dark.b", 0.11),
        scene.signal_named("color/bg@dark.a", 1.0),
    ];
    let accent_dark = [
        scene.signal_named("color/accent@dark.r", 0.4),
        scene.signal_named("color/accent@dark.g", 0.65),
        scene.signal_named("color/accent@dark.b", 1.0),
        scene.signal_named("color/accent@dark.a", 1.0),
    ];

    let fill_channels = [
        LangChannel::FillR,
        LangChannel::FillG,
        LangChannel::FillB,
        LangChannel::FillA,
    ];
    let card = |gap_signal: dashlang::Signal<f32>,
                card_fill: [dashlang::Signal<f32>; 4],
                chip_fill: [dashlang::Signal<f32>; 4]| {
        let mut card = node("card")
            .mode(LayoutMode::Vertical)
            .size(100.0, 105.0)
            .fill(rgba(1.0, 1.0, 1.0, 1.0))
            .bind(LangChannel::Gap, gap_signal);
        for (channel, signal) in fill_channels.into_iter().zip(card_fill) {
            card = card.bind(channel, signal);
        }
        let mut chip = node("chip")
            .size(24.0, 24.0)
            .fill(rgba(0.13, 0.45, 0.9, 1.0));
        for (channel, signal) in fill_channels.into_iter().zip(chip_fill) {
            chip = chip.bind(channel, signal);
        }
        card.child(chip).child(anon().size(60.0, 17.0))
    };

    scene.roots([node("variables-bound")
        .mode(LayoutMode::Horizontal)
        .size(272.0, 153.0)
        .child(card(gap, bg, accent))
        .child(card(gap_dark, bg_dark, accent_dark))]);

    let mut lang_arena = dashlang::Arena::new();
    scene.build_live(&mut lang_arena, Box::new(TaffySolver::new()));

    assert_eq!(
        lang_arena.signals(),
        figma_arena.signals(),
        "both producers declare the same signal table"
    );
    assert_eq!(
        lang_arena.bindings(),
        figma_arena.bindings(),
        "both producers emit the same binding rows"
    );
}

/// A `Custom` transform reaching dashc is a named diagnostic, never a
/// silent drop (issue #167 acceptance; D8).
#[test]
fn a_custom_transform_reaching_compile_is_refused_by_name() {
    use dashc_wasm::{Binding, BindingChannel, BindingTransform, Document, SignalDecl, compile};

    let mut doc = Document::new();
    doc.push(dashc_wasm::Node {
        box2d: dashc_wasm::Box2D {
            width: 10.0,
            height: 10.0,
            ..Default::default()
        },
        ..Default::default()
    });
    doc.signals.push(SignalDecl {
        name: "speed".to_string(),
        initial: 0.0,
    });
    doc.bindings.push(Binding {
        signal: 0,
        node: 0,
        channel: BindingChannel::Width,
        transform: BindingTransform::Custom(3),
    });

    let report = compile(&doc).expect_err("a Custom transform blocks the document");
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|d| d.rule == rule::CUSTOM_TRANSFORM)
        .expect("the Custom transform is named");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(diagnostic.message.contains("does not serialize"));
}
