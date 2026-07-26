//! Story B1 round trips: the baked-vector tables and the `Paint` shape
//! channel survive a build → finish → decode cycle. `VectorAtlas` /
//! `VectorShape` sit in document-level pools (`Document.vector_atlases` /
//! `Document.vector_shapes`), referenced by index the same way the paint pool
//! and image assets are.

use dashbuf::{
    AssetEntry, AssetEntryArgs, AtlasRect, Color, Document, DocumentArgs, Fill, ImageFormat,
    NO_FIELD, Node, NodeArgs, Paint, PaintArgs, PlaneBounds, SolidFill, SolidFillArgs, VectorAtlas,
    VectorAtlasArgs, VectorShape, VectorShapeArgs, root_as_document,
};
use flatbuffers::FlatBufferBuilder;

#[test]
fn vector_atlas_shape_and_shape_field_round_trip() {
    let mut builder = FlatBufferBuilder::new();

    // A minimal asset-table entry standing in for the packed atlas PNG.
    let hash = builder.create_vector(&[7u8; 32]);
    let image = AssetEntry::create(
        &mut builder,
        &AssetEntryArgs {
            hash: Some(hash),
            format: ImageFormat::Png,
            width: 4,
            height: 4,
        },
    );

    let atlas = VectorAtlas::create(
        &mut builder,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: 48.0,
            distance_range: 4.0,
        },
    );

    // Struct fields are set on the args, not the builder.
    let shape = VectorShape::create(
        &mut builder,
        &VectorShapeArgs {
            atlas: 0,
            atlas_rect: Some(&AtlasRect::new(1, 2, 56, 54)),
            plane_bounds: Some(&PlaneBounds::new(-6.5, -6.5, 46.5, 46.5)),
        },
    );

    // A paint entry that carries a solid fill masked by vector shape 0.
    let solid = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.5, 0.25, 1.0)),
        },
    );
    let field_paint = Paint::create(
        &mut builder,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            shape_field: 0,
            ..Default::default()
        },
    );
    // A second paint entry with no shape channel — its shape_field must read
    // back as the NO_FIELD sentinel (the parametric-shape default).
    let plain_paint = Paint::create(&mut builder, &PaintArgs::default());

    let node = Node::create(
        &mut builder,
        &NodeArgs {
            paint_entry: 0,
            ..Default::default()
        },
    );

    let nodes = builder.create_vector(&[node]);
    let assets = builder.create_vector(&[image]);
    let paints = builder.create_vector(&[field_paint, plain_paint]);
    let vector_atlases = builder.create_vector(&[atlas]);
    let vector_shapes = builder.create_vector(&[shape]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            assets: Some(assets),
            paints: Some(paints),
            vector_atlases: Some(vector_atlases),
            vector_shapes: Some(vector_shapes),
            ..Default::default()
        },
    );
    builder.finish(document, None);
    let bytes = builder.finished_data().to_vec();

    let doc = root_as_document(&bytes).expect("valid dashbuf document");

    // Paint shape channel.
    let paints = doc.paints().expect("paints");
    assert_eq!(
        paints.get(0).shape_field(),
        0,
        "field paint keeps its index"
    );
    assert_eq!(
        paints.get(1).shape_field(),
        NO_FIELD,
        "a paint with no shape channel reads back the NO_FIELD sentinel"
    );

    // Vector atlas pool.
    let atlas = doc.vector_atlases().expect("vector atlases").get(0);
    assert_eq!(atlas.image(), 0);
    assert_eq!(atlas.px_per_em(), 48.0);
    assert_eq!(atlas.distance_range(), 4.0);

    // Vector shape pool.
    let shape = doc.vector_shapes().expect("vector shapes").get(0);
    assert_eq!(shape.atlas(), 0);
    let rect = shape.atlas_rect().expect("atlas rect present");
    assert_eq!(
        (rect.x(), rect.y(), rect.width(), rect.height()),
        (1, 2, 56, 54)
    );
    let plane = shape.plane_bounds().expect("plane bounds present");
    assert_eq!(
        (plane.left(), plane.top(), plane.right(), plane.bottom()),
        (-6.5, -6.5, 46.5, 46.5)
    );
}
