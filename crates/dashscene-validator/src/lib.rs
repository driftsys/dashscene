//! Shared validation crate: paint-vocabulary profiles, diagnostics, waivers (docs/specification/02-principles.md P4, docs/design/architecture.md).
//!
//! P4 — "vocabulary is validated, never discovered" — needs three gates,
//! because the three producer surfaces carry genuinely different
//! information:
//!
//! | gate | entry point | answers |
//! |---|---|---|
//! | import | [`triage`] | is this source construct in the target's profile? (docs/specification/04-figma-vocabulary-profile.md) |
//! | load | [`validate_document`] | is this `.dsb` internally consistent? |
//! | paint | [`validate_scene`] | does this solved scene stay inside painter budgets? |
//!
//! They are not interchangeable. A `.dsb` document cannot carry an
//! out-of-profile construct — by the time a construct is in the schema it
//! is in the vocabulary — so the triage runs on the *producer's* source
//! vocabulary. Conversely a solved scene has no indices left to dangle
//! (`docs/decisions/boundary-b-unification.md`), while a document has no
//! resolved boxes to measure a stroke against (P1). See
//! `docs/design/dashscene-validator.md`.
//!
//! The validator owns the verdict, never the source format: P5 —
//! "Figma compatibility is a property of one producer" — so a producer
//! maps its own vocabulary onto [`Construct`] and asks for the verdict
//! here.
//!
//! ```
//! use dashscene_validator::{Construct, NodePath, Profile, Severity, rule, triage};
//!
//! let scrim = NodePath::new(7, "/card/scrim");
//!
//! // Backdrop blur is profile:full-only (docs/specification/04-figma-vocabulary-profile.md): a lean painter
//! // never gets it, so under profile:core it blocks the document.
//! let d = triage(Construct::BackdropBlur, Profile::Core, scrim.clone());
//! assert_eq!(d.rule, rule::BACKDROP_BLUR);
//! assert_eq!(d.severity, Severity::Error);
//!
//! // Under profile:full it is deferred vocabulary with a declared
//! // degrade — a warning, which a strict build still refuses.
//! let d = triage(Construct::BackdropBlur, Profile::Full, scrim);
//! assert_eq!(d.severity, Severity::Warning);
//! ```

mod document;
mod paint;
mod scene;
mod triage;

pub use document::validate_document;
pub use scene::validate_scene;
pub use triage::{Construct, triage};

use std::fmt;

/// The stable, greppable diagnostic ids. A diagnostic a designer sees has
/// to be searchable, so rule ids are strings, not numbers.
pub mod rule {
    // Import gate — docs/specification/04-figma-vocabulary-profile.md's LATER (warn) band.
    pub const LAYER_BLUR: &str = "profile.layer-blur";
    pub const BACKDROP_BLUR: &str = "profile.backdrop-blur";
    pub const ADVANCED_BLEND_MODE: &str = "profile.advanced-blend-mode";
    pub const CORNER_SMOOTHING: &str = "profile.corner-smoothing";
    pub const LUMINANCE_MASK: &str = "profile.luminance-mask";
    pub const CLIP_ON_ROTATED: &str = "profile.clip-on-rotated";
    pub const KASHIDA_JUSTIFICATION: &str = "profile.kashida-justification";

    // Import gate — docs/specification/04-figma-vocabulary-profile.md's REJECT (error) band.
    pub const NOISE_OR_TEXTURE_EFFECT: &str = "profile.noise-or-texture-effect";
    pub const PROGRESSIVE_BLUR: &str = "profile.progressive-blur";
    pub const ANIMATED_BOOLEAN_OP: &str = "profile.animated-boolean-op";
    pub const ANIMATED_VARIABLE_FONT_AXIS: &str = "profile.animated-variable-font-axis";

    // Load gate — document referential integrity (issue #63).
    pub const PARENT_OUT_OF_RANGE: &str = "node.parent-out-of-range";
    pub const PARENT_NOT_BEFORE_CHILD: &str = "node.parent-not-before-child";
    pub const PAINT_ENTRY_OUT_OF_RANGE: &str = "paint.entry-out-of-range";
    pub const CONFLICTING_PAINT_REPRESENTATION: &str = "paint.conflicting-representation";
    pub const TEXT_STRING_OUT_OF_RANGE: &str = "text.string-out-of-range";
    pub const TEXT_STYLE_OUT_OF_RANGE: &str = "text.style-out-of-range";
    /// A text style whose `color` is absent. The schema makes it optional (a
    /// struct field in a table always is), so a producer can omit it — and a
    /// consumer that invents a default has silently discovered vocabulary,
    /// which is what P4 forbids.
    pub const TEXT_STYLE_NO_COLOR: &str = "text.style-no-color";

    // Load gate — the append-only enum range check. The schema's own
    // contract: "a reader built before an append receives the unknown
    // value as a raw integer — the load gate must range-check and emit a
    // named diagnostic (P4/R6), never default silently."
    pub const UNKNOWN_ENUM: &str = "vocabulary.unknown-enum";

    // Paint vocabulary — checked on both a document and a solved scene
    // (issues #100, #63).
    pub const GRADIENT_NO_STOPS: &str = "paint.gradient.no-stops";
    pub const GRADIENT_STOP_BUDGET: &str = "paint.gradient.stop-budget";
    pub const GRADIENT_STOP_OFFSET_INVALID: &str = "paint.gradient.stop-offset-invalid";
    pub const GRADIENT_STOP_ORDER: &str = "paint.gradient.stop-order";
    pub const STROKE_INVALID_WIDTH: &str = "paint.stroke.invalid-width";
    pub const IMAGE_OUT_OF_RANGE: &str = "paint.image-out-of-range";

    // Image assets — the painter decodes them behind an `expect` documented
    // as "validated upstream (P4)". This is that upstream.
    pub const IMAGE_NO_BYTES: &str = "asset.image-no-bytes";

    // Paint gate — needs the solved box, so it exists only on a scene.
    pub const STROKE_EXCEEDS_BOX: &str = "paint.stroke.exceeds-box";

    // Paint gate — the resolved clip regions core computes at commit
    // (issue #97). They exist only on a scene: the document carries clip
    // *intent* (`Paint.clip`), never the resolved ancestor-intersected
    // region (P1).
    pub const CLIP_INDEX_OUT_OF_RANGE: &str = "clip.index-out-of-range";
}

/// The gradient stop budget this validator enforces, re-exported so a
/// caller never has to name a second `8`.
///
/// It lives on boundary B (`dashpaint`) precisely so that the painter that
/// panics above it and the validator that rejects it upstream cannot drift
/// apart — the guarantee "a validated scene never trips the painter's stop
/// assertion" is only true while both read the same constant.
pub use dashpaint::MAX_GRADIENT_STOPS;

/// A named paint-vocabulary subset a target honors (docs/design/architecture.md, R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Lean / native painters: the subset a fixed-vocabulary painter can
    /// honor without a render-target round-trip.
    Core,
    /// Unity-class: everything `Core` honors, plus the constructs
    /// docs/specification/04-figma-vocabulary-profile.md annotates `(profile:full)`.
    Full,
}

/// docs/design/architecture.md: an `Error` blocks the document; a `Warning` is deferred
/// vocabulary with a declared degrade. Release builds run strict — zero
/// warnings, or an explicit waiver entry (waivers are v0.7, issue #41).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Warning,
    Error,
}

/// A node's identity: the document DFS index — which is the rect-table
/// index too (docs/design/dashbuf.md) — and its name path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePath {
    pub index: u32,
    /// Slash-joined ancestor names, e.g. `/card/badge`. Empty when the
    /// surface carries no names — boundary B does not.
    pub path: String,
}

impl NodePath {
    pub fn new(index: u32, path: impl Into<String>) -> Self {
        Self {
            index,
            path: path.into(),
        }
    }

    /// A path for a surface that carries no names, e.g. a committed scene.
    pub fn unnamed(index: u32) -> Self {
        Self {
            index,
            path: String::new(),
        }
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "#{}", self.index)
        } else {
            write!(f, "{} (#{})", self.path, self.index)
        }
    }
}

/// What a diagnostic points at.
///
/// Not everything the validator reports is a node. A pooled paint entry and
/// an image asset are shared by every node that references them, so each is
/// reported **once, at its own index** — repeating one authoring mistake per
/// referencing node would bury the rest of the report. Their indices are
/// pool indices, not DFS node indices, and this enum is what keeps them from
/// being mistaken for one: a consumer that resolves a diagnostic to a layer
/// (an editor jumping to it, issue #41's waiver machinery keying on it) must
/// not silently land on an unrelated node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// A node in the document, by DFS index and name path.
    Node(NodePath),
    /// An entry of the paint pool, by its index in `Document.paints` /
    /// `PaintTable`.
    PaintEntry(u32),
    /// An image asset, by its index in `Document.images` / `ImageTable`.
    ImageAsset(u32),
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(path) => write!(f, "{path}"),
            Self::PaintEntry(index) => write!(f, "<paint pool #{index}>"),
            Self::ImageAsset(index) => write!(f, "<image asset #{index}>"),
        }
    }
}

/// One named diagnostic (docs/design/architecture.md: `{rule id, node path, severity}`).
///
/// The workaround hint docs/design/architecture.md also names is v0.7 scope (issue #41),
/// alongside waivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub at: Location,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(
            f,
            "{severity}[{}] at {}: {}",
            self.rule, self.at, self.message
        )
    }
}

/// Every diagnostic one gate produced, in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the document is blocked — docs/design/architecture.md: an error blocks
    /// emission, a warning does not.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Whether a strict build passes: zero diagnostics of any severity.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Whether any diagnostic carries this rule id. Keeps rule ids the
    /// thing callers and tests pin, rather than message text.
    pub fn has(&self, rule: &str) -> bool {
        self.diagnostics.iter().any(|d| d.rule == rule)
    }

    /// The first diagnostic carrying this rule id, for asserting on where it
    /// points.
    pub fn find(&self, rule: &str) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.rule == rule)
    }

    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

/// A producer assembles its own findings into a `Report`.
///
/// The import gate (`triage`) hands back one `Diagnostic` at a time, and the
/// producer that owns the Figma mapping (`dashc`, P5) is the only code that
/// knows when it is done finding them. Without this, a producer could triage
/// a construct and then have no way to report it — a silent drop by
/// construction, which P4 forbids.
impl FromIterator<Diagnostic> for Report {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        Self {
            diagnostics: iter.into_iter().collect(),
        }
    }
}

/// Merges one gate's diagnostics into another's — `dashc` folds the load
/// gate's `Report` into the import gate's before deciding whether to emit.
impl Extend<Diagnostic> for Report {
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, iter: I) {
        self.diagnostics.extend(iter);
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for diagnostic in &self.diagnostics {
            writeln!(f, "{diagnostic}")?;
        }
        Ok(())
    }
}
