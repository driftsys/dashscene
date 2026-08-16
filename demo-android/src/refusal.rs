//! Why the last resize was refused, kept so the loop can report it once per
//! refusal rather than once per frame (issue #1194).
//!
//! # Why this is a module and not three lines in `host`
//!
//! Because it is a part that can be wrong without a device. `mod host` is
//! `#[cfg(target_os = "android")]` and compiles nowhere else, so no test tier
//! reaches anything inside it — and the one ordering error that matters here,
//! clearing the record before the early return rather than after the match,
//! compiles and lints clean on every gate this repository runs.
//!
//! It is the rule `dashscene-android`'s own `handshake` and `machine` modules
//! state and that `timing` beside this one already follows: keep the decidable
//! part outside the platform gate. Issue #1194's "Verification when it is
//! fixed" names the alternative as a device run reading logcat, which is the
//! more expensive half of the same question.
//!
//! # What the key is for
//!
//! `LoopState::step` bounds its report by extent — `record_refusal` answers
//! true only when the wanted extent changes — so the reason it asks for is
//! always the reason for *that* extent. Keying the record the same way means
//! the message is built once per refused extent instead of once per vsync, for
//! the reason `dashscene-android`'s `DocumentFrames` compares its `DsStatus`
//! before resolving a message: a refused extent is offered again every frame on
//! purpose, so anything done unconditionally in `resize` is done sixty times a
//! second for as long as the surface lives.
//!
//! `DocumentFrames` keys on the status because its message costs a
//! `ds_last_error_message` round trip. This keys on the extent because
//! `SurfaceRenderer::resize` has one failure — `check_extent`'s
//! `RendererError::Extent`, whose whole content is the pair offered and the
//! device maximum — so the extent decides the text.

/// The reason the last `Frames::resize` answered `false`, or
/// `None` if the last one succeeded.
///
/// Only the message crosses to the loop; the extent beside it exists to decide
/// whether a new message has to be built.
#[derive(Default)]
pub struct Refusal {
    recorded: Option<((u32, u32), String)>,
}

impl Refusal {
    /// Records that `(width, height)` was refused, building `reason` only if
    /// this extent is not the one already recorded.
    ///
    /// `reason` is a closure rather than a `String` so that a caller pays the
    /// formatting only when the record actually changes.
    pub fn refused(&mut self, width: u32, height: u32, reason: impl FnOnce() -> String) {
        if self.recorded.as_ref().map(|(extent, _)| *extent) != Some((width, height)) {
            self.recorded = Some(((width, height), reason()));
        }
    }

    /// Forgets whatever was recorded.
    ///
    /// Called from two places, and the second is easy to miss: a resize that is
    /// taken up, so a stale reason is not reported against the next refusal —
    /// and a **detach**, because the renderer after it is not the one the
    /// record was written against. `LoopState::acquire` clears its own
    /// `last_refused` for exactly that reason, and a record that outlived the
    /// device it describes would answer for one that has refused nothing.
    pub fn clear(&mut self) {
        self.recorded = None;
    }

    /// What `Frames::refusal_reason` answers.
    pub fn reason(&self) -> Option<String> {
        self.recorded.as_ref().map(|(_, message)| message.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::Refusal;
    use std::cell::Cell;

    #[test]
    fn a_fresh_record_has_no_reason() {
        assert_eq!(Refusal::default().reason(), None);
    }

    #[test]
    fn the_reason_is_built_once_per_refused_extent_and_not_once_per_call() {
        // The bound this module exists for. `LoopState::step` offers a refused
        // extent again every frame, so the closure standing in for the format
        // must run once for the extent and not once per offer.
        let built = Cell::new(0);
        let mut refusal = Refusal::default();
        for _ in 0..60 {
            refusal.refused(8192, 8192, || {
                built.set(built.get() + 1);
                "too large".to_owned()
            });
        }
        assert_eq!(built.get(), 1);
        assert_eq!(refusal.reason().as_deref(), Some("too large"));
    }

    #[test]
    fn a_different_extent_is_a_different_reason() {
        let mut refusal = Refusal::default();
        refusal.refused(8192, 8192, || "first".to_owned());
        refusal.refused(4096, 8192, || "second".to_owned());
        assert_eq!(refusal.reason().as_deref(), Some("second"));
    }

    #[test]
    fn the_same_extent_after_a_clear_is_recorded_again() {
        // The case a plain "have I seen this extent" flag would get wrong: a
        // resize taken up, or a detach, and then the same extent refused by the
        // device that followed.
        let mut refusal = Refusal::default();
        refusal.refused(8192, 8192, || "first".to_owned());
        refusal.clear();
        assert_eq!(refusal.reason(), None);
        refusal.refused(8192, 8192, || "second".to_owned());
        assert_eq!(refusal.reason().as_deref(), Some("second"));
    }
}
