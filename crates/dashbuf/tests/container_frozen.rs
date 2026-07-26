//! The frozen container fixture: a `.dsb` envelope written once, checked into
//! the repo, and decoded here **without using the crate's own constants**.
//!
//! `docs/decisions/dsb-frozen-fixture-r7-guard.md` recorded this obligation
//! when the schema guard landed: "The fixture is a single flatbuffer, matching
//! today's `.dsb`. When the sectioned container lands, the envelope needs its
//! own frozen fixture; this one stays as the structured-section guard." Story
//! #399 is that landing.
//!
//! # Why the sibling suite is not enough
//!
//! `tests/container.rs` writes and reads back inside one process with one
//! build, so writer and reader move together. Every number it asserts comes
//! from `dashbuf::container`'s own constants, which makes assertions like
//! `assert_eq!(header.magic, MAGIC)` true by construction. Changing `MAGIC`,
//! `FORMAT_VERSION`, `PAGE_ALIGN`, or a field's offset — in both directions at
//! once, which is how such a change would actually be made — leaves that suite
//! entirely green while every `.dsb` already written to disk stops parsing.
//!
//! This suite exists to fail in exactly that case. Every value below is a
//! literal, transcribed from `docs/design/dsb-container-format.md`, and every
//! byte position is a literal offset into the committed file. Nothing here
//! imports a layout constant.
//!
//! # Regenerating the fixture
//!
//!     UPDATE_CONTAINER_FIXTURE=1 cargo test -p dashbuf --test container_frozen
//!
//! rewrites `tests/fixtures/v0_11_container.dsb` from [`build_fixture`], then
//! reads back what it wrote. **This is not a routine step.** The fixture's
//! whole value is that its bytes never change; regenerating it erases the
//! evidence of whatever broke them. Regenerate only on a deliberate, reviewed
//! envelope version bump — the same posture as `UPDATE_DSB_FIXTURE=1` and
//! `UPDATE_GOLDENS=1`, and never to make this suite go green.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{env, fs};

use dashbuf::container::{Container, Section, SectionKind};

const FIXTURE: &str = "tests/fixtures/v0_11_container.dsb";

/// The two payloads the fixture carries. Short, printable, and distinguishable
/// from each other in a hex dump.
const UI_PAYLOAD: &[u8] = b"ui-section-payload";
const BLOB_PAYLOAD: &[u8] = b"blob-payload";

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)
}

fn fixture_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let path = fixture_path();
        match env::var_os("UPDATE_CONTAINER_FIXTURE") {
            None => {}
            Some(value) if value == "1" => {
                fs::write(&path, build_fixture()).expect("write the container fixture");
                eprintln!("UPDATE_CONTAINER_FIXTURE: wrote {}", path.display());
            }
            Some(other) => panic!(
                "UPDATE_CONTAINER_FIXTURE={} is not recognized — set \
                 UPDATE_CONTAINER_FIXTURE=1 (regenerating destroys the frozen \
                 bytes this guard is made of, so only the documented value is \
                 accepted)",
                other.to_string_lossy()
            ),
        }
        fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "cannot read the frozen container fixture {}: {error}. It is \
                 checked into the repo; a clean checkout has it. Do not \
                 regenerate it to make a test pass — see the module docs.",
                path.display()
            )
        })
    })
}

fn u16_at(at: usize) -> u16 {
    u16::from_le_bytes(fixture_bytes()[at..at + 2].try_into().expect("2 bytes"))
}

fn u32_at(at: usize) -> u32 {
    u32::from_le_bytes(fixture_bytes()[at..at + 4].try_into().expect("4 bytes"))
}

fn u64_at(at: usize) -> u64 {
    u64::from_le_bytes(fixture_bytes()[at..at + 8].try_into().expect("8 bytes"))
}

// ---------------------------------------------------------------------
// The frozen bytes, read at literal offsets
// ---------------------------------------------------------------------

/// The signature, the version, and the two self-describing sizes. A changed
/// magic or a bumped version without a regenerated fixture stops here.
#[test]
fn the_frozen_header_reads_back() {
    assert_eq!(
        &fixture_bytes()[0..8],
        &[0x89, b'D', b'S', b'B', 0x0D, 0x0A, 0x1A, 0x0A],
        "the file signature moved"
    );
    assert_eq!(u16_at(8), 1, "format_version");
    assert_eq!(u16_at(10), 64, "header_size");
    assert_eq!(u16_at(12), 64, "section_stride");
    assert_eq!(u16_at(14), 0, "reserved_0");
    assert_eq!(u32_at(16), 2, "section_count");
    assert_eq!(u32_at(20), 0, "flags");
    assert_eq!(u32_at(56), 0, "signature_offset");
    assert_eq!(u32_at(60), 0, "signature_length");
}

/// The two section entries at their literal positions: the header is 64 bytes,
/// so entry 0 starts at 64 and entry 1 at 128.
///
/// The offsets asserted here are the alignment policy made concrete. Entry 0
/// sits at 192 — immediately after the header and two entries. Entry 1 sits at
/// 4096, the page-aligned hot/cold boundary, even though its payload is 12
/// bytes: the first blob carries the boundary whatever its size.
#[test]
fn the_frozen_section_table_reads_back() {
    // Entry 0 — the structured ui section.
    assert_eq!(u16_at(64), 1, "entry 0 kind: structured");
    assert_eq!(u16_at(66), 1, "entry 0 flavor: ui");
    assert_eq!(u32_at(68), 0, "entry 0 reserved_0");
    assert_eq!(u64_at(72), 192, "entry 0 offset");
    assert_eq!(u64_at(80), UI_PAYLOAD.len() as u64, "entry 0 length");
    assert_eq!(&fixture_bytes()[120..128], &[0; 8], "entry 0 reserved_1");

    // Entry 1 — the asset blob, past the page-aligned boundary.
    assert_eq!(u16_at(128), 2, "entry 1 kind: blob");
    assert_eq!(u16_at(130), 1, "entry 1 flavor: asset");
    assert_eq!(u32_at(132), 0, "entry 1 reserved_0");
    assert_eq!(u64_at(136), 4096, "entry 1 offset");
    assert_eq!(u64_at(144), BLOB_PAYLOAD.len() as u64, "entry 1 length");
    assert_eq!(&fixture_bytes()[184..192], &[0; 8], "entry 1 reserved_1");
}

/// The payloads, at the offsets the table names, and the file's total length.
#[test]
fn the_frozen_payloads_read_back() {
    assert_eq!(&fixture_bytes()[192..192 + UI_PAYLOAD.len()], UI_PAYLOAD);
    assert_eq!(
        &fixture_bytes()[4096..4096 + BLOB_PAYLOAD.len()],
        BLOB_PAYLOAD
    );
    assert_eq!(
        fixture_bytes().len(),
        4096 + BLOB_PAYLOAD.len(),
        "the file ends at the last payload byte"
    );
    assert!(
        fixture_bytes()[192 + UI_PAYLOAD.len()..4096]
            .iter()
            .all(|&b| b == 0),
        "the hot/cold gap is not zero-filled"
    );
}

/// The hashes, as literal hex. These pin the digest algorithm itself: swapping
/// BLAKE3 for another 32-byte hash would leave every round-trip test green and
/// fail here, which is the point.
#[test]
fn the_frozen_hashes_read_back() {
    let hex = |at: usize| {
        fixture_bytes()[at..at + 32]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        hex(24),
        "c2ff1413062e2a2902d323c5bb50154500edfb12b9d647e07aacc55e4398a8d9",
        "root_hash over the 128-byte section table"
    );
    assert_eq!(
        hex(88),
        "dc5ff33dca2f5fcfbd29abfe524aae941fa553feb8b21e5c4e05d0d6599987a9",
        "entry 0 content hash — BLAKE3 of the ui payload"
    );
    assert_eq!(
        hex(152),
        "c950cac188ecfa290b747e2589a2644bebf3d37fe8ec61941620d7b95468fcb1",
        "entry 1 content hash — BLAKE3 of the blob payload"
    );
}

/// The digest, checked against the two published BLAKE3 test vectors rather
/// than against itself.
///
/// The hashes in the fixture came out of this crate, so on their own they prove
/// only that the crate agrees with itself. These two values are from the BLAKE3
/// reference test vectors — the empty input, and the single zero byte — so they
/// anchor the fixture's hashes to the specification. A dependency that silently
/// became a different digest, or a BLAKE3 build with a broken backend on some
/// target, fails here with a message that says what happened.
#[test]
fn the_digest_matches_the_published_blake3_vectors() {
    assert_eq!(
        blake3::hash(b"").to_hex().as_str(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        "BLAKE3 of the empty input"
    );
    assert_eq!(
        blake3::hash(&[0u8]).to_hex().as_str(),
        "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
        "BLAKE3 of a single zero byte"
    );
}

/// The current parser accepts the frozen bytes and resolves them to the same
/// payloads. This is the compatibility half: the assertions above pin what the
/// bytes are, and this one pins that today's code still reads them.
#[test]
fn the_current_parser_accepts_the_frozen_container() {
    let container = Container::parse(fixture_bytes()).expect("the frozen fixture parses");
    assert_eq!(container.len(), 2);
    assert_eq!(container.find(SectionKind::Structured, 1), Some(0));
    assert_eq!(container.find(SectionKind::Blob, 1), Some(1));
    assert_eq!(container.section_bytes(0), UI_PAYLOAD);
    assert_eq!(container.section_bytes(1), BLOB_PAYLOAD);
    container.verify_hot().expect("the hot region verifies");
    container.verify_section(1).expect("the blob verifies");
}

// ---------------------------------------------------------------------
// The writer. Runs only under UPDATE_CONTAINER_FIXTURE=1 — see the module
// docs. Editing it changes nothing until the fixture is regenerated, which is
// the point: the bytes on disk, not this function, are the frozen envelope.
// ---------------------------------------------------------------------

fn build_fixture() -> Vec<u8> {
    dashbuf::container::write(&[
        Section::structured(1, UI_PAYLOAD),
        Section::blob(1, BLOB_PAYLOAD),
    ])
    .expect("the fixture sections are writable")
}
