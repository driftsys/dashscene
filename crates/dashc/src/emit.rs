//! Emitting a [`Document`] as `.dsb` bytes.
//!
//! R7: same input → byte-identical document. Hashing, signing, and CI all
//! depend on it, so nothing here may depend on iteration order of a hash
//! map, on addresses, or on anything else that varies between runs. The one
//! place that could is the paint pool, and it is interned in **first-use DFS
//! order** — the same rule `dashscene-core`'s commit uses, so a document and
//! the scene it loads into agree on pool order too.

use std::collections::HashMap;

use dashbuf::{
    Color, CornerRadii, Document as FbDocument, DocumentArgs as FbDocumentArgs,
    EdgeInsets as FbEdgeInsets, FixedSizeLayout, Gradient, GradientArgs, GradientStop, Image,
    ImageArgs, ImageFill, ImageFillArgs, LayoutConstraints as FbLayoutConstraints,
    LayoutConstraintsArgs, LayoutContainer as FbLayoutContainer, LayoutContainerArgs, Mat23,
    NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE, Node as FbNode, NodeArgs as FbNodeArgs,
    Paint as BufPaint, PaintArgs, SolidFill, SolidFillArgs, Stroke, StrokeArgs,
    TextStyle as FbTextStyle, TextStyleArgs, Vec2,
};
use dashpaint::{ImageAsset, PaintEntry, PaintKind};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use crate::document::{
    AxisSizing, CrossAxisAlign, Document, EdgeInsets, LayoutMode, MainAxisAlign, Node, Paint,
    TextStyle,
};

/// Serializes a document to `.dsb` bytes.
///
/// Deterministic: the same [`Document`] always produces the same bytes (R7).
pub fn emit(doc: &Document) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();

    // The paint pool, interned in first-use DFS order. A `Vec` of keys, not
    // a HashMap iteration, decides the pool's order — a HashMap's order is
    // unspecified and would make the bytes vary between runs, breaking R7.
    let mut pool: Vec<&Paint> = Vec::new();
    let mut pool_of: HashMap<PaintKey, u32> = HashMap::new();
    let mut entry_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());

    // The string and text-style pools intern in the same first-use DFS order,
    // for the same R7 reason: two text nodes sharing a string or a style
    // share one pool entry, and the order is the node walk's, never a hash
    // map's. `docs/design/dashbuf.md`: "Dedup is the producer's job; the pool
    // makes it representable."
    let mut strings: Vec<&str> = Vec::new();
    let mut string_of_pool: HashMap<&str, u32> = HashMap::new();
    let mut string_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());
    let mut styles: Vec<&TextStyle> = Vec::new();
    let mut style_of_pool: HashMap<TextStyleKey, u32> = HashMap::new();
    let mut style_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());

    for node in &doc.nodes {
        entry_of.push(node.paint.as_ref().map(|paint| {
            let key = paint_key(paint);
            *pool_of.entry(key).or_insert_with(|| {
                let index = u32::try_from(pool.len()).expect("paint pool exceeds u32::MAX");
                pool.push(paint);
                index
            })
        }));
        string_of.push(node.text.as_deref().map(|text| {
            *string_of_pool.entry(text).or_insert_with(|| {
                let index = u32::try_from(strings.len()).expect("string pool exceeds u32::MAX");
                strings.push(text);
                index
            })
        }));
        style_of.push(node.text_style.as_ref().map(|style| {
            let key = text_style_key(style);
            *style_of_pool.entry(key).or_insert_with(|| {
                let index = u32::try_from(styles.len()).expect("text-style pool exceeds u32::MAX");
                styles.push(style);
                index
            })
        }));
    }

    let images: Vec<WIPOffset<Image>> = doc.images.iter().map(|a| build_image(&mut b, a)).collect();
    let paints: Vec<WIPOffset<BufPaint>> = pool.iter().map(|p| build_paint(&mut b, p)).collect();
    let string_offsets: Vec<WIPOffset<&str>> = strings.iter().map(|s| b.create_string(s)).collect();
    let style_offsets: Vec<WIPOffset<FbTextStyle>> =
        styles.iter().map(|s| build_text_style(&mut b, s)).collect();
    let nodes: Vec<WIPOffset<FbNode>> = doc
        .nodes
        .iter()
        .zip(&entry_of)
        .zip(string_of.iter().zip(&style_of))
        .map(|((node, entry), (text, style))| build_node(&mut b, node, *entry, *text, *style))
        .collect();

    let nodes = b.create_vector(&nodes);
    let images = (!images.is_empty()).then(|| b.create_vector(&images));
    let paints = (!paints.is_empty()).then(|| b.create_vector(&paints));
    let strings = (!string_offsets.is_empty()).then(|| b.create_vector(&string_offsets));
    let text_styles = (!style_offsets.is_empty()).then(|| b.create_vector(&style_offsets));

    let document = FbDocument::create(
        &mut b,
        &FbDocumentArgs {
            nodes: Some(nodes),
            images,
            paints,
            strings,
            text_styles,
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

fn build_node<'a>(
    b: &mut FlatBufferBuilder<'a>,
    node: &Node,
    paint_entry: Option<u32>,
    text: Option<u32>,
    text_style: Option<u32>,
) -> WIPOffset<FbNode<'a>> {
    let name = node.name.as_deref().map(|n| b.create_string(n));

    // The two v0.2 flex tables. Absent stays absent — `container: None`
    // is the schema's mode-`None` state and `constraints: None` its
    // fully-default state — so a fixed-layout document emits the same
    // bytes it did before the flex vocabulary was carried (R7: the frozen
    // goldens hold).
    let flex = node.container.map(|c| {
        let padding = insets(c.padding);
        FbLayoutContainer::create(
            b,
            &LayoutContainerArgs {
                mode: match c.mode {
                    LayoutMode::Horizontal => dashbuf::LayoutMode::Horizontal,
                    LayoutMode::Vertical => dashbuf::LayoutMode::Vertical,
                },
                gap: c.gap,
                padding: (c.padding != EdgeInsets::default()).then_some(&padding),
                main_align: match c.main_align {
                    MainAxisAlign::Start => dashbuf::MainAxisAlign::Start,
                    MainAxisAlign::Center => dashbuf::MainAxisAlign::Center,
                    MainAxisAlign::End => dashbuf::MainAxisAlign::End,
                    MainAxisAlign::SpaceBetween => dashbuf::MainAxisAlign::SpaceBetween,
                },
                cross_align: match c.cross_align {
                    CrossAxisAlign::Start => dashbuf::CrossAxisAlign::Start,
                    CrossAxisAlign::Center => dashbuf::CrossAxisAlign::Center,
                    CrossAxisAlign::End => dashbuf::CrossAxisAlign::End,
                },
            },
        )
    });
    let constraints = node.constraints.map(|c| {
        let margin = insets(c.margin);
        FbLayoutConstraints::create(
            b,
            &LayoutConstraintsArgs {
                sizing_h: axis_sizing(c.sizing_h),
                sizing_v: axis_sizing(c.sizing_v),
                min_width: c.min_width,
                max_width: c.max_width,
                min_height: c.min_height,
                max_height: c.max_height,
                margin: (c.margin != EdgeInsets::default()).then_some(&margin),
            },
        )
    });

    FbNode::create(
        b,
        &FbNodeArgs {
            name,
            parent: node.parent.unwrap_or(NO_PARENT),
            layout: Some(&FixedSizeLayout::new(
                node.box2d.x,
                node.box2d.y,
                node.box2d.width,
                node.box2d.height,
            )),
            // The v0.1 `paint` shorthand is never written: `paint_entry`
            // supersedes it, and writing both is a producer error the load
            // gate names (`paint.conflicting-representation`).
            paint_entry: paint_entry.unwrap_or(NO_PAINT),
            // A non-text node leaves both at the sentinel — the schema
            // default, so it is omitted from the buffer and a text-free
            // document emits the bytes it did before this vocabulary (R7).
            text: text.unwrap_or(NO_TEXT),
            text_style: text_style.unwrap_or(NO_TEXT_STYLE),
            flex,
            constraints,
            ..Default::default()
        },
    )
}

/// Builds one `TextStyle` pool entry. The color is always written: the
/// lowering never emits a color-less style (a text node with no solid fill is
/// refused at the walk), and the loader treats an absent color as a producer
/// error (`text.style-no-color`, P4).
fn build_text_style<'a>(
    b: &mut FlatBufferBuilder<'a>,
    style: &TextStyle,
) -> WIPOffset<FbTextStyle<'a>> {
    let family = b.create_string(&style.family);
    FbTextStyle::create(
        b,
        &TextStyleArgs {
            family: Some(family),
            size: style.size,
            weight: style.weight,
            color: Some(&color_of(style.color)),
        },
    )
}

fn axis_sizing(sizing: AxisSizing) -> dashbuf::AxisSizing {
    match sizing {
        AxisSizing::Fixed => dashbuf::AxisSizing::Fixed,
        AxisSizing::Hug => dashbuf::AxisSizing::Hug,
        AxisSizing::Fill => dashbuf::AxisSizing::Fill,
    }
}

fn insets(e: EdgeInsets) -> FbEdgeInsets {
    FbEdgeInsets::new(e.left, e.top, e.right, e.bottom)
}

fn build_image<'a>(b: &mut FlatBufferBuilder<'a>, asset: &ImageAsset) -> WIPOffset<Image<'a>> {
    let bytes = b.create_vector(&asset.bytes);
    Image::create(
        b,
        &ImageArgs {
            format: match asset.format {
                dashpaint::ImageFormat::Png => dashbuf::ImageFormat::Png,
            },
            bytes: Some(bytes),
        },
    )
}

fn build_paint<'a>(b: &mut FlatBufferBuilder<'a>, paint: &Paint) -> WIPOffset<BufPaint<'a>> {
    let entry = &paint.entry;

    let (fill_type, fill) = match &entry.fill {
        None => (dashbuf::Fill::NONE, None),
        Some(PaintKind::Solid { color }) => {
            let solid = SolidFill::create(
                b,
                &SolidFillArgs {
                    color: Some(&color_of(*color)),
                },
            );
            (dashbuf::Fill::SolidFill, Some(solid.as_union_value()))
        }
        Some(PaintKind::Gradient(g)) => {
            let stops: Vec<GradientStop> = g
                .stops
                .iter()
                .map(|s| GradientStop::new(s.offset, &color_of(s.color)))
                .collect();
            let stops = b.create_vector(&stops);
            let gradient = Gradient::create(
                b,
                &GradientArgs {
                    kind: match g.kind {
                        dashpaint::GradientKind::Linear => dashbuf::GradientKind::Linear,
                        dashpaint::GradientKind::Radial => dashbuf::GradientKind::Radial,
                        dashpaint::GradientKind::Angular => dashbuf::GradientKind::Angular,
                        dashpaint::GradientKind::Diamond => dashbuf::GradientKind::Diamond,
                    },
                    handle_origin: Some(&vec2_of(g.handle_origin)),
                    handle_primary: Some(&vec2_of(g.handle_primary)),
                    handle_secondary: Some(&vec2_of(g.handle_secondary)),
                    stops: Some(stops),
                },
            );
            (dashbuf::Fill::Gradient, Some(gradient.as_union_value()))
        }
        Some(PaintKind::Image {
            image,
            scale_mode,
            transform,
            tile_scale,
        }) => {
            let image_fill = ImageFill::create(
                b,
                &ImageFillArgs {
                    image: *image,
                    scale_mode: match scale_mode {
                        dashpaint::ScaleMode::Fill => dashbuf::ScaleMode::Fill,
                        dashpaint::ScaleMode::Fit => dashbuf::ScaleMode::Fit,
                        dashpaint::ScaleMode::Crop => dashbuf::ScaleMode::Crop,
                        dashpaint::ScaleMode::Tile => dashbuf::ScaleMode::Tile,
                    },
                    transform: transform.as_ref().map(mat23_of).as_ref(),
                    tile_scale: *tile_scale,
                },
            );
            (dashbuf::Fill::ImageFill, Some(image_fill.as_union_value()))
        }
    };

    let stroke = entry.stroke.as_ref().map(|s| {
        Stroke::create(
            b,
            &StrokeArgs {
                width: s.width,
                align: match s.align {
                    dashpaint::StrokeAlign::Inside => dashbuf::StrokeAlign::Inside,
                    dashpaint::StrokeAlign::Center => dashbuf::StrokeAlign::Center,
                    dashpaint::StrokeAlign::Outside => dashbuf::StrokeAlign::Outside,
                },
                color: Some(&color_of(s.color)),
            },
        )
    });

    // Sharp corners are the overwhelmingly common case, and `Paint.corners`
    // is optional — so an all-zero radii struct is 16 bytes of zeros in every
    // sharp-cornered entry. Omit it; the loader already reads an absent
    // `corners` as the default, so this round-trips identically.
    let corners = (entry.corners != dashpaint::CornerRadii::default()).then(|| {
        CornerRadii::new(
            entry.corners.top_left,
            entry.corners.top_right,
            entry.corners.bottom_right,
            entry.corners.bottom_left,
        )
    });

    BufPaint::create(
        b,
        &PaintArgs {
            fill_type,
            fill,
            stroke,
            corners: corners.as_ref(),
            clip: paint.clip,
        },
    )
}

fn color_of(c: dashpaint::Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

fn vec2_of(v: dashpaint::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

fn mat23_of(m: &dashpaint::Mat23) -> Mat23 {
    Mat23::new(m.a, m.b, m.c, m.d, m.tx, m.ty)
}

/// The pool's interning key: a canonical bit encoding of the paint entry
/// plus the clip flag.
///
/// `f32`s go in by bit pattern, not by value: `f32` is not `Eq`/`Hash`, and
/// NaN is not equal to itself, so a value-keyed pool would emit a fresh
/// entry for every NaN and break R7's byte-reproducibility.
///
/// The clip flag is part of the key because the schema pools clip with the
/// paint entry while the arena carries it per node — two nodes with the same
/// style but different clip are two document entries.
type PaintKey = (Vec<u32>, bool);

fn paint_key(paint: &Paint) -> PaintKey {
    (entry_bits(&paint.entry), paint.clip)
}

/// The text-style pool's interning key. The `f32` size and the color go in by
/// bit pattern for the same reason the paint key's do (`f32` is not
/// `Eq`/`Hash`, and a value key would mint a fresh entry per NaN, breaking
/// R7's byte-reproducibility).
type TextStyleKey = (String, u32, u16, [u32; 4]);

fn text_style_key(style: &TextStyle) -> TextStyleKey {
    (
        style.family.clone(),
        style.size.to_bits(),
        style.weight,
        color_bits(style.color),
    )
}

fn entry_bits(entry: &PaintEntry) -> Vec<u32> {
    let mut key = Vec::new();
    match &entry.fill {
        None => key.push(0),
        Some(PaintKind::Solid { color }) => {
            key.push(1);
            key.extend(color_bits(*color));
        }
        Some(PaintKind::Gradient(g)) => {
            key.push(2);
            key.push(g.kind as u32);
            key.extend([g.handle_origin.x.to_bits(), g.handle_origin.y.to_bits()]);
            key.extend([g.handle_primary.x.to_bits(), g.handle_primary.y.to_bits()]);
            key.extend([
                g.handle_secondary.x.to_bits(),
                g.handle_secondary.y.to_bits(),
            ]);
            key.push(g.stops.len() as u32);
            for s in &g.stops {
                key.push(s.offset.to_bits());
                key.extend(color_bits(s.color));
            }
        }
        Some(PaintKind::Image {
            image,
            scale_mode,
            transform,
            tile_scale,
        }) => {
            key.push(3);
            key.push(*image);
            key.push(*scale_mode as u32);
            key.push(tile_scale.to_bits());
            match transform {
                None => key.push(0),
                Some(m) => {
                    key.push(1);
                    key.extend([
                        m.a.to_bits(),
                        m.b.to_bits(),
                        m.c.to_bits(),
                        m.d.to_bits(),
                        m.tx.to_bits(),
                        m.ty.to_bits(),
                    ]);
                }
            }
        }
    }
    match &entry.stroke {
        None => key.push(0),
        Some(s) => {
            key.push(1);
            key.push(s.width.to_bits());
            key.push(s.align as u32);
            key.extend(color_bits(s.color));
        }
    }
    key.extend([
        entry.corners.top_left.to_bits(),
        entry.corners.top_right.to_bits(),
        entry.corners.bottom_right.to_bits(),
        entry.corners.bottom_left.to_bits(),
    ]);
    key
}

fn color_bits(c: dashpaint::Color) -> [u32; 4] {
    [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]
}
