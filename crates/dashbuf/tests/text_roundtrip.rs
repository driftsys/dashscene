//! Round-trips for the v0.5 text vocabulary (#26): strings pool, text
//! style pool, sentinel-indexed node fields. One focused test per
//! construct, matching paint_roundtrip.rs's style.

use dashbuf::{
    Color, Document, DocumentArgs, Node, NodeArgs, TextStyle, TextStyleArgs, root_as_document,
};
use flatbuffers::FlatBufferBuilder;

/// `Node.text` / `Node.text_style`'s "absent" sentinel — same
/// convention as `Node.parent`'s NO_PARENT and `paint_entry`'s
/// NO_PAINT.
const NO_TEXT: u32 = u32::MAX;

/// Builds a document with `strings`, `text_styles`, and one node per
/// `(text, text_style)` index pair.
fn build_doc(strings: &[&str], styles: &[(&str, f32, u16)], nodes: &[(u32, u32)]) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let strings: Vec<_> = strings.iter().map(|s| b.create_string(s)).collect();
    let strings = b.create_vector(&strings);
    let styles: Vec<_> = styles
        .iter()
        .map(|(family, size, weight)| {
            let family = b.create_string(family);
            TextStyle::create(
                &mut b,
                &TextStyleArgs {
                    family: Some(family),
                    size_px: *size,
                    weight: *weight,
                    color: Some(&Color::new(0.1, 0.2, 0.3, 1.0)),
                },
            )
        })
        .collect();
    let styles = b.create_vector(&styles);
    let nodes: Vec<_> = nodes
        .iter()
        .map(|(text, style)| {
            Node::create(
                &mut b,
                &NodeArgs {
                    text: *text,
                    text_style: *style,
                    ..Default::default()
                },
            )
        })
        .collect();
    let nodes = b.create_vector(&nodes);
    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            strings: Some(strings),
            text_styles: Some(styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    b.finished_data().to_vec()
}

#[test]
fn text_node_reads_back_through_the_pools() {
    let bytes = build_doc(
        &["Speed", "km/h"],
        &[("Noto Sans", 16.0, 400)],
        &[(0, 0), (1, 0)],
    );
    let doc = root_as_document(&bytes).expect("verifies");
    let nodes = doc.nodes().unwrap();
    let strings = doc.strings().unwrap();
    let styles = doc.text_styles().unwrap();
    let n0 = nodes.get(0);
    assert_eq!(strings.get(n0.text() as usize), "Speed");
    let style = styles.get(n0.text_style() as usize);
    assert_eq!(style.family(), "Noto Sans");
    assert_eq!(style.size_px(), 16.0);
    assert_eq!(style.weight(), 400);
    assert_eq!(style.color().unwrap().r(), 0.1);
    assert_eq!(strings.get(nodes.get(1).text() as usize), "km/h");
}

#[test]
fn two_nodes_can_share_one_interned_string() {
    let bytes = build_doc(&["OK"], &[("Noto Sans", 12.0, 700)], &[(0, 0), (0, 0)]);
    let doc = root_as_document(&bytes).expect("verifies");
    let nodes = doc.nodes().unwrap();
    assert_eq!(nodes.get(0).text(), nodes.get(1).text());
}

#[test]
fn a_node_without_text_reads_the_sentinels_by_default() {
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);
    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();
    let doc = root_as_document(&bytes).expect("verifies");
    let n = doc.nodes().unwrap().get(0);
    assert_eq!(n.text(), NO_TEXT);
    assert_eq!(n.text_style(), NO_TEXT);
}

#[test]
fn weight_defaults_to_regular() {
    let mut b = FlatBufferBuilder::new();
    let family = b.create_string("Noto Sans");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            ..Default::default()
        },
    );
    let styles = b.create_vector(&[style]);
    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            text_styles: Some(styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();
    let doc = root_as_document(&bytes).expect("verifies");
    assert_eq!(doc.text_styles().unwrap().get(0).weight(), 400);
}
