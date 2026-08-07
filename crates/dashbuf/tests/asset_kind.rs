//! `AssetEntry.kind` — the append that moved no bytes.
//!
//! The v0.12 slice may regenerate a golden in exactly one story (#434), and
//! this field arrived in a different one (#432). The mechanism that makes that
//! safe is flatbuffers' default omission: `flatc` writes no vtable slot for a
//! scalar equal to its declared default, so an entry whose kind is `Image` — the
//! default, and what every entry written before this field existed is —
//! occupies exactly the bytes it did before.
//!
//! `dashc`'s `the_fixture_emits_the_golden_dsb` already proves the *outcome*:
//! it recompiles `goldens/dsb/v03-paint.dsb` through the current emitter and
//! asserts byte equality with the committed file. This file proves the
//! *mechanism*, so a future change that starts writing the slot unconditionally
//! fails here with a message that says why, rather than in a golden diff that
//! only says the bytes moved.

use dashbuf::{
    AssetEntry, AssetEntryArgs, AssetKind, Document, DocumentArgs, ImageFormat, Node, NodeArgs,
    root_as_document,
};
use flatbuffers::FlatBufferBuilder;

/// A one-entry document whose only asset has `kind`.
fn document_with(kind: AssetKind) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let hash = builder.create_vector(&[3u8; 32]);
    let entry = AssetEntry::create(
        &mut builder,
        &AssetEntryArgs {
            hash: Some(hash),
            format: ImageFormat::Png,
            width: 8,
            height: 8,
            kind,
        },
    );
    let assets = builder.create_vector(&[entry]);
    let name = builder.create_string("root");
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: Some(name),
            parent: dashbuf::NO_PARENT,
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            assets: Some(assets),
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

/// The default is omitted and a non-default is written, which is the whole
/// reason appending this field cost no committed byte.
#[test]
fn the_default_kind_writes_no_slot_and_a_non_default_kind_does() {
    let as_image = document_with(AssetKind::Image);
    let as_field = document_with(AssetKind::DistanceField);

    assert!(
        as_image.len() < as_field.len(),
        "an entry at the default kind must be smaller than one that is not: {} vs {} bytes. If \
         these are equal, `flatc` is writing the slot unconditionally and every committed `.dsb` \
         has moved.",
        as_image.len(),
        as_field.len()
    );
}

/// Both values survive the round trip, so the omission is an encoding detail
/// and not a lost field.
#[test]
fn both_kinds_read_back_as_written() {
    for kind in [AssetKind::Image, AssetKind::DistanceField] {
        let bytes = document_with(kind);
        let document = root_as_document(&bytes).expect("the document is valid");
        let entry = document.assets().expect("the document has assets").get(0);
        assert_eq!(entry.kind(), kind);
    }
}

/// An entry written before the field existed reads back as `Image`.
///
/// `goldens/dsb/v03-paint.dsb` is committed and frozen, and its one asset entry
/// predates this field entirely. That the default reading is the *correct*
/// reading for it is what made the append safe, rather than merely cheap.
#[test]
fn a_pre_existing_committed_entry_reads_as_an_image() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the committed golden is readable");
    let (document, payloads) = dashbuf::open_verified(&file).expect("the committed golden opens");

    let entries = document.assets().expect("v03-paint carries one asset");
    assert_eq!(entries.len(), 1);
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        entries.get(0).kind(),
        AssetKind::Image,
        "an entry written before `kind` existed must read as an image, because that is what it is"
    );
}
