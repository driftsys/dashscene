//! Loads a compiled `.dsb` into the host, as the same [`shell::SceneBuilder`]
//! / [`shell::ScenePulse`] pair [`crate::placeholder_scene`] builds by hand
//! (story #575, epic #568).
//!
//! `dashscene_core::load`'s own doc comment states the read contract: run
//! `dashbuf::open` (the envelope plus the flatbuffers verifier — it calls
//! `root_as_document` internally, since a `.dsb` has been a sectioned
//! container since v0.11), then `dashscene_validator::validate_document`
//! (the referential load gate), then `dashscene_core::load_document` (the
//! replay through the ordinary producer API: `add_node` / `set_prop` /
//! `commit`). [`dashlang::attach_live`] is the loader-side counterpart of
//! `Scene::build_live` — it builds a [`LiveScene`] from the binding tables an
//! arena already carries, rather than from a freshly authored one — so a
//! loaded document drives the same [`LiveScene::tick`] the placeholder scene
//! does. Nothing here is a second path through the frame loop.
//!
//! [`shell::SceneBuilder`]: crate::shell::SceneBuilder
//! [`shell::ScenePulse`]: crate::shell::ScenePulse

use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_engine::TaffySolver;

/// `goldens/dsb/v03-paint.dsb`: `dashc::compile_figma`'s output for a real
/// Figma capture, `corpus/figma-fixtures/v03-paint.json` — a 960x680 paint
/// vocabulary swatch board (solid, gradient, and image fills, a stroke,
/// corner radii) that two CI suites already pin byte-for-byte
/// (`goldens/dsb/README.md`).
///
/// Picked over the other nine `goldens/dsb` fixtures because it is the
/// richest compiled document in the tree (14 nodes, 14 paint entries) and
/// the only one carrying a real embedded image through the v0.11 asset
/// table, so loading it exercises the payload-binding step in
/// `dashscene_core::load_document` that the other nine never reach.
///
/// `goldens/images/v03-paint.png` is **not** a picture of this document,
/// despite the name: `goldens/tooling/tests/v03_families.rs` names it a
/// hand-built boundary-B golden at 96x96, decoupled from any producer,
/// while this document's own root resolves to 960x680
/// (`crates/dashc/tests/figma_lowering.rs::the_fixture_compiles_loads_and_renders`
/// pins its 14-rect shape and that it rasterizes, with no pixel golden).
/// No committed `goldens/dsb` fixture has a wired, pixel-compared
/// end-to-end picture today (issue #616).
///
/// This is the committed golden byte for byte — not recompiled, not
/// modified — read at compile time so the host carries no runtime path
/// dependency on the working tree.
const DOCUMENT: &[u8] = include_bytes!("../../goldens/dsb/v03-paint.dsb");

/// Loads [`DOCUMENT`] into `arena` and attaches a [`LiveScene`] to it.
///
/// `width` and `height` go unused: a loaded document already carries its own
/// resolved canvas size (P1 — the document is intent, and this document's
/// intent already includes concrete geometry from the Figma capture it was
/// compiled from), unlike [`crate::placeholder_scene`], which derives every
/// offset from the drawable it is given. A resize therefore reloads the same
/// picture rather than rescaling it.
///
/// # Panics
///
/// If the committed golden fails to open or fails the load gate — it never
/// should, since it is frozen and pinned elsewhere, and a panic here is
/// louder than silently painting nothing.
pub fn scene(arena: &mut Arena, _width: u32, _height: u32) -> LiveScene {
    let (document, payloads) =
        dashbuf::open(DOCUMENT).expect("goldens/dsb/v03-paint.dsb is a committed, frozen golden");
    let report = dashscene_validator::validate_document(&document);
    assert!(
        !report.has_errors(),
        "goldens/dsb/v03-paint.dsb fails the load gate: {report:?}"
    );
    dashscene_core::load_document(&document, &payloads, arena);
    dashlang::attach_live(arena, Box::new(TaffySolver::new()))
}

/// No-op: `DOCUMENT` carries no `dashcue` signal or binding rows, and no
/// variant table either — inspected while building this story, and true of
/// every one of the ten committed `goldens/dsb` fixtures today (issue #617)
/// — so there is nothing for a pulse to drive.
///
/// The frame loop still ticks the attached scene every frame regardless:
/// with no live binding and no scheduler track, `LiveScene::tick` takes its
/// idle-frame early return after the first present, and the loop settles and
/// stops painting. That is this story's other finding — the idle skip holds
/// for a loaded document exactly as it does for the placeholder scene.
pub fn pulse(_live: &mut LiveScene, _index: u64) {}
