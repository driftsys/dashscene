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

mod common;

// The lib target is `dashc_wasm`, not `dashc`: on wasm32 both the lib and the
// bin compile to a same-named output file, which cargo flags as a collision
// (see the crate manifest).
use dashc_wasm::{
    BindingChannel, Box2D, Document, Easing, LoopTrack, Node, Paint, PaintEntry, Placeholder,
    PropTransition, TransitionSpec, VariantMember, VariantOverride, VariantSet, VariantTransition,
    VariantValue, compile, emit,
};
use dashpaint::{
    Blur, BlurKind, Color, FillSpec, GlyphRunTable, Gradient, GradientKind, GradientStop,
    ImageAsset, ImageFill, ImageFormat, Mat23, Painter, ScaleMode, Shadow, ShadowKind, StopRange,
    Stroke, StrokeAlign, Vec2,
};
use dashscene_core::{Arena, Prop, load_document};
use dashscene_engine::TaffySolver;
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

fn gradient() -> FillSpec {
    FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Linear,
            handle_origin: Vec2 { x: 0.0, y: 0.0 },
            handle_primary: Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: Vec2 { x: 0.0, y: 1.0 },
            // The table assigns the range on intern; this fixture has no
            // table (story #578).
            stops: StopRange::NONE,
        },
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
    }
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

/// A paint entry with nothing but a solid fill — the
/// `dashpaint::PaintEntry::solid` shorthand story #578 removed (it belongs
/// on `PaintTable` now, since only a table can assign the fill an index);
/// recreated here for the many fixtures in this file that want exactly that.
fn solid_entry(color: Color) -> PaintEntry {
    PaintEntry {
        fill: Some(FillSpec::Solid { color }),
        ..PaintEntry::default()
    }
}

/// A document exercising the v0.3 vocabulary: a clipping frame with a
/// gradient fill, an overflowing solid child, a stroked+rounded child, and
/// an image-filled child.
fn v03_document() -> Document {
    let mut doc = Document::new();
    // One asset, content-addressed: `push_asset` returns the index of an
    // existing entry when the bytes repeat, so the index this returns is the
    // asset table's, not a running counter (story #107).
    let image = doc.push_asset(dashc_wasm::Asset {
        format: ImageFormat::Png,
        kind: dashc_wasm::AssetKind::Image,
        bytes: png_pixel(),
        width: 1,
        height: 1,
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
                extra_fills: Vec::new(),
            },
            clip: true,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
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
            entry: solid_entry(RED),
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
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
                extra_fills: Vec::new(),
            },
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
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
                fill: Some(FillSpec::Image(ImageFill {
                    image,
                    scale_mode: ScaleMode::Fill,
                    transform: Mat23::IDENTITY,
                    tile_scale: 1.0,
                })),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
                extra_fills: Vec::new(),
            },
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
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
        Prop::FillWith(FillSpec::Image(ImageFill {
            image,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        })),
    );

    txn.commit();
}

/// Compiles, loads, and returns the arena the document produced.
fn load(doc: &Document) -> Arena {
    let bytes = compile(doc).expect("the v0.3 document validates");
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
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

/// A shadow-carrying document: one node whose paint entry stacks a drop and
/// an inner shadow. Every prior determinism/byte/round-trip test uses the
/// shadow-less V03 fixture, so this covers the emit path for the v0.8
/// vocabulary (R7/E6).
fn shadowed_document() -> Document {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("card".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        paint: Some(Paint {
            entry: solid_entry(RED),
            clip: false,
            shape_field: None,
            shadows: vec![
                Shadow {
                    kind: ShadowKind::Drop,
                    offset: Vec2 { x: 0.0, y: 4.0 },
                    blur: 8.0,
                    spread: 1.0,
                    color: Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.25,
                    },
                },
                Shadow {
                    kind: ShadowKind::Inner,
                    offset: Vec2 { x: 2.0, y: 2.0 },
                    blur: 4.0,
                    spread: -1.0,
                    color: Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.1,
                        a: 0.5,
                    },
                },
            ],
            blurs: Vec::new(),
        }),
        ..Node::default()
    });
    doc
}

#[test]
fn emission_of_a_shadowed_document_is_byte_reproducible() {
    // R7 for the shadow vocabulary: same shadowed input → identical bytes.
    let first = emit(&shadowed_document());
    let second = emit(&shadowed_document());
    assert_eq!(first, second, "shadow emission is not deterministic");

    // And the bytes carry the shadows: emit actually writes the list, in
    // order, rather than dropping it.
    let document = dashbuf::root_as_document(&first).expect("valid buffer");
    let paints = document.paints().expect("paint pool present");
    let shadows = paints.get(0).shadows().expect("shadows present");
    assert_eq!(shadows.len(), 2);
    assert_eq!(shadows.get(0).kind(), dashbuf::ShadowKind::Drop);
    assert_eq!(shadows.get(0).blur(), 8.0);
    assert_eq!(shadows.get(1).kind(), dashbuf::ShadowKind::Inner);
    assert_eq!(shadows.get(1).spread(), -1.0);
}

/// One 20x20 node with the shared RED fill and whatever blur list is asked
/// for — the fixture the blur pool tests below vary. `blurs` empty is the
/// plain (pre-v0.11) entry.
fn blurred_node(name: &str, blurs: Vec<Blur>) -> Node {
    Node {
        name: Some(name.to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        paint: Some(Paint {
            entry: solid_entry(RED),
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs,
        }),
        ..Node::default()
    }
}

fn backdrop(radius: f32) -> Blur {
    Blur {
        kind: BlurKind::Backdrop,
        radius,
    }
}

#[test]
fn a_frosted_and_a_plain_entry_with_the_same_fill_are_two_pool_entries() {
    // Story #393: the emit pool's dedup key folds the blur list in. Every node
    // below carries the identical solid fill, so the blur is the only thing
    // the key can separate them by. A key that ignored it would intern all
    // five onto one pool entry, and every node would point at whichever style
    // the DFS walk reached first — the frosted panels would render flat, or
    // the plain one would render frosted.
    let mut doc = Document::new();
    doc.push(blurred_node("frosted-a", vec![backdrop(12.0)]));
    doc.push(blurred_node("frosted-b", vec![backdrop(12.0)]));
    doc.push(blurred_node("wider", vec![backdrop(24.0)]));
    doc.push(blurred_node(
        "layer",
        vec![Blur {
            kind: BlurKind::Layer,
            radius: 12.0,
        }],
    ));
    doc.push(blurred_node("plain", Vec::new()));

    let bytes = compile(&doc).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    assert_eq!(
        document.paints().expect("a paint pool").len(),
        4,
        "identical blurs dedup; radius, kind, and absence each earn an entry"
    );

    let nodes = document.nodes().expect("nodes present");
    let entry = |i: usize| nodes.get(i).paint_entry();
    assert_eq!(entry(0), entry(1), "an identical fill + blur dedups");
    assert_ne!(entry(0), entry(2), "a different radius is a distinct entry");
    assert_ne!(entry(0), entry(3), "a different kind is a distinct entry");
    assert_ne!(
        entry(0),
        entry(4),
        "a frosted entry never collapses onto the plain entry with the same fill"
    );
}

#[test]
fn a_blur_and_a_shadow_section_do_not_alias_each_other_in_the_pool_key() {
    // The pool key encodes `shadows` and `blurs` as a count word followed by
    // that many fixed-width element records, and the two sections are
    // adjacent. This pins that the framing stays unambiguous when one is
    // populated and the other empty: one blur and no shadow must not key the
    // same as one shadow and no blur.
    let mut doc = Document::new();
    doc.push(blurred_node("blurred", vec![backdrop(4.0)]));
    let mut shadowed = blurred_node("shadowed", Vec::new());
    shadowed.paint.as_mut().expect("the fixture paints").shadows = vec![Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 0.0 },
        blur: 4.0,
        spread: 0.0,
        color: RED,
    }];
    doc.push(shadowed);

    let bytes = compile(&doc).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    assert_eq!(document.paints().expect("a paint pool").len(), 2);
}

#[test]
fn emission_of_a_blurred_document_is_byte_reproducible_and_round_trips() {
    // R7 for the blur vocabulary: same blurred input → identical bytes.
    let doc = || {
        let mut doc = Document::new();
        doc.push(blurred_node(
            "frosted",
            vec![
                backdrop(12.0),
                Blur {
                    kind: BlurKind::Layer,
                    radius: 3.5,
                },
            ],
        ));
        doc
    };
    let first = emit(&doc());
    let second = emit(&doc());
    assert_eq!(first, second, "blur emission is not deterministic");

    // The bytes carry the blurs, in order, rather than dropping them.
    let bytes = compile(&doc()).expect("validates");
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    let paints = document.paints().expect("paint pool present");
    let blurs = paints.get(0).blurs().expect("blurs present");
    assert_eq!(blurs.len(), 2);
    assert_eq!(blurs.get(0).kind(), dashbuf::BlurKind::Backdrop);
    assert_eq!(blurs.get(0).radius(), 12.0);
    assert_eq!(blurs.get(1).kind(), dashbuf::BlurKind::Layer);
    assert_eq!(blurs.get(1).radius(), 3.5);

    // And the whole path holds: document → .dsb → load → arena → commit hands
    // boundary B the same list, in the same order, with the backdrop blur
    // still declaring that the node samples what is beneath it.
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    let scene = arena.committed();
    let entry = scene.paints().resolve(scene.rects()[0].paint);
    assert_eq!(
        scene.paints().blurs(entry),
        &[
            backdrop(12.0),
            Blur {
                kind: BlurKind::Layer,
                radius: 3.5,
            },
        ],
    );
    assert!(scene.paints().samples_backdrop(entry));
}

#[test]
fn a_blur_less_entry_omits_the_blurs_field_entirely() {
    // R7's other half: an empty blur list must write no field at all, not an
    // empty vector. A zero-length vector would still occupy a vtable slot and
    // change the bytes of every document written before v0.11 — which is what
    // would have broken the frozen `.dsb` fixture and the committed goldens.
    let bytes = compile(&shadowed_document()).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    let paint = document.paints().expect("paint pool present").get(0);
    assert!(
        paint.blurs().is_none(),
        "an empty blur list must be an absent field, not an empty vector"
    );
}

#[test]
fn nodes_sharing_a_style_share_one_pool_entry() {
    let mut doc = Document::new();
    let paint = Paint {
        entry: solid_entry(RED),
        clip: false,
        shape_field: None,
        shadows: Vec::new(),
        blurs: Vec::new(),
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
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
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
                entry: solid_entry(RED),
                clip,
                shape_field: None,
                shadows: Vec::new(),
                blurs: Vec::new(),
            }),
            ..Node::default()
        });
    }

    let bytes = compile(&doc).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
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
                fill: Some(FillSpec::Gradient {
                    gradient: Gradient {
                        kind: GradientKind::Linear,
                        handle_origin: Vec2 { x: 0.0, y: 0.0 },
                        handle_primary: Vec2 { x: 1.0, y: 0.0 },
                        handle_secondary: Vec2 { x: 0.0, y: 1.0 },
                        stops: StopRange::NONE,
                    },
                    stops: Vec::new(),
                }),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
                extra_fills: Vec::new(),
            },
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
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
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    load_document(&document, &payloads, &mut arena);
    load_document(&document, &payloads, &mut arena);

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
            let kind = scene
                .paints()
                .get(dashpaint::PaintIndex(i as u32))
                .expect("in range")
                .fill;
            match scene.paints().fill(kind) {
                dashpaint::Fill::Image(image_fill) => Some(image_fill.image),
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
    use dashc_wasm::{EdgeInsets, GridTrack, LayoutConstraints, LayoutContainer};

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
            // A plain H row carries no cross gap or grid tracks; the grid
            // subtree below exercises those v0.8 fields.
            cross_gap: None,
            grid_rows: Vec::new(),
            grid_columns: Vec::new(),
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
            ..LayoutConstraints::default()
        }),
        ..Node::default()
    });

    // A grid subtree exercises the v0.8 appends (story #264): the mode, the
    // cross gap, both track lists at distinct sizings, and a placed child's
    // anchor and spans. Every value is non-default, so a field written into
    // the wrong slot fails on the value.
    let grid = doc.push(Node {
        name: Some("grid".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 200.0,
        },
        container: Some(LayoutContainer {
            mode: dashc_wasm::LayoutMode::Grid,
            gap: 12.0,
            padding: EdgeInsets::default(),
            main_align: dashc_wasm::MainAxisAlign::Start,
            cross_align: dashc_wasm::CrossAxisAlign::Start,
            cross_gap: Some(20.0),
            // Three tracks per axis, so the placed child's non-default
            // anchor + span below stays inside the declared grid (the load
            // gate's grid.span-out-of-range rule, story #264).
            grid_rows: vec![
                GridTrack::Fixed(96.0),
                GridTrack::Fraction(2.0),
                GridTrack::Fixed(40.0),
            ],
            grid_columns: vec![
                GridTrack::Fraction(1.0),
                GridTrack::Fixed(160.0),
                GridTrack::Fraction(3.0),
            ],
        }),
        ..Node::default()
    });
    doc.push(Node {
        name: Some("placed".to_owned()),
        parent: Some(grid),
        box2d: Box2D::default(),
        constraints: Some(LayoutConstraints {
            sizing_h: dashc_wasm::AxisSizing::Fill,
            sizing_v: dashc_wasm::AxisSizing::Fill,
            // Anchor (row 1, col 0) with spans (2, 3): row 1+2 = 3 and
            // column 0+3 = 3 both reach exactly the third track.
            grid_row: Some(1),
            grid_column: Some(0),
            grid_row_span: 2,
            grid_column_span: 3,
            ..LayoutConstraints::default()
        }),
        ..Node::default()
    });

    let bytes = compile(&doc).expect("validates");
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);

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

    // The grid subtree round-trips its v0.8 fields (story #264).
    let grid_root = arena.roots()[1];
    let grid_layout = arena.layout(grid_root);
    assert_eq!(grid_layout.mode, dashscene_core::LayoutMode::Grid);
    assert_eq!(grid_layout.gap, 12.0);
    assert_eq!(grid_layout.cross_gap, Some(20.0));
    let (rows, columns) = arena.grid_tracks(grid_root);
    assert_eq!(
        rows,
        [
            dashscene_core::GridTrack::Fixed(96.0),
            dashscene_core::GridTrack::Fraction(2.0),
            dashscene_core::GridTrack::Fixed(40.0),
        ],
    );
    assert_eq!(
        columns,
        [
            dashscene_core::GridTrack::Fraction(1.0),
            dashscene_core::GridTrack::Fixed(160.0),
            dashscene_core::GridTrack::Fraction(3.0),
        ],
    );

    let placed = arena.children(grid_root)[0];
    let placed_layout = arena.layout(placed);
    assert_eq!(placed_layout.grid_row, Some(1));
    assert_eq!(placed_layout.grid_column, Some(0));
    assert_eq!(placed_layout.grid_row_span, 2);
    assert_eq!(placed_layout.grid_column_span, 3);
}

/// A one-node document whose only container carries the padding given, for
/// the byte-presence test below.
fn container_document(padding: dashc_wasm::EdgeInsets) -> Document {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("row".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        },
        container: Some(dashc_wasm::LayoutContainer {
            mode: dashc_wasm::LayoutMode::Horizontal,
            gap: 0.0,
            padding,
            main_align: dashc_wasm::MainAxisAlign::Start,
            cross_align: dashc_wasm::CrossAxisAlign::Start,
            cross_gap: None,
            grid_rows: Vec::new(),
            grid_columns: Vec::new(),
        }),
        ..Node::default()
    });
    doc
}

#[test]
fn a_default_padding_container_omits_the_field_a_non_default_one_writes_it() {
    // The other half of the "absent means zero insets" contract the schema
    // comment states for `LayoutContainer.padding` (issue #522): emit.rs
    // writes the field only when it differs from `EdgeInsets::default()`,
    // the same shape `a_blur_less_entry_omits_the_blurs_field_entirely`
    // above pins for the blur list. No committed corpus fixture reaches the
    // omit branch — every auto-layout container captured or lowered so far
    // carries non-default padding (measured across the whole corpus, not
    // only the fixtures with a `.dsb` golden) — and authoring a new Figma
    // capture needs a human step this cannot take
    // (docs/decisions/figma-corpus-self-authored-only.md). Going through the
    // loader cannot tell the two settings apart either:
    // `dashscene-core::load.rs` sets `Prop::Padding` only when
    // `flex.padding()` is `Some`, and the arena's own unset default is
    // already all zero, so a solved scene is identical either way. Only a
    // check on the raw flatbuffer accessor sees the difference, hence no
    // golden here — no `.dsb` byte diff would say anything a reviewer could
    // read either.
    use dashc_wasm::EdgeInsets;

    let default_bytes = compile(&container_document(EdgeInsets::default())).expect("validates");
    let default_document = dashbuf::root_as_document(
        dashbuf::container::ui_document(&default_bytes).expect("a .dsb file"),
    )
    .expect("valid buffer");
    let default_flex = default_document
        .nodes()
        .expect("nodes present")
        .get(0)
        .flex()
        .expect("a container node");
    assert!(
        default_flex.padding().is_none(),
        "a default-padding container must write no padding field, not an all-zero one"
    );

    let nonzero = EdgeInsets {
        left: 8.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    let nonzero_bytes = compile(&container_document(nonzero)).expect("validates");
    let nonzero_document = dashbuf::root_as_document(
        dashbuf::container::ui_document(&nonzero_bytes).expect("a .dsb file"),
    )
    .expect("valid buffer");
    let nonzero_flex = nonzero_document
        .nodes()
        .expect("nodes present")
        .get(0)
        .flex()
        .expect("a container node");
    let p = nonzero_flex
        .padding()
        .expect("a non-default-padding container must write the field");
    assert_eq!(
        (p.left(), p.top(), p.right(), p.bottom()),
        (8.0, 0.0, 0.0, 0.0)
    );
}

/// The document a rotated node compiles from: one 20 x 8 node, turned about a
/// stated anchor, so the whole pipeline has something whose angle and anchor
/// are both distinguishable from the defaults.
fn rotated_document(rotation: f32, rotation_anchor: (f32, f32)) -> Document {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("turned".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 4.0,
            y: 6.0,
            width: 20.0,
            height: 8.0,
        },
        paint: Some(Paint {
            entry: solid_entry(RED),
            ..Paint::default()
        }),
        rotation,
        rotation_anchor,
        ..Node::default()
    });
    doc
}

#[test]
fn a_rotation_survives_the_round_trip_into_the_rect_it_paints() {
    // Story #770's half of "motion is data in the document": the angle and its
    // anchor are written into the `.dsb`, read back by the loader, and land on
    // the rect entry a painter reads — not merely on the arena's staged
    // intent, which is a different table and would leave the picture upright.
    let arena = load(&rotated_document(0.75, (12.0, 3.0)));
    let scene = arena.committed();
    let rect = scene.rects()[0];

    assert_eq!(rect.rotation, 0.75, "the angle round-tripped");
    assert_eq!(
        (rect.rotation_anchor.x, rect.rotation_anchor.y),
        (12.0, 3.0),
        "the anchor round-tripped beside it",
    );
    assert_eq!(
        (rect.x, rect.y, rect.w, rect.h),
        (4.0, 6.0, 20.0, 8.0),
        "the box is the node's own, unrotated — the document carries intent, \
         never the rotated silhouette (P1), so a rotation moves no box here",
    );
}

#[test]
fn an_unrotated_node_writes_no_rotation_fields() {
    // The R7 append check for this vocabulary: all three fields equal their
    // schema default on an unrotated node, so flatc omits them and a document
    // written before story #770 encodes byte-identically.
    let bytes = compile(&rotated_document(0.0, (0.0, 0.0))).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    let node = document.nodes().expect("nodes present").get(0);

    assert_eq!(node.rotation(), 0.0);
    assert_eq!(node.rotation_anchor_x(), 0.0);
    assert_eq!(node.rotation_anchor_y(), 0.0);

    // The same document with a rotation must differ, or the assertion above
    // would pass on a producer that never writes the field at all.
    let rotated = compile(&rotated_document(0.75, (12.0, 3.0))).expect("validates");
    assert_ne!(
        bytes, rotated,
        "a rotated document must encode differently from an unrotated one",
    );
}

/// The shelf the committed `goldens/dsb/v018-variant-shelf.dsb` fixture is
/// compiled from (issue #617): a horizontal flex row of three chips and one
/// variant set that collapses it.
///
/// **Authored, never imported.** `dashc`'s Figma path resolves an `INSTANCE`
/// to its one active subtree at compile time, so a static REST export names
/// one concrete state and has no switchable set to preserve — which is why
/// all ten fixtures that preceded this one report zero variant sets.
///
/// The collapse is shaped so the switch produces rects **the document does
/// not state**: hiding `middle` takes it out of the laid-out set, and `right`
/// then slides left by gap + width although it carries no override at all.
/// That is what a FLIP needs on both sides — before and after rects a solver
/// produced — and it is what a set of authored `X` overrides would not give.
fn variant_shelf() -> Document {
    use dashc_wasm::{EdgeInsets, LayoutConstraints, LayoutContainer};

    const GREEN: Color = Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };

    let mut doc = Document::new();
    let shelf = doc.push(Node {
        name: Some("shelf".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 60.0,
        },
        container: Some(LayoutContainer {
            mode: dashc_wasm::LayoutMode::Horizontal,
            gap: 8.0,
            padding: EdgeInsets {
                left: 4.0,
                top: 4.0,
                right: 4.0,
                bottom: 4.0,
            },
            main_align: dashc_wasm::MainAxisAlign::Start,
            cross_align: dashc_wasm::CrossAxisAlign::Center,
            cross_gap: None,
            grid_rows: Vec::new(),
            grid_columns: Vec::new(),
        }),
        ..Node::default()
    });

    let mut chip = |name: &str, color: Color| {
        doc.push(Node {
            name: Some(name.to_owned()),
            parent: Some(shelf),
            box2d: Box2D {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
            paint: Some(Paint {
                entry: solid_entry(color),
                ..Paint::default()
            }),
            constraints: Some(LayoutConstraints::default()),
            ..Node::default()
        })
    };
    let left = chip("left", RED);
    let middle = chip("middle", BLUE);
    let right = chip("right", GREEN);

    doc.variant_sets.push(VariantSet {
        members: vec![
            VariantMember {
                name: Some("full".to_owned()),
                overrides: Vec::new(),
                ..VariantMember::default()
            },
            VariantMember {
                name: Some("collapsed".to_owned()),
                overrides: vec![
                    VariantOverride {
                        node: middle,
                        value: VariantValue::Visible(false),
                    },
                    VariantOverride {
                        node: left,
                        value: VariantValue::Width(64.0),
                    },
                ],
                // The motion the epic's definition of done turns on: a
                // one-second linear tween on the chip that carries no
                // override, so the file states how the reflow travels and
                // not only where it ends. Linear over one second makes every
                // sample exact — t = 0.25 is 100 - 6 = 94 — so the test that
                // reads it back compares without a tolerance.
                transition: Some(VariantTransition {
                    tracks: vec![PropTransition {
                        node: right,
                        channel: BindingChannel::X,
                        spec: TransitionSpec::Tween {
                            duration: 1.0,
                            easing: Easing::Linear,
                        },
                    }],
                    stagger: 0.0,
                }),
            },
        ],
        active_member: 0,
    });
    doc
}

#[test]
fn the_variant_shelf_emits_its_golden_dsb() {
    // Issue #617's deliverable: a committed `.dsb` that carries a variant
    // table, so a loaded document has something to switch. Every fixture
    // before this one is Figma-compiled and reports zero variant sets, which
    // is why loading one drives `attach_live`, seeds a single commit, and
    // then has nothing left to drive.
    let bytes = compile(&variant_shelf()).expect("the shelf validates");
    common::assert_dsb_golden(&bytes, "v018-variant-shelf.dsb");
}

#[test]
fn the_committed_shelf_fixture_carries_a_switchable_variant_table() {
    // Read the committed bytes rather than recompiling them: what #617
    // measured is a property of the *file* on disk, and a test that compiles
    // its own copy would still pass if the file were never written.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens/dsb/v018-variant-shelf.dsb");
    let bytes = std::fs::read(&path).expect("the committed fixture is readable");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");

    let sets = document
        .variant_sets()
        .expect("the committed fixture carries a variant table");
    assert_eq!(sets.len(), 1);
    assert_eq!(
        sets.get(0).members().expect("members present").len(),
        2,
        "two members, so there is something for set_variant to select between",
    );
}

#[test]
fn the_shelfs_collapsed_member_states_the_intent_a_solver_reflows_from() {
    // What this path can assert, and no more: `load_document` resolves the
    // active member's overrides into the committed tables, and nothing here
    // solves. Taffy is `dashscene-engine`'s and `dashc` does not depend on
    // it, so every rect below is the *authored* box with the overrides
    // applied — the intent a solver is later handed, not a layout.
    //
    // The reflow that intent produces — `right` sliding left although it
    // carries no override — is asserted where a solver exists, over these
    // very bytes: `goldens/tooling/tests/loaded_variant_flip.rs`.
    let full = load(&variant_shelf());
    let mut collapsed_doc = variant_shelf();
    collapsed_doc.variant_sets[0].active_member = 1;
    let collapsed = load(&collapsed_doc);

    // DFS order is shelf, left, middle, right.
    assert_eq!(
        full.committed().rects()[1].w,
        40.0,
        "the full member leaves `left` at its authored width",
    );
    assert_eq!(
        collapsed.committed().rects()[1].w,
        64.0,
        "the collapsed member widens it",
    );

    let full_root = full.roots()[0];
    let collapsed_root = collapsed.roots()[0];
    assert!(
        full.layout(full.children(full_root)[1]).visible,
        "the full member shows `middle`",
    );
    assert!(
        !collapsed
            .layout(collapsed.children(collapsed_root)[1])
            .visible,
        "the collapsed member takes it out of the laid-out set, which is what \
         makes the siblings reflow once a solver runs",
    );
}

/// The document a variant set compiles from (story #771 part 1 — the
/// emitter; #617's fixture is part 2, above): a passthrough root and three
/// children, plus one set whose second
/// member overrides **every arm** of the schema's `VariantPropValue` union
/// across them.
///
/// One document rather than seven, because what is under test is that the
/// emitter reaches every arm: an arm no override names is emitted nowhere,
/// and nothing fails. Three children rather than one, because the arms
/// partition by what they touch — `geometry` takes the four rect arms,
/// `painted` the two paint-side ones, and `toggled` the visibility arm that
/// leaves the laid-out set.
///
/// The root is a passthrough container (`container: None`), so the children
/// place at their authored offsets and an `X`/`Y` override is visible in the
/// committed rect rather than absorbed by a solver.
fn variant_document(active_member: u32) -> Document {
    let mut doc = Document::new();
    let root = doc.push(Node {
        name: Some("root".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
        ..Node::default()
    });
    let geometry = doc.push(Node {
        name: Some("geometry".to_owned()),
        parent: Some(root),
        box2d: Box2D {
            x: 1.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        paint: Some(Paint {
            entry: solid_entry(RED),
            ..Paint::default()
        }),
        ..Node::default()
    });
    let painted = doc.push(Node {
        name: Some("painted".to_owned()),
        parent: Some(root),
        box2d: Box2D {
            x: 12.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        paint: Some(Paint {
            entry: solid_entry(RED),
            ..Paint::default()
        }),
        ..Node::default()
    });
    let toggled = doc.push(Node {
        name: Some("toggled".to_owned()),
        parent: Some(root),
        box2d: Box2D {
            x: 24.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        paint: Some(Paint {
            entry: solid_entry(BLUE),
            ..Paint::default()
        }),
        ..Node::default()
    });
    doc.variant_sets.push(VariantSet {
        members: vec![
            VariantMember {
                name: Some("base".to_owned()),
                overrides: Vec::new(),
                ..VariantMember::default()
            },
            VariantMember {
                name: Some("switched".to_owned()),
                overrides: vec![
                    VariantOverride {
                        node: geometry,
                        value: VariantValue::X(5.0),
                    },
                    VariantOverride {
                        node: geometry,
                        value: VariantValue::Y(6.0),
                    },
                    VariantOverride {
                        node: geometry,
                        value: VariantValue::Width(21.0),
                    },
                    VariantOverride {
                        node: geometry,
                        value: VariantValue::Height(22.0),
                    },
                    VariantOverride {
                        node: painted,
                        value: VariantValue::Fill(BLUE),
                    },
                    VariantOverride {
                        node: painted,
                        value: VariantValue::Rotation {
                            angle: 0.75,
                            anchor: (12.0, 3.0),
                        },
                    },
                    VariantOverride {
                        node: toggled,
                        value: VariantValue::Visible(false),
                    },
                ],
                ..VariantMember::default()
            },
        ],
        active_member,
    });
    doc
}

/// The solid colour the committed paint pool resolves for one rect.
fn solid_at(arena: &Arena, rect: usize) -> Color {
    let scene = arena.committed();
    let entry = scene.paints().resolve(scene.rects()[rect].paint);
    match scene.paints().fill(entry.fill) {
        dashpaint::Fill::Solid(color) => color,
        other => panic!("rect {rect} resolves {other:?}, not a solid fill"),
    }
}

#[test]
fn a_variant_set_survives_the_round_trip_into_the_scene_it_switches() {
    // Story #771 part 1, and issue #617's whole subject: before this the
    // emitter had no variant path at all, so a compiled `.dsb` could not
    // carry a switchable state and every committed golden reported zero
    // variant sets. Both members are asserted through one emitter, so the
    // test fails if the overrides are written but never applied *and* if
    // they are applied but never written.
    let base = load(&variant_document(0));
    let switched = load(&variant_document(1));

    let base_geometry = base.committed().rects()[1];
    assert_eq!(
        (
            base_geometry.x,
            base_geometry.y,
            base_geometry.w,
            base_geometry.h
        ),
        (1.0, 2.0, 10.0, 10.0),
        "member 0 overrides nothing, so the authored box stands",
    );

    let switched_geometry = switched.committed().rects()[1];
    assert_eq!(
        (
            switched_geometry.x,
            switched_geometry.y,
            switched_geometry.w,
            switched_geometry.h
        ),
        (5.0, 6.0, 21.0, 22.0),
        "all four rect arms round-tripped and resolved",
    );

    assert_eq!(solid_at(&base, 2), RED, "the base fill");
    assert_eq!(solid_at(&switched, 2), BLUE, "the VariantFill arm");

    assert_eq!(
        base.committed().rects()[2].rotation,
        0.0,
        "the base member leaves the node upright",
    );
    let painted = switched.committed().rects()[2];
    assert_eq!(painted.rotation, 0.75, "the VariantRotation angle");
    assert_eq!(
        (painted.rotation_anchor.x, painted.rotation_anchor.y),
        (12.0, 3.0),
        "and its anchor, which travels with the angle rather than beside it",
    );

    let base_root = base.roots()[0];
    let switched_root = switched.roots()[0];
    assert!(
        base.layout(base.children(base_root)[2]).visible,
        "the base member shows the toggled child",
    );
    assert!(
        !switched.layout(switched.children(switched_root)[2]).visible,
        "the VariantVisible arm hid it — the topology change a FLIP needs",
    );
}

#[test]
fn a_variant_less_document_writes_no_variant_table() {
    // The R7 append check for this emitter: a document that declares no
    // variant set writes the schema's absent vector, not an empty one, so
    // every `.dsb` compiled before this emitter existed encodes
    // byte-identically.
    let mut plain = variant_document(0);
    plain.variant_sets.clear();
    let bytes = compile(&plain).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");

    assert!(
        document.variant_sets().is_none(),
        "an empty variant list must write no field at all",
    );

    // And the same document *with* the set must differ, or the assertion
    // above would also pass on an emitter that never writes the table.
    let carried = compile(&variant_document(0)).expect("validates");
    assert_ne!(
        bytes, carried,
        "a document carrying a variant set must encode differently from one without",
    );
}

#[test]
fn a_variant_sets_member_order_is_the_authored_order() {
    // Selection is a flat index into `members`
    // (`docs/decisions/variant-set-flat-index.md`), so the emitter reordering
    // or deduplicating members would silently repoint `active_member` and
    // every `set_variant` a runtime makes. Read the names back, in order.
    let bytes = compile(&variant_document(1)).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    let set = document.variant_sets().expect("the set is present").get(0);
    let names: Vec<&str> = set
        .members()
        .expect("members present")
        .iter()
        .map(|member| member.name().expect("each member is named"))
        .collect();

    assert_eq!(names, vec!["base", "switched"]);
    assert_eq!(
        set.active_member(),
        1,
        "the authored active member travels with the set",
    );
}

#[test]
fn an_anchor_is_written_even_when_the_angle_is_zero() {
    // The anchor is the node's stated turning point, not a function of the
    // angle. A binding that later drives only `BindingChannel::Rotation` reads
    // this back, so dropping it at a zero angle would silently re-anchor the
    // node to its top-left the moment that binding fired.
    let arena = load(&rotated_document(0.0, (10.0, 4.0)));
    let rect = arena.committed().rects()[0];

    assert_eq!(rect.rotation, 0.0);
    assert_eq!(
        (rect.rotation_anchor.x, rect.rotation_anchor.y),
        (10.0, 4.0),
        "the anchor survives a zero angle",
    );
}

/// A loop track survives the round trip and drives the loaded scene (story
/// #772).
///
/// The whole path in one test: authored `Document` → emitter → `.dsb` bytes
/// → loader → arena → `attach_live` → the committed rect. Before this the
/// emitter had no loop path at all, so an ambient animation reached a
/// document by no route.
#[test]
fn a_loop_track_survives_the_round_trip_and_drives_the_loaded_scene() {
    let mut doc = Document::new();
    let root = doc.push(Node {
        name: Some("spinner".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        },
        ..Node::default()
    });
    doc.loops.push(LoopTrack {
        node: root,
        channel: BindingChannel::Rotation,
        from: 0.0,
        to: 8.0,
        // Powers of two throughout, so the samples below are exact.
        spec: TransitionSpec::Tween {
            duration: 0.5,
            easing: Easing::Linear,
        },
        phase_offset: 0.25,
    });

    let bytes = compile(&doc).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");

    // The row is in the file, with every field it was authored with.
    let loops = document.loops().expect("the document carries a loop table");
    assert_eq!(loops.len(), 1);
    let row = loops.get(0);
    assert_eq!(row.node(), root);
    assert_eq!(row.channel(), dashbuf::BindingChannel::Rotation);
    assert_eq!((row.from(), row.to()), (0.0, 8.0));
    assert_eq!(row.phase_offset(), 0.25);

    // And it drives the loaded scene through the ordinary frame loop. The
    // offset puts it half a cycle in before the first tick.
    let mut arena = Arena::new();
    load_document(&document, &[], &mut arena);
    let mut live = dashlang::attach_live(&mut arena, Box::new(TaffySolver::new()));

    let angle = |arena: &Arena| arena.committed().rects()[0].rotation;
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 5.0, "a quarter cycle in, plus one eighth");
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 6.0);
    live.tick(0.0625, &mut arena);
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 0.0, "the cycle wrapped rather than settling");
}

/// The R7 append check for the loop table: a document declaring no loop
/// writes the schema's absent vector, not an empty one, so every `.dsb`
/// compiled before this emitter existed encodes byte-identically.
#[test]
fn a_loopless_document_writes_no_loop_table() {
    let plain = variant_document(0);
    let bytes = compile(&plain).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    assert!(
        document.loops().is_none(),
        "an empty loop list must write no field at all",
    );

    // And the same document *with* a loop must differ, or the assertion
    // above would also pass on an emitter that never writes the table.
    let mut carried = variant_document(0);
    carried.loops.push(LoopTrack {
        node: 0,
        channel: BindingChannel::Opacity,
        from: 0.25,
        to: 1.0,
        spec: TransitionSpec::Tween {
            duration: 1.0,
            easing: Easing::Linear,
        },
        phase_offset: 0.0,
    });
    let carried = compile(&carried).expect("validates");
    assert_ne!(
        bytes, carried,
        "a document carrying a loop must encode differently from one without",
    );
}

/// The document a declared placeholder compiles from (story #1126): one node
/// reserving a box for content a host contributes at runtime, taking the
/// placeholder itself as an argument so the same document can be compiled with
/// and without the vocabulary — which is what the R7 check below needs.
fn slot_document(placeholder: Option<Placeholder>) -> Document {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("gauge-slot".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 4.0,
            y: 2.0,
            width: 64.0,
            height: 32.0,
        },
        placeholder,
        ..Node::default()
    });
    doc
}

/// A placeholder with every field set to a value distinguishable from its
/// schema default, so a field-id shift cannot read back as a pass.
fn declared_slot() -> Placeholder {
    Placeholder {
        contribution_id: Some("cluster.speedo".to_owned()),
        fragment_ref: Some("gauge.dsb".to_owned()),
        declared_size: Some((48.0, 24.0)),
        interim_fill: Some(FillSpec::Solid { color: BLUE }),
    }
}

#[test]
fn a_declared_placeholder_survives_the_round_trip() {
    // Story #1126 builds the surface node replacement binds to, and stops
    // there: all four values are carried by the document and read back by the
    // loader, and nothing resolves them. Placeholder *activation* is v1
    // (docs/specification/05-qualification.md).
    let arena = load(&slot_document(Some(declared_slot())));
    let node = arena.roots()[0];

    let declared = arena
        .placeholder(node)
        .expect("the node is a declared placeholder");

    assert_eq!(
        declared.contribution_id.as_deref(),
        Some("cluster.speedo"),
        "the id a runtime producer binds a contribution against",
    );
    assert_eq!(
        declared.fragment_ref.as_deref(),
        Some("gauge.dsb"),
        "the external subtree to stream in",
    );
    assert_eq!(
        declared.declared_size,
        Some((48.0, 24.0)),
        "the size a measure callback will report while nothing is bound — \
         not the node's own box, which is 64 x 32 here so the two cannot be \
         confused for one another",
    );
    assert_eq!(
        declared.interim_fill,
        Some(FillSpec::Solid { color: BLUE }),
        "shown while the contribution loads; the whole fill vocabulary, \
         not a bare color",
    );
}

#[test]
fn an_ordinary_node_declares_no_placeholder() {
    let arena = load(&slot_document(None));

    assert!(
        arena.placeholder(arena.roots()[0]).is_none(),
        "presence of the table is the predicate story #1127 reads, so a node \
         without one must read back as not a placeholder",
    );
}

#[test]
fn a_node_with_no_placeholder_writes_no_placeholder_table() {
    // The R7 append check for this vocabulary, in the shape story #770's
    // rotation check uses: the table is absent on an ordinary node, so flatc
    // omits it and a document written before story #1126 encodes
    // byte-identically.
    let bytes = compile(&slot_document(None)).expect("validates");
    let document =
        dashbuf::root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
            .expect("valid buffer");
    let node = document.nodes().expect("nodes present").get(0);

    assert!(
        node.placeholder().is_none(),
        "an ordinary node carries no Placeholder table",
    );

    // The same document carrying a placeholder must differ, or the assertion
    // above would pass on a producer that never writes the table at all.
    let declared = compile(&slot_document(Some(declared_slot()))).expect("validates");
    assert_ne!(
        bytes, declared,
        "a declared placeholder must encode differently from an ordinary node",
    );
}

/// A placeholder whose every field is at its schema default — the third shape
/// the vocabulary documents, after "fully populated" and "absent": a box that
/// reserves space without naming a binding.
///
/// Its table has an empty vtable, so it is the one input for which an emitter
/// that learned to skip an all-default nested table — the optimisation
/// `flex`/`constraints` already apply through `.map` — would silently flip the
/// node back to ordinary. Presence is the predicate story #1127 reads, so that
/// would make an unfilled placeholder report as an ordinary node.
#[test]
fn an_all_default_placeholder_still_reads_back_as_declared() {
    let arena = load(&slot_document(Some(Placeholder::default())));

    let declared = arena
        .placeholder(arena.roots()[0])
        .expect("an all-default placeholder is still a declared placeholder");

    assert_eq!(declared.contribution_id, None, "names no binding");
    assert_eq!(declared.fragment_ref, None, "streams no fragment");
    assert_eq!(declared.declared_size, None, "declares no measure size");
    assert_eq!(declared.interim_fill, None, "shows nothing meanwhile");
}

/// A gradient interim fill round-trips, which the solid case cannot prove:
/// `flatc` names a union's accessors after its field, so `Placeholder`'s
/// `interim_fill_as_gradient` is a different method from `FillLayer`'s
/// `fill_as_gradient` and is wired by hand in `load.rs`'s `FillUnion` impl.
#[test]
fn a_gradient_interim_fill_round_trips() {
    let arena = load(&slot_document(Some(Placeholder {
        interim_fill: Some(gradient()),
        ..Placeholder::default()
    })));

    let declared = arena
        .placeholder(arena.roots()[0])
        .expect("the node is a declared placeholder");

    match declared.interim_fill.as_ref().expect("an interim fill") {
        FillSpec::Gradient { gradient: g, stops } => {
            assert_eq!(g.kind, GradientKind::Linear);
            assert_eq!(stops.len(), 2, "both stops survived");
            assert_eq!(stops[0].color, RED);
            assert_eq!(stops[1].color, BLUE);
        }
        other => panic!("the interim fill read back as {other:?}, not a gradient"),
    }
}

/// An image interim fill round-trips, and its asset index is remapped through
/// the load's own asset table rather than carried through raw.
///
/// This is the arm that indexes `image_of[..]` in the loader, so it is also
/// the one a wrong index panics in.
#[test]
fn an_image_interim_fill_round_trips_through_the_asset_table() {
    let mut doc = Document::new();
    let image = doc.push_asset(dashc_wasm::Asset {
        format: ImageFormat::Png,
        kind: dashc_wasm::AssetKind::Image,
        bytes: png_pixel(),
        width: 1,
        height: 1,
    });
    doc.push(Node {
        name: Some("gauge-slot".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 32.0,
        },
        placeholder: Some(Placeholder {
            interim_fill: Some(FillSpec::Image(ImageFill {
                image,
                scale_mode: ScaleMode::Fill,
                transform: Mat23::IDENTITY,
                tile_scale: 1.0,
            })),
            ..Placeholder::default()
        }),
        ..Node::default()
    });

    // Loaded twice into one arena, because a fresh arena hands out the same
    // indices the document uses — so index 0 would equal row 0 and the
    // assertion would hold with the remap deleted. The second load's rows
    // start at 1, which is what makes this bite.
    let mut arena = Arena::new();
    let bytes = compile(&doc).expect("validates");
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    load_document(&document, &payloads, &mut arena);
    load_document(&document, &payloads, &mut arena);

    let roots = arena.roots().to_vec();
    assert_eq!(roots.len(), 2, "both loads staged their own node");
    assert_eq!(
        arena.committed().images().len(),
        2,
        "both loads staged their own copy of the asset"
    );

    for (root, expected) in roots.iter().zip([0u32, 1]) {
        let declared = arena
            .placeholder(*root)
            .expect("the node is a declared placeholder");
        match declared.interim_fill.as_ref().expect("an interim fill") {
            FillSpec::Image(fill) => {
                assert_eq!(fill.scale_mode, ScaleMode::Fill, "the scale mode survived");
                assert_eq!(
                    fill.image, expected,
                    "the interim fill's asset index is the arena's row, not the \
                     document's — the second load must point at the second asset, \
                     or it repaints with the first document's picture",
                );
            }
            other => panic!("the interim fill read back as {other:?}, not an image"),
        }
    }
}
