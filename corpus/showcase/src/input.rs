//! Input semantics shared by every host that draws these scenes.
//!
//! Names no scene, no node, no signal name and no colour, and depends on no
//! windowing library: a host maps its own events onto these three functions and
//! passes the signal name and optional action it was handed on
//! [`crate::Showcase`]. That is what lets hosts with unrelated event types
//! share one vocabulary rather than author one each.
//!
//! **`demo` is the only caller today.** `demo-android` and the Unity sample
//! reach it as the showcase-parity work lands; until they do, this module is
//! shared by intent rather than in fact.
//!
//! This is the second home of these bodies. They were `demo/src/input.rs`'s,
//! where story #573 wrote them to name no scene deliberately; what kept them
//! from being shared was the crate rather than the design — `demo` is a binary,
//! and its `key` takes a `winit` type, so no other host could call either.
//! The winit mapping stays there and only the semantics moved here.

use dashlang::LiveScene;
use dashscene_core::Arena;

use crate::SceneAction;

/// The signal value a pointer at `x_physical` names, normalised to `width` and
/// clamped to the `0.0..=1.0` range every showcase signal is authored over.
///
/// `None` for a zero-width drawable — a minimised window, or a surface whose
/// extent has not been configured yet: there is nothing to normalise against,
/// and no frame to show the result in either.
pub fn signal_from_x(x_physical: f64, width: u32) -> Option<f32> {
    if width == 0 {
        return None;
    }
    Some((x_physical as f32 / width as f32).clamp(0.0, 1.0))
}

/// Writes `value` to the scene's named signal.
///
/// Returns whether anything was written, so a caller knows whether to force a
/// redraw. `false` rather than a panic when the scene declares no such name:
/// the name is the scene's to choose, and a document loaded from a `.dsb` is
/// exactly the case that presents it.
pub fn set_signal(live: &mut LiveScene, signal: &str, value: f32) -> bool {
    match live.signal_named(signal) {
        Some(handle) => {
            live.set(handle, value);
            true
        }
        None => false,
    }
}

/// Runs the scene's own variant switch, if it declares one.
///
/// Returns whether anything ran. A scene with no variant set is not an error:
/// the command does nothing, rather than the host inventing a fallback for it.
pub fn run_action(live: &mut LiveScene, arena: &mut Arena, action: Option<SceneAction>) -> bool {
    match action {
        Some(action) => {
            action(live, arena);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_width_drawable_yields_no_signal_value() {
        assert_eq!(signal_from_x(100.0, 0), None);
    }

    #[test]
    fn the_pointer_normalises_over_the_drawable_width() {
        assert_eq!(signal_from_x(0.0, 1000), Some(0.0));
        assert_eq!(signal_from_x(500.0, 1000), Some(0.5));
        assert_eq!(signal_from_x(1000.0, 1000), Some(1.0));
    }

    #[test]
    fn a_pointer_outside_the_drawable_clamps_into_range() {
        assert_eq!(signal_from_x(-40.0, 1000), Some(0.0));
        assert_eq!(signal_from_x(1400.0, 1000), Some(1.0));
    }
}
