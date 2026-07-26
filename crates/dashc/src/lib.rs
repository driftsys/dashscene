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
    Asset, AssetKind, AxisSizing, Binding, BindingChannel, BindingTransform, Box2D, CrossAxisAlign,
    Document, EdgeInsets, GridTrack, LayoutConstraints, LayoutContainer, LayoutMode, MainAxisAlign,
    Node, Paint, SignalDecl, TextAlign, TextAlignV, TextStyle,
};
pub use emit::emit;
// `CompileError` only: it is `compile_figma`'s error type, so it belongs at the
// root beside it. The lowering and its REST types stay behind `figma::` —
// re-exporting them here would give one item two public names.
pub use figma::CompileError;

use std::collections::BTreeMap;

use dashbuf::bank;
use dashpaint::ImageAsset;
use dashscene_validator::{Diagnostic, Location, Profile, Report, Severity};

/// How the Figma front end treats a construct the document cannot express.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitPolicy {
    /// All-or-nothing: any vocabulary gap refuses the whole file (R6, the
    /// original `unsupported-figma-constructs-refuse-the-compile.md` posture).
    Strict,
    /// Skip an unsupported node with a warning, still emit. Never approximates:
    /// a construct that could only ship approximately still refuses.
    Partial,
}

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

    // The same gate for an asset with no payload. Before story #107 the
    // document carried image bytes, so the load gate could see an empty one and
    // named it `asset.image-no-bytes`. The document now carries identity and
    // metadata, so that gate cannot see bytes at all — and without this check an
    // empty payload reaches the container writer, which refuses it, through an
    // `expect`. A named diagnostic becoming a panic is exactly what P4 forbids,
    // so the check moves here, where the bytes still are.
    let asset_gate: Report = doc
        .assets
        .iter()
        .enumerate()
        .filter(|(_, asset)| asset.bytes.is_empty())
        .map(|(i, _)| Diagnostic {
            rule: dashscene_validator::rule::IMAGE_NO_BYTES,
            severity: Severity::Error,
            at: Location::ImageAsset(i as u32),
            message: "asset carries no bytes; a painter cannot decode it, and the file format                       has no empty section"
                .to_owned(),
        })
        .collect();
    if asset_gate.has_errors() {
        return (Vec::new(), asset_gate);
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

    let mut report = dashscene_validator::validate_document(&document);

    // The load gate's asset half, run over what was just emitted (story #437,
    // debt #416). `emit` builds the entry vector by mapping over `doc.assets`
    // in order, so the payload list below pairs with the entries positionally,
    // which is the pairing `validate_asset_payloads` documents.
    //
    // Running it here rather than only in `dashc check` makes the compiler
    // check its own output before the bytes leave it, and it is not a
    // tautology: the Figma image path records the extent `identify` read, but
    // the MSDF vector atlas path (`figma::Lowering`) records the extent the
    // bake asked for, on a PNG this crate encodes. Those are two independent
    // derivations of one number, and this is what would notice them parting.
    let payloads: Vec<&[u8]> = doc.assets.iter().map(|a| a.bytes.as_slice()).collect();
    // The pairing is positional, so a length disagreement means `emit` started
    // reordering or dropping assets. That is an internal invariant break, and
    // without this it would surface as false `asset.format-mismatch` errors
    // blaming the document for comparing the wrong pairs.
    debug_assert_eq!(
        payloads.len(),
        document.assets().map_or(0, |assets| assets.len()),
        "emit must map one asset entry per doc.assets entry, in order"
    );
    report.extend(
        dashscene_validator::validate_asset_payloads(&document, &payloads)
            .diagnostics()
            .iter()
            .cloned(),
    );

    (bytes, report)
}

/// Assembles an emitted ui document and its assets into a `.dsb` file.
///
/// This is the one place the compiler crosses from "a flatbuffer" to "a file"
/// (`docs/design/dsb-container-format.md`). Everything above it — the emitter,
/// the flatbuffers verifier, the load gate — works on the section payload,
/// because that is what those things are about. Only the bytes that leave the
/// compiler are a file.
///
/// `dashc` compiles the **RAW** profile, and only RAW: it is deterministic and
/// lossless, and every lossy step belongs to the packer
/// (`docs/decisions/asset-quality-profile-naming.md`). RAW is the null binding,
/// so the bank below is the identity map over the canonical payloads and the
/// file carries the imported bytes unchanged.
///
/// The `expect` cannot fire, on any of [`bank::AssembleError`]'s four arms:
///
/// - `Document` — `emit_and_validate` ran the flatbuffers verifier over these
///   same bytes and both callers return early on an error report;
/// - `Unbound` — the bank is built from the same `doc.assets` the emitter wrote
///   one entry per, hashed the same way, so every entry hash is in it;
/// - `UnusedPayloads` — `Document::push_asset` deduplicates by content hash, so
///   `doc.assets` holds no repeated payload and every binding is named once;
/// - `Write` — an empty payload is the only reachable arm, and `dashc`'s asset
///   gate refuses one before this point, with a named diagnostic rather than a
///   panic.
fn package(ui_section: &[u8], assets: &[document::Asset]) -> Vec<u8> {
    let cold = bank::ColdBank::raw(assets.iter().map(|asset| asset.bytes.as_slice()));
    bank::assemble(ui_section, &cold).expect(
        "the document and its RAW bank assemble: the ui section verified above, the bank is          the identity map over the same assets the emitter wrote entries for, and an asset with          no bytes cannot pass dashc's image gate",
    )
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
/// The returned bytes are a complete `.dsb` **file** — the sectioned envelope
/// with the emitted document as its one structured section
/// (`docs/design/dsb-container-format.md`). A caller that wants the document
/// rather than the file reads it back with `dashbuf::container::ui_document`.
///
/// They are byte-reproducible for a given `Document` (R7), envelope included:
/// the container writer is a pure function of its input, with zero-filled
/// alignment gaps and content hashes that depend on content alone.
pub fn compile(doc: &Document) -> Result<Vec<u8>, Report> {
    let (bytes, report) = emit_and_validate(doc);

    if report.has_errors() {
        return Err(report);
    }
    Ok(package(&bytes, &doc.assets))
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
///
/// The bytes are a complete `.dsb` file, as [`compile`]'s are.
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
    compile_figma_with_bindings_and_policy(json, profile, images, bindings, EmitPolicy::Strict)
}

/// [`compile_figma_with_bindings`], choosing the emit policy
/// (`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`).
///
/// Under [`EmitPolicy::Strict`] any vocabulary gap is an error that withholds
/// the bytes (R6). Under [`EmitPolicy::Partial`] an omission-class gap
/// (`figma.unsupported`) is a warning instead — the node is still skipped, so
/// the document emits with the gap riding back as a warning, never a silent
/// drop (P4). An approximation-if-shipped construct (a REJECT-band feature on a
/// lowered node) and `figma.no-content` stay fatal in both modes.
pub fn compile_figma_with_bindings_and_policy(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bindings: &[figma::BoundVariable],
    policy: EmitPolicy,
) -> Result<(Vec<u8>, Report), CompileError> {
    let file = figma::parse_file(json)?;
    let (doc, found) =
        figma::lower_with_bindings_and_policy(&file, profile, images, bindings, policy)?;

    let mut report: Report = found.into_iter().collect();

    let (bytes, load_report) = emit_and_validate(&doc);
    report.extend(load_report.diagnostics().iter().cloned());

    if report.has_errors() {
        return Err(CompileError::Diagnostics(report));
    }
    Ok((package(&bytes, &doc.assets), report))
}
