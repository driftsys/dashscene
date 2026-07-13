//! The R7 append-only guard (issue #64): a frozen `.dsb` byte fixture,
//! written once by an older schema generation and checked into the
//! repo, decoded here with the *current* generated bindings.
//!
//! Why this file exists: the other three suites in this directory build
//! and decode with the same freshly generated bindings, so writer and
//! reader move together. A schema edit that shifts a field id or a
//! union discriminant — which breaks every `.dsb` already written to
//! disk — leaves them fully green. Only bytes that predate the edit can
//! catch it, and only if they are bytes, not a builder call.
//!
//! What it catches: a field-id shift usually still *decodes* (the
//! verifier is happy, the vtable slot exists) and quietly returns
//! another field's value or the field's default. So every assertion
//! below is on a value, and every value is deliberately non-default —
//! a sentinel that reads back as 0, an `Angular` gradient that reads
//! back as `Linear`, a `paint_entry` that reads back as `NO_PAINT`
//! are exactly the silent-wrong-value failures this suite exists to
//! turn into a red test.
//!
//! # Regenerating the fixture
//!
//!     UPDATE_DSB_FIXTURE=1 cargo test -p dashbuf --test schema_evolution
//!
//! rewrites `tests/fixtures/v0_5_document.dsb` from `build_fixture()`
//! below, then decodes what it wrote. **This is not a routine step.**
//! The fixture's value is precisely that its bytes never change: a
//! regeneration erases the evidence of whatever broke them. Regenerate
//! only on a deliberate, reviewed format-generation bump — the same
//! posture as `goldens/`' `UPDATE_GOLDENS=1`, and never as a way to
//! make this suite go green. See
//! `docs/decisions/dsb-frozen-fixture-r7-guard.md`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{env, fs};

use dashbuf::{
    AxisSizing, Color, CornerRadii, CrossAxisAlign, Document, DocumentArgs, EdgeInsets, Fill,
    FixedSizeLayout, Gradient, GradientArgs, GradientKind, GradientStop, Image, ImageArgs,
    ImageFill, ImageFillArgs, ImageFormat, LayoutConstraints, LayoutConstraintsArgs,
    LayoutContainer, LayoutContainerArgs, LayoutMode, MainAxisAlign, Mat23, NO_PAINT, NO_PARENT,
    NO_TEXT, NO_TEXT_STYLE, Node, NodeArgs, Paint, PaintArgs, ScaleMode, SolidFill, SolidFillArgs,
    Stroke, StrokeAlign, StrokeArgs, TextStyle, TextStyleArgs, Vec2, root_as_document,
};
use flatbuffers::FlatBufferBuilder;

/// The frozen fixture, relative to the crate root.
const FIXTURE: &str = "tests/fixtures/v0_5_document.dsb";

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

/// The checked-in fixture bytes, read once per test binary.
///
/// With `UPDATE_DSB_FIXTURE=1` the fixture is rewritten from
/// [`build_fixture`] before it is read back — see the module docs for
/// when that is legitimate. `OnceLock` serializes the rewrite against
/// the parallel test threads that read through this same function.
fn fixture_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let path = fixture_path();
        match env::var_os("UPDATE_DSB_FIXTURE") {
            None => {}
            Some(value) if value == "1" => {
                fs::write(&path, build_fixture()).expect("write .dsb fixture");
                eprintln!("UPDATE_DSB_FIXTURE: wrote {}", path.display());
            }
            Some(other) => panic!(
                "UPDATE_DSB_FIXTURE={} is not recognized — set \
                 UPDATE_DSB_FIXTURE=1 (regenerating destroys the frozen \
                 bytes this guard is made of, so only the documented \
                 value is accepted)",
                other.to_string_lossy()
            ),
        }
        fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "cannot read the frozen .dsb fixture {}: {error}. It is \
                 checked into the repo; a clean checkout has it. Do not \
                 regenerate it to make a test pass — see the module docs.",
                path.display()
            )
        })
    })
}

fn document() -> Document<'static> {
    root_as_document(fixture_bytes()).expect("the frozen fixture is a valid dashbuf document")
}

fn node(index: usize) -> Node<'static> {
    document().nodes().expect("nodes present").get(index)
}

fn paint(index: usize) -> Paint<'static> {
    document().paints().expect("paint pool present").get(index)
}

// ---------------------------------------------------------------------
// Decode assertions — the guard proper. Every value below was written
// by the schema generation frozen into the fixture; each must still
// read back identically through today's bindings.
// ---------------------------------------------------------------------

/// The tree shape: four nodes in DFS order, with the root carrying the
/// `NO_PARENT` sentinel and the children indexing back at the root.
#[test]
fn frozen_node_tree_reads_back() {
    let nodes = document().nodes().expect("nodes present");
    assert_eq!(nodes.len(), 4);

    let root = nodes.get(0);
    assert_eq!(root.name(), Some("root"));
    assert_eq!(root.parent(), NO_PARENT);
    let layout = root.layout().expect("root layout present");
    assert_eq!(
        (layout.x(), layout.y(), layout.width(), layout.height()),
        (8.0, 4.0, 320.0, 200.0)
    );

    for index in 1..4 {
        assert_eq!(
            nodes.get(index).parent(),
            0,
            "node {index} parents the root"
        );
    }
    assert_eq!(nodes.get(1).name(), Some("gradient-child"));
    assert_eq!(nodes.get(2).name(), Some("text-child"));
    assert_eq!(nodes.get(3).name(), Some("bare-child"));
}

/// The four sentinel-defaulted `Node` fields. A field-id shift most
/// often surfaces here: a sentinel field read out of the wrong vtable
/// slot comes back as 0 (a valid-looking index into pool entry 0), and
/// a real index comes back as the sentinel.
#[test]
fn frozen_node_sentinels_read_back() {
    // `bare-child` set none of the four: all read as their sentinels.
    let bare = node(3);
    assert_eq!(bare.paint_entry(), NO_PAINT);
    assert_eq!(bare.text(), NO_TEXT);
    assert_eq!(bare.text_style(), NO_TEXT_STYLE);
    assert_eq!(bare.flex(), None);
    assert_eq!(bare.constraints(), None);

    // …while the nodes that did set them read back the written index,
    // each deliberately non-zero where the pool allows it.
    assert_eq!(node(0).paint_entry(), 0);
    assert_eq!(node(1).paint_entry(), 1);
    assert_eq!(node(2).paint_entry(), 2);
    assert_eq!(node(2).text(), 1);
    assert_eq!(node(2).text_style(), 0);
    // A node with paint but no text keeps the text sentinels.
    assert_eq!(node(1).text(), NO_TEXT);
    assert_eq!(node(1).text_style(), NO_TEXT_STYLE);
}

/// The v0.1 legacy inline `Node.paint` shorthand, still readable
/// alongside the v0.3 pool (`document-paint-pool-and-legacy-paint-field.md`).
#[test]
fn frozen_legacy_inline_paint_reads_back() {
    let color = node(0)
        .paint()
        .expect("legacy inline paint present")
        .color()
        .expect("color present");
    assert_eq!(
        (color.r(), color.g(), color.b(), color.a()),
        (0.0, 0.0, 1.0, 0.5)
    );
}

/// Pool entry 0: a solid fill plus stroke, corners, and `clip = true`.
/// The `Fill` union's discriminant is stored in the bytes as a raw
/// integer; reordering the union's members makes this entry decode as
/// some other fill kind.
#[test]
fn frozen_solid_fill_entry_reads_back() {
    let entry = paint(0);
    assert_eq!(entry.fill_type(), Fill::SolidFill);
    let color = entry
        .fill_as_solid_fill()
        .expect("solid fill present")
        .color()
        .expect("color present");
    assert_eq!(
        (color.r(), color.g(), color.b(), color.a()),
        (1.0, 0.0, 0.0, 1.0)
    );

    let stroke = entry.stroke().expect("stroke present");
    assert_eq!(stroke.width(), 2.5);
    assert_eq!(stroke.align(), StrokeAlign::Outside);
    let stroke_color = stroke.color();
    assert_eq!(
        (
            stroke_color.r(),
            stroke_color.g(),
            stroke_color.b(),
            stroke_color.a()
        ),
        (0.0, 1.0, 0.0, 1.0)
    );

    let corners = entry.corners().expect("corners present");
    assert_eq!(
        (
            corners.top_left(),
            corners.top_right(),
            corners.bottom_right(),
            corners.bottom_left()
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
    // Written `true` against the schema default of `false`: a shifted
    // id reads the default and this assertion is what notices.
    assert!(entry.clip());
}

/// Pool entry 1: an `Angular` gradient. Both the union discriminant and
/// the `GradientKind` enum value are non-zero on purpose — a reordered
/// enum decodes as a different, still-valid kind.
#[test]
fn frozen_gradient_fill_entry_reads_back() {
    let entry = paint(1);
    assert_eq!(entry.fill_type(), Fill::Gradient);
    let gradient = entry.fill_as_gradient().expect("gradient fill present");
    assert_eq!(gradient.kind(), GradientKind::Angular);

    let origin = gradient.handle_origin();
    let primary = gradient.handle_primary();
    let secondary = gradient.handle_secondary();
    assert_eq!((origin.x(), origin.y()), (0.25, 0.5));
    assert_eq!((primary.x(), primary.y()), (1.0, 0.0));
    assert_eq!((secondary.x(), secondary.y()), (0.0, 1.0));

    let stops = gradient.stops();
    assert_eq!(stops.len(), 2);
    assert_eq!(stops.get(0).offset(), 0.0);
    assert_eq!(stops.get(0).color().r(), 1.0);
    assert_eq!(stops.get(1).offset(), 1.0);
    assert_eq!(stops.get(1).color().b(), 1.0);
    assert_eq!(stops.get(1).color().a(), 0.5);
    // Entry 1 stroked nothing and clipped nothing: absence still reads
    // as absence, not as entry 0's stroke.
    assert_eq!(entry.stroke(), None);
    assert!(!entry.clip());
}

/// Pool entry 2: an image fill, with every field written to a
/// non-default value, and its asset resolved through `Document.images`.
#[test]
fn frozen_image_fill_entry_reads_back() {
    let entry = paint(2);
    assert_eq!(entry.fill_type(), Fill::ImageFill);
    let fill = entry.fill_as_image_fill().expect("image fill present");
    assert_eq!(fill.image(), 1);
    assert_eq!(fill.scale_mode(), ScaleMode::Crop);
    assert_eq!(fill.tile_scale(), 2.0);
    let transform = fill.transform().expect("transform present");
    assert_eq!(
        (
            transform.a(),
            transform.b(),
            transform.c(),
            transform.d(),
            transform.tx(),
            transform.ty()
        ),
        (1.0, 0.0, 0.0, 1.0, 0.25, 0.5)
    );

    let images = document().images().expect("images present");
    assert_eq!(images.len(), 2);
    // The fill points at index 1, not the decoy at 0.
    let image = images.get(fill.image() as usize);
    assert_eq!(image.format(), ImageFormat::Png);
    assert_eq!(image.bytes().expect("bytes present").bytes(), [1, 2, 3, 4]);
    assert_eq!(images.get(0).bytes().expect("bytes present").bytes(), [9]);
}

/// The container-side flex table.
#[test]
fn frozen_flex_container_reads_back() {
    let flex = node(0).flex().expect("root is a flex container");
    assert_eq!(flex.mode(), LayoutMode::Vertical);
    assert_eq!(flex.gap(), 12.0);
    assert_eq!(flex.main_align(), MainAxisAlign::SpaceBetween);
    assert_eq!(flex.cross_align(), CrossAxisAlign::End);
    let padding = flex.padding().expect("padding present");
    assert_eq!(
        (
            padding.left(),
            padding.top(),
            padding.right(),
            padding.bottom()
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
}

/// The child-side flex table, including the optional-scalar min/max
/// (absent means unconstrained — never a sentinel) and the negative
/// margin the negative-gap lowering writes.
#[test]
fn frozen_flex_constraints_read_back() {
    let constraints = node(1).constraints().expect("constraints present");
    assert_eq!(constraints.sizing_h(), AxisSizing::Fill);
    assert_eq!(constraints.sizing_v(), AxisSizing::Hug);
    assert_eq!(constraints.min_width(), Some(10.0));
    assert_eq!(constraints.max_width(), Some(100.0));
    assert_eq!(constraints.min_height(), Some(20.0));
    // Written absent: still unconstrained, never 0.0.
    assert_eq!(constraints.max_height(), None);
    let margin = constraints.margin().expect("margin present");
    assert_eq!(
        (margin.left(), margin.top(), margin.right(), margin.bottom()),
        (-4.0, 0.0, 5.0, 0.0)
    );

    // The text child wrote only sizing, so its min/max stay absent.
    let text_constraints = node(2).constraints().expect("constraints present");
    assert_eq!(text_constraints.sizing_h(), AxisSizing::Fixed);
    assert_eq!(text_constraints.min_width(), None);
    assert_eq!(text_constraints.margin(), None);
}

/// The two text pools, reached through the node's sentinel-indexed
/// references.
#[test]
fn frozen_text_pools_read_back() {
    let doc = document();
    let strings = doc.strings().expect("string pool present");
    assert_eq!(strings.len(), 2);
    assert_eq!(strings.get(0), "unreferenced");
    assert_eq!(strings.get(1), "Hello, dashscene");

    let text_node = node(2);
    assert_eq!(strings.get(text_node.text() as usize), "Hello, dashscene");

    let styles = doc.text_styles().expect("text style pool present");
    assert_eq!(styles.len(), 1);
    let style = styles.get(text_node.text_style() as usize);
    assert_eq!(style.family(), "Inter");
    assert_eq!(style.size(), 16.0);
    // Written 700 against the schema default of 400.
    assert_eq!(style.weight(), 700);
    let color = style.color().expect("style color present");
    assert_eq!(
        (color.r(), color.g(), color.b(), color.a()),
        (0.1, 0.2, 0.3, 1.0)
    );
}

// ---------------------------------------------------------------------
// The writer. Runs only under UPDATE_DSB_FIXTURE=1 — see the module
// docs. Editing it changes nothing until the fixture is regenerated,
// which is the point: the bytes on disk, not this function, are the
// older schema generation.
// ---------------------------------------------------------------------

/// Builds the fixture document: four nodes, three paint-pool entries
/// (one per `Fill` union member), two images, both text pools, and both
/// flex tables — every field written to a value distinguishable from
/// its default.
fn build_fixture() -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();

    let decoy_bytes = b.create_vector(&[9u8]);
    let decoy_image = Image::create(
        &mut b,
        &ImageArgs {
            format: ImageFormat::Png,
            bytes: Some(decoy_bytes),
        },
    );
    let real_bytes = b.create_vector(&[1u8, 2, 3, 4]);
    let real_image = Image::create(
        &mut b,
        &ImageArgs {
            format: ImageFormat::Png,
            bytes: Some(real_bytes),
        },
    );
    let images = b.create_vector(&[decoy_image, real_image]);

    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
        },
    );
    let stroke = Stroke::create(
        &mut b,
        &StrokeArgs {
            width: 2.5,
            align: StrokeAlign::Outside,
            color: Some(&Color::new(0.0, 1.0, 0.0, 1.0)),
        },
    );
    let solid_entry = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            stroke: Some(stroke),
            corners: Some(&CornerRadii::new(1.0, 2.0, 3.0, 4.0)),
            clip: true,
        },
    );

    let stops = b.create_vector(&[
        GradientStop::new(0.0, &Color::new(1.0, 0.0, 0.0, 1.0)),
        GradientStop::new(1.0, &Color::new(0.0, 0.0, 1.0, 0.5)),
    ]);
    let gradient = Gradient::create(
        &mut b,
        &GradientArgs {
            kind: GradientKind::Angular,
            handle_origin: Some(&Vec2::new(0.25, 0.5)),
            handle_primary: Some(&Vec2::new(1.0, 0.0)),
            handle_secondary: Some(&Vec2::new(0.0, 1.0)),
            stops: Some(stops),
        },
    );
    let gradient_entry = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::Gradient,
            fill: Some(gradient.as_union_value()),
            ..Default::default()
        },
    );

    let image_fill = ImageFill::create(
        &mut b,
        &ImageFillArgs {
            image: 1,
            scale_mode: ScaleMode::Crop,
            transform: Some(&Mat23::new(1.0, 0.0, 0.0, 1.0, 0.25, 0.5)),
            tile_scale: 2.0,
        },
    );
    let image_entry = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::ImageFill,
            fill: Some(image_fill.as_union_value()),
            ..Default::default()
        },
    );
    let paints = b.create_vector(&[solid_entry, gradient_entry, image_entry]);

    let unreferenced = b.create_string("unreferenced");
    let hello = b.create_string("Hello, dashscene");
    let strings = b.create_vector(&[unreferenced, hello]);

    let family = b.create_string("Inter");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            size: 16.0,
            weight: 700,
            color: Some(&Color::new(0.1, 0.2, 0.3, 1.0)),
        },
    );
    let text_styles = b.create_vector(&[style]);

    let root_flex = LayoutContainer::create(
        &mut b,
        &LayoutContainerArgs {
            mode: LayoutMode::Vertical,
            gap: 12.0,
            padding: Some(&EdgeInsets::new(1.0, 2.0, 3.0, 4.0)),
            main_align: MainAxisAlign::SpaceBetween,
            cross_align: CrossAxisAlign::End,
        },
    );
    let root_name = b.create_string("root");
    let legacy_paint = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(0.0, 0.0, 1.0, 0.5)),
        },
    );
    let root = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(root_name),
            parent: NO_PARENT,
            layout: Some(&FixedSizeLayout::new(8.0, 4.0, 320.0, 200.0)),
            paint: Some(legacy_paint),
            paint_entry: 0,
            flex: Some(root_flex),
            ..Default::default()
        },
    );

    let gradient_constraints = LayoutConstraints::create(
        &mut b,
        &LayoutConstraintsArgs {
            sizing_h: AxisSizing::Fill,
            sizing_v: AxisSizing::Hug,
            min_width: Some(10.0),
            max_width: Some(100.0),
            min_height: Some(20.0),
            max_height: None,
            margin: Some(&EdgeInsets::new(-4.0, 0.0, 5.0, 0.0)),
        },
    );
    let gradient_name = b.create_string("gradient-child");
    let gradient_child = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(gradient_name),
            parent: 0,
            paint_entry: 1,
            constraints: Some(gradient_constraints),
            ..Default::default()
        },
    );

    let text_constraints = LayoutConstraints::create(
        &mut b,
        &LayoutConstraintsArgs {
            sizing_h: AxisSizing::Fixed,
            sizing_v: AxisSizing::Fixed,
            ..Default::default()
        },
    );
    let text_name = b.create_string("text-child");
    let text_child = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(text_name),
            parent: 0,
            paint_entry: 2,
            text: 1,
            text_style: 0,
            constraints: Some(text_constraints),
            ..Default::default()
        },
    );

    let bare_name = b.create_string("bare-child");
    let bare_child = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(bare_name),
            parent: 0,
            ..Default::default()
        },
    );

    let nodes = b.create_vector(&[root, gradient_child, text_child, bare_child]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            images: Some(images),
            paints: Some(paints),
            strings: Some(strings),
            text_styles: Some(text_styles),
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}
