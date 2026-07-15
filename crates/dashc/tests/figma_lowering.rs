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

use dashc_wasm::compile_figma;
use dashc_wasm::figma::rest::{FigmaFile, PaintTag};
use dashc_wasm::figma::{CompileError, lower};
use dashpaint::{
    Color, CornerRadii, GradientKind, ImageAsset, ImageFormat, Mat23, PaintEntry, PaintKind,
    Painter, ScaleMode, StrokeAlign, Vec2,
};
use dashscene_core::{Arena, load_document};
use dashscene_skia::SkiaPainter;
use dashscene_validator::{Location, Profile};

/// The designated input for this story (corpus/figma-fixtures/manifest.json).
const V03_PAINT: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");

/// The diagnostic fixture. It can never emit a `.dsb`, but not because its
/// constructs are REJECT-band — the compile stops earlier than that: the root
/// frame carries `layoutMode: HORIZONTAL`, and auto-layout is refused before
/// the triage gate runs. Reaching the three effects it was authored to carry
/// therefore needs [`effects_2025_without_auto_layout`].
const EFFECTS_2025: &str = include_str!("../../../corpus/figma-fixtures/effects-2025.json");

fn parse(json: &str) -> FigmaFile {
    serde_json::from_str(json).expect("the captured fixture parses")
}

/// `effects-2025.json` with the root frame's auto-layout removed, and nothing
/// else touched.
///
/// The captured root is a `layoutMode: HORIZONTAL` frame, which the walk now
/// refuses outright — see
/// `the_reject_fixtures_auto_layout_root_is_refused`. Its three REJECT-band
/// effects are the point of the fixture, though, so they are exercised through
/// this derived document rather than being hand-written: the effects that
/// reach the triage table are still the captured ones (P5), and only the
/// construct that blocks the walk is dropped.
fn effects_2025_without_auto_layout() -> serde_json::Value {
    let mut file: serde_json::Value =
        serde_json::from_str(EFFECTS_2025).expect("the captured fixture parses");

    file["document"]["children"][0]["children"][0]
        .as_object_mut()
        .expect("the fixture's first canvas has a root frame")
        .remove("layoutMode")
        .expect("the fixture's root frame is auto-layout, which is what this strips");

    file
}

/// [`effects_2025_without_auto_layout`], parsed.
fn effects_2025() -> FigmaFile {
    serde_json::from_value(effects_2025_without_auto_layout())
        .expect("the derived fixture still parses")
}

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
    assert_eq!(fill.kind, PaintTag::Image);
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

/// The node named `name`, and its index in the rect table.
fn node<'a>(doc: &'a dashc_wasm::Document, name: &str) -> (u32, &'a dashc_wasm::Node) {
    doc.nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.name.as_deref() == Some(name))
        .map(|(i, n)| (i as u32, n))
        .unwrap_or_else(|| panic!("no lowered node named {name}"))
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
    assert_eq!(doc.images[*image as usize].bytes, IMAGE_PNG);
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

    assert_eq!(doc.images.len(), 1, "one imageRef is one asset");
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
    let (_, diagnostics) = lower(&effects_2025(), Profile::Core, &images())
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
    let (doc, diagnostics) = lower(&effects_2025(), Profile::Core, &images())
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

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(
        path.contains("two-strokes"),
        "the error names the node: {path}"
    );
    assert_eq!(what, "more than one visible stroke");
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
fn a_second_visible_fill_fails_loudly_rather_than_being_silently_dropped() {
    // The same P4 rule on the fill side.
    let file = document(serde_json::json!({
        "name": "two-fills",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [
            { "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } },
            { "type": "SOLID", "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } },
        ],
    }));

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(
        path.contains("two-fills"),
        "the error names the node: {path}"
    );
    assert_eq!(what, "more than one visible fill");
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

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(path.contains("rotated"), "the error names the node: {path}");
    assert_eq!(what, "node rotation");
}

#[test]
fn a_mask_node_fails_loudly_rather_than_silently_dropping_the_mask() {
    // Document has no mask vocabulary and no Construct variant for it, so a mask
    // node cannot become a diagnostic — P4 forbids silently painting it as an
    // ordinary frame. Neither fixture carries a mask node, so this is
    // synthetic.
    let file = document(serde_json::json!({
        "name": "masked",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "isMask": true,
    }));

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(path.contains("masked"), "the error names the node: {path}");
    assert_eq!(what, "a mask node");
}

#[test]
fn an_auto_layout_frame_fails_loudly_rather_than_baking_the_solver_result() {
    // Two violations at once, and either one alone would be enough to refuse.
    //
    // P4: Document has no flex vocabulary, so the mode, the itemSpacing and the
    // padding below have no field to lower into and no Construct to triage
    // onto — passing the frame through drops all four in silence.
    //
    // P1: worse, the drop is not visible in the output. Figma's flex solver is
    // what computed the child's absoluteBoundingBox, so a walk that ignored
    // layoutMode would lower that box as a fixed rect and produce a document
    // that renders correctly at exactly one size — a solver result written in
    // as though it were authored intent.
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
            // Not authored: 16 and 40 are where Figma's solver put it.
            "absoluteBoundingBox": { "x": 16.0, "y": 40.0, "width": 68.0, "height": 30.0 },
        }],
    }));

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(path.contains("column"), "the error names the node: {path}");
    assert_eq!(what, "auto-layout (VERTICAL)");
}

#[test]
fn the_reject_fixtures_auto_layout_root_is_refused() {
    // The captured root frame of effects-2025 is `layoutMode: HORIZONTAL`, so
    // the gate is not hypothetical — a fixture already in the corpus reaches
    // it. Before the gate existed this file lowered clean, with its flex
    // intent gone and its children's solver-computed boxes written in as
    // fixed rects.
    let err = lower(&parse(EFFECTS_2025), Profile::Core, &images()).unwrap_err();

    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert_eq!(path, "/effects-2025");
    assert_eq!(what, "auto-layout (HORIZONTAL)");
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
    let (index, _) = node(&doc, "fixed");

    assert_eq!(index, 0);
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

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(
        path.contains("dashed-border"),
        "the error names the node: {path}"
    );
    assert_eq!(what, "a DASHED stroke");
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

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert!(
        path.contains("dotted-border"),
        "the error names the node: {path}"
    );
    assert_eq!(what, "a dashed stroke");
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

    let err = lower(&file, Profile::Core, &BTreeMap::new()).unwrap_err();
    let CompileError::Unsupported { path, what } = err else {
        panic!("expected Unsupported, got {err:?}");
    };
    assert_eq!(path, "/boxless-root");
    assert_eq!(what, "node boxless-root has no absoluteBoundingBox");
}

#[test]
fn the_fixture_compiles_loads_and_renders() {
    // Story #139's acceptance criterion, end to end.
    let (bytes, report) =
        compile_figma(V03_PAINT, Profile::Core, &images()).expect("v03-paint compiles");

    assert!(report.is_empty(), "the paint fixture is entirely NOW-band");

    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);

    let scene = arena.committed();
    assert_eq!(scene.rects().len(), 14, "13 frames plus the root");

    let mut painter = SkiaPainter::new(960, 680);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
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
    let json = effects_2025_without_auto_layout().to_string();
    let err = compile_figma(&json, Profile::Core, &images())
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
    // The import gate resolves an imageRef against the caller's map, but
    // never inspects the resolved asset's byte content. An empty asset
    // triages clean and only the load gate's asset.image-no-bytes rule
    // catches it — so this case only fails if compile_figma actually merges
    // the load gate's report into the one it returns.
    let json = document_json(serde_json::json!({
        "name": "empty-image",
        "type": "FRAME",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
        "fills": [{ "type": "IMAGE", "scaleMode": "FILL", "imageRef": IMAGE_REF }],
    }))
    .to_string();

    let empty_asset = BTreeMap::from([(
        IMAGE_REF.to_string(),
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
    )]);

    let err = compile_figma(&json, Profile::Core, &empty_asset)
        .expect_err("an empty image asset fails the load gate");

    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };
    assert!(report.has("asset.image-no-bytes"));
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
