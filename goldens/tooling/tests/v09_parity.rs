//! E1 same-screen-both-ways parity (story #48), first cut:
//!
//!     Figma REST JSON ─┐
//!                       ├─► same intent ─► TaffySolver ─► committed scene ─► Skia
//!     dashlang scene  ─┘
//!
//! One screen is authored twice — as a synthetic Figma REST document and in
//! the Rust DSL — and the two are asserted bit-identical in three places: the
//! committed rect table, the committed paint pool, and the Skia CPU render.
//! That is exit criterion E1 (`docs/specification/05-qualification.md`), for
//! the layout-plus-solid-fill subset both producers express.
//!
//! Scope is deliberately within the intersection of the two producers'
//! vocabularies: nested FIXED and FILL frames, horizontal and vertical
//! `layoutMode`, gap,
//! four-edge padding, MIN/CENTER/MAX main- and cross-axis alignment, and a
//! single SOLID fill per node. No text, stroke, gradient, corner radius,
//! ellipse, clip, opacity, mask, or binding — those are outside the shared
//! vocabulary and/or gated on the epic #47 binding-parity scope decision
//! (docs/roadmap.md "v0.9 — parity", issues #252/#256).
//!
//! Determinism: every authored dimension is an integer and every solved rect
//! lands on an integer, so the solid fills are integer-aligned, produce no
//! anti-aliased edges, and the two renders compare exactly — the same
//! bit-stable comparison the v0.2 flex goldens use
//! (`docs/decisions/golden-comparison-space.md`,
//! `docs/decisions/v02-flex-goldens-per-construct.md`). The two producers'
//! renders are asserted equal to each other, and both are anchored to one
//! reviewed golden picture.

use std::collections::BTreeMap;

use dashc_wasm::compile_figma;
use dashlang::{AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, node, scene};
use dashpaint::{GlyphRunTable, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_validator::Profile;

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

// The five distinct fills, authored identically on both sides so the interned
// paint pool matches — including its dedup: GOLD and GREEN each appear twice
// (a chip and a cell), so a correct pool holds five entries, not seven.
const NAVY: Color = rgb(0.05, 0.1, 0.2);
const RED: Color = rgb(0.8, 0.1, 0.1);
const GOLD: Color = rgb(0.9, 0.7, 0.1);
const GREEN: Color = rgb(0.1, 0.7, 0.2);
const BLUE: Color = rgb(0.2, 0.4, 0.9);

/// A Figma SOLID paint built from the same `Color` constant the DSL side
/// fills with, so the two producers' colors cannot drift apart.
fn solid(c: Color) -> serde_json::Value {
    serde_json::json!({
        "type": "SOLID",
        "color": { "r": c.r, "g": c.g, "b": c.b, "a": c.a },
    })
}

/// A fixed-size SOLID-filled leaf FRAME. The position and extent are Figma's
/// own solver output; the importer zeroes an auto-layout child's position and
/// keeps only a Fixed axis's extent (P1), so these values are the boxes Figma
/// would report, not what the runtime re-solves to.
fn leaf(name: &str, x: f32, y: f32, w: f32, h: f32, fill: Color) -> serde_json::Value {
    serde_json::json!({
        "name": name, "type": "FRAME",
        "absoluteBoundingBox": { "x": x, "y": y, "width": w, "height": h },
        "fills": [solid(fill)],
    })
}

/// The screen as a synthetic Figma REST document: DOCUMENT → CANVAS → root
/// FRAME. Authored node-for-node in the same depth-first order as
/// [`dsl_scene`]. Built from shallow `json!` fragments (a deeply nested single
/// macro exceeds the expansion recursion limit).
fn figma_document() -> String {
    let header = serde_json::json!({
        "name": "header", "type": "FRAME",
        "layoutMode": "HORIZONTAL",
        "layoutSizingHorizontal": "FILL", "layoutSizingVertical": "FIXED",
        "itemSpacing": 10.0,
        "primaryAxisAlignItems": "MIN", "counterAxisAlignItems": "CENTER",
        "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 140.0, "height": 30.0 },
        "fills": [solid(RED)],
        "children": [
            leaf("chip-a", 10.0, 15.0, 40.0, 20.0, GOLD),
            leaf("chip-b", 60.0, 15.0, 30.0, 20.0, GREEN),
        ],
    });
    let body = serde_json::json!({
        "name": "body", "type": "FRAME",
        "layoutMode": "HORIZONTAL",
        "layoutSizingHorizontal": "FILL", "layoutSizingVertical": "FILL",
        "itemSpacing": 10.0,
        "primaryAxisAlignItems": "CENTER", "counterAxisAlignItems": "MAX",
        "absoluteBoundingBox": { "x": 10.0, "y": 50.0, "width": 140.0, "height": 40.0 },
        "fills": [solid(BLUE)],
        "children": [
            leaf("cell-a", 35.0, 70.0, 40.0, 20.0, GOLD),
            leaf("cell-b", 85.0, 70.0, 40.0, 20.0, GREEN),
        ],
    });
    let root = serde_json::json!({
        "name": "root", "type": "FRAME",
        "layoutMode": "VERTICAL",
        "itemSpacing": 10.0,
        "paddingLeft": 10.0, "paddingTop": 10.0,
        "paddingRight": 10.0, "paddingBottom": 10.0,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 160.0, "height": 100.0 },
        "fills": [solid(NAVY)],
        "children": [header, body],
    });
    serde_json::json!({
        "document": { "name": "Document", "type": "DOCUMENT", "children": [{
            "name": "Page 1", "type": "CANVAS", "children": [root],
        }]},
    })
    .to_string()
}

/// The same screen in the DSL. Every construct maps one-for-one to a Figma
/// field in [`figma_document`]: `mode` ↔ `layoutMode`, `gap` ↔ `itemSpacing`,
/// `sizing_h`/`sizing_v` ↔ `layoutSizing*` (a FILL axis authors width/height
/// zero, matching the importer zeroing that axis), `main_align`/`cross_align`
/// ↔ `primaryAxisAlignItems`/`counterAxisAlignItems` (Start=MIN, Center,
/// End=MAX), `fill` ↔ the SOLID paint. Nodes are declared in the same
/// depth-first order, so the interned paint pool matches.
fn dsl_scene(arena: &mut Arena) {
    scene([node("root")
        .size(160.0, 100.0)
        .mode(LayoutMode::Vertical)
        .gap(10.0)
        .padding(10.0, 10.0, 10.0, 10.0)
        .fill(NAVY)
        .child(
            node("header")
                .mode(LayoutMode::Horizontal)
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .gap(10.0)
                .main_align(MainAxisAlign::Start)
                .cross_align(CrossAxisAlign::Center)
                .fill(RED)
                .child(node("chip-a").size(40.0, 20.0).fill(GOLD))
                .child(node("chip-b").size(30.0, 20.0).fill(GREEN)),
        )
        .child(
            node("body")
                .mode(LayoutMode::Horizontal)
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .size(0.0, 0.0)
                .gap(10.0)
                .main_align(MainAxisAlign::Center)
                .cross_align(CrossAxisAlign::End)
                .fill(BLUE)
                .child(node("cell-a").size(40.0, 20.0).fill(GOLD))
                .child(node("cell-b").size(40.0, 20.0).fill(GREEN)),
        )])
    .build_with(arena, &mut TaffySolver::new());
}

/// Compiles the synthetic Figma document, loads it, and re-solves through the
/// engine — the importer path a real Figma producer takes. Returns the
/// committed arena.
fn figma_arena() -> Arena {
    let json = figma_document();
    let (bytes, report) =
        compile_figma(&json, Profile::Core, &BTreeMap::new()).expect("the Figma screen compiles");
    // The Figma side lowers clean: the whole screen is inside profile:core, so
    // no diagnostic blocks or accompanies the emission.
    assert!(report.is_empty(), "the Figma side lowers clean: {report}");

    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    // `load_document` commits with the fixed solver; the flex intent needs the
    // engine. An empty transaction re-committed through a fresh `TaffySolver`
    // performs a full first solve (the pattern the flex-lowering tests use).
    arena.open().commit_with(&mut TaffySolver::new());
    arena
}

/// Paints the committed scene on a canvas sized to the root's solved rect and
/// returns the PNG. The scene carries no images, clips, groups, or glyph runs,
/// so the tables passed are the committed scene's own (all empty).
fn render(arena: &Arena) -> Vec<u8> {
    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    painter.png_bytes()
}

#[test]
fn the_same_screen_authored_both_ways_is_bit_identical() {
    let figma = figma_arena();
    let mut dsl = Arena::new();
    dsl_scene(&mut dsl);

    // E1, part one: bit-identical rect tables and paint pools. Addressing the
    // committed scenes directly, so this is the whole table, not a per-node
    // spot check.
    assert_eq!(
        figma.committed().rects(),
        dsl.committed().rects(),
        "the two producers solve to identical rect tables",
    );
    assert_eq!(
        figma.committed().paints(),
        dsl.committed().paints(),
        "the two producers intern identical paint pools",
    );
    // Pin the dedup itself: GOLD and GREEN each recur, so five pooled entries,
    // not seven. A comment cannot catch a lockstep interning regression on both
    // producers at once; this assertion can.
    assert_eq!(
        dsl.committed().paints().len(),
        5,
        "the paint pool dedups the repeated GOLD and GREEN fills to five entries",
    );

    // The intended layout, hand-computed and integral, in depth-first order:
    // root, header, its two chips, body, its two cells. This anchors the pair
    // above to the screen the fixture means to author — a check the two
    // producers agreeing with each other cannot give on its own.
    let expected: [(f32, f32, f32, f32); 7] = [
        (0.0, 0.0, 160.0, 100.0),  // root: fixed 160x100
        (10.0, 10.0, 140.0, 30.0), // header: fill width 140, fixed height 30, at the padding origin
        (10.0, 15.0, 40.0, 20.0),  // chip-a: MIN main, CENTER cross ((30-20)/2 = 5)
        (60.0, 15.0, 30.0, 20.0),  // chip-b: 10 + 40 + 10 gap
        (10.0, 50.0, 140.0, 40.0), // body: fill width 140, fill remaining height (80 - 30 - 10)
        (35.0, 70.0, 40.0, 20.0),  // cell-a: CENTER main ((140 - 90)/2 = 25), MAX cross (40 - 20)
        (85.0, 70.0, 40.0, 20.0),  // cell-b: 35 + 40 + 10 gap
    ];
    let rects = dsl.committed().rects();
    assert_eq!(rects.len(), expected.len(), "one rect per authored node");
    for (i, (rect, (x, y, w, h))) in rects.iter().zip(expected).enumerate() {
        assert_eq!((rect.x, rect.y, rect.w, rect.h), (x, y, w, h), "rect {i}");
    }

    // E1, part two: bit-identical Skia CPU renders. Integer-aligned solid
    // fills, so exact (no anti-aliasing tolerance).
    let figma_png = render(&figma);
    let dsl_png = render(&dsl);
    assert_eq!(
        figma_png, dsl_png,
        "the two producers rasterize to identical pixels",
    );

    // One reviewed golden anchors both producers to a picture, so a future
    // change that moved both in lockstep is still caught.
    goldens::assert_matches_golden("v09-parity", &dsl_png);
}
