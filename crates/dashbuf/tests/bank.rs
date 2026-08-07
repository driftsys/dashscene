//! Cold-bank assembly (stories #433 and #434): what `dashbuf::bank::assemble`
//! puts where, the derivation manifest that makes a derived bank readable, and
//! the one invariant a single assembly could not test.
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
//! enough for the properties here, which are about where assembly puts bytes
//! and what the manifest records — not about what a packer produces. The
//! measurement against a real HiFi packing, over real compiler output, is
//! `goldens/tooling/tests/derived_bank.rs`.
//!
//! # Three assets, not one
//!
//! This is the half of story #434's coverage that the golden cannot carry.
//!
//! Every document in the corpus has at most one asset (`goldens/dsb/README.md`:
//! `v03-paint.dsb` is the only fixture with an image), and a one-asset document
//! cannot fail an ordering, resolution, or wrong-index bug — every index in it
//! is 0. The documents here are hand-built with three assets of three different
//! sizes, one of them over the large-blob page-alignment threshold, so those
//! bugs have somewhere to show.

use dashbuf::bank::{AssembleError, ColdBank, assemble};
use dashbuf::container::{
    Container, FLAVOR_ASSET, FLAVOR_BINDINGS, FLAVOR_UI, HASH_LEN, HEADER_SIZE,
    LARGE_BLOB_THRESHOLD, SECTION_STRIDE, SectionKind,
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

    let (document, resident) = dashbuf::open_verified(&file).expect("the assembled file opens");
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

    // The invariant, in the terms the document actually promises: the ui
    // section's *bytes*. An `AssetEntry` names a hash and never an offset, so
    // nothing in the document depends on where the section sits.
    assert_eq!(
        a.ui_document().expect("a ui section"),
        b.ui_document().expect("a ui section"),
        "the hot section is byte-identical across the two assemblies",
    );
    assert_eq!(a.section(0).length, b.section(0).length);
    assert_eq!(a.section(0).hash, b.section(0).hash);

    // What the derived assembly *does* add, stated exactly rather than left as
    // "the files differ somewhere": one section, the derivation manifest, and
    // the one 64-byte table stride it costs. A change that added a second
    // section, or that moved the ui section for any other reason, fails here.
    assert_eq!(
        a.len() + 1,
        b.len(),
        "the manifest is the one added section"
    );
    assert!(
        a.bindings_manifest().expect("a well-formed file").is_none(),
        "the RAW assembly is the identity map, so it writes no manifest",
    );
    assert!(
        b.bindings_manifest().expect("a well-formed file").is_some(),
        "the derived assembly writes the manifest its payloads need",
    );
    assert_eq!(
        b.section(0).offset,
        a.section(0).offset + SECTION_STRIDE as u64,
        "the ui section moves by exactly the manifest's table entry and nothing more",
    );
}

#[test]
fn every_difference_between_two_assemblies_is_envelope_manifest_or_cold_bytes() {
    // The invariant in its strongest form: not "the hot section survived", but
    // "nothing outside the envelope, the manifest and the cold payloads changed
    // at all".
    //
    // The derived file is the one measured against, because it is the one with
    // the extra section: its envelope is a stride longer and its ui section a
    // stride further in, so a range taken from the RAW file would put the
    // manifest's own bytes outside every named region.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let derived = vec![vec![0x11; 40], vec![0x22; 4096], vec![0x33; 3]];

    let raw = assemble(&ui, &ColdBank::raw(payloads.iter().map(Vec::as_slice)))
        .expect("the RAW bank assembles");
    let hifi = assemble(&ui, &stand_in_derived_bank(&ui, &derived))
        .expect("the stand-in derived bank assembles");
    let container = Container::parse(&hifi).expect("parses");

    // The envelope: the header and the whole section table. The header's root
    // hash covers the table, and the table records where the cold bytes are, so
    // both are expected to differ.
    let envelope_end = HEADER_SIZE + container.len() * SECTION_STRIDE;
    // The manifest section: present only in the derived file, so every byte of
    // it is a difference by construction.
    let manifest = container.section(1);
    let manifest_range = manifest.offset as usize..(manifest.offset + manifest.length) as usize;
    // The cold region: from the first blob's offset to the end of the longer
    // file. Nothing hot lives past it — the hot region is a contiguous prefix.
    let first_cold = container.section(2).offset as usize;

    // The ui section is the one region that must be equal, and in the derived
    // file it sits one stride later than in the RAW one.
    let ui_at = container.section(0).offset as usize;
    assert_eq!(
        &hifi[ui_at..ui_at + ui.len()],
        &raw[ui_at - SECTION_STRIDE..ui_at - SECTION_STRIDE + ui.len()],
        "the ui section is the same bytes at its shifted address",
    );

    // Everything else: compared at the derived file's own addresses, with the
    // RAW file's bytes taken from a stride earlier, which is where the same
    // hot byte lives there.
    let hot = envelope_end..first_cold.min(hifi.len());
    for (offset, (derived_byte, raw_byte)) in hifi[hot.clone()]
        .iter()
        .zip(&raw[envelope_end - SECTION_STRIDE..])
        .enumerate()
    {
        let at = envelope_end + offset;
        if manifest_range.contains(&at) {
            continue;
        }
        assert_eq!(
            derived_byte,
            raw_byte,
            "byte {at} of the derived file differs from byte {} of the RAW one, \
             and it lies in neither the envelope (0..{envelope_end}), the manifest \
             ({manifest_range:?}), nor the cold region ({first_cold}..)",
            at - SECTION_STRIDE,
        );
    }
    // The mirror above only reaches into the RAW file's own hot region. It does
    // so only because the RAW file is at least as long as the derived file's
    // hot region minus the stride — assert it rather than trust it, or a
    // shortened RAW file would make the loop compare nothing.
    assert!(
        raw.len() >= first_cold - SECTION_STRIDE,
        "the RAW file ({} bytes) is shorter than the hot region this compared \
         against it ({}), so the loop above skipped bytes",
        raw.len(),
        first_cold - SECTION_STRIDE,
    );
}

// ---------------------------------------------------------------------------
// The derivation manifest
// ---------------------------------------------------------------------------

#[test]
fn a_derived_assembly_round_trips_through_open() {
    // The read side of a bank whose payloads are not their own preimage, which
    // is the whole of story #434. Three assets with three distinct derived
    // payloads, so a manifest that resolved every canonical hash to one payload,
    // or that paired the rows off by one, comes back wrong rather than plausible.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let derived = vec![vec![0x11; 40], vec![0x22; 4096], vec![0x33; 3]];

    let file = assemble(&ui, &stand_in_derived_bank(&ui, &derived)).expect("assembles");

    let (document, resident) = dashbuf::open_verified(&file).expect("the derived file opens");
    assert_eq!(document.assets().expect("assets").len(), payloads.len());
    let expected: Vec<&[u8]> = derived.iter().map(Vec::as_slice).collect();
    assert_eq!(
        resident, expected,
        "each entry resolves through the manifest to the payload bound to it, in entry order",
    );
    for (index, canonical) in payloads.iter().enumerate() {
        assert_ne!(
            resident[index],
            canonical.as_slice(),
            "asset {index} resolved to its canonical bytes, so nothing was derived",
        );
    }
}

#[test]
fn the_manifest_carries_a_row_for_every_derived_binding_and_none_for_an_identity_one() {
    // A mixed bank: entry 1 keeps its canonical payload while 0 and 2 are
    // derived. The manifest must be exactly the two derived rows — a writer that
    // recorded every binding would put an identity row in the file, and a writer
    // that recorded none would leave the derived payloads unresolvable.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let hashes = canonical_hashes(&ui);
    let derived_0 = vec![0x11; 40];
    let derived_2 = vec![0x33; 3];

    let file = assemble(
        &ui,
        &ColdBank::derived([
            (hashes[0], derived_0.as_slice()),
            (hashes[1], payloads[1].as_slice()),
            (hashes[2], derived_2.as_slice()),
        ]),
    )
    .expect("assembles");

    let container = Container::parse(&file).expect("parses");
    let manifest = container
        .bindings_manifest()
        .expect("a well-formed file")
        .expect("a mixed bank needs a manifest");
    let rows = flatbuffers::root::<dashbuf::AssetBindings<'_>>(manifest)
        .expect("a valid binding table")
        .bindings()
        .expect("rows");

    assert_eq!(rows.len(), 2, "one row per derived binding, and no more");
    assert_eq!(rows.get(0).canonical().bytes(), hashes[0]);
    assert_eq!(
        rows.get(0).resident().bytes(),
        blake3::hash(&derived_0).as_bytes(),
    );
    assert_eq!(rows.get(1).canonical().bytes(), hashes[2]);
    assert_eq!(
        rows.get(1).resident().bytes(),
        blake3::hash(&derived_2).as_bytes(),
    );

    // And the identity binding still resolves, through the absent row.
    let (_, resident) = dashbuf::open_verified(&file).expect("opens");
    assert_eq!(resident[1], payloads[1].as_slice());
}

#[test]
fn a_bank_that_is_the_identity_map_writes_no_manifest_at_all() {
    // The property every committed golden depends on. `ColdBank::derived` over
    // canonical payloads is the same bank as `ColdBank::raw`, so it must
    // assemble to the same bytes — a manifest emitted here would have moved
    // seven goldens for nothing.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let hashes = canonical_hashes(&ui);

    let raw = assemble(&ui, &ColdBank::raw(payloads.iter().map(Vec::as_slice)))
        .expect("the RAW bank assembles");
    let spelled_out = assemble(
        &ui,
        &ColdBank::derived(
            hashes
                .into_iter()
                .zip(payloads.iter().map(Vec::as_slice))
                .collect::<Vec<_>>(),
        ),
    )
    .expect("assembles");

    assert_eq!(
        raw, spelled_out,
        "a binding is derived when the payload is not its own preimage, and by no other \
         signal: the two constructors describe the same bank here",
    );
    assert!(
        Container::parse(&raw)
            .expect("parses")
            .bindings_manifest()
            .expect("a well-formed file")
            .is_none(),
    );
}

#[test]
fn tampering_with_a_manifest_row_is_caught_before_it_resolves_anything() {
    // The manifest is a section, so its content hash is in the section table and
    // the table is covered by the header's root hash. Redirecting a canonical
    // hash at a different payload therefore cannot be done quietly, which is
    // what lets a reader trust the mapping at all.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let derived = vec![vec![0x11; 40], vec![0x22; 4096], vec![0x33; 3]];
    let mut file = assemble(&ui, &stand_in_derived_bank(&ui, &derived)).expect("assembles");

    let manifest = Container::parse(&file).expect("parses").section(1);
    let at = manifest.offset as usize;
    // Flip one bit inside a resident hash.
    file[at + manifest.length as usize - 1] ^= 0x01;

    let error = dashbuf::open_verified(&file).expect_err("a tampered manifest is refused");
    assert!(
        matches!(
            error,
            dashbuf::OpenError::Container(
                dashbuf::container::ContainerError::SectionHashMismatch { .. }
            )
        ),
        "{error}",
    );
}

/// A file carrying `ui` and hand-built manifest bytes, with no blob sections.
///
/// The manifest cases below are shapes `assemble` cannot produce, so they have
/// to be written directly. The document names no assets, which keeps every
/// failure below about the manifest and nothing else.
fn file_with_manifest(manifest: &[u8]) -> Vec<u8> {
    dashbuf::container::write(&[
        dashbuf::container::Section::structured(FLAVOR_UI, &ui_section(&[])),
        dashbuf::container::Section::structured(FLAVOR_BINDINGS, manifest),
    ])
    .expect("the hand-built sections are writable")
}

/// An `AssetBindings` buffer over `rows`, with no length rule applied.
fn manifest_bytes(rows: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let bindings: Vec<_> = rows
        .iter()
        .map(|(canonical, resident)| {
            let canonical = builder.create_vector(canonical);
            let resident = builder.create_vector(resident);
            dashbuf::AssetBinding::create(
                &mut builder,
                &dashbuf::AssetBindingArgs {
                    canonical: Some(canonical),
                    resident: Some(resident),
                },
            )
        })
        .collect();
    let bindings = builder.create_vector(&bindings);
    let manifest = dashbuf::AssetBindings::create(
        &mut builder,
        &dashbuf::AssetBindingsArgs {
            bindings: Some(bindings),
        },
    );
    builder.finish(manifest, None);
    builder.finished_data().to_vec()
}

#[test]
fn a_manifest_row_whose_hash_is_the_wrong_length_is_refused() {
    // It could never match an asset entry or a blob section, so tolerating it
    // would be a claim that silently does nothing — the drop P4 forbids.
    let file = file_with_manifest(&manifest_bytes(&[(&[0xAA; 8], &[0xBB; HASH_LEN])]));
    let error = dashbuf::open_verified(&file).expect_err("refused");
    assert!(
        matches!(
            error,
            dashbuf::OpenError::BindingHashLength {
                row: 0,
                canonical: 8,
                resident: 32
            }
        ),
        "{error}",
    );
}

#[test]
fn a_manifest_that_binds_one_canonical_hash_twice_is_refused() {
    // Two rows are two answers to what one asset resides as, and resolving the
    // first would be a silent choice between them.
    let file = file_with_manifest(&manifest_bytes(&[
        (&[0xAA; HASH_LEN], &[0xB1; HASH_LEN]),
        (&[0xCC; HASH_LEN], &[0xB2; HASH_LEN]),
        (&[0xAA; HASH_LEN], &[0xB3; HASH_LEN]),
    ]));
    let error = dashbuf::open_verified(&file).expect_err("refused");
    assert!(
        matches!(error, dashbuf::OpenError::BindingRepeated { row: 2 }),
        "{error}",
    );
}

#[test]
fn a_manifest_section_that_is_not_a_binding_table_is_refused() {
    let file = file_with_manifest(b"not a flatbuffer at all");
    let error = dashbuf::open_verified(&file).expect_err("refused");
    assert!(matches!(error, dashbuf::OpenError::Bindings(_)), "{error}");
}

#[test]
fn two_manifest_sections_are_refused() {
    let manifest = manifest_bytes(&[(&[0xAA; HASH_LEN], &[0xBB; HASH_LEN])]);
    let ui = ui_section(&[]);
    let file = dashbuf::container::write(&[
        dashbuf::container::Section::structured(FLAVOR_UI, &ui),
        dashbuf::container::Section::structured(FLAVOR_BINDINGS, &manifest),
        dashbuf::container::Section::structured(FLAVOR_BINDINGS, &manifest),
    ])
    .expect("writable");

    let error = dashbuf::open_verified(&file).expect_err("refused");
    assert!(
        matches!(
            error,
            dashbuf::OpenError::Container(
                dashbuf::container::ContainerError::NotOneBindingsSection { found: 2 }
            )
        ),
        "{error}",
    );
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[test]
fn a_bank_binding_one_canonical_hash_to_two_payloads_is_refused() {
    // One file cannot carry both under one identity, so assembling would drop a
    // claim silently. Unreachable from `ColdBank::raw` by construction — a
    // payload is bound to its own hash there — which is why this is checked on
    // the bank and not on the file.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);
    let hashes = canonical_hashes(&ui);
    let other = vec![0x44; 17];

    let mut bindings: Vec<([u8; HASH_LEN], &[u8])> = hashes
        .iter()
        .copied()
        .zip(payloads.iter().map(Vec::as_slice))
        .collect();
    bindings.push((hashes[1], other.as_slice()));

    assert_eq!(
        assemble(&ui, &ColdBank::derived(bindings)),
        Err(AssembleError::ContradictoryBinding {
            canonical: hashes[1]
        }),
    );
}

#[test]
fn the_identity_map_cannot_produce_a_contradictory_bank() {
    // The proof behind `dashc::package`'s `expect`, as a test rather than as a
    // doc comment alone. `ColdBank::raw` binds every payload to its own hash, so
    // two bindings under one canonical hash are two *identical* payloads no
    // matter what the caller passes — the contradiction is unconstructible from
    // that side, which is what makes a panic there sound and a panic in the
    // packer's assembly path unsound.
    let payloads = canonical_payloads();
    let ui = ui_section(&payloads);

    let mut repeated: Vec<&[u8]> = Vec::new();
    for payload in &payloads {
        repeated.push(payload);
        repeated.push(payload);
    }
    repeated.push(payloads[0].as_slice());

    assemble(&ui, &ColdBank::raw(repeated))
        .expect("no sequence of canonical payloads makes a self-contradicting bank");
}

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
