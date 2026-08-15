//! The load gate for the story B1 baked-vector index chain: a paint entry's
//! `shape_field` into the vector-shape pool, a shape's `atlas` into the atlas
//! pool, and an atlas's `image` into the asset table. The flatbuffer verifier
//! accepts each dangling index — it checks buffer structure, not referential
//! integrity — so an out-of-range one would otherwise fail far from the
//! producing bug, at paint time.

use dashbuf::{
    AssetEntry, AssetEntryArgs, AtlasRect, Color, Document, DocumentArgs, Fill, ImageFormat, Node,
    NodeArgs, Paint, PaintArgs, PlaneBounds, SolidFill, SolidFillArgs, VectorAtlas,
    VectorAtlasArgs, VectorShape, VectorShapeArgs, root_as_document,
};
use dashscene_validator::{Location, rule, validate_document};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

/// Builds a one-node document with the given paint pool, asset-table
/// entries, and vector pools, and returns the serialized bytes.
#[allow(clippy::too_many_arguments)]
fn finish(
    mut b: FlatBufferBuilder<'static>,
    paints: &[WIPOffset<Paint<'static>>],
    assets: &[WIPOffset<AssetEntry<'static>>],
    vector_atlases: &[WIPOffset<VectorAtlas<'static>>],
    vector_shapes: &[WIPOffset<VectorShape<'static>>],
) -> Vec<u8> {
    let node = Node::create(
        &mut b,
        &NodeArgs {
            paint_entry: if paints.is_empty() { u32::MAX } else { 0 },
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let paints = (!paints.is_empty()).then(|| b.create_vector(paints));
    let assets = (!assets.is_empty()).then(|| b.create_vector(assets));
    let vector_atlases = (!vector_atlases.is_empty()).then(|| b.create_vector(vector_atlases));
    let vector_shapes = (!vector_shapes.is_empty()).then(|| b.create_vector(vector_shapes));
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            paints,
            assets,
            vector_atlases,
            vector_shapes,
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

fn solid_paint(b: &mut FlatBufferBuilder<'static>, shape_field: u32) -> WIPOffset<Paint<'static>> {
    let solid = SolidFill::create(
        b,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
        },
    );
    Paint::create(
        b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            shape_field,
            ..Default::default()
        },
    )
}

/// A clean asset-table entry: a 32-byte filler hash and a non-zero extent,
/// so it trips neither `asset.hash-wrong-length` nor `asset.zero-extent`.
fn png_asset(b: &mut FlatBufferBuilder<'static>) -> WIPOffset<AssetEntry<'static>> {
    let hash = b.create_vector(&[7u8; 32]);
    AssetEntry::create(
        b,
        &AssetEntryArgs {
            hash: Some(hash),
            format: ImageFormat::Png,
            width: 4,
            height: 4,
            kind: dashbuf::AssetKind::Image,
        },
    )
}

#[test]
fn shape_field_out_of_range_is_named() {
    let mut b = FlatBufferBuilder::new();
    // shape_field 5, but the document carries no vector shapes.
    let paint = solid_paint(&mut b, 5);
    let bytes = finish(b, &[paint], &[], &[], &[]);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    let report = validate_document(&doc);
    assert!(report.has(rule::SHAPE_FIELD_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
    assert_eq!(
        report.find(rule::SHAPE_FIELD_OUT_OF_RANGE).unwrap().at,
        Location::PaintEntry(0)
    );
}

#[test]
fn vector_shape_atlas_out_of_range_is_named() {
    let mut b = FlatBufferBuilder::new();
    // A shape naming atlas 3, but no atlases exist.
    let shape = VectorShape::create(
        &mut b,
        &VectorShapeArgs {
            atlas: 3,
            atlas_rect: Some(&AtlasRect::new(0, 0, 8, 8)),
            plane_bounds: Some(&PlaneBounds::new(0.0, 0.0, 8.0, 8.0)),
        },
    );
    let bytes = finish(b, &[], &[], &[], &[shape]);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    let report = validate_document(&doc);
    assert!(
        report.has(rule::VECTOR_SHAPE_ATLAS_OUT_OF_RANGE),
        "{report}"
    );
    assert_eq!(
        report
            .find(rule::VECTOR_SHAPE_ATLAS_OUT_OF_RANGE)
            .unwrap()
            .at,
        Location::VectorShape(0)
    );
}

#[test]
fn vector_atlas_image_out_of_range_is_named() {
    let mut b = FlatBufferBuilder::new();
    // An atlas naming image 2, but no images exist.
    let atlas = VectorAtlas::create(
        &mut b,
        &VectorAtlasArgs {
            image: 2,
            px_per_em: 48.0,
            distance_range: 4.0,
        },
    );
    let bytes = finish(b, &[], &[], &[atlas], &[]);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    let report = validate_document(&doc);
    assert!(
        report.has(rule::VECTOR_ATLAS_IMAGE_OUT_OF_RANGE),
        "{report}"
    );
    assert_eq!(
        report
            .find(rule::VECTOR_ATLAS_IMAGE_OUT_OF_RANGE)
            .unwrap()
            .at,
        Location::VectorAtlas(0)
    );
}

/// An atlas builder that varies only `distance_range`, so the three tests
/// below differ in the value under test and nothing else.
fn atlas_with_distance_range(
    b: &mut FlatBufferBuilder<'static>,
    distance_range: f32,
) -> WIPOffset<VectorAtlas<'static>> {
    VectorAtlas::create(
        b,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: 48.0,
            distance_range,
        },
    )
}

/// The atlas scalar the loader folds into every `VectorField` the atlas covers
/// is named when it is out of the painters' domain (issue #1002).
///
/// Not an index like the two rules above it, and the reason it belongs beside
/// them is where it is read: `dashscene-core`'s loader takes a shape's
/// `distance_range` from the atlas the shape names, so a document with one bad
/// atlas takes every shape over it out of domain. `dashpaint`'s
/// `PaintTable::push_with` refuses the same predicate as a panic (issue #986),
/// which is the second line of defence and not the first — without this rule a
/// `.dsb` validates clean, loads, and panics at the seam.
///
/// One test over the array rather than one per value, and it asserts the
/// message as well as the rule name: `-0.0` is in the list because the
/// predicate is spelled `> 0.0` rather than against an explicit zero, and a
/// NaN because `!(NaN > 0.0)` is what makes a comparison-based guard refuse it
/// at all.
#[test]
fn vector_atlas_distance_range_out_of_domain_is_named() {
    for range in [0.0, -0.0, -2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut b = FlatBufferBuilder::new();
        let image = png_asset(&mut b);
        let atlas = atlas_with_distance_range(&mut b, range);
        let bytes = finish(b, &[], &[image], &[atlas], &[]);
        let doc = root_as_document(&bytes).expect("valid dashbuf document");

        let report = validate_document(&doc);
        let found = report
            .find(rule::VECTOR_ATLAS_DISTANCE_RANGE_OUT_OF_DOMAIN)
            .unwrap_or_else(|| panic!("a distance range of {range} must be named:\n{report}"));
        assert!(report.has_errors());
        assert_eq!(found.at, Location::VectorAtlas(0));
        assert!(
            found.message.contains("not finite and greater than zero"),
            "the diagnostic must say which domain was left; got: {}",
            found.message
        );
    }
}

/// An atlas written by a producer that does not know the field is named too.
///
/// A flatbuffers table scalar with no `(required)` decodes to its default, so
/// an omitted `distance_range` reads back `0.0` and is byte-identical on the
/// wire to one that wrote `0.0` deliberately. The omitted case is the one a
/// producer reaches by accident rather than by authoring a bad number, which is
/// why it is pinned separately from the value sweep above (issue #1002).
#[test]
fn a_vector_atlas_that_omits_its_distance_range_is_named() {
    let mut b = FlatBufferBuilder::new();
    let image = png_asset(&mut b);
    let atlas = VectorAtlas::create(
        &mut b,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: 48.0,
            ..Default::default()
        },
    );
    let bytes = finish(b, &[], &[image], &[atlas], &[]);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    let report = validate_document(&doc);
    assert!(
        report.has(rule::VECTOR_ATLAS_DISTANCE_RANGE_OUT_OF_DOMAIN),
        "{report}"
    );
}

/// A distance range at the bottom of the accepted domain validates clean, so
/// the rule refuses what is out of the domain rather than what is merely small.
///
/// The twin of `push_with_accepts_a_narrow_distance_range` in
/// `dashpaint`'s boundary-B suite, and here for the same reason: without it the
/// predicate could be tightened to `>= f32::MIN_POSITIVE` and the whole rest of
/// this file would stay green. Neither value is a *useful* range — both drive
/// the painters' `px_range` to zero — but the seam this rule stands in front of
/// accepts them, and a validator that refused what the loader accepts would
/// name a document the runtime would have drawn.
#[test]
fn a_narrow_vector_atlas_distance_range_is_clean() {
    for range in [f32::MIN_POSITIVE, f32::from_bits(1)] {
        let mut b = FlatBufferBuilder::new();
        let image = png_asset(&mut b);
        let atlas = atlas_with_distance_range(&mut b, range);
        let bytes = finish(b, &[], &[image], &[atlas], &[]);
        let doc = root_as_document(&bytes).expect("valid dashbuf document");

        let report = validate_document(&doc);
        assert!(
            !report.has(rule::VECTOR_ATLAS_DISTANCE_RANGE_OUT_OF_DOMAIN),
            "{range} is finite and greater than zero, so it is out of no domain this rule \
             states:\n{report}"
        );
    }
}

#[test]
fn a_consistent_vector_chain_is_clean() {
    let mut b = FlatBufferBuilder::new();
    let image = png_asset(&mut b);
    let atlas = VectorAtlas::create(
        &mut b,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: 48.0,
            distance_range: 4.0,
        },
    );
    let shape = VectorShape::create(
        &mut b,
        &VectorShapeArgs {
            atlas: 0,
            atlas_rect: Some(&AtlasRect::new(1, 1, 8, 8)),
            plane_bounds: Some(&PlaneBounds::new(-1.0, -1.0, 9.0, 9.0)),
        },
    );
    let paint = solid_paint(&mut b, 0);
    let bytes = finish(b, &[paint], &[image], &[atlas], &[shape]);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    let report = validate_document(&doc);
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}
