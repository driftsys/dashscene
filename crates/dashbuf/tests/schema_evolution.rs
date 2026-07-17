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
    AxisSizing, Binding, BindingArgs, BindingChannel, BindingTransform, Color, CornerRadii,
    CrossAxisAlign, Document, DocumentArgs, EdgeInsets, Fill, FixedSizeLayout, Gradient,
    GradientArgs, GradientKind, GradientStop, GridTrack, GridTrackArgs, GridTrackSizing, Image,
    ImageArgs, ImageFill, ImageFillArgs, ImageFormat, LayoutConstraints, LayoutConstraintsArgs,
    LayoutContainer, LayoutContainerArgs, LayoutMode, MainAxisAlign, Mat23, NO_PAINT, NO_PARENT,
    NO_TEXT, NO_TEXT_STYLE, Node, NodeArgs, Paint, PaintArgs, ScaleMode, SignalDecl,
    SignalDeclArgs, SolidFill, SolidFillArgs, Stroke, StrokeAlign, StrokeArgs, TextStyle,
    TextStyleArgs, TransformScale, TransformScaleArgs, VariantFill, VariantFillArgs, VariantMember,
    VariantMemberArgs, VariantOverride, VariantOverrideArgs, VariantPropValue, VariantSet,
    VariantSetArgs, VariantX, VariantXArgs, Vec2, root_as_document,
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

/// The tree shape: five nodes in DFS order, with the root carrying the
/// `NO_PARENT` sentinel and the children indexing back at the root.
#[test]
fn frozen_node_tree_reads_back() {
    let nodes = document().nodes().expect("nodes present");
    assert_eq!(nodes.len(), 5);

    let root = nodes.get(0);
    assert_eq!(root.name(), Some("root"));
    assert_eq!(root.parent(), NO_PARENT);
    let layout = root.layout().expect("root layout present");
    assert_eq!(
        (layout.x(), layout.y(), layout.width(), layout.height()),
        (8.0, 4.0, 320.0, 200.0)
    );

    for index in 1..5 {
        assert_eq!(
            nodes.get(index).parent(),
            0,
            "node {index} parents the root"
        );
    }
    assert_eq!(nodes.get(1).name(), Some("gradient-child"));
    assert_eq!(nodes.get(2).name(), Some("text-child"));
    assert_eq!(nodes.get(3).name(), Some("bare-child"));
    assert_eq!(nodes.get(4).name(), Some("grid-child"));
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

/// The v0.8 layout appends (story #43): the enum tail members
/// (`Grid` = 4, `Baseline` = 3), the cross-axis gap, both track lists
/// (a `Fixed` and a `Fraction` track per axis, at per-axis-distinct
/// values), and the grid placement — every value non-default, so a
/// shifted field id or discriminant reads back wrong here.
#[test]
fn frozen_v08_layout_fields_read_back() {
    let grid = node(4);
    let flex = grid.flex().expect("grid-child is a container");
    assert_eq!(flex.mode(), LayoutMode::Grid);
    assert_eq!(flex.cross_align(), CrossAxisAlign::Baseline);
    assert_eq!(flex.gap(), 12.0);
    assert_eq!(flex.cross_gap(), Some(16.0));

    let rows = flex.grid_rows().expect("row tracks present");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.get(0).sizing(), GridTrackSizing::Fixed);
    assert_eq!(rows.get(0).value(), 96.0);
    assert_eq!(rows.get(1).sizing(), GridTrackSizing::Fraction);
    assert_eq!(rows.get(1).value(), 2.0);
    let columns = flex.grid_columns().expect("column tracks present");
    assert_eq!(columns.len(), 2);
    assert_eq!(columns.get(0).sizing(), GridTrackSizing::Fraction);
    assert_eq!(columns.get(0).value(), 1.0);
    assert_eq!(columns.get(1).sizing(), GridTrackSizing::Fixed);
    assert_eq!(columns.get(1).value(), 160.0);

    let constraints = grid.constraints().expect("constraints present");
    assert_eq!(constraints.grid_row(), Some(1));
    assert_eq!(constraints.grid_column(), Some(2));
    assert_eq!(constraints.grid_row_span(), 2);
    assert_eq!(constraints.grid_column_span(), 3);

    // The nodes that predate v0.8 keep the appends absent (spans read
    // their default of 1) — the append cost old documents nothing.
    let old = node(0).flex().expect("root flex present");
    assert_eq!(old.cross_gap(), None);
    assert!(old.grid_rows().is_none());
    assert!(old.grid_columns().is_none());
    let old_constraints = node(1).constraints().expect("constraints present");
    assert_eq!(old_constraints.grid_row(), None);
    assert_eq!(old_constraints.grid_column(), None);
    assert_eq!(old_constraints.grid_row_span(), 1);
    assert_eq!(old_constraints.grid_column_span(), 1);
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

/// The v0.4 variant table (story #20): one set, two members, one
/// override of each of two `VariantPropValue` kinds. `active_member` is
/// written non-zero against the schema default of 0 — the same
/// non-default-value discipline as `Paint.clip` above.
#[test]
fn frozen_variant_set_reads_back() {
    let sets = document().variant_sets().expect("variant_sets present");
    assert_eq!(sets.len(), 1);
    let set = sets.get(0);
    assert_eq!(set.active_member(), 1);

    let members = set.members().expect("members present");
    assert_eq!(members.len(), 2);
    assert_eq!(members.get(0).name(), Some("Default"));
    assert!(members.get(0).overrides().is_none_or(|o| o.is_empty()));

    let hover = members.get(1);
    assert_eq!(hover.name(), Some("Hover"));
    let overrides = hover.overrides().expect("overrides present");
    assert_eq!(overrides.len(), 2);

    let x = overrides.get(0);
    assert_eq!(x.node(), 1);
    assert_eq!(x.value_type(), VariantPropValue::VariantX);
    assert_eq!(
        x.value_as_variant_x().expect("VariantX present").value(),
        99.0
    );

    let fill = overrides.get(1);
    assert_eq!(fill.node(), 0);
    assert_eq!(fill.value_type(), VariantPropValue::VariantFill);
    let color = fill
        .value_as_variant_fill()
        .expect("VariantFill present")
        .color();
    assert_eq!(
        (color.r(), color.g(), color.b(), color.a()),
        (0.2, 0.4, 0.6, 1.0)
    );
}

/// The v0.7 binding tables (story #167): two signal declarations (one
/// named, one anonymous) and two binding rows — one with the union-NONE
/// identity transform, one with a `Scale` transform. `channel` is
/// written non-zero (`Gap` = 4) against the enum default of `X` = 0,
/// and the second row's `signal`/`node` are non-zero, the same
/// non-default-value discipline as the suites above.
#[test]
fn frozen_binding_tables_read_back() {
    let doc = document();
    let signals = doc.signals().expect("signals present");
    assert_eq!(signals.len(), 2);
    assert_eq!(signals.get(0).name(), Some("size/gap"));
    assert_eq!(signals.get(0).initial(), 16.0);
    assert_eq!(signals.get(1).name(), None);
    assert_eq!(signals.get(1).initial(), 0.25);

    let bindings = doc.bindings().expect("bindings present");
    assert_eq!(bindings.len(), 3);

    let gap = bindings.get(0);
    assert_eq!(gap.signal(), 0);
    assert_eq!(gap.node(), 0);
    assert_eq!(gap.channel(), BindingChannel::Gap);
    // Union NONE is the identity transform.
    assert_eq!(gap.transform_type(), BindingTransform::NONE);

    let scaled = bindings.get(1);
    assert_eq!(scaled.signal(), 1);
    assert_eq!(scaled.node(), 1);
    assert_eq!(scaled.channel(), BindingChannel::FillR);
    assert_eq!(scaled.transform_type(), BindingTransform::TransformScale);
    assert_eq!(
        scaled
            .transform_as_transform_scale()
            .expect("TransformScale present")
            .factor(),
        2.0
    );

    // The v0.8 Opacity channel (9): written against the default X = 0, so
    // a renumbered enum reads a different channel back.
    let opacity = bindings.get(2);
    assert_eq!(opacity.signal(), 1);
    assert_eq!(opacity.node(), 2);
    assert_eq!(opacity.channel(), BindingChannel::Opacity);
    assert_eq!(opacity.transform_type(), BindingTransform::NONE);
}

/// The v0.8 masks + group-opacity node fields (story #44), each written
/// against its schema default so a shifted field id reads the default and
/// this assertion is what notices.
#[test]
fn frozen_node_masks_and_opacity_read_back() {
    // root wrote opacity 0.5 against the 1.0 default.
    assert_eq!(node(0).opacity(), 0.5);
    assert!(!node(0).mask());
    assert!(node(0).visible());

    // gradient-child wrote mask = true against the false default.
    assert!(node(1).mask());
    assert_eq!(node(1).opacity(), 1.0);
    assert!(node(1).visible());

    // bare-child wrote visible = false against the true default.
    assert!(!node(3).visible());
    assert_eq!(node(3).opacity(), 1.0);
    assert!(!node(3).mask());
}

// ---------------------------------------------------------------------
// The writer. Runs only under UPDATE_DSB_FIXTURE=1 — see the module
// docs. Editing it changes nothing until the fixture is regenerated,
// which is the point: the bytes on disk, not this function, are the
// older schema generation.
// ---------------------------------------------------------------------

/// Builds the fixture document: five nodes, three paint-pool entries
/// (one per `Fill` union member), two images, both text pools, both
/// flex tables, and the v0.8 layout appends (cross gap, grid tracks,
/// grid placement, the Grid/Baseline enum tails) — every field written
/// to a value distinguishable from its default.
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
            // The v0.8 appends stay absent here — grid-child below is
            // the node that writes them.
            cross_gap: None,
            grid_rows: None,
            grid_columns: None,
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
            // v0.8 (story #44): a group opacity written non-default (0.5
            // against the 1.0 default), so a shifted field id reads the
            // default and the guard notices.
            opacity: 0.5,
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
            // The v0.8 placement appends stay absent here — grid-child
            // below is the node that writes them.
            grid_row: None,
            grid_column: None,
            grid_row_span: 1,
            grid_column_span: 1,
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
            // v0.8 (story #44): a mask node, written true against the
            // false default.
            mask: true,
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
            // v0.8 (story #44): a hidden node, written false against the
            // true default (debt #143).
            visible: false,
            ..Default::default()
        },
    );

    // v0.8 layout appends (story #43): every new field at a value
    // distinguishable from its default — mode Grid (4) and Baseline (3)
    // are the appended enum tails, the tracks mix both sizings at
    // distinct values per axis, and the placement writes non-default
    // anchors and spans.
    let row_fixed = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fixed,
            value: 96.0,
        },
    );
    let row_flex = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fraction,
            value: 2.0,
        },
    );
    let grid_rows = b.create_vector(&[row_fixed, row_flex]);
    let col_flex = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fraction,
            value: 1.0,
        },
    );
    let col_fixed = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fixed,
            value: 160.0,
        },
    );
    let grid_columns = b.create_vector(&[col_flex, col_fixed]);
    let grid_flex = LayoutContainer::create(
        &mut b,
        &LayoutContainerArgs {
            mode: LayoutMode::Grid,
            gap: 12.0,
            padding: None,
            main_align: MainAxisAlign::Start,
            cross_align: CrossAxisAlign::Baseline,
            cross_gap: Some(16.0),
            grid_rows: Some(grid_rows),
            grid_columns: Some(grid_columns),
        },
    );
    let grid_constraints = LayoutConstraints::create(
        &mut b,
        &LayoutConstraintsArgs {
            sizing_h: AxisSizing::Fill,
            sizing_v: AxisSizing::Fill,
            grid_row: Some(1),
            grid_column: Some(2),
            grid_row_span: 2,
            grid_column_span: 3,
            ..Default::default()
        },
    );
    let grid_name = b.create_string("grid-child");
    let grid_child = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(grid_name),
            parent: 0,
            flex: Some(grid_flex),
            constraints: Some(grid_constraints),
            ..Default::default()
        },
    );

    let nodes = b.create_vector(&[root, gradient_child, text_child, bare_child, grid_child]);

    // v0.4 variant table (story #20): one set, two members, one override
    // of each of two `VariantPropValue` kinds — enough to catch a
    // shifted field id or reordered union discriminant the same way the
    // suites above do.
    let variant_x = VariantX::create(&mut b, &VariantXArgs { value: 99.0 });
    let variant_fill = VariantFill::create(
        &mut b,
        &VariantFillArgs {
            color: Some(&Color::new(0.2, 0.4, 0.6, 1.0)),
        },
    );
    let override_x = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 1,
            value_type: VariantPropValue::VariantX,
            value: Some(variant_x.as_union_value()),
        },
    );
    let override_fill = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 0,
            value_type: VariantPropValue::VariantFill,
            value: Some(variant_fill.as_union_value()),
        },
    );
    let overrides = b.create_vector(&[override_x, override_fill]);
    let hover_name = b.create_string("Hover");
    let hover_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            name: Some(hover_name),
            overrides: Some(overrides),
        },
    );
    let default_name = b.create_string("Default");
    let default_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            name: Some(default_name),
            overrides: None,
        },
    );
    let members = b.create_vector(&[default_member, hover_member]);
    let variant_set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            // Written non-zero against the schema default of 0: a
            // shifted id reads the default and this is what notices.
            active_member: 1,
        },
    );
    let variant_sets = b.create_vector(&[variant_set]);

    // v0.7 binding tables (story #167): two declarations, two rows — one
    // identity (union NONE), one carrying a Scale transform.
    let gap_name = b.create_string("size/gap");
    let gap_signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: Some(gap_name),
            initial: 16.0,
        },
    );
    let anon_signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 0.25,
        },
    );
    let signals = b.create_vector(&[gap_signal, anon_signal]);

    let gap_binding = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 0,
            node: 0,
            channel: BindingChannel::Gap,
            transform_type: BindingTransform::NONE,
            transform: None,
        },
    );
    let scale = TransformScale::create(&mut b, &TransformScaleArgs { factor: 2.0 });
    let scaled_binding = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 1,
            node: 1,
            channel: BindingChannel::FillR,
            transform_type: BindingTransform::TransformScale,
            transform: Some(scale.as_union_value()),
        },
    );
    // v0.8 (story #44): the appended Opacity channel (9), so a renumbered
    // BindingChannel enum reads back a different channel and the guard
    // notices.
    let opacity_binding = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 1,
            node: 2,
            channel: BindingChannel::Opacity,
            transform_type: BindingTransform::NONE,
            transform: None,
        },
    );
    let bindings = b.create_vector(&[gap_binding, scaled_binding, opacity_binding]);

    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            images: Some(images),
            paints: Some(paints),
            strings: Some(strings),
            text_styles: Some(text_styles),
            variant_sets: Some(variant_sets),
            signals: Some(signals),
            bindings: Some(bindings),
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}
