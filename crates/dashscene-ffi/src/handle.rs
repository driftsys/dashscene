//! The bit layout of a [`crate::DsRuntime`] handle.
//!
//! A handle is a `u64` carrying three fields, and it is **not an address**:
//!
//! ```text
//!  63          44 43        32 31                          0
//! +--------------+------------+----------------------------+
//! | thread  (20) | index (12) |     generation (32)        |
//! +--------------+------------+----------------------------+
//! ```
//!
//! - **thread** — drawn once per thread from a process-wide counter and never
//!   recycled. This field, and only this field, is what makes a handle value
//!   unique for the life of the process
//!   (`docs/decisions/the-c-abi-runtime-handle-is-generational.md`, decision
//!   2). A per-thread slot index with a per-thread generation — the shape that
//!   record's rationale section pushes an implementer toward — gives two
//!   threads the same first handle, which is the defect the ruling forbids.
//! - **index** — the slot in *that thread's* table. Bounded, so a table that
//!   fills refuses rather than growing without limit.
//! - **generation** — bumped every time a slot is freed, so a stale handle
//!   resolves to nothing rather than to whatever now occupies its slot.
//!
//! Zero is unrepresentable as a live handle: `thread` and `generation` both
//! start at 1. That is what lets `0` mean "no runtime" in C without colliding
//! with a real one.
//!
//! Widths are this crate's answer to the record's open question 1. The thread
//! field is the wide one because an Android host creates a render thread per
//! surface lifecycle, so that is the counter that grows without bound; live
//! runtimes per thread do not.

/// Bits naming the thread that minted the handle.
pub(crate) const THREAD_BITS: u32 = 20;
/// Bits naming the slot within that thread's table.
pub(crate) const INDEX_BITS: u32 = 12;
/// Bits distinguishing successive occupants of one slot.
pub(crate) const GENERATION_BITS: u32 = 32;

/// The largest thread number a handle can carry. Thread numbers start at 1.
pub(crate) const MAX_THREAD: u32 = (1 << THREAD_BITS) - 1;
/// The largest slot index a handle can carry.
pub(crate) const MAX_INDEX: u16 = ((1u32 << INDEX_BITS) - 1) as u16;
/// The largest generation a slot can reach before it is retired.
pub(crate) const MAX_GENERATION: u32 = u32::MAX;

/// Packs the three fields into a handle value.
pub(crate) fn pack(thread: u32, index: u16, generation: u32) -> u64 {
    debug_assert!(
        (1..=MAX_THREAD).contains(&thread),
        "thread {thread} out of range"
    );
    debug_assert!(index <= MAX_INDEX, "index {index} out of range");
    debug_assert!(generation >= 1, "generation must start at 1");
    (u64::from(thread) << (INDEX_BITS + GENERATION_BITS))
        | (u64::from(index) << GENERATION_BITS)
        | u64::from(generation)
}

/// Splits a handle value back into `(thread, index, generation)`.
pub(crate) fn unpack(handle: u64) -> (u32, u16, u32) {
    let thread = (handle >> (INDEX_BITS + GENERATION_BITS)) as u32;
    let index = ((handle >> GENERATION_BITS) & u64::from(MAX_INDEX)) as u16;
    let generation = (handle & u64::from(MAX_GENERATION)) as u32;
    (thread, index, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_round_trips_at_every_field_boundary() {
        for (thread, index, generation) in [
            (1u32, 0u16, 1u32),
            (MAX_THREAD, MAX_INDEX, MAX_GENERATION),
            (1, MAX_INDEX, 1),
            (MAX_THREAD, 0, MAX_GENERATION),
            (0x5_AAAA, 0xC3, 0x1234_5678),
        ] {
            assert_eq!(
                unpack(pack(thread, index, generation)),
                (thread, index, generation),
                "({thread}, {index}, {generation}) must survive the round trip",
            );
        }
    }

    #[test]
    fn no_field_bleeds_into_a_neighbour() {
        // Each field at its maximum with the others at their minimum. A shift
        // or mask that is one bit wrong shows up here as a neighbour that is
        // no longer at its minimum.
        let (t, i, g) = unpack(pack(MAX_THREAD, 0, 1));
        assert_eq!((t, i, g), (MAX_THREAD, 0, 1), "thread at max");

        let (t, i, g) = unpack(pack(1, MAX_INDEX, 1));
        assert_eq!((t, i, g), (1, MAX_INDEX, 1), "index at max");

        let (t, i, g) = unpack(pack(1, 0, MAX_GENERATION));
        assert_eq!((t, i, g), (1, 0, MAX_GENERATION), "generation at max");
    }

    #[test]
    fn the_widths_sum_to_sixty_four_and_zero_is_unrepresentable() {
        assert_eq!(
            THREAD_BITS + INDEX_BITS + GENERATION_BITS,
            64,
            "the three fields must exactly fill a u64",
        );
        // `0` means "no runtime" in the C header. It must not be a value any
        // live handle can take, which holds because thread and generation
        // both start at 1.
        assert_ne!(pack(1, 0, 1), 0, "the smallest live handle is not zero");
        assert_ne!(pack(MAX_THREAD, MAX_INDEX, MAX_GENERATION), 0);
    }
}
