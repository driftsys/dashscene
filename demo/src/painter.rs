//! Which painter draws (story #585).
//!
//! # This is a property of the demonstration, and of nothing else
//!
//! `demo/` is one of the three workspace members that are never published. A
//! product host links the painter it ships with and constructs that one
//! presenter; it does not carry both and choose, and nothing in `dashpaint` or
//! in either painter learns that this module exists. The v0.15 roadmap entry is
//! explicit that the slice does not switch the entry tier, and a run-time
//! switch here is not a step towards one.
//!
//! What it is instead is the instrument story #585 exists to build. Watching a
//! primitive land beside the reference painter — same document, same arena,
//! same clock, same pulse — is a better instrument than diffing two PNGs, and
//! the rest of the slice is developed against it.

use std::sync::Arc;

use dashscene_desktop::{GpuPresenter, Present, PresentError};
use winit::window::Window;

use crate::present::SkiaPresenter;

/// Which of the two painters the host draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// `dashscene-skia`: CPU raster, posted through `softbuffer`. The default,
    /// because it is the painter that draws the whole v0 vocabulary.
    Skia,
    /// `dashscene-gpu`: instanced quads and analytic SDF, presenting to the
    /// window's own swapchain. Draws the subset epic #569 has reached.
    Gpu,
}

/// The command-line flag that selects one.
const FLAG: &str = "--painter";

impl Choice {
    /// Takes the painter flag out of `arguments`, leaving everything else in
    /// place for [`crate::scenes::select`].
    ///
    /// Removing rather than peeking: scene selection reads the first remaining
    /// argument as a scene name, so a flag left in the list would be looked up
    /// as one and refused.
    ///
    /// Returns the default when the flag is absent, and `Err` with the offending
    /// value when it names no painter — a run that silently drew with the wrong
    /// painter would be worse than one that did not start.
    pub fn take(arguments: &mut Vec<String>) -> Result<Self, String> {
        let Some(at) = arguments.iter().position(|argument| argument == FLAG) else {
            return Ok(Choice::Skia);
        };
        arguments.remove(at);
        if at == arguments.len() {
            return Err(format!("{FLAG} needs a painter: skia or gpu"));
        }
        match arguments.remove(at).as_str() {
            "skia" => Ok(Choice::Skia),
            "gpu" => Ok(Choice::Gpu),
            other => Err(format!("no painter named {other:?}: skia or gpu")),
        }
    }

    /// The other one — what the swap key selects.
    pub fn other(self) -> Self {
        match self {
            Choice::Skia => Choice::Gpu,
            Choice::Gpu => Choice::Skia,
        }
    }

    /// The value this painter announces through the showcase's badge
    /// signal, so the running window names the painter that drew it.
    ///
    /// The numbers are `showcase::badge`'s, not this module's. They are
    /// taken from there rather than written again so the two crates
    /// cannot be given different values in two places.
    pub fn badge_value(self) -> f32 {
        match self {
            Choice::Skia => showcase::badge::SKIA,
            Choice::Gpu => showcase::badge::GPU,
        }
    }

    /// Binds this painter to `window`.
    ///
    /// The one place either presenter is constructed, so the swap key and the
    /// first frame cannot build them differently.
    pub fn presenter(self, window: Arc<Window>) -> Result<Box<dyn Present>, PresentError> {
        match self {
            Choice::Skia => Ok(Box::new(SkiaPresenter::new(window)?)),
            Choice::Gpu => Ok(Box::new(GpuPresenter::new(window)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, FLAG};

    fn arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    /// No flag is the reference painter, which is what `cargo run -p demo` has
    /// always drawn with.
    #[test]
    fn no_flag_selects_the_reference_painter() {
        let mut list = arguments(&["typography"]);
        assert_eq!(Choice::take(&mut list), Ok(Choice::Skia));
        assert_eq!(list, arguments(&["typography"]));
    }

    /// The flag and its value are both removed, wherever in the list they sit.
    /// A leftover token would reach scene selection and be refused as a scene
    /// name.
    #[test]
    fn the_flag_and_its_value_are_taken_out_of_the_list() {
        let mut list = arguments(&["--all", FLAG, "gpu"]);
        assert_eq!(Choice::take(&mut list), Ok(Choice::Gpu));
        assert_eq!(list, arguments(&["--all"]));

        let mut leading = arguments(&[FLAG, "gpu", "typography"]);
        assert_eq!(Choice::take(&mut leading), Ok(Choice::Gpu));
        assert_eq!(leading, arguments(&["typography"]));
    }

    /// A painter that does not exist stops the run rather than falling back to
    /// the default, which would draw a frame nobody asked for and report
    /// nothing.
    #[test]
    fn an_unknown_painter_is_refused() {
        let mut list = arguments(&[FLAG, "vulkan"]);
        assert!(Choice::take(&mut list).is_err());
    }

    /// The flag with nothing after it is the same kind of mistake, and the
    /// value must not be taken from the scene name that follows.
    #[test]
    fn the_flag_with_no_value_is_refused() {
        let mut list = arguments(&["typography", FLAG]);
        assert!(Choice::take(&mut list).is_err());
    }

    /// The swap key alternates rather than cycling through a list, so this is
    /// the whole of its behaviour.
    #[test]
    fn each_painter_is_the_other_one_of_the_other() {
        assert_eq!(Choice::Skia.other(), Choice::Gpu);
        assert_eq!(Choice::Gpu.other().other(), Choice::Gpu);
    }

    /// The host and the showcase must agree on what each value means.
    /// They are separate crates, and this is the one seam where they can
    /// drift apart without either failing to compile.
    #[test]
    fn each_painter_announces_the_name_the_showcase_gives_that_value() {
        assert_eq!(
            showcase::badge::label(Choice::Skia.badge_value()),
            "dashscene-skia"
        );
        assert_eq!(
            showcase::badge::label(Choice::Gpu.badge_value()),
            "dashscene-gpu"
        );
    }

    /// A value the badge does not recognise renders as nothing, so a
    /// painter announcing one would go unnamed on screen rather than
    /// loudly wrong.
    #[test]
    fn no_painter_announces_the_unannounced_value() {
        for painter in [Choice::Skia, Choice::Gpu] {
            assert_ne!(painter.badge_value(), 0.0);
            assert!(!showcase::badge::label(painter.badge_value()).is_empty());
        }
    }
}
