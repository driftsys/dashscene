//! The Figma REST front end — the only Figma-aware code in the Rust tree.
//!
//! Figma compatibility is a property of one producer (P5), so nothing
//! downstream of this module knows what a `FRAME` or an `imageRef` is: the
//! walk lowers Figma's vocabulary into `Document`, and `Document` is
//! Figma-agnostic.
//!
//! The lowering does no I/O. `dashc` compiles to `wasm32-unknown-unknown`, so
//! it cannot fetch — and Figma serializes an image fill as a bare `imageRef`
//! with no bytes. The caller resolves refs and passes them in.
//!
//! # Unsupported constructs are diagnostics, and the walk keeps going
//!
//! A construct the document vocabulary cannot express — a `TEXT` node before
//! the text lowering, a `GRID` frame before v0.8, a dashed stroke — becomes
//! an error-severity [`Diagnostic`] under [`rule::UNSUPPORTED`], assembled by
//! this producer (`docs/decisions/producer-assembles-its-own-diagnostics.md`).
//! The node and its subtree are skipped — never lowered approximately — and
//! the walk continues, so one pass reports every finding instead of stopping
//! at the first (debt #149). The error severity is what keeps R6 intact: a
//! document with any unsupported construct never emits.
//!
//! [`CompileError`] is reserved for the failures that stop the walk itself:
//! input that does not parse, a file with no root frame, an `imageRef` the
//! caller failed to resolve.

pub mod bindings;
pub mod rest;
pub(crate) mod triage;

pub use bindings::{BoundValue, BoundVariable};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde::Deserialize;

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, ImageAsset, Mat23, PaintEntry,
    PaintKind, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2,
};
use dashscene_validator::{Diagnostic, Location, NodePath, Profile, Report, Severity};

// `Node` and `Paint` collide with `rest`'s Figma-vocabulary types of the same
// name (imported below, unaliased, since they are what the rest of this
// module's signatures use); the document's types are aliased here instead.
// The rule: each file leaves its own subject bare and aliases the visitor.
// Here the Figma REST types are the subject, so the IR types are aliased;
// in `emit.rs` the IR is the subject, so the flatbuffer types are aliased
// (its `Node` stays bare and the flatbuffer's is `FbNode`).
use crate::document::{
    AxisSizing, Box2D, CrossAxisAlign, Document, EdgeInsets, GridTrack as DocGridTrack,
    LayoutConstraints, LayoutContainer as DocContainer, LayoutMode, MainAxisAlign, Node as DocNode,
    Paint as DocPaint, TextStyle as DocTextStyle,
};
use crate::figma::rest::{FigmaFile, Node, Paint, PaintTag};

/// The diagnostic rules this producer assembles itself — constructs with no
/// `dashscene_validator::Construct` variant, because adding one would turn
/// the validator's vocabulary into a list of one producer's expressiveness
/// gaps (P5).
pub mod rule {
    /// A construct the document vocabulary cannot express yet. Always an
    /// error: lowering it approximately would render a picture the designer
    /// never authored, and dropping it in silence is what P4 forbids. The
    /// message names the construct; the node path names the layer.
    pub const UNSUPPORTED: &str = "figma.unsupported";

    /// The walk resolved every top-level node to no paintable content — the
    /// definitions-only case: a canvas holding only `COMPONENT`/`COMPONENT_SET`
    /// resolves but paints nothing
    /// (`docs/decisions/figma-component-lowering.md`), so the document would
    /// emit as zero nodes. Always an error: a zero-node `.dsb` is a picture with
    /// no roots, which a downstream consumer panics loading, and emitting it in
    /// silence is what P4 forbids. The message names what was skipped and why.
    pub const NO_CONTENT: &str = "figma.no-content";
}

/// The JSON nesting depth [`parse_file`] accepts.
///
/// A `rest::Node` costs two JSON levels (the object plus its `children`
/// array), so this admits roughly 125 nested frames — double the 61 frames
/// serde_json's default recursion limit allowed (debt #148) and far beyond
/// any real design file. The bound exists because the pre-scan below is what
/// lets the serde recursion limit be disabled without risking a stack
/// overflow — which on `wasm32-unknown-unknown` would be a trap, and the ABI
/// promises a status, never a trap (`docs/decisions/dashc-wasm-abi.md`).
///
/// The value is sized to the wasm module's stack, the smallest this code
/// runs on: rustc gives `wasm32-unknown-unknown` 1 MiB, and a release-build
/// parse measures roughly 1.8 KiB of stack per JSON level (2026-07-16, via
/// a native probe on shrunken threads — 510 levels fit 1 MiB and overflowed
/// 512 KiB), so 256 levels spend under half the budget. A debug build costs
/// roughly 15 KiB per level, so a native debug caller wanting the full depth
/// needs a stack of a few MiB — the depth tests spawn one explicitly.
pub const MAX_JSON_DEPTH: usize = 256;

/// The one fractional tolerance the three `ELLIPSE` geometry gates share —
/// extents equality, arc sweep, and inner radius
/// (`docs/decisions/figma-ellipse-as-circle.md`).
///
/// A real capture carries float noise: Figma composes transforms up the tree
/// and reports decimal extents, so an authored circle arrives as `56.0 ×
/// 55.99998` (relative difference ~4e-7), a full sweep as `2π` minus a
/// rounding bit, and a solid ellipse's `innerRadius` as a hair above zero
/// rather than exactly zero. An exact `==`/`!= 0.0` gate would refuse those
/// genuine full circles — the exact real-file shape #37 targets.
///
/// `1e-3` sits two-plus orders of magnitude above that noise and far below any
/// ellipse the painter would render visibly non-circular. As a fraction of the
/// full-scale quantity it admits at most: `0.1 %` of the larger extent (`0.056
/// px` on a `56 px` circle — sub-pixel), `0.1 %` of a full turn (`0.36°` of
/// sweep), and `0.1 %` of the outer radius of inner hole. So it cannot admit a
/// genuine non-circular ellipse, arc, or ring — a `56 × 50` ellipse differs by
/// `11 %`, over a hundred times the tolerance.
const ELLIPSE_GEOMETRY_TOLERANCE: f32 = 1e-3;

/// Why a Figma file could not be compiled at all.
///
/// Distinct from a `Diagnostic`, which is a verdict *about* a document that
/// was understood. These are the cases where lowering cannot proceed —
/// constructs the vocabulary cannot express are diagnostics instead (see the
/// module doc), so `Unsupported` is left with the structural refusals that
/// have no node to diagnose at.
#[derive(Debug)]
pub enum CompileError {
    /// The input was not the Figma REST JSON it claimed to be, or it nests
    /// deeper than [`MAX_JSON_DEPTH`] JSON levels.
    Parse(serde_json::Error),
    /// A file shape the walk cannot start on — today, a document with no
    /// top-level node under any `CANVAS`.
    Unsupported { path: String, what: String },
    /// An image fill whose `imageRef` the caller did not resolve. The load
    /// gate rejects a zero-byte asset, so no placeholder can be invented.
    /// A caller-contract failure, not a vocabulary verdict — it aborts.
    UnresolvedImage { path: String, image_ref: String },
    /// The document carried at least one error-severity diagnostic, so R6
    /// blocks emission.
    Diagnostics(Report),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "not valid Figma REST JSON: {e}"),
            Self::Unsupported { path, what } => {
                write!(f, "{path}: {what} is not in the document vocabulary")
            }
            Self::UnresolvedImage { path, image_ref } => {
                write!(f, "{path}: no image supplied for imageRef {image_ref}")
            }
            Self::Diagnostics(report) => write!(f, "{report}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Parses Figma REST JSON with the depth guard in front.
///
/// serde_json's default recursion limit (128 levels) capped the lowering at
/// 61 nested frames and failed deeper files with an opaque message naming no
/// limit (debt #148). Here the limit is explicit instead: a linear,
/// non-recursive scan refuses anything deeper than [`MAX_JSON_DEPTH`] with an
/// error that names both depths, and within that bound the serde limit is
/// disabled — the scan is what makes that safe (see [`MAX_JSON_DEPTH`]).
pub(crate) fn parse_file(json: &str) -> Result<FigmaFile, CompileError> {
    let depth = json_depth(json);
    if depth > MAX_JSON_DEPTH {
        return Err(CompileError::Parse(
            <serde_json::Error as serde::de::Error>::custom(format!(
                "the file nests {depth} JSON levels deep; the limit is {MAX_JSON_DEPTH}"
            )),
        ));
    }

    let mut deserializer = serde_json::Deserializer::from_str(json);
    deserializer.disable_recursion_limit();
    let file = FigmaFile::deserialize(&mut deserializer).map_err(CompileError::Parse)?;
    deserializer.end().map_err(CompileError::Parse)?;
    Ok(file)
}

/// The maximum `{`/`[` nesting depth of `json`, by a linear scan.
///
/// String-aware — a brace inside a string literal does not nest — and
/// tolerant of malformed input: it measures depth, and the parser proper
/// reports everything else. Non-recursive by construction, so it is safe on
/// input the parser must not be given.
fn json_depth(json: &str) -> usize {
    let (mut depth, mut max) = (0usize, 0usize);
    let mut in_string = false;
    let mut escaped = false;
    for byte in json.bytes() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                max = max.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Lowers a parsed Figma file into a `Document` plus the diagnostics its
/// out-of-profile and out-of-vocabulary constructs earned.
///
/// The diagnostics carry two kinds of verdict in document order: the import
/// gate's (`triage`, over constructs the profile bands) and this producer's
/// own [`rule::UNSUPPORTED`] errors (over constructs the vocabulary cannot
/// express at all). An unsupported node's subtree is skipped, so on any
/// error the returned `Document` is partial — [`crate::compile_figma`]
/// blocks emission in exactly that case (R6).
///
/// `images` maps an `imageRef` to its bytes. Figma's `GET /file` carries no
/// image bytes, and `dashc` cannot fetch them (wasm), so whoever *can* fetch
/// — the Deno importer — resolves them and passes them here.
pub fn lower(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Document, Vec<Diagnostic>), CompileError> {
    lower_with_bindings(file, profile, images, &[])
}

/// [`lower`], plus the importer's joined variable-binding rows (story
/// #167): after the walk, each row is mapped onto the document node its
/// Figma id lowered to and applied as `Document.signals`/
/// `Document.bindings` — see [`bindings::apply`] for the property →
/// channel mapping and the named verdicts.
pub fn lower_with_bindings(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bound: &[BoundVariable],
) -> Result<(Document, Vec<Diagnostic>), CompileError> {
    lower_with_bindings_and_policy(file, profile, images, bound, crate::EmitPolicy::Strict)
}

/// [`lower_with_bindings`], choosing the emit policy
/// (`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`).
///
/// The policy rides on the [`Walk`] and reaches [`Walk::unsupported_at`]: under
/// [`crate::EmitPolicy::Partial`] a `figma.unsupported` omission is minted at
/// `Severity::Warning` instead of `Severity::Error`, so the skipped node's gap
/// no longer blocks emission. Nothing else in the walk changes — the subtree is
/// skipped either way, and `figma.no-content` and the triaged
/// approximation-if-shipped constructs keep their severity.
pub fn lower_with_bindings_and_policy(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bound: &[BoundVariable],
    policy: crate::EmitPolicy,
) -> Result<(Document, Vec<Diagnostic>), CompileError> {
    let roots = top_level_nodes(&file.document)?;

    let mut walk = Walk {
        doc: Document::new(),
        diagnostics: Vec::new(),
        image_of: BTreeMap::new(),
        index_of_id: BTreeMap::new(),
        profile,
        images,
        policy,
    };

    // Iterative depth-first walk (debt #148: a recursive walk would turn
    // the parse-depth headroom above into a stack overflow). Children are
    // pushed in reverse, so popping yields document (DFS preorder) order —
    // which is both the rect-table order and the diagnostics order.
    //
    // Every top-level node is a document root: a declared-roots export computes
    // exactly the set to lower, so the walk no longer selects one positionally
    // (debt #147, `docs/decisions/figma-component-lowering.md`). Roots are
    // seeded in reverse for the same LIFO reason, so the first canvas child
    // lowers first and each root's subtree completes before the next root
    // begins. A root has no parent origin: it is relative to itself, so it drops
    // its page position and lowers to (0, 0, w, h) — each root independently.
    // Component definitions among the roots resolve but do not paint;
    // `Walk::visit` skips them.
    let root_segments = disambiguated_segments(&roots);
    // The top-level definitions, named for the no-content diagnostic below —
    // captured before `roots` is consumed by the seed loop.
    let definitions: Vec<String> = roots
        .iter()
        .filter(|n| n.kind == "COMPONENT" || n.kind == "COMPONENT_SET")
        .map(|n| format!("{} ({})", n.name, n.kind))
        .collect();
    let mut stack: Vec<Visit> = Vec::with_capacity(roots.len());
    for (root, segment) in roots.into_iter().zip(root_segments).rev() {
        stack.push(Visit {
            node: root,
            parent: None,
            parent_origin: None,
            path: format!("/{segment}"),
            flow: None,
        });
    }
    while let Some(visit) = stack.pop() {
        walk.visit(visit, &mut stack)?;
    }

    // A document that lowered to no content is refused by name (P4). The
    // definitions-only case reaches here silently — every top-level node was a
    // `COMPONENT`/`COMPONENT_SET` the walk skipped, leaving zero nodes and no
    // diagnostic — so a zero-node `.dsb` would emit, and a consumer that expects
    // at least one root panics loading it. When some other finding already
    // blocks the document (an unsupported top-level node), that error explains
    // the emptiness and R6 already blocks, so this diagnostic is not added on
    // top of it.
    // The joined binding rows (story #167), applied after the walk so
    // every lowered node's index is known. Their diagnostics append after
    // the walk's — the rows arrive in sidecar (document) order, so the
    // report stays deterministic (R7).
    let binding_diagnostics = bindings::apply(&mut walk.doc, bound, &walk.index_of_id);
    walk.diagnostics.extend(binding_diagnostics);

    if walk.doc.nodes.is_empty()
        && !walk
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    {
        let what = if definitions.is_empty() {
            "the document lowered to no content".to_string()
        } else {
            format!(
                "the document lowered to no content: its only top-level nodes \
                 are component definitions ({}), which resolve but do not paint",
                definitions.join(", "),
            )
        };
        walk.diagnostics.push(Diagnostic {
            rule: rule::NO_CONTENT,
            severity: Severity::Error,
            at: Location::Node(NodePath::new(0, "/")),
            message: what,
        });
    }

    Ok((walk.doc, walk.diagnostics))
}

/// The `imageRef`s the lowering will demand, sorted and deduplicated.
///
/// The Deno importer cannot fetch what it cannot name, and Figma's `GET /file`
/// carries no image bytes — only refs. Rather than have the importer walk the
/// JSON looking for them (a second copy of "where an imageRef lives", free to
/// drift from the walk that actually consumes them), it asks here. The scan
/// covers every top-level node's subtree — component definitions included — and
/// both fills and strokes, so it names exactly the refs a declared-roots export
/// ships (`importers/figma/src/closure.ts` counts a pulled component set's fills
/// too), which is what keeps the closure↔dashc drift oracle exact.
///
/// Deliberately a superset of what [`lower`] embeds: a paint this returns may
/// still be refused by the lowering (a stacked fill, an invisible one) or sit in
/// a definition the lowering resolves but does not paint this slice. Fetching an
/// image that turns out to be unused costs one download; missing one is a failed
/// compile.
pub fn image_refs(file: &FigmaFile) -> Result<Vec<String>, CompileError> {
    let mut found = BTreeSet::new();
    // Iterative for the same reason the lowering walk is (debt #148).
    let mut stack = top_level_nodes(&file.document)?;
    while let Some(node) = stack.pop() {
        for paint in node.fills.iter().chain(node.strokes.iter()) {
            if paint.kind == PaintTag::Image
                && let Some(image_ref) = &paint.image_ref
            {
                found.insert(image_ref.clone());
            }
        }
        stack.extend(&node.children);
    }
    Ok(found.into_iter().collect())
}

/// Every top-level node under every `CANVAS` — the roots the walk lowers.
///
/// v0.3 selected the first `FRAME` under the first `CANVAS` and dropped every
/// sibling and every later canvas in silence (debt #147). The walk now lowers
/// every top-level node as a document root: a declared-roots export
/// (`importers/figma/src/closure.ts`) computes exactly the set to pass, so the
/// walk no longer selects one positionally, and a component-carrying export —
/// whose pruned file carries the export root beside the component definitions it
/// requires — lowers whole (`docs/decisions/figma-component-lowering.md`).
/// Component definitions among the roots resolve but do not paint; `Walk::visit`
/// skips them. A document with no top-level node under any canvas has nothing to
/// lower and is refused.
fn top_level_nodes(document: &Node) -> Result<Vec<&Node>, CompileError> {
    let roots: Vec<&Node> = document
        .children
        .iter()
        .filter(|n| n.kind == "CANVAS")
        .flat_map(|canvas| canvas.children.iter())
        .collect();
    if roots.is_empty() {
        return Err(CompileError::Unsupported {
            path: "/".to_string(),
            what: "a document with no top-level node under any CANVAS".to_string(),
        });
    }
    Ok(roots)
}

/// The slash-path segment for each node in `nodes`, disambiguating duplicate
/// sibling names with the Figma id (or the position when a synthetic node has
/// no id) so two same-named siblings never share one diagnostic path
/// (debt #150). One rule for both sibling sets the walk builds paths over: the
/// document's top-level roots and a parent's children.
fn disambiguated_segments(nodes: &[&Node]) -> Vec<String> {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for node in nodes {
        *counts.entry(node.name.as_str()).or_default() += 1;
    }
    nodes
        .iter()
        .enumerate()
        .map(|(position, node)| {
            if counts[node.name.as_str()] > 1 {
                match &node.id {
                    Some(id) => format!("{} ({id})", node.name),
                    None => format!("{} (#{position})", node.name),
                }
            } else {
                node.name.clone()
            }
        })
        .collect()
}

/// The flex context a child is visited under — what its parent's container
/// intent means for *this* child.
#[derive(Clone, Copy)]
struct Flow {
    /// The parent's main axis.
    horizontal: bool,
    /// The leading main-axis margin this child absorbs: the parent's
    /// negative authored gap for every in-flow child after the first, zero
    /// otherwise (`docs/decisions/negative-gap-lowering.md`).
    leading_margin: f32,
    /// Whether the parent hugs each axis — a `Fill` child on a hug axis is
    /// refused (see `constraints_of`).
    parent_hugs_h: bool,
    parent_hugs_v: bool,
}

/// One pending node of the iterative walk.
struct Visit<'a> {
    node: &'a Node,
    parent: Option<u32>,
    /// The parent's *absolute* origin — what turns Figma's page-absolute box
    /// into the parent-relative intent `Document` wants. `None` at the root,
    /// which has no parent and so is relative to itself.
    parent_origin: Option<(f32, f32)>,
    /// The node's slash-joined name path, duplicate siblings disambiguated.
    path: String,
    /// `Some` when the parent is a flex container.
    flow: Option<Flow>,
}

struct Walk<'a> {
    doc: Document,
    diagnostics: Vec<Diagnostic>,
    /// Interns `imageRef` → image-table index, so two nodes sharing one image
    /// share one asset.
    image_of: BTreeMap<String, u32>,
    /// Figma node id → (document index, diagnostic path) for every node
    /// the walk lowered — what the binding rows join against (story #167).
    index_of_id: bindings::IndexOfId,
    profile: Profile,
    images: &'a BTreeMap<String, ImageAsset>,
    /// How an omission-class gap (`figma.unsupported`) is minted: an error
    /// under `Strict`, a warning under `Partial` (story S0-impl).
    policy: crate::EmitPolicy,
}

impl Walk<'_> {
    /// Lowers one node, or diagnoses why it cannot be lowered, and schedules
    /// its children.
    ///
    /// Every unsupported finding on the node is collected before the verdict,
    /// so a node carrying two gaps reports both. If any finding blocks the
    /// node, its subtree is skipped: the children's geometry is relative to a
    /// parent that does not exist in the document, and under a grid or
    /// wrapped container their boxes are solver output P1 forbids reading.
    ///
    /// `Err` is reserved for [`CompileError::UnresolvedImage`] — a
    /// caller-contract failure, not a vocabulary verdict.
    fn visit<'a>(
        &mut self,
        visit: Visit<'a>,
        stack: &mut Vec<Visit<'a>>,
    ) -> Result<(), CompileError> {
        let node = visit.node;
        let path = &visit.path;

        // A `COMPONENT` or `COMPONENT_SET` is a definition (story #242): it
        // resolves — the walk accepts it and its subtree — but does not paint as
        // document content, so it is skipped whole, never diagnosed. This story
        // lowers the authored state; the v0.4 variant table that would carry the
        // alternative members is consumer-side and out of scope
        // (`docs/decisions/figma-component-lowering.md`). Skipping the subtree
        // also means a definition's own findings — its dashed stroke, a member's
        // unsupported construct — never fire: nothing in it paints.
        if node.kind == "COMPONENT" || node.kind == "COMPONENT_SET" {
            return Ok(());
        }

        // `FRAME`, `INSTANCE`, `TEXT`, and `ELLIPSE` are the node kinds with a
        // lowering (stories #140, #242, #160, #239). An `INSTANCE` lowers like
        // a `FRAME`: Figma bakes the referenced component's content — with the
        // instance's overrides applied — into the instance's own children, so
        // the baked subtree goes through the ordinary walk and an
        // out-of-vocabulary override on it is a named diagnostic like any other
        // (P4). `RECTANGLE` is a paint-bearing leaf lowered through the same
        // container/paint path with no `layoutMode` and no children (#309).
        // `SECTION` and `GROUP` lower as absolute containers — no `layoutMode`,
        // so their children are positioned by authored offset through the same
        // path (#309). Any other kind reports its type and nothing else: its
        // other properties belong to whatever story lowers that type (the
        // remaining shape kinds when a shape construct lands), so diagnosing
        // them here would be noise around the verdict that matters.
        if node.kind != "FRAME"
            && node.kind != "INSTANCE"
            && node.kind != "TEXT"
            && node.kind != "ELLIPSE"
            && node.kind != "RECTANGLE"
            && node.kind != "SECTION"
            && node.kind != "GROUP"
        {
            self.unsupported(path, format!("node type {}", node.kind));
            return Ok(());
        }

        // What blocks this node, all findings collected before the verdict.
        let mut blockers: Vec<String> = Vec::new();

        // v0.8 (story #44) un-pinned node opacity, mask membership, and
        // hidden nodes — the document now carries all three
        // (`docs/decisions/masks-and-group-opacity.md`, debt #143), lowered
        // below into the DocNode's `opacity`/`mask`/`visible`. A hidden
        // node keeps its DFS index (Prop::Visible → Display::None), so it
        // no longer shifts the indices every later node depends on.
        let node_opacity = node.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        let node_visible = node.visible != Some(false);

        // A mask lowers only when it is a box-shaped, geometry (outline)
        // mask — the only kind the hard clip-region vocabulary can express
        // (M6). A soft alpha or luminance mask, and a mask whose shape is
        // not a box (a text node's letterforms; a VECTOR/BOOLEAN shape is
        // already refused above as an unsupported node type), refuse by name
        // rather than lowering as a hard rounded-box stencil (P4). An absent
        // `maskType` is a synthetic node and lowers as the geometric
        // default.
        let node_mask = if node.is_mask == Some(true) {
            match node.mask_type.as_deref() {
                Some("ALPHA") => {
                    blockers.push(
                        "an alpha mask (a soft mask has no hard box-clip lowering)".to_string(),
                    );
                    false
                }
                Some("LUMINANCE") => {
                    blockers.push("a luminance mask".to_string());
                    false
                }
                _ if node.kind == "TEXT" => {
                    blockers
                        .push("a text node used as a mask (letterforms are not a box)".to_string());
                    false
                }
                _ => true,
            }
        } else {
            false
        };

        // Rotation stays refused: no schema or paint support for it lands
        // here, so a rotated node is a named diagnostic rather than lowered
        // as though it were axis-aligned (P4). Figma omits `rotation`
        // entirely when it is zero, so `None` and `Some(0.0)` both mean
        // unrotated.
        if node.rotation.is_some_and(|r| r != 0.0) {
            blockers.push("node rotation".to_string());
        }
        // `sectionContentsHidden` hides a SECTION's children in Figma. The
        // document has no vocabulary for a hidden-contents section, so
        // lowering its children anyway would silently render content Figma
        // hides (P4).
        if node.kind == "SECTION" && node.section_contents_hidden == Some(true) {
            blockers.push("a section with hidden contents (sectionContentsHidden)".to_string());
        }
        // An absolutely-positioned child sits outside its auto-layout
        // parent's flow; treating it as in-flow would reflow every sibling
        // after it. Absent means AUTO (in flow).
        if node.layout_positioning.as_deref() == Some("ABSOLUTE") {
            blockers.push("absolute positioning inside auto-layout".to_string());
        }
        // Strokes that consume layout space are the strokes-in-layout
        // Figma≠CSS difference; the schema has no border vocabulary, so a
        // frame solved with them would come out a different size.
        if node.strokes_included_in_layout == Some(true) {
            blockers.push("strokes included in layout (strokesIncludedInLayout)".to_string());
        }
        // Document order is paint order; a reversed child stack has no
        // lowering short of reordering the nodes themselves.
        if node.item_reverse_z_index == Some(true) {
            blockers.push("reversed child paint order (itemReverseZIndex)".to_string());
        }

        // The per-axis sizing intent is shared by both kinds: a text node
        // hugs, fills, or fixes each axis exactly as a frame does when its
        // `layoutSizing*` is present (D1), and a Hug axis flows through the
        // engine's measure seam (#29).
        let mut constraints = constraints_of(node, visit.flow.as_ref(), &mut blockers);

        if node.absolute_bounding_box.is_none() {
            blockers.push(format!("node {} has no absoluteBoundingBox", node.name));
        }

        // Type-specific lowering: a frame — or an instance, which is frame-like
        // and carries its resolved content as its own children — carries its
        // container intent and paint; a text node its characters and style; an
        // ellipse its fill and stroke as a circle (a rounded rect with corner
        // radius = half the extent, `docs/decisions/figma-ellipse-as-circle.md`).
        // A text node's fill is the glyph color (it lowers into the style, not a
        // paint entry), so `paint` stays `None` and the node's `paint_entry` is
        // the "draws nothing" sentinel.
        let mut container: Option<DocContainer> = None;
        let mut paint: Option<DocPaint> = None;
        let mut text: Option<String> = None;
        let mut text_style: Option<DocTextStyle> = None;
        if node.kind == "TEXT" {
            let (t, ts) = self.text_of(node, &mut blockers);
            text = t;
            text_style = ts;
            // Outside auto-layout Figma sets no `layoutSizing*`, so
            // `textAutoResize` is the sizing source (a free-standing label
            // must hug, not fix-size from its resolved box).
            constraints = text_sizing(node, constraints);
        } else if node.kind == "ELLIPSE" {
            // A leaf: no container. The paint lowering keeps its own refusals
            // (a stacked fill, a dashed stroke), and the ellipse gate adds its
            // own (an arc, a ring, a non-circular or non-fixed ellipse); each
            // blocks the node like any other finding. `UnresolvedImage` aborts.
            paint = match self.ellipse_paint_of(node, path, constraints, &mut blockers) {
                Ok(paint) => paint,
                Err(CompileError::Unsupported { what, .. }) => {
                    blockers.push(what);
                    None
                }
                Err(other) => return Err(other),
            };
        } else {
            container = container_of(node, &mut blockers);
            // The paint lowering keeps its own refusals; an unsupported paint
            // blocks the node like any other finding. `UnresolvedImage`
            // aborts: it is the caller's contract, not the designer's file.
            paint = match self.paint_of(node, path) {
                Ok(paint) => paint,
                Err(CompileError::Unsupported { what, .. }) => {
                    blockers.push(what);
                    None
                }
                Err(other) => return Err(other),
            };
        }

        // The import gate: the producer maps, the validator decides (P5).
        // Unmapped effects (a baked shadow, debt #144) have no Construct and
        // block the node instead.
        let (constructs, effect_blockers) = triage::constructs_of(node);
        blockers.extend(effect_blockers);

        if !blockers.is_empty() {
            // The index this node would have taken had it lowered. The node is
            // skipped either way — refused under Strict, omitted with a warning
            // under Partial — so it never enters the document; the index is an
            // advisory locator, the path is the stable half, and two skipped
            // siblings may share one.
            let index = self.doc.nodes.len() as u32;
            for what in blockers {
                self.unsupported_at(index, path, what);
            }
            // The node's triaged constructs are still real findings on a
            // real layer; dropping them because a sibling property was
            // unsupported would re-create the one-finding-per-pass loop
            // this walk exists to avoid (debt #149).
            for construct in constructs {
                self.diagnostics.push(dashscene_validator::triage(
                    construct,
                    self.profile,
                    NodePath::new(index, path.clone()),
                ));
            }
            return Ok(());
        }

        let bbox = node
            .absolute_bounding_box
            .expect("an absent box is a blocker, checked above");
        // Where a frame sits on the Figma page is a page-layout artifact, not
        // intent (P1). The root has no parent to be relative to, so it is
        // relative to itself and lowers to (0, 0, w, h).
        let origin = visit.parent_origin.unwrap_or((bbox.x, bbox.y));
        // Inside a flex parent the solver owns placement, so the box Figma
        // reports is its solver's output, not authored intent — the P1 ground
        // of `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`.
        // The same split applies per axis to the size: a Fixed axis's extent
        // is the authored datum, a Hug/Fill axis's extent is solved. What is
        // not intent lowers as zero — the absence the solver ignores.
        let (x, y) = if visit.flow.is_some() {
            (0.0, 0.0)
        } else {
            (bbox.x - origin.0, bbox.y - origin.1)
        };
        let sizing = constraints.unwrap_or_default();
        let width = if sizing.sizing_h == AxisSizing::Fixed {
            bbox.width
        } else {
            0.0
        };
        let height = if sizing.sizing_v == AxisSizing::Fixed {
            bbox.height
        } else {
            0.0
        };

        // The container as emitted: a negative authored gap is lowered here,
        // producer-side, so the document never carries one — the dashc half
        // of `docs/decisions/negative-gap-lowering.md`, mirroring core's
        // `Txn::lower_negative_gaps` (gap to zero, the gap onto the leading
        // main-axis margin of every child after the first). Under
        // SPACE_BETWEEN Figma ignores the authored spacing entirely — the
        // solver owns it — so the gap lowers to zero with no margins.
        let mut child_leading_margin = 0.0;
        let container = container.map(|mut c| {
            if c.main_align == MainAxisAlign::SpaceBetween {
                c.gap = 0.0;
            } else if c.gap < 0.0 && matches!(c.mode, LayoutMode::Horizontal | LayoutMode::Vertical)
            {
                // Only a flex-flow gap lowers to a leading margin. A
                // negative Wrap gap is refused in `container_of` (a margin
                // would distort the line breaks), and a Grid gap is track
                // spacing, not flow spacing
                // (`docs/decisions/v08-layout-vocabulary-shape.md` D5).
                child_leading_margin = c.gap;
                c.gap = 0.0;
            }
            c
        });

        // Computed before the container is moved into the node, since the
        // v0.8 grid track lists make `LayoutContainer` non-`Copy`.
        let flow = container.as_ref().map(|c| Flow {
            horizontal: c.mode == LayoutMode::Horizontal,
            leading_margin: child_leading_margin,
            parent_hugs_h: sizing.sizing_h == AxisSizing::Hug,
            parent_hugs_v: sizing.sizing_v == AxisSizing::Hug,
        });

        let index = self.doc.push(DocNode {
            name: Some(node.name.clone()),
            parent: visit.parent,
            box2d: Box2D {
                x,
                y,
                width,
                height,
            },
            paint,
            container,
            constraints,
            text,
            text_style,
            opacity: node_opacity,
            mask: node_mask,
            visible: node_visible,
        });
        // Where this Figma node landed — the join key for the binding
        // rows (story #167). A synthetic node without an id (a test
        // shape) simply cannot be bound. The visible fill's paint opacity
        // rides along: the lowering folded it into the literal's alpha,
        // so a FillA binding must capture the same multiply.
        if let Some(id) = &node.id {
            let fill_opacity = match single_visible_paint(&node.fills) {
                OnePaint::One(paint) => paint.opacity.unwrap_or(1.0),
                OnePaint::None | OnePaint::Many => 1.0,
            };
            self.index_of_id.insert(
                id.clone(),
                bindings::LoweredNode {
                    index,
                    path: path.clone(),
                    fill_opacity,
                },
            );
        }

        for construct in constructs {
            self.diagnostics.push(dashscene_validator::triage(
                construct,
                self.profile,
                NodePath::new(index, path.clone()),
            ));
        }

        // Figma permits duplicate sibling names, and a path built from names
        // alone would give two siblings one diagnostic path (debt #150), so the
        // segments are disambiguated by the same rule the top-level roots use.
        let child_refs: Vec<&Node> = node.children.iter().collect();
        let segments = disambiguated_segments(&child_refs);

        // Reversed, so the LIFO stack pops them in document order.
        for (position, child) in node.children.iter().enumerate().rev() {
            let flow = flow.map(|f| Flow {
                leading_margin: if position == 0 { 0.0 } else { f.leading_margin },
                ..f
            });
            stack.push(Visit {
                node: child,
                parent: Some(index),
                parent_origin: Some((bbox.x, bbox.y)),
                path: format!("{path}/{}", segments[position]),
                flow,
            });
        }
        Ok(())
    }

    /// One unsupported-construct diagnostic at the index the node would have
    /// taken had it lowered.
    fn unsupported(&mut self, path: &str, what: String) {
        let index = self.doc.nodes.len() as u32;
        self.unsupported_at(index, path, what);
    }

    fn unsupported_at(&mut self, index: u32, path: &str, what: String) {
        // The one policy-sensitive diagnostic (story S0-impl): an omission is
        // an error under Strict (R6 refuses the file) and a warning under
        // Partial (the node is skipped either way, so the document still
        // emits with the gap named — P4, never a silent drop).
        let severity = match self.policy {
            crate::EmitPolicy::Strict => Severity::Error,
            crate::EmitPolicy::Partial => Severity::Warning,
        };
        self.diagnostics.push(Diagnostic {
            rule: rule::UNSUPPORTED,
            severity,
            at: Location::Node(NodePath::new(index, path)),
            message: format!("{what} is not in the document vocabulary yet"),
        });
    }

    fn paint_of(&mut self, node: &Node, path: &str) -> Result<Option<DocPaint>, CompileError> {
        let entry = PaintEntry {
            fill: self.fill_of(node, path)?,
            stroke: self.stroke_of(node, path)?,
            corners: corners_of(node),
            shadows: shadows_of(node, path)?,
        };

        // A layout-only container draws nothing but still occupies a rect-table
        // slot. A clipping frame with no paint still needs its clip intent.
        if entry == PaintEntry::default() && !node.clips_content {
            return Ok(None);
        }
        Ok(Some(DocPaint {
            entry,
            clip: node.clips_content,
        }))
    }

    /// Lowers a full-circle `ELLIPSE` into the rounded-rect paint vocabulary,
    /// or collects the reasons it cannot be lowered.
    ///
    /// Only a circle lowers exactly. The paint entry's per-corner radius is
    /// one scalar, so a rounded rect with radius = half the extent is a circle
    /// when the two extents are equal and a stadium when they are not
    /// (`docs/decisions/figma-ellipse-as-circle.md`). So a non-circular,
    /// non-fixed-size, arced, or ringed ellipse is refused by name (P4), each
    /// finding collected before the verdict. The returned paint carries the
    /// circle's corners and the frame fill/stroke; the caller skips the node
    /// whenever `blockers` is non-empty, discarding the paint.
    fn ellipse_paint_of(
        &mut self,
        node: &Node,
        path: &str,
        constraints: Option<LayoutConstraints>,
        blockers: &mut Vec<String>,
    ) -> Result<Option<DocPaint>, CompileError> {
        // `arcData`: a full ellipse sweeps 2π with no inner radius. A partial
        // sweep is a pie; a non-zero inner radius is a ring. Neither has a
        // rounded-rect lowering. Both gates are toleranced against real-capture
        // float noise (see `ELLIPSE_GEOMETRY_TOLERANCE`): the sweep against a
        // full turn, the inner radius (already a 0–1 fraction of the outer
        // radius) against zero. Absent `arcData` is Figma's full-ellipse
        // default.
        if let Some(arc) = &node.arc_data {
            let sweep = (arc.ending_angle - arc.starting_angle).abs();
            let full_turn = std::f32::consts::TAU;
            if (sweep - full_turn).abs() > ELLIPSE_GEOMETRY_TOLERANCE * full_turn {
                blockers.push("an elliptical arc (partial arcData sweep)".to_string());
            }
            if arc.inner_radius.abs() > ELLIPSE_GEOMETRY_TOLERANCE {
                blockers.push("a ring (arcData innerRadius)".to_string());
            }
        }

        // The corner radius that turns a rounded rect into a circle is half
        // the extent — a static paint parameter. It is exact only when the two
        // extents are equal (unequal extents need a per-axis radius the
        // vocabulary lacks) and both axes are Fixed (a Hug/Fill extent is
        // solver output P1 forbids baking, so a static radius could not track
        // it). Equality is toleranced relative to the larger extent, against
        // the same real-capture noise (`ELLIPSE_GEOMETRY_TOLERANCE`).
        let sizing = constraints.unwrap_or_default();
        if sizing.sizing_h != AxisSizing::Fixed || sizing.sizing_v != AxisSizing::Fixed {
            blockers.push("a non-fixed-size ellipse".to_string());
        }
        let radius = match node.absolute_bounding_box {
            Some(bbox)
                if (bbox.width - bbox.height).abs()
                    <= ELLIPSE_GEOMETRY_TOLERANCE * bbox.width.max(bbox.height) =>
            {
                // Half the larger extent: on the larger axis this leaves no
                // straight edge, and skia clamps it on the smaller axis, so the
                // sub-pixel difference the tolerance admits cannot overshoot.
                bbox.width.max(bbox.height) / 2.0
            }
            Some(_) => {
                blockers.push("a non-circular ellipse (unequal extents)".to_string());
                0.0
            }
            // An absent box is already a blocker in `visit`; the node is
            // skipped, so this radius is never emitted.
            None => 0.0,
        };

        // The fill and stroke lower exactly as a frame's — only the corners
        // differ (a frame reads `cornerRadius`; an ellipse has none, so the
        // circle radius stands in). A refused fill or stroke propagates as
        // `Unsupported`, which the caller turns into a blocker.
        let entry = PaintEntry {
            fill: self.fill_of(node, path)?,
            stroke: self.stroke_of(node, path)?,
            corners: CornerRadii {
                top_left: radius,
                top_right: radius,
                bottom_right: radius,
                bottom_left: radius,
            },
            shadows: shadows_of(node, path)?,
        };
        // A circle with neither fill, stroke, nor shadow draws nothing — the
        // corners alone shape no ink. An ellipse is a leaf, so it never clips.
        if entry.fill.is_none() && entry.stroke.is_none() && entry.shadows.is_empty() {
            return Ok(None);
        }
        Ok(Some(DocPaint { entry, clip: false }))
    }

    fn fill_of(&mut self, node: &Node, path: &str) -> Result<Option<PaintKind>, CompileError> {
        match single_visible_paint(&node.fills) {
            // A layout-only frame with no fill draws nothing.
            OnePaint::None => Ok(None),
            OnePaint::One(fill) => self.paint_kind(fill, path).map(Some),
            // PaintEntry.fill is one Option<PaintKind>; Figma's fills is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4.
            OnePaint::Many => Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "more than one visible fill".to_string(),
            }),
        }
    }

    fn paint_kind(&mut self, paint: &Paint, path: &str) -> Result<PaintKind, CompileError> {
        let unsupported = |what: &str| CompileError::Unsupported {
            path: path.to_string(),
            what: what.to_string(),
        };

        match paint.kind {
            PaintTag::Solid => {
                let color = paint
                    .color
                    .ok_or_else(|| unsupported("a SOLID with no color"))?;
                Ok(PaintKind::Solid {
                    color: color_of(color, paint.opacity),
                })
            }
            PaintTag::GradientLinear
            | PaintTag::GradientRadial
            | PaintTag::GradientAngular
            | PaintTag::GradientDiamond => {
                let handles = &paint.gradient_handle_positions;
                let [origin, primary, secondary] = handles[..] else {
                    return Err(unsupported("a gradient without three handles"));
                };
                Ok(PaintKind::Gradient(Gradient {
                    kind: match paint.kind {
                        PaintTag::GradientLinear => GradientKind::Linear,
                        PaintTag::GradientRadial => GradientKind::Radial,
                        PaintTag::GradientAngular => GradientKind::Angular,
                        _ => GradientKind::Diamond,
                    },
                    handle_origin: Vec2 {
                        x: origin.x,
                        y: origin.y,
                    },
                    handle_primary: Vec2 {
                        x: primary.x,
                        y: primary.y,
                    },
                    handle_secondary: Vec2 {
                        x: secondary.x,
                        y: secondary.y,
                    },
                    stops: paint
                        .gradient_stops
                        .iter()
                        // Figma calls the location `position`; dashpaint calls
                        // it `offset`.
                        .map(|s| GradientStop {
                            offset: s.position,
                            color: color_of(s.color, paint.opacity),
                        })
                        .collect(),
                }))
            }
            PaintTag::Image => {
                let image_ref = paint
                    .image_ref
                    .as_deref()
                    .ok_or_else(|| unsupported("an IMAGE fill with no imageRef"))?;

                let image = match self.image_of.get(image_ref) {
                    Some(index) => *index,
                    None => {
                        let asset = self.images.get(image_ref).ok_or_else(|| {
                            CompileError::UnresolvedImage {
                                path: path.to_string(),
                                image_ref: image_ref.to_string(),
                            }
                        })?;
                        let index = self.doc.push_image(asset.clone());
                        self.image_of.insert(image_ref.to_string(), index);
                        index
                    }
                };

                Ok(PaintKind::Image {
                    image,
                    scale_mode: match paint
                        .scale_mode
                        .ok_or_else(|| unsupported("an IMAGE fill with no scaleMode"))?
                    {
                        rest::ScaleMode::Fill => ScaleMode::Fill,
                        rest::ScaleMode::Fit => ScaleMode::Fit,
                        rest::ScaleMode::Crop => ScaleMode::Crop,
                        rest::ScaleMode::Tile => ScaleMode::Tile,
                    },
                    // Both are `dashpaint` vocabulary already, so dropping
                    // them would not be an expressiveness gap — it would
                    // lower a cropped or tiled image to a *wrong* image, in
                    // silence (P4). Figma's imageTransform is row-major
                    // `[[a, b, tx], [c, d, ty]]`, the same six components
                    // `Mat23` holds; absent means identity.
                    transform: paint.image_transform.map(|[[a, b, tx], [c, d, ty]]| Mat23 {
                        a,
                        b,
                        c,
                        d,
                        tx,
                        ty,
                    }),
                    tile_scale: paint.scaling_factor.unwrap_or(1.0),
                })
            }
        }
    }

    fn stroke_of(&mut self, node: &Node, path: &str) -> Result<Option<Stroke>, CompileError> {
        // strokeWeight and strokeAlign are present even when `strokes` is
        // empty (pinned by the fixture), so the stroke is gated on the array,
        // never on the weight.
        let stroke = match single_visible_paint(&node.strokes) {
            OnePaint::None => return Ok(None),
            // PaintEntry.stroke is one Option<Stroke>; Figma's strokes is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4. Same rule
            // as `fill_of`.
            OnePaint::Many => {
                return Err(CompileError::Unsupported {
                    path: path.to_string(),
                    what: "more than one visible stroke".to_string(),
                });
            }
            OnePaint::One(stroke) => stroke,
        };

        // dashpaint::Stroke is solid and uniform: one color, one width, one
        // align. A dashed or variable-width stroke has nothing to lower into,
        // so it is refused rather than repainted as a plain solid stroke of
        // the same color and weight — a drop the designer cannot see in the
        // output, which is exactly what P4 forbids. Every frame in
        // v03-paint.json carries `complexStrokeProperties.strokeType: BASIC`,
        // which is what pins the field shape (debt #145 covers the
        // variable-width case, which has no Construct variant either).
        if let Some(stroke_type) = node
            .complex_stroke_properties
            .as_ref()
            .and_then(|properties| properties.stroke_type.as_deref())
            && stroke_type != "BASIC"
        {
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: format!("a {stroke_type} stroke"),
            });
        }
        // Figma writes `strokeDashes: null` for a continuous stroke, and an
        // empty array means the same, so only a non-empty pattern is a drop.
        if node.stroke_dashes.as_ref().is_some_and(|d| !d.is_empty()) {
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "a dashed stroke".to_string(),
            });
        }

        let color = match stroke.kind {
            PaintTag::Solid => stroke.color.ok_or_else(|| CompileError::Unsupported {
                path: path.to_string(),
                what: "a SOLID stroke with no color".to_string(),
            })?,
            // v0.3 strokes are solid-only (dashpaint::Stroke).
            _ => {
                return Err(CompileError::Unsupported {
                    path: path.to_string(),
                    what: "a non-solid stroke".to_string(),
                });
            }
        };

        Ok(Some(Stroke {
            width: node.stroke_weight.unwrap_or(1.0),
            align: match node.stroke_align.unwrap_or(rest::StrokeAlign::Inside) {
                rest::StrokeAlign::Inside => StrokeAlign::Inside,
                rest::StrokeAlign::Center => StrokeAlign::Center,
                rest::StrokeAlign::Outside => StrokeAlign::Outside,
            },
            color: color_of(color, stroke.opacity),
        }))
    }

    /// Lowers a `TEXT` node's characters and style, or collects the reasons it
    /// cannot be lowered.
    ///
    /// The document's `TextStyle` carries family, em size, CSS-scale weight,
    /// and the fill color (story #26) — the axes the runtime shapes and the
    /// painter fills. Every other authored text feature has no vocabulary, so
    /// a non-default value on it is a named diagnostic (P4), never lowered as
    /// though it rendered: lowering centered text flush-left, or dropping a
    /// letter-spacing, would paint a picture the designer never authored. The
    /// returned pair is `(Some, Some)` only when nothing blocked; the caller
    /// discards it and skips the node whenever `blockers` is non-empty.
    fn text_of(
        &self,
        node: &Node,
        blockers: &mut Vec<String>,
    ) -> (Option<String>, Option<DocTextStyle>) {
        let Some(characters) = &node.characters else {
            // Figma always serializes `characters` on a `TEXT` node (an empty
            // box carries `""`, which lowers cleanly). Its absence is a
            // malformed node, not an empty one.
            blockers.push("a text node with no characters".to_string());
            return (None, None);
        };
        let Some(style) = &node.style else {
            blockers.push("a text node with no style".to_string());
            return (None, None);
        };

        // Multiple style segments: the schema is one style per node.
        if !node.style_override_table.is_empty() {
            blockers.push("multiple text style segments (styleOverrideTable)".to_string());
        }
        // Horizontal alignment: `LEFT` is the "no explicit alignment" state —
        // the runtime flushes an LTR paragraph left and an RTL one right by
        // direction (`docs/design/typeset-latin.md`), so it needs no field.
        // `CENTER`/`RIGHT`/`JUSTIFIED` have no vocabulary.
        match style.text_align_horizontal.as_deref() {
            None | Some("LEFT") => {}
            Some(other) => blockers.push(format!("text alignment {other}")),
        }
        // Vertical alignment within the box: `TOP` is the default the runtime
        // places from.
        match style.text_align_vertical.as_deref() {
            None | Some("TOP") => {}
            Some(other) => blockers.push(format!("vertical text alignment {other}")),
        }
        // Line height: `INTRINSIC_%` is Figma's "Auto" — the font's natural
        // line advance, which is exactly what the runtime uses. A fixed
        // percentage or pixel line height has no vocabulary.
        match style.line_height_unit.as_deref() {
            None | Some("INTRINSIC_%") => {}
            Some(other) => blockers.push(format!("a {other} line height")),
        }
        // Letter spacing: the runtime tracks none.
        if style.letter_spacing.is_some_and(|spacing| spacing != 0.0) {
            blockers.push("letter spacing".to_string());
        }
        // Italic / non-upright style: the document font reference is family +
        // weight only.
        if style
            .font_style
            .as_deref()
            .is_some_and(|s| s.contains("Italic"))
        {
            blockers.push("italic text".to_string());
        }
        // Text decoration (underline / strikethrough).
        if style
            .text_decoration
            .as_deref()
            .is_some_and(|d| d != "NONE")
        {
            blockers.push("text decoration".to_string());
        }
        // A case transform rewrites the rendered glyphs.
        if style.text_case.as_deref().is_some_and(|c| c != "ORIGINAL") {
            blockers.push("a text case transform".to_string());
        }
        // Truncation / ellipsis and unknown resize modes: the
        // `WIDTH_AND_HEIGHT`/`HEIGHT`/`NONE` modes map to sizing (see
        // `text_sizing`); `TRUNCATE` is the one value that has no equivalent,
        // and an unrecognised value has no mapping at all — both are refused
        // rather than fix-sized silently.
        match style.text_auto_resize.as_deref() {
            None | Some("WIDTH_AND_HEIGHT") | Some("HEIGHT") | Some("NONE") => {}
            Some("TRUNCATE") => blockers.push("text truncation".to_string()),
            Some(other) => blockers.push(format!("text auto-resize {other}")),
        }
        // A hyperlink on the run.
        if style.hyperlink.is_some() {
            blockers.push("a text hyperlink".to_string());
        }
        // OpenType feature flags.
        if !style.opentype_flags.is_empty() {
            blockers.push("OpenType features".to_string());
        }

        // A text outline (stroke) has no vocabulary — the style carries a fill
        // color only, so an outline is refused rather than dropped (P4). Gate
        // on the strokes array, never `strokeWeight` (present even with no
        // stroke) — the same rule `stroke_of` uses for a frame.
        if node.strokes.iter().any(|s| s.visible != Some(false)) {
            blockers.push("a text stroke (outline)".to_string());
        }

        // The fill: a single visible SOLID, lowered into the style's color. A
        // gradient, image, or stacked text fill has no lowering into one
        // color.
        let color = self.text_fill_of(node, blockers);

        // Weight lowers verbatim; the 100–900 range check is the validator's
        // (#41/#129). A fractional value (Figma serializes integers) rounds.
        let weight = style.font_weight.unwrap_or(400.0).round() as u16;

        let Some(color) = color else {
            // `text_fill_of` pushed the blocker; without a color there is no
            // valid style to build.
            return (Some(characters.clone()), None);
        };
        (
            Some(characters.clone()),
            Some(DocTextStyle {
                family: style.font_family.clone(),
                size: style.font_size,
                weight,
                color,
            }),
        )
    }

    /// A text node's glyph color: its single visible SOLID fill. A text node's
    /// `fills` array is the glyph color, so the same stacking and non-solid
    /// refusals `fill_of` applies to a frame apply here — a silent drop of one
    /// of two fills, or of a gradient, is exactly what P4 forbids.
    fn text_fill_of(&self, node: &Node, blockers: &mut Vec<String>) -> Option<Color> {
        let fill = match single_visible_paint(&node.fills) {
            OnePaint::None => {
                blockers.push("a text node with no fill".to_string());
                return None;
            }
            OnePaint::Many => {
                blockers.push("more than one visible text fill".to_string());
                return None;
            }
            OnePaint::One(fill) => fill,
        };
        match fill.kind {
            PaintTag::Solid => {
                let Some(color) = fill.color else {
                    blockers.push("a text SOLID fill with no color".to_string());
                    return None;
                };
                Some(color_of(color, fill.opacity))
            }
            _ => {
                blockers.push("a non-solid text fill".to_string());
                None
            }
        }
    }
}

/// A three-way selection over a node's `fills` or `strokes`: no visible paint,
/// exactly one, or a stack. Both a frame's paint entry (`fill_of`/`stroke_of`)
/// and a text node's glyph color (`text_fill_of`) take a single visible paint
/// and refuse a stack rather than silently pick one (P4); each caller maps the
/// three cases to its own verdict.
enum OnePaint<'a> {
    None,
    One(&'a Paint),
    Many,
}

fn single_visible_paint(paints: &[Paint]) -> OnePaint<'_> {
    let mut visible = paints.iter().filter(|p| p.visible != Some(false));
    match (visible.next(), visible.next()) {
        (None, _) => OnePaint::None,
        (Some(paint), None) => OnePaint::One(paint),
        (Some(_), Some(_)) => OnePaint::Many,
    }
}

/// The container-side flex intent of `node`, or `None` for a passthrough
/// (`layoutMode` absent or `NONE`).
///
/// The authored intent lowers — mode, gaps, padding, alignment, grid tracks
/// and placement — and never the solved boxes (P1, the surviving ground of
/// `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`). Since
/// v0.8 (story #264) `GRID` lowers onto `LayoutMode::Grid` with its track
/// lists, `layoutWrap: WRAP` onto `LayoutMode::Wrap`, and
/// `counterAxisAlignItems: BASELINE` onto `CrossAxisAlign::Baseline`
/// (`docs/decisions/v08-layout-vocabulary-shape.md`). What still has no
/// vocabulary is a named blocker, not an approximation.
fn container_of(node: &Node, blockers: &mut Vec<String>) -> Option<DocContainer> {
    let base_mode = match node.layout_mode.as_deref() {
        None | Some("NONE") => return None,
        Some("HORIZONTAL") => LayoutMode::Horizontal,
        Some("VERTICAL") => LayoutMode::Vertical,
        Some("GRID") => LayoutMode::Grid,
        Some(other) => {
            blockers.push(format!("auto-layout ({other})"));
            return None;
        }
    };

    // layoutWrap: WRAP turns a horizontal row into a wrapping row — Figma
    // allows wrap on horizontal auto-layout only (D1). WRAP on any other
    // mode, or an unknown wrap value, has no lowering and is refused.
    let mode = match node.layout_wrap.as_deref() {
        None | Some("NO_WRAP") => base_mode,
        Some("WRAP") if base_mode == LayoutMode::Horizontal => LayoutMode::Wrap,
        Some(wrap) => {
            blockers.push(format!(
                "wrapping auto-layout ({wrap}) on a non-horizontal frame"
            ));
            base_mode
        }
    };

    // counterAxisAlignContent distributes wrap lines, so it is meaningful
    // only under Wrap; only AUTO (the packed default the runtime already
    // carries) has a lowering, and SPACE_BETWEEN — or any other value —
    // has no vocabulary yet, so it is refused by name (P4) rather than
    // packed silently (`docs/decisions/v08-layout-vocabulary-shape.md`,
    // "Out of scope"). A stale value on a non-wrap frame is inert in Figma
    // too, so it is ignored rather than refused.
    if mode == LayoutMode::Wrap {
        match node.counter_axis_align_content.as_deref() {
            None | Some("AUTO") => {}
            Some(other) => {
                blockers.push(format!(
                    "wrap line distribution (counterAxisAlignContent {other})"
                ));
            }
        }
    }

    // The two gaps depend on the mode: Grid reads its per-axis grid gaps,
    // Wrap the item spacing plus the cross-axis line spacing, and H/V the
    // item spacing alone (its cross gap follows the main gap, so it lowers
    // as absent).
    let (gap, cross_gap) = match mode {
        LayoutMode::Grid => (node.grid_column_gap.unwrap_or(0.0), node.grid_row_gap),
        LayoutMode::Wrap => (node.item_spacing.unwrap_or(0.0), node.counter_axis_spacing),
        LayoutMode::Horizontal | LayoutMode::Vertical => (node.item_spacing.unwrap_or(0.0), None),
    };

    // Negative gaps under Wrap and Grid are refused by name, both axes
    // (`docs/decisions/v08-layout-vocabulary-shape.md` D5). A wrap gap has
    // no margin encoding: wrap decides its line breaks after the lowering,
    // so a leading margin would pull every later line's first chip into the
    // padding band and distort the breaks — the engine refuses the core
    // equivalent, and the dashc side matches. A grid gap is track spacing,
    // not flow spacing, so a leading margin would shift cell content rather
    // than overlap tracks; there is no margin form for it either. A
    // negative H/V main gap is not refused here — the walk rewrites it to a
    // leading margin (`docs/decisions/negative-gap-lowering.md`).
    let negative_gap = |value: f32, what: &str| (value < 0.0).then(|| what.to_string());
    match mode {
        LayoutMode::Wrap => {
            blockers.extend(negative_gap(
                gap,
                "a wrapping row with a negative gap (layoutWrap WRAP + negative itemSpacing)",
            ));
            blockers.extend(cross_gap.and_then(|g| {
                negative_gap(
                    g,
                    "a wrapping row with a negative cross gap \
                     (layoutWrap WRAP + negative counterAxisSpacing)",
                )
            }));
        }
        LayoutMode::Grid => {
            blockers.extend(negative_gap(
                gap,
                "a grid with a negative column gap (negative gridColumnGap)",
            ));
            blockers.extend(cross_gap.and_then(|g| {
                negative_gap(g, "a grid with a negative row gap (negative gridRowGap)")
            }));
        }
        LayoutMode::Horizontal | LayoutMode::Vertical => {}
    }

    // The grid track lists, parsed from Figma's serialized track strings.
    // Empty for a non-grid container; a track token with no vocabulary is
    // refused by name (P4).
    let grid_rows = grid_tracks_of(mode, node.grid_rows_sizing.as_deref(), blockers);
    let grid_columns = grid_tracks_of(mode, node.grid_columns_sizing.as_deref(), blockers);

    // Absent alignment is Figma's MIN — the start of the axis.
    let main_align = match node.primary_axis_align_items.as_deref() {
        None | Some("MIN") => MainAxisAlign::Start,
        Some("CENTER") => MainAxisAlign::Center,
        Some("MAX") => MainAxisAlign::End,
        Some("SPACE_BETWEEN") => MainAxisAlign::SpaceBetween,
        Some(other) => {
            blockers.push(format!("main-axis alignment {other}"));
            MainAxisAlign::Start
        }
    };
    let cross_align = match node.counter_axis_align_items.as_deref() {
        None | Some("MIN") => CrossAxisAlign::Start,
        Some("CENTER") => CrossAxisAlign::Center,
        Some("MAX") => CrossAxisAlign::End,
        // Baseline appended to the schema's CrossAxisAlign at v0.8 (Q-4).
        Some("BASELINE") => CrossAxisAlign::Baseline,
        Some(other) => {
            blockers.push(format!("cross-axis alignment {other}"));
            CrossAxisAlign::Start
        }
    };

    Some(DocContainer {
        mode,
        gap,
        padding: EdgeInsets {
            left: node.padding_left.unwrap_or(0.0),
            top: node.padding_top.unwrap_or(0.0),
            right: node.padding_right.unwrap_or(0.0),
            bottom: node.padding_bottom.unwrap_or(0.0),
        },
        main_align,
        cross_align,
        cross_gap,
        grid_rows,
        grid_columns,
    })
}

/// Parses one grid axis's serialized track string into the schema's track
/// vocabulary, collecting the reason any token cannot be lowered. Only
/// meaningful under `Grid`; every other mode carries no tracks. An absent
/// string under `Grid` is Figma's implicit auto tracks, which lower as an
/// empty list (the schema's "implicit auto tracks" state).
fn grid_tracks_of(
    mode: LayoutMode,
    sizing: Option<&str>,
    blockers: &mut Vec<String>,
) -> Vec<DocGridTrack> {
    if mode != LayoutMode::Grid {
        return Vec::new();
    }
    let Some(sizing) = sizing else {
        return Vec::new();
    };
    let mut tracks = Vec::new();
    for token in split_top_level_whitespace(sizing) {
        match parse_grid_track(token) {
            Some(track) => tracks.push(track),
            // A track the Fixed|Fraction vocabulary cannot express (an
            // `auto`, a `min-content`, a `minmax` with a non-zero minimum,
            // a non-finite length or weight): named refusal, never a
            // silent substitution (P4).
            None => blockers.push(format!("an unsupported grid track ({token})")),
        }
    }
    tracks
}

/// Splits a track string on **top-level** whitespace — whitespace outside
/// any parentheses — so a `minmax(0, 1fr)` with a space after the comma (a
/// valid CSS serialization) stays one token rather than over-splitting into
/// `minmax(0,` + `1fr)` and refusing the whole grid.
fn split_top_level_whitespace(s: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut depth: u32 = 0;
    let mut token_start: Option<usize> = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if ch.is_whitespace() && depth == 0 {
            if let Some(start) = token_start.take() {
                tokens.push(&s[start..i]);
            }
        } else if token_start.is_none() {
            token_start = Some(i);
        }
    }
    if let Some(start) = token_start {
        tokens.push(&s[start..]);
    }
    tokens
}

/// One Figma grid track token. `Npx` lowers to `Fixed(N)`; `minmax(0, Nfr)`
/// lowers to `Fraction(N)` — Figma's own serialized fraction form, whose
/// zero minimum (unlike a bare `fr`'s min-content one) divides free space
/// exactly the way the captured grid does (D2). Any other token, and any
/// token that parses to a non-finite value, has no lowering — `None` here
/// becomes a named refusal, so a `NaN`/`inf` track never reaches the
/// document even on the `lower` producer API that runs no load gate (P4).
fn parse_grid_track(token: &str) -> Option<DocGridTrack> {
    // `Npx`: a fixed length in document units.
    if let Some(px) = token.strip_suffix("px") {
        let value = px.trim().parse::<f32>().ok()?;
        return value.is_finite().then_some(DocGridTrack::Fixed(value));
    }
    // `minmax(min, Nfr)`: a fraction weight, valid only with a zero
    // minimum. The minimum is numeric-parsed and accepted when it is zero
    // in any unit Figma might serialize (`0`, `0px`, `0.0`, `0%`); the
    // weight is read off the `Nfr` maximum.
    if let Some(inner) = token
        .strip_prefix("minmax(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (min, max) = inner.split_once(',')?;
        if !parses_to_zero(min.trim()) {
            return None;
        }
        let weight = max.trim().strip_suffix("fr")?.trim().parse::<f32>().ok()?;
        return weight.is_finite().then_some(DocGridTrack::Fraction(weight));
    }
    None
}

/// Whether a grid track's minmax minimum is zero in any unit Figma might
/// serialize (`0`, `0px`, `0.0`, `0%`). A zero minimum is the only one the
/// `Fraction` lowering expresses: it divides free space with no floor.
fn parses_to_zero(min: &str) -> bool {
    let number = min
        .strip_suffix("px")
        .or_else(|| min.strip_suffix('%'))
        .unwrap_or(min);
    number.trim().parse::<f32>().is_ok_and(|value| value == 0.0)
}

/// The child-side flex intent of `node`, or `None` when every field is the
/// default (the schema's absent-table state, so the fixed-layout fixtures
/// emit byte-identically to before this vocabulary existed).
fn constraints_of(
    node: &Node,
    flow: Option<&Flow>,
    blockers: &mut Vec<String>,
) -> Option<LayoutConstraints> {
    // Absent sizing is a node outside any auto-layout context: fixed.
    let mut sizing = |field: &Option<String>, axis: &str| match field.as_deref() {
        None | Some("FIXED") => AxisSizing::Fixed,
        Some("HUG") => AxisSizing::Hug,
        Some("FILL") => AxisSizing::Fill,
        Some(other) => {
            blockers.push(format!("{axis} sizing {other}"));
            AxisSizing::Fixed
        }
    };
    let sizing_h = sizing(&node.layout_sizing_horizontal, "horizontal");
    let sizing_v = sizing(&node.layout_sizing_vertical, "vertical");

    // A Fill child on an axis its parent hugs is a sizing cycle Figma and
    // CSS resolve differently: Figma falls back to the child's stored size
    // — solver state P1 forbids reading — while a CSS flex solve derives
    // the hug from the children's content. Lowering it would solve to a
    // different picture than Figma renders, so it is refused (pinned by
    // variables-bound.json, whose Fill cards sit in a hug root). Each
    // message names its axis: a child can trip both at once, and two
    // identical diagnostics would read as one finding.
    if let Some(flow) = flow {
        if flow.parent_hugs_h && sizing_h == AxisSizing::Fill {
            blockers.push("a Fill child on its parent's hug axis (horizontal)".to_string());
        }
        if flow.parent_hugs_v && sizing_v == AxisSizing::Fill {
            blockers.push("a Fill child on its parent's hug axis (vertical)".to_string());
        }
    }

    let margin = flow
        .filter(|f| f.leading_margin != 0.0)
        .map(|f| {
            // The leading main-axis edge: left in a row, top in a column —
            // the same rule as core's `Txn::lower_negative_gaps`.
            if f.horizontal {
                EdgeInsets {
                    left: f.leading_margin,
                    ..EdgeInsets::default()
                }
            } else {
                EdgeInsets {
                    top: f.leading_margin,
                    ..EdgeInsets::default()
                }
            }
        })
        .unwrap_or_default();

    let constraints = LayoutConstraints {
        sizing_h,
        sizing_v,
        min_width: node.min_width,
        max_width: node.max_width,
        min_height: node.min_height,
        max_height: node.max_height,
        margin,
        // Grid placement (story #264): the 0-based anchor cell and the
        // track span. Absent anchors are auto-placement; absent spans
        // default to 1, matching the schema and `Default`.
        grid_row: node.grid_row_anchor_index,
        grid_column: node.grid_column_anchor_index,
        grid_row_span: node.grid_row_span.unwrap_or(1),
        grid_column_span: node.grid_column_span.unwrap_or(1),
    };
    (constraints != LayoutConstraints::default()).then_some(constraints)
}

/// A `TEXT` node's per-axis sizing, reconciling the two Figma encodings.
///
/// The modern `layoutSizingHorizontal`/`layoutSizingVertical` pair is the
/// sizing source when present (D1) — it is what Figma's layout engine
/// renders, so it wins over a `textAutoResize` that disagrees (Figma keeps the
/// two consistent, so a disagreement is stale input). `constraints_of` has
/// already lowered that pair into `from_layout_sizing`, so when either axis
/// carries it, that result stands.
///
/// Outside auto-layout Figma sets no `layoutSizing*` at all, so
/// `textAutoResize` is the sizing source. Without this a free-standing label
/// would lower `Fixed`/`Fixed` from `constraints_of`'s absent-is-fixed
/// default and carry its resolved box as authored extent — but Figma's
/// default for a text box is auto (`WIDTH_AND_HEIGHT` — hug both). `HEIGHT`
/// fixes the width and grows the height; `NONE` fixes the box. For a
/// free-standing node the `absoluteBoundingBox` is authored (the designer
/// placed and sized it — it is not an auto-layout solver result, so P1 permits
/// a Fixed axis to read it). `TRUNCATE` and any unknown value are refused in
/// [`Walk::text_of`], so the `NONE`-equivalent fixed fallback here is never
/// emitted for them.
fn text_sizing(
    node: &Node,
    from_layout_sizing: Option<LayoutConstraints>,
) -> Option<LayoutConstraints> {
    if node.layout_sizing_horizontal.is_some() || node.layout_sizing_vertical.is_some() {
        return from_layout_sizing;
    }
    let (sizing_h, sizing_v) = match node
        .style
        .as_ref()
        .and_then(|s| s.text_auto_resize.as_deref())
    {
        Some("WIDTH_AND_HEIGHT") | None => (AxisSizing::Hug, AxisSizing::Hug),
        Some("HEIGHT") => (AxisSizing::Fixed, AxisSizing::Hug),
        _ => (AxisSizing::Fixed, AxisSizing::Fixed),
    };
    let constraints = LayoutConstraints {
        sizing_h,
        sizing_v,
        ..from_layout_sizing.unwrap_or_default()
    };
    (constraints != LayoutConstraints::default()).then_some(constraints)
}

/// `cornerRadius` and `rectangleCornerRadii` are mutually exclusive — Figma
/// nulls whichever does not apply. `rectangleCornerRadii` is
/// `[top_left, top_right, bottom_right, bottom_left]`, matching
/// `CornerRadii`'s field order.
fn corners_of(node: &Node) -> CornerRadii {
    if let Some([top_left, top_right, bottom_right, bottom_left]) = node.rectangle_corner_radii {
        return CornerRadii {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        };
    }
    let r = node.corner_radius.unwrap_or(0.0);
    CornerRadii {
        top_left: r,
        top_right: r,
        bottom_right: r,
        bottom_left: r,
    }
}

/// Lowers a node's visible `DROP_SHADOW`/`INNER_SHADOW` effects into the
/// paint entry's shadow list, in Figma's effect order (story #45). Non-shadow
/// effects (noise, blur) are triaged in [`triage::constructs_of`], not here.
/// A hidden effect is skipped, like a hidden paint. A shadow with no color has
/// no meaning and is refused by name (P4) — the same posture as a `SOLID` with
/// no color; `Unsupported` becomes a blocker at the caller.
///
/// Figma's `showShadowBehindNode` is not modeled: the REST subset is
/// deliberately partial, and this painter always draws a drop shadow behind
/// the node (the Figma default). That is a documented fidelity limitation, not
/// a dropped field the schema could carry.
fn shadows_of(node: &Node, path: &str) -> Result<Vec<Shadow>, CompileError> {
    let mut shadows = Vec::new();
    for effect in node.effects.iter().filter(|e| e.visible != Some(false)) {
        let kind = match effect.kind.as_str() {
            "DROP_SHADOW" => ShadowKind::Drop,
            "INNER_SHADOW" => ShadowKind::Inner,
            _ => continue,
        };
        let Some(color) = effect.color else {
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "a shadow with no color".to_string(),
            });
        };
        let offset = effect.offset.unwrap_or(rest::Vector { x: 0.0, y: 0.0 });
        shadows.push(Shadow {
            kind,
            offset: Vec2 {
                x: offset.x,
                y: offset.y,
            },
            blur: effect.radius.unwrap_or(0.0),
            spread: effect.spread.unwrap_or(0.0),
            color: color_of(color, None),
        });
    }
    Ok(shadows)
}

/// Figma's paint `opacity` multiplies the color's alpha. Ignoring it would be
/// a silent drop (P4); it is two lines, so it is not one.
fn color_of(color: rest::Color, opacity: Option<f32>) -> Color {
    Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a * opacity.unwrap_or(1.0),
    }
}
