//! Loading a document out of a memory mapping (story #595, epic #594).
//!
//! `docs/decisions/dsb-sectioned-container.md` specified the loading model as
//! "one `mmap`, once" from v0.11, and `crates/dashbuf/src/container.rs` has said
//! an mmap "is therefore a drop-in" for just as long. Neither was ever true in
//! fact, because nothing in the workspace mapped anything. These tests are what
//! turns the claim into a measurement.
//!
//! **That model said "of the whole file" until story #1124**, which added
//! `MappedFile::open_range` for a document that does not begin one — an
//! uncompressed entry inside an Android APK. The last four tests here are that
//! case, and the record now covers both.
//!
//! Two things are asserted of the whole-file case, and the second is the one
//! with teeth:
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

// ---------------------------------------------------------------------------
// `open_range` — a document that does not begin its file (story #1124).
//
// An Android APK stores an uncompressed asset as a byte range inside
// `base.apk`, and `AssetManager.openFd` reports that range rather than a path.
// These assert the property that makes mapping such a range usable: the
// document is read relative to the range, and the offset needs no alignment.
// ---------------------------------------------------------------------------

/// Writes `payload` at `offset` inside a file of junk, and returns the path.
///
/// The prefix and suffix are **not** zeros: a reader that ignored the offset
/// and mapped from byte 0 would then meet a plausible-looking run of nulls
/// rather than bytes that cannot be a header. The junk is deterministic so a
/// failure reproduces.
fn document_inside_a_container(
    dir: &std::path::Path,
    name: &str,
    prefix: &[u8],
    payload: &[u8],
) -> PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("the container is created");
    file.write_all(prefix).expect("the prefix is written");
    file.write_all(payload).expect("the payload is written");
    // A suffix too, so the document is not the tail of its container either.
    let suffix: Vec<u8> = (0..977u32).map(|i| (i % 241) as u8 + 1).collect();
    file.write_all(&suffix).expect("the suffix is written");
    path
}

/// `len` bytes of junk. Deterministic so a failure reproduces, and **not**
/// zeros: a reader that ignored the offset would meet a plausible run of nulls
/// rather than bytes that cannot be a header.
fn junk(len: u64) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8 + 1).collect()
}

/// The host's page size, which is **not** the format's `PAGE_ALIGN`.
///
/// That constant is 4096 and is a property of the file. This is a property of
/// whoever maps it, and the two differ on every 16 KiB-page host — this
/// workspace is built on one, and Android 15 requires 16 KB page support on new
/// devices. A test that hard-coded 4096 would assert against the wrong quantum
/// on such a host.
///
/// **Unix only, and the test below with it.** `sysconf` does not exist on
/// Windows, and its answer would be the wrong quantum there anyway: memmap2's
/// Windows backend rounds an offset to `allocation_granularity()`, 64 KiB,
/// rather than to the page size. Covering that target means asking Windows a
/// different question, which no host in scope needs yet.
#[cfg(unix)]
fn host_page_size() -> u64 {
    // SAFETY: `sysconf` with a constant name reads a process-wide value and
    // touches nothing of ours.
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    assert!(value > 0, "sysconf(_SC_PAGESIZE) answered {value}");
    value as u64
}

/// The claim `open_range` exists for: a `.dsb` at an offset inside a larger
/// file loads what the same `.dsb` loads on its own.
///
/// **Every offset here is deliberately not a multiple of the host's page
/// size.** That is the case `MmapOptions::offset` has to handle by mapping from
/// the page below and returning a slice starting at the byte asked for, and it
/// is the case an APK produces — `zipalign` aligns an ordinary stored entry to
/// 4 bytes, not to a page.
#[cfg(unix)]
#[test]
fn a_document_at_an_offset_inside_a_container_loads_as_itself() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let page = host_page_size();

    // Three offsets rather than one, and all three derived from the host's page
    // size rather than written down. A single offset cannot fail a base bug;
    // one below the page size maps from 0 and so exercises no rounding at all;
    // and the third is well past the first page.
    //
    // **No counter guards that**, and an earlier draft had one that could not
    // fail — `page + 1 > page` holds for every page size, so counting offsets
    // above the page size only re-read the array. What carries the claim is the
    // pointer assertion in the loop below, which holds for each offset
    // individually and which a copying or whole-file-slicing implementation
    // fails.
    let offsets = [1u64, page + 1, page * 10 + 1];

    // **Built once, not once per fixture.** The prefix does not depend on the
    // document, and at `page * 10 + 1` on a 16 KiB-page host it is 160 KiB — so
    // generating it inside the fixture loop wrote several megabytes to rebuild
    // the same bytes eleven times.
    let prefixes: Vec<Vec<u8>> = offsets.iter().map(|offset| junk(*offset)).collect();

    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let read = std::fs::read(&path).expect("the fixture reads");
        let (want_document, want_payloads) =
            dashbuf::open_verified(&read).expect("open accepts a golden");

        for (offset, prefix) in offsets.iter().copied().zip(&prefixes) {
            assert_ne!(offset % page, 0, "the offsets must not be page-aligned");
            let container = document_inside_a_container(
                dir.path(),
                &format!("{name}.{offset}.container"),
                prefix,
                &read,
            );

            let mapped = MappedFile::open_range(&container, offset, read.len() as u64)
                .unwrap_or_else(|error| panic!("{name} at {offset} maps: {error}"));

            assert_eq!(
                mapped.bytes(),
                read.as_slice(),
                "{name} at {offset}: the mapped range is not the document"
            );
            assert_eq!(mapped.len(), read.len(), "{name} at {offset}: length");

            // **The aligned-down mapping, asserted directly rather than
            // inferred from the bytes matching.** `MmapOptions::offset` maps
            // from the page at or below the offset and returns a slice starting
            // at the byte asked for, so the slice's address carries the
            // offset's position within a page. Equal bytes would also hold for
            // an implementation that copied, or that mapped the whole file and
            // sliced it; this would not.
            assert_eq!(
                mapped.bytes().as_ptr() as u64 % page,
                offset % page,
                "{name} at {offset}: the mapping does not start at the offset's own \
                 position within a page"
            );

            let (got_document, got_payloads) =
                dashbuf::open_verified(&mapped).expect("open accepts the mapped range");
            assert_eq!(got_payloads, want_payloads, "{name} at {offset}: payloads");
            assert_eq!(
                got_document.nodes().map(|nodes| nodes.len()),
                want_document.nodes().map(|nodes| nodes.len()),
                "{name} at {offset}: node count"
            );
        }
    }
}

/// The whole file is the range `0..len`, so `open_range` subsumes `open`.
#[test]
fn open_range_over_the_whole_file_is_open() {
    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let whole = MappedFile::open(&path).expect("the fixture maps");
        let ranged = MappedFile::open_range(&path, 0, whole.len() as u64)
            .expect("the whole file is a valid range");
        assert_eq!(ranged.bytes(), whole.bytes(), "{name}");
    }
}

/// A zero-length range is refused by name, like an empty file is.
///
/// The container is **not** empty, so this is the range being refused rather
/// than the file — which is why it does not simply reuse `open`'s check.
#[test]
fn a_zero_length_range_is_refused_and_names_the_reason() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path =
        document_inside_a_container(dir.path(), "zero.container", &junk(64), b"not a document");

    let error = MappedFile::open_range(&path, 64, 0).expect_err("a zero-length range is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let message = error.to_string();
    assert!(
        message.contains("zero.container"),
        "the message names the container: {message}"
    );
    // Derived from the constant, so a change to the header's size cannot leave
    // this asserting the old number.
    let header = format!("{}-byte header", dashbuf::container::HEADER_SIZE);
    assert!(
        message.contains(&header),
        "the message says why, naming {header}: {message}"
    );
}

/// A range reaching past the end of the file is refused **here**, naming both
/// ends.
///
/// This is the one check that cannot be left to the operating system: `mmap`
/// past the end of a file succeeds and answers `SIGBUS` when the page is
/// touched, which arrives with nothing naming the range that caused it.
#[test]
fn a_range_past_the_end_of_the_file_is_refused_rather_than_mapped() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let payload = b"eight-and-more bytes of payload";
    let path = document_inside_a_container(dir.path(), "short.container", &junk(100), payload);
    let file_len = std::fs::metadata(&path)
        .expect("the container exists")
        .len();

    // One byte past the end, so this fails for the range and not for being
    // wildly wrong.
    let error = MappedFile::open_range(&path, 0, file_len + 1).expect_err("the range is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let message = error.to_string();
    assert!(
        message.contains("short.container") && message.contains(&file_len.to_string()),
        "the message names the container and its length: {message}"
    );

    // And the same range expressed as an offset rather than a length, which is
    // the shape a wrong entry offset produces.
    let error = MappedFile::open_range(&path, file_len, 1).expect_err("the range is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

    // An offset that overflows when the length is added is refused too, rather
    // than wrapping into a range that happens to be inside the file.
    let error =
        MappedFile::open_range(&path, u64::MAX, 2).expect_err("an overflowing range is refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("overflows"),
        "the message says which check refused it: {error}"
    );

    // The range that does fit still maps, so the three refusals above are not
    // this container being unmappable.
    let ok = MappedFile::open_range(&path, 100, payload.len() as u64).expect("the range fits");
    assert_eq!(ok.bytes(), payload);
}
