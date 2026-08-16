//! The byte route to a `TextResources` is callable through this crate alone
//! (issue #992).
//!
//! The crate re-exported `TextResources` beside `Typesetter` and `Atlas` so an
//! embedder could **name** the parameter its load paths take. Naming was as far
//! as it went: `TextResources::from_faces` resolved through this crate, and
//! calling it did not, because `FaceBytes` did not cross and the argument is a
//! `Vec` of them. `AtlasBytes` is needed for any face that carries a sheet —
//! `FaceBytes::atlas` is an `Option`, so a measure-only cascade can be written
//! with `atlas: None` and never name it — and a facade that carried one without
//! the other would reach only that half.
//!
//! What is checked here is the property that fixes, not the prose: **every type
//! the byte route needs is reachable from outside this crate, through it.**
//! Nothing below names `dashscene_engine` except the identity coercions, whose
//! whole job is to name both paths — a coercion against this crate's alias
//! alone would still pass if the re-export were replaced by a local type
//! wearing the same name, which is the one substitution that would make the
//! argument for re-exporting false. That is the reasoning
//! `tests/adapter_accessors.rs` gives for its own pairs.
//!
//! # What these do not cover
//!
//! **No test here assembles a `TextResources` successfully**, and nothing in
//! this crate can: a cascade needs real font bytes and a committed sheet, and
//! this crate ships neither. All three `from_faces` calls below are refusals,
//! and the deepest of them reaches `Font::from_bytes` before failing — as far
//! into the walk as placeholder bytes go, and what keeps every assertion here
//! deterministic.
//!
//! So a change that broke the family grouping past that point, the atlas
//! conversion, or the slot ordering would leave these green. Those are
//! `dashscene-engine`'s to pin, beside the code that performs them, and it does.
//! What this file is for is the seam: that the call is reachable and its types
//! nameable from outside. `corpus/showcase` exercises the route on real bytes
//! but reaches `dashscene-engine` directly, so it is not a witness for anything
//! about this facade.
//!
//! # Two notes on the file itself
//!
//! **The doc links are backticks.** Nothing resolves intra-doc links here:
//! `just doc-links` is `cargo doc --workspace`, which does not build test
//! targets at all. Issue #1116 named the neighbouring gap — `#[cfg(test)]`
//! modules inside a documented crate — and was closed as not planned, because
//! `cargo doc` cannot reach those either. A `tests/` target is a third case
//! again, and no gate covers it, so backticks are what keeps this file honest
//! rather than a lint.
//!
//! **The web twin repeats this file's body, deliberately.** The property is
//! per-crate — it is about what *this* crate re-exports — so a shared helper
//! would be a third thing to keep in step with two facades that are allowed to
//! diverge. `tests/adapter_accessors.rs` is also per-crate, though its two
//! halves are not copies of each other; that pairing is precedent for the split,
//! not for the wording.

use dashscene_desktop::{AtlasBytes, FaceBytes, TextResources, TextResourcesError};

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
    let _typesetter: fn(TextResources) -> dashscene_desktop::Typesetter =
        |resources| resources.typesetter;
    let _atlases: fn(TextResources) -> std::sync::Arc<Vec<dashscene_desktop::Atlas>> =
        |resources| resources.atlases;
}

#[test]
fn the_pairing_constructor_is_callable_with_both_halves_named_here() {
    let _new: fn(
        dashscene_desktop::Typesetter,
        std::sync::Arc<Vec<dashscene_desktop::Atlas>>,
    ) -> TextResources = TextResources::new;
}
