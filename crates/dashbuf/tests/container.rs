//! The `.dsb` container envelope (story #399): layout, determinism, and one
//! rejection test per failure mode.
//!
//! The layout assertions check *offsets*, not just that a round trip works. A
//! container that packed every section densely would round-trip perfectly and
//! still defeat the whole point of the format, which is that the hot region
//! can be verified without faulting a cold page.

use dashbuf::container::{
    Container, ContainerError, FLAVOR_ASSET, FLAVOR_UI, HASH_LEN, HEADER_SIZE,
    LARGE_BLOB_THRESHOLD, MAGIC, PAGE_ALIGN, SECTION_ALIGN, SECTION_STRIDE, Section, SectionKind,
    WriteError, write,
};

/// A recognizable payload of `len` bytes, seeded by `seed` so two payloads of
/// the same length are still distinguishable.
fn payload(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// Re-stamps the header's root hash over the (possibly mutated) section table,
/// so a structural test reaches the structural check instead of stopping at
/// `RootHashMismatch`. A writer with a bug produces a self-consistent file;
/// this models that, which is the harder case.
fn restamp_root_hash(bytes: &mut [u8]) {
    let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let table_end = HEADER_SIZE + count * SECTION_STRIDE;
    let hash = *blake3::hash(&bytes[HEADER_SIZE..table_end]).as_bytes();
    bytes[24..24 + HASH_LEN].copy_from_slice(&hash);
}

/// Byte offset of field `field_at` inside section-table entry `index`.
fn entry_field(index: usize, field_at: usize) -> usize {
    HEADER_SIZE + index * SECTION_STRIDE + field_at
}

// ---------------------------------------------------------------------
// Round trip and layout
// ---------------------------------------------------------------------

#[test]
fn one_structured_section_round_trips() {
    let ui = payload(1, 300);
    let bytes = write(&[Section::structured(FLAVOR_UI, &ui)]).expect("writes");

    let container = Container::parse(&bytes).expect("parses");
    assert_eq!(container.len(), 1);
    assert!(!container.is_empty());

    let header = container.header();
    assert_eq!(header.magic, MAGIC);
    assert_eq!(header.section_count, 1);
    assert_eq!(header.flags, 0);
    // The signature reference is reserved in this version and written zero.
    assert_eq!(header.signature_offset, 0);
    assert_eq!(header.signature_length, 0);

    let entry = container.section(0);
    assert_eq!(entry.kind, SectionKind::Structured as u16);
    assert_eq!(entry.flavor, FLAVOR_UI);
    assert_eq!(entry.length, ui.len() as u64);
    assert_eq!(container.section_bytes(0), &ui[..]);
    container.verify_section(0).expect("payload hash matches");
    container.verify_hot().expect("hot region verifies");
}

/// The one place alignment is format-relevant: the boundary between the last
/// hot byte and the first cold byte is page-aligned, so the load gate can
/// verify the hot region without faulting a cold page.
#[test]
fn the_hot_cold_boundary_is_page_aligned() {
    let ui = payload(1, 300);
    let blob = payload(2, 100);
    let bytes = write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &blob),
    ])
    .expect("writes");

    let container = Container::parse(&bytes).expect("parses");
    let structured = container.section(0);
    let cold = container.section(1);

    // The structured section sits immediately after the table, on the small
    // quantum — header + two entries is already 192, itself 64-aligned.
    assert_eq!(structured.offset, (HEADER_SIZE + 2 * SECTION_STRIDE) as u64);
    assert_eq!(cold.offset % PAGE_ALIGN as u64, 0);
    assert!(cold.offset >= structured.offset + structured.length);
    assert_eq!(container.section_bytes(1), &blob[..]);
}

/// A document with no assets must not pay for a boundary it does not have.
/// Getting this wrong would add up to a page of zeros to every committed
/// golden for nothing.
#[test]
fn no_blobs_means_no_page_padding() {
    let ui = payload(1, 300);
    let bytes = write(&[Section::structured(FLAVOR_UI, &ui)]).expect("writes");
    assert_eq!(bytes.len(), HEADER_SIZE + SECTION_STRIDE + ui.len());
}

/// Small blobs pack on the 64-byte quantum; a blob at or above the large
/// threshold gets its own page so it can be prefetched and evicted alone.
#[test]
fn blob_alignment_follows_size() {
    let ui = payload(1, 16);
    let small_a = payload(2, 100);
    let small_b = payload(3, 100);
    let large = payload(4, LARGE_BLOB_THRESHOLD);
    let bytes = write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &small_a),
        Section::blob(FLAVOR_ASSET, &small_b),
        Section::blob(FLAVOR_ASSET, &large),
    ])
    .expect("writes");

    let container = Container::parse(&bytes).expect("parses");
    let first_small = container.section(1);
    let second_small = container.section(2);
    let large_entry = container.section(3);

    // The first blob carries the hot/cold boundary regardless of its size.
    assert_eq!(first_small.offset % PAGE_ALIGN as u64, 0);
    // The second small blob packs densely behind it — same page, in fact.
    assert_eq!(second_small.offset % SECTION_ALIGN as u64, 0);
    assert!(second_small.offset < first_small.offset + PAGE_ALIGN as u64);
    // The large one starts its own page.
    assert_eq!(large_entry.offset % PAGE_ALIGN as u64, 0);

    for (index, expected) in [(1, &small_a), (2, &small_b), (3, &large)] {
        assert_eq!(container.section_bytes(index), &expected[..]);
        container.verify_section(index).expect("payload verifies");
    }
}

#[test]
fn every_section_starts_on_the_small_quantum() {
    let ui = payload(1, 37);
    let a = payload(2, 1);
    let b = payload(3, 65);
    let bytes = write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &a),
        Section::blob(FLAVOR_ASSET, &b),
    ])
    .expect("writes");

    let container = Container::parse(&bytes).expect("parses");
    for entry in container.sections() {
        assert_eq!(
            entry.offset % SECTION_ALIGN as u64,
            0,
            "section at {} is not on the {SECTION_ALIGN}-byte quantum",
            entry.offset
        );
    }
}

/// Every gap the writer leaves is zero-filled. Padding that carried
/// uninitialized or stale bytes would break R7 while every round trip stayed
/// green.
/// Every gap the writer leaves — between structured sections, across the
/// hot/cold boundary, and between blobs — is zero-filled. The fixture is built
/// so all three classes are present and none is a multiple of the alignment.
#[test]
fn alignment_padding_is_zero_filled() {
    let ui_a = payload(1, 37);
    let ui_b = payload(2, 5);
    let small = payload(3, 3);
    let large = payload(4, LARGE_BLOB_THRESHOLD + 7);
    let bytes = write(&[
        Section::structured(FLAVOR_UI, &ui_a),
        Section::structured(2, &ui_b),
        Section::blob(FLAVOR_ASSET, &small),
        Section::blob(FLAVOR_ASSET, &large),
    ])
    .expect("writes");
    let container = Container::parse(&bytes).expect("parses");

    let mut gaps = 0;
    let mut previous_end = (HEADER_SIZE + container.len() * SECTION_STRIDE) as u64;
    for (index, entry) in container.sections().enumerate() {
        let gap = &bytes[previous_end as usize..entry.offset as usize];
        if !gap.is_empty() {
            gaps += 1;
            assert!(
                gap.iter().all(|&b| b == 0),
                "the gap before section {index} is not zero-filled"
            );
        }
        previous_end = entry.offset + entry.length;
    }
    assert!(gaps >= 3, "the fixture must exercise all three gap classes");
    assert_eq!(
        previous_end as usize,
        bytes.len(),
        "the file ends at the last payload byte"
    );
}

#[test]
fn writing_twice_is_byte_identical() {
    let ui = payload(1, 300);
    let blob = payload(2, 5000);
    let sections = [
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &blob),
    ];
    assert_eq!(write(&sections).unwrap(), write(&sections).unwrap());
}

#[test]
fn find_locates_a_section_by_kind_and_flavor() {
    let ui = payload(1, 8);
    let blob = payload(2, 8);
    let bytes = write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &blob),
    ])
    .expect("writes");
    let container = Container::parse(&bytes).expect("parses");

    assert_eq!(container.find(SectionKind::Structured, FLAVOR_UI), Some(0));
    assert_eq!(container.find(SectionKind::Blob, FLAVOR_ASSET), Some(1));
    assert_eq!(container.find(SectionKind::Structured, 99), None);
}

#[test]
fn an_empty_container_parses() {
    let bytes = write(&[]).expect("writes");
    let container = Container::parse(&bytes).expect("parses");
    assert_eq!(container.len(), 0);
    assert!(container.is_empty());
    assert_eq!(bytes.len(), HEADER_SIZE);
    container.verify_hot().expect("nothing to verify");
}

// ---------------------------------------------------------------------
// Verification is on demand, not at parse
// ---------------------------------------------------------------------

/// Parsing must not hash payloads: a caller verifying only the hot region
/// must not be made to touch a cold page. The observable consequence is that
/// a corrupted blob parses fine and only fails when it is asked about.
#[test]
fn parse_does_not_hash_payloads_and_verify_hot_skips_blobs() {
    let ui = payload(1, 64);
    let blob = payload(2, 64);
    let mut bytes = write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &blob),
    ])
    .expect("writes");

    let cold_at = Container::parse(&bytes).expect("parses").section(1).offset as usize;
    bytes[cold_at] ^= 0xFF;

    let container = Container::parse(&bytes).expect("a corrupted blob still parses");
    container
        .verify_hot()
        .expect("the hot region is untouched by a cold-page corruption");
    assert_eq!(
        container.verify_section(1),
        Err(ContainerError::SectionHashMismatch { index: 1 })
    );
}

#[test]
fn a_corrupted_structured_payload_fails_hot_verification() {
    let ui = payload(1, 64);
    let mut bytes = write(&[Section::structured(FLAVOR_UI, &ui)]).expect("writes");
    let at = Container::parse(&bytes).expect("parses").section(0).offset as usize;
    bytes[at] ^= 0xFF;

    let container = Container::parse(&bytes).expect("parses");
    assert_eq!(
        container.verify_hot(),
        Err(ContainerError::SectionHashMismatch { index: 0 })
    );
}

// ---------------------------------------------------------------------
// Rejection — one test per failure mode
// ---------------------------------------------------------------------

/// A valid two-section container to mutate.
fn sample() -> Vec<u8> {
    let ui = payload(1, 128);
    let blob = payload(2, 128);
    write(&[
        Section::structured(FLAVOR_UI, &ui),
        Section::blob(FLAVOR_ASSET, &blob),
    ])
    .expect("writes")
}

#[test]
fn a_short_buffer_is_refused() {
    assert_eq!(
        Container::parse(&[]).unwrap_err(),
        ContainerError::TooSmall { len: 0 }
    );
    assert_eq!(
        Container::parse(&sample()[..HEADER_SIZE - 1]).unwrap_err(),
        ContainerError::TooSmall {
            len: HEADER_SIZE - 1
        }
    );
}

/// A pre-envelope `.dsb` — a bare flatbuffer — lands here. There is no
/// transitional reader by design: accepting both shapes would let a stale
/// golden pass under either and hide exactly the drift the format change has
/// to expose.
#[test]
fn a_bare_flatbuffer_is_refused_by_magic() {
    let mut bytes = sample();
    bytes[0..8].copy_from_slice(b"\x0c\x00\x00\x00\x00\x00\x00\x00");
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::BadMagic
    );
}

#[test]
fn an_unknown_format_version_is_refused() {
    let mut bytes = sample();
    bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::UnsupportedVersion { found: 2 }
    );
}

#[test]
fn a_changed_header_size_or_stride_is_refused() {
    let mut bytes = sample();
    bytes[10..12].copy_from_slice(&80u16.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::BadLayout {
            field: "header_size",
            found: 80
        }
    );

    let mut bytes = sample();
    bytes[12..14].copy_from_slice(&128u16.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::BadLayout {
            field: "section_stride",
            found: 128
        }
    );
}

#[test]
fn a_non_zero_reserved_field_is_refused() {
    let mut bytes = sample();
    bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::ReservedNotZero {
            field: "header.reserved_0"
        }
    );

    // The three header fields outside `root_hash`'s range. Nothing else would
    // notice these being set, so if `parse` does not refuse them, a later
    // writer can slip meaning past a version-1 reader.
    for (at, field) in [
        (20usize, "header.flags"),
        (56, "header.signature_offset"),
        (60, "header.signature_length"),
    ] {
        let mut bytes = sample();
        bytes[at..at + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            ContainerError::ReservedNotZero { field },
            "a non-zero {field} was accepted"
        );
    }

    // Both entry reserved fields: reserved_0 at offset 4, reserved_1 at 56.
    for (field_at, field) in [(4usize, "section.reserved_0"), (56, "section.reserved_1")] {
        let mut bytes = sample();
        bytes[entry_field(1, field_at)] = 1;
        restamp_root_hash(&mut bytes);
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            ContainerError::ReservedNotZero { field }
        );
    }
}

#[test]
fn a_section_table_past_the_end_is_refused() {
    let mut bytes = sample();
    bytes[16..20].copy_from_slice(&9999u32.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::TableOutOfRange
    );

    // A count whose table size overflows `usize` must not wrap into a small
    // range that then passes the bounds check.
    let mut bytes = sample();
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::TableOutOfRange
    );
}

#[test]
fn a_section_past_the_end_is_refused() {
    let mut bytes = sample();
    // length at offset 16 of entry 1.
    let at = entry_field(1, 16);
    bytes[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::SectionOutOfRange { index: 1 }
    );
}

#[test]
fn a_section_reaching_into_the_table_is_refused() {
    let mut bytes = sample();
    // offset at offset 8 of entry 0, pulled back over the header.
    let at = entry_field(0, 8);
    bytes[at..at + 8].copy_from_slice(&0u64.to_le_bytes());
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::SectionOverlapsTable { index: 0 }
    );
}

#[test]
fn overlapping_sections_are_refused() {
    let mut bytes = sample();
    // Pull entry 1's offset back on top of entry 0's payload.
    let entry_0_offset = Container::parse(&bytes).expect("parses").section(0).offset;
    let at = entry_field(1, 8);
    bytes[at..at + 8].copy_from_slice(&entry_0_offset.to_le_bytes());
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::SectionsOutOfOrder { index: 1 }
    );
}

#[test]
fn an_unknown_section_kind_is_refused() {
    let mut bytes = sample();
    bytes[entry_field(1, 0)] = 7;
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::UnknownSectionKind { index: 1, kind: 7 }
    );
}

#[test]
fn a_tampered_section_table_is_refused() {
    let mut bytes = sample();
    // Change the flavor of entry 0 without re-stamping: the table no longer
    // matches the root hash, and that is caught before anything is read out
    // of it.
    bytes[entry_field(0, 2)] ^= 0xFF;
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::RootHashMismatch
    );
}

#[test]
fn a_section_reaching_past_the_last_byte_is_refused() {
    let mut bytes = sample();
    // Grow entry 1's length by one byte — in range for the `checked_add`, out
    // of range for the file. The `u64::MAX` case above only exercises the
    // overflow branch; this one exercises the plain bounds check.
    let entry_1_length = Container::parse(&bytes).expect("parses").section(1).length;
    let at = entry_field(1, 16);
    bytes[at..at + 8].copy_from_slice(&(entry_1_length + 1).to_le_bytes());
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::SectionOutOfRange { index: 1 }
    );
}

/// The reader must not trust the writer on this: a hand-built file that puts a
/// structured section behind the cold boundary would make `verify_hot` hash a
/// cold page — the exact fault the format exists to avoid.
#[test]
fn a_structured_section_after_a_blob_is_refused_at_parse() {
    let mut bytes = sample();
    // Swap the two kind values so entry 1, out past the page boundary, claims
    // to be structured.
    bytes[entry_field(0, 0)] = SectionKind::Blob as u8;
    bytes[entry_field(1, 0)] = SectionKind::Structured as u8;
    restamp_root_hash(&mut bytes);
    assert_eq!(
        Container::parse(&bytes).unwrap_err(),
        ContainerError::StructuredAfterBlob { index: 1 }
    );
}

// ---------------------------------------------------------------------
// Writer refusals
//
// `WriteError::TooManySections` has no test: reaching it needs 2^32 sections,
// which cannot be constructed. Every other failure mode has one.
// ---------------------------------------------------------------------

#[test]
fn a_structured_section_after_a_blob_is_refused() {
    let ui = payload(1, 8);
    let blob = payload(2, 8);
    assert_eq!(
        write(&[
            Section::blob(FLAVOR_ASSET, &blob),
            Section::structured(FLAVOR_UI, &ui),
        ]),
        Err(WriteError::StructuredAfterBlob { index: 1 })
    );
}

/// An empty section would still claim its alignment. For the first blob that
/// is a whole page of padding for a payload that does not exist, which is the
/// same cost the no-blob rule exists to avoid.
#[test]
fn an_empty_payload_is_refused() {
    let ui = payload(1, 8);
    assert_eq!(
        write(&[Section::structured(FLAVOR_UI, &[])]),
        Err(WriteError::EmptyPayload { index: 0 })
    );
    assert_eq!(
        write(&[
            Section::structured(FLAVOR_UI, &ui),
            Section::blob(FLAVOR_ASSET, &[]),
        ]),
        Err(WriteError::EmptyPayload { index: 1 })
    );
}
