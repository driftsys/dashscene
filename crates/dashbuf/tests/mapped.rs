//! Loading a document out of a memory mapping (story #595, epic #594).
//!
//! `docs/decisions/dsb-sectioned-container.md` has specified the loading model
//! as "one `mmap` of the whole file, once" since v0.11, and
//! `crates/dashbuf/src/container.rs` has said an mmap "is therefore a drop-in"
//! for just as long. Neither was ever true in fact, because nothing in the
//! workspace mapped anything. These tests are what turns the claim into a
//! measurement.
//!
//! Two things are asserted, and the second is the one with teeth:
//!
//! - **A mapped file loads what a read file loads.** Same documents, same
//!   payloads, over every committed fixture. This is the drop-in claim.
//! - **Every section a mapped file hands back is a pointer into the mapping.**
//!   Not equal bytes — the same bytes, at the offset the section table
//!   declares. A reader that quietly copied would satisfy the first test and
//!   fail this one, and so would a reader that returned the right bytes from
//!   the wrong offset.
//!
//! The second is checked per section rather than per asset payload, because
//! only two of the committed fixtures carry an asset at all. Walking every
//! section of every fixture is what gives an offset bug somewhere to show, and
//! the test asserts that the walk really did see more than one distinct
//! non-zero offset — against a single offset, a wrong base or a wrong stride
//! still lands on the only value there is.

use std::io::Write;
use std::path::PathBuf;

use dashbuf::container::Container;
use dashbuf::map::MappedFile;

/// Every committed `.dsb` golden, read from the tree rather than listed here.
///
/// The same walk `tests/prefix_load.rs` does, and for the same reason: a
/// restated list goes stale while still passing, so the suite would simply stop
/// covering a fixture added later.
fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{} is not readable: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "dsb"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no .dsb fixtures in {}", dir.display());
    paths
}

/// The drop-in claim: `open` over a mapping produces what `open` over a `Vec`
/// produces.
#[test]
fn a_mapped_file_loads_what_a_read_file_loads() {
    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let read = std::fs::read(&path).expect("the fixture reads");
        let mapped = MappedFile::open(&path).expect("the fixture maps");

        assert_eq!(mapped.bytes(), read.as_slice(), "{name}: mapped bytes");

        let (want_document, want_payloads) =
            dashbuf::open_verified(&read).expect("open accepts a golden");
        let (got_document, got_payloads) =
            dashbuf::open_verified(&mapped).expect("open accepts a mapping");

        assert_eq!(got_payloads, want_payloads, "{name}: payloads");
        assert_eq!(
            got_document.nodes().map(|nodes| nodes.len()),
            want_document.nodes().map(|nodes| nodes.len()),
            "{name}: node count"
        );
        assert_eq!(
            got_document.assets().map(|assets| assets.len()),
            want_document.assets().map(|assets| assets.len()),
            "{name}: asset count"
        );
    }
}

/// Nothing is copied: every section is the mapping's own memory, at the offset
/// the section table declares.
///
/// This is the property mapping exists for. Comparing bytes cannot see it —
/// a copy has equal bytes — so the assertion is on the address, and the
/// address is checked against the table rather than merely against the
/// mapping's bounds, which a wrong-offset read would still satisfy.
#[test]
fn every_section_of_a_mapped_file_is_a_pointer_into_the_mapping() {
    let mut offsets_seen: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();

    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let mapped = MappedFile::open(&path).expect("the fixture maps");
        let base = mapped.bytes().as_ptr() as usize;
        let container = Container::parse(&mapped).expect("the fixture parses");

        for index in 0..container.len() {
            let entry = container.section(index);
            let bytes = container.section_bytes(index);
            assert_eq!(
                bytes.as_ptr() as usize - base,
                entry.offset as usize,
                "{name}: section {index} is not at the offset its table entry declares — \
                 it was copied, or read from somewhere else"
            );
            assert_eq!(
                bytes.len(),
                entry.length as usize,
                "{name}: section {index} is not the length its table entry declares"
            );
            offsets_seen.insert(entry.offset);
        }
    }

    // The anti-uniform-fixture guard. Most committed fixtures carry a single
    // section, and every one of those sits at the same offset — the end of the
    // envelope and its table. Checked against that alone, a wrong base or a
    // wrong stride still lands on the only offset there is, so the walk has to
    // be shown to have seen more than one.
    let distinct: Vec<u64> = offsets_seen.iter().copied().filter(|at| *at > 0).collect();
    assert!(
        distinct.len() >= 2,
        "the walk saw {} distinct non-zero section offset(s) ({distinct:?}); one offset cannot \
         fail a base or a stride bug",
        distinct.len()
    );
}

/// A path that is not there is an ordinary IO error naming it, not a panic.
#[test]
fn mapping_a_missing_file_is_an_io_error() {
    let error =
        MappedFile::open("goldens/dsb/there-is-no-such-fixture.dsb").expect_err("no such file");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// An empty file is refused here, by name, rather than handed to an operating
/// system that answers "invalid argument".
///
/// A zero-length mapping is an error on every platform this runs on, so the
/// choice is only whether the message says which file and why.
#[test]
fn mapping_an_empty_file_names_the_file_and_the_reason() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("empty.dsb");
    std::fs::File::create(&path).expect("an empty file is created");

    let error = MappedFile::open(&path).expect_err("an empty file is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let message = error.to_string();
    assert!(
        message.contains("empty.dsb"),
        "the message names the file: {message}"
    );
    // Derived from the constant rather than restated, so a change to the
    // header's size cannot leave this test asserting the old number.
    let header = format!("{}-byte header", dashbuf::container::HEADER_SIZE);
    assert!(
        message.contains(&header),
        "the message says why, naming {header}: {message}"
    );
}

/// A file written and then mapped reads back as itself, so a mapping is not
/// only a thing the committed fixtures happen to satisfy.
///
/// The fixture is deliberately not a `.dsb`: this is about the mapping, not
/// about the format, and using a real document here would let a format check
/// stand in for the property being tested.
#[test]
fn a_mapped_file_is_the_bytes_that_were_written() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("bytes");
    // Not uniform, and not a repeat: a mapping that returned the first page
    // twice, or a fixed byte, would pass over a uniform fixture.
    let written: Vec<u8> = (0..9973u32).map(|i| (i % 251) as u8).collect();
    std::fs::File::create(&path)
        .expect("the file is created")
        .write_all(&written)
        .expect("the bytes are written");

    let mapped = MappedFile::open(&path).expect("the file maps");
    assert_eq!(mapped.len(), written.len());
    assert_eq!(mapped.bytes(), written.as_slice());
    assert_eq!(
        &mapped[..],
        written.as_slice(),
        "Deref yields the same slice"
    );
}
