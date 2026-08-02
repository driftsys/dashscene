//! The showcase scenes the demonstration host draws (v0.14, story #574).
//!
//! These live under `corpus/` and not under `demo/` because they exercise the
//! full v0 paint vocabulary, which is what the stress corpus is for. `demo/`
//! holds the host — a window, a clock and a frame loop — and this holds the
//! content it draws (epic #568).
//!
//! # What is here
//!
//! Three scenes, each a builder and a scripted phase matching the host's own
//! [`SceneBuilder`] and [`ScenePulse`] shapes, plus a [`SceneAction`] where the
//! scene has a variant switch to offer:
//!
//! - [`surfaces`] — fills, strokes, corners, images, a baked vector field,
//!   shadows, clips, masks, group opacity and a backdrop blur, in a wrapping
//!   gallery under a sliding frosted panel;
//! - [`typography`] — MSDF text in Latin and Arabic, with one string driven by
//!   a signal;
//! - [`layout`] — the flex vocabulary, a grid with spans, a reflow driven by a
//!   topology change, and a variant switch a key runs.
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
//! In one pass. `dashlang`'s builder carries the whole v0 paint vocabulary —
//! fills, gradients, strokes, corners, shadows, blur, clip, mask, opacity,
//! and text with its style — alongside geometry, the flex vocabulary and the
//! reactive bindings, so a scene authors structure, motion and paint on the
//! same value tree.
//!
//! Two constructs still need a short second pass over the built arena,
//! addressing its nodes by the name they were given on the tree: an image
//! fill (including a cropped one and a baked vector field's coverage mask),
//! because each references an index `dashscene_core::Txn::add_image` issues
//! and no such index exists until the tree is built into an arena; and a
//! variant-set declaration, because `Txn::add_variant_set` is likewise an
//! arena operation. `surfaces` runs this second pass for its image fills and
//! its vector field; `layout` runs it only to declare its variant set;
//! `typography` needs no second pass at all. See [`vocabulary`] for why
//! running it after `build_live` is safe against a live scene's arena.
//!
//! # What a scene tells the host about input
//!
//! Two fields on [`Showcase`], and no third mechanism (stories #573, #625).
//!
//! [`Showcase::signal`] names the one scalar signal the scene already declares,
//! so the host's pointer and arrow keys can drive it through
//! `LiveScene::signal_named` without knowing what it means. That half needs
//! only the name.
//!
//! [`Showcase::action`] is the other half, and it exists because a **variant
//! switch cannot be expressed as a signal write**. `Txn::set_variant` needs the
//! arena, and the scene builder is handed one exactly once while the scripted
//! phase ([`ScenePulse`]) is handed only a `LiveScene`. Before this seam the
//! host had to author a variant set itself against a node it knew by name,
//! which is the host authoring content — the thing the `demo/` and
//! `corpus/showcase/` split exists to prevent. So the scene declares its own
//! set at build time, where it has the arena, and owns the switch; the host
//! binds a key to [`Showcase::action`] and constructs nothing.

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

/// Runs the scene's own variant switch against the arena it was built into.
/// The same shape as the host's `SceneAction`.
///
/// Takes the arena as well as the live scene because that is the whole point:
/// `Txn::set_variant` is an arena mutation and has no signal equivalent.
pub type SceneAction = fn(&mut LiveScene, &mut Arena);

/// One selectable scene.
pub struct Showcase {
    /// The name the host selects it by, on the command line.
    pub name: &'static str,
    /// One line describing what a person is looking at.
    pub summary: &'static str,
    pub build: SceneBuilder,
    pub pulse: ScenePulse,
    /// The scalar signal input drives, 0..1.
    ///
    /// The name the scene passed to `Scene::signal_named`, which is what
    /// `LiveScene::signal_named` looks up at run time. Every scene declares
    /// exactly one, so the host needs no signal vocabulary of its own.
    pub signal: &'static str,
    /// What a key does — the scene's own variant switch.
    ///
    /// `None` for a scene that declares no variant set, and the key then does
    /// nothing rather than the host inventing a fallback.
    pub action: Option<SceneAction>,
}

/// Every scene, in the order the checklist walks them.
pub const SCENES: &[Showcase] = &[
    Showcase {
        name: "surfaces",
        summary: "fills, strokes, corners, images, a baked vector field, shadows, clips, masks, \
                  group opacity and a backdrop blur",
        build: surfaces::build,
        pulse: surfaces::pulse,
        signal: surfaces::SWEEP,
        action: None,
    },
    Showcase {
        name: "typography",
        summary: "MSDF text in Latin and Arabic, with one string driven by a signal",
        build: typography::build,
        pulse: typography::pulse,
        signal: typography::LEVEL,
        action: None,
    },
    Showcase {
        name: "layout",
        summary: "the flex vocabulary, a grid with spans, a reflow driven by a topology change, \
                  and a variant switch on a key",
        build: layout::build,
        pulse: layout::pulse,
        signal: layout::SPREAD,
        action: Some(layout::switch_variant),
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
