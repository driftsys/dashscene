//! The seam a demonstration producer builds a scene through (story #1342).
//!
//! # Why a seam here rather than a producer here
//!
//! The Unity demonstration draws the `corpus/showcase` scenes, and those scenes
//! are Rust: `showcase::SceneBuilder` is `fn(&mut Arena, u32, u32) -> LiveScene`
//! and their motion is host-driven. No entry point in `include/dashscene.h`
//! mutates a document — signal binding is layer 1 and layer 1 is `v1` for every
//! host (issues #1261, #1262) — so a C# host cannot animate them, and a C#
//! re-authoring would be a second definition of the scenes that drifts from the
//! one `demo-android` draws.
//!
//! **The producer is therefore native.** P3 constrains *when* producer code
//! runs, not what language it is written in, and a native producer that builds
//! the scene into this crate's arena and pulses it inside the tick satisfies it
//! exactly.
//!
//! **What lives here is the seam and not the producer**, because `showcase` is
//! `publish = false` and unpackageable in principle — its `corpus_bytes!` reads
//! paths under `CARGO_MANIFEST_DIR/../../corpus/`, outside its own package
//! directory. This crate is published, and a published crate cannot name an
//! unpublished one: an optional, default-off path dependency on `showcase`
//! fails `just package` at the manifest check, which is version-level and does
//! not consult features. Measured, not assumed — see
//! `docs/decisions/the-demo-producer-links-the-abi-rather-than-shipping-in-it.md`.
//!
//! So the scenes and the `ds_demo_*` entry points live in `unity/demo-producer`,
//! which links this crate as an `rlib`, and this module is what such a producer
//! cannot do from outside: install a scene into the runtime's arena, reach the
//! live scene between ticks, and record a refusal where a host will read it.
//!
//! # What this module does not widen
//!
//! **It exports no C symbol.** All three functions are ordinary Rust, so the shipped
//! `cdylib` exports the same set with this feature on as with it off — the
//! "symbol set that varies by feature" hazard does not arise here, and
//! `just demo-exports` asserts the two sets against each other rather than
//! leaving it to be believed, and CI's `demo-build` job runs it.
//!
//! **It adds no dependency.** Every type named below is one this crate already
//! takes.
//!
//! **It is feature-gated all the same**, because it makes the arena reachable.
//! A consumer building the default feature set cannot open a document and write
//! into it, which is P1's boundary and not a detail of this demonstration.

use std::sync::Arc;

use dashlang::LiveScene;
use dashpaint::Atlas;
use dashscene_core::Arena;

use crate::{
    DsRuntime, DsStatus, announce_document_replaced, drop_document, guard, on_runtime_committing,
    set_last_error,
};

/// Builds a scene into `runtime`'s arena and installs it as the loaded
/// document.
///
/// `build` returns the scene **and** the atlas set its own solver holds — the
/// same `Arc`, not a copy — so that `ds_runtime_atlas` hands out the sheets the
/// staged runs actually sample. A scene whose solver carries no typesetter
/// returns an empty set, which is what a text-free document installs too.
///
/// **The atlases come back from the closure rather than being passed in, and
/// that is the whole reason this signature is shaped so.** A caller writing
/// `install_scene(rt, &mut gen, showcase::resources::atlases(), …)` would have
/// its argument evaluated *before* the call, so a panic in it — and
/// `corpus_bytes!` panics on an unreadable corpus file — would unwind out of
/// the caller's `extern "C"` wrapper past this function's guard entirely, which
/// aborts the process. Everything fallible has to be reachable only from
/// inside `build`.
///
/// `out_generation` receives the runtime's document generation after the
/// install. A producer that holds state about which scene it installed must
/// store this beside it and hand it back to [`with_scene`]: a later
/// `ds_runtime_load_document` replaces the arena and bumps the generation, and
/// nothing else would tell an out-of-crate producer that its scene is gone.
///
/// # The sequence is `load_into`'s, with one deliberate difference
///
/// Drop the previous document, build, install the scene, install the atlases,
/// announce the replacement. The announcement in particular is copied rather
/// than re-derived: it is the only notice an attached painter gets that the
/// arena's generations restarted, and a producer that skipped it would leave
/// the painter describing the outgoing document.
///
/// **Where it differs is the placement of the drop, and `load_into` is
/// emphatic about that placement.** That function raises every failure it can
/// *before* `drop_document`, so a refused load leaves the previously loaded
/// document drawable — its own note says so, `drop_document`'s "Where a caller
/// puts it" section says so, and `a_refused_byte_load_leaves_the_loaded_document_drawable`
/// pins it. This function drops first, because it has nothing fallible to raise
/// first: a scene builder is a function pointer, not a parse and a validation,
/// so there is no refusal to hoist above the drop. The consequence is stated in
/// the panic section below rather than left to be discovered.
///
/// # A panic in `build` is contained; one outside it is not
///
/// The whole body runs under `guard`, so a scene builder that panics yields
/// [`DsStatus::Panic`] rather than unwinding into whatever called the producer's
/// `extern "C"` wrapper — which, being a non-unwinding ABI, would abort the
/// process. The runtime is then left with no document, because the drop above
/// has already run, and every later call reports [`DsStatus::NoDocument`].
/// That differs from a refused load, which leaves the previous document
/// drawable.
///
/// **Only what `build` reaches is covered.** Anything a caller evaluates to
/// produce the arguments runs before this function is entered. That is why the
/// atlases arrive from inside the closure.
pub fn install_scene(
    runtime: DsRuntime,
    out_generation: &mut u64,
    build: impl FnOnce(&mut Arena) -> (LiveScene, Arc<Vec<Atlas>>),
) -> DsStatus {
    guard(|| {
        on_runtime_committing(runtime, "dashscene_ffi::demo::install_scene", |runtime| {
            drop_document(runtime);
            let (live, atlases) = build(&mut runtime.arena);
            runtime.scene = Some(live);
            runtime.atlases = atlases;
            announce_document_replaced(runtime);
            *out_generation = runtime.document_generation;
            DsStatus::Ok
        })
    })
}

/// Runs `f` against the installed scene and the arena it was built into.
///
/// This is what a scripted pulse and a variant switch both need: `ScenePulse`
/// takes the live scene, and `SceneAction` takes both, because `Txn::set_variant`
/// is an arena mutation with no signal equivalent.
///
/// `generation` is what [`install_scene`] returned. It is checked against the
/// runtime's current one, so a `ds_runtime_load_document` between the install
/// and this call is refused rather than silently driving the producer's script
/// against a document it knows nothing about. Without it a variant switch would
/// run `Txn::set_variant` against a foreign arena — which succeeds where that
/// arena happens to carry a set at the same ordinal, and panics where it does
/// not.
///
/// `what` names the caller in a refusal, the way every entry point in this crate
/// names itself.
///
/// # It commits nothing
///
/// A write staged here is visible to the next `ds_runtime_tick`, which is the
/// commit. That is P3 working: the producer mutates, and the runtime owns time.
/// It goes through `on_runtime_committing` regardless — a staged write that
/// outlived a frame lease would be committed under it by that tick, so the
/// refusal belongs here and not one call later.
pub fn with_scene(
    runtime: DsRuntime,
    generation: u64,
    what: &str,
    f: impl FnOnce(&mut LiveScene, &mut Arena),
) -> DsStatus {
    guard(|| {
        on_runtime_committing(runtime, what, |runtime| {
            let Some(scene) = runtime.scene.as_mut() else {
                set_last_error(format!("{what}: no document loaded"));
                return DsStatus::NoDocument;
            };
            if runtime.document_generation != generation {
                set_last_error(format!(
                    "{what}: this runtime's document has been replaced since the scene was \
                     installed (generation {} against {generation}), so the scene this call \
                     names is no longer the one loaded",
                    runtime.document_generation
                ));
                return DsStatus::NoDocument;
            }
            f(scene, &mut runtime.arena);
            DsStatus::Ok
        })
    })
}

/// Records `message` as the last error and returns `status`, so a producer's
/// refusals reach `ds_last_error_message` like every refusal this crate makes
/// itself.
///
/// Without it a producer outside this crate can return a [`DsStatus`] and no
/// diagnostic — and the convention every indexable refusal here follows is to
/// name the index that was asked for and the count that exists, which is the
/// half a bare status cannot carry.
pub fn refuse(status: DsStatus, message: impl Into<String>) -> DsStatus {
    set_last_error(message);
    status
}
