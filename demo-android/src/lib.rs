//! The Android showcase host: the same demonstration `demo` and `demo-web` run,
//! on a `SurfaceView` (story #842).
//!
//! # Why this does not go through the C ABI
//!
//! `dashscene-android`'s own document path does, and that is D2 working as
//! intended. This host cannot. The showcase's scenes are **built in code** —
//! `SceneBuilder` is `fn(&mut Arena, u32, u32) -> LiveScene` — and the ABI's
//! arena lives inside an opaque `DsRuntime`, with no builder entry point: that
//! is layer 2, D8, deferred with its layer rather than invented here.
//!
//! So this host owns the arena, the scene, the painter and the surface, exactly
//! as `demo` and `demo-web` do, and meets the platform half at
//! `dashscene_android::Frames`. The render thread, the looper, the vsync
//! callback and the destroy handshake are that crate's and are not restated
//! here — a second Android frame loop, written beside the first because the
//! first only knew about `.dsb` bytes, is the divergence story #834 exists to
//! prevent.
//!
//! # Why the text draws at all
//!
//! Each scene builds its own solver. `typography::build` constructs a
//! `ShowcaseSolver` carrying the fonts and the typesetter, so the `LiveScene`
//! this host is handed can already measure and stage glyph runs. That is worth
//! saying because the opposite is true of a **loaded document**: all three
//! integration crates inject a bare `TaffySolver`, which has no typesetter and
//! no atlases, so a `.dsb` with text collapses its boxes and draws no glyphs.
//! Issue #863 carries that; this host does not hit it, and does not fix it.
//!
//! # What an embedder should not read into this
//!
//! `publish = false`. This is a demonstration, and the scene registry, the
//! scripted pulse and the timing instrument below are demonstration concerns. An
//! embedder writes its own `Frames` and keeps the loop.

mod timing;

pub use timing::{Sample, Timing};

/// Picks a scene by name, falling back to the first.
///
/// Compiled on every target and tested on the host: it is the one thing in this
/// crate that can be wrong without a device, so it is kept out of the platform
/// half for the reason `dashscene-web` keeps `fetch` and `shown` out of its own.
pub fn select(name: Option<&str>) -> &'static showcase::Showcase {
    match name.and_then(showcase::by_name) {
        Some(scene) => scene,
        // Not an error. A launch with no extra, or with one naming a scene that
        // does not exist, should still draw something rather than a blank
        // window with a log line — this is a demonstration, and the first scene
        // is as good a default as any.
        None => &showcase::SCENES[0],
    }
}

#[cfg(target_os = "android")]
mod host;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_scene_is_selected() {
        assert_eq!(select(Some("typography")).name, "typography");
        assert_eq!(select(Some("layout")).name, "layout");
    }

    /// A missing or unknown name still draws, rather than failing a launch.
    #[test]
    fn an_unknown_or_absent_name_falls_back_to_the_first_scene() {
        let first = showcase::SCENES[0].name;
        assert_eq!(select(None).name, first);
        assert_eq!(select(Some("not-a-scene")).name, first);
        assert_eq!(select(Some("")).name, first);
    }

    /// Every scene the registry offers can be selected by its own name, so the
    /// launch parameter reaches all of them rather than the two anyone tried.
    #[test]
    fn every_scene_is_reachable_by_name() {
        for scene in showcase::SCENES {
            assert_eq!(
                select(Some(scene.name)).name,
                scene.name,
                "{} is in the registry and not selectable",
                scene.name
            );
        }
    }
}
