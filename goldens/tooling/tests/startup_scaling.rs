//! The startup-scaling criterion — the falsifiable form of R5 (story #598,
//! epic #594, guardrail G-20).
//!
//! R5 says cold-start cost is "proportional to what is shown, not to file size
//! (mmap + section discipline)", and `docs/specification/05-qualification.md`
//! makes this the **first v1 exit criterion**: "A scaling benchmark with a
//! small-root document and a many-frame corpus document asserts that cold-start
//! cost tracks the shown root, not the document size." Nothing had ever
//! measured it.
//!
//! `docs/decisions/startup-scaling-is-measured-by-a-counter.md` settles what is
//! measured and what it is measured over. The short form, with each decision
//! named where this file depends on it:
//!
//! - **D1 — cost is a count of bytes, not an elapsed time.** So there is no
//!   benchmark framework here, and no timing is asserted on. A byte count is
//!   exact and identical on every machine, where a timing ratio needs a
//!   threshold that drifts and cannot run on the two-core CI runners without
//!   flaking.
//! - **D2 — both reads and copies count.**
//!   `dashbuf::residency::BlobResidency::touch_with_cost` records the hash of each
//!   payload a load makes resident; `load_document_bound_with_cost` records a
//!   copy out of one. Each alone makes cold start scale with file size, so a
//!   counter seeing only one cannot falsify the other.
//!   [`each_recording_site_counts_its_own_read_and_no_other`] pins the two
//!   apart; the criterion below cannot, because dropping either recording would
//!   scale both documents equally and leave the ratio unchanged.
//! - **D3 — the boundary is the load path, not the frame.** [`load_cost`] runs
//!   the steps `demo/src/document.rs` runs — map, open, the load gate, the
//!   prefetch, the replay into a committed arena — and stops. Nothing here
//!   selects a painter, so no painter's internal copies reach the number. See
//!   [`load_cost`] for the two steps of that host it deliberately leaves out,
//!   and for the one property the counter cannot see.
//! - **D4 — both documents show the same root, and the assertion is equality.**
//!   The ratio is reported, derived from the two counts.
//! - **D5 — the many-frame document is generated when the benchmark runs**,
//!   from a `dashc_wasm::Document` built in code, with payloads from
//!   `corpus/photo`. Nothing multi-megabyte enters git.
//! - **D6 — wall-clock and the machine are recorded and asserted on nothing.**
//!   [`report`] prints both; no assertion reads either.
//! - **D7 — it was demonstrated failing at the base commit**, not asserted to
//!   fail. The numbers are below.
//!
//! # It was demonstrated failing before it was made to pass
//!
//! That is the point of writing it first, and epic #594's definition of done
//! required the failure to be demonstrated by running it. A benchmark that has
//! only ever been seen passing is the `t2-check-has-no-teeth` shape v0.13 spent
//! an entire tier removing.
//!
//! Measured at the pre-slice load path (story #598's first half, PR #759), on
//! macos aarch64:
//!
//! ```text
//! small-root  (1 frame)    hashed 197 387 B, copied 197 387 B, total 394 774 B
//! many-frame  (65 frames)  hashed 1 935 927 B, copied 1 935 927 B, total 3 871 854 B
//! ratio                    9.81x, against a criterion of 1.00x
//! ```
//!
//! Every payload was read twice — once to hash it and once to copy it — for
//! every entry in the file rather than for the frame being shown. Stories #595
//! (map the file), #596 (bind ranges, copy nothing) and #597 (verify at the
//! touch, prefetch the shown root) removed the three reasons in that order, and
//! this re-run is where the criterion is measured against the load path they
//! left. The number is in the assertion below, and in
//! `docs/technotes/engineering-guardrails.md` under G-19 and G-20.
//!
//! Until this re-run the binary was held out of every tier in a profile of its
//! own, because a knowingly-red test cannot sit in a gate. It is an ordinary
//! `regression` test now, so a regression in R5 fails a build like any other
//! (`docs/decisions/test-tiers.md`).
//!
//! # Seeing the numbers
//!
//! The assertion is an equality, so a passing run says only that it passed. The
//! counts, the ratio, the wall clock and the machine are printed, and both
//! nextest and libtest capture a passing test's stdout — so ask for it:
//!
//! ```text
//! cargo test -p goldens --test startup_scaling -- --nocapture
//! ```
//!
//! CI runs exactly that, in the `render-oracle` job, beside the render oracle
//! and the calibrated budgets and for the same reason: a measured number
//! belongs in the log. The `just scaling` recipe used to carry
//! `--success-output=immediate` for this, and was deleted with the profile;
//! that step is what replaces it.
//!
//! # Where the document itself lives, and why its payloads are all different
//!
//! In [`common::many_root`], since story #836. It was defined here while this
//! was the only criterion stated over it; the per-frame criterion
//! (`per_frame_scaling.rs`) is stated over the same shape and reuses this
//! builder rather than authoring a second many-root document. Nothing about
//! the document changed in the move.
//!
//! That module's own documentation carries the reason every extra frame has to
//! carry a **distinct** payload — `push_asset` deduplicates by content hash, so
//! repeats would make the two documents the same size and this criterion would
//! pass while measuring nothing. It is stated there rather than in both places,
//! because two copies of a reason are two things that have to agree.
//! [`the_many_frame_document_carries_one_payload_per_frame`] is the guard that
//! fails if the payloads ever collapse, and it is what says the move changed
//! nothing.

use std::sync::Arc;
use std::time::Instant;

use dashbuf::cost::LoadCost;
use dashbuf::map::MappedFile;
use dashbuf::residency::BlobResidency;
use dashscene_core::{
    Arena, BoundPayload, MappedPayload, Region, load_document_bound_with_cost, load_document_mapped,
};

mod common;

use common::many_root::{EXTRA_FRAMES, ROOT_PHOTO, corpus_payload, document};

/// What one load of a `.dsb` cost, and how long it took.
struct Measured {
    cost: LoadCost,
    elapsed: std::time::Duration,
    /// Asset payload bytes the file carries, summed over its entries — the
    /// document's own size in assets, which is what R5 says the cost must
    /// *not* track.
    payload_bytes: u64,
}

/// Writes `file` to a temporary path, maps it, and loads it the way the native
/// host does — returning what that cost.
///
/// **It writes the document to a file and maps it, rather than loading the bytes
/// it already holds.** That is D9 of
/// `docs/decisions/verification-moves-from-open-to-touch.md`, and it is what
/// makes this a measurement of what a host really does rather than of a path
/// only this benchmark takes. Until this re-run (#598) it called
/// `open_verified_with_cost` plus `load_document_bound_with_cost` over bytes in
/// memory — the **owning** path, which cannot be bounded by what is shown even
/// in principle, because `load_document` copies every payload into an owned
/// `ImageAsset` and so needs bytes for every entry. Left that way it would have
/// kept reporting the owning path's number no matter what story #597 did.
///
/// The steps are `demo/src/document.rs`'s, in its order:
///
/// 1. `dashbuf::open` — the envelope, every structured section's hash, the
///    flatbuffers verifier, and each asset entry resolved to **where** its
///    payload lies. No payload byte is read.
/// 2. the referential load gate.
/// 3. the prefetch: the assets the shown root's subtree draws, each made
///    resident through `BlobResidency::touch_with_cost`. This is the only thing
///    here that reads a payload, and the counter records it.
/// 4. `load_document_mapped` into a committed arena, binding ranges.
///
/// D3 puts the boundary exactly here — a painter's own copies are not a
/// property of loading.
///
/// Two differences from that host, both deliberate. It does not run the host's
/// derived-payload refusal (the `#640` guard, which compares each resident hash
/// against its entry's): these documents are RAW by construction, so the guard
/// could only ever pass, and it reads no payload byte either way. And it does
/// not select a painter, which is what D3's boundary means.
///
/// **What the counter cannot see, and what does see it.**
/// `load_document_mapped` takes no [`LoadCost`] — by design, since it reads no
/// payload byte — so a regression that made the *replay* copy a payload would
/// not move the number this test asserts on. That property is held by an
/// address rather than a count, in
/// `crates/dashscene-core/tests/mapped_load.rs`: the bytes a painter resolves
/// out of the arena must be pointers into the mapping, at the offset the file
/// declares. A copy has equal bytes, so only the address can tell.
fn load_cost(file: &[u8]) -> Measured {
    let cost = LoadCost::new();

    // Outside the timer: writing the document is this benchmark's own setup, not
    // part of loading one. The mapping is inside it, because a host pays for
    // that.
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("scaling.dsb");
    std::fs::write(&path, file).expect("the generated document writes");

    let started = Instant::now();

    let mapped = Arc::new(MappedFile::open(&path).expect("the generated document maps"));
    let bytes = mapped.bytes();
    let (document, wanted) = dashbuf::open(bytes).expect("the file opens");
    let report = dashscene_validator::validate_document(&document);
    assert!(
        !report.has_errors(),
        "the generated document loads: {report}"
    );

    // The shown root, and nothing else. Both documents carry the same subtree as
    // their first root by construction — `document()` pushes it before any tile
    // frame — so this is the same set of payloads out of either.
    let residency = BlobResidency::new();
    let shown = dashbuf::prefetch::resolve(&document, dashbuf::prefetch::ShownRoot::FIRST)
        .expect("the document has a root");
    for index in dashbuf::prefetch::assets_of_root(&document, shown) {
        let want = &wanted[index as usize];
        let payload = &bytes[want.range.start as usize..want.range.end as usize];
        residency
            .touch_with_cost(want, payload, &cost)
            .expect("the shown root's payload is the one the file names");
    }

    let payloads: Vec<MappedPayload> = wanted
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
        .collect();
    let mut arena = Arena::new();
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(&document, region, &payloads, &mut arena);

    let elapsed = started.elapsed();
    let payload_bytes = wanted
        .iter()
        .map(|want| want.range.end - want.range.start)
        .sum();
    Measured {
        cost,
        elapsed,
        payload_bytes,
    }
}

/// Prints one document's numbers. D6: the wall clock and the machine are
/// recorded here and asserted on nowhere.
fn report(label: &str, measured: &Measured) {
    println!(
        "STARTUP SCALING — {label}: hashed {} B, copied {} B, total {} B \
         (the file's asset payloads are {} B); {:.1} ms on {} {}",
        measured.cost.hashed(),
        measured.cost.copied(),
        measured.cost.total(),
        measured.payload_bytes,
        measured.elapsed.as_secs_f64() * 1000.0,
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
}

/// The criterion. Showing one root must cost the same out of a one-frame
/// document and out of a sixty-five-frame one.
///
/// Two assertions, and they fail differently:
///
/// - The small document must read **at least** the shown root's own payload.
///   Without it, a load path that read nothing at all would satisfy the
///   equality below while making no asset resident, and the criterion would
///   pass by doing nothing. This is the assertion that keeps D3's boundary —
///   "a committed arena with the shown root's assets resident" — honest.
/// - The many-frame document must read **exactly** what the small one reads.
///   This is R5. It failed at 9.81x against the pre-slice load path, which read
///   every payload twice for every entry in the file rather than for the frame
///   being shown. It holds now because the load path reads a payload only when
///   a prefetch makes it resident, and the prefetch is the shown root's assets
///   and nothing else.
#[test]
fn cold_start_tracks_the_shown_root_not_the_document_size() {
    let small = load_cost(&document(0));
    let many = load_cost(&document(EXTRA_FRAMES));

    report("small-root document (1 frame)", &small);
    report(
        &format!("many-frame document ({} frames)", EXTRA_FRAMES + 1),
        &many,
    );
    println!(
        "STARTUP SCALING — ratio {:.2}x (criterion: 1.00x)",
        many.cost.total() as f64 / small.cost.total() as f64
    );

    let root_payload = corpus_payload(ROOT_PHOTO).len() as u64;
    assert!(
        small.cost.total() >= root_payload,
        "loading the small-root document read {} asset payload bytes, fewer than the shown root's \
         own payload ({root_payload} B): the shown root's asset was never made resident, so the \
         equality below would hold without anything being loaded",
        small.cost.total()
    );

    // And the two documents must differ in the quantity R5 says the cost must
    // not track. Without this the criterion is satisfied by making them the
    // same document: `EXTRA_FRAMES = 0` compiles, passes the equality, and
    // passes the per-frame guard too — because that guard expects
    // `EXTRA_FRAMES + 1` payloads and so shrinks with the fixture it guards. A
    // mutation pass found exactly that, which is the uniform-fixture trap
    // reaching the guard rather than only the data.
    assert!(
        many.payload_bytes > 4 * small.payload_bytes,
        "the many-frame document carries {} asset payload bytes against the small one's {}, so \
         the two documents are not meaningfully different sizes and the equality below is \
         vacuous: it would hold for two copies of one document",
        many.payload_bytes,
        small.payload_bytes
    );

    assert_eq!(
        many.cost.total(),
        small.cost.total(),
        "R5 (guardrail G-20): showing the same root cost {} asset payload bytes out of a \
         {}-frame document and {} out of a 1-frame one, a factor of {:.2}. Cold start tracks the \
         document's size rather than the shown root. This held at 1.00x when epic #594 closed, \
         so something has made the load path read payloads the shown root does not draw — check \
         the prefetch set (dashbuf::prefetch) and what touches a payload \
         (dashbuf::residency::BlobResidency::touch), and see \
         docs/decisions/startup-scaling-is-measured-by-a-counter.md",
        many.cost.total(),
        EXTRA_FRAMES + 1,
        small.cost.total(),
        many.cost.total() as f64 / small.cost.total() as f64,
    );
}

/// Each recording site counts its own read and no other: the touch records a
/// hash and no copy, the owning loader records a copy and no hash.
///
/// D2 counts both because each alone makes cold start scale with file size, so
/// a counter seeing only one cannot falsify the other — and the criterion above
/// cannot tell them apart, because dropping either recording scales both
/// documents equally and leaves the ratio unchanged. This is where each site is
/// pinned on its own.
///
/// **Both sites still exist, and which one a load pays is the whole of R5.** The
/// mapped path reads a payload once, at the touch that makes it resident, and
/// copies nothing; the owning path — an embedded document, a browser's fetched
/// buffers — copies every payload it is given, because `load_document` puts the
/// bytes in an owned `ImageAsset` and so needs bytes for every entry. This test
/// asserts both against the same payload, so the difference between the two
/// paths is a number here rather than a claim.
///
/// It is also the finding epic #594 was opened against, stated as a test: the
/// pre-slice path paid **both**, for every entry in the file, so a payload's
/// bytes were read twice before anything was drawn and the first of those reads
/// faulted in every page of a file that was supposed to be mapped.
#[test]
fn each_recording_site_counts_its_own_read_and_no_other() {
    let file = document(0);
    let root_payload = corpus_payload(ROOT_PHOTO).len() as u64;

    // The read side: one touch of the one payload the shown root draws.
    let touching = LoadCost::new();
    let (document, wanted) = dashbuf::open(&file).expect("the file opens");
    let residency = BlobResidency::new();
    let shown = dashbuf::prefetch::resolve(&document, dashbuf::prefetch::ShownRoot::FIRST)
        .expect("the document has a root");
    let prefetch = dashbuf::prefetch::assets_of_root(&document, shown);
    assert_eq!(prefetch.len(), 1, "the small document shows one asset");
    for index in &prefetch {
        let want = &wanted[*index as usize];
        residency
            .touch_with_cost(
                want,
                &file[want.range.start as usize..want.range.end as usize],
                &touching,
            )
            .expect("the payload is the one the file names");
    }
    assert_eq!(
        touching.hashed(),
        root_payload,
        "the touch hashes the whole payload it makes resident"
    );
    assert_eq!(touching.copied(), 0, "a touch copies nothing");

    // The copy side: the owning loader, over the same payload.
    let loading = LoadCost::new();
    let (document, payloads) = dashbuf::open_verified(&file).expect("the file opens");
    let bound: Vec<BoundPayload<'_>> = payloads
        .iter()
        .map(|b| BoundPayload::canonical(b))
        .collect();
    let mut arena = Arena::new();
    load_document_bound_with_cost(&document, &bound, &mut arena, &loading);
    assert_eq!(
        loading.copied(),
        root_payload,
        "the loader copies the whole payload into an owned ImageAsset"
    );
    assert_eq!(loading.hashed(), 0, "the loader hashes nothing");
}

/// The fixture guard: every extra frame must carry a payload of its own.
///
/// `Document::push_asset` deduplicates by content hash. If the tiles ever
/// stopped differing — one tile cut sixty-four times, a synthetic fill, a
/// stride bug that read the same rows — the many-frame document would compile
/// to one entry and the criterion above would pass while measuring nothing.
/// This fails first, and by name.
///
/// It also pins that the shown root's payload is byte-identical in the two
/// documents, which is what makes "the same root" true rather than assumed.
#[test]
fn the_many_frame_document_carries_one_payload_per_frame() {
    let small = document(0);
    let many = document(EXTRA_FRAMES);

    let (_, small_payloads) = dashbuf::open_verified(&small).expect("the small document opens");
    let (_, many_payloads) = dashbuf::open_verified(&many).expect("the many-frame document opens");

    assert_eq!(small_payloads.len(), 1, "the small document is one frame");
    assert_eq!(
        many_payloads.len(),
        EXTRA_FRAMES + 1,
        "the many-frame document must carry one distinct payload per frame: identical bytes \
         collapse to one asset entry, which would make the two documents the same size"
    );

    let mut seen: Vec<&[u8]> = Vec::new();
    for payload in &many_payloads {
        assert!(
            !seen.contains(payload),
            "two of the many-frame document's payloads are the same bytes"
        );
        seen.push(payload);
    }

    assert_eq!(
        small_payloads[0], many_payloads[0],
        "the shown root's payload must be the same bytes in both documents"
    );
}
