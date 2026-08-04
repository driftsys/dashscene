//! The prefix envelope reader (story #587): reading a `.dsb` envelope out of a
//! leading byte range, without holding the file.
//!
//! Specified by `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`.
//! `Container::parse` stays strict — it bounds-checks every section against the
//! length of the slice it is given, which is free under `mmap` and fatal in
//! wasm, where the same check would force the whole file into linear memory
//! before the envelope could be read at all.
//!
//! The last group of tests is the record's own stated guard, that the two
//! readers must agree. The record expected a test to be the only thing holding
//! them together; as built they share one implementation of every rule, so the
//! test is a second line rather than the first. It still earns its place: it is
//! what would catch a rule being added to one entry point instead of to the
//! shared walk.

use dashbuf::container::{
    Container, ContainerError, FLAVOR_ASSET, FLAVOR_BINDINGS, FLAVOR_UI, HASH_LEN, HEADER_SIZE,
    MAGIC, SECTION_STRIDE, Section, SectionKind, write,
};
use dashbuf::prefix::{Envelope, MIN_PREFIX, PrefixError};

/// A recognizable payload of `len` bytes, seeded by `seed` so two payloads of
/// the same length are still distinguishable.
fn payload(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// A four-section container: two structured, then two blobs.
///
/// Four rather than two, and no two entries agreeing in any field, because the
/// table is addressed as `HEADER_SIZE + index * SECTION_STRIDE`. Entry zero sits
/// at the base whatever the stride is, so one entry cannot falsify the
/// addressing at all; and two entries that agree in a field cannot tell a
/// reader that reads the wrong one apart from a reader that reads the right
/// one. These four differ in kind, flavor, offset, length and hash.
fn sample() -> Vec<u8> {
    write(&[
        Section::structured(FLAVOR_UI, &payload(1, 128)),
        Section::structured(FLAVOR_BINDINGS, &payload(2, 192)),
        Section::blob(FLAVOR_ASSET, &payload(3, 256)),
        Section::blob(FLAVOR_ASSET, &payload(4, 320)),
    ])
    .expect("writes")
}

/// How many bytes the header and section table of `file` occupy, taken from
/// the reader's own answer rather than restated from the fixture's section
/// count. A restated literal goes stale the moment the fixture grows a
/// section, and it goes stale while still passing.
fn envelope_len(file: &[u8]) -> usize {
    match Envelope::read(&file[..MIN_PREFIX], file.len() as u64) {
        Err(PrefixError::NeedMore { need }) => need,
        other => panic!("a header-sized prefix should ask for the table, got {other:?}"),
    }
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
// The behavior the story exists for
// ---------------------------------------------------------------------

/// The whole point: the envelope is readable from a byte range that the strict
/// reader refuses, because every section in it runs past the end of that range.
#[test]
fn the_envelope_reads_from_a_prefix_the_strict_reader_refuses() {
    let file = sample();
    let prefix = &file[..envelope_len(&file)];

    let envelope = Envelope::read(prefix, file.len() as u64)
        .expect("the envelope is complete in its own prefix");
    assert_eq!(envelope.sections().len(), 4);

    assert_eq!(
        Container::parse(prefix).unwrap_err(),
        ContainerError::SectionOutOfRange { index: 0 },
        "the strict reader is the one that must refuse this, or the story has no subject"
    );
}

#[test]
fn a_prefix_shorter_than_the_header_asks_for_the_header() {
    let file = sample();
    assert_eq!(
        Envelope::read(&file[..MIN_PREFIX - 1], file.len() as u64).unwrap_err(),
        PrefixError::NeedMore { need: MIN_PREFIX }
    );
}

/// A file shorter than the header is not a short prefix of a longer file — it
/// is not a `.dsb` at all, and fetching more of it would return nothing.
///
/// Named with the strict reader's own diagnostic, so the two readers agree on
/// these bytes as they do on every other fault. The first draft answered
/// `NeedMore` here and would have sent a host back for bytes that do not exist;
/// the review caught that it also made the decision record's "a truncated file
/// is still named at the gate" untrue.
#[test]
fn a_file_shorter_than_the_header_is_refused_rather_than_asked_for() {
    assert_eq!(
        Envelope::read(&[], 0).unwrap_err(),
        PrefixError::Malformed(ContainerError::TooSmall { len: 0 })
    );

    let file = sample();
    let runt = &file[..MIN_PREFIX - 1];
    assert_eq!(
        Envelope::read(runt, runt.len() as u64).unwrap_err(),
        PrefixError::Malformed(ContainerError::TooSmall {
            len: MIN_PREFIX - 1
        })
    );
    assert_eq!(
        Container::parse(runt).unwrap_err(),
        ContainerError::TooSmall {
            len: MIN_PREFIX - 1
        },
        "and it is the fault the strict reader names for the same bytes"
    );
}

#[test]
fn a_prefix_holding_only_the_header_asks_for_the_whole_table() {
    let file = sample();
    assert_eq!(
        Envelope::read(&file[..MIN_PREFIX], file.len() as u64).unwrap_err(),
        PrefixError::NeedMore {
            need: HEADER_SIZE + 4 * SECTION_STRIDE
        }
    );
}

/// One byte short of the table is still short. The boundary is where an
/// off-by-one lives, and `need` must not shrink as the prefix grows.
#[test]
fn a_prefix_one_byte_short_of_the_table_still_asks_for_the_whole_table() {
    let file = sample();
    let complete = envelope_len(&file);
    assert_eq!(
        Envelope::read(&file[..complete - 1], file.len() as u64).unwrap_err(),
        PrefixError::NeedMore { need: complete }
    );
}

/// The hot run is the contiguous prefix a host copies in one range: the
/// envelope plus every structured section. It ends at the **last** structured
/// section, which is why the fixture has two — ending at the first would pass
/// with only one.
#[test]
fn the_hot_run_ends_after_the_last_structured_section() {
    let file = sample();
    let envelope = Envelope::read(&file[..envelope_len(&file)], file.len() as u64).expect("reads");

    let last_structured = envelope.sections()[1];
    assert_eq!(
        envelope.hot_len(),
        last_structured.offset + last_structured.length
    );
    assert!(
        envelope.hot_len() < envelope.sections()[2].offset,
        "the first blob must lie outside the hot run"
    );
}

/// A file of nothing but blobs has no structured section to end the hot run,
/// so the run is the envelope alone. Deriving it from "the last structured
/// section" without this case underflows or reaches into a blob.
#[test]
fn the_hot_run_of_a_blob_only_file_is_the_envelope_alone() {
    let file = write(&[
        Section::blob(FLAVOR_ASSET, &payload(3, 256)),
        Section::blob(FLAVOR_ASSET, &payload(4, 320)),
    ])
    .expect("writes");
    let envelope = Envelope::read(&file[..envelope_len(&file)], file.len() as u64).expect("reads");

    assert_eq!(
        envelope.hot_len(),
        (HEADER_SIZE + 2 * SECTION_STRIDE) as u64
    );
}

/// Pulling a blob individually, which is the half of the flow that happens
/// after the hot run is copied. The assertion names the **second** blob: a
/// lookup that ignored the hash and returned the first blob it saw would pass
/// against the first.
#[test]
fn a_blob_is_located_by_its_content_hash() {
    let file = sample();
    let envelope = Envelope::read(&file[..envelope_len(&file)], file.len() as u64).expect("reads");

    let wanted = envelope.sections()[3];
    let range = envelope
        .blob_by_hash(&wanted.hash)
        .expect("the file carries that blob");

    assert_eq!(range.start, wanted.offset);
    assert_eq!(range.end, wanted.offset + wanted.length);
    assert_eq!(
        &file[range.start as usize..range.end as usize],
        &payload(4, 320)[..],
        "the range must name the payload the hash names"
    );
}

#[test]
fn a_hash_no_blob_carries_is_refused() {
    let file = sample();
    let envelope = Envelope::read(&file[..envelope_len(&file)], file.len() as u64).expect("reads");

    assert_eq!(
        envelope.blob_by_hash(&[0xAB; HASH_LEN]).unwrap_err(),
        ContainerError::NoBlobForHash
    );
}

/// A structured section's hash is not a blob's. The lookup searches blobs
/// only, exactly as `Container::blob_by_hash` does — a search over every
/// section would resolve an asset to the ui document.
#[test]
fn a_structured_sections_hash_is_not_found_by_the_blob_lookup() {
    let file = sample();
    let envelope = Envelope::read(&file[..envelope_len(&file)], file.len() as u64).expect("reads");

    let ui = envelope.sections()[0];
    assert_eq!(
        envelope.blob_by_hash(&ui.hash).unwrap_err(),
        ContainerError::NoBlobForHash
    );
}

// ---------------------------------------------------------------------
// The two readers agree — the guard the decision record asks for
// ---------------------------------------------------------------------

/// Given the whole file, the two readers must produce the same section table,
/// entry for entry. They share the layout constants and the decoders; this is
/// what holds their *walk* of the table together.
#[test]
fn the_two_readers_agree_on_the_section_table() {
    let file = sample();
    let strict = Container::parse(&file).expect("parses");
    let envelope = Envelope::read(&file, file.len() as u64).expect("reads");

    assert_eq!(strict.len(), envelope.sections().len());
    for index in 0..strict.len() {
        assert_eq!(
            strict.section(index),
            envelope.sections()[index],
            "entry {index}"
        );
    }
}

/// Every fault both readers can see must be named the same by both. This is
/// the guard that actually catches drift: a check added to one reader and not
/// the other shows up here as one reader accepting what the other refuses.
///
/// The one fault deliberately absent is `SectionOutOfRange` against the slice
/// length, which is the single difference between the two readers and the
/// reason this story exists.
/// One named mutation of a valid container's bytes.
type Fault = (&'static str, Box<dyn Fn(&mut Vec<u8>)>);

#[test]
fn the_two_readers_name_the_same_fault() {
    let cases: Vec<Fault> = vec![
        (
            "bad magic",
            Box::new(|bytes: &mut Vec<u8>| bytes[0..8].copy_from_slice(&[0; 8])),
        ),
        (
            "unsupported version",
            Box::new(|bytes: &mut Vec<u8>| bytes[8..10].copy_from_slice(&2u16.to_le_bytes())),
        ),
        (
            "bad header_size",
            Box::new(|bytes: &mut Vec<u8>| bytes[10..12].copy_from_slice(&32u16.to_le_bytes())),
        ),
        (
            "bad section_stride",
            Box::new(|bytes: &mut Vec<u8>| bytes[12..14].copy_from_slice(&48u16.to_le_bytes())),
        ),
        (
            "header reserved_0 set",
            Box::new(|bytes: &mut Vec<u8>| bytes[14..16].copy_from_slice(&1u16.to_le_bytes())),
        ),
        (
            "flags set",
            Box::new(|bytes: &mut Vec<u8>| bytes[20..24].copy_from_slice(&1u32.to_le_bytes())),
        ),
        (
            "signature_offset set",
            Box::new(|bytes: &mut Vec<u8>| bytes[56..60].copy_from_slice(&1u32.to_le_bytes())),
        ),
        (
            "signature_length set",
            Box::new(|bytes: &mut Vec<u8>| bytes[60..64].copy_from_slice(&1u32.to_le_bytes())),
        ),
        (
            "root hash mismatch",
            Box::new(|bytes: &mut Vec<u8>| bytes[24] ^= 0xFF),
        ),
        (
            "entry reserved_0 set",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(1, 4);
                bytes[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
        (
            "entry reserved_1 set",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(1, 56);
                bytes[at] = 1;
                restamp_root_hash(bytes);
            }),
        ),
        (
            "unknown section kind",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(1, 0);
                bytes[at..at + 2].copy_from_slice(&9u16.to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
        (
            // Entry 3, not entry 2. Retyping the *first* blob as structured
            // leaves the order S,S,S,B — which is valid, and the first draft of
            // this case asserted against a file both readers accepted.
            "structured after blob",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(3, 0);
                bytes[at..at + 2].copy_from_slice(&(SectionKind::Structured as u16).to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
        (
            "section overlaps the table",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(0, 8);
                bytes[at..at + 8].copy_from_slice(&0u64.to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
        (
            "sections out of order",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(2, 8);
                let first = u64::from_le_bytes(
                    bytes[entry_field(0, 8)..entry_field(0, 8) + 8]
                        .try_into()
                        .unwrap(),
                );
                bytes[at..at + 8].copy_from_slice(&first.to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
        (
            "section length overflows",
            Box::new(|bytes: &mut Vec<u8>| {
                let at = entry_field(3, 16);
                bytes[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
                restamp_root_hash(bytes);
            }),
        ),
    ];

    for (name, mutate) in cases {
        let mut bytes = sample();
        mutate(&mut bytes);

        let strict = Container::parse(&bytes).map(|_| ()).unwrap_err();
        let prefix = match Envelope::read(&bytes, bytes.len() as u64) {
            Ok(_) => panic!("{name}: the prefix reader accepted what the strict reader refused"),
            Err(PrefixError::NeedMore { need }) => {
                panic!("{name}: a whole file should not be short, it asked for {need}")
            }
            Err(PrefixError::Malformed(error)) => error,
        };

        assert_eq!(
            strict, prefix,
            "{name}: the two readers name it differently"
        );
    }
}

/// The magic is checked before anything else is believed, on a prefix exactly
/// as on a file. Without this a non-`.dsb` response — an HTML error page from
/// a fetch, which is the realistic wasm case — would be read as a header.
#[test]
fn a_prefix_that_is_not_a_dsb_is_refused_by_magic() {
    let mut bytes = sample();
    bytes[0..8].copy_from_slice(b"<!DOCTYP");
    assert_eq!(
        Envelope::read(&bytes[..MIN_PREFIX], bytes.len() as u64).unwrap_err(),
        PrefixError::Malformed(ContainerError::BadMagic)
    );
    assert_ne!(&bytes[0..8], &MAGIC);
}

/// A section count whose table cannot fit the file must be refused rather than
/// asking for an absurd prefix — a host that trusted `need` would try to fetch
/// it.
///
/// This is why [`Envelope::read`] is given the file's length. `u32::MAX`
/// sections is a 256 GiB table, which fits a 64-bit `usize` and does not fit a
/// 32-bit one, so a reader relying on the multiply to overflow answers
/// differently on wasm32 than on the machine it was written on.
#[test]
fn a_section_count_that_cannot_fit_the_file_is_refused() {
    let mut bytes = sample();
    bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Envelope::read(&bytes[..MIN_PREFIX], bytes.len() as u64).unwrap_err(),
        PrefixError::Malformed(ContainerError::TableOutOfRange)
    );
}
