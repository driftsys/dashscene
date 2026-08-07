//! Touch + hash + mark ready — the one step that turns a payload's [`Wanted`]
//! into bytes anything may read.
//!
//! Specified by `docs/decisions/verification-moves-from-open-to-touch.md` D3,
//! D7 and D8, and by the loading model
//! `docs/decisions/dsb-sectioned-container.md` states: "Blob sections are
//! untouched until the loader thread prefetches them (touch + hash + mark
//! ready)."
//!
//! # Why the hash moved here
//!
//! Both readers used to hash every payload before handing it over —
//! [`crate::open_verified`] through `Container::blob_by_hash`, and
//! [`crate::prefix::Plan::bind`] against the section table — so a load read
//! every payload the document named whether or not anything was going to draw
//! it. Under a mapping that faults in every page holding one, which is the cost
//! R5 says cold start must not pay.
//!
//! The rule that made them do it is not weakened: **a painter must never
//! receive bytes that have not been hashed.** What changes is when. A payload
//! is proven at the moment it is made resident, by this one call, so there is
//! one place a payload is proven rather than two that could disagree.
//!
//! # It takes the bytes rather than holding the region
//!
//! D3 describes a `BlobResidency` that holds the region and slices it. The browser
//! host has no region to hold: a payload there is its own HTTP range response,
//! in its own buffer, and D7 says that host "fetches a range and touches it".
//! One method taking `(want, bytes)` therefore serves both hosts identically,
//! where a region-holding one would need a second entry point for the host that
//! has no region — and two entry points is the shape D7 exists to remove. The
//! proof is the hash either way: bytes that do not hash to what the section
//! table records are refused whatever slice they came out of. Story #599
//! records this against D3.

use std::collections::HashSet;
use std::fmt;
use std::sync::Mutex;

use crate::Wanted;
use crate::cost::LoadCost;

/// A payload's bytes are not the payload the file names.
///
/// Its own type rather than a variant on an existing error, because it is the
/// only way [`BlobResidency::touch`] can fail: an error a function cannot return
/// does not belong in its error type, which is the reasoning
/// [`crate::prefix::BindError`] already records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadMismatch {
    /// The blob section the bytes were offered for, as [`Wanted::section`].
    pub section: usize,
}

impl fmt::Display for PayloadMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the payload offered for section {} does not match its recorded content hash",
            self.section
        )
    }
}

impl std::error::Error for PayloadMismatch {}

/// Which of a file's payloads have been proven, and the only way to prove one.
///
/// `Send + Sync`, so a loader thread can make payloads resident while another
/// thread holds the arena. No such thread is built this slice — the demo
/// builds its scene before the frame loop starts, so the faults are already off
/// the frame thread — but the arena holds its image table across threads
/// already (`docs/decisions/assets-borrow-from-the-mapping.md` D4), and a
/// thread is the next step rather than a different design.
///
/// Readiness is per blob section, which is what the format's own packing rule
/// is stated over: "two small blobs sharing a page is harmless because
/// verification and readiness are per-blob, and a shared page faulting early is
/// free prefetch."
#[derive(Debug, Default)]
pub struct BlobResidency {
    ready: Mutex<HashSet<usize>>,
}

impl BlobResidency {
    /// A residency in which nothing is ready yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Proves `bytes` are the payload `want` names, records the blob as ready,
    /// and hands the bytes back.
    ///
    /// `bytes` is what the host read for [`Wanted::range`] — a slice of the
    /// mapping on a native host, a range response's buffer in a browser. It is
    /// hashed and compared with [`Wanted::hash`], which is the section table's
    /// own record of the content, so bytes that are not that payload are
    /// refused whatever they are and wherever they came from. A payload of a
    /// different length hashes differently, so there is no length check beside
    /// the hash that could fail on its own.
    ///
    /// **Every touch hashes, including a touch of a blob already ready.** D3
    /// specified the opposite — a second touch returning immediately — and that
    /// is sound only for a residency that holds the region, because such a
    /// residency can return the bytes it *proved*. This one is handed the bytes
    /// (see the module doc), so skipping the hash would mean returning bytes
    /// nothing checked. A `Wanted` list is one per asset entry and is not
    /// deduplicated, so two entries naming one payload really do touch one blob
    /// twice, and in the browser host those are two separate range responses
    /// that need not carry the same bytes.
    ///
    /// It costs a second BLAKE3 pass over a payload some document names twice.
    /// It does not cost a second page fault, which is what R5 is about: the
    /// caller has already read the bytes by the time it calls this. Story #599
    /// records this against D3.
    pub fn touch<'b>(&self, want: &Wanted, bytes: &'b [u8]) -> Result<&'b [u8], PayloadMismatch> {
        self.touch_with_cost(want, bytes, &LoadCost::new())
    }

    /// [`BlobResidency::touch`], recording into `cost` the payload bytes it read.
    ///
    /// The startup-scaling criterion's one recording site on the read side
    /// (`docs/decisions/verification-moves-from-open-to-touch.md` D8): what it
    /// counts is what was made resident, rather than what a reader resolved.
    /// A payload resolved and never touched costs nothing and is counted as
    /// nothing, which is the claim R5 makes.
    ///
    /// A blob touched twice is counted twice, because it was read twice —
    /// which is what [`LoadCost::total`] means by "a payload read twice counts
    /// twice".
    pub fn touch_with_cost<'b>(
        &self,
        want: &Wanted,
        bytes: &'b [u8],
        cost: &LoadCost,
    ) -> Result<&'b [u8], PayloadMismatch> {
        if *blake3::hash(bytes).as_bytes() != want.hash {
            return Err(PayloadMismatch {
                section: want.section,
            });
        }
        cost.record_hashed(bytes.len() as u64);
        self.ready
            .lock()
            .expect("the ready set is never held across a panic")
            .insert(want.section);
        Ok(bytes)
    }

    /// Whether blob `section` has been proven.
    pub fn is_ready(&self, section: usize) -> bool {
        self.ready
            .lock()
            .expect("the ready set is never held across a panic")
            .contains(&section)
    }

    /// How many blobs have been proven — what a caller asserts a prefetch
    /// against.
    pub fn ready_count(&self) -> usize {
        self.ready
            .lock()
            .expect("the ready set is never held across a panic")
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::{BlobResidency, Wanted};
    use crate::cost::LoadCost;

    /// A `Wanted` for `payload`, as a section table would record it.
    fn want(section: usize, payload: &[u8]) -> Wanted {
        Wanted {
            section,
            range: 0..payload.len() as u64,
            hash: *blake3::hash(payload).as_bytes(),
        }
    }

    /// The payload the table names is proven, counted, and marked ready.
    ///
    /// The count is asserted against the payload's own length rather than
    /// against zero-or-not, so a recording site that counted the range's length
    /// or a fixed number would fail here.
    #[test]
    fn touching_a_payload_hashes_it_counts_it_and_marks_it_ready() {
        let payload = b"the shown root's picture";
        let want = want(3, payload);
        let residency = BlobResidency::new();
        let cost = LoadCost::new();

        assert!(!residency.is_ready(3), "nothing is ready before a touch");
        let got = residency
            .touch_with_cost(&want, payload, &cost)
            .expect("the payload is the one the table names");

        assert_eq!(got, payload);
        assert!(residency.is_ready(3));
        assert_eq!(residency.ready_count(), 1);
        assert_eq!(cost.hashed(), payload.len() as u64);
        assert_eq!(cost.copied(), 0, "a touch copies nothing");
    }

    /// Bytes that are not the payload are refused by section, and the blob stays
    /// unready.
    ///
    /// The two are asserted together on purpose: a `touch` that refused the
    /// bytes but marked the blob ready anyway would let the next touch of the
    /// same section return them through the ready fast path.
    #[test]
    fn bytes_that_are_not_the_payload_are_refused_and_leave_the_blob_unready() {
        let want = want(3, b"the shown root's picture");
        let residency = BlobResidency::new();

        let error = residency
            .touch(&want, b"the shown root's picturE")
            .expect_err("one bit is a different payload");

        assert_eq!(error.section, 3);
        assert!(!residency.is_ready(3));
        assert_eq!(residency.ready_count(), 0);
    }

    /// A second touch of a ready blob is proved again, not waved through.
    ///
    /// The property that matters is the **second** assertion: offering wrong
    /// bytes for a section that is already ready is refused. A residency that
    /// returned early on `is_ready` would hand those bytes straight back, and in
    /// the browser host the two touches of one blob are two separate range
    /// responses that need not carry the same bytes.
    ///
    /// The count is asserted first and for the same reason: it is the observable
    /// difference between hashing again and not, and it says the second touch
    /// really did read the payload rather than being skipped.
    #[test]
    fn a_second_touch_of_a_ready_blob_is_proved_again() {
        let payload = b"the shown root's picture";
        let want = want(3, payload);
        let residency = BlobResidency::new();
        let cost = LoadCost::new();

        residency
            .touch_with_cost(&want, payload, &cost)
            .expect("the first touch proves it");
        residency
            .touch_with_cost(&want, payload, &cost)
            .expect("the second touch proves it again");

        assert_eq!(
            cost.hashed(),
            2 * payload.len() as u64,
            "a blob read twice is counted twice"
        );
        assert_eq!(residency.ready_count(), 1, "readiness is still per blob");

        let error = residency
            .touch(&want, b"different bytes for a ready blob")
            .expect_err("a ready blob does not make later bytes trustworthy");
        assert_eq!(error.section, 3);
    }

    /// Readiness is per blob: proving one section says nothing about another.
    ///
    /// Two distinct sections rather than one, because a residency that marked
    /// everything ready on the first touch would pass every test above.
    #[test]
    fn readiness_is_per_blob_section() {
        let shown = want(3, b"the shown root's picture");
        let cold = want(9, b"a frame nobody is looking at");
        let residency = BlobResidency::new();

        residency
            .touch(&shown, b"the shown root's picture")
            .expect("proven");

        assert!(residency.is_ready(3));
        assert!(!residency.is_ready(9), "touching one blob readies one blob");
        assert_eq!(residency.ready_count(), 1);
        assert_eq!(cold.section, 9);
    }
}
