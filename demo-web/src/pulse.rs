//! When the scene's scripted signal change is applied.
//!
//! A showcase scene animates because something writes its signal; nothing in
//! `LiveScene::tick` does that on its own. The native host has a driver that
//! wakes the loop every [`INTERVAL_MS`] and advances a counter
//! (`demo/src/shell.rs`), and the scene is a pure function of that counter — so
//! the phase lives in the host and the scene stays scriptable from it.
//!
//! This host had no such driver at first, so every scene drew one frame and
//! parked. It looked exactly like a working host that had nothing to animate,
//! which is why it survived the tests: nothing here can tell "settled" from
//! "never driven".

/// How often the scene's signal is advanced, in milliseconds.
///
/// The native host's 2500 ms, and for its stated reason: long enough that the
/// loop visibly settles between pulses, so an idle frame is observable rather
/// than asserted.
pub(crate) const INTERVAL_MS: f64 = 2500.0;

/// How many pulses should have been applied by `elapsed_ms` after the start.
///
/// The **count**, not a step: a tab that was backgrounded for a minute comes
/// back with many intervals elapsed, and the host applies only the newest
/// index. That is correct rather than lazy, because a scene's pulse is a pure
/// function of its index — replaying the ones in between would write signals
/// that the newest write immediately overwrites.
pub(crate) fn count_by(elapsed_ms: f64) -> u64 {
    // `is_nan` first and by name: a timestamp that is not a number has reached
    // no interval, and `NaN <= 0.0` is false, so a comparison alone would let it
    // through to a cast whose result nobody should have to reason about.
    if elapsed_ms.is_nan() || elapsed_ms <= 0.0 {
        return 0;
    }
    (elapsed_ms / INTERVAL_MS) as u64
}

#[cfg(test)]
mod tests {
    use super::{INTERVAL_MS, count_by};

    /// Nothing has been driven before the first interval elapses, so the first
    /// pulse is index zero at the first boundary rather than at the start.
    #[test]
    fn no_pulse_has_fired_before_the_first_interval() {
        assert_eq!(count_by(0.0), 0);
        assert_eq!(count_by(INTERVAL_MS - 1.0), 0);
    }

    #[test]
    fn the_count_advances_once_per_interval() {
        assert_eq!(count_by(INTERVAL_MS), 1);
        assert_eq!(count_by(INTERVAL_MS * 2.0), 2);
        assert_eq!(count_by(INTERVAL_MS * 2.5), 2, "and not part way through");
        assert_eq!(count_by(INTERVAL_MS * 3.0), 3);
    }

    /// A backgrounded tab resumes with many intervals elapsed. The count is
    /// what it should be, and the caller applies only that index — the point of
    /// returning a count rather than a step.
    #[test]
    fn a_long_gap_reports_the_count_it_reached() {
        assert_eq!(count_by(INTERVAL_MS * 24.0), 24);
    }

    /// A clock that has not advanced, or has gone backwards, has not driven
    /// anything. Casting a negative or `NaN` float to `u64` saturates rather
    /// than doing anything useful, so it is answered before the cast.
    #[test]
    fn a_clock_that_did_not_advance_drives_nothing() {
        assert_eq!(count_by(-1.0), 0);
        assert_eq!(count_by(-INTERVAL_MS * 10.0), 0);
        assert_eq!(count_by(f64::NAN), 0);
    }
}
