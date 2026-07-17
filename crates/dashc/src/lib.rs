//! Compiler CLI: Figma importer orchestration target, Figma-to-dashscene lowering, diagnostics, .dsb emission. Also builds to wasm32-unknown-unknown for the Deno importer (docs/design/architecture.md, docs/design/dashc.md).
//!
//! Same Rust code path whether invoked natively (CI, the `dashc` CLI) or
//! compiled to wasm32-unknown-unknown and called from the Deno Figma
//! importer (`importers/figma/`) — no reimplementation, see
//! docs/decisions/figma-importer-deno-plus-dashc-wasm.md.
//!
//! # The pipeline
//!
//! ```text
//!   source             →  lower  →  Document  →  emit  →  validate  →  .dsb
//!   (Figma REST JSON)                (in-memory document)
//! ```
//!
//! The IR is the dashscene document; `.dsb` is its file extension
//! (`docs/decisions/dashscene-document-is-the-ir.md`).
//!
//! [`Document`] is the in-memory document — what a producer lowers *into* and
//! what [`emit`] writes *out of*. Its paint types are `dashpaint`'s, so one
//! paint vocabulary spans the document, the runtime, and the painter, and a
//! lowering cannot invent a construct no painter can draw.
//!
//! # The `figma` module
//!
//! The [`figma`] module parses the Figma REST subset, pinned to the v0.3
//! fixture at `corpus/figma-fixtures/v03-paint.json`. Every field shape is
//! real, not guessed (P5). [`figma::lower`] lowers it into [`Document`], and
//! [`compile_figma`] wraps the lowering, emission, and validation into one
//! call.
//!
//! # Emission is gated (P4, R6)
//!
//! [`compile`] emits before it validates — see its own doc comment for why
//! that order, not the reverse — and an **error blocks the document**, never
//! a silent drop. A warning does not block; a strict build refuses it
//! (waivers are v0.7, issue #41).
//!
//! [`compile_figma`] is the headline entry point: it is the only function
//! that merges both gates — the **import gate** (`triage`, over constructs
//! with no `.dsb` representation) and the **load gate** (`validate_document`,
//! over the emitted document) — into one report before deciding whether to
//! emit.

mod document;
mod emit;

// Public because `tests/abi.rs` calls the exports directly: that native test is
// what pins the wire format, so the module cannot be private.
pub mod abi;
pub mod figma;

pub use document::{
    AxisSizing, Binding, BindingChannel, BindingTransform, Box2D, CrossAxisAlign, Document,
    EdgeInsets, GridTrack, LayoutConstraints, LayoutContainer, LayoutMode, MainAxisAlign, Node,
    Paint, SignalDecl, TextStyle,
};
pub use emit::emit;
// `CompileError` only: it is `compile_figma`'s error type, so it belongs at the
// root beside it. The lowering and its REST types stay behind `figma::` —
// re-exporting them here would give one item two public names.
pub use figma::CompileError;

use std::collections::BTreeMap;

use dashpaint::ImageAsset;
use dashscene_validator::{Diagnostic, Location, Profile, Report, Severity};

/// The diagnostic rules the document gate itself assembles — verdicts
/// about a `Document` no serialized form can carry, so the load gate
/// (which sees only serialized documents) can never raise them.
pub mod rule {
    /// A binding carries a `Custom` transform. The closure it references
    /// lives in a producer-side table and does not serialize, so emitting
    /// the row without it would silently change what the binding computes
    /// — the drop P4 forbids. `Custom` stays a `dashlang`-only escape
    /// hatch (`docs/decisions/reactive-layer-home-and-staging.md`).
    pub const CUSTOM_TRANSFORM: &str = "binding.custom-transform";
}

/// Emits `doc`, then runs the load gate over the emitted document.
///
/// Shared by [`compile`] and [`compile_figma`]: both need "emit, then
/// validate what was actually emitted" (see `compile`'s doc comment for why
/// the order is emit-then-validate, not the reverse).
fn emit_and_validate(doc: &Document) -> (Vec<u8>, Report) {
    // The document gate: a `Custom` binding transform cannot serialize
    // (its closure lives producer-side), so it is refused by name before
    // the emitter — which panics on it — ever runs. Nothing is emitted:
    // both callers block on an error report, so no bytes escape.
    let custom_gate: Report = doc
        .bindings
        .iter()
        .enumerate()
        .filter_map(|(i, row)| match row.transform {
            document::BindingTransform::Custom(id) => Some(Diagnostic {
                rule: rule::CUSTOM_TRANSFORM,
                severity: Severity::Error,
                at: Location::Binding(i as u32),
                message: format!(
                    "binding {i} carries a Custom transform (closure {id}), which does not \
                     serialize; keep Custom bindings producer-side or use the declarative \
                     vocabulary (Identity, Scale, MapRange, Clamp)"
                ),
            }),
            _ => None,
        })
        .collect();
    if custom_gate.has_errors() {
        return (Vec::new(), custom_gate);
    }

    let bytes = emit(doc);

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
    (bytes, report)
}

/// Emits a [`Document`] as `.dsb` bytes, or refuses with the diagnostics that
/// block it.
///
/// This is the gate docs/design/architecture.md describes: an error blocks the document.
/// (The seed design spelled the extension `.scb`, the working name the seed document
/// used; docs/decisions/dsb-format-and-one-schema.md retired it in favour of `.dsb`.)
///
/// The document is emitted first and validated **as a document**, not as a
/// `Document`: the load gate's rules are about the serialized index model — a
/// dangling `paint_entry`, an unknown enum value — so validating a shape the
/// emitter has not produced yet would check something other than what ships.
/// The bytes are discarded if the report has errors, so nothing invalid ever
/// escapes.
///
/// The returned bytes are byte-reproducible for a given `Document` (R7).
pub fn compile(doc: &Document) -> Result<Vec<u8>, Report> {
    let (bytes, report) = emit_and_validate(doc);

    if report.has_errors() {
        return Err(report);
    }
    Ok(bytes)
}

/// Compiles Figma REST JSON to a `.dsb`.
///
/// Two gates, one report. The **import gate** (`triage`) runs while lowering,
/// on constructs that have no representation in the `.dsb` schema at all; the
/// **load gate** (`validate_document`) runs on the emitted document. An error
/// from either blocks emission (R6). Warnings do not block, so they come back
/// with the bytes — dropping them on the success path would be the silent drop
/// P4 forbids.
///
/// `images` resolves the `imageRef` of every image fill; see `figma::lower`.
pub fn compile_figma(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Vec<u8>, Report), CompileError> {
    compile_figma_with_bindings(json, profile, images, &[])
}

/// [`compile_figma`], plus the joined variable-binding rows the importer
/// produced (story #167; `docs/decisions/token-resolution-phase-split.md`).
///
/// Each row is one `boundVariables` site the importer joined against the
/// plugin-exported vartable: the Figma node id, the property path, the
/// mode-qualified signal name, and the mode-resolved value. The lowering
/// turns the supported sites into `Document.signals`/`Document.bindings`
/// (`itemSpacing` becomes a `Gap` binding; a solid fill's color becomes
/// four `Fill` channel bindings); a site the channel vocabulary cannot
/// carry yet is a named warning — the resolved literal still ships, so
/// the picture is right and nothing is dropped in silence (P4).
pub fn compile_figma_with_bindings(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bindings: &[figma::BoundVariable],
) -> Result<(Vec<u8>, Report), CompileError> {
    let file = figma::parse_file(json)?;
    let (doc, found) = figma::lower_with_bindings(&file, profile, images, bindings)?;

    let mut report: Report = found.into_iter().collect();

    let (bytes, load_report) = emit_and_validate(&doc);
    report.extend(load_report.diagnostics().iter().cloned());

    if report.has_errors() {
        return Err(CompileError::Diagnostics(report));
    }
    Ok((bytes, report))
}
