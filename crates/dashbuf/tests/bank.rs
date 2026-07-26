//! Cold-bank assembly (story #433): what `dashbuf::bank::assemble` puts where,
//! and the one invariant a single assembly could not test.
//!
//! # The invariant, and why it needed a second assembly
//!
//! `docs/decisions/asset-model-content-addressed-blobs.md` records that hot
//! sections are byte-identical across assemblies of one document, because an
//! `AssetEntry` names a content hash and never a section index. v0.11 shipped
//! one assembly — RAW — so it could only record that as intent. Two assemblies
//! of one document are what turns it into a measurement, and
//! `ColdBank::derived` is what makes a second one constructible.
//!
//! # What the second assembly here is, and is not
//!
//! The derived payloads below are **stand-ins**, not packer output: arbitrary
//! bytes bound to the document's canonical hashes. That is deliberate and it is
//! enough for this property, which is about where assembly puts bytes and not
//! about what a packer produces. It is *not* a loadable derived bank — resolving
//! a canonical hash to a payload that is not its own preimage needs the
//! derivation manifest, which is story #434's. A file assembled from the bank
//! below is well-formed and its cold payloads cannot yet be resolved by
//! `dashbuf::open`, which is the read-side half #434 completes.
//!
//! # Three assets, not one
//!
//! Every document in the corpus has at most one asset (`goldens/dsb/README.md`:
//! `v03-paint.dsb` is the only fixture with an image), and a one-asset document
//! cannot fail an ordering, resolution, or wrong-index bug — every index in it
//! is 0. The documents here are hand-built with three assets of three different
//! sizes, one of them over the large-blob page-alignment threshold, so those
//! bugs have somewhere to show.

use dashbuf::bank::{AssembleError, ColdBank, assemble};
use dashbuf::container::{
    Container, FLAVOR_ASSET, FLAVOR_UI, HASH_LEN, HEADER_SIZE, LARGE_BLOB_THRESHOLD,
    SECTION_STRIDE, SectionKind,
};
use dashbuf::{
    AssetEntry, AssetEntryArgs, AssetKind, Document, DocumentArgs, ImageFormat, Node, NodeArgs,
    root_as_document,
};
use flatbuffers::FlatBufferBuilder;

/// Three payloads with three distinct lengths, one of them large enough to be
/// page-aligned on its own. Byte values differ per payload so a swapped pair is
/// visible rather than silently equal.
fn canonical_payloads() -> Vec<Vec<u8>> {
    vec![
        vec![0xA1; 93],
        vec![0xB2; LARGE_BLOB_THRESHOLD + 7],
        vec![0xC3; 512],
    ]
}

/// A ui-document flatbuffer naming `payloads` as its assets, in order.
///
/// One node, so the document is a document; the extents are arbitrary, because
/// nothing in assembly reads them.
fn ui_section(payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();

    let entries: Vec<_> = payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let hash = builder.create_vector(blake3::hash(payload).as_bytes());
            AssetEntry::create(
                &mut builder,
                &AssetEntryArgs {
                    hash: Some(hash),
                    format: ImageFormat::Png,
                    kind: AssetKind::Image,
                    width: 16 + index as u32,
                    height: 32 + index as u32,
                },
            )
        })
        .collect();
    let assets = builder.create_vector(&entries);

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

/// The canonical hash of every asset entry in `ui`, in entry order.
fn canonical_hashes(ui: &[u8]) -> Vec<[u8; HASH_LEN]> {
    root_as_document(ui)
        .expect("a valid document")
        .assets()
        .expect("assets present")
        .iter()
        .map(|entry| {
            entry
                .hash()
                .bytes()
                .try_into()
                .expect("a 32-byte content hash")
        })
        .collect()
}

// ---------------------------------------------------------------------------
// RAW is the null binding
// ---------------------------------------------------------------------------

#[test]
fn a_raw_bank_binds_each_payload_to_its_own_hash() {
    // The identity map, stated as a test rather than only as a doc comment:
    // this is the whole difference between RAW and every other profile, and it
    // is why a RAW assembly may not move a byte.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    let file = assemble(&ui, &bank).expect("the document and its RAW bank assemble");

    let container = Container::parse(&file).expect("the assembled file parses");
    for (index, payload) in payloads.iter().enumerate() {
        // The section's own recorded content hash — computed by the writer from
        // the bytes — equals the canonical hash the document names.
        let entry = container.section(1 + index);
        assert_eq!(
            entry.hash,
            canonical_hashes(&ui)[index],
            "blob {index}'s content hash is the canonical hash the entry names",
        );
        assert_eq!(
            container.blob_by_hash(&entry.hash).expect("resolves"),
            payload.as_slice(),
            "blob {index} resolves to the canonical payload itself",
        );
    }
}

#[test]
fn a_raw_assembly_round_trips_through_open() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    let file = assemble(&ui, &bank).expect("assembles");

    let (document, resident) = dashbuf::open(&file).expect("the assembled file opens");
    assert_eq!(document.assets().expect("assets").len(), payloads.len());
    let expected: Vec<&[u8]> = payloads.iter().map(Vec::as_slice).collect();
    assert_eq!(resident, expected, "payloads come back in entry order");
}

#[test]
fn assembly_is_deterministic() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    assert_eq!(
        assemble(&ui, &bank).expect("assembles"),
        assemble(&ui, &bank).expect("assembles"),
        "R7: the same document and bank assemble to the same bytes",
    );
}

// ---------------------------------------------------------------------------
// Layout: hot at the head, cold page-aligned at the tail
// ---------------------------------------------------------------------------

#[test]
fn the_ui_section_is_first_and_every_blob_follows_it() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    let file = assemble(&ui, &bank).expect("assembles");
    let container = Container::parse(&file).expect("parses");

    assert_eq!(container.len(), 1 + payloads.len());
    assert_eq!(container.section(0).kind, SectionKind::Structured as u16);
    assert_eq!(container.section(0).flavor, FLAVOR_UI);
    assert_eq!(container.section_bytes(0), ui, "the ui section is verbatim");
    for index in 1..container.len() {
        assert_eq!(container.section(index).kind, SectionKind::Blob as u16);
        assert_eq!(container.section(index).flavor, FLAVOR_ASSET);
    }
}

#[test]
fn blob_order_is_entry_order_not_bank_order() {
    // The bank is handed its bindings in reverse, so a positional zip against
    // the bank would place the payloads backwards. Assembly resolves by hash,
    // so it does not. With one asset this test would pass no matter which of
    // the two assembly did, which is why the document has three.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let hashes = canonical_hashes(&ui);

    let mut reversed: Vec<([u8; HASH_LEN], &[u8])> = hashes
        .iter()
        .copied()
        .zip(payloads.iter().map(Vec::as_slice))
        .collect();
    reversed.reverse();

    let file = assemble(&ui, &ColdBank::derived(reversed)).expect("assembles");
    let container = Container::parse(&file).expect("parses");
    for (index, payload) in payloads.iter().enumerate() {
        assert_eq!(
            container.section_bytes(1 + index),
            payload.as_slice(),
            "blob {index} is the payload entry {index} names",
        );
    }
}

#[test]
fn the_first_blob_starts_on_a_page_and_the_large_blob_gets_its_own() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    let file = assemble(&ui, &bank).expect("assembles");
    let container = Container::parse(&file).expect("parses");

    let first_cold = container.section(1).offset;
    assert_eq!(
        first_cold % 4096,
        0,
        "the hot/cold boundary is page-aligned: that is what lets a load gate \
         verify the hot region without faulting a cold page",
    );
    assert!(
        container.section(0).offset + container.section(0).length <= first_cold,
        "every hot byte precedes every cold byte",
    );
    // Payload 1 is over the threshold, so it is page-aligned on its own; payload
    // 2 is under it and packs densely on the 64-byte quantum.
    assert!(payloads[1].len() >= LARGE_BLOB_THRESHOLD);
    assert!(payloads[2].len() < LARGE_BLOB_THRESHOLD);
    assert_eq!(
        container.section(2).offset % 4096,
        0,
        "a blob at or over the threshold gets a page of its own, so it can be \
         prefetched and evicted with one madvise range",
    );
    // Both halves are needed. "Packs on the 64-byte quantum" is satisfied by a
    // page-aligned offset too, so on its own it cannot tell dense packing from a
    // page per blob — lowering LARGE_BLOB_THRESHOLD until this payload counted
    // as large would leave it green. The second assertion is the one that fails.
    assert_eq!(container.section(3).offset % 64, 0);
    assert_ne!(
        container.section(3).offset % 4096,
        0,
        "a blob under the threshold packs densely rather than claiming a page: \
         verification and readiness are per blob, so a shared page is free prefetch",
    );
}

#[test]
fn a_document_with_no_assets_assembles_to_one_section() {
    // Six of the seven committed goldens are this shape, and the container
    // decision requires that they pay nothing for a boundary they do not have.
    let ui = ui_section(&[]);
    let file = assemble(&ui, &ColdBank::raw(Vec::<&[u8]>::new())).expect("assembles");
    let container = Container::parse(&file).expect("parses");

    assert_eq!(container.len(), 1);
    assert_eq!(
        file.len(),
        HEADER_SIZE + SECTION_STRIDE + ui.len(),
        "no blob means no boundary and no page of padding for one",
    );
}

// ---------------------------------------------------------------------------
// The invariant: hot sections do not vary with the bank
// ---------------------------------------------------------------------------

/// A stand-in derived bank over `ui`'s canonical hashes: same hashes, different
/// resident bytes, different lengths.
fn stand_in_derived_bank<'a>(ui: &[u8], derived: &'a [Vec<u8>]) -> ColdBank<'a> {
    ColdBank::derived(
        canonical_hashes(ui)
            .into_iter()
            .zip(derived.iter().map(Vec::as_slice)),
    )
}

#[test]
fn hot_sections_are_byte_identical_across_two_assemblies_of_one_document() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    // Different bytes, different lengths, and a different large/small split:
    // payload 1 is over the page threshold canonically and under it here, so
    // the two assemblies do not even agree on which blobs are page-aligned.
    let derived = vec![vec![0x11; 40], vec![0x22; 4096], vec![0x33; 3]];

    let raw = assemble(&ui, &ColdBank::raw(payloads.iter().map(Vec::as_slice)))
        .expect("the RAW bank assembles");
    let hifi = assemble(&ui, &stand_in_derived_bank(&ui, &derived))
        .expect("the stand-in derived bank assembles");

    assert_ne!(raw, hifi, "the two files differ, or this proves nothing");

    let (a, b) = (
        Container::parse(&raw).expect("parses"),
        Container::parse(&hifi).expect("parses"),
    );

    // The asset count fixes the section count, so the table is the same length
    // and the ui section does not even move.
    assert_eq!(a.len(), b.len());
    assert_eq!(a.section(0).offset, b.section(0).offset);
    assert_eq!(a.section(0).length, b.section(0).length);
    assert_eq!(a.section(0).hash, b.section(0).hash);
    assert_eq!(
        a.ui_document().expect("a ui section"),
        b.ui_document().expect("a ui section"),
        "the hot section is byte-identical across the two assemblies",
    );
}

#[test]
fn every_difference_between_two_assemblies_is_envelope_or_cold_bytes() {
    // The invariant in its strongest form: not "the hot section survived", but
    // "nothing outside the envelope and the cold payloads changed at all".
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let derived = vec![vec![0x11; 40], vec![0x22; 4096], vec![0x33; 3]];

    let raw = assemble(&ui, &ColdBank::raw(payloads.iter().map(Vec::as_slice)))
        .expect("the RAW bank assembles");
    let hifi = assemble(&ui, &stand_in_derived_bank(&ui, &derived))
        .expect("the stand-in derived bank assembles");
    let container = Container::parse(&raw).expect("parses");

    // The envelope: the header and the whole section table. The header's root
    // hash covers the table, and the table records where the cold bytes are, so
    // both are expected to differ.
    let envelope_end = HEADER_SIZE + container.len() * SECTION_STRIDE;
    // The cold region: from the first blob's offset to the end of the longer
    // file. Nothing hot lives past it — the hot region is a contiguous prefix.
    let first_cold = container.section(1).offset as usize;

    let common = raw.len().min(hifi.len());
    for at in 0..common {
        if raw[at] == hifi[at] {
            continue;
        }
        assert!(
            at < envelope_end || at >= first_cold,
            "byte {at} differs between the two assemblies but lies in neither the \
             envelope (0..{envelope_end}) nor the cold region ({first_cold}..)",
        );
    }
    // The loop above only compares the bytes both files have. That is enough
    // only because the shorter file already reaches into the cold region: every
    // offset past it is therefore a cold offset, so a length difference is a
    // cold-region difference and needs no separate check.
    assert!(
        common >= first_cold,
        "the shorter file ({common} bytes) stops before the cold region starts \
         ({first_cold}), so the comparison above skipped hot bytes",
    );
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn a_bank_missing_a_payload_is_refused_by_index() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    // The middle payload is left out, so the failure names entry 1 rather than
    // whichever entry happens to be checked first.
    let bank = ColdBank::raw([payloads[0].as_slice(), payloads[2].as_slice()]);
    assert_eq!(
        assemble(&ui, &bank),
        Err(AssembleError::Unbound { index: 1 }),
        "assembling anyway would write a file whose asset table points at a \
         payload the file does not carry",
    );
}

#[test]
fn a_bank_with_payloads_no_entry_names_is_refused() {
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads[..2]);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    assert_eq!(
        assemble(&ui, &bank),
        Err(AssembleError::UnusedPayloads { count: 1 }),
        "an unreachable cold payload is a silent size regression, so it is refused",
    );
}

#[test]
fn a_bank_holding_one_payload_twice_is_not_mistaken_for_an_unused_one() {
    // A regression guard, for a defect this branch introduced and fixed before
    // merging. `position_of` returns the first binding for a hash, so counting
    // unused bindings by the positions resolution *picked* reported the second
    // of two identical bindings as unnamed, and refused the document.
    //
    // That mattered because `dashc::package` turns an `AssembleError` into a
    // panic through an `expect`. `Document::push_asset` deduplicates by content
    // hash so `dashc` does not build such a bank today, but `Document.assets` is
    // a public `Vec`, and the writer this replaced accepted the same input
    // silently. A named condition must not become a crash (P4).
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);

    // The same payload bound twice, under the hash it is the preimage of.
    let mut bindings: Vec<([u8; HASH_LEN], &[u8])> = canonical_hashes(&ui)
        .into_iter()
        .zip(payloads.iter().map(Vec::as_slice))
        .collect();
    bindings.push(bindings[0]);

    let file = assemble(&ui, &ColdBank::derived(bindings))
        .expect("a bank naming one canonical hash twice is not a bank holding an unnamed payload");
    let container = Container::parse(&file).expect("parses");
    assert_eq!(
        container.len(),
        1 + payloads.len(),
        "the duplicate binding adds no section: sections come from entries",
    );
    for (index, payload) in payloads.iter().enumerate() {
        assert_eq!(container.section_bytes(1 + index), payload.as_slice());
    }
}

#[test]
fn a_ui_section_that_is_not_a_document_is_refused() {
    let bank = ColdBank::raw(Vec::<&[u8]>::new());
    let error = assemble(b"not a flatbuffer at all", &bank).expect_err("refused");
    assert!(matches!(error, AssembleError::Document(_)), "{error}");
}
