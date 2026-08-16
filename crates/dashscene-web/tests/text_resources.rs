//! The byte route to a `TextResources` is callable through this crate alone
//! (issue #992).
//!
//! The desktop half of this check is
//! `crates/dashscene-desktop/tests/text_resources.rs`, and the reasoning, the
//! coverage limits and the two notes at the end of its module doc all apply
//! here unchanged — read that file for them rather than a second copy.
//!
//! **One thing differs. Unlike `tests/adapter_accessors.rs`, this file is not
//! gated on `wasm32` and `cargo test` runs it.** That file is gated because
//! `Surface` is compiled for the browser only, so a host-target check of it
//! would check nothing. These re-exports are on every target — the types exist
//! wherever the crate does, and `load_document` taking them is what is gated —
//! so the check runs where the tests run rather than only compiling under
//! `just wasm-lint`.

use dashscene_web::{AtlasBytes, FaceBytes, TextResources, TextResourcesError};

/// One face, with or without a sheet. The font bytes are placeholders — no
/// assertion here gets past `Font::from_bytes`.
fn face(family: &str, with_atlas: bool) -> FaceBytes {
    FaceBytes {
        family: family.to_owned(),
        weight: 400,
        font: vec![0; 4],
        face_index: 0,
        atlas: with_atlas.then(|| AtlasBytes {
            png: vec![0; 4],
            metrics: vec![0; 4],
        }),
    }
}

/// The whole point: the argument is built and the call is made naming nothing
/// but this crate.
///
/// `MixedAtlases` is the reply because the list mixes them, and that check runs
/// before any font or sheet is decoded.
#[test]
fn the_byte_route_is_callable_naming_only_this_crate() {
    let refused = TextResources::from_faces(vec![face("Inter", true), face("Inter Mono", false)]);
    assert!(matches!(refused, Err(TextResourcesError::MixedAtlases)));
}

/// The call is not refused at the door: it groups the faces and gets as far as
/// parsing one.
///
/// Placeholder bytes are not a font, so the reply names the entry that failed
/// rather than a structural complaint about the list. That is the deepest point
/// in the walk reachable without shipping a typeface in this crate, and it is
/// what distinguishes "the route is wired up" from "the route rejects
/// everything".
#[test]
fn the_walk_reaches_the_font_parse() {
    let refused = TextResources::from_faces(vec![face("Inter", false)]);
    assert!(matches!(
        refused,
        Err(TextResourcesError::Font { index: 0, .. })
    ));
}

/// The error is nameable, not merely printable. A route whose failure could
/// only be formatted would leave a caller unable to branch on it, which is the
/// half of "callable" that a `Display` impl does not supply.
#[test]
fn the_refusal_is_a_named_variant_and_a_sentence() {
    let Err(refused) = TextResources::from_faces(Vec::new()) else {
        panic!("no faces at all is refused");
    };
    assert!(matches!(refused, TextResourcesError::NoFaces));
    assert!(!refused.to_string().is_empty());
}

/// This crate's re-exports are the engine's own types, not local ones wearing
/// their names. An identity coercion compiles only if the two paths name one
/// type.
///
/// All four names the engine re-export carries. The facade re-exports two more
/// for text — `Typesetter` and `Atlas`, from `dashscene-typeset` and `dashpaint`
/// — in statements of their own, and the test below pins those by a different
/// mechanism.
#[test]
fn the_re_exports_are_the_engines_own_types() {
    let _resources: fn(dashscene_engine::TextResources) -> TextResources = |resources| resources;
    let _face: fn(dashscene_engine::FaceBytes) -> FaceBytes = |face| face;
    let _atlas: fn(dashscene_engine::AtlasBytes) -> AtlasBytes = |atlas| atlas;
    let _error: fn(dashscene_engine::TextResourcesError) -> TextResourcesError = |error| error;
}

/// The other constructor is reachable too, with **both** its parameter types
/// named through this crate — which is also what pins those two re-exports:
/// the coercion compiles only if they are the types `TextResources::new` takes.
///
/// That matters because the worked example needs it:
/// `corpus/showcase/src/resources.rs` takes `typesetter` from one `from_faces`
/// walk and `atlases` from another, because those two have different lifetimes,
/// and pairs them with `new`. Carrying only the byte route's inputs would leave
/// that shape unwritable through the facade.
///
/// What the facade still does not carry is the vocabulary to build a
/// `Typesetter` or an `Atlas` from parts — that is `dashscene-typeset`'s and
/// `dashpaint`'s, and this coercion says nothing about it.
/// The two halves come back **out** through this crate as well, which the
/// showcase shape needs and no coercion above covers.
///
/// `TextResources` is `#[non_exhaustive]`, so an embedder cannot build one with
/// a struct literal from outside `dashscene-engine` — but reading its fields is
/// what the two-lifetime shape does, taking `typesetter` from one walk and
/// `atlases` from another. Replacing those fields with accessors would leave the
/// route advertised above unwritable while every other test here still passed.
#[test]
fn the_two_halves_are_readable_back_out() {
    let _typesetter: fn(TextResources) -> dashscene_web::Typesetter =
        |resources| resources.typesetter;
    let _atlases: fn(TextResources) -> std::sync::Arc<Vec<dashscene_web::Atlas>> =
        |resources| resources.atlases;
}

#[test]
fn the_pairing_constructor_is_callable_with_both_halves_named_here() {
    let _new: fn(
        dashscene_web::Typesetter,
        std::sync::Arc<Vec<dashscene_web::Atlas>>,
    ) -> TextResources = TextResources::new;
}
