//! What the frame loop does about a frame that failed (story #834).
//!
//! Split out of the `host` module so it can be asserted without a display. The
//! loop itself runs on `winit`'s event loop and needs an `ActiveEventLoop`,
//! which cannot be constructed outside a running application — so the loop's
//! *decision* is what a test can reach, and the decision is what was wrong.
//!
//! Before this story `Host::frame` treated every `Err` from `paint` as fatal and
//! called `event_loop.exit()`. The window closed and the process ended. The
//! recovery existed the whole time — [`crate::Reaction::Rebind`] drops the
//! presenter and asks [`crate::App::presenter`] for another, which is precisely
//! what `dashscene_gpu::FrameError::Lost` says the remedy is — and it was
//! reachable only from [`crate::App::event`] and [`crate::App::woken`]. A
//! present failure ended the loop before either could run, so the one entry
//! point that could recover a lost surface could not be reached by the failure
//! it exists for (issue #818).

use crate::present::PresentError;

/// What the loop does about a failure.
///
/// Public because it is the contract an embedder is owed: [`crate::DesktopError`]
/// reaches the caller of [`crate::run`] after the loop has already decided, and
/// "was that the end of it?" is not a question the message text should have to
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Drop the presenter, ask [`crate::App::presenter`] for another, and keep
    /// running.
    ///
    /// The arena, the live scene, the frame clock and the embedder's own state
    /// are untouched: the next frame is the same frame drawn through a new
    /// device. This is exactly [`crate::Reaction::Rebind`], which the loop
    /// already knew how to apply.
    Rebind,
    /// Stop, and report. The loop cannot produce another frame.
    Stop,
}

/// Classifies a failed present.
///
/// One rule, and it is `dashscene_gpu`'s rather than this crate's:
/// [`dashscene_gpu::FrameError::is_recoverable`] is what `dashscene-web`,
/// `dashscene-desktop` and `dashscene-android` all read, so three hosts cannot
/// disagree about what a recoverable failure is. That divergence is what story
/// #834 was opened to prevent.
///
/// Everything a presenter can report other than a frame failure is fatal: a
/// surface that could not be built, a drawable past the device maximum, a
/// framebuffer that does not hold the pixels that were drawn. None of them
/// becomes true again by being retried, and the extent cases are refusals rather
/// than losses.
pub fn recovery(error: &PresentError) -> Recovery {
    match error {
        PresentError::Frame(error) if error.is_recoverable() => Recovery::Rebind,
        _ => Recovery::Stop,
    }
}

#[cfg(test)]
mod tests {
    use dashscene_gpu::FrameError;

    use super::*;

    #[test]
    fn a_lost_surface_rebinds_the_presenter() {
        assert_eq!(
            recovery(&PresentError::Frame(FrameError::Lost)),
            Recovery::Rebind
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
            recovery(&PresentError::Frame(FrameError::Outdated)),
            Recovery::Stop
        );
        assert_eq!(
            recovery(&PresentError::Frame(FrameError::Validation)),
            Recovery::Stop
        );
    }

    /// Everything that is not a `dashscene-gpu` frame failure, including the
    /// raster presenter's own `Post`. A rebind would build a second presenter to
    /// meet the same refusal.
    #[test]
    fn a_failure_outside_the_frame_stops_the_loop() {
        assert_eq!(
            recovery(&PresentError::Surface("no adapter".to_owned())),
            Recovery::Stop
        );
        assert_eq!(
            recovery(&PresentError::Post("softbuffer".to_owned())),
            Recovery::Stop
        );
        assert_eq!(
            recovery(&PresentError::Extent {
                width: 40_000,
                height: 40_000,
                max: 16_384,
            }),
            Recovery::Stop
        );
        assert_eq!(
            recovery(&PresentError::ExtentMismatch {
                painted: 4,
                framebuffer: 9,
            }),
            Recovery::Stop
        );
    }
}
