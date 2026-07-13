//! Loading a `.dsb` document into the arena — the document→runtime path
//! (DESIGN_1.md §5, §6.1).
//!
//! A `.dsb` is the serialized *intent* (P1), and the arena is the runtime's
//! intent model, so loading is a straight replay of the document's nodes
//! through the ordinary producer API: `add_node` + `set_prop` + `commit`. It
//! adds no semantics — a loaded scene is indistinguishable from the same
//! scene staged by hand, and a test pins exactly that.
//!
//! # This function assumes a validated document (P4)
//!
//! It does not re-check referential integrity, and it panics on an index
//! that misses — the same contract as `PaintTable::resolve` and the
//! `Painter` trait. The caller runs the gates first, and there are two of
//! them:
//!
//! ```text
//! let doc = dashbuf::root_as_document(bytes)?;   // flatbuffer verifier: structure
//! let report = dashscene_validator::validate_document(&doc);  // load gate: references
//! if report.has_errors() { /* refuse; never load */ }
//! load_document(&doc, &mut arena);               // safe iff the gate passed
//! ```
//!
//! `dashscene-validator` is published *after* `dashscene-core`, so this
//! crate cannot call it — which is exactly why the contract is stated here
//! rather than enforced here.

use dashbuf::{Document, Fill, NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE};

use crate::arena::{
    Arena, AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, TextStyle,
};
use crate::committed::{
    Color, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat, Mat23, PaintKind,
    ScaleMode, Stroke, StrokeAlign, Vec2,
};

/// Replays a validated `.dsb` document into `arena` and commits it,
/// returning the commit's generation.
///
/// The document's nodes are appended to whatever the arena already holds —
/// the loader is a producer, not an owner, matching `dashlang::Scene::build`.
///
/// # Panics
///
/// On any index the document carries that does not resolve (a paint entry,
/// an image asset, a string, a text style, a parent). Those are precisely
/// what `dashscene_validator::validate_document` reports as errors, so a
/// panic here means the caller skipped the gate.
pub fn load_document(doc: &Document<'_>, arena: &mut Arena) -> u64 {
    let nodes = doc.nodes().unwrap_or_default();
    let paints = doc.paints().unwrap_or_default();
    let strings = doc.strings().unwrap_or_default();
    let text_styles = doc.text_styles().unwrap_or_default();

    let mut txn = arena.open();

    // Assets first: a paint entry's image fill references them by index, so
    // they must exist before any paint prop is staged.
    //
    // The document's indices are 0..n, but the arena may already hold assets
    // from an earlier load, so the document's index is NOT the arena's. Keep
    // the mapping and rewrite every image fill through it — assuming they
    // coincide would silently repaint one document's nodes with another
    // document's assets.
    let image_of: Vec<u32> = doc
        .images()
        .unwrap_or_default()
        .iter()
        .map(|image| {
            txn.add_image(ImageAsset {
                format: image_format(image.format()),
                bytes: image
                    .bytes()
                    .map(|b| b.iter().collect())
                    .unwrap_or_default(),
            })
        })
        .collect();

    // The node array is in DFS order, so a parent is always staged before
    // its children and `ids[parent]` is always populated by the time a child
    // reads it (the load gate's `node.parent-not-before-child` rule is what
    // makes this safe to assume).
    let mut ids: Vec<NodeId> = Vec::with_capacity(nodes.len());

    for node in nodes.iter() {
        let parent = match node.parent() {
            NO_PARENT => None,
            index => Some(ids[index as usize]),
        };
        let id = txn.add_node(parent, node.name());
        ids.push(id);

        if let Some(layout) = node.layout() {
            txn.set_prop(id, Prop::X(layout.x()));
            txn.set_prop(id, Prop::Y(layout.y()));
            txn.set_prop(id, Prop::Width(layout.width()));
            txn.set_prop(id, Prop::Height(layout.height()));
        }

        // `paint_entry` supersedes the v0.1 `paint` shorthand — the load
        // gate rejects a node that sets both (`paint.conflicting-representation`).
        if node.paint_entry() != NO_PAINT {
            let paint = paints.get(node.paint_entry() as usize);
            load_paint(&mut txn, id, &paint, &image_of);
        } else if let Some(solid) = node.paint()
            && let Some(color) = solid.color()
        {
            txn.set_prop(id, Prop::Fill(color_of(color)));
        }

        if node.text() != NO_TEXT {
            txn.set_prop(id, Prop::Text(strings.get(node.text() as usize).to_owned()));
        }
        if node.text_style() != NO_TEXT_STYLE {
            let style = text_styles.get(node.text_style() as usize);
            txn.set_prop(
                id,
                Prop::TextStyle(TextStyle {
                    family: style.family().to_owned(),
                    size: style.size(),
                    weight: style.weight(),
                    // Never defaulted. An absent color is `text.style-no-color`
                    // at the load gate, so it cannot reach here — and inventing
                    // one would be exactly the silent discovery P4 forbids.
                    color: color_of(style.color().expect(
                        "text style carries a color (validated upstream, P4: text.style-no-color)",
                    )),
                }),
            );
        }

        if let Some(flex) = node.flex() {
            txn.set_prop(id, Prop::Mode(layout_mode(flex.mode())));
            txn.set_prop(id, Prop::Gap(flex.gap()));
            if let Some(p) = flex.padding() {
                txn.set_prop(
                    id,
                    Prop::Padding {
                        left: p.left(),
                        top: p.top(),
                        right: p.right(),
                        bottom: p.bottom(),
                    },
                );
            }
            txn.set_prop(id, Prop::MainAlign(main_align(flex.main_align())));
            txn.set_prop(id, Prop::CrossAlign(cross_align(flex.cross_align())));
        }

        if let Some(c) = node.constraints() {
            txn.set_prop(id, Prop::SizingH(axis_sizing(c.sizing_h())));
            txn.set_prop(id, Prop::SizingV(axis_sizing(c.sizing_v())));
            // Absent min/max means unconstrained — absence of intent is not a
            // value of intent (P1), so an absent bound stages no prop at all
            // rather than a sentinel.
            if let Some(v) = c.min_width() {
                txn.set_prop(id, Prop::MinWidth(v));
            }
            if let Some(v) = c.max_width() {
                txn.set_prop(id, Prop::MaxWidth(v));
            }
            if let Some(v) = c.min_height() {
                txn.set_prop(id, Prop::MinHeight(v));
            }
            if let Some(v) = c.max_height() {
                txn.set_prop(id, Prop::MaxHeight(v));
            }
            if let Some(m) = c.margin() {
                txn.set_prop(
                    id,
                    Prop::Margin {
                        left: m.left(),
                        top: m.top(),
                        right: m.right(),
                        bottom: m.bottom(),
                    },
                );
            }
        }
    }

    txn.commit()
}

/// One pool entry's fill, stroke, corners, and clip, staged onto `id`.
fn load_paint(
    txn: &mut crate::arena::Txn<'_>,
    id: NodeId,
    paint: &dashbuf::Paint<'_>,
    image_of: &[u32],
) {
    match paint.fill_type() {
        Fill::SolidFill => {
            if let Some(solid) = paint.fill_as_solid_fill()
                && let Some(color) = solid.color()
            {
                txn.set_prop(id, Prop::Fill(color_of(color)));
            }
        }
        Fill::Gradient => {
            if let Some(g) = paint.fill_as_gradient() {
                txn.set_prop(
                    id,
                    Prop::FillWith(PaintKind::Gradient(Gradient {
                        kind: gradient_kind(g.kind()),
                        handle_origin: vec2_of(g.handle_origin()),
                        handle_primary: vec2_of(g.handle_primary()),
                        handle_secondary: vec2_of(g.handle_secondary()),
                        stops: g
                            .stops()
                            .iter()
                            .map(|s| GradientStop {
                                offset: s.offset(),
                                color: color_of(s.color()),
                            })
                            .collect(),
                    })),
                );
            }
        }
        Fill::ImageFill => {
            if let Some(f) = paint.fill_as_image_fill() {
                txn.set_prop(
                    id,
                    Prop::FillWith(PaintKind::Image {
                        // Through the mapping, never the document's own index.
                        image: image_of[f.image() as usize],
                        scale_mode: scale_mode(f.scale_mode()),
                        transform: f.transform().map(mat23_of),
                        tile_scale: f.tile_scale(),
                    }),
                );
            }
        }
        // A pool entry with no fill is a stroke-only or clip-only entry — a
        // legitimate shape, not a missing one.
        _ => {}
    }

    if let Some(s) = paint.stroke() {
        // `Stroke.color` is `(required)` in the schema, so the accessor is
        // not an Option.
        txn.set_prop(
            id,
            Prop::Stroke(Stroke {
                width: s.width(),
                align: stroke_align(s.align()),
                color: color_of(s.color()),
            }),
        );
    }

    if let Some(c) = paint.corners() {
        txn.set_prop(
            id,
            Prop::Corners {
                top_left: c.top_left(),
                top_right: c.top_right(),
                bottom_right: c.bottom_right(),
                bottom_left: c.bottom_left(),
            },
        );
    }

    // The document pools clip with the paint entry; the arena carries it as
    // node intent (issue #97). Two nodes sharing a style but differing in
    // clip therefore need two pool entries in the document, which is what
    // the emitter's pool key accounts for.
    if paint.clip() {
        txn.set_prop(id, Prop::Clip(true));
    }
}

fn color_of(c: &dashbuf::Color) -> Color {
    Color {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}

fn vec2_of(v: &dashbuf::Vec2) -> Vec2 {
    Vec2 { x: v.x(), y: v.y() }
}

fn mat23_of(m: &dashbuf::Mat23) -> Mat23 {
    Mat23 {
        a: m.a(),
        b: m.b(),
        c: m.c(),
        d: m.d(),
        tx: m.tx(),
        ty: m.ty(),
    }
}

// The enum maps are exhaustive over the values this build knows. A value it
// does not know is `vocabulary.unknown-enum` at the load gate, so it never
// reaches here — the wildcard arm exists because `flatc` models an
// append-only enum as a newtype over `u8`, which has no exhaustive match.

fn image_format(f: dashbuf::ImageFormat) -> ImageFormat {
    match f {
        dashbuf::ImageFormat::Png => ImageFormat::Png,
        other => unreachable!("unknown ImageFormat {other:?}: rejected by the load gate (P4)"),
    }
}

fn gradient_kind(k: dashbuf::GradientKind) -> GradientKind {
    match k {
        dashbuf::GradientKind::Linear => GradientKind::Linear,
        dashbuf::GradientKind::Radial => GradientKind::Radial,
        dashbuf::GradientKind::Angular => GradientKind::Angular,
        dashbuf::GradientKind::Diamond => GradientKind::Diamond,
        other => unreachable!("unknown GradientKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn scale_mode(m: dashbuf::ScaleMode) -> ScaleMode {
    match m {
        dashbuf::ScaleMode::Fill => ScaleMode::Fill,
        dashbuf::ScaleMode::Fit => ScaleMode::Fit,
        dashbuf::ScaleMode::Crop => ScaleMode::Crop,
        dashbuf::ScaleMode::Tile => ScaleMode::Tile,
        other => unreachable!("unknown ScaleMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn stroke_align(a: dashbuf::StrokeAlign) -> StrokeAlign {
    match a {
        dashbuf::StrokeAlign::Inside => StrokeAlign::Inside,
        dashbuf::StrokeAlign::Center => StrokeAlign::Center,
        dashbuf::StrokeAlign::Outside => StrokeAlign::Outside,
        other => unreachable!("unknown StrokeAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn layout_mode(m: dashbuf::LayoutMode) -> LayoutMode {
    match m {
        dashbuf::LayoutMode::None => LayoutMode::None,
        dashbuf::LayoutMode::Horizontal => LayoutMode::Horizontal,
        dashbuf::LayoutMode::Vertical => LayoutMode::Vertical,
        other => unreachable!("unknown LayoutMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn main_align(a: dashbuf::MainAxisAlign) -> MainAxisAlign {
    match a {
        dashbuf::MainAxisAlign::Start => MainAxisAlign::Start,
        dashbuf::MainAxisAlign::Center => MainAxisAlign::Center,
        dashbuf::MainAxisAlign::End => MainAxisAlign::End,
        dashbuf::MainAxisAlign::SpaceBetween => MainAxisAlign::SpaceBetween,
        other => unreachable!("unknown MainAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn cross_align(a: dashbuf::CrossAxisAlign) -> CrossAxisAlign {
    match a {
        dashbuf::CrossAxisAlign::Start => CrossAxisAlign::Start,
        dashbuf::CrossAxisAlign::Center => CrossAxisAlign::Center,
        dashbuf::CrossAxisAlign::End => CrossAxisAlign::End,
        other => unreachable!("unknown CrossAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn axis_sizing(s: dashbuf::AxisSizing) -> AxisSizing {
    match s {
        dashbuf::AxisSizing::Fixed => AxisSizing::Fixed,
        dashbuf::AxisSizing::Hug => AxisSizing::Hug,
        dashbuf::AxisSizing::Fill => AxisSizing::Fill,
        other => unreachable!("unknown AxisSizing {other:?}: rejected by the load gate (P4)"),
    }
}
