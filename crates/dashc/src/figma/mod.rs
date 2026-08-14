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
pub(crate) mod prototype;
pub mod rest;
pub(crate) mod triage;
pub mod variants;
pub mod vector_field;

pub use bindings::{BoundValue, BoundVariable};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde::Deserialize;

use dashpaint::{
    Blur, BlurKind, Color, CornerRadii, FillSpec, Gradient, GradientKind, GradientStop, ImageAsset,
    ImageFill, Mat23, ScaleMode, Shadow, ShadowKind, StopRange, Stroke, StrokeAlign, Vec2,
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
    Asset, AssetKind, AxisSizing, Box2D, CrossAxisAlign, Document, EdgeInsets,
    GridTrack as DocGridTrack, LayoutConstraints, LayoutContainer as DocContainer, LayoutMode,
    MainAxisAlign, Node as DocNode, Paint as DocPaint, PaintEntry, TextAlign as DocTextAlign,
    TextAlignV as DocTextAlignV, TextStyle as DocTextStyle, VectorAtlas as DocVectorAtlas,
    VectorShape as DocVectorShape,
};
use crate::figma::rest::{Effect, FigmaFile, Geometry, Node, Paint};
use crate::figma::vector_field::{VectorAtlasBaker, VectorFieldError, VectorPath, WindingRule};

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

    /// A refused node had descendants, and they went with it (issue #875).
    ///
    /// Separate from [`UNSUPPORTED`] because it answers a different question.
    /// That rule says *what* was refused; this one says *how much* went with it.
    /// The walk returns before pushing the refused node's children onto the
    /// visit stack, so before this rule existed a subtree of any depth **left
    /// exactly one diagnostic naming only its root**.
    ///
    /// Dropping the subtree is correct — the children have no parent to attach
    /// to, and re-parenting them onto the grandparent would move them in the
    /// tree and change what the solver sees. The defect was in the reporting:
    /// P4 was satisfied for the refused node and not for what went with it, so
    /// a reader saw one warning and had to work out for themselves that the
    /// missing content was downstream of it.
    ///
    /// Emitted once per refused node, never once per blocker: a node with three
    /// blockers loses its subtree once.
    pub const SUBTREE_DROPPED: &str = "figma.subtree-dropped";

    /// The walk resolved every top-level node to no paintable content — the
    /// definitions-only case: a canvas holding only `COMPONENT`/`COMPONENT_SET`
    /// resolves but paints nothing
    /// (`docs/decisions/figma-component-lowering.md`), so the document would
    /// emit as zero nodes. Always an error: a zero-node `.dsb` is a picture with
    /// no roots, which a downstream consumer panics loading, and emitting it in
    /// silence is what P4 forbids. The message names what was skipped and why.
    pub const NO_CONTENT: &str = "figma.no-content";

    // The four image-identification diagnostics (story #400,
    // `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`). All four
    // are always errors, never a `figma.unsupported`-style warning under
    // `EmitPolicy::Partial`: an image that cannot be identified cannot be
    // given an intrinsic size, and #107's asset entry needs one — there is no
    // approximation to fall back to. That is also why these bypass the
    // `unsupported`/blockers mechanism entirely: they abort the compile via
    // `CompileError::Diagnostics` the moment they are found, unconditionally
    // in both emit policies. `CompileError::UnresolvedImage`, the other
    // caller-contract failure over the same `images` map, no longer aborts
    // quite so unconditionally: issue #484 folds it into a node's existing
    // skip under `EmitPolicy::Partial` when the node has another blocker
    // already (`fills_of`), and only aborts immediately the way these four
    // always do when the node has none.

    /// The bytes an `imageRef` resolved to match none of the three
    /// signatures `dashpaint::image_id` knows (PNG, JPEG, GIF).
    pub const IMAGE_UNKNOWN_SIGNATURE: &str = "figma.image-unknown-signature";
    /// The bytes' own signature names a format that contradicts the
    /// producer's tag on the `ImageAsset` — the asymmetry story #400 closes.
    pub const IMAGE_FORMAT_MISMATCH: &str = "figma.image-format-mismatch";
    /// The signature matched, but the header itself is truncated or
    /// malformed for its declared format (`dashpaint::image_id::ImageIdError::Malformed`).
    pub const IMAGE_HEADER_MALFORMED: &str = "figma.image-header-malformed";
    /// The header parsed cleanly but reports a zero width or height.
    pub const IMAGE_ZERO_DIMENSION: &str = "figma.image-zero-dimension";
}

/// One image-identification diagnostic (`rule::IMAGE_*`), always
/// `Severity::Error` — unlike `Walk::unsupported_at`, the emit policy never
/// downgrades these (see the `rule` module doc). Wrapped in a
/// [`CompileError::Diagnostics`] of exactly one entry so it propagates
/// through the same `Err(other) => return Err(other)` path every paint call
/// site already has for `CompileError::UnresolvedImage`, aborting the
/// compile immediately rather than continuing the walk — an unidentifiable
/// image is a caller-contract failure over the `images` map, not a
/// vocabulary gap the rest of the document can ship without.
///
/// `index` is the advisory locator [`Walk::unsupported_at`] uses:
/// the document index this node would have taken had it lowered.
fn image_diagnostic(rule: &'static str, index: u32, path: &str, message: String) -> CompileError {
    CompileError::Diagnostics(
        std::iter::once(Diagnostic {
            rule,
            severity: Severity::Error,
            at: Location::Node(NodePath::new(index, path.to_string())),
            message,
        })
        .collect(),
    )
}

/// The JSON nesting depth `parse_file` accepts.
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
    /// A caller-contract failure, not a vocabulary verdict — it aborts under
    /// `EmitPolicy::Strict` always, and under `EmitPolicy::Partial` whenever
    /// the carrying node has no other blocker (issue #484,
    /// `docs/decisions/dashc-identifies-images-never-decodes.md`). When the
    /// node already has another blocker under `Partial`, `fills_of` folds
    /// this into that node's skip instead, named alongside its other
    /// blocker(s), rather than returning this variant.
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
/// `Document.bindings` — see `bindings::apply` for the property →
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
/// The policy rides on the `Walk` and reaches two diagnostics. Under
/// [`crate::EmitPolicy::Partial`] a `figma.unsupported` omission
/// (`Walk::unsupported_at`) and the `figma.subtree-dropped` that names what
/// went with it (`Walk::refused_subtree`, issue #875) are both minted at
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
        baker: VectorAtlasBaker::new(),
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

    // Finish the baked-vector atlas (story B1): pack every unique field into
    // one PNG, append it to the asset table, and record the atlas and each
    // shape's placement. Skipped when no `VECTOR` node lowered, so a
    // vector-free document carries no atlas and emits byte-identically (R7).
    // The shape indices the walk stamped onto paint entries are the values
    // `baker.add` returned, which index this same `vector_shapes` list.
    if !walk.baker.is_empty() {
        // Packing cannot fail once each field baked (the fallible step is
        // `baker.add`, at the walk); the `Result` is the generator's uniform
        // signature.
        let bake = std::mem::take(&mut walk.baker)
            .finish()
            .expect("vector atlas packing is infallible once the fields baked");
        // The second path into `push_image`, and the only one story #400's
        // gate does not cover. That is deliberate: the gate verifies a
        // *producer's* format tag against bytes the compiler did not make,
        // and these bytes are the compiler's own atlas encoder's output, whose
        // format is not asserted by anyone. Running `identify` here would test
        // our encoder, not an input.
        //
        // Said out loud because two ungated-versus-gated paths into one sink is
        // the shape #396 records — a construct that is triage-clean and still
        // never lowered, because two walks over the same data drifted apart.
        // The atlas's intrinsic size comes from the baker, which already knows
        // it, so story #107 does not need a header parse here either.
        let image = walk.doc.push_asset(Asset {
            format: dashpaint::ImageFormat::Png,
            // A multi-channel distance field, not a picture. This is the only
            // place that knows — the payload is a PNG either way, so nothing
            // downstream could tell it apart from an image fill by inspection,
            // and a packer that guessed would put a baked vector on a lossy
            // ladder (docs/decisions/asset-quality-profile-bands.md).
            kind: AssetKind::DistanceField,
            bytes: bake.image_png,
            // The baker already knows the sheet's extent, so this path needs no
            // header parse — which is why the image gate's exemption below costs
            // story #107 nothing.
            width: bake.width,
            height: bake.height,
        });
        walk.doc.vector_atlases.push(DocVectorAtlas {
            image,
            px_per_em: bake.px_per_em as f32,
            distance_range: bake.distance_range as f32,
        });
        for placement in &bake.shapes {
            let r = placement.atlas_rect;
            let p = placement.plane_bounds;
            walk.doc.vector_shapes.push(DocVectorShape {
                atlas: 0,
                atlas_rect: [r.x, r.y, r.width, r.height],
                plane_bounds: [p.left as f32, p.top as f32, p.right as f32, p.bottom as f32],
            });
        }
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

    // The variant table and the prototype interactions that animate it
    // (story #773), applied after the walk for the same reason the binding
    // rows are: a `VariantOverride` names a document node index, so every
    // node has to have landed first. This pass is also the only one that
    // reads a `COMPONENT_SET` — the walk skips definitions whole — so it
    // traverses the file itself rather than riding on the walk's stack.
    let variant_diagnostics = variants::apply(&mut walk.doc, file, &walk.index_of_id, walk.policy);
    walk.diagnostics.extend(variant_diagnostics);

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
            if paint.kind == "IMAGE"
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
    /// The baked-vector atlas builder (story B1). Every `VECTOR` node's
    /// geometry bakes into this one shared atlas; identical geometry dedups by
    /// path hash. Finished after the walk into `Document.vector_atlases` /
    /// `vector_shapes` and one packed atlas PNG in the asset table.
    baker: VectorAtlasBaker,
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
    /// `Err` is reserved for the caller-contract failures over the `images`
    /// map, not a vocabulary verdict: [`CompileError::UnresolvedImage`], and
    /// — story #400 — [`CompileError::Diagnostics`] carrying exactly one
    /// `rule::IMAGE_*` finding when an `imageRef`'s bytes fail the P4
    /// identification gate.
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
            && node.kind != "VECTOR"
        {
            let index = self.doc.nodes.len() as u32;
            self.unsupported_at(index, path, format!("node type {}", node.kind));
            // The commoner of the two subtree drops by far: any node kind the
            // walk does not lower reaches here, where a blocker needs a
            // specific construct (issue #875).
            self.refused_subtree(index, path, node);
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

        // Rotation lowers as of story #770, but only where the document can
        // carry it faithfully. Figma omits `rotation` entirely when it is
        // zero, so `None` and `Some(0.0)` both mean unrotated and reach
        // none of this.
        //
        // Two shapes are still refused by name rather than lowered wrong
        // (P4):
        //
        // A rotated node **with children** — because a rotation in this
        // document does not compose down the tree. `Prop::Rotation` is
        // per-node paint intent: the commit walk resolves every node's box
        // absolutely and hands the painter one rect per node, and a clip
        // region is an axis-aligned box, so nothing carries a parent's turn
        // onto a descendant. Figma's rotation *is* hierarchical, so
        // accepting a rotated frame would draw its frame turned and its
        // contents straight — a plausible picture that is silently wrong,
        // which is the failure the capability mechanism exists to prevent.
        // Whether the document should gain a composing transform is issue
        // #845, deliberately not decided here.
        //
        // A rotated node **without `size`** — because the extent would have
        // to come from `absolute_bounding_box`, which for a rotated node is
        // the bounds of the rotated shape rather than the node's own box
        // (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
        if node.rotation.is_some_and(|r| r != 0.0) {
            if !node.children.is_empty() {
                blockers.push(
                    "a rotated node with children (a rotation does not compose down the tree)"
                        .to_string(),
                );
            }
            if node.size.is_none() {
                blockers.push("a rotated node with no size".to_string());
            }
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
        // Whether the type-specific lowering below ran `shadows_of`, so the
        // guard after the chain knows whether this node's shadows reached the
        // document (debt #396). It starts false on purpose: a lowering path
        // added later that forgets to lower shadows names the gap instead of
        // dropping it, which is the direction P4 requires the mistake to fall.
        let mut shadows_lowered = false;

        // Debt #485. An IMAGE fill registers its asset as a side effect of
        // `paint_kind` inspecting it, before this node's blockers are fully
        // known — a later blocker (a sibling fill, a stroke, an effect) can
        // still skip the node whole. Recording the table length here and
        // truncating back to it below, whenever this node turns out blocked,
        // undoes exactly the registrations this node's own inspection made:
        // nothing else can have referenced an index this node just minted,
        // since a node's children are only visited after it returns.
        let assets_before = self.doc.assets.len();
        // Debt #356, the same shape as the asset table above. `vector_paint_of`
        // registers a VECTOR's geometry with the baker before this node's
        // blockers are all known — the effect triage and the shadow carrier
        // guard both run after it — so a node skipped afterwards used to leave
        // its baked tile in the packed atlas with no paint entry referencing it.
        let shapes_before = self.baker.len();
        if node.kind == "TEXT" {
            let (t, ts) = self.text_of(node, &mut blockers);
            text = t;
            text_style = ts;
            // A TEXT node builds no `PaintEntry`, so it has nowhere to put a
            // blur. Story #393 made backdrop blur lowered vocabulary, which
            // removed the diagnostic it used to raise — so without this the
            // blur would vanish with nothing reported, the silent drop P4
            // forbids. Naming it keeps the gap visible until a text node can
            // carry paint (no measured need has asked for one yet).
            if !blurs_of(node).is_empty() {
                blockers.push("a blur on a text node".to_string());
            }
            // Outside auto-layout Figma sets no `layoutSizing*`, so
            // `textAutoResize` is the sizing source (a free-standing label
            // must hug, not fix-size from its resolved box).
            constraints = text_sizing(node, constraints);
        } else if node.kind == "ELLIPSE" {
            // A leaf: no container. The paint lowering keeps its own refusals
            // (a stacked fill, a dashed stroke), and the ellipse gate adds its
            // own (an arc, a ring, a non-circular or non-fixed ellipse); each
            // blocks the node like any other finding. `UnresolvedImage` aborts.
            paint = self.ellipse_paint_of(node, path, constraints, &mut blockers)?;
            shadows_lowered = true;
        } else if node.kind == "VECTOR" {
            // A leaf: no container. Its geometry bakes into an MSDF field
            // carried on the paint entry as a coverage mask (story B1). The
            // field-input selection rule and the bake keep their own refusals;
            // each blocks the node like any other finding. `UnresolvedImage`
            // aborts (an image-filled vector defers to the caller's contract).
            paint = self.vector_paint_of(node, path, &mut blockers)?;
        } else {
            container = container_of(node, &mut blockers);
            // The paint lowering keeps its own refusals; each unsupported
            // paint construct blocks the node like any other finding.
            // `UnresolvedImage` aborts: it is the caller's contract, not the
            // designer's file.
            paint = self.paint_of(node, path, &mut blockers)?;
            shadows_lowered = true;
        }

        // The P4 coupling guard for shadows (debt #396). `constructs_of` and
        // `shadows_of` are two independent walks over `node.effects`:
        // `effect_construct` returns `None` for a drop or inner shadow
        // *because* `shadows_of` lowers it, so "triage has nothing to say"
        // equals "lowers cleanly" only where the lowering actually ran. It
        // does not run on a TEXT node, which builds no `PaintEntry`, nor on a
        // baked VECTOR, whose silhouette is its MSDF field rather than the
        // parametric box the painter's shadow draws use. Without this the
        // effect reaches neither walk and vanishes with nothing reported —
        // the silent drop P4 forbids.
        //
        // The guard is stated over the *carrier*, not per node kind, so a
        // path added later cannot re-open the same gap silently.
        if !shadows_lowered
            && node
                .effects
                .iter()
                .any(|e| visible_shadow_kind(e).is_some())
        {
            blockers.push(format!(
                "a shadow on a {} node",
                node.kind.to_ascii_lowercase()
            ));
        }

        // The import gate: the producer maps, the validator decides (P5).
        // Unmapped effects (a baked shadow, debt #144) have no Construct and
        // block the node instead.
        let (constructs, effect_blockers) = triage::constructs_of(node);
        blockers.extend(effect_blockers);

        // Story #393 removed the one construct this gate omitted whole. A
        // backdrop blur used to be an error under profile:core, and shipping
        // the node without its blur would have approximated, so under Partial
        // the node was omitted entirely and the gap reported as
        // `figma.unsupported`. Backdrop blur is now core vocabulary that
        // lowers through `blurs_of`, so there is nothing left to omit and the
        // whole-node skip is gone with it. The mechanism this replaced is
        // described in
        // `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`
        // under "Consequence accepted at this gate"; if another construct ever
        // needs the same treatment, that is where the reasoning lives.

        if !blockers.is_empty() {
            // Undo any asset this node's own paint lowering registered above
            // (debt #485): the node never enters the document, so nothing is
            // left to name an asset that reached the table only because this
            // node's fill happened to be inspected before its blocker was
            // found. `image_of` also drops its entries for those indices, or
            // a later node reusing the same imageRef would cache-hit an
            // index that no longer names what it used to.
            self.doc.assets.truncate(assets_before);
            self.image_of
                .retain(|_, index| (*index as usize) < assets_before);
            // And any field this node's own geometry baked (debt #356). A
            // dedup hit registered nothing, so this is a no-op for a repeated
            // icon — only a shape this node was the first to bake goes away,
            // which is exactly the one nothing else can reference.
            self.baker.truncate(shapes_before);

            // The index this node would have taken had it lowered. The node is
            // skipped either way — refused under Strict, omitted with a warning
            // under Partial — so it never enters the document; the index is an
            // advisory locator, the path is the stable half, and two skipped
            // siblings may share one.
            let index = self.doc.nodes.len() as u32;
            for what in blockers {
                self.unsupported_at(index, path, what);
            }
            // Once for the node, not once per blocker: a node with three
            // blockers loses its subtree once (issue #875).
            self.refused_subtree(index, path, node);
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

        // Story #770. For an unrotated node the bounding box *is* the node's
        // box, and everything below reduces to what it computed before this
        // vocabulary. For a rotated one the bounding box is the axis-aligned
        // bounds of the rotated shape — a result (P1) — and is wrong in both
        // halves: its extent is too large (22.5 % at 15°, unbounded as the
        // aspect ratio grows), and its top-left is not the node's origin.
        //
        // `size` carries the node's own extent, and the origin is recovered
        // by subtracting the rotated box's own offset to those bounds. A
        // rotated node with no `size` is a blocker above, so the fallback
        // here is only ever taken by an unrotated one.
        let rotation = node.rotation.unwrap_or(0.0);
        let (own_w, own_h) = match node.size {
            Some(size) if rotation != 0.0 => (size.x, size.y),
            _ => (bbox.width, bbox.height),
        };
        let (bounds_dx, bounds_dy) = rotated_bounds_offset(rotation, own_w, own_h);
        // The node's own top-left, page-absolute: where its local (0, 0)
        // sits, which is also the point Figma turns it about.
        let node_origin = (bbox.x - bounds_dx, bbox.y - bounds_dy);

        // Where a frame sits on the Figma page is a page-layout artifact, not
        // intent (P1). The root has no parent to be relative to, so it is
        // relative to itself and lowers to (0, 0, w, h).
        let origin = visit.parent_origin.unwrap_or(node_origin);
        // Inside a flex parent the solver owns placement, so the box Figma
        // reports is its solver's output, not authored intent — the P1 ground
        // of `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`.
        // The same split applies per axis to the size: a Fixed axis's extent
        // is the authored datum, a Hug/Fill axis's extent is solved. What is
        // not intent lowers as zero — the absence the solver ignores.
        let (x, y) = if visit.flow.is_some() {
            (0.0, 0.0)
        } else {
            (node_origin.0 - origin.0, node_origin.1 - origin.1)
        };
        let sizing = constraints.unwrap_or_default();
        let width = if sizing.sizing_h == AxisSizing::Fixed {
            own_w
        } else {
            0.0
        };
        let height = if sizing.sizing_v == AxisSizing::Fixed {
            own_h
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
            // Story #770. Figma turns a node about its own local origin, so
            // the anchor is `(0, 0)` — the row this repository's ruling gives
            // for this producer
            // (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
            // The angle needs no conversion: it is already radians, and
            // already this repository's sign convention (`rest.rs`).
            rotation,
            rotation_anchor: (0.0, 0.0),
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
    ///
    /// The index is the caller's, because a refusal now reports twice at the
    /// same one: the construct, and the subtree that went with it
    /// ([`Walk::refused_subtree`], issue #875). A thin wrapper computing
    /// `self.doc.nodes.len()` for one of the two would let them disagree.
    fn unsupported_at(&mut self, index: u32, path: &str, what: String) {
        // One of the two policy-sensitive diagnostics (story S0-impl;
        // `figma.subtree-dropped` is the other, issue #875): an omission is
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

    /// Names the subtree a refusal took with it, at the same index the refusal
    /// itself was reported at (issue #875).
    ///
    /// `fn visit` has **three** early returns before the child-push loop, and
    /// this is called at two of them: the unsupported-node-kind arm and the
    /// blocker verdict. The third — a `COMPONENT` or `COMPONENT_SET`, which
    /// resolves but paints nothing — is deliberately exempt: nothing below a
    /// definition would have reached the document anyway, so a count there would
    /// name a loss that did not happen.
    ///
    /// Silent for a leaf,
    /// because there is nothing extra to say about one — the refusal's own
    /// diagnostic already covers it, and a line reading "0 descendants" on
    /// every refused rectangle would bury the case that matters.
    ///
    /// Severity follows the emit policy for the same reason
    /// [`Walk::unsupported_at`] does: under `Strict` the file is withheld
    /// anyway, and under `Partial` — which is what the production importer runs
    /// — this is the line that makes the hole visible at all.
    fn refused_subtree(&mut self, index: u32, path: &str, node: &Node) {
        // Iterative, with an explicit stack, for the reason the whole walk is
        // (debt #148): tree depth costs heap and never call stack. `lower` is
        // public and takes an already-built `FigmaFile`, so the parse-side
        // `MAX_JSON_DEPTH` does not bound what reaches here — and `dashc`
        // compiles to `wasm32-unknown-unknown`, where a stack overflow is a trap
        // and `docs/decisions/dashc-wasm-abi.md` promises a status instead.
        // `image_refs` above has the same shape.
        let mut count = 0usize;
        let mut stack: Vec<&Node> = node.children.iter().collect();
        while let Some(next) = stack.pop() {
            count += 1;
            stack.extend(next.children.iter());
        }
        if count == 0 {
            return;
        }
        let severity = match self.policy {
            crate::EmitPolicy::Strict => Severity::Error,
            crate::EmitPolicy::Partial => Severity::Warning,
        };
        self.diagnostics.push(Diagnostic {
            rule: rule::SUBTREE_DROPPED,
            severity,
            at: Location::Node(NodePath::new(index, path)),
            message: format!(
                "this layer was refused, so the {count} layer(s) below it in the Figma file were \
                 dropped with it and are not named individually; they have no parent to attach \
                 to. Some may not have lowered on their own terms either"
            ),
        });
    }

    /// Lowers a parametric (rounded-box) node's paint, collecting the reasons
    /// it cannot be lowered.
    ///
    /// The fill, the stroke and the shadows are three independent findings, so
    /// all three are evaluated and each names its own refusal (debt #329). They
    /// used to be built with `?` in struct-field order, which made Rust's
    /// left-to-right field evaluation the reporting order: the first refusal
    /// short-circuited the rest, and a designer had to fix one construct and
    /// recompile to learn the next. P4 asks for every out-of-profile construct
    /// to be named, so one pass now names them all.
    fn paint_of(
        &mut self,
        node: &Node,
        path: &str,
        blockers: &mut Vec<String>,
    ) -> Result<Option<DocPaint>, CompileError> {
        // Story C1 (debt #146): a node's visible fills lower as a stack — the
        // first (bottom) becomes `fill`, the rest (`extra_fills`) are painted
        // over it in the same array order.
        let mut fills = self.fills_of(node, path, blockers)?;
        let fill = (!fills.is_empty()).then(|| fills.remove(0));
        let entry = PaintEntry {
            fill,
            extra_fills: fills,
            stroke: self.stroke_of(node, blockers),
            corners: corners_of(node),
        };

        // A layout-only container draws nothing but still occupies a rect-table
        // slot. A clipping frame with no paint still needs its clip intent.
        let shadows = shadows_of(node, blockers);
        let blurs = blurs_of(node);
        // The effects are no longer part of the entry, so `default()` alone
        // no longer means "draws nothing": a node whose only paint is a drop
        // shadow would compare equal to it and be dropped (story #578).
        if entry == PaintEntry::default()
            && shadows.is_empty()
            && blurs.is_empty()
            && !node.clips_content
        {
            return Ok(None);
        }
        Ok(Some(DocPaint {
            entry,
            shadows,
            blurs,
            clip: node.clips_content,
            // A parametric (rounded-box) node carries no baked shape; the
            // VECTOR arm is the only place a shape index is set.
            shape_field: None,
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
        // circle radius stands in). A refused fill, stroke or shadow adds its
        // own blocker alongside the ellipse gates above (debt #329).
        let entry = PaintEntry {
            fill: self.fill_of(node, path, blockers)?,
            // An ellipse keeps the single-fill restriction (`fill_of`); the
            // stacked-fill vocabulary (story C1) widens `paint_of` only — no
            // stacked-fill ellipse is in the measured need.
            extra_fills: Vec::new(),
            stroke: self.stroke_of(node, blockers),
            corners: CornerRadii {
                top_left: radius,
                top_right: radius,
                bottom_right: radius,
                bottom_left: radius,
            },
        };
        let shadows = shadows_of(node, blockers);
        let blurs = blurs_of(node);
        // A circle with neither fill, stroke, shadow nor blur draws nothing —
        // the corners alone shape no ink. A backdrop blur counts as ink even
        // with no fill of its own: it changes the pixels beneath it, which is
        // the whole point of the effect.
        // An ellipse is a leaf, so it never clips.
        if entry.fill.is_none() && entry.stroke.is_none() && shadows.is_empty() && blurs.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(DocPaint {
            entry,
            shadows,
            blurs,
            clip: false,
            // An ellipse is a parametric (rounded-box) shape, not a baked one.
            shape_field: None,
        }))
    }

    /// Lowers a `VECTOR` node into a baked MSDF field carried on its paint
    /// entry as a coverage mask (story B1), or collects the reasons it cannot
    /// be lowered.
    ///
    /// The field-input selection rule widens by exactly the measured census:
    /// a filled node bakes its `fillGeometry`; a stroke-only node bakes
    /// Figma's already-expanded `strokeGeometry`; a same-colour fill+stroke
    /// unions both into one field; a differently-coloured fill+stroke, and a
    /// non-solid fill carrying a stroke, are each refused under their own name
    /// (v0.11 candidates, in neither live target); and a node
    /// with no fieldable geometry — or a path outside the `M`/`L`/`C`/`Z`
    /// census, or a degenerate extent — is refused by name (P4), preserving
    /// the node rather than approximating it. The winding rule (NONZERO /
    /// EVENODD) rides into the bake so holes fill correctly.
    fn vector_paint_of(
        &mut self,
        node: &Node,
        path: &str,
        blockers: &mut Vec<String>,
    ) -> Result<Option<DocPaint>, CompileError> {
        // The fill and stroke lower through the shared paint path; their own
        // refusals (a stacked fill, a non-solid or dashed stroke) become
        // blockers, and `UnresolvedImage` aborts.
        let fill = self.fill_of(node, path, blockers)?;
        let stroke = self.stroke_of(node, blockers);

        // Select the field-input geometry and the fill that paints it.
        let (geometry, paint_fill): (Vec<Geometry>, FillSpec) = match (fill, stroke) {
            // Filled: the fill's own geometry, painted by the fill.
            (Some(fill), None) => (node.fill_geometry.clone(), fill),
            // Stroke-only (a hairline arrow): Figma's expanded outline,
            // painted a synthesized solid of the stroke's colour.
            (None, Some(stroke)) => (
                node.stroke_geometry.clone(),
                FillSpec::Solid {
                    color: stroke.color,
                },
            ),
            // Both: only a same-colour fill+stroke (the hero's white/white
            // hairlines) unions cleanly into one field painted that colour.
            //
            // The two ways that can fail are named apart (debt #358). One
            // message used to cover both, and it described only the second:
            // a gradient- or image-filled vector with a stroke was refused as
            // "differently-coloured", which is not why it cannot lower — the
            // union is one field painted by one `FillSpec`, and a non-solid
            // fill has no colour to compare with the stroke's in the first
            // place. The refusal was correct either way; the cause was not.
            (Some(fill), Some(stroke)) => {
                let refusal = match &fill {
                    FillSpec::Solid { color } if *color == stroke.color => None,
                    FillSpec::Solid { .. } => {
                        Some("a vector with a differently-coloured fill and stroke")
                    }
                    _ => Some("a vector with a non-solid fill and a stroke"),
                };
                if let Some(what) = refusal {
                    blockers.push(what.to_string());
                    return Ok(None);
                }
                let mut geometry = node.fill_geometry.clone();
                geometry.extend(node.stroke_geometry.iter().cloned());
                (geometry, fill)
            }
            // Neither fill nor stroke: no ink, a layout-only leaf.
            (None, None) => return Ok(None),
        };

        if geometry.is_empty() {
            // Unfieldable: no path geometry at all (a geometry-free fetch, or
            // a genuinely degenerate node). Named, never silently dropped (P4).
            blockers.push("a vector with no path geometry".to_string());
            return Ok(None);
        }

        // Settle the winding rule (all contours must agree — a mixed-winding
        // node has no single-field bake) and concatenate the contours into one
        // multi-contour path.
        let winding = match uniform_winding(&geometry) {
            Ok(winding) => winding,
            Err(what) => {
                blockers.push(what);
                return Ok(None);
            }
        };
        let path_data = geometry
            .iter()
            .map(|g| g.path.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Bake into the shared atlas; identical geometry dedups by path hash.
        // A path outside the census or a degenerate extent is refused by name.
        let shape_field = match self.baker.add(&VectorPath {
            path: &path_data,
            winding,
        }) {
            Ok(index) => index,
            Err(error) => {
                blockers.push(vector_field_blocker(&error));
                return Ok(None);
            }
        };

        let entry = PaintEntry {
            fill: Some(paint_fill),
            // A baked vector keeps the single-fill restriction (`fill_of`
            // above); the stacked-fill vocabulary (story C1) widens
            // `paint_of` only — no stacked-fill vector is in the measured
            // need.
            extra_fills: Vec::new(),
            // The outline is baked into the field, not a parametric stroke.
            stroke: None,
            corners: CornerRadii::default(),
        };
        // A baked vector DOES carry its blur. The hero's frosted panel is
        // exactly this shape — a VECTOR with BACKGROUND_BLUR radius 100 —
        // and `docs/decisions/baked-vector-msdf-field.md` records that
        // lowering the hero's vectors is what unmasked it. Dropping it here
        // would silently lose the one node story #393 exists to fix.
        //
        // Shadows stay empty, and the node is refused whenever it has one
        // (the carrier guard in `visit`, debt #396). The asymmetry with the
        // blur above is the painter's, not a gap: `draw_backdrop_blur_field`
        // confines a blur to the baked coverage, but `draw_drop_shadow` and
        // `draw_inner_shadow` build their geometry from the node's
        // parametric box and corners. A baked vector's silhouette is its
        // field, so lowering a shadow here would cast a rectangle behind a
        // star — an approximation, where B1's rule is to skip and name
        // (`docs/decisions/baked-vector-msdf-field.md`). Refusing keeps the
        // gap visible until the painter can cast from a field.
        Ok(Some(DocPaint {
            entry,
            // Empty for the reason the long comment above gives: a baked
            // vector is refused whenever it carries a shadow.
            shadows: Vec::new(),
            blurs: blurs_of(node),
            clip: false,
            // The resolved `VectorField` is a runtime form; the `.dsb`
            // carries the shape index here instead.
            shape_field: Some(shape_field),
        }))
    }

    /// The single visible fill of a node whose paint entry carries exactly one
    /// (an ellipse, a baked vector), or the reasons it has none.
    ///
    /// Every visible fill is lowered whatever the count, so a stack names both
    /// its stacking refusal *and* each fill's own kind refusal (debt #329).
    /// Before that, the stacking refusal returned first and a two-fill node
    /// whose second fill was a `PATTERN` never mentioned the `PATTERN` at all.
    fn fill_of(
        &mut self,
        node: &Node,
        path: &str,
        blockers: &mut Vec<String>,
    ) -> Result<Option<FillSpec>, CompileError> {
        let kinds = self.fills_of(node, path, blockers)?;
        match single_visible_paint(&node.fills) {
            // A layout-only frame with no fill draws nothing.
            OnePaint::None => Ok(None),
            OnePaint::One(_) => Ok(kinds.into_iter().next()),
            // PaintEntry.fill is one Option<FillSpec>; Figma's fills is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4. `paint_of`
            // (the plain rectangle/frame path) lowers a stack instead, through
            // `fills_of`; an ellipse, a baked vector, and a text glyph color
            // keep refusing here — the measured need (the hero, the
            // stacked-fills fixture) is a plain frame/rectangle.
            OnePaint::Many => {
                blockers.push("more than one visible fill".to_string());
                Ok(None)
            }
        }
    }

    /// Lowers every visible fill of `node`, in Figma's `fills` array order —
    /// the same back-to-front convention as `effects`/`children`, so array
    /// order is paint order with no reversal (story C1, debt #146). Unlike
    /// `fill_of`, a stack of visible fills is not itself a blocker; a fill
    /// whose own kind has no lowering still refuses by name (P4), through
    /// `paint_kind`.
    ///
    /// Every visible fill is inspected, not only those before the first
    /// refusal (debt #329): two unlowerable fills in one stack are two
    /// findings, and the second is not implied by the first. A fill that
    /// refuses contributes a blocker and no `FillSpec`, so the returned list
    /// is shorter than the visible-fill count exactly when `blockers` grew.
    ///
    /// An unresolved `imageRef` (issue #484) is deferred rather than aborting
    /// the instant it is found: the array may carry another unlowerable fill
    /// on either side of it, and #329 already made every other fill-array
    /// finding order-independent by collecting the whole array before
    /// deciding. Aborting here at the first unresolved ref would reopen that
    /// same order-dependence one level further out — a real blocker hidden
    /// behind an earlier one, the exact defect #329 was filed to fix — so the
    /// verdict waits until every fill in the array is known. Once it is: a
    /// node with no other blocker still aborts (its image would actually be
    /// referenced once lowered, so the caller's contract failure stands,
    /// whatever the emit policy); a node already headed for the skip over
    /// another blocker gets the unresolved ref folded into that same skip,
    /// named alongside it, under `EmitPolicy::Partial` only — `Strict` aborts
    /// immediately, unchanged
    /// (`docs/decisions/dashc-identifies-images-never-decodes.md`).
    fn fills_of(
        &mut self,
        node: &Node,
        path: &str,
        blockers: &mut Vec<String>,
    ) -> Result<Vec<FillSpec>, CompileError> {
        let mut kinds = Vec::new();
        let mut pending_images: Vec<CompileError> = Vec::new();
        for fill in visible_paints(&node.fills) {
            match self.paint_kind(fill, path) {
                Ok(kind) => kinds.push(kind),
                Err(CompileError::Unsupported { what, .. }) => blockers.push(what),
                Err(err @ CompileError::UnresolvedImage { .. })
                    if self.policy == crate::EmitPolicy::Partial =>
                {
                    pending_images.push(err);
                }
                // Under `Strict`, an unresolved ref aborts immediately, exactly
                // as before. The image gate's own verdicts (`rule::IMAGE_*`)
                // abort immediately in both modes too — they are not in scope
                // of the deferral above (see the `rule` module doc).
                Err(other) => return Err(other),
            }
        }

        if !pending_images.is_empty() {
            if blockers.is_empty() {
                // Nothing else blocks this node, so it would actually
                // reference the image once lowered: the caller's contract
                // failure still aborts, the same verdict `Strict` reaches.
                return Err(pending_images
                    .into_iter()
                    .next()
                    .expect("checked non-empty"));
            }
            // The node is already headed for the skip over its other
            // blocker(s) — the ruling on #484 — so each unresolved image
            // joins them as a named reason instead of aborting the compile
            // over an image the skipped node will never fetch, decode, or
            // reference.
            for err in pending_images {
                let CompileError::UnresolvedImage { image_ref, .. } = err else {
                    unreachable!("only UnresolvedImage is ever pushed onto pending_images");
                };
                blockers.push(format!(
                    "an IMAGE fill with an unresolved imageRef {image_ref}"
                ));
            }
        }

        Ok(kinds)
    }

    fn paint_kind(&mut self, paint: &Paint, path: &str) -> Result<FillSpec, CompileError> {
        let unsupported = |what: &str| CompileError::Unsupported {
            path: path.to_string(),
            what: what.to_string(),
        };

        match paint.kind.as_str() {
            "SOLID" => {
                let color = paint
                    .color
                    .ok_or_else(|| unsupported("a SOLID with no color"))?;
                Ok(FillSpec::Solid {
                    color: color_of(color, paint.opacity),
                })
            }
            "GRADIENT_LINEAR" | "GRADIENT_RADIAL" | "GRADIENT_ANGULAR" | "GRADIENT_DIAMOND" => {
                let handles = &paint.gradient_handle_positions;
                let [origin, primary, secondary] = handles[..] else {
                    return Err(unsupported("a gradient without three handles"));
                };
                Ok(FillSpec::Gradient {
                    gradient: Gradient {
                        kind: match paint.kind.as_str() {
                            "GRADIENT_LINEAR" => GradientKind::Linear,
                            "GRADIENT_RADIAL" => GradientKind::Radial,
                            "GRADIENT_ANGULAR" => GradientKind::Angular,
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
                        // The table assigns the range on intern; this
                        // lowering has no table (story #578).
                        stops: StopRange::NONE,
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
                })
            }
            "IMAGE" => {
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

                        // The P4 gate story #400 exists for: the producer
                        // tagged `asset.format`, and nothing has verified it
                        // against the bytes themselves until now. The node's
                        // would-be index is the same advisory locator
                        // `unsupported`/`unsupported_at` use.
                        let node_index = self.doc.nodes.len() as u32;
                        let header =
                            dashpaint::image_id::identify(&asset.bytes).map_err(|error| {
                                let rule = match &error {
                                    dashpaint::image_id::ImageIdError::UnknownSignature => {
                                        rule::IMAGE_UNKNOWN_SIGNATURE
                                    }
                                    dashpaint::image_id::ImageIdError::Malformed { .. } => {
                                        rule::IMAGE_HEADER_MALFORMED
                                    }
                                };
                                image_diagnostic(
                                    rule,
                                    node_index,
                                    path,
                                    format!("imageRef {image_ref}: {error}"),
                                )
                            })?;
                        if header.format != asset.format {
                            return Err(image_diagnostic(
                                rule::IMAGE_FORMAT_MISMATCH,
                                node_index,
                                path,
                                format!(
                                    "imageRef {image_ref}: the bytes' own signature is \
                                     {:?}, which contradicts the producer's tag {:?}",
                                    header.format, asset.format
                                ),
                            ));
                        }
                        if header.width == 0 || header.height == 0 {
                            return Err(image_diagnostic(
                                rule::IMAGE_ZERO_DIMENSION,
                                node_index,
                                path,
                                format!(
                                    "imageRef {image_ref}: the header reports {}x{}",
                                    header.width, header.height
                                ),
                            ));
                        }

                        let index = self.doc.push_asset(Asset {
                            format: asset.format,
                            // An imported image fill: displayed picture data.
                            kind: AssetKind::Image,
                            bytes: asset.bytes.clone(),
                            width: header.width,
                            height: header.height,
                        });
                        self.image_of.insert(image_ref.to_string(), index);
                        index
                    }
                };

                Ok(FillSpec::Image(ImageFill {
                    image,
                    scale_mode: match paint
                        .scale_mode
                        .as_deref()
                        .ok_or_else(|| unsupported("an IMAGE fill with no scaleMode"))?
                    {
                        "FILL" => ScaleMode::Fill,
                        "FIT" => ScaleMode::Fit,
                        "CROP" => ScaleMode::Crop,
                        "TILE" => ScaleMode::Tile,
                        other => return Err(unsupported(&format!("an image scaleMode {other}"))),
                    },
                    // Both are `dashpaint` vocabulary already, so dropping
                    // them would not be an expressiveness gap — it would
                    // lower a cropped or tiled image to a *wrong* image, in
                    // silence (P4). Figma's imageTransform is row-major
                    // `[[a, b, tx], [c, d, ty]]`, the same six components
                    // `Mat23` holds; `Mat23::IDENTITY` is what an absent one
                    // means (story #578 removed `ImageFill::transform`'s
                    // `Option`).
                    transform: paint
                        .image_transform
                        .map(|[[a, b, tx], [c, d, ty]]| Mat23 { a, b, c, d, tx, ty })
                        .unwrap_or(Mat23::IDENTITY),
                    tile_scale: paint.scaling_factor.unwrap_or(1.0),
                }))
            }
            other => Err(unsupported(&format!("a {other} paint"))),
        }
    }

    /// The node's single visible stroke, or the reasons it has none.
    ///
    /// The gates below — stacking, stroke type, dash pattern, paint kind and
    /// alignment — are independent properties of the same stroke, so each
    /// collects its own blocker and none returns (debt #329). A dashed
    /// gradient stroke used to name only whichever gate the source listed
    /// first.
    fn stroke_of(&self, node: &Node, blockers: &mut Vec<String>) -> Option<Stroke> {
        // strokeWeight and strokeAlign are present even when `strokes` is
        // empty (pinned by the fixture), so the stroke is gated on the array,
        // never on the weight. A node with no stroke has no stroke findings —
        // the gates below would read fields Figma leaves set from an earlier
        // edit.
        let stroke = match single_visible_paint(&node.strokes) {
            OnePaint::None => return None,
            // PaintEntry.stroke is one Option<Stroke>; Figma's strokes is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4. Same rule
            // as `fill_of`.
            OnePaint::Many => {
                blockers.push("more than one visible stroke".to_string());
                None
            }
            OnePaint::One(stroke) => Some(stroke),
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
            blockers.push(format!("a {stroke_type} stroke"));
        }
        // Figma writes `strokeDashes: null` for a continuous stroke, and an
        // empty array means the same, so only a non-empty pattern is a drop.
        if node.stroke_dashes.as_ref().is_some_and(|d| !d.is_empty()) {
            blockers.push("a dashed stroke".to_string());
        }

        let color = stroke.and_then(|stroke| match stroke.kind.as_str() {
            "SOLID" => match stroke.color {
                Some(color) => Some(color_of(color, stroke.opacity)),
                None => {
                    blockers.push("a SOLID stroke with no color".to_string());
                    None
                }
            },
            // v0.3 strokes are solid-only (dashpaint::Stroke).
            _ => {
                blockers.push("a non-solid stroke".to_string());
                None
            }
        });

        let align = match node.stroke_align.as_deref().unwrap_or("INSIDE") {
            "INSIDE" => Some(StrokeAlign::Inside),
            "CENTER" => Some(StrokeAlign::Center),
            "OUTSIDE" => Some(StrokeAlign::Outside),
            other => {
                blockers.push(format!("a {other} stroke alignment"));
                None
            }
        };

        match (color, align) {
            (Some(color), Some(align)) => Some(Stroke {
                width: node.stroke_weight.unwrap_or(1.0),
                align,
                color,
            }),
            // Some gate above refused, so the caller skips the node; the
            // half-built stroke it would carry is never emitted.
            _ => None,
        }
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
        // Horizontal alignment (story #310): `LEFT` is the default — the runtime
        // flushes an LTR paragraph left and an RTL one right by direction
        // (`docs/design/typeset-latin.md`). `CENTER`/`RIGHT` lower onto the
        // style; `JUSTIFIED` has no vocabulary and stays a named diagnostic.
        let text_align = match style.text_align_horizontal.as_deref() {
            None | Some("LEFT") => DocTextAlign::Left,
            Some("CENTER") => DocTextAlign::Center,
            Some("RIGHT") => DocTextAlign::Right,
            Some(other) => {
                blockers.push(format!("text alignment {other}"));
                DocTextAlign::Left
            }
        };
        // Vertical alignment within the box (story #310): `TOP` is the default
        // the runtime places from; `CENTER`/`BOTTOM` lower onto the style.
        let text_align_v = match style.text_align_vertical.as_deref() {
            None | Some("TOP") => DocTextAlignV::Top,
            Some("CENTER") => DocTextAlignV::Center,
            Some("BOTTOM") => DocTextAlignV::Bottom,
            Some(other) => {
                blockers.push(format!("vertical text alignment {other}"));
                DocTextAlignV::Top
            }
        };
        // Line height (story #310): `INTRINSIC_%` is Figma's "Auto" — the
        // font's natural line advance, which the runtime uses (lowered as
        // `None`). Only a `PIXELS` line height lowers into a fixed value; a
        // percentage line height (`FONT_SIZE_%`, `PERCENT`) has no vocabulary
        // and stays a named diagnostic.
        let line_height_px = match style.line_height_unit.as_deref() {
            None | Some("INTRINSIC_%") => None,
            Some("PIXELS") => style.line_height_px,
            Some(other) => {
                blockers.push(format!("a {other} line height"));
                None
            }
        };
        // Letter spacing (story #310): lowers verbatim; absent is zero.
        let letter_spacing = style.letter_spacing.unwrap_or(0.0);
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
        // rather than fix-sized silently. An absent field arrives here as
        // `NONE`, normalized at the parse boundary (debt #339).
        match style.text_auto_resize.as_str() {
            "WIDTH_AND_HEIGHT" | "HEIGHT" | "NONE" => {}
            "TRUNCATE" => blockers.push("text truncation".to_string()),
            other => blockers.push(format!("text auto-resize {other}")),
        }
        // A hyperlink on the run.
        if style.hyperlink.is_some() {
            blockers.push("a text hyperlink".to_string());
        }
        // OpenType feature flags (story #341): the vocabulary lowers exactly
        // the measured flag, standard ligatures off (`{LIGA: 0}`), into
        // `ligatures_off`. That is the narrowing this story earned by
        // measurement, not a guess at what else might show up — any other
        // flag, any other value on `LIGA`, or `LIGA` alongside another flag
        // has no vocabulary and stays the named diagnostic (P4).
        let ligatures_off = match (style.opentype_flags.len(), style.opentype_flags.get("LIGA")) {
            (0, _) => false,
            (1, Some(v)) if v.as_i64() == Some(0) => true,
            _ => {
                blockers.push("OpenType features".to_string());
                false
            }
        };

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
                line_height_px,
                letter_spacing,
                text_align,
                text_align_v,
                ligatures_off,
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
        match fill.kind.as_str() {
            "SOLID" => {
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
    let mut visible = visible_paints(paints);
    match (visible.next(), visible.next()) {
        (None, _) => OnePaint::None,
        (Some(paint), None) => OnePaint::One(paint),
        (Some(_), Some(_)) => OnePaint::Many,
    }
}

/// Every paint in `paints` Figma does not mark `visible: false` — the shared
/// filter `single_visible_paint` (one-or-refuse) and `fills_of` (the ordered
/// stack, story C1) both take.
fn visible_paints(paints: &[Paint]) -> impl Iterator<Item = &Paint> {
    paints.iter().filter(|p| p.visible != Some(false))
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
/// `textAutoResize` is the sizing source. `WIDTH_AND_HEIGHT` hugs both axes;
/// `HEIGHT` fixes the width and grows the height; `NONE` fixes the box — and
/// so does an **absent** field, because `NONE` is the REST default and the API
/// omits it: a fixed-box label serializes with no `textAutoResize` at all,
/// while an auto-sizing node always carries the field explicitly (every
/// committed capture does). Mapping absent to auto instead mis-lowered a fixed
/// free-standing label as hug-both — its box collapsed to its content, so the
/// text axes had no room to place the block — caught by the #332 import
/// oracle's `import-text-axes` frame. For a free-standing node the
/// `absoluteBoundingBox` is authored (the designer placed and sized it — it is
/// not an auto-layout solver result, so P1 permits a Fixed axis to read it).
/// `TRUNCATE` and any unknown value are refused in [`Walk::text_of`], so the
/// `NONE`-equivalent fixed fallback here is never emitted for them.
fn text_sizing(
    node: &Node,
    from_layout_sizing: Option<LayoutConstraints>,
) -> Option<LayoutConstraints> {
    if node.layout_sizing_horizontal.is_some() || node.layout_sizing_vertical.is_some() {
        return from_layout_sizing;
    }
    let (sizing_h, sizing_v) = match node.style.as_ref().map(|s| s.text_auto_resize.as_str()) {
        Some("WIDTH_AND_HEIGHT") => (AxisSizing::Hug, AxisSizing::Hug),
        Some("HEIGHT") => (AxisSizing::Fixed, AxisSizing::Hug),
        // `NONE` — which an absent field parses as (debt #339) — and a node
        // with no style at all: the box is fixed as authored.
        _ => (AxisSizing::Fixed, AxisSizing::Fixed),
    };
    let constraints = LayoutConstraints {
        sizing_h,
        sizing_v,
        ..from_layout_sizing.unwrap_or_default()
    };
    (constraints != LayoutConstraints::default()).then_some(constraints)
}

/// The single winding rule a `VECTOR` node's contours share (story B1), or a
/// blocker message when they disagree or carry an unknown rule. fdsm applies
/// one fill rule to the whole shape, so a node whose contours mix NONZERO and
/// EVENODD has no single-field bake and is refused rather than baked wrong
/// (P4). The caller guarantees `geometry` is non-empty.
fn uniform_winding(geometry: &[Geometry]) -> Result<WindingRule, String> {
    let mut winding: Option<WindingRule> = None;
    for contour in geometry {
        let rule = match contour.winding_rule.as_deref() {
            // Figma always emits the rule alongside path geometry; absent
            // defaults to NONZERO.
            None | Some("NONZERO") => WindingRule::NonZero,
            Some("EVENODD") => WindingRule::EvenOdd,
            Some(other) => return Err(format!("a vector with winding rule {other}")),
        };
        match winding {
            None => winding = Some(rule),
            Some(prev) if prev != rule => {
                return Err("a vector with mixed per-contour winding rules".to_string());
            }
            Some(_) => {}
        }
    }
    Ok(winding.expect("the caller guarantees non-empty geometry"))
}

/// The named-refusal message for a bake error (story B1) — the generator-side
/// cause of a `figma.unsupported` verdict, preserving the node rather than
/// approximating it (P4).
fn vector_field_blocker(error: &VectorFieldError) -> String {
    match error {
        VectorFieldError::UnsupportedCommand(command) => {
            format!("a vector path command {command:?} (census is M/L/C/Z)")
        }
        VectorFieldError::MalformedPath(detail) => format!("a malformed vector path ({detail})"),
        VectorFieldError::DegenerateGeometry => {
            "a degenerate vector (no fillable extent)".to_string()
        }
    }
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
/// no color — as a collected blocker, so it masks neither the node's other
/// shadows nor its fill and stroke (debt #329).
///
/// Figma's `showShadowBehindNode` is not modeled: the REST subset is
/// deliberately partial, and this painter always draws a drop shadow behind
/// the node (the Figma default). That is a documented fidelity limitation, not
/// a dropped field the schema could carry.
fn shadows_of(node: &Node, blockers: &mut Vec<String>) -> Vec<Shadow> {
    let mut shadows = Vec::new();
    for effect in &node.effects {
        let Some(kind) = visible_shadow_kind(effect) else {
            continue;
        };
        let Some(color) = effect.color else {
            blockers.push("a shadow with no color".to_string());
            continue;
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
    shadows
}

/// The [`ShadowKind`] a visible `DROP_SHADOW`/`INNER_SHADOW` effect lowers to,
/// or `None` for anything else.
///
/// One function answers both questions that must agree: what [`shadows_of`]
/// lowers, and what the `visit` carrier guard requires a lowering for (debt
/// #396). Returning the kind rather than a boolean is what makes them agree by
/// construction — the guard cannot come to recognize a shadow the lowering
/// does not, because there is one list and the lowering reads its result. A
/// hidden effect is not one, exactly as it is not for the lowering: it casts
/// nothing in Figma.
fn visible_shadow_kind(effect: &Effect) -> Option<ShadowKind> {
    if effect.visible == Some(false) {
        return None;
    }
    match effect.kind.as_str() {
        "DROP_SHADOW" => Some(ShadowKind::Drop),
        "INNER_SHADOW" => Some(ShadowKind::Inner),
        _ => None,
    }
}

/// A node's blurs (story #393,
/// `docs/decisions/backdrop-blur-is-core-vocabulary.md`), mirroring
/// [`shadows_of`]: the same visible filter, the same skip-what-we-do-not-lower
/// rule, in Figma's own effect order.
///
/// Only `BACKGROUND_BLUR` lowers. `LAYER_BLUR` is deliberately not handled
/// here — it stays budgeted at v1 and its triage still raises a diagnostic, so
/// falling through leaves that gap named rather than silently emitting a blur
/// the runtime would treat as a backdrop one.
///
/// An absent `radius` lowers to 0.0, matching `shadows_of`'s treatment of an
/// absent blur radius: a zero-radius blur is a no-op the painter can skip, not
/// a malformed document.
fn blurs_of(node: &Node) -> Vec<Blur> {
    node.effects
        .iter()
        .filter(|e| e.visible != Some(false))
        .filter_map(|effect| match effect.kind.as_str() {
            "BACKGROUND_BLUR" => Some(Blur {
                kind: BlurKind::Backdrop,
                radius: effect.radius.unwrap_or(0.0),
            }),
            _ => None,
        })
        .collect()
}

/// How far a rotated box's axis-aligned bounds sit from the box's own
/// origin: the minimum x and y over the four corners of `w` × `h` turned by
/// `rotation` radians about that origin (story #770).
///
/// Subtracting this from `absoluteBoundingBox`'s top-left recovers the
/// node's own top-left, which is the datum Figma rotates about and the one
/// the document wants. Both components are `0.0` at a zero rotation, which
/// is what makes an unrotated node lower exactly as it did before this
/// vocabulary.
///
/// The convention is this repository's: y-down and clockwise-positive, so a
/// point turns as `(x cos − y sin, x sin + y cos)`. Figma's `rotation` feeds
/// in unconverted (`rest::Node::rotation`).
///
/// Worked against `corpus/figma-fixtures/node-fx.json`'s `rotated-15deg`, a
/// 100 × 100 RECTANGLE at `rotation: -0.26179940325453416` whose
/// `absoluteBoundingBox` is `(30, 4.118092656135559)`: the offset is
/// `(0, -25.881905)`, so the node's own origin is `(30, 30)` — the `tx`/`ty`
/// of its own `relativeTransform`, to every digit Figma reports.
fn rotated_bounds_offset(rotation: f32, w: f32, h: f32) -> (f32, f32) {
    if rotation == 0.0 {
        return (0.0, 0.0);
    }
    let (sin, cos) = rotation.sin_cos();
    // The origin corner maps to itself, so `0.0` seeds both minima and only
    // the other three corners are candidates.
    let xs = [0.0, w * cos, -h * sin, w * cos - h * sin];
    let ys = [0.0, w * sin, h * cos, w * sin + h * cos];
    (
        xs.iter().copied().fold(f32::INFINITY, f32::min),
        ys.iter().copied().fold(f32::INFINITY, f32::min),
    )
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
