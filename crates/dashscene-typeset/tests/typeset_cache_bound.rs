//! The shaping and bidi caches are bounded (issue #975).
//!
//! Both caches are keyed by paragraph text and, until this fix, held every
//! key they had ever been given. That was sound while shaping happened on
//! solving ticks only: a dashboard's label set is small and fixed, so the
//! maps stopped growing once every label had been seen once.
//!
//! Issue #621 changed the traffic. `stage_text` became a per-frame call
//! rather than a per-solve one, so a node whose string differs every frame
//! — a clock, a `format!("{v:.1}")` readout, a counter — hands each cache a
//! key it has never seen, 60 times a second, for the process lifetime. That
//! case is the headline case #621 existed to fix, which is what makes the
//! growth real rather than speculative.
//!
//! The assertions come in two halves, and both are load-bearing:
//!
//! - **The bound holds.** Feeding far more distinct paragraphs than the
//!   capacity leaves exactly `CACHE_CAPACITY` entries in each map. Asserting
//!   the count alone would pass against a cache that never stored anything,
//!   so `misses` is asserted to keep climbing: the entries are being
//!   evicted and reshaped, not refused.
//! - **The bound does not thrash.** A working set comfortably under the
//!   capacity, laid out twice, evicts nothing and hits on every paragraph of
//!   the second pass. A capacity below a real frame's working set would
//!   bound memory and destroy the cache at the same time, and only this half
//!   catches that.

use dashscene_typeset::text::{CACHE_CAPACITY, TextShape, Typesetter};

mod common;

use common::FONT;

/// Distinct paragraphs in the shape a changing readout produces: same
/// prefix, one differing number. Close enough to each other that a cache
/// keying on anything weaker than the whole text would collapse them.
fn readout(i: usize) -> String {
    format!("Range {i}.{} km", i % 10)
}

/// Enough distinct paragraphs to turn a full cache over twice, so eviction is
/// forced several times over rather than only just reached. Twice rather than
/// four times because every paragraph past the first turnover shapes for real
/// in a debug build and no test tier runs `--release`; the assertions below
/// are unchanged by the difference.
const OVERFLOW: usize = CACHE_CAPACITY * 2;

/// Lays `text` out under the **non-default** posture — ligatures forced off
/// (story #341) — which `Typesetter::posture` interns as a second slot and
/// gives its own cache. `layout` alone never reaches that code path.
fn layout_ligatures_off(ts: &mut Typesetter, text: &str) {
    let shape = TextShape {
        ligatures_off: true,
        ..TextShape::default()
    };
    ts.layout_with(text, 16.0, None, shape);
}

#[test]
fn a_changing_string_cannot_grow_the_caches_without_bound() {
    let mut ts = common::typesetter(FONT);

    for i in 0..OVERFLOW {
        ts.layout(&readout(i), 16.0, None);
    }

    let stats = ts.cache_stats();
    assert_eq!(
        stats.shaped_entries, CACHE_CAPACITY,
        "the shaped-run cache must stop at its capacity, not grow to {OVERFLOW}"
    );
    assert_eq!(
        stats.bidi_entries, CACHE_CAPACITY,
        "the bidi cache must stop at its capacity, not grow to {OVERFLOW}"
    );

    // Every paragraph was distinct, so every one of them missed. This is
    // what separates a cache that evicted from one that simply refused to
    // store: a full map with no shaping behind it would satisfy the two
    // assertions above.
    assert_eq!(
        stats.misses, OVERFLOW as u64,
        "each distinct paragraph shapes once"
    );
    assert_eq!(stats.hits, 0, "no paragraph repeated");
    // The number that distinguishes this from a working set that fits: every
    // paragraph past the first CACHE_CAPACITY displaced one.
    assert_eq!(
        stats.evictions,
        (OVERFLOW - CACHE_CAPACITY) as u64,
        "each paragraph past the capacity dropped exactly one"
    );
}

#[test]
fn a_working_set_under_the_capacity_is_never_evicted() {
    // Half the capacity — a dense screen's distinct labels, well inside the
    // bound. The whole point of the capacity is that this case behaves
    // exactly as the unbounded cache did.
    let working_set: Vec<String> = (0..CACHE_CAPACITY / 2).map(readout).collect();
    // Without this the test is vacuous at CACHE_CAPACITY = 1: the division
    // truncates to an empty set, both loops run zero times, and every
    // assertion below holds against zero — so the half of the pair whose job
    // is to catch a capacity too small to hold a frame would report green for
    // the smallest capacity there is.
    assert!(
        !working_set.is_empty(),
        "CACHE_CAPACITY must be at least 2 for this test to assert anything"
    );
    let mut ts = common::typesetter(FONT);

    for text in &working_set {
        ts.layout(text, 16.0, None);
    }
    let cold = ts.cache_stats();
    assert_eq!(
        cold.misses,
        working_set.len() as u64,
        "the first pass shapes"
    );
    assert_eq!(cold.hits, 0);

    for text in &working_set {
        ts.layout(text, 16.0, None);
    }
    let warm = ts.cache_stats();
    assert_eq!(
        warm.misses, cold.misses,
        "the second pass must shape nothing — a capacity under a real \
         frame's working set would bound memory and destroy the cache"
    );
    assert_eq!(
        warm.hits,
        working_set.len() as u64,
        "every paragraph of the second pass hits"
    );
    assert_eq!(warm.shaped_entries, working_set.len());
    assert_eq!(warm.bidi_entries, working_set.len());
    assert_eq!(
        warm.evictions, 0,
        "a working set inside the capacity drops nothing — this is the \
         number that tells this case from a thrashing one"
    );
}

/// Eviction is by recency, not by insertion order: a paragraph kept warm
/// across a flood of one-shot strings survives it.
///
/// This is the property that makes the bound safe for a real scene, where
/// the static labels are laid out every frame alongside the one changing
/// readout. An insertion-ordered bound would evict the labels — the entries
/// worth keeping — and keep the readout strings, which are never asked for
/// again.
///
/// **Both caches are asserted.** They are two independent lookups with two
/// independent counters, so a recency check written against `misses` alone
/// pins the shaped-run half and leaves the bidi half free to be insertion
/// ordered. `bidi_resolutions` is what pins the other one.
#[test]
fn a_paragraph_used_every_frame_survives_a_flood_of_new_ones() {
    const LABEL: &str = "Range";
    let mut ts = common::typesetter(FONT);

    ts.layout(LABEL, 16.0, None);
    let cold = ts.cache_stats();

    // Enough one-shot paragraphs to turn the cache over twice, with the
    // label re-laid between each, exactly as a frame loop would.
    for i in 0..CACHE_CAPACITY * 2 {
        ts.layout(&readout(i), 16.0, None);
        ts.layout(LABEL, 16.0, None);
    }

    let flooded = ts.cache_stats();
    assert_eq!(
        flooded.misses,
        cold.misses + (CACHE_CAPACITY * 2) as u64,
        "only the one-shot paragraphs shaped; the label stayed resident in \
         the shaped-run cache"
    );
    assert_eq!(
        flooded.bidi_resolutions,
        cold.bidi_resolutions + (CACHE_CAPACITY * 2) as u64,
        "only the one-shot paragraphs resolved; the label stayed resident in \
         the bidi cache"
    );
}

/// A posture interned after construction gets a bounded cache too.
///
/// `Typesetter::posture` creates a cache per distinct (slot set, ligature)
/// pair, on a line separate from the constructor's. Every other test here
/// lays out through `layout`, which requests weight 400 with ligatures on —
/// posture 0, the map the constructor built. So the constructor could be
/// bounded and `posture` left unbounded with the rest of this file green,
/// which is exactly what a mutation of that one line demonstrates.
///
/// That matters for a real document: a formatted readout on a label with
/// ligatures off, or at any weight resolving to a different face, takes its
/// cache from `posture` rather than from the constructor.
#[test]
fn a_posture_interned_after_construction_is_bounded_too() {
    let mut ts = common::typesetter(FONT);

    // Posture 0, so the second posture's map is the one that has to grow.
    ts.layout("Range", 16.0, None);
    let default_only = ts.cache_stats();
    assert_eq!(default_only.shaped_entries, 1, "posture 0 holds the label");

    for i in 0..OVERFLOW {
        layout_ligatures_off(&mut ts, &readout(i));
    }

    let stats = ts.cache_stats();
    assert_eq!(
        stats.shaped_entries,
        CACHE_CAPACITY + 1,
        "the second posture bounds itself at CACHE_CAPACITY, beside posture \
         0's one resident label — this is the `postures * CACHE_CAPACITY` \
         the shaped_entries doc describes"
    );
    // The bidi resolution has no posture, so the one shared map bounds the
    // union rather than gaining a second capacity.
    assert_eq!(
        stats.bidi_entries, CACHE_CAPACITY,
        "one bidi map serves every posture and stays at its own capacity"
    );
}
