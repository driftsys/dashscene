//! Loading a document through the prefix reader (story #587): the two-phase
//! counterpart of [`dashbuf::open_verified`], for a host that fetches byte
//! ranges rather than holding the file.
//!
//! `dashbuf::open_verified` does three things at once over a full slice —
//! envelope, flatbuffers verify, and asset binding. A host reading a prefix can
//! do the first two from the hot run and cannot do the third, because the
//! payloads are exactly the part it has not fetched. So the binding splits in
//! half around the fetch, and the rules stay in this crate rather than being
//! restated by every host.
//!
//! Since story #597 the proving half of that binding is
//! [`dashbuf::residency::BlobResidency::touch`] rather than `Plan::bind`, and it is
//! the same call the native host makes over its mapping — so the tests below
//! that used to name `bind` name the touch instead.
//!
//! The load-bearing test is [`the_prefix_flow_loads_what_open_loads`]: the two
//! paths must agree, on every committed fixture, or one of them is wrong.

use std::path::PathBuf;

use dashbuf::container::{ContainerError, FLAVOR_UI, HASH_LEN, SectionKind};
use dashbuf::prefix::{BindError, Envelope, MIN_PREFIX, PrefixError};
use dashbuf::residency::{BlobResidency, PayloadMismatch};
use dashbuf::{Document, prefix};

/// Every committed `.dsb` golden, read from the tree rather than listed here.
///
/// A restated list goes stale as fixtures are added, and it goes stale while
/// still passing — the suite would simply stop covering the new file.
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

/// Drives the whole prefix flow against `file`, serving every fetch from the
/// bytes in hand — which is what a network host does, minus the network.
///
/// Each step takes only what the previous step said it needed, so a step that
/// reached outside its range would read zeroes rather than the right answer by
/// accident.
fn load_by_prefix(file: &[u8]) -> (Document<'_>, Vec<&[u8]>) {
    let file_len = file.len() as u64;

    // Round one: the fixed header, which states how long the table is.
    let need = match Envelope::read(&file[..MIN_PREFIX], file_len) {
        Err(PrefixError::NeedMore { need }) => need,
        other => panic!("the header should ask for the table, got {other:?}"),
    };

    // Round two: the envelope.
    let envelope = Envelope::read(&file[..need], file_len).expect("the envelope reads");

    // Round three: the hot run, one contiguous range.
    let hot = &file[..envelope.hot_len() as usize];
    let plan = prefix::plan(&envelope, hot).expect("the document plans");

    // Round four onwards: each payload the document names, on its own, proven
    // as it arrives — which is what `demo-web` does with a fetched range.
    let residency = BlobResidency::new();
    let fetched: Vec<&[u8]> = plan
        .wanted()
        .iter()
        .map(|want| {
            let bytes = &file[want.range.start as usize..want.range.end as usize];
            residency.touch(want, bytes).expect("the payload is proven")
        })
        .collect();

    let payloads = plan.bind(&fetched).expect("the payloads bind");
    (plan.document(), payloads)
}

/// The guard: for every committed fixture, the prefix flow and
/// `dashbuf::open_verified` must produce the same document and the same
/// payloads.
///
/// Anything the two-phase split gets wrong — the wrong ui section, an unresolved
/// manifest row, payloads in the wrong order — shows up here as a disagreement
/// with the reader that has been right since v0.11.
#[test]
fn the_prefix_flow_loads_what_open_loads() {
    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let file = std::fs::read(&path).expect("the fixture reads");

        let (want_document, want_payloads) =
            dashbuf::open_verified(&file).expect("open accepts a golden");
        let (got_document, got_payloads) = load_by_prefix(&file);

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
        assert_eq!(
            got_document.paints().map(|paints| paints.len()),
            want_document.paints().map(|paints| paints.len()),
            "{name}: paint count"
        );
    }
}

/// A plan asks for exactly the payloads the document names, and no more. The
/// hot run already holds every structured section, so a plan that asked for one
/// of those would be asking a host to fetch what it has.
#[test]
fn a_plan_wants_one_range_per_asset_entry() {
    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let file = std::fs::read(&path).expect("the fixture reads");
        let file_len = file.len() as u64;

        let envelope = Envelope::read(&file, file_len).expect("the envelope reads");
        let hot = &file[..envelope.hot_len() as usize];
        let plan = prefix::plan(&envelope, hot).expect("the document plans");

        let entries = plan.document().assets().map_or(0, |assets| assets.len());
        assert_eq!(plan.wanted().len(), entries, "{name}");

        for want in plan.wanted() {
            assert!(
                want.range.start >= envelope.hot_len(),
                "{name}: a wanted range lies inside the hot run the host already has"
            );
            assert!(
                want.range.end <= file_len,
                "{name}: past the end of the file"
            );
        }
    }
}

/// The two `v03-paint` fixtures are the only committed ones carrying a payload,
/// so they are the only ones where the blob half of the flow does anything.
/// Without this, every assertion above would be satisfied by a plan that never
/// wants anything.
#[test]
fn the_image_fixture_wants_its_payload() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan = prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("plans");

    assert_eq!(plan.wanted().len(), 1);
    let want = &plan.wanted()[0];
    assert!(
        want.range.end - want.range.start > 0,
        "the payload is not empty"
    );
    assert_eq!(
        *blake3::hash(&file[want.range.start as usize..want.range.end as usize]).as_bytes(),
        want.hash,
        "the plan names the hash the payload actually has"
    );
}

/// A payload that does not hash to what the table records is refused at the
/// touch. This is the check `Container::blob_by_hash` runs on the caller's
/// behalf and that a prefix host cannot run for itself, because it never held
/// the bytes until now.
///
/// It used to be `Plan::bind`'s check and it is `BlobResidency::touch`'s since story
/// #597. The property is the same one and this is the same assertion; only the
/// call that makes it moved. **`bind` must not still be refusing it** — the
/// second assertion below is what says the check moved rather than being
/// duplicated, and it is what fails if the hash is ever put back into `bind`.
#[test]
fn a_payload_that_does_not_match_its_hash_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan = prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("plans");

    let want = &plan.wanted()[0];
    let mut payload = file[want.range.start as usize..want.range.end as usize].to_vec();
    payload[0] ^= 0xFF;

    let residency = BlobResidency::new();
    assert_eq!(
        residency
            .touch(want, &payload)
            .expect_err("a corrupted payload is refused"),
        PayloadMismatch {
            section: want.section
        }
    );
    assert!(
        !residency.is_ready(want.section),
        "a refused payload is not resident"
    );

    plan.bind(&[&payload])
        .expect("bind counts payloads and no longer hashes them");
}

/// A truncated payload is refused — by its hash, which is the only thing that
/// can refuse it.
///
/// Named for what it proves rather than for a length check: a first draft
/// carried one beside the hash, and a mutation pass showed it was unfalsifiable,
/// because a payload of a different length hashes differently. This test caught
/// nothing the previous one did not, and now says so.
#[test]
fn a_truncated_payload_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan = prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("plans");

    let want = &plan.wanted()[0];
    let short = &file[want.range.start as usize..want.range.end as usize - 1];

    BlobResidency::new()
        .touch(want, short)
        .expect_err("a short payload is refused");
}

/// Handing back a different number of payloads than were asked for is a host
/// bug, and binding them by position would silently pair the wrong payload with
/// the wrong entry.
#[test]
fn binding_the_wrong_number_of_payloads_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan = prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("plans");

    assert_eq!(
        plan.bind(&[]).expect_err("too few is refused"),
        BindError::Count {
            wanted: 1,
            given: 0
        }
    );

    let payload = [0u8; HASH_LEN];
    assert_eq!(
        plan.bind(&[&payload, &payload])
            .expect_err("too many is refused"),
        BindError::Count {
            wanted: 1,
            given: 2
        }
    );
}

/// A ui section that does not match its recorded hash is refused.
///
/// The hot run arrives over a network, so its bytes are exactly as trustworthy
/// as the transport — which is what the content hashes are in the file for.
/// `dashbuf::open` verifies the ui section before parsing it, and a prefix load
/// has to do the same or the flatbuffers verifier becomes the only thing
/// standing between a corrupted transfer and the arena.
///
/// Added because a mutation pass removed that verification and every other test
/// still passed.
#[test]
fn a_corrupted_ui_section_in_the_hot_run_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let ui = envelope
        .sections()
        .iter()
        .find(|entry| entry.flavor == FLAVOR_UI && entry.kind == SectionKind::Structured as u16)
        .expect("a .dsb carries a ui section");

    let mut hot = file[..envelope.hot_len() as usize].to_vec();
    // Inside the ui section's own payload, not in the envelope: the envelope is
    // covered by the root hash and would be refused a step earlier, which would
    // make this test pass without the section check ever running.
    hot[ui.offset as usize] ^= 0xFF;

    // Named, not merely `expect_err`. A first draft asserted only that some
    // error came back, and it passed with the hash check removed — because the
    // flatbuffers verifier then rejected the same bytes for its own reason. An
    // unnamed error is not evidence that the check under test ran.
    let error = prefix::plan(&envelope, &hot).expect_err("a corrupted ui section is refused");
    assert!(
        matches!(
            error,
            dashbuf::OpenError::Container(ContainerError::SectionHashMismatch { .. })
        ),
        "the content hash must be what refuses it, got {error:?}"
    );
}

/// A hot run that does not carry the ui section cannot plan. A host that
/// fetched too little must be told so, rather than reading whatever happens to
/// sit at the ui section's offset.
#[test]
fn a_hot_run_short_of_the_ui_section_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");
    let file = std::fs::read(&path).expect("the fixture reads");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let truncated = &file[..envelope.hot_len() as usize - 1];

    prefix::plan(&envelope, truncated).expect_err("a short hot run is refused");
}
