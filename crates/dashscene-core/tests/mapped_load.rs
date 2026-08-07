//! Loading a document whose payloads stay in the mapping (story #596, epic
//! #594).
//!
//! Epic #594's definition of done is "no asset payload is copied between the
//! mapping and the painter". Equal bytes cannot see that — a copy has equal
//! bytes — so the assertion here is on the **address**: the bytes a painter
//! resolves out of the arena must be the file's own pages, at the offset the
//! file says the payload lives at.
//!
//! `docs/decisions/assets-borrow-from-the-mapping.md` is the shape being
//! checked: the table's pool is owned or mapped and never both (D1),
//! `ImageEntry` does not change (D2), the `Painter` trait does not change and
//! no boundary-B type gains a lifetime (D3), and the loader builds from ranges
//! rather than slices (D6).

use std::path::PathBuf;
use std::sync::Arc;

use dashbuf::map::MappedFile;
use dashbuf::prefix::{self, Envelope, MIN_PREFIX, PrefixError};
use dashbuf::residency::BlobResidency;
use dashpaint::ImageFormat;
use dashscene_core::{Arena, MappedPayload, Region, load_document, load_document_mapped};

/// The two committed fixtures that carry an asset at all. Every other
/// `goldens/dsb` fixture has no payload, so a mapped load of one could not fail
/// a payload bug.
const WITH_ASSETS: [&str; 2] = ["v03-paint.dsb", "v03-paint-hifi.dsb"];

/// Of those two, the one whose payload is the document's own canonical bytes.
/// `v03-paint-hifi.dsb` carries a derived payload behind a manifest section, so
/// a load that binds it as canonical is binding bytes the document has no name
/// for (issue #640).
const RAW_WITH_ASSETS: [&str; 1] = ["v03-paint.dsb"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../goldens/dsb")
        .join(name)
}

/// The prefix flow over a mapping: the envelope, the plan, one range per asset
/// entry in entry order, and every payload proven before the loader sees it.
///
/// The calls a host makes, kept here rather than reached for so the test fails
/// on the loader rather than on the host. It is the **prefix** host's sequence:
/// since story #597 `demo/src/document.rs` reads through `dashbuf::open`
/// instead, and `demo-web/src/document.rs` is the one that still plans. Either
/// way the loader below is handed ranges, which is what these tests are about.
fn plan_over(file: &[u8]) -> (dashbuf::Document<'_>, Vec<MappedPayload>) {
    let file_len = file.len() as u64;
    let mut need = MIN_PREFIX.min(file.len());
    let mut envelope = None;
    for _ in 0..2 {
        match Envelope::read(&file[..need], file_len) {
            Ok(read) => {
                envelope = Some(read);
                break;
            }
            Err(PrefixError::NeedMore { need: more }) => need = more.min(file.len()),
            Err(PrefixError::Malformed(error)) => panic!("not a .dsb: {error}"),
        }
    }
    let envelope = envelope.expect("the envelope resolves in two rounds");
    let hot = &file[..envelope.hot_len() as usize];
    let plan = prefix::plan(&envelope, hot).expect("the document plans");
    // The verification step, as a host runs it. Since story #597 that is
    // `BlobResidency::touch`, one payload at a time, and **not** `Plan::bind`,
    // which now checks the count and nothing else
    // (`docs/decisions/verification-moves-from-open-to-touch.md` D7). Both run
    // here for the same reason a host runs both: the touch proves each payload,
    // and `bind` is the only thing that can say the number of payloads is the
    // number the document asked for.
    //
    // The check is the point, and dropping it would let these tests pass over a
    // load path that hands a painter unhashed bytes. It has to name the touch to
    // do that: `bind` alone no longer refuses a corrupted payload, and
    // `a_corrupted_payload_is_refused_before_the_loader_sees_it` is what fails if
    // this is ever reduced to it again.
    let residency = BlobResidency::new();
    let resident: Vec<&[u8]> = plan
        .wanted()
        .iter()
        .map(|want| {
            let bytes = &file[want.range.start as usize..want.range.end as usize];
            residency
                .touch(want, bytes)
                .expect("every payload matches its hash")
        })
        .collect();
    plan.bind(&resident).expect("one payload per asset entry");
    let payloads = plan
        .wanted()
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
        .collect();
    // `Plan::document` borrows the hot run, which borrows `file`, so the
    // document outlives this call exactly as long as the mapping does.
    (plan.document(), payloads)
}

/// The property epic #594 exists for: a painter resolving an image out of a
/// mapped load reads the file's own memory.
///
/// Asserted three ways over each fixture, because each catches a different
/// wrong answer:
///
/// - the resolved bytes lie **inside** the region — a copy does not;
/// - their offset from the region's base is the **file offset** the envelope
///   states for that payload — a right-length read from the wrong place does
///   not;
/// - the bytes equal what the file holds there — a right offset over a wrong
///   length does not.
#[test]
fn a_mapped_load_resolves_images_to_the_files_own_pages() {
    for name in WITH_ASSETS {
        let mapped = Arc::new(MappedFile::open(fixture(name)).expect("the fixture maps"));
        let file = mapped.bytes();
        let (document, payloads) = plan_over(file);
        assert!(!payloads.is_empty(), "{name} carries an asset");

        let mut arena = Arena::new();
        let region: Arc<dyn Region> = mapped.clone();
        load_document_mapped(&document, region, &payloads, &mut arena);

        let base = file.as_ptr() as usize;
        let images = arena.committed().images();
        assert_eq!(images.len(), payloads.len(), "{name}: one row per entry");

        for (index, payload) in payloads.iter().enumerate() {
            let resolved = images.resolve(index as u32);
            let at = resolved.bytes.as_ptr() as usize;
            assert!(
                at >= base && at < base + file.len(),
                "{name}: image {index} is not inside the mapping — it was copied"
            );
            assert_eq!(
                at - base,
                payload.range.start as usize,
                "{name}: image {index} is at the wrong file offset"
            );
            assert_eq!(
                resolved.bytes,
                &file[payload.range.start as usize..payload.range.end as usize],
                "{name}: image {index} is not the payload the envelope names"
            );
        }
    }
}

/// The same document loaded both ways produces equal tables.
///
/// This is what says the mapped arm is the same picture and not merely a
/// cheaper one, and it is the test the new `PartialEq` exists to make possible:
/// an owned table and a mapped table hold the same payloads at necessarily
/// different offsets, so a comparison that read `offset` would call them
/// different.
///
/// RAW fixtures only. `v03-paint-hifi.dsb` binds a **derived** payload through
/// its manifest, and neither arm is told which rung it is: the owned arm reads
/// the payload's header, finds a KTX2 where the entry says `Png` and panics by
/// name, and the mapped arm reads no header at all. Comparing them there would
/// be comparing two answers to a question neither was asked — the host states a
/// derivation through `MappedPayload::derived`, which
/// [`a_derived_mapped_payload_is_tagged_with_the_format_the_caller_states`]
/// covers.
#[test]
fn an_owned_load_and_a_mapped_load_agree() {
    for name in RAW_WITH_ASSETS {
        let mapped = Arc::new(MappedFile::open(fixture(name)).expect("the fixture maps"));
        let file = mapped.bytes();

        let mut by_value = Arena::new();
        let (document, payloads) = dashbuf::open_verified(file).expect("the fixture opens");
        load_document(&document, &payloads, &mut by_value);

        let mut by_range = Arena::new();
        let (document, ranges) = plan_over(file);
        let region: Arc<dyn Region> = mapped.clone();
        load_document_mapped(&document, region, &ranges, &mut by_range);

        assert_eq!(
            by_value.committed().images(),
            by_range.committed().images(),
            "{name}: the two arms disagree about the image table"
        );
        assert_eq!(
            by_value.committed().rects().len(),
            by_range.committed().rects().len(),
            "{name}: the two arms disagree about the rect table"
        );
    }
}

/// A table is owned or mapped and never both (D1), and the refusal is by name.
///
/// The loader adopts the region before staging any row, so an arena that
/// already holds a payload is the case that has to be refused — otherwise the
/// staged payloads would be silently dropped when the pool was replaced.
#[test]
#[should_panic(expected = "owned or mapped and never both")]
fn a_mapped_load_into_an_arena_that_already_holds_assets_is_refused() {
    let name = WITH_ASSETS[0];
    let mapped = Arc::new(MappedFile::open(fixture(name)).expect("the fixture maps"));
    let file = mapped.bytes();

    let mut arena = Arena::new();
    let (document, payloads) = dashbuf::open_verified(file).expect("the fixture opens");
    load_document(&document, &payloads, &mut arena);

    let (document, ranges) = plan_over(file);
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(&document, region, &ranges, &mut arena);
}

/// A derived payload is tagged with the format the **caller** states, not the
/// one the entry names.
///
/// This is issue #640's rule on the mapped path. A document records its
/// canonical payload's format and never carries a derivation, so a host binding
/// the rung its profile selected is binding bytes the document has no name for;
/// `MappedPayload::derived` is where the two are stated together, exactly as
/// `BoundPayload::derived` is on the owning path.
///
/// It matters more here than there. The owning path reads an encoded payload's
/// header and panics on a mismatch, so a host that forgot to state a
/// derivation finds out; the mapped path reads no header — that is the whole
/// point of it — so nothing downstream can catch a wrong tag. The format the
/// caller states is the only answer there is.
#[test]
fn a_derived_mapped_payload_is_tagged_with_the_format_the_caller_states() {
    let mapped = Arc::new(MappedFile::open(fixture("v03-paint-hifi.dsb")).expect("it maps"));
    let file = mapped.bytes();
    let (document, canonical) = plan_over(file);
    assert_eq!(canonical.len(), 1, "the fixture carries one asset");

    // The same range, stated as something other than what the entry says. Jpeg
    // rather than a baked rung: a baked format's payload length is fixed by its
    // extent, and this range is a KTX2's, so stating one here would be stating
    // an extent the bytes do not have — which `push_mapped` now refuses, and
    // which `a_baked_mapped_row_whose_range_is_the_wrong_length_is_refused`
    // covers. What is being checked here is only which format wins.
    let range = canonical[0].range.clone();
    let stated = ImageFormat::Jpeg;
    assert_ne!(
        stated,
        ImageFormat::Png,
        "the stated format must differ from the entry's, or this proves nothing"
    );

    let mut arena = Arena::new();
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(
        &document,
        region,
        &[MappedPayload::derived(range.clone(), stated)],
        &mut arena,
    );

    let resolved = arena.committed().images().resolve(0);
    assert_eq!(
        resolved.format, stated,
        "the row carries the format the caller stated, not the entry's"
    );
    assert_eq!(
        resolved.bytes,
        &file[range.start as usize..range.end as usize],
        "and it still names the same bytes"
    );
}

/// A baked payload whose range is not the length its extent implies is refused
/// by name.
///
/// `ImageTable::push_baked` has made this check since issue #716 — "until this
/// assertion a baked binding could state any extent at all beside any bytes at
/// all" — and the mapped path needs it more, not less: it reads no header at
/// all, so nothing downstream could notice. The check is arithmetic over
/// numbers already in hand, so it costs no page fault.
#[test]
#[should_panic(expected = "describe different images")]
fn a_baked_mapped_row_whose_range_is_the_wrong_length_is_refused() {
    let mapped = Arc::new(MappedFile::open(fixture("v03-paint-hifi.dsb")).expect("it maps"));
    let file = mapped.bytes();
    let (document, canonical) = plan_over(file);

    // A baked rung at the entry's extent needs a specific number of bytes, and
    // the KTX2 range this fixture carries is not it.
    let range = canonical[0].range.clone();
    let mut arena = Arena::new();
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(
        &document,
        region,
        &[MappedPayload::derived(range, ImageFormat::Astc4x4Unorm)],
        &mut arena,
    );
}

/// A payload that does not hash to what the file records never reaches the
/// loader.
///
/// The teeth behind `plan_over`'s verification step. Story #597 moved that check
/// out of `Plan::bind` and into `BlobResidency::touch`, and `bind` no longer refuses
/// a corrupted payload — so without this test, reducing `plan_over` back to
/// `bind` alone would leave every test in this file passing while the mapped
/// load path hashed nothing at all. It is the assertion that says the helper's
/// comment is true rather than aspirational.
///
/// The section table is left untouched, so the file still records what the
/// payload should hash to and nothing before the touch can notice: the root hash
/// covers the table, not the file.
#[test]
fn a_corrupted_payload_is_refused_before_the_loader_sees_it() {
    let file = std::fs::read(fixture("v03-paint.dsb")).expect("the fixture reads");
    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan =
        prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("the document plans");

    let want = &plan.wanted()[0];
    let mut corrupted = file[want.range.start as usize..want.range.end as usize].to_vec();
    corrupted[0] ^= 0xFF;

    let residency = BlobResidency::new();
    let error = residency
        .touch(want, &corrupted)
        .expect_err("a corrupted payload is refused");
    assert_eq!(error.section, want.section);
    assert!(
        !residency.is_ready(want.section),
        "a refused payload is not resident"
    );

    // And the check `plan_over` used to rely on does not catch it, which is why
    // the touch above has to be the one that does.
    plan.bind(&[&corrupted])
        .expect("bind counts payloads and no longer hashes them");
}

/// And the helper every test above runs through is the thing that refuses it.
///
/// [`a_corrupted_payload_is_refused_before_the_loader_sees_it`] proves
/// `BlobResidency::touch` refuses a bad payload; it does not prove `plan_over` calls
/// it. This does, and it is what fails if the touch is ever taken back out of
/// the helper — which a mutation pass showed nothing else in this file catches,
/// because every committed fixture's payloads are valid, so a helper that
/// verified nothing would load them all exactly as well.
#[test]
#[should_panic(expected = "every payload matches its hash")]
fn the_helper_refuses_a_corrupted_payload() {
    let mut file = std::fs::read(fixture("v03-paint.dsb")).expect("the fixture reads");
    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let blob = envelope
        .sections()
        .iter()
        .find(|entry| entry.kind == dashbuf::container::SectionKind::Blob as u16)
        .copied()
        .expect("v03-paint.dsb carries a payload");
    file[blob.offset as usize] ^= 0xFF;

    plan_over(&file);
}
