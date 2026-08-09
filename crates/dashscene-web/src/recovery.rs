//! What the frame loop does about a frame that failed (story #834).
//!
//! Compiled on every target, not only `wasm32`, and that is the point rather
//! than an accident — the same argument the `fetch` and `shown` modules are kept
//! out of the browser half for. The loop itself needs a browser; the *decision*
//! it makes does not, and the decision is what was wrong.
//!
//! Before this story `Host::run_loop` treated every `Err` as fatal and stopped
//! rescheduling `requestAnimationFrame`, so a recoverable context loss — a
//! driver reset, a laptop switching between integrated and discrete graphics, a
//! browser reclaiming a backgrounded tab's context — froze the page until the
//! user reloaded. Nothing could have caught that: the loop is behind
//! `#[cfg(target_arch = "wasm32")]`, so no test on the host platform ever
//! compiled it (issue #813).

use crate::WebError;

/// What the loop does about a failure.
///
/// Public because it is the contract an embedder is owed: [`WebError`] reaches
/// the reporter after the loop has already decided, and "was that the end of
/// it?" is not a question the message text should have to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Rebuild the surface against the same canvas and keep scheduling frames.
    ///
    /// The scene, the arena and the embedder's own state are untouched: the
    /// next frame is the same frame drawn through a new device.
    Rebuild,
    /// Stop. The loop cannot produce another frame, and rescheduling would
    /// repeat the failure sixty times a second into the console.
    Stop,
}

/// Classifies a failed frame.
///
/// One rule, and it is `dashscene_gpu`'s rather than this crate's:
/// [`dashscene_gpu::FrameError::is_recoverable`] is what
/// `dashscene-web`, `dashscene-desktop` and `dashscene-android` all read, so
/// three hosts cannot disagree about what a recoverable failure is. That
/// divergence is what story #834 was opened to prevent.
///
/// Everything that is not a frame failure is fatal by construction: a canvas
/// that is not on the page, a document that does not parse, a payload that does
/// not match its hash. None of them becomes true again by being retried.
pub fn recovery(error: &WebError) -> Recovery {
    match error {
        WebError::Frame(error) if error.is_recoverable() => Recovery::Rebuild,
        _ => Recovery::Stop,
    }
}

#[cfg(test)]
mod tests {
    use dashscene_gpu::FrameError;

    use super::*;

    #[test]
    fn a_lost_surface_is_rebuilt() {
        assert_eq!(
            recovery(&WebError::Frame(FrameError::Lost)),
            Recovery::Rebuild
        );
    }

    /// The two frame failures that are **not** recoverable, asserted by name.
    ///
    /// A `_ => Stop` catch-all would pass this test whatever the classification
    /// did, so each is named: the point is that `Outdated` and `Validation` stay
    /// fatal, not that something falls through to the default arm.
    #[test]
    fn the_other_frame_failures_stop_the_loop() {
        assert_eq!(
            recovery(&WebError::Frame(FrameError::Outdated)),
            Recovery::Stop
        );
        assert_eq!(
            recovery(&WebError::Frame(FrameError::Validation)),
            Recovery::Stop
        );
    }

    /// A failure that is not a frame failure at all. Retrying a canvas that is
    /// not on the page produces the same answer forever.
    #[test]
    fn a_failure_outside_the_frame_stops_the_loop() {
        assert_eq!(
            recovery(&WebError::NoCanvas("scene".to_owned())),
            Recovery::Stop
        );
        assert_eq!(recovery(&WebError::ShortFile), Recovery::Stop);
    }
}
