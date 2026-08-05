//! The load path's byte counter — the instrument the startup-scaling criterion
//! is measured with (`docs/decisions/startup-scaling-is-measured-by-a-counter.md`).
//!
//! R5 says cold-start cost is proportional to what is shown, not to file size,
//! and `docs/specification/05-qualification.md` makes a scaling benchmark the
//! first v1 exit criterion under guardrail G-20. D1 of that decision record
//! settles what "cost" is: **a count of asset payload bytes the load path
//! reads**, not an elapsed time. A byte count is exact, identical on every
//! machine, and either right or wrong with no tolerance to argue about, where a
//! timing ratio needs a threshold that drifts and cannot run on the two-core CI
//! runners without flaking. It is the same instrument this repository already
//! applies to costs with no visible symptom — `dashscene_gpu::Residency`'s
//! decode count and `Renderer::allocations`.
//!
//! # What it counts, and what it does not
//!
//! D2 counts a payload's bytes whether they are read to **hash** them or read
//! to **copy** them. Both happen to every payload today, and each alone is
//! enough to make cold start scale with file size, so a counter seeing only one
//! of them cannot falsify the other. That puts the two recording sites in two
//! crates: [`crate::open_with_cost`] records the hash of each asset payload it
//! resolves, and `dashscene_core::load_document_bound_with_cost` records the
//! loader's copy out of it.
//!
//! Three reads are deliberately outside it:
//!
//! - **The hot sections.** `Container::ui_document` is hashed on every open,
//!   and the derivation manifest is hashed whenever the file carries one — a
//!   RAW file carries none, which is the ordinary case today. Neither is an
//!   asset payload, and their size is a property of the document rather than of
//!   its assets.
//! - **`dashpaint`'s pool copy.** `ImageTable::push_row` copies the bytes a
//!   second time, into the table's own pool. `dashpaint` is boundary B and
//!   depends on nothing at all; instrumenting it would mean giving the
//!   dependency-free crate a dependency to carry a measurement. It is left out
//!   on the arithmetic: it copies exactly the payloads the loader already
//!   copied, so it scales both documents by the same factor and the criterion —
//!   an equality between two documents — is unaffected. Story #596 removes it
//!   along with the loader's copy (`docs/decisions/assets-borrow-from-the-mapping.md`).
//! - **What a painter does afterwards.** D3 bounds the measurement at a
//!   committed arena, so the number is a property of loading rather than of
//!   whichever painter is selected.

use std::sync::atomic::{AtomicU64, Ordering};

/// Asset payload bytes one load read, split by why they were read.
///
/// Passed into [`crate::open_with_cost`] and
/// `dashscene_core::load_document_bound_with_cost`, which is why it counts
/// through `&self` rather than `&mut self`: one load crosses two crates and
/// several calls, and threading a `&mut` through all of them would put the
/// instrument into signatures that have nothing to do with it.
///
/// # Reading the counts
///
/// The counters are [`Ordering::Relaxed`], which orders nothing. That is
/// correct for a load that records and is then read on the same thread, and it
/// stays correct when story #597 moves hashing onto a loader thread **provided
/// the reader synchronises with that thread first** — joining it, or whatever
/// marks its payload ready. Relaxed guarantees every increment lands; it does
/// not guarantee a reader elsewhere sees them without such an edge.
#[derive(Debug, Default)]
pub struct LoadCost {
    hashed: AtomicU64,
    copied: AtomicU64,
}

impl LoadCost {
    /// A counter at zero.
    pub const fn new() -> Self {
        Self {
            hashed: AtomicU64::new(0),
            copied: AtomicU64::new(0),
        }
    }

    /// Records `bytes` read in order to hash a payload.
    pub fn record_hashed(&self, bytes: u64) {
        self.hashed.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Records `bytes` read in order to copy a payload out of the file.
    pub fn record_copied(&self, bytes: u64) {
        self.copied.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Asset payload bytes read to hash them.
    pub fn hashed(&self) -> u64 {
        self.hashed.load(Ordering::Relaxed)
    }

    /// Asset payload bytes read to copy them out of the file.
    pub fn copied(&self) -> u64 {
        self.copied.load(Ordering::Relaxed)
    }

    /// Asset payload bytes read, for whichever reason — the criterion's number.
    ///
    /// A payload read twice counts twice, which is the point: reading it to
    /// hash it and reading it again to copy it are two faults of the same
    /// pages, and R5 is a claim about what cold start touches.
    pub fn total(&self) -> u64 {
        self.hashed() + self.copied()
    }
}

#[cfg(test)]
mod tests {
    use super::LoadCost;

    /// The two counters are separate, and `total` is their sum — asserted with
    /// two different values, since equal ones would pass with the fields
    /// swapped or with one increment feeding both.
    #[test]
    fn the_two_counters_are_independent_and_total_is_their_sum() {
        let cost = LoadCost::new();
        cost.record_hashed(7);
        cost.record_copied(11);

        assert_eq!(cost.hashed(), 7);
        assert_eq!(cost.copied(), 11);
        assert_eq!(cost.total(), 18);
    }

    /// Every record accumulates rather than replacing. One payload read twice
    /// is two reads, which is what the criterion is about.
    #[test]
    fn records_accumulate() {
        let cost = LoadCost::new();
        cost.record_hashed(3);
        cost.record_hashed(5);

        assert_eq!(cost.hashed(), 8);
        assert_eq!(cost.copied(), 0);
    }
}
