//! The v0.3 compile pipeline, end to end (story #16):
//!
//!     Document → emit → validate → .dsb → dashscene-core → Skia painter
//!
//! The load half of the pipeline. The Figma front end — Figma REST JSON →
//! lower → Document — now exists and is exercised separately, in
//! `tests/figma_lowering.rs` (story #139); this file starts from a hand-built
//! `Document` and exercises everything downstream of the lowering: emission,
//! the load gate, `dashscene-core`'s load path, and the Skia painter.
//!
//! The claim these tests defend is that **loading adds no semantics**: a
//! scene loaded from a `.dsb` is indistinguishable from the same scene
//! staged by hand through the producer API — same rects, same paint pool,
//! same pixels.

// The lib target is `dashc_wasm`, not `dashc`: on wasm32 both the lib and the
// bin compile to a same-named output file, which cargo flags as a collision
// (see the crate manifest).
use dashc_wasm::{Box2D, Document, Node, Paint, compile, emit};
use dashpaint::{
    Color, GlyphRunTable, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat,
    PaintEntry, PaintKind, Painter, ScaleMode, Stroke, StrokeAlign, Vec2,
};
use dashscene_core::{Arena, Prop, load_document};
use dashscene_skia::SkiaPainter;

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

/// A 1x1 red PNG — the smallest asset that actually decodes.
fn png_pixel() -> Vec<u8> {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    PNG.to_vec()
}

fn gradient() -> PaintKind {
    PaintKind::Gradient(Gradient {
        kind: GradientKind::Linear,
        handle_origin: Vec2 { x: 0.0, y: 0.0 },
        handle_primary: Vec2 { x: 1.0, y: 0.0 },
        handle_secondary: Vec2 { x: 0.0, y: 1.0 },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: BLUE,
            },
        ],
    })
}

fn stroke() -> Stroke {
    Stroke {
        width: 2.0,
        align: StrokeAlign::Inside,
        color: BLUE,
    }
}

fn corners() -> dashpaint::CornerRadii {
    dashpaint::CornerRadii {
        top_left: 4.0,
        top_right: 0.0,
        bottom_right: 4.0,
        bottom_left: 0.0,
    }
}

/// A document exercising the v0.3 vocabulary: a clipping frame with a
/// gradient fill, an overflowing solid child, a stroked+rounded child, and
/// an image-filled child.
fn v03_document() -> Document {
    let mut doc = Document::new();
    let image = doc.push_image(ImageAsset {
        format: ImageFormat::Png,
        bytes: png_pixel(),
    });

    let frame = doc.push(Node {
        name: Some("frame".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: Some(gradient()),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
            },
            clip: true,
        }),
        ..Node::default()
    });

    // Overflows the frame, so the clip is observable.
    doc.push(Node {
        name: Some("overflow".to_owned()),
        parent: Some(frame),
        box2d: Box2D {
            x: 24.0,
            y: 24.0,
            width: 40.0,
            height: 40.0,
        },
        paint: Some(Paint {
            entry: PaintEntry::solid(RED),
            clip: false,
        }),
        ..Node::default()
    });

    doc.push(Node {
        name: Some("stroked".to_owned()),
        parent: Some(frame),
        box2d: Box2D {
            x: 2.0,
            y: 2.0,
            width: 16.0,
            height: 16.0,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: None,
                stroke: Some(stroke()),
                corners: corners(),
            },
            clip: false,
        }),
        ..Node::default()
    });

    doc.push(Node {
        name: Some("photo".to_owned()),
        parent: Some(frame),
        box2d: Box2D {
            x: 2.0,
            y: 22.0,
            width: 10.0,
            height: 10.0,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: Some(PaintKind::Image {
                    image,
                    scale_mode: ScaleMode::Fill,
                    transform: None,
                    tile_scale: 1.0,
                }),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
            },
            clip: false,
        }),
        ..Node::default()
    });

    doc
}

/// The same scene, staged by hand through the producer API. This is the
/// oracle: `load_document` must be indistinguishable from it.
fn v03_by_hand(arena: &mut Arena) {
    let image = {
        let mut txn = arena.open();
        let i = txn.add_image(ImageAsset {
            format: ImageFormat::Png,
            bytes: png_pixel(),
        });
        txn.commit();
        i
    };

    let mut txn = arena.open();

    let frame = txn.add_node(None, Some("frame"));
    txn.set_prop(frame, Prop::X(0.0));
    txn.set_prop(frame, Prop::Y(0.0));
    txn.set_prop(frame, Prop::Width(40.0));
    txn.set_prop(frame, Prop::Height(40.0));
    txn.set_prop(frame, Prop::FillWith(gradient()));
    txn.set_prop(frame, Prop::Clip(true));

    let overflow = txn.add_node(Some(frame), Some("overflow"));
    txn.set_prop(overflow, Prop::X(24.0));
    txn.set_prop(overflow, Prop::Y(24.0));
    txn.set_prop(overflow, Prop::Width(40.0));
    txn.set_prop(overflow, Prop::Height(40.0));
    txn.set_prop(overflow, Prop::Fill(RED));

    let stroked = txn.add_node(Some(frame), Some("stroked"));
    txn.set_prop(stroked, Prop::X(2.0));
    txn.set_prop(stroked, Prop::Y(2.0));
    txn.set_prop(stroked, Prop::Width(16.0));
    txn.set_prop(stroked, Prop::Height(16.0));
    txn.set_prop(stroked, Prop::Stroke(stroke()));
    txn.set_prop(
        stroked,
        Prop::Corners {
            top_left: 4.0,
            top_right: 0.0,
            bottom_right: 4.0,
            bottom_left: 0.0,
        },
    );

    let photo = txn.add_node(Some(frame), Some("photo"));
    txn.set_prop(photo, Prop::X(2.0));
    txn.set_prop(photo, Prop::Y(22.0));
    txn.set_prop(photo, Prop::Width(10.0));
    txn.set_prop(photo, Prop::Height(10.0));
    txn.set_prop(
        photo,
        Prop::FillWith(PaintKind::Image {
            image,
            scale_mode: ScaleMode::Fill,
            transform: None,
            tile_scale: 1.0,
        }),
    );

    txn.commit();
}

/// Compiles, loads, and returns the arena the document produced.
fn load(doc: &Document) -> Arena {
    let bytes = compile(doc).expect("the v0.3 document validates");
    let document = dashbuf::root_as_document(&bytes).expect("valid buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    arena
}

fn render(arena: &Arena) -> Vec<u8> {
    let scene = arena.committed();
    let mut painter = SkiaPainter::new(40, 40);
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
fn a_document_loads_into_the_same_scene_it_was_built_from() {
    // The load path adds no semantics: same rects, same paint pool, same
    // clip regions. This is what lets the importer and the DSL be two
    // producers of one runtime (E1, story #48) rather than two runtimes.
    let loaded = load(&v03_document());

    let mut by_hand = Arena::new();
    v03_by_hand(&mut by_hand);

    let a = loaded.committed();
    let b = by_hand.committed();

    assert_eq!(a.rects(), b.rects(), "rect tables differ");
    assert_eq!(a.paints(), b.paints(), "paint pools differ");
    assert_eq!(a.clips(), b.clips(), "clip tables differ");
    assert_eq!(a.images(), b.images(), "image tables differ");
}

#[test]
fn a_loaded_document_renders_the_same_pixels_as_the_hand_built_scene() {
    // The end of the story's acceptance criterion: ".dsb → loads in
    // dashscene-core → renders via the Skia painter".
    let loaded = load(&v03_document());

    let mut by_hand = Arena::new();
    v03_by_hand(&mut by_hand);

    assert_eq!(
        render(&loaded),
        render(&by_hand),
        "a loaded document must rasterize identically to the scene it encodes"
    );
}

#[test]
fn emission_is_byte_reproducible() {
    // R7: same input → byte-identical document. Hashing, signing, and CI all
    // depend on it. The paint pool is the one place that could vary, and it
    // interns in first-use DFS order rather than by hash-map iteration.
    let first = emit(&v03_document());
    let second = emit(&v03_document());
    assert_eq!(first, second, "emission is not deterministic");
}

#[test]
fn nodes_sharing_a_style_share_one_pool_entry() {
    let mut doc = Document::new();
    let paint = Paint {
        entry: PaintEntry::solid(RED),
        clip: false,
    };
    for i in 0..3 {
        doc.push(Node {
            name: Some(format!("box{i}")),
            parent: None,
            box2d: Box2D {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            paint: Some(paint.clone()),
            ..Node::default()
        });
    }

    let bytes = compile(&doc).expect("validates");
    let document = dashbuf::root_as_document(&bytes).expect("valid buffer");
    assert_eq!(
        document.paints().expect("a paint pool").len(),
        1,
        "three nodes with one style must dedup to one pool entry"
    );
}

#[test]
fn two_nodes_that_differ_only_in_clip_do_not_share_a_pool_entry() {
    // The schema pools clip with the paint entry (`Paint.clip`), while the
    // arena carries clip as node intent (issue #97). So a style and a clip
    // flag together key the pool — sharing an entry here would silently make
    // one of the two nodes clip when it should not.
    let mut doc = Document::new();
    for clip in [false, true] {
        doc.push(Node {
            name: Some(format!("box-{clip}")),
            parent: None,
            box2d: Box2D {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            paint: Some(Paint {
                entry: PaintEntry::solid(RED),
                clip,
            }),
            ..Node::default()
        });
    }

    let bytes = compile(&doc).expect("validates");
    let document = dashbuf::root_as_document(&bytes).expect("valid buffer");
    assert_eq!(document.paints().expect("a paint pool").len(), 2);
}

#[test]
fn an_invalid_document_is_refused_rather_than_emitted() {
    // R6/P4: an error blocks the document, never a silent drop. A gradient
    // with no stops is exactly the case the painter would panic on.
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("broken".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: Some(PaintKind::Gradient(Gradient {
                    kind: GradientKind::Linear,
                    handle_origin: Vec2 { x: 0.0, y: 0.0 },
                    handle_primary: Vec2 { x: 1.0, y: 0.0 },
                    handle_secondary: Vec2 { x: 0.0, y: 1.0 },
                    stops: Vec::new(),
                })),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
            },
            clip: false,
        }),
        ..Node::default()
    });

    let report = compile(&doc).expect_err("an empty gradient must block emission");
    assert!(
        report.has(dashscene_validator::rule::GRADIENT_NO_STOPS),
        "{report}"
    );
}

#[test]
fn image_indices_are_remapped_when_loading_into_a_non_empty_arena() {
    // A document's image indices are 0..n, but an arena that already holds
    // assets hands out different ones. Assuming the two coincide would
    // repaint the second document's nodes with the first document's assets.
    let mut arena = Arena::new();

    let bytes = compile(&v03_document()).expect("validates");
    let document = dashbuf::root_as_document(&bytes).expect("valid buffer");
    load_document(&document, &mut arena);
    load_document(&document, &mut arena);

    let scene = arena.committed();
    assert_eq!(
        scene.images().len(),
        2,
        "both loads staged their own copy of the asset"
    );

    // Every image fill in the pool must point at an asset that exists, and
    // the second document's must point at the second asset — index 1, not 0.
    let referenced: Vec<u32> = (0..scene.paints().len())
        .filter_map(|i| {
            match &scene
                .paints()
                .get(dashpaint::PaintIndex(i as u32))
                .expect("in range")
                .fill
            {
                Some(PaintKind::Image { image, .. }) => Some(*image),
                _ => None,
            }
        })
        .collect();

    assert_eq!(
        referenced,
        vec![0, 1],
        "the second load's image fill must be remapped onto its own asset"
    );
}

#[test]
fn flex_intent_round_trips_through_the_document() {
    // What dashc's emitter writes into the two v0.2 flex tables, core's
    // loader must read back as the same intent — this is the seam the flex
    // lowering (story #140) hangs on. Every value is deliberately
    // non-default, so a field that stopped being written (or started being
    // read from the wrong table) fails on the value, not on presence.
    use dashc_wasm::{EdgeInsets, LayoutConstraints, LayoutContainer};

    let mut doc = Document::new();
    let row = doc.push(Node {
        name: Some("row".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 50.0,
        },
        container: Some(LayoutContainer {
            mode: dashc_wasm::LayoutMode::Horizontal,
            gap: 8.0,
            padding: EdgeInsets {
                left: 1.0,
                top: 2.0,
                right: 3.0,
                bottom: 4.0,
            },
            main_align: dashc_wasm::MainAxisAlign::SpaceBetween,
            cross_align: dashc_wasm::CrossAxisAlign::Center,
        }),
        ..Node::default()
    });
    doc.push(Node {
        name: Some("stretchy".to_owned()),
        parent: Some(row),
        box2d: Box2D::default(),
        constraints: Some(LayoutConstraints {
            sizing_h: dashc_wasm::AxisSizing::Fill,
            sizing_v: dashc_wasm::AxisSizing::Hug,
            min_width: Some(10.0),
            max_width: Some(100.0),
            min_height: None,
            max_height: Some(40.0),
            margin: EdgeInsets {
                left: -16.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
        }),
        ..Node::default()
    });

    let bytes = compile(&doc).expect("validates");
    let document = dashbuf::root_as_document(&bytes).expect("valid buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);

    let root = arena.roots()[0];
    let row_layout = arena.layout(root);
    assert_eq!(row_layout.mode, dashscene_core::LayoutMode::Horizontal);
    assert_eq!(row_layout.gap, 8.0);
    assert_eq!(
        (
            row_layout.padding.left,
            row_layout.padding.top,
            row_layout.padding.right,
            row_layout.padding.bottom,
        ),
        (1.0, 2.0, 3.0, 4.0),
    );
    assert_eq!(
        row_layout.main_align,
        dashscene_core::MainAxisAlign::SpaceBetween
    );
    assert_eq!(
        row_layout.cross_align,
        dashscene_core::CrossAxisAlign::Center
    );

    let child = arena.children(root)[0];
    let child_layout = arena.layout(child);
    assert_eq!(child_layout.sizing_h, dashscene_core::AxisSizing::Fill);
    assert_eq!(child_layout.sizing_v, dashscene_core::AxisSizing::Hug);
    assert_eq!(child_layout.min_width, Some(10.0));
    assert_eq!(child_layout.max_width, Some(100.0));
    assert_eq!(child_layout.min_height, None);
    assert_eq!(child_layout.max_height, Some(40.0));
    assert_eq!(child_layout.margin.left, -16.0);
}
