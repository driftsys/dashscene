//! Shared helpers for the round-trip test suites (`tests/roundtrip.rs`,
//! `tests/paint_roundtrip.rs`) — the build/decode helpers and color
//! fixtures both files need, so a document-level schema change is a
//! mechanical edit in one place rather than two (issue #65). Each test
//! binary compiles its own copy of this module, so a helper unused by one
//! binary is still used by the other — hence the `dead_code` allowance
//! (the same pattern as `dashc`'s and `dashscene-typeset`'s `tests/common`).

#![allow(dead_code)]

use dashbuf::{AssetEntry, Color, Document, DocumentArgs, Node, Paint, root_as_document};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

/// The canonical opaque red every round-trip test that needs *a* color,
/// rather than a specific one, reaches for.
pub fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

/// A second, distinct fixture color — half-transparent blue — for the
/// tests that need two colors to tell apart (a gradient's two stops, a
/// legacy paint beside a pooled one).
pub fn half_blue() -> Color {
    Color::new(0.0, 0.0, 1.0, 0.5)
}

/// Finishes a document holding the given node, paint pool, and asset-table
/// entries, and returns the serialized buffer bytes.
pub fn finish_document(
    mut builder: FlatBufferBuilder<'static>,
    node: WIPOffset<Node<'static>>,
    paints: &[WIPOffset<Paint<'static>>],
    assets: &[WIPOffset<AssetEntry<'static>>],
) -> Vec<u8> {
    let nodes = builder.create_vector(&[node]);
    let assets = (!assets.is_empty()).then(|| builder.create_vector(assets));
    let paints = (!paints.is_empty()).then(|| builder.create_vector(paints));
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            assets,
            paints,
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

/// Decodes the buffer and resolves the single node's paint-pool entry.
pub fn single_node_paint(bytes: &[u8]) -> Paint<'_> {
    let document = root_as_document(bytes).expect("valid dashbuf document");
    let node = document.nodes().expect("nodes present").get(0);
    document
        .paints()
        .expect("paint pool present")
        .get(node.paint_entry() as usize)
}
