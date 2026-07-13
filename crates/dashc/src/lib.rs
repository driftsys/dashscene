//! Compiler CLI: Figma importer orchestration target, Figma-to-SCD lowering, diagnostics, .dsb emission. Also builds to wasm32-unknown-unknown for the Deno importer (DESIGN_1.md §4, §6.1).
//!
//! Same Rust code path whether invoked natively (CI, the `dashc` CLI) or
//! compiled to wasm32-unknown-unknown and called from the Deno Figma
//! importer (`importers/figma/`) — no reimplementation, see
//! SCOPE_DECISIONS.md §4.
//!
//! # The pipeline
//!
//! ```text
//!   source             →  lower  →  Scd  →  validate  →  emit  →  .dsb
//!   (Figma REST JSON)                (in-memory document)
//! ```
//!
//! [`Scd`] is the in-memory document — what a producer lowers *into* and
//! what [`emit`] writes *out of*. Its paint types are `dashpaint`'s, so one
//! paint vocabulary spans the document, the runtime, and the painter, and a
//! lowering cannot invent a construct no painter can draw.
//!
//! # The Figma front end is not here yet
//!
//! Lowering Figma REST JSON into [`Scd`] needs a captured fixture to build
//! against, and the v0.3 fixture has not been captured —
//! `corpus/figma-fixtures/` holds only its manifest. Guessing at the REST
//! shape would build the lowering against a fiction, and P5 makes Figma
//! fidelity this producer's whole purpose. So this slice ships the half that
//! does not depend on the fixture: the SCD model, the deterministic emitter,
//! the emission gate, and the validated round trip through `dashscene-core`
//! and the reference painter.
//!
//! # Emission is gated (P4, R6)
//!
//! [`compile`] validates before it emits, and an **error blocks the
//! document** — never a silent drop. A warning does not block; a strict
//! build refuses it (waivers are v0.7, issue #41).

mod emit;
mod scd;

pub use emit::emit;
pub use scd::{Box2D, Paint, Scd, ScdNode};

use dashscene_validator::Report;

/// Emits an [`Scd`] as `.dsb` bytes, or refuses with the diagnostics that
/// block it.
///
/// This is the gate DESIGN §5 describes: "error blocks .scb".
///
/// The document is emitted first and validated **as a document**, not as an
/// `Scd`: the load gate's rules are about the serialized index model — a
/// dangling `paint_entry`, an unknown enum value — so validating a shape the
/// emitter has not produced yet would check something other than what ships.
/// The bytes are discarded if the report has errors, so nothing invalid ever
/// escapes.
///
/// The returned bytes are byte-reproducible for a given `Scd` (R7).
pub fn compile(scd: &Scd) -> Result<Vec<u8>, Report> {
    let bytes = emit(scd);

    // The flatbuffer verifier runs over the emitter's own output. That is
    // deliberate, and it is not the hot path: `compile` is a build step, so
    // paying O(bytes) to prove the emitter did not produce a malformed buffer
    // is a good trade. (Skipping it in release via
    // `root_as_document_unchecked` was considered and rejected: it buys build
    // time with an `unsafe` block, which is the wrong currency for a
    // compiler.)
    //
    // The `expect` is right too: a malformed buffer here is this crate's own
    // invariant broken, not bad input — bad *input* is what the report below
    // is for.
    let document = dashbuf::root_as_document(&bytes)
        .expect("the emitter always produces a structurally valid buffer");

    let report = dashscene_validator::validate_document(&document);

    if report.has_errors() {
        return Err(report);
    }
    Ok(bytes)
}
