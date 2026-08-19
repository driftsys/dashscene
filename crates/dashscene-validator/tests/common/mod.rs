//! The `.dsb` document builder these tests share: a document stated as its
//! pools and its nodes, so each test carries only the field it is about.
//!
//! Each test binary compiles its own copy of this module, so a helper unused
//! by one binary is still used by another — hence the `dead_code` allowance
//! (the same pattern as `dashc`'s and `dashscene-typeset`'s `tests/common`).

#![allow(dead_code)]

use dashbuf::{
    AssetEntry, AssetEntryArgs, Color, Document, DocumentArgs, Fill, Gradient, GradientArgs,
    GradientKind, GradientStop, ImageFill, ImageFillArgs, ImageFormat, NO_PAINT, Node, NodeArgs,
    Paint, PaintArgs, ScaleMode, SolidFill, SolidFillArgs, Stroke, StrokeAlign, StrokeArgs,
    TextStyle, TextStyleArgs, Vec2,
};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

pub fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

/// A document builder that keeps each test to the one field it is about.
#[derive(Default)]
pub struct Doc {
    pub nodes: Vec<NodeSpec>,
    pub paints: Vec<PaintSpec>,
    pub images: Vec<ImageSpec>,
    /// The string pool, by content. `strings(n)` fills it with a filler
    /// entry per slot, for a test that only needs the pool to be a given
    /// size; `named_strings` states the entries, for a test whose subject is
    /// which string an index resolves to.
    ///
    /// Both setters replace this pool rather than appending to it, unlike
    /// `images`, which extends — so each asserts the pool is still empty. A
    /// test that named a string and then asked for a pool size would otherwise
    /// lose the name, and the index it aimed at would resolve to a filler entry
    /// instead of failing.
    pub strings: Vec<&'static str>,
    pub text_styles: usize,
}

/// Since story #107 the document carries asset identity and metadata, never
/// bytes (P1 applied to assets) — `hash`/`width`/`height` stand in for the
/// old `Image.bytes` payload the pre-#107 pool carried.
#[derive(Clone)]
pub struct ImageSpec {
    pub hash: [u8; 32],
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

#[derive(Default, Clone)]
pub struct NodeSpec {
    pub name: &'static str,
    pub parent: Option<u32>,
    pub paint_entry: Option<u32>,
    pub legacy_paint: bool,
    pub text: Option<u32>,
    pub text_style: Option<u32>,
    /// `None` leaves the schema default (1.0); `Some` sets it, so a test
    /// can drive an out-of-range value (story #44).
    pub opacity: Option<f32>,
    /// Marks the node a mask (story #44).
    pub mask: bool,
    /// `Some((x, y, width, height))` writes a `FixedSizeLayout`; `None` omits
    /// the struct, which is what almost every test here wants. Added for issue
    /// #1048's authored-box rules.
    pub layout: Option<(f32, f32, f32, f32)>,
    /// A declared placeholder (story #1126): the two string indices, and an
    /// optional image index for its `interim_fill`. `None` writes no
    /// `Placeholder` table, which is what every other test here wants.
    pub placeholder: Option<PlaceholderSpec>,
}

/// One declared placeholder, in the terms this suite drives:
/// `(contribution_id, fragment_ref, interim image index)`.
#[derive(Clone, Copy, Default)]
pub struct PlaceholderSpec {
    pub contribution_id: Option<u32>,
    pub fragment_ref: Option<u32>,
    pub interim_image: Option<u32>,
    /// `Some` writes a `declared_size`; `None` omits the struct, which is the
    /// undeclared state.
    pub declared_size: Option<(f32, f32)>,
}

#[derive(Clone)]
pub enum PaintSpec {
    Solid,
    Gradient { kind: GradientKind, stops: Vec<f32> },
    Image { index: u32, scale_mode: ScaleMode },
    Stroke { width: f32, align: StrokeAlign },
}

impl Doc {
    pub fn node(mut self, spec: NodeSpec) -> Self {
        self.nodes.push(spec);
        self
    }

    pub fn paint(mut self, spec: PaintSpec) -> Self {
        self.paints.push(spec);
        self
    }

    /// `n` well-formed asset-table entries, each with a distinct filler hash
    /// — a document-level test does not care about the payload, only that
    /// each entry is self-consistent (a 32-byte hash, a non-zero extent).
    pub fn images(mut self, n: usize) -> Self {
        let start = self.images.len();
        self.images.extend((0..n).map(|i| ImageSpec {
            hash: [(start + i) as u8 + 7; 32],
            format: ImageFormat::Png,
            width: 4,
            height: 4,
        }));
        self
    }

    /// One asset-table entry whose intrinsic extent is zero on both axes.
    pub fn zero_extent_image(mut self) -> Self {
        self.images.push(ImageSpec {
            hash: [9u8; 32],
            format: ImageFormat::Png,
            width: 0,
            height: 0,
        });
        self
    }

    /// One well-formed asset carrying a container format this build does not
    /// know.
    pub fn image_with_format(mut self, format: ImageFormat) -> Self {
        self.images.push(ImageSpec {
            hash: [10u8; 32],
            format,
            width: 4,
            height: 4,
        });
        self
    }

    pub fn strings(mut self, n: usize) -> Self {
        assert!(self.strings.is_empty(), "the string pool is set once");
        self.strings = vec!["hi"; n];
        self
    }

    /// The string pool stated by content, so a test can assert which entry an
    /// index resolves to.
    pub fn named_strings(mut self, names: &[&'static str]) -> Self {
        assert!(self.strings.is_empty(), "the string pool is set once");
        self.strings = names.to_vec();
        self
    }

    pub fn text_styles(mut self, n: usize) -> Self {
        self.text_styles = n;
        self
    }

    pub fn build(self) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();

        let paints: Vec<WIPOffset<Paint>> = self
            .paints
            .iter()
            .map(|spec| build_paint(&mut b, spec))
            .collect();

        let assets: Vec<WIPOffset<AssetEntry>> = self
            .images
            .iter()
            .map(|spec| {
                let hash = b.create_vector(&spec.hash);
                AssetEntry::create(
                    &mut b,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: spec.format,
                        width: spec.width,
                        height: spec.height,
                        kind: dashbuf::AssetKind::Image,
                    },
                )
            })
            .collect();

        let strings: Vec<WIPOffset<&str>> =
            self.strings.iter().map(|s| b.create_string(s)).collect();

        let text_styles: Vec<WIPOffset<TextStyle>> = (0..self.text_styles)
            .map(|_| {
                let family = b.create_string("Inter");
                TextStyle::create(
                    &mut b,
                    &TextStyleArgs {
                        family: Some(family),
                        size: 16.0,
                        weight: 400,
                        color: Some(&red()),
                        ..Default::default()
                    },
                )
            })
            .collect();

        let nodes: Vec<WIPOffset<Node>> = self
            .nodes
            .iter()
            .map(|spec| {
                let name = b.create_string(spec.name);
                let legacy = spec.legacy_paint.then(|| {
                    SolidFill::create(
                        &mut b,
                        &SolidFillArgs {
                            color: Some(&red()),
                        },
                    )
                });
                let layout = spec
                    .layout
                    .map(|(x, y, w, h)| dashbuf::FixedSizeLayout::new(x, y, w, h));
                let placeholder = spec.placeholder.map(|ph| {
                    let declared = ph.declared_size.map(|(w, h)| Vec2::new(w, h));
                    let interim = ph.interim_image.map(|image| {
                        ImageFill::create(
                            &mut b,
                            &ImageFillArgs {
                                image,
                                scale_mode: ScaleMode::Fill,
                                ..Default::default()
                            },
                        )
                    });
                    dashbuf::Placeholder::create(
                        &mut b,
                        &dashbuf::PlaceholderArgs {
                            contribution_id: ph.contribution_id.unwrap_or(dashbuf::NO_CONTRIBUTION),
                            fragment_ref: ph.fragment_ref.unwrap_or(dashbuf::NO_FRAGMENT),
                            declared_size: declared.as_ref(),
                            interim_fill_type: if interim.is_some() {
                                Fill::ImageFill
                            } else {
                                Fill::NONE
                            },
                            interim_fill: interim.map(|f| f.as_union_value()),
                        },
                    )
                });
                Node::create(
                    &mut b,
                    &NodeArgs {
                        name: Some(name),
                        parent: spec.parent.unwrap_or(dashbuf::NO_PARENT),
                        paint: legacy,
                        paint_entry: spec.paint_entry.unwrap_or(NO_PAINT),
                        text: spec.text.unwrap_or(dashbuf::NO_TEXT),
                        text_style: spec.text_style.unwrap_or(dashbuf::NO_TEXT_STYLE),
                        opacity: spec.opacity.unwrap_or(1.0),
                        mask: spec.mask,
                        layout: layout.as_ref(),
                        placeholder,
                        ..Default::default()
                    },
                )
            })
            .collect();

        let nodes = b.create_vector(&nodes);
        let paints = (!paints.is_empty()).then(|| b.create_vector(&paints));
        let assets = (!assets.is_empty()).then(|| b.create_vector(&assets));
        let strings = (!strings.is_empty()).then(|| b.create_vector(&strings));
        let text_styles = (!text_styles.is_empty()).then(|| b.create_vector(&text_styles));

        let doc = Document::create(
            &mut b,
            &DocumentArgs {
                nodes: Some(nodes),
                assets,
                paints,
                strings,
                text_styles,
                ..Default::default()
            },
        );
        b.finish(doc, None);
        b.finished_data().to_vec()
    }
}

pub fn build_paint<'a>(b: &mut FlatBufferBuilder<'a>, spec: &PaintSpec) -> WIPOffset<Paint<'a>> {
    match spec {
        PaintSpec::Solid => {
            let fill = SolidFill::create(
                b,
                &SolidFillArgs {
                    color: Some(&red()),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::SolidFill,
                    fill: Some(fill.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Gradient { kind, stops } => {
            let stops: Vec<GradientStop> = stops
                .iter()
                .map(|&offset| GradientStop::new(offset, &red()))
                .collect();
            let stops = b.create_vector(&stops);
            let gradient = Gradient::create(
                b,
                &GradientArgs {
                    kind: *kind,
                    handle_origin: Some(&Vec2::new(0.0, 0.0)),
                    handle_primary: Some(&Vec2::new(1.0, 0.0)),
                    handle_secondary: Some(&Vec2::new(0.0, 1.0)),
                    stops: Some(stops),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::Gradient,
                    fill: Some(gradient.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Image { index, scale_mode } => {
            let image_fill = ImageFill::create(
                b,
                &ImageFillArgs {
                    image: *index,
                    scale_mode: *scale_mode,
                    transform: None,
                    tile_scale: 1.0,
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::ImageFill,
                    fill: Some(image_fill.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Stroke { width, align } => {
            let stroke = Stroke::create(
                b,
                &StrokeArgs {
                    width: *width,
                    align: *align,
                    color: Some(&red()),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    stroke: Some(stroke),
                    ..Default::default()
                },
            )
        }
    }
}

pub fn named(name: &'static str) -> NodeSpec {
    NodeSpec {
        name,
        ..Default::default()
    }
}
