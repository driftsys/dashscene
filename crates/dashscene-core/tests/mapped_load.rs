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

/// The prefix flow over a mapping: the envelope, the plan, and one range per
/// asset entry in entry order.
///
/// The same three calls `demo/src/document.rs` makes, kept here rather than
/// reached for so the test fails on the loader rather than on the host.
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
    // The verification step, as the host runs it: `Plan::bind` hashes every
    // payload against the table. Its slices are discarded because the loader
    // takes ranges; the check is the point, and dropping it here would let
    // these tests pass over a load path that hands a painter unhashed bytes.
    let resident: Vec<&[u8]> = plan
        .wanted()
        .iter()
        .map(|want| &file[want.range.start as usize..want.range.end as usize])
        .collect();
    plan.bind(&resident)
        .expect("every payload matches its hash");
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
        let (document, payloads) = dashbuf::open(file).expect("the fixture opens");
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
    let (document, payloads) = dashbuf::open(file).expect("the fixture opens");
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
