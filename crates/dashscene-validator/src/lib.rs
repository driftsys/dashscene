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
mod waiver;

pub use document::validate_document;
pub use scene::validate_scene;
pub use triage::{Construct, triage};
pub use waiver::{StrictReport, Waiver};

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
    /// A stroke whose width varies along its length (a 2025 Figma Draw
    /// effect). REJECT-band — no paint entry can express a per-length width,
    /// so it is baked or dropped, never degraded (issue #145;
    /// `docs/archive/2026-07-14-scope-decisions.md` §8).
    pub const VARIABLE_WIDTH_STROKE: &str = "profile.variable-width-stroke";

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
    /// A text style whose `weight` is outside the CSS scale 100..=900 the
    /// schema pins (`dashbuf.fbs`, `TextStyle.weight`). Font selection would
    /// otherwise clamp silently or pick an unintended face — the silent
    /// vocabulary drop P4 forbids (issue #129).
    pub const TEXT_STYLE_WEIGHT_OUT_OF_RANGE: &str = "text.style-weight-out-of-range";

    // Load gate — the v0.4 variant table (issue #20).
    pub const VARIANT_OVERRIDE_NODE_OUT_OF_RANGE: &str = "variant.override-node-out-of-range";
    pub const VARIANT_SET_NO_MEMBERS: &str = "variant.set-no-members";
    pub const VARIANT_ACTIVE_MEMBER_OUT_OF_RANGE: &str = "variant.active-member-out-of-range";

    // Load gate — the v0.7 binding tables (story #167). The loader
    // resolves both indices unchecked (it assumes a validated document),
    // so a dangling one must be named here or it panics at load.
    pub const BINDING_SIGNAL_OUT_OF_RANGE: &str = "binding.signal-out-of-range";
    pub const BINDING_NODE_OUT_OF_RANGE: &str = "binding.node-out-of-range";
    /// Two signal declarations carry the same non-empty name. A runtime
    /// looks a document signal up by name, so a duplicate makes the
    /// lookup ambiguous — one declaration would shadow the other
    /// silently (P4).
    pub const SIGNAL_NAME_DUPLICATE: &str = "signal.name-duplicate";

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

    // Geometry — an extent or radius that cannot rasterize (issue #128).
    /// A rect whose width or height is non-finite (NaN/infinite) or negative.
    /// Rects come from the solver, so this is a broken inter-crate contract
    /// rather than authoring — but the paint gate is the last checkpoint
    /// before a painter rasterizes NaN geometry. Paint gate only: a document
    /// carries no resolved extent (P1).
    pub const RECT_INVALID_EXTENT: &str = "geometry.rect-invalid-extent";
    /// A corner radius that is negative or non-finite. Geometry-free authored
    /// intent (like a stroke width), so it is checked on both a document
    /// (`Paint.corners`) and a solved scene (`PaintEntry.corners`). A
    /// clipping node's corners are copied verbatim into every `ClipBox` of
    /// its subtree (`crates/dashscene-core/src/arena.rs`), so checking a
    /// paint entry's corners catches an out-of-spec clip at its authoring
    /// source — the painter's `RRect::new_rect_radii` does not clamp a
    /// negative radius, so the whole subtree would clip wrongly.
    pub const CORNER_RADIUS_INVALID: &str = "geometry.corner-radius-invalid";

    // Waiver vocabulary — P4 applies to the waiver declarations themselves:
    // an out-of-scope waiver is a named diagnostic, never a silent no-op
    // (issue #41). These ids never appear on a document/scene diagnostic, so
    // they are deliberately absent from [`ALL`] and are not themselves
    // waivable.
    /// A waiver names a rule id that is not a real diagnostic rule.
    pub const WAIVER_UNKNOWN_RULE: &str = "waiver.unknown-rule";
    /// A waiver matches an `Error`-severity diagnostic. An error blocks the
    /// document unconditionally; only a warning is ever waivable.
    pub const WAIVER_COVERS_AN_ERROR: &str = "waiver.covers-an-error";
    /// A waiver matches no diagnostic in the report — dead, and worth
    /// surfacing so the waiver set stays honest.
    pub const WAIVER_UNUSED: &str = "waiver.unused";
    /// A waiver whose every matched diagnostic an earlier waiver already
    /// covers — a duplicate. Surfaced so the set does not accrete redundant
    /// entries that all silently "apply".
    pub const WAIVER_REDUNDANT: &str = "waiver.redundant";

    /// Every rule id a document or scene diagnostic can carry: the import,
    /// load, and paint gates. This is the vocabulary a waiver may name; the
    /// `waiver.*` meta-rules above are not in it.
    ///
    /// [`is_known`] answers membership in this slice, so a new rule not added
    /// here is treated as unknown by the waiver check — never silently
    /// accepted. `tests/triage.rs::the_rule_registry_is_unique_and_covers_every_construct`
    /// pins that every construct's rule is present, so the slice cannot rot.
    pub const ALL: &[&str] = &[
        LAYER_BLUR,
        BACKDROP_BLUR,
        ADVANCED_BLEND_MODE,
        CORNER_SMOOTHING,
        LUMINANCE_MASK,
        CLIP_ON_ROTATED,
        KASHIDA_JUSTIFICATION,
        NOISE_OR_TEXTURE_EFFECT,
        PROGRESSIVE_BLUR,
        ANIMATED_BOOLEAN_OP,
        ANIMATED_VARIABLE_FONT_AXIS,
        VARIABLE_WIDTH_STROKE,
        PARENT_OUT_OF_RANGE,
        PARENT_NOT_BEFORE_CHILD,
        PAINT_ENTRY_OUT_OF_RANGE,
        CONFLICTING_PAINT_REPRESENTATION,
        TEXT_STRING_OUT_OF_RANGE,
        TEXT_STYLE_OUT_OF_RANGE,
        TEXT_STYLE_NO_COLOR,
        TEXT_STYLE_WEIGHT_OUT_OF_RANGE,
        VARIANT_OVERRIDE_NODE_OUT_OF_RANGE,
        VARIANT_SET_NO_MEMBERS,
        VARIANT_ACTIVE_MEMBER_OUT_OF_RANGE,
        BINDING_SIGNAL_OUT_OF_RANGE,
        BINDING_NODE_OUT_OF_RANGE,
        SIGNAL_NAME_DUPLICATE,
        UNKNOWN_ENUM,
        GRADIENT_NO_STOPS,
        GRADIENT_STOP_BUDGET,
        GRADIENT_STOP_OFFSET_INVALID,
        GRADIENT_STOP_ORDER,
        STROKE_INVALID_WIDTH,
        IMAGE_OUT_OF_RANGE,
        IMAGE_NO_BYTES,
        STROKE_EXCEEDS_BOX,
        CLIP_INDEX_OUT_OF_RANGE,
        RECT_INVALID_EXTENT,
        CORNER_RADIUS_INVALID,
    ];

    /// Whether `rule` is a real document/scene diagnostic rule id. A waiver
    /// naming an id this answers `false` for is out of scope (P4).
    pub fn is_known(rule: &str) -> bool {
        ALL.contains(&rule)
    }

    /// The designer-visible workaround for a diagnostic, keyed by rule id.
    ///
    /// docs/specification/04-figma-vocabulary-profile.md's REJECT and LATER
    /// bands each carry a documented workaround ("bake it, slot it, design
    /// without it"). The import gate is where that guidance belongs — the
    /// referential-integrity and geometry rules stand in front of producer
    /// bugs, not designer choices, so they carry no workaround and answer
    /// `None`.
    pub fn workaround(rule: &str) -> Option<&'static str> {
        let hint = match rule {
            LAYER_BLUR => {
                "budgeted at v1; until then bake the blur into the layer's raster or omit it"
            }
            BACKDROP_BLUR => {
                "profile:full only; on a lean target remove the backdrop blur or flatten it into \
                 the layer"
            }
            ADVANCED_BLEND_MODE => {
                "profile:full only; on a lean target bake the blended result into a flat fill"
            }
            CORNER_SMOOTHING => "use plain rounded corners; the squircle is not yet supported",
            LUMINANCE_MASK => "bake the masked result into a raster and import it as an image",
            CLIP_ON_ROTATED => "keep the clipping node axis-aligned, or bake the clipped content",
            KASHIDA_JUSTIFICATION => "use standard justification; kashida elongation is deferred",
            NOISE_OR_TEXTURE_EFFECT => {
                "bake the noise or texture into a raster fill and import it as an image"
            }
            PROGRESSIVE_BLUR => "bake the progressive blur into a raster, or design without it",
            ANIMATED_BOOLEAN_OP => "bake each keyframe of the boolean operation as a static shape",
            ANIMATED_VARIABLE_FONT_AXIS => {
                "slot the text for the runtime to supply, or choose a static font instance"
            }
            VARIABLE_WIDTH_STROKE => {
                "bake the variable-width stroke into a filled shape and import it"
            }
            _ => return None,
        };
        Some(hint)
    }
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
/// Not everything the validator reports is a node. A pooled paint entry, an
/// image asset, a variant set, and a text style are each shared by every node
/// that references them, so each is reported **once, at its own index** —
/// repeating one authoring mistake per referencing node would bury the rest
/// of the report. Their indices are pool indices, not DFS node indices, and
/// this enum is what keeps them from being mistaken for one: a consumer that
/// resolves a diagnostic to a layer (an editor jumping to it, issue #41's
/// waiver machinery keying on it) must not silently land on an unrelated
/// node. Every pooled surface therefore has its own variant — a pool index
/// must never be wrapped in a `Node`, where it would resolve as a DFS index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// A node in the document, by DFS index and name path.
    Node(NodePath),
    /// An entry of the paint pool, by its index in `Document.paints` /
    /// `PaintTable`.
    PaintEntry(u32),
    /// An image asset, by its index in `Document.images` / `ImageTable`.
    ImageAsset(u32),
    /// A variant set, by its index in `Document.variant_sets` — not a node,
    /// the same reasoning as `PaintEntry`/`ImageAsset` (issue #20).
    VariantSet(u32),
    /// A text style, by its index in `Document.text_styles` — not a node, the
    /// same reasoning as `PaintEntry`/`ImageAsset` (issue #41).
    TextStyle(u32),
    /// A signal declaration, by its index in `Document.signals` (story
    /// #167) — a pooled surface like the others.
    Signal(u32),
    /// A binding row, by its index in `Document.bindings` (story #167).
    Binding(u32),
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(path) => write!(f, "{path}"),
            Self::PaintEntry(index) => write!(f, "<paint pool #{index}>"),
            Self::ImageAsset(index) => write!(f, "<image asset #{index}>"),
            Self::VariantSet(index) => write!(f, "<variant set #{index}>"),
            Self::TextStyle(index) => write!(f, "<text style #{index}>"),
            Self::Signal(index) => write!(f, "<signal #{index}>"),
            Self::Binding(index) => write!(f, "<binding #{index}>"),
        }
    }
}

/// One named diagnostic (docs/design/architecture.md: `{rule id, node path, severity}`).
///
/// The fourth element of `docs/archive/2026-07-14-design-1-seed.md` §6.1's
/// tuple — the workaround hint — is [`Diagnostic::workaround`] (issue #41).
/// It is a rule-keyed derivation rather than a stored field so that
/// `dashscene-validator` stays free of a fifth `Diagnostic` field: `dashc`
/// owns serializable mirror types of this struct
/// (`docs/decisions/dashc-wasm-abi.md`), and a new field would break the
/// `Diagnostic { .. }` literals it constructs. The hint is a pure function
/// of the rule id, so nothing is lost by deriving it on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub at: Location,
    pub message: String,
}

impl Diagnostic {
    /// The designer-visible workaround for this diagnostic, or `None` for a
    /// rule that stands in front of a producer bug rather than a design
    /// choice. See [`rule::workaround`].
    pub fn workaround(&self) -> Option<&'static str> {
        rule::workaround(self.rule)
    }
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
        )?;
        if let Some(workaround) = self.workaround() {
            write!(f, " — workaround: {workaround}")?;
        }
        Ok(())
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

    /// Whether the report carries no findings at all — zero diagnostics of
    /// any severity. The strict release gate is [`Report::strict`], which can
    /// pass a report that carries waived warnings; this answers the stricter
    /// "nothing was found".
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

    /// The strict-mode verdict: whether a release build may proceed past
    /// this report given `waivers`. A strict build refuses any warning
    /// (docs/design/architecture.md) unless a declared [`Waiver`] records
    /// that its degrade is acceptable for that one target. Errors are never
    /// waivable, and an out-of-scope waiver is itself diagnosed (P4). See
    /// [`StrictReport`].
    pub fn strict<'a>(&'a self, waivers: &'a [Waiver]) -> StrictReport<'a> {
        waiver::strict(self, waivers)
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
