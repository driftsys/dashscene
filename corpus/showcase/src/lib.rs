//! The showcase scenes the demonstration host draws (v0.14, story #574).
//!
//! These live under `corpus/` and not under `demo/` because they exercise the
//! full v0 paint vocabulary, which is what the stress corpus is for. `demo/`
//! holds the host — a window, a clock and a frame loop — and this holds the
//! content it draws (epic #568).
//!
//! # What is here
//!
//! Three scenes, each a pair of plain functions matching the host's own
//! [`SceneBuilder`] and [`ScenePulse`] shapes:
//!
//! - [`surfaces`] — fills, strokes, corners, images, a baked vector field,
//!   shadows, clips, masks, group opacity and a backdrop blur, in a wrapping
//!   gallery under a sliding frosted panel;
//! - [`typography`] — MSDF text in Latin and Arabic, with one string driven by
//!   a signal;
//! - [`layout`] — the flex vocabulary, a grid with spans, and a reflow driven
//!   by a topology change.
//!
//! # Coverage is a checklist, not a test
//!
//! There is no coverage test in this crate and there never should be. A
//! demonstration wired into CI becomes a suite whose green state reads as
//! evidence it never established (epic #568). What the scenes cover is written
//! down in `corpus/showcase/README.md`, one line per construct, and a person
//! walks it against the running demonstration. The only automated claim is
//! that this compiles.
//!
//! There are no golden images for these scenes either, for the same reason.
//! `goldens/` holds the frames the project pins; these are frames it shows.
//!
//! # How a scene is built
//!
//! In two passes, because `dashlang`'s builder carries geometry, the flex
//! vocabulary, one solid fill and the reactive bindings, and nothing of the
//! paint vocabulary (issue #118). Structure and motion are authored through
//! `dashlang`; the paint intent is staged onto the named nodes afterwards
//! through `dashscene_core::Txn`. See [`vocabulary`] for why the second pass
//! is safe to run against a live scene's arena.

pub mod layout;
pub mod resources;
pub mod solver;
pub mod surfaces;
pub mod typography;
pub mod vocabulary;

use dashlang::LiveScene;
use dashscene_core::Arena;

/// Builds a scene into `arena` for a drawable of `width` x `height` physical
/// pixels. The same shape as the host's `SceneBuilder`.
pub type SceneBuilder = fn(&mut Arena, u32, u32) -> LiveScene;

/// Applies the scene's scripted signal change for phase `index`. The same
/// shape as the host's `ScenePulse`.
pub type ScenePulse = fn(&mut LiveScene, u64);

/// One selectable scene.
pub struct Showcase {
    /// The name the host selects it by, on the command line.
    pub name: &'static str,
    /// One line describing what a person is looking at.
    pub summary: &'static str,
    pub build: SceneBuilder,
    pub pulse: ScenePulse,
}

/// Every scene, in the order the checklist walks them.
pub const SCENES: &[Showcase] = &[
    Showcase {
        name: "surfaces",
        summary: "fills, strokes, corners, images, a baked vector field, shadows, clips, masks, \
                  group opacity and a backdrop blur",
        build: surfaces::build,
        pulse: surfaces::pulse,
    },
    Showcase {
        name: "typography",
        summary: "MSDF text in Latin and Arabic, with one string driven by a signal",
        build: typography::build,
        pulse: typography::pulse,
    },
    Showcase {
        name: "layout",
        summary: "the flex vocabulary, a grid with spans, and a reflow driven by a topology change",
        build: layout::build,
        pulse: layout::pulse,
    },
];

/// The scene the host opens with when it is given no name. `surfaces` shows
/// the most of the vocabulary in one frame, which is what makes it the still
/// the entry-path documentation wants.
pub const DEFAULT: &str = "surfaces";

/// The scene called `name`, or `None`.
pub fn by_name(name: &str) -> Option<&'static Showcase> {
    SCENES.iter().find(|scene| scene.name == name)
}
