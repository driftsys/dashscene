//! The Figma REST front end, end to end (story #139):
//!
//!     Figma REST JSON → lower → Document → emit → validate → .dsb
//!                                                              ↓
//!                                     dashscene-core → Skia painter
//!
//! Emission precedes validation because the load gate's rules are about the
//! index model, so they are checked against the serialized document rather
//! than against `Document`. An error from either gate withholds the bytes (R6).
//!
//! Every assertion here is pinned by the captured corpus, not by a reading of
//! Figma's documentation: `v03-paint.json` is the emission fixture and
//! `effects-2025.json` is the diagnostic fixture (corpus/figma-fixtures/README.md).

use std::collections::BTreeMap;

use dashc_wasm::figma::rest::FigmaFile;
use dashc_wasm::figma::{CompileError, lower};
use dashc_wasm::{EmitPolicy, compile_figma, compile_figma_with_bindings_and_policy};
use dashpaint::{
    Color, CornerRadii, GlyphRunTable, GradientKind, ImageAsset, ImageFormat, Mat23, PaintEntry,
    PaintKind, Painter, ScaleMode, ShadowKind, StrokeAlign, Vec2,
};
use dashscene_core::{Arena, load_document};
use dashscene_skia::SkiaPainter;
use dashscene_validator::{Location, Profile, Severity};

mod common;
use common::{node, parse};

/// The designated input for this story (corpus/figma-fixtures/manifest.json).
const V03_PAINT: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");

/// The diagnostic fixture. It can never emit a `.dsb`: everything it was
/// authored to carry is REJECT-band. Its root frame is auto-layout
/// (`layoutMode: HORIZONTAL`), which the v0.3 walk refused before reaching
/// the effects; since story #140 lowers auto-layout, the raw capture reaches
/// its three effects with no derivation.
const EFFECTS_2025: &str = include_str!("../../../corpus/figma-fixtures/effects-2025.json");

/// A one-page document whose root `FRAME` is `root`.
///
/// The fixtures pin the field *shapes* (P5), but they cannot cover a
/// construct no captured file contains — a rotated node, a second stroke, a
/// cropped image. Those cases are built here, out of the shapes the fixtures
/// already pinned.
fn document(root: serde_json::Value) -> FigmaFile {
    serde_json::from_value(document_json(root)).expect("the synthetic document parses")
}

/// The same one-page document shape as [`document`], as raw JSON text.
///
/// `compile_figma` parses `&str`, not a [`FigmaFile`] — and `FigmaFile` has
/// no `Serialize` impl, so a synthetic `compile_figma` case is built from
/// this rather than from `document`'s output.
fn document_json(root: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "document": {
            "name": "Document",
            "type": "DOCUMENT",
            "children": [{
                "name": "Page 1",
                "type": "CANVAS",
                "children": [root],
            }],
        },
    })
}

#[test]
fn the_fixture_parses_into_the_rest_subset() {
    let file = parse(V03_PAINT);

    let canvas = &file.document.children[0];
    assert_eq!(canvas.kind, "CANVAS");

    let root = &canvas.children[0];
    assert_eq!(root.name, "v03-paint");
    assert!(root.clips_content);

    let bbox = root
        .absolute_bounding_box
        .expect("the root frame has a box");
    assert_eq!((bbox.width, bbox.height), (960.0, 680.0));
}

#[test]
fn corner_radius_and_rectangle_corner_radii_are_mutually_exclusive() {
    // Figma nulls whichever does not apply. A lowering that read both would
    // be guessing; the capture settles it.
    let file = parse(V03_PAINT);
    let uniform = find(&file, "corners-uniform");
    let per_corner = find(&file, "corners-per-corner");

    assert_eq!(uniform.corner_radius, Some(16.0));
    assert_eq!(uniform.rectangle_corner_radii, None);

    assert_eq!(per_corner.corner_radius, None);
    assert_eq!(
        per_corner.rectangle_corner_radii,
        Some([0.0, 24.0, 4.0, 48.0]),
    );
}

#[test]
fn stroke_weight_and_align_are_present_even_with_no_stroke() {
    // The trap: a lowering that gated the stroke on `strokeWeight` being
    // present would give every unstroked frame a 1px stroke.
    let file = parse(V03_PAINT);
    let unstroked = find(&file, "fill-solid");

    assert!(unstroked.strokes.is_empty());
    assert_eq!(unstroked.stroke_weight, Some(1.0));
    assert!(unstroked.stroke_align.is_some());
}

#[test]
fn an_image_fill_carries_only_a_ref() {
    // No bytes anywhere in the file JSON — the whole reason the caller
    // supplies an imageRef→bytes map (design D1).
    let file = parse(V03_PAINT);
    let node = find(&file, "image-fit");

    let fill = &node.fills[0];
    assert_eq!(fill.kind, "IMAGE");
    assert_eq!(
        fill.image_ref.as_deref(),
        Some("390616a0e7321eddb464388366d9a2a1bcb7f4c3"),
    );
    assert!(fill.color.is_none(), "an image fill carries no color");
}

#[test]
fn progressive_blur_is_a_layer_blur_carrying_a_blur_type() {
    // The type alone cannot decide the band: plain LAYER_BLUR warns,
    // LAYER_BLUR + blurType PROGRESSIVE rejects.
    let file = parse(EFFECTS_2025);
    let node = find(&file, "progressive-blur");

    let effect = &node.effects[0];
    assert_eq!(effect.kind, "LAYER_BLUR");
    assert_eq!(effect.blur_type.as_deref(), Some("PROGRESSIVE"));
}

/// Depth-first search for a node by name. Panics if absent — a fixture that
/// lost a node should fail loudly, not skip the assertion.
fn find<'a>(file: &'a FigmaFile, name: &str) -> &'a dashc_wasm::figma::rest::Node {
    fn walk<'a>(
        node: &'a dashc_wasm::figma::rest::Node,
        name: &str,
    ) -> Option<&'a dashc_wasm::figma::rest::Node> {
        if node.name == name {
            return Some(node);
        }
        node.children.iter().find_map(|child| walk(child, name))
    }
    walk(&file.document, name).unwrap_or_else(|| panic!("fixture has no node named {name}"))
}

/// The fixture's image fill is an `imageRef` with no bytes anywhere in the JSON,
/// so the caller supplies them (design D1). In production that is the Deno
/// importer resolving `GET /images`; here it is the same corpus file the
/// importer's own tests read — which is what makes the golden below a
/// cross-language contract rather than two unrelated assertions.
const IMAGE_PNG: &[u8] = include_bytes!(
    "../../../corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png"
);

const IMAGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";

fn images() -> BTreeMap<String, ImageAsset> {
    BTreeMap::from([(
        IMAGE_REF.to_string(),
        ImageAsset {
            format: ImageFormat::Png,
            bytes: IMAGE_PNG.to_vec(),
        },
    )])
}

fn lowered() -> dashc_wasm::Document {
    let (doc, diagnostics) = lower(&parse(V03_PAINT), Profile::Core, &images())
        .expect("the paint fixture is entirely NOW-band");
    assert!(
        diagnostics.is_empty(),
        "v03-paint must triage clean, or it could never emit",
    );
    doc
}

/// Asserts `diagnostics` carries exactly one `figma.unsupported` error, that
/// its path contains `at`, and that the construct it names is `what` — the
/// shape every refusal test in this file pins. The unsupported node itself
/// must not have lowered: its subtree is skipped, never approximated.
fn assert_sole_unsupported(
    doc: &dashc_wasm::Document,
    diagnostics: &[dashscene_validator::Diagnostic],
    at: &str,
    what: &str,
) {
    let found: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.unsupported")
        .collect();
    let [diagnostic] = found[..] else {
        panic!("expected exactly one unsupported diagnostic, got {found:?}");
    };

    assert_eq!(diagnostic.severity, dashscene_validator::Severity::Error);
    assert_eq!(
        diagnostic.message,
        format!("{what} is not in the document vocabulary yet"),
    );
    let Location::Node(path) = &diagnostic.at else {
        panic!("an unsupported construct is located at a node");
    };
    assert!(
        path.path.contains(at),
        "the diagnostic names the node: {}",
        path.path,
    );
    assert!(
        !doc.nodes
            .iter()
            .any(|n| n.name.as_deref().is_some_and(|name| at.contains(name))),
        "the unsupported node must be skipped, not lowered",
    );
}

#[test]
fn the_fixture_root_is_the_first_rect_table_entry() {
    let doc = lowered();
    let (index, root) = node(&doc, "v03-paint");

    assert_eq!(index, 0, "the root is the first rect-table entry");
    assert_eq!(root.parent, None);
    assert_eq!((root.box2d.width, root.box2d.height), (960.0, 680.0));
}

#[test]
fn the_root_frame_drops_its_page_position() {
    // Where a frame sits on the Figma canvas is a page-layout artifact, not
    // intent (P1).
    //
    // Synthetic, and it has to be: the captured fixture's root already sits at
    // absolute (0, 0), so it cannot tell a lowering that subtracts the root
    // origin from one that never subtracts at all. This root sits at
    // (100, 200), so the subtraction is observable — and the child pins that
    // dropping the root's page position does not also shift its children.
    let file = document(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 100.0, "y": 200.0, "width": 300.0, "height": 400.0 },
        "children": [{
            "name": "child",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 140.0, "y": 260.0, "width": 50.0, "height": 60.0 },
        }],
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");

    let (index, root) = node(&doc, "root");
    assert_eq!(index, 0, "the root is the first rect-table entry");
    assert_eq!(root.parent, None);
    assert_eq!((root.box2d.x, root.box2d.y), (0.0, 0.0));
    assert_eq!((root.box2d.width, root.box2d.height), (300.0, 400.0));

    let (_, child) = node(&doc, "child");
    assert_eq!(child.parent, Some(index));
    assert_eq!((child.box2d.x, child.box2d.y), (40.0, 60.0));
}

#[test]
fn a_childs_box_is_relative_to_its_parent() {
    // Figma's absoluteBoundingBox is page-absolute; Document's Box2D is
    // parent-relative intent. The overflow child is the sharpest case: it sits
    // at an absolute x of -28 inside a parent at 32, so it must land at -60.
    let doc = lowered();
    let (_, child) = node(&doc, "overflow-child");

    assert_eq!((child.box2d.x, child.box2d.y), (-60.0, -30.0));
    assert_eq!((child.box2d.width, child.box2d.height), (520.0, 180.0));
}

#[test]
fn a_clipping_frame_carries_the_clip_intent() {
    let doc = lowered();
    let (clip_index, clip_frame) = node(&doc, "clip-frame");
    let (_, child) = node(&doc, "overflow-child");

    assert!(clip_frame.paint.as_ref().expect("has paint").clip);
    assert_eq!(child.parent, Some(clip_index));
}

#[test]
fn all_three_stroke_aligns_lower() {
    // absoluteRenderBounds differs from absoluteBoundingBox for CENTER and
    // OUTSIDE by exactly the stroke expansion. It is a *result*, so P1 says
    // the lowering must never read it — the box plus the align is the intent.
    let doc = lowered();

    for (name, align) in [
        ("stroke-inside", StrokeAlign::Inside),
        ("stroke-center", StrokeAlign::Center),
        ("stroke-outside", StrokeAlign::Outside),
    ] {
        let (_, n) = node(&doc, name);
        let stroke = n
            .paint
            .as_ref()
            .unwrap()
            .entry
            .stroke
            .expect("has a stroke");

        assert_eq!(stroke.align, align, "{name}");
        assert_eq!(stroke.width, 8.0, "{name}");
        // The box is the authored one, not the render bounds.
        assert_eq!((n.box2d.width, n.box2d.height), (200.0, 140.0), "{name}");
    }
}

#[test]
fn an_unstroked_frame_gets_no_stroke() {
    // strokeWeight is 1 on every node in the fixture, stroked or not.
    let doc = lowered();
    let (_, n) = node(&doc, "fill-solid");

    assert!(n.paint.as_ref().unwrap().entry.stroke.is_none());
}

#[test]
fn both_corner_forms_lower() {
    let doc = lowered();

    let (_, uniform) = node(&doc, "corners-uniform");
    assert_eq!(
        uniform.paint.as_ref().unwrap().entry.corners,
        CornerRadii {
            top_left: 16.0,
            top_right: 16.0,
            bottom_right: 16.0,
            bottom_left: 16.0,
        },
    );

    let (_, per_corner) = node(&doc, "corners-per-corner");
    assert_eq!(
        per_corner.paint.as_ref().unwrap().entry.corners,
        CornerRadii {
            top_left: 0.0,
            top_right: 24.0,
            bottom_right: 4.0,
            bottom_left: 48.0,
        },
    );
}

#[test]
fn drop_and_inner_shadows_lower_into_the_paint_entry() {
    // Un-pins the DROP_SHADOW/INNER_SHADOW refusal (debt #144): the shadow
    // parameters lower into the paint entry in Figma's effect order, and the
    // node is no diagnostic at all. `spread` absent lowers to zero.
    let file = document(serde_json::json!({
        "name": "card",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        "effects": [
            {
                "type": "DROP_SHADOW",
                "visible": true,
                "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 0.25 },
                "offset": { "x": 0.0, "y": 4.0 },
                "radius": 8.0,
                "spread": 1.0,
            },
            {
                "type": "INNER_SHADOW",
                "visible": true,
                "color": { "r": 0.1, "g": 0.1, "b": 0.1, "a": 0.5 },
                "offset": { "x": 2.0, "y": 2.0 },
                "radius": 4.0,
            },
        ],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("a shadowed frame lowers");
    assert!(
        diagnostics.is_empty(),
        "drop and inner shadows are no diagnostic: {diagnostics:?}",
    );

    let (_, card) = node(&doc, "card");
    let shadows = &card.paint.as_ref().unwrap().entry.shadows;
    assert_eq!(shadows.len(), 2);

    assert_eq!(shadows[0].kind, ShadowKind::Drop);
    assert_eq!(shadows[0].offset, Vec2 { x: 0.0, y: 4.0 });
    assert_eq!(shadows[0].blur, 8.0);
    assert_eq!(shadows[0].spread, 1.0);
    assert_eq!(
        shadows[0].color,
        Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.25
        },
    );

    assert_eq!(shadows[1].kind, ShadowKind::Inner);
    assert_eq!(shadows[1].offset, Vec2 { x: 2.0, y: 2.0 });
    assert_eq!(shadows[1].blur, 4.0);
    assert_eq!(shadows[1].spread, 0.0, "an absent spread lowers to zero");
}

#[test]
fn a_hidden_shadow_does_not_lower() {
    // A hidden effect is skipped, like a hidden paint (P4: not a silent drop —
    // a hidden effect casts nothing in Figma either).
    let file = document(serde_json::json!({
        "name": "card",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        "effects": [{
            "type": "DROP_SHADOW",
            "visible": false,
            "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 0.25 },
            "offset": { "x": 0.0, "y": 4.0 },
            "radius": 8.0,
        }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new()).expect("lowers");
    assert!(diagnostics.is_empty());
    let (_, card) = node(&doc, "card");
    assert!(
        card.paint.as_ref().unwrap().entry.shadows.is_empty(),
        "a hidden shadow lowers to nothing",
    );
}

#[test]
fn a_shadow_with_an_advanced_blend_lowers_normal_and_warns_under_full() {
    // The intended degrade for a non-NORMAL shadow blend mode mirrors a
    // paint blend mode: under Profile::Full the effect is out-of-profile
    // vocabulary that degrades, so the shadow still lowers (drawn NORMAL —
    // the painter has no blend-mode vocabulary) AND an AdvancedBlendMode
    // warning fires, so the drop-to-NORMAL is never silent (P4). Under
    // Profile::Core the same construct is an error and blocks the document,
    // so the shadow never renders.
    let root = serde_json::json!({
        "name": "card",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        "effects": [{
            "type": "DROP_SHADOW",
            "visible": true,
            "blendMode": "MULTIPLY",
            "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 0.25 },
            "offset": { "x": 0.0, "y": 4.0 },
            "radius": 8.0,
        }],
    });

    // Under Full: the shadow lowers and a warning comes back with it.
    let (doc, diagnostics) =
        lower(&document(root.clone()), Profile::Full, &BTreeMap::new()).expect("lowers under Full");
    let (_, card) = node(&doc, "card");
    assert_eq!(
        card.paint.as_ref().unwrap().entry.shadows.len(),
        1,
        "the shadow lowers (drawn NORMAL) even though its blend mode is dropped"
    );
    let blend: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "profile.advanced-blend-mode")
        .collect();
    let [warning] = blend[..] else {
        panic!("expected one advanced-blend-mode diagnostic, got {blend:?}");
    };
    assert_eq!(
        warning.severity,
        dashscene_validator::Severity::Warning,
        "under Full the blend mode degrades to a warning, not a block"
    );

    // Under Core: the same construct is an error, so the document does not
    // emit and the shadow never renders.
    let (_, core_diagnostics) =
        lower(&document(root), Profile::Core, &BTreeMap::new()).expect("lowers with diagnostics");
    assert!(
        core_diagnostics
            .iter()
            .any(|d| d.rule == "profile.advanced-blend-mode"
                && d.severity == dashscene_validator::Severity::Error),
        "under Core the advanced blend mode is an error that blocks the document"
    );
}

#[test]
fn a_shadow_with_no_color_is_refused() {
    // A shadow with no color has no meaning; refused by name (P4), the same
    // posture as a SOLID with no color.
    let file = document(serde_json::json!({
        "name": "card",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        "effects": [{ "type": "DROP_SHADOW", "visible": true, "radius": 8.0 }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new()).expect("lowers");
    assert_sole_unsupported(&doc, &diagnostics, "card", "a shadow with no color");
}

#[test]
fn all_four_gradient_kinds_lower() {
    let doc = lowered();

    for (name, kind) in [
        ("gradient-linear", GradientKind::Linear),
        ("gradient-radial", GradientKind::Radial),
        ("gradient-angular", GradientKind::Angular),
        ("gradient-diamond", GradientKind::Diamond),
    ] {
        let (_, n) = node(&doc, name);
        let Some(PaintKind::Gradient(g)) = &n.paint.as_ref().unwrap().entry.fill else {
            panic!("{name} did not lower to a gradient");
        };

        assert_eq!(g.kind, kind, "{name}");
        assert_eq!(g.stops.len(), 3, "{name}");
        // Figma calls it `position`, dashpaint calls it `offset`.
        assert_eq!(g.stops[1].offset, 0.5, "{name}");
    }
}

#[test]
fn the_gradient_handles_lower_in_figma_order() {
    // origin, primary-axis end, secondary-axis end — `dashpaint::Gradient`
    // stores Figma's convention verbatim, so the three must not be permuted.
    // The fixture's linear gradient has three distinct handles, so any swap
    // is visible.
    let doc = lowered();
    let (_, n) = node(&doc, "gradient-linear");

    let Some(PaintKind::Gradient(g)) = &n.paint.as_ref().unwrap().entry.fill else {
        panic!("gradient-linear did not lower to a gradient");
    };

    assert_eq!(g.handle_origin, Vec2 { x: 0.0, y: 0.5 });
    assert_eq!(g.handle_primary, Vec2 { x: 1.0, y: 0.5 });
    assert_eq!(g.handle_secondary, Vec2 { x: 0.0, y: 1.0 });
}

#[test]
fn the_lowered_colors_are_the_fixture_colors() {
    // Channel order is the kind of mistake that survives every structural
    // assertion: a fill still lowers, a gradient still has three stops, and
    // the picture is simply the wrong color. So the numbers get pinned.
    let doc = lowered();

    let (_, solid) = node(&doc, "fill-solid");
    let Some(PaintKind::Solid { color }) = &solid.paint.as_ref().unwrap().entry.fill else {
        panic!("fill-solid did not lower to a solid fill");
    };
    assert_eq!(
        *color,
        Color {
            r: 0.2,
            g: 0.5,
            b: 0.85,
            a: 1.0,
        },
    );

    let (_, gradient) = node(&doc, "gradient-linear");
    let Some(PaintKind::Gradient(g)) = &gradient.paint.as_ref().unwrap().entry.fill else {
        panic!("gradient-linear did not lower to a gradient");
    };
    assert_eq!(
        g.stops[0].color,
        Color {
            r: 1.0,
            g: 0.85,
            b: 0.2,
            a: 1.0,
        },
    );
    assert_eq!(
        g.stops[2].color,
        Color {
            r: 0.2,
            g: 0.25,
            b: 0.7,
            a: 1.0,
        },
    );

    let (_, stroked) = node(&doc, "stroke-inside");
    let stroke = stroked
        .paint
        .as_ref()
        .unwrap()
        .entry
        .stroke
        .expect("has a stroke");
    assert_eq!(
        stroke.color,
        Color {
            r: 0.85,
            g: 0.25,
            b: 0.35,
            a: 1.0,
        },
    );
}

#[test]
fn a_paint_opacity_multiplies_the_lowered_alpha() {
    // Figma's paint `opacity` multiplies the color's alpha. No fixture paint
    // carries one, so this is synthetic — and dropping it would be a silent
    // drop (P4): the fill would simply be too opaque.
    let file = document(serde_json::json!({
        "name": "translucent",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{
            "type": "SOLID",
            "opacity": 0.5,
            "color": { "r": 0.2, "g": 0.4, "b": 0.6, "a": 0.8 },
        }],
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (_, n) = node(&doc, "translucent");

    let Some(PaintKind::Solid { color }) = &n.paint.as_ref().unwrap().entry.fill else {
        panic!("translucent did not lower to a solid fill");
    };
    assert_eq!(
        *color,
        Color {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 0.8 * 0.5,
        },
    );
}

#[test]
fn an_image_fill_resolves_through_the_caller_supplied_map() {
    let doc = lowered();
    let (_, n) = node(&doc, "image-fit");

    let Some(PaintKind::Image {
        image,
        scale_mode,
        transform,
        tile_scale,
    }) = &n.paint.as_ref().unwrap().entry.fill
    else {
        panic!("image-fit did not lower to an image fill");
    };

    assert_eq!(*scale_mode, ScaleMode::Fit);
    assert_eq!(doc.assets[*image as usize].bytes, IMAGE_PNG);
    // FIT carries neither a crop transform nor a tile scale.
    assert_eq!(*transform, None, "identity when Figma sends no transform");
    assert_eq!(*tile_scale, 1.0);
}

#[test]
fn a_cropped_image_fill_lowers_its_crop_transform() {
    // `scaleMode: CROP` carries an `imageTransform`: Figma's row-major 2x3
    // affine, `[[a, b, tx], [c, d, ty]]`. `dashpaint::PaintKind::Image`
    // already carries it, so dropping it would not be an expressiveness gap
    // — it would lower a cropped image to a *wrong* image, in silence (P4).
    //
    // The six components are all distinct, so a transposed or column-major
    // reading fails rather than coincidentally matching.
    let file = document(serde_json::json!({
        "name": "image-crop",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{
            "type": "IMAGE",
            "scaleMode": "CROP",
            "imageRef": IMAGE_REF,
            "imageTransform": [[0.5, 0.125, 0.25], [0.75, 2.0, 0.375]],
        }],
    }));

    let (doc, _) = lower(&file, Profile::Core, &images()).expect("the document lowers");
    let (_, n) = node(&doc, "image-crop");

    let Some(PaintKind::Image {
        scale_mode,
        transform,
        tile_scale,
        ..
    }) = &n.paint.as_ref().unwrap().entry.fill
    else {
        panic!("image-crop did not lower to an image fill");
    };

    assert_eq!(*scale_mode, ScaleMode::Crop);
    assert_eq!(
        *transform,
        Some(Mat23 {
            a: 0.5,
            b: 0.125,
            c: 0.75,
            d: 2.0,
            tx: 0.25,
            ty: 0.375,
        }),
    );
    assert_eq!(*tile_scale, 1.0, "a crop carries no tile scale");
}

#[test]
fn a_tiled_image_fill_lowers_its_tile_scale() {
    // `scaleMode: TILE` carries a `scalingFactor` — the tile magnification.
    // Defaulting it to 1.0 would silently retile the image at the wrong size.
    let file = document(serde_json::json!({
        "name": "image-tile",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{
            "type": "IMAGE",
            "scaleMode": "TILE",
            "imageRef": IMAGE_REF,
            "scalingFactor": 0.25,
        }],
    }));

    let (doc, _) = lower(&file, Profile::Core, &images()).expect("the document lowers");
    let (_, n) = node(&doc, "image-tile");

    let Some(PaintKind::Image {
        scale_mode,
        transform,
        tile_scale,
        ..
    }) = &n.paint.as_ref().unwrap().entry.fill
    else {
        panic!("image-tile did not lower to an image fill");
    };

    assert_eq!(*scale_mode, ScaleMode::Tile);
    assert_eq!(*tile_scale, 0.25);
    assert_eq!(*transform, None, "a tile carries no crop transform");
}

#[test]
fn two_nodes_sharing_an_image_ref_share_one_asset() {
    // The walk interns `imageRef` → image-table index. Without it, one asset
    // is decoded and stored once per referencing node.
    let file = document(serde_json::json!({
        "name": "gallery",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [
            {
                "name": "left",
                "type": "FRAME",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 50.0, "height": 50.0 },
                "fills": [{ "type": "IMAGE", "scaleMode": "FILL", "imageRef": IMAGE_REF }],
            },
            {
                "name": "right",
                "type": "FRAME",
                "absoluteBoundingBox": { "x": 50.0, "y": 0.0, "width": 50.0, "height": 50.0 },
                "fills": [{ "type": "IMAGE", "scaleMode": "FILL", "imageRef": IMAGE_REF }],
            },
        ],
    }));

    let (doc, _) = lower(&file, Profile::Core, &images()).expect("the document lowers");

    assert_eq!(doc.assets.len(), 1, "one imageRef is one asset");
    assert_eq!(image_index(&doc, "left"), 0);
    assert_eq!(
        image_index(&doc, "right"),
        image_index(&doc, "left"),
        "both nodes point at the same asset",
    );
}

/// The image-table index of the node's image fill.
fn image_index(doc: &dashc_wasm::Document, name: &str) -> u32 {
    let (_, n) = node(doc, name);
    let Some(PaintKind::Image { image, .. }) = &n.paint.as_ref().unwrap().entry.fill else {
        panic!("{name} did not lower to an image fill");
    };
    *image
}

#[test]
fn an_unresolved_image_ref_fails_loudly() {
    // The load gate rejects a zero-byte asset (asset.image-no-bytes), so the
    // lowering cannot invent one. Better a named error than a fabricated pixel.
    let empty = BTreeMap::new();
    let err = lower(&parse(V03_PAINT), Profile::Core, &empty).unwrap_err();

    let CompileError::UnresolvedImage { image_ref, path } = err else {
        panic!("expected an UnresolvedImage error");
    };
    assert_eq!(image_ref, IMAGE_REF);
    assert!(
        path.contains("image-fit"),
        "the error names the node: {path}"
    );
}

/// The three REJECT-band constructs of `effects-2025`, in document order, and
/// the node each one belongs to.
const EFFECTS_2025_DIAGNOSTICS: [(&str, &str); 3] = [
    ("profile.noise-or-texture-effect", "noise"),
    ("profile.noise-or-texture-effect", "texture"),
    ("profile.progressive-blur", "progressive-blur"),
];

#[test]
fn the_reject_fixture_triages_every_construct_as_an_error() {
    // The raw capture, auto-layout root included: since #140 the walk
    // lowers the root's flex intent and reaches the three effects the
    // fixture was authored to carry.
    let (_, diagnostics) = lower(&parse(EFFECTS_2025), Profile::Core, &images())
        .expect("the effects fixture lowers; its constructs are diagnosed, not fatal");

    // The count, not just the membership: a construct that stopped being
    // triaged at all would still satisfy a `contains` over the rest.
    assert_eq!(diagnostics.len(), EFFECTS_2025_DIAGNOSTICS.len());

    let rules: Vec<&str> = diagnostics.iter().map(|d| d.rule).collect();
    assert!(rules.contains(&"profile.noise-or-texture-effect"));
    assert!(rules.contains(&"profile.progressive-blur"));
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity == dashscene_validator::Severity::Error),
        "every construct in effects-2025 is REJECT-band",
    );
}

#[test]
fn each_diagnostic_points_at_its_own_node() {
    // A diagnostic's `at` is what an editor jumps to and what a waiver keys
    // on (issue #41). An off-by-one index sends both to the wrong layer, and
    // every other assertion in this file would still pass.
    let (doc, diagnostics) = lower(&parse(EFFECTS_2025), Profile::Core, &images())
        .expect("the effects fixture lowers; its constructs are diagnosed, not fatal");

    assert_eq!(diagnostics.len(), EFFECTS_2025_DIAGNOSTICS.len());

    for (diagnostic, (rule, name)) in diagnostics.iter().zip(EFFECTS_2025_DIAGNOSTICS) {
        assert_eq!(diagnostic.rule, rule, "{name}");

        let Location::Node(at) = &diagnostic.at else {
            panic!("{name}: a triaged construct is located at a node");
        };
        // The node's own DFS index — which is its rect-table index (docs/design/dashbuf.md).
        let (index, _) = node(&doc, name);
        assert_eq!(at.index, index, "{name}");
        assert_eq!(at.path, format!("/effects-2025/{name}"), "{name}");
    }
}

#[test]
fn a_clipping_frame_with_no_paint_keeps_its_clip_intent() {
    // A clipping frame that draws nothing still has to carry its clip: the
    // clip is intent, and losing it lets the children overflow. Every fixture
    // node has a fill, so this branch is only reachable synthetically.
    let file = document(serde_json::json!({
        "name": "clip-only",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (_, n) = node(&doc, "clip-only");

    let paint = n
        .paint
        .as_ref()
        .expect("a clipping frame keeps a paint entry, or the clip is lost");
    assert!(paint.clip);
    assert_eq!(
        paint.entry,
        PaintEntry::default(),
        "it draws nothing: no fill, no stroke, sharp corners",
    );
}

#[test]
fn a_frame_with_nothing_at_all_lowers_to_no_paint() {
    // The other half: a layout-only container draws nothing and clips nothing,
    // so it takes a rect-table slot and no paint.
    let file = document(serde_json::json!({
        "name": "layout-only",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (_, n) = node(&doc, "layout-only");

    assert_eq!(n.paint, None);
}

#[test]
fn a_second_visible_stroke_fails_loudly_rather_than_being_silently_dropped() {
    // `PaintEntry.stroke` is one `Option<Stroke>`; Figma's `strokes` is an
    // array. Stacking is a Document expressiveness gap, and taking the first
    // stroke and discarding the rest is the silent drop P4 forbids — the
    // sibling case of `more than one visible fill`.
    let file = document(serde_json::json!({
        "name": "two-strokes",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "strokes": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            { "type": "SOLID", "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } },
        ],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(
        &doc,
        &diagnostics,
        "two-strokes",
        "more than one visible stroke",
    );
}

#[test]
fn a_hidden_second_stroke_is_not_a_second_visible_stroke() {
    // The rejection counts *visible* strokes. A hidden one draws nothing, so
    // it is not a drop — rejecting it would refuse a file Figma renders fine.
    let file = document(serde_json::json!({
        "name": "one-visible-stroke",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "strokeWeight": 4.0,
        "strokeAlign": "CENTER",
        "strokes": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            { "type": "SOLID", "visible": false, "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } },
        ],
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (_, n) = node(&doc, "one-visible-stroke");

    let stroke = n
        .paint
        .as_ref()
        .unwrap()
        .entry
        .stroke
        .expect("the visible stroke lowers");
    assert_eq!(stroke.width, 4.0);
    assert_eq!(stroke.align, StrokeAlign::Center);
    assert_eq!(
        stroke.color,
        Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        "the visible stroke is the one that lowers, not the hidden one",
    );
}

#[test]
fn two_visible_fills_lower_bottom_to_top() {
    // Story C1 (debt #146): a plain frame's stacked fills lower in Figma's
    // array order instead of refusing — the first (bottom) becomes
    // `entry.fill`, the rest become `entry.extra_fills`, painted over it in
    // the same order. Mirrors the stacked-fills fixture's own shape (a solid
    // base, a semi-transparent gradient on top); each fill's own `opacity`
    // is already folded into its color/stops, same as a single fill.
    let file = document(serde_json::json!({
        "name": "two-fills",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            {
                "type": "SOLID",
                "opacity": 0.5,
                "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 },
            },
        ],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    assert!(
        diagnostics.is_empty(),
        "a stacked fill is not a diagnostic: {diagnostics:?}"
    );
    let (_, n) = node(&doc, "two-fills");
    let entry = &n.paint.as_ref().expect("the node paints").entry;

    assert_eq!(
        entry.fill,
        Some(PaintKind::Solid {
            color: Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        }),
        "the bottom fill lowers exactly as a single fill always has",
    );
    assert_eq!(
        entry.extra_fills,
        vec![PaintKind::Solid {
            color: Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 0.5,
            },
        }],
        "the top fill's own opacity folds into its color, same as a lone fill's",
    );
}

#[test]
fn a_hidden_fill_amid_a_stack_is_not_a_third_visible_fill() {
    // The stack counts *visible* fills, same rule `fill_of`'s single-fill
    // case already applies: a hidden layer draws nothing, so it neither
    // becomes part of the stack nor blocks it.
    let file = document(serde_json::json!({
        "name": "hidden-amid-stack",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            {
                "type": "SOLID",
                "visible": false,
                "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0 },
            },
            { "type": "SOLID", "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } },
        ],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    assert!(diagnostics.is_empty());
    let (_, n) = node(&doc, "hidden-amid-stack");
    let entry = &n.paint.as_ref().expect("the node paints").entry;

    assert_eq!(
        entry.extra_fills,
        vec![PaintKind::Solid {
            color: Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        }],
        "the hidden middle fill is skipped, not stacked",
    );
}

#[test]
fn an_unsupported_fill_within_a_stack_is_still_refused_by_name() {
    // Stacking a visible fill is no longer itself a blocker, but a fill
    // whose own kind has no lowering still is — P4 does not relax just
    // because the fill sits in a stack.
    let file = document(serde_json::json!({
        "name": "stack-with-a-pattern",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            { "type": "PATTERN" },
        ],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(
        &doc,
        &diagnostics,
        "stack-with-a-pattern",
        "a PATTERN paint",
    );
}

#[test]
fn a_rotated_node_fails_loudly_rather_than_silently_dropping_the_rotation() {
    // Document has no rotation vocabulary and no Construct variant for it, so a
    // rotated node cannot become a diagnostic — P4 forbids lowering it as
    // though it were axis-aligned. Neither fixture carries a rotated node
    // (Figma omits `rotation` when it is zero), so this is synthetic.
    let file = document(serde_json::json!({
        "name": "rotated",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "rotation": 0.25,
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(&doc, &diagnostics, "rotated", "node rotation");
}

#[test]
fn a_box_outline_mask_lowers_to_a_mask() {
    // v0.8 (story #44) un-pinned box outline masks (debt #143): a geometry
    // mask on a box shape lowers into `Node.mask`.
    let file = document(serde_json::json!({
        "name": "masked",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "isMask": true,
        "maskType": "OUTLINE",
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the mask lowers");
    assert!(
        diagnostics.iter().all(|d| d.rule != "figma.unsupported"),
        "a box outline mask is no longer refused: {diagnostics:?}",
    );
    let (_, n) = node(&doc, "masked");
    assert!(n.mask, "the mask node lowered as a mask");
}

#[test]
fn an_absent_mask_type_lowers_as_the_geometric_default() {
    // A synthetic mask node with no maskType lowers as a box mask.
    let file = document(serde_json::json!({
        "name": "masked",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "isMask": true,
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the mask lowers");
    let (_, n) = node(&doc, "masked");
    assert!(n.mask);
}

#[test]
fn an_alpha_mask_is_refused_by_name() {
    // M6: a soft alpha mask has no hard box-clip lowering, so it refuses
    // rather than lowering as an opaque stencil (a silent drop of the fade).
    let file = document(serde_json::json!({
        "name": "alpha-masked",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "isMask": true,
        "maskType": "ALPHA",
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(
        &doc,
        &diagnostics,
        "alpha-masked",
        "an alpha mask (a soft mask has no hard box-clip lowering)",
    );
}

#[test]
fn a_luminance_mask_is_refused_by_name() {
    // M6: a luminance mask is a soft mask; refuse it by name.
    let file = document(serde_json::json!({
        "name": "luminance-masked",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "isMask": true,
        "maskType": "LUMINANCE",
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(&doc, &diagnostics, "luminance-masked", "a luminance mask");
}

#[test]
fn a_text_node_used_as_a_mask_is_refused_by_name() {
    // M6: a text node's shape is its letterforms, not a box, so a text mask
    // cannot lower as a rounded-box stencil (a silent drop of the shape).
    let file = document(serde_json::json!({
        "name": "text-masked",
        "type": "TEXT",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "characters": "Hi",
        "isMask": true,
        "style": { "fontFamily": "Inter", "fontSize": 12.0, "fontWeight": 400 },
        "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(
        &doc,
        &diagnostics,
        "text-masked",
        "a text node used as a mask (letterforms are not a box)",
    );
}

#[test]
fn a_node_opacity_lowers_to_group_opacity() {
    // v0.8 (story #44) un-pinned node opacity (debt #143): it lowers into
    // `Node.opacity` rather than refusing.
    let file = document(serde_json::json!({
        "name": "translucent-group",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "opacity": 0.4,
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the opacity lowers");
    assert!(
        diagnostics.iter().all(|d| d.rule != "figma.unsupported"),
        "node opacity is no longer refused: {diagnostics:?}",
    );
    let (_, n) = node(&doc, "translucent-group");
    assert_eq!(n.opacity, 0.4);
}

#[test]
fn a_hidden_node_lowers_and_keeps_its_index() {
    // v0.8 (story #44) un-pinned hidden nodes (debt #143): a hidden node
    // lowers with `visible = false` (Prop::Visible → Display::None) and
    // keeps its DFS index instead of forcing a refusal.
    let file = document(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 20.0, "height": 20.0 },
        "children": [{
            "name": "toggled-off",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "visible": false,
        }],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the hidden node lowers");
    assert!(
        diagnostics.iter().all(|d| d.rule != "figma.unsupported"),
        "a hidden node is no longer refused: {diagnostics:?}",
    );
    let (index, n) = node(&doc, "toggled-off");
    assert!(!n.visible, "the hidden node lowered as not visible");
    assert_eq!(index, 1, "and kept its DFS index rather than being dropped");
}

#[test]
fn a_hidden_node_does_not_break_its_visible_siblings() {
    // Under partial-emit and otherwise, a hidden node lowers in place — it is
    // never omitted from the tree (P1: the document carries the node, not
    // just its resolved visual result, so a later staged `set_prop(Visible,
    // true)` can un-hide it without re-lowering). Confirms a sibling after
    // the hidden node lowers normally, with no diagnostic and no index
    // disruption.
    let file = document(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 20.0, "height": 20.0 },
        "children": [
            {
                "name": "toggled-off",
                "type": "FRAME",
                "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
                "visible": false,
            },
            {
                "name": "after-the-hidden-node",
                "type": "FRAME",
                "absoluteBoundingBox": { "x": 10.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            },
        ],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("the file lowers");
    assert!(
        diagnostics.iter().all(|d| d.rule != "figma.unsupported"),
        "a hidden node and its sibling both lower cleanly: {diagnostics:?}",
    );
    let (hidden_index, hidden) = node(&doc, "toggled-off");
    assert!(!hidden.visible, "the hidden node lowered as not visible");
    let (sibling_index, sibling) = node(&doc, "after-the-hidden-node");
    assert!(
        sibling.visible,
        "the sibling after the hidden node lowers as visible"
    );
    assert_eq!(hidden_index, 1, "the hidden node keeps its DFS index");
    assert_eq!(
        sibling_index, 2,
        "the sibling's DFS index is not shifted by the hidden node before it"
    );
}

#[test]
fn an_auto_layout_child_never_bakes_the_solved_position() {
    // The surviving ground of
    // docs/decisions/figma-auto-layout-refused-on-two-grounds.md: inside an
    // auto-layout frame, absoluteBoundingBox is what Figma's flex solver
    // computed. The lowering carries the *intent* — mode, gap, padding — and
    // the child's solved position lowers as zeros, never as a fixed offset
    // that would look right at exactly one size (P1).
    let file = document(serde_json::json!({
        "name": "column",
        "type": "FRAME",
        "layoutMode": "VERTICAL",
        "itemSpacing": 24.0,
        "paddingLeft": 16.0,
        "paddingTop": 16.0,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 200.0 },
        "children": [{
            "name": "row-item",
            "type": "FRAME",
            // Not authored: 16 and 40 are where Figma's solver put it. The
            // 68×30 extent is authored — the child's sizing is fixed.
            "absoluteBoundingBox": { "x": 16.0, "y": 40.0, "width": 68.0, "height": 30.0 },
        }],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("auto-layout lowers since #140");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let (_, column) = node(&doc, "column");
    let container = column
        .container
        .as_ref()
        .expect("the column is a flex container");
    assert_eq!(container.gap, 24.0);
    assert_eq!(
        (container.padding.left, container.padding.top),
        (16.0, 16.0)
    );
    assert_eq!(
        (container.padding.right, container.padding.bottom),
        (0.0, 0.0),
        "Figma omits a zero padding edge",
    );

    let (_, item) = node(&doc, "row-item");
    assert_eq!(
        (item.box2d.x, item.box2d.y),
        (0.0, 0.0),
        "the solved position must not be written in as intent (P1)",
    );
    assert_eq!(
        (item.box2d.width, item.box2d.height),
        (68.0, 30.0),
        "the fixed extent is authored intent, and it lowers",
    );
}

#[test]
fn a_layout_mode_of_none_is_not_auto_layout() {
    // Figma writes `NONE` on a frame whose auto-layout is off — the state
    // every frame in v03-paint.json is in. A gate that fired on the field's
    // mere presence would refuse the emission fixture itself.
    let file = document(serde_json::json!({
        "name": "fixed",
        "type": "FRAME",
        "layoutMode": "NONE",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (index, fixed) = node(&doc, "fixed");

    assert_eq!(index, 0);
    assert_eq!(
        fixed.container, None,
        "NONE is a passthrough, not a flex container"
    );
}

#[test]
fn a_non_basic_stroke_fails_loudly_rather_than_lowering_as_a_solid_one() {
    // dashpaint::Stroke is solid and uniform: one color, one width, one align.
    // A DASHED stroke has nothing to lower into, and the drop is invisible in
    // the output — the frame simply gets a plain solid stroke of the right
    // color, which is exactly the silent drop P4 forbids.
    //
    // The field shape is pinned by v03-paint.json, where every frame carries
    // `complexStrokeProperties: {"strokeType": "BASIC"}`; no captured fixture
    // has a non-BASIC one, so the value here is synthetic.
    let file = document(serde_json::json!({
        "name": "dashed-border",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "complexStrokeProperties": { "strokeType": "DASHED" },
        "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(&doc, &diagnostics, "dashed-border", "a DASHED stroke");
}

#[test]
fn a_stroke_dash_pattern_fails_loudly_rather_than_lowering_as_a_continuous_stroke() {
    // The shape is the captured one: corpus/figma-fixtures/lowering-variant-topology.json
    // carries `"strokeDashes": [10, 5]` alongside `{"strokeType": "BASIC"}`.
    //
    // That pairing is the whole reason this gate exists. Figma expresses a dash
    // pattern WITHOUT changing the stroke type, so the complexStrokeProperties
    // gate never fires on a dashed stroke — this gate is the only thing between
    // a dashed border and a silently continuous repaint (P4). The BASIC stroke
    // type below is therefore load-bearing, not incidental.
    let file = document(serde_json::json!({
        "name": "dotted-border",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "complexStrokeProperties": { "strokeType": "BASIC" },
        "strokeDashes": [10.0, 5.0],
        "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(&doc, &diagnostics, "dotted-border", "a dashed stroke");
}

#[test]
fn a_basic_stroke_with_an_empty_dash_pattern_lowers_normally() {
    // The other side of both stroke gates, and the reason they are not written
    // as `is_some()` checks: BASIC is what every stroked frame in
    // v03-paint.json carries, and Figma writes `strokeDashes: null` — an empty
    // array means the same — for a continuous stroke. Refusing either would
    // refuse the emission fixture.
    let file = document(serde_json::json!({
        "name": "plain-border",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "complexStrokeProperties": { "strokeType": "BASIC" },
        "strokeDashes": [],
        "strokeWeight": 3.0,
        "strokeAlign": "OUTSIDE",
        "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
    }));

    let (doc, _) = lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers");
    let (_, n) = node(&doc, "plain-border");

    let stroke = n
        .paint
        .as_ref()
        .unwrap()
        .entry
        .stroke
        .expect("a BASIC stroke lowers");
    assert_eq!(stroke.width, 3.0);
    assert_eq!(stroke.align, StrokeAlign::Outside);
}

#[test]
fn a_node_with_no_box_is_named_in_the_error() {
    // The root is the case that used to lose its name: its box was read once
    // before the walk started, with no path to report it under.
    let file = document(serde_json::json!({
        "name": "boxless-root",
        "type": "FRAME",
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("an unsupported construct is diagnosed, not fatal");
    assert_sole_unsupported(
        &doc,
        &diagnostics,
        "/boxless-root",
        "node boxless-root has no absoluteBoundingBox",
    );
}

#[test]
fn the_fixture_compiles_loads_and_renders() {
    // Story #139's acceptance criterion, end to end.
    let (bytes, report) =
        compile_figma(V03_PAINT, Profile::Core, &images()).expect("v03-paint compiles");

    assert!(report.is_empty(), "the paint fixture is entirely NOW-band");

    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);

    let scene = arena.committed();
    assert_eq!(scene.rects().len(), 14, "13 frames plus the root");

    let mut painter = SkiaPainter::new(960, 680);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    let png = painter.png_bytes();

    assert!(!png.is_empty(), "the fixture rasterizes");
    assert_eq!(&png[1..4], b"PNG", "and it is a PNG");
}

#[test]
fn emission_from_the_fixture_is_byte_reproducible() {
    // R7: same input → byte-identical document.
    let (first, _) = compile_figma(V03_PAINT, Profile::Core, &images()).unwrap();
    let (second, _) = compile_figma(V03_PAINT, Profile::Core, &images()).unwrap();

    assert_eq!(first, second, "emission is not deterministic");
}

#[test]
fn the_reject_fixture_is_refused_rather_than_emitted() {
    // effects-2025 is a DIAGNOSTIC fixture (corpus/figma-fixtures/README.md): everything in it is
    // REJECT-band, so under R6 it can never emit a .dsb. The report must name
    // each construct — never a silent drop (P4).
    let err = compile_figma(EFFECTS_2025, Profile::Core, &images())
        .expect_err("a REJECT-band document must never emit");

    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };

    assert!(report.has_errors());
    assert!(report.has("profile.noise-or-texture-effect"));
    assert!(report.has("profile.progressive-blur"));
}

#[test]
fn malformed_json_is_a_parse_error_not_a_panic() {
    let err = compile_figma("{ not json", Profile::Core, &images()).unwrap_err();
    assert!(matches!(err, CompileError::Parse(_)));
}

#[test]
fn a_warning_does_not_block_emission_and_comes_back_with_the_bytes() {
    // A plain LAYER_BLUR is LATER-band under profile:core: a warning, not an
    // error, so R6 does not block it. Dropping it from the success return
    // would be the silent drop P4 forbids — the case a fixture that triages
    // entirely clean (v03-paint) cannot exercise.
    let json = document_json(serde_json::json!({
        "name": "blurred",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "effects": [{ "type": "LAYER_BLUR", "visible": true }],
    }))
    .to_string();

    let (bytes, report) = compile_figma(&json, Profile::Core, &BTreeMap::new())
        .expect("a warning does not block emission");

    assert!(!bytes.is_empty());
    assert!(
        report.has("profile.layer-blur"),
        "the warning must come back with the bytes, not be dropped",
    );
}

#[test]
fn a_load_gate_only_error_still_blocks_emission() {
    // The lowering copies strokeWeight straight through with no sign check
    // (`Walk::stroke_of`) — it is triage-clean, and story #400's image gate
    // has nothing to say about a stroke. Only the load gate's
    // paint.stroke.invalid-width rule catches a negative width, so this case
    // only fails if compile_figma actually merges the load gate's report
    // into the one it returns.
    //
    // (This used to be an empty-byte image asset: asset.image-no-bytes was
    // the load-gate-only rule the empty asset's bytes tripped. Story #400's
    // image-identification gate now catches an empty payload earlier, as
    // figma.image-unknown-signature, before the load gate ever sees it — see
    // the rejection tests in tests/image_id_gate.rs — so this test switched
    // to a trigger the new gate has no opinion on.)
    let json = document_json(serde_json::json!({
        "name": "negative-stroke",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "strokes": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        "strokeWeight": -5.0,
        "strokeAlign": "INSIDE",
    }))
    .to_string();

    let err = compile_figma(&json, Profile::Core, &BTreeMap::new())
        .expect_err("a negative stroke width fails the load gate");

    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };
    assert!(report.has("paint.stroke.invalid-width"));
}

/// The Deno importer does not scan the file for `imageRef`s — it asks. This is
/// why: the answer comes from the same module that consumes it, so the resolver
/// and the lowering cannot disagree about where an `imageRef` lives.
#[test]
fn image_refs_names_every_ref_the_lowering_demands() {
    let refs =
        dashc_wasm::figma::image_refs(&parse(V03_PAINT)).expect("the fixture has a root frame");

    assert_eq!(refs, vec![IMAGE_REF.to_string()]);
}

#[test]
fn image_refs_refuses_a_file_with_no_root_frame() {
    let file: FigmaFile = serde_json::from_value(serde_json::json!({
        "document": { "id": "0:0", "name": "Document", "type": "DOCUMENT", "children": [] }
    }))
    .expect("the synthetic document parses");

    assert!(matches!(
        dashc_wasm::figma::image_refs(&file),
        Err(CompileError::Unsupported { .. })
    ));
}

#[test]
fn a_rectangle_lowers_as_a_box_leaf() {
    // A RECTANGLE is a paint-bearing leaf: no children, its authored box and
    // fill lower through the same paint path a FRAME uses.
    let file = document(serde_json::json!({
        "name": "rect",
        "type": "RECTANGLE",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 80.0, "height": 40.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        "cornerRadius": 8.0,
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("a rectangle lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "RECTANGLE must not be an unsupported node type: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, rect) = node(&doc, "rect");
    assert_eq!((rect.box2d.width, rect.box2d.height), (80.0, 40.0));
    assert!(rect.paint.is_some(), "the rectangle carries its fill");
    assert_eq!(
        rect.paint.as_ref().unwrap().entry.corners,
        CornerRadii {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
        "the rectangle's corner radius lowers into its paint entry",
    );
    assert!(
        rect.container.is_none(),
        "a rectangle is a leaf, not a container"
    );
}

#[test]
fn a_section_lowers_as_an_absolute_container_with_offset_children() {
    // A SECTION has no layoutMode, so it is an absolute container: its child's
    // position is the authored offset (child bbox - section bbox), and the
    // child carries its authored size (absent sizing outside auto-layout is
    // Fixed).
    let file = document(serde_json::json!({
        "name": "section",
        "type": "SECTION",
        "absoluteBoundingBox": { "x": 100.0, "y": 100.0, "width": 400.0, "height": 300.0 },
        "children": [{
            "name": "card",
            "type": "RECTANGLE",
            "absoluteBoundingBox": { "x": 150.0, "y": 180.0, "width": 80.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0 } }],
        }],
    }));

    let (doc, diagnostics) =
        lower(&file, Profile::Core, &BTreeMap::new()).expect("a section lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "SECTION must not be unsupported: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, card) = node(&doc, "card");
    // Offset from the section origin (150-100, 180-100), authored size preserved.
    assert_eq!((card.box2d.x, card.box2d.y), (50.0, 80.0));
    assert_eq!((card.box2d.width, card.box2d.height), (80.0, 40.0));
}

#[test]
fn a_group_lowers_as_an_absolute_container_carrying_opacity() {
    // GROUP is an inert container: it lowers as an absolute container, and its
    // own opacity rides the existing node-opacity machinery (v0.8, #44).
    let file = document(serde_json::json!({
        "name": "group",
        "type": "GROUP",
        "opacity": 0.5,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 120.0 },
        "children": [{
            "name": "member",
            "type": "RECTANGLE",
            "absoluteBoundingBox": { "x": 10.0, "y": 20.0, "width": 40.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } }],
        }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new()).expect("a group lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "GROUP must not be unsupported: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, group) = node(&doc, "group");
    assert_eq!(group.opacity, 0.5, "group opacity is carried, not dropped");
    let (_, member) = node(&doc, "member");
    assert_eq!((member.box2d.x, member.box2d.y), (10.0, 20.0));
}

#[test]
fn a_group_with_an_advanced_blend_mode_is_diagnosed_not_dropped() {
    // The P4 guard: an inert container is passed through, but a GROUP carrying
    // visual intent the schema cannot express (a non-NORMAL blend mode) is a
    // named diagnostic, never a silent accept. The diagnostic must be the
    // blend mode, never the node-type refusal GROUP would have gotten before
    // it joined the allowlist.
    let file = document(serde_json::json!({
        "name": "blended-group",
        "type": "GROUP",
        "blendMode": "MULTIPLY",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [],
    }));

    let (_doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    // GROUP is admitted: the diagnostic is the blend mode, not a refused type.
    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "GROUP must be admitted, not refused as an unsupported node type: {:?}",
        common::unsupported(&diagnostics),
    );
    // The blend-mode intent is named (P4), not silently dropped.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "profile.advanced-blend-mode"),
        "the advanced blend mode must surface a named diagnostic: {diagnostics:?}",
    );
}

#[test]
fn a_section_with_hidden_contents_is_diagnosed() {
    // sectionContentsHidden hides a section's children in Figma. We do not model
    // that, so a section carrying it is a named diagnostic (P4), not a silent
    // render of children that should be hidden.
    let file = document(serde_json::json!({
        "name": "collapsed",
        "type": "SECTION",
        "sectionContentsHidden": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 120.0 },
        "children": [{
            "name": "hidden-card",
            "type": "RECTANGLE",
            "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 40.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        }],
    }));

    let (_doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    assert!(
        common::unsupported(&diagnostics)
            .iter()
            .any(|(_, what)| what.contains("sectionContentsHidden")),
        "a hidden-contents section must be diagnosed: {:?}",
        common::unsupported(&diagnostics),
    );
}

#[test]
fn a_vector_with_geometry_lowers_to_a_baked_field() {
    // Story B1: a VECTOR node with a fill and `fillGeometry` bakes into an
    // MSDF field carried on its paint entry as a coverage mask.
    let file = document(serde_json::json!({
        "name": "vec",
        "type": "VECTOR",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        "fillGeometry": [{
            "path": "M 0 0 L 10 0 L 10 10 L 0 10 Z",
            "windingRule": "NONZERO",
        }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "a fielded vector lowers clean: {:?}",
        common::unsupported(&diagnostics),
    );
    assert_eq!(doc.vector_atlases.len(), 1, "one packed atlas");
    assert_eq!(doc.vector_shapes.len(), 1, "one baked shape");
    let paint = doc.nodes[0]
        .paint
        .as_ref()
        .expect("the vector carries a paint entry");
    assert_eq!(
        paint.shape_field,
        Some(0),
        "the paint entry references the baked shape",
    );
}

#[test]
fn a_vector_without_geometry_is_refused_by_name() {
    // A VECTOR with ink but no path geometry (a geometry-free fetch, or a
    // genuinely degenerate node) has nothing to bake, so it is refused by name
    // (P4), never emitted as an empty box.
    let file = document(serde_json::json!({
        "name": "vec",
        "type": "VECTOR",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
    }));

    let (_doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    assert!(
        common::unsupported(&diagnostics)
            .iter()
            .any(|(_, what)| what == "a vector with no path geometry"),
        "an ink-bearing geometry-less vector is refused by name: {:?}",
        common::unsupported(&diagnostics),
    );
}

/// The `.dsb` the Deno importer must reproduce byte for byte.
///
/// This is one half of story #17's acceptance criterion. The other half is
/// `importers/figma/src/wasm_test.ts`, which asserts the same bytes come back
/// through the wasm ABI. Neither test can see the other's toolchain, so the
/// golden is what makes "byte-identical to dashc-native output" checkable in
/// two CI jobs that never meet.
///
/// Regenerate with `UPDATE_GOLDENS=1` after a deliberate change to emission or
/// to the captured fixture, review the diff, and commit. A missing golden is a
/// failure, never an auto-create: CI on a clean checkout must fail loudly
/// rather than mint its own truth (`goldens/README.md`).
#[test]
fn the_fixture_emits_the_golden_dsb() {
    let (bytes, report) =
        compile_figma(V03_PAINT, Profile::Core, &images()).expect("the paint fixture compiles");
    assert!(report.is_empty(), "v03-paint emits clean");

    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden has a parent"))
            .expect("the goldens directory is writable");
        std::fs::write(&path, &bytes).expect("the golden is writable");
        return;
    }

    let golden = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering` to create it",
            path.display(),
        )
    });

    assert_eq!(
        bytes,
        golden,
        "emission drifted from the golden ({} bytes vs {}). If this is intended, \
         regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
        bytes.len(),
        golden.len(),
    );
}

// -- Emit policy: Strict refuses, Partial skips-and-warns (story S0-impl) ----

/// A FRAME whose only problem is a VECTOR child that cannot bake: an
/// omission-class gap (`figma.unsupported`, "a vector with no path geometry" —
/// the child has ink but no `fillGeometry`, story B1). Strict refuses the
/// file; Partial omits the VECTOR and emits the frame with a warning.
fn frame_with_vector_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "glyph",
            "type": "VECTOR",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }]
        }],
    }))
}

#[test]
fn strict_refuses_a_file_with_an_unsupported_construct() {
    let json = frame_with_vector_child().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_emits_the_frame_and_warns_on_the_skipped_vector() {
    let json = frame_with_vector_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document");
    assert!(!bytes.is_empty(), "a document is emitted");
    // The gap survives as a WARNING (P4), never dropped.
    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    assert_eq!(warnings.len(), 1, "one figma.unsupported for the VECTOR");
    assert_eq!(warnings[0].severity, Severity::Warning);
    // The frame is present, the VECTOR is omitted: exactly one node.
    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    assert_eq!(
        arena.committed().rects().len(),
        1,
        "the frame is present, the VECTOR omitted",
    );
}

// -- Partial never approximates, and never ships zero content (story S0-impl) -

/// A noise effect is REJECT-band: shipping the node without it would be an
/// approximation, so Partial must still refuse — the never-approximate line.
fn frame_with_noise_effect() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "effects": [{ "type": "NOISE", "visible": true }],
    }))
}

#[test]
fn partial_still_refuses_a_reject_band_construct() {
    let json = frame_with_noise_effect().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    );
    assert!(
        matches!(result, Err(CompileError::Diagnostics(_))),
        "a REJECT-band construct is never shipped approximated, even under Partial",
    );
}

/// A FRAME whose FRAME child carries a backdrop blur — the shape the real
/// corpus fixture has (`corpus/figma-fixtures/backdrop-blur.json`: a frosted
/// panel over a background).
///
/// This used to hold a VECTOR child, because the case it pinned was the
/// whole-node omission a backdrop blur forced under profile:core. Story #393
/// made backdrop blur core vocabulary and removed that omission. The VECTOR
/// case is covered too, by
/// `a_backdrop_blur_on_a_baked_vector_is_kept_not_dropped` below — it is the
/// shape the hero's frosted panel actually has, so it is the one that must not
/// regress.
fn frame_with_backdrop_blurred_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "panel",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 0.2 } }],
            "effects": [{ "type": "BACKGROUND_BLUR", "visible": true, "radius": 100.0 }],
        }],
    }))
}

#[test]
fn a_backdrop_blur_lowers_under_both_policies_and_keeps_its_radius() {
    // Story #393 moved backdrop blur into the NOW band
    // (docs/decisions/backdrop-blur-is-core-vocabulary.md). It used to be an
    // error under Profile::Core, so Partial omitted the whole node and Strict
    // refused the file; the two tests that pinned those behaviours lived here
    // and this one replaces them. The construct lowers now, so neither policy
    // has anything to report and the node keeps its blur.
    for policy in [EmitPolicy::Partial, EmitPolicy::Strict] {
        let json = frame_with_backdrop_blurred_child().to_string();
        let images = BTreeMap::new();
        let (bytes, report) =
            compile_figma_with_bindings_and_policy(&json, Profile::Core, &images, &[], policy)
                .expect("a backdrop blur is core vocabulary and compiles");
        assert!(
            !report.has(dashc_wasm::figma::rule::UNSUPPORTED),
            "{policy:?}: nothing is omitted, so nothing is reported: {:?}",
            report.diagnostics(),
        );

        // The radius survives the whole path: Figma effect -> dashc lowering
        // -> paint pool -> document -> load. A blur that lowered to the right
        // node with the wrong radius would render, and would be wrong.
        let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
        let mut arena = Arena::new();
        load_document(&document, &payloads, &mut arena);
        let scene = arena.committed();
        assert_eq!(
            scene.rects().len(),
            2,
            "{policy:?}: the frame and the blurred child both lower",
        );
        let blurs: Vec<_> = scene
            .rects()
            .iter()
            .filter_map(|rect| scene.paints().get(rect.paint))
            .flat_map(|entry| entry.blurs.iter())
            .collect();
        assert_eq!(
            blurs.len(),
            1,
            "{policy:?}: exactly one blur, got {blurs:?}"
        );
        assert_eq!(blurs[0].kind, dashscene_core::BlurKind::Backdrop);
        assert_eq!(blurs[0].radius, 100.0);
    }
}

/// The hero's own shape: a baked VECTOR carrying `BACKGROUND_BLUR`.
///
/// This is a regression test with a specific history. Story #393's first draft
/// hardcoded `blurs: Vec::new()` on the baked-vector paint entry, which made
/// the blur vanish with no diagnostic — the silent drop P4 forbids — on the one
/// node the story exists to fix. `docs/decisions/baked-vector-msdf-field.md`
/// records that lowering the hero's vectors is what unmasked a `VECTOR`
/// carrying `BACKGROUND_BLUR` radius 100 in the first place.
#[test]
fn a_backdrop_blur_on_a_baked_vector_is_kept_not_dropped() {
    let json = document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "bg",
            "type": "VECTOR",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 0.7 } }],
            "fillGeometry": [{
                "path": "M 0 0 L 10 0 L 10 10 L 0 10 Z",
                "windingRule": "NONZERO",
            }],
            "effects": [{ "type": "BACKGROUND_BLUR", "visible": true, "radius": 100.0 }],
        }],
    }))
    .to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    )
    .expect("a blurred vector is core vocabulary and compiles");
    assert!(
        !report.has(dashc_wasm::figma::rule::UNSUPPORTED),
        "nothing is omitted: {:?}",
        report.diagnostics(),
    );

    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    let scene = arena.committed();
    let blurs: Vec<_> = scene
        .rects()
        .iter()
        .filter_map(|rect| scene.paints().get(rect.paint))
        .flat_map(|entry| entry.blurs.iter())
        .collect();
    assert_eq!(blurs.len(), 1, "the vector keeps its blur, got {blurs:?}");
    assert_eq!(blurs[0].kind, dashscene_core::BlurKind::Backdrop);
    assert_eq!(blurs[0].radius, 100.0);
}

/// A blur on a TEXT node is named, not dropped. A text node builds no
/// `PaintEntry`, so it has nowhere to carry one; before story #393 the
/// construct's own error verdict caught this, and removing that verdict would
/// have made it silent.
#[test]
fn a_blur_on_a_text_node_is_a_named_blocker() {
    let json = document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "label",
            "type": "TEXT",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 40.0, "height": 10.0 },
            "characters": "hi",
            "style": { "fontFamily": "Inter", "fontSize": 12.0 },
            "effects": [{ "type": "BACKGROUND_BLUR", "visible": true, "radius": 8.0 }],
        }],
    }))
    .to_string();
    let images = BTreeMap::new();
    let (_bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial emit returns a document with the text node skipped");
    let named = report
        .diagnostics()
        .iter()
        .any(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED && d.message.contains("blur"));
    assert!(
        named,
        "the gap is named, never silent: {:?}",
        report.diagnostics(),
    );
}

/// A canvas holding only a COMPONENT resolves to no paintable content:
/// figma.no-content, a zero-node .dsb that panics a loader. Always an error.
#[test]
fn partial_still_refuses_a_no_content_file() {
    let json = serde_json::json!({
        "document": {
            "name": "Document", "type": "DOCUMENT",
            "children": [{
                "name": "Page 1", "type": "CANVAS",
                "children": [{ "name": "def", "type": "COMPONENT",
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 } }],
            }],
        },
    })
    .to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

// -- Unknown paint-vocabulary values degrade to a diagnostic, never a parse
// crash (story S2). Each fixture nests the unsupported construct one level
// under a valid root, mirroring frame_with_vector_child: the root itself must
// still lower, or Partial's "never ship zero content" gate (figma.no-content)
// would fire instead of the warning under test.

/// A child FRAME whose only fill is a Figma paint type this file did not
/// model until now (`PATTERN`, a repeating source-node tile — story S2 says
/// diagnose, never model).
fn frame_with_pattern_fill_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "swatch",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "PATTERN" }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_paint_type_naming_it() {
    let json = frame_with_pattern_fill_child().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_paint_type_naming_it() {
    let json = frame_with_pattern_fill_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown paint type");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message, "a PATTERN paint is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}

/// A child FRAME whose image fill carries `scaleMode: STRETCH` — a
/// non-uniform scale-to-fill Figma supports that `dashpaint::ScaleMode` does
/// not (story S2 says diagnose, never model). Needs a resolvable imageRef, or
/// paint_kind fails with UnresolvedImage before ever reaching the scaleMode
/// match.
fn frame_with_stretch_image_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "photo",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "IMAGE", "scaleMode": "STRETCH", "imageRef": IMAGE_REF }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_scale_mode_naming_it() {
    let json = frame_with_stretch_image_child().to_string();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images(),
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_scale_mode_naming_it() {
    let json = frame_with_stretch_image_child().to_string();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images(),
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown scaleMode");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message, "an image scaleMode STRETCH is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}

/// A child FRAME whose stroke carries a strokeAlign Figma might add that this
/// file has never modeled (synthetic — no captured fixture has one).
fn frame_with_unknown_stroke_align_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "odd-border",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "strokeAlign": "MIDDLE",
            "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_stroke_align_naming_it() {
    let json = frame_with_unknown_stroke_align_child().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_stroke_align_naming_it() {
    let json = frame_with_unknown_stroke_align_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown strokeAlign");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message, "a MIDDLE stroke alignment is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}
