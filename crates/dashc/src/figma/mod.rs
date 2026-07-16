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

pub mod rest;
pub(crate) mod triage;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde::Deserialize;

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, ImageAsset, Mat23, PaintEntry,
    PaintKind, ScaleMode, Stroke, StrokeAlign, Vec2,
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
    AxisSizing, Box2D, CrossAxisAlign, Document, EdgeInsets, LayoutConstraints,
    LayoutContainer as DocContainer, LayoutMode, MainAxisAlign, Node as DocNode, Paint as DocPaint,
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
    /// root `FRAME` under its first `CANVAS`.
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
    let root = root_frame(&file.document)?;

    let mut walk = Walk {
        doc: Document::new(),
        diagnostics: Vec::new(),
        image_of: BTreeMap::new(),
        profile,
        images,
    };

    // Iterative depth-first walk (debt #148: a recursive walk would turn
    // the parse-depth headroom above into a stack overflow). Children are
    // pushed in reverse, so popping yields document (DFS preorder) order —
    // which is both the rect-table order and the diagnostics order.
    //
    // The root has no parent origin: it is relative to itself, so it drops
    // its page position and lowers to (0, 0, w, h).
    let mut stack = vec![Visit {
        node: root,
        parent: None,
        parent_origin: None,
        path: format!("/{}", root.name),
        flow: None,
    }];
    while let Some(visit) = stack.pop() {
        walk.visit(visit, &mut stack)?;
    }

    Ok((walk.doc, walk.diagnostics))
}

/// The `imageRef`s the lowering will demand, sorted and deduplicated.
///
/// The Deno importer cannot fetch what it cannot name, and Figma's `GET /file`
/// carries no image bytes — only refs. Rather than have the importer walk the
/// JSON looking for them (a second copy of "where an imageRef lives", free to
/// drift from the walk that actually consumes them), it asks here. The scan
/// covers the same subtree [`lower`] walks, and both fills and strokes, so a
/// ref this returns is a ref the lowering can resolve.
///
/// Deliberately a superset: a paint this returns may still be refused by the
/// lowering (a stacked fill, an invisible one). Fetching an image that turns
/// out to be unused costs one download; missing one is a failed compile.
pub fn image_refs(file: &FigmaFile) -> Result<Vec<String>, CompileError> {
    let mut found = BTreeSet::new();
    // Iterative for the same reason the lowering walk is (debt #148).
    let mut stack = vec![root_frame(&file.document)?];
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

/// The first `FRAME` under the first `CANVAS`.
///
/// v0.3 exports one root frame. Declared roots plus a reachability closure
/// (docs/design/dashc.md) is the v0.7 story; until then the rule is positional and
/// stated rather than inferred — every other sibling and every later canvas
/// is silently dropped (debt #147).
fn root_frame(document: &Node) -> Result<&Node, CompileError> {
    document
        .children
        .iter()
        .find(|n| n.kind == "CANVAS")
        .and_then(|canvas| canvas.children.iter().find(|n| n.kind == "FRAME"))
        .ok_or_else(|| CompileError::Unsupported {
            path: "/".to_string(),
            what: "a document with no root FRAME under its first CANVAS".to_string(),
        })
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
    profile: Profile,
    images: &'a BTreeMap<String, ImageAsset>,
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

        // A non-FRAME node reports its type and nothing else: its other
        // properties belong to whatever story lowers that type (TEXT is
        // #160), so diagnosing them here would be noise around the verdict
        // that matters.
        if node.kind != "FRAME" {
            self.unsupported(path, format!("node type {}", node.kind));
            return Ok(());
        }

        // What blocks this node, all findings collected before the verdict.
        let mut blockers: Vec<String> = Vec::new();

        // Document has no field for a hidden node, and no way to represent one
        // without shifting the DFS indices every later node depends on — so
        // it is diagnosed rather than lowered as though it were visible
        // (P4). Hidden layers are routine in real Figma files (debt #143).
        if node.visible == Some(false) {
            blockers.push("a hidden node".to_string());
        }
        // Document carries no opacity, rotation, or mask vocabulary — no
        // Construct fits any of them (debt #143), so each is diagnosed
        // rather than lowered as though it were opaque, axis-aligned, or an
        // ordinary frame (P4). Figma omits `rotation` entirely when it is
        // zero, so `None` and `Some(0.0)` both mean unrotated.
        if node.opacity.is_some_and(|o| o < 1.0) {
            blockers.push("node opacity".to_string());
        }
        if node.rotation.is_some_and(|r| r != 0.0) {
            blockers.push("node rotation".to_string());
        }
        if node.is_mask == Some(true) {
            blockers.push("a mask node".to_string());
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

        let container = container_of(node, &mut blockers);
        let constraints = constraints_of(node, visit.flow.as_ref(), &mut blockers);

        if node.absolute_bounding_box.is_none() {
            blockers.push(format!("node {} has no absoluteBoundingBox", node.name));
        }

        // The paint lowering keeps its own refusals; an unsupported paint
        // blocks the node like any other finding. `UnresolvedImage` aborts:
        // it is the caller's contract, not the designer's file.
        let paint = match self.paint_of(node, path) {
            Ok(paint) => paint,
            Err(CompileError::Unsupported { what, .. }) => {
                blockers.push(what);
                None
            }
            Err(other) => return Err(other),
        };

        // The import gate: the producer maps, the validator decides (P5).
        // Unmapped effects (a baked shadow, debt #144) have no Construct and
        // block the node instead.
        let (constructs, effect_blockers) = triage::constructs_of(node);
        blockers.extend(effect_blockers);

        if !blockers.is_empty() {
            // The index this node would have taken. No document survives an
            // error (R6), so the index is advisory — the path is the stable
            // half — and two skipped siblings may share one.
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
            } else if c.gap < 0.0 {
                child_leading_margin = c.gap;
                c.gap = 0.0;
            }
            c
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
        });

        for construct in constructs {
            self.diagnostics.push(dashscene_validator::triage(
                construct,
                self.profile,
                NodePath::new(index, path.clone()),
            ));
        }

        // Figma permits duplicate sibling names, and a path built from names
        // alone would give two siblings one diagnostic path (debt #150). A
        // duplicated name is suffixed with the node's Figma id — the stable,
        // URL-pastable one every capture carries — or with its child
        // position when a synthetic node has no id.
        let mut name_counts: HashMap<&str, u32> = HashMap::new();
        for child in &node.children {
            *name_counts.entry(child.name.as_str()).or_default() += 1;
        }

        let flow = container.map(|c| Flow {
            horizontal: c.mode == LayoutMode::Horizontal,
            leading_margin: child_leading_margin,
            parent_hugs_h: sizing.sizing_h == AxisSizing::Hug,
            parent_hugs_v: sizing.sizing_v == AxisSizing::Hug,
        });

        // Reversed, so the LIFO stack pops them in document order.
        for (position, child) in node.children.iter().enumerate().rev() {
            let segment = if name_counts[child.name.as_str()] > 1 {
                match &child.id {
                    Some(id) => format!("{} ({id})", child.name),
                    None => format!("{} (#{position})", child.name),
                }
            } else {
                child.name.clone()
            };
            let flow = flow.map(|f| Flow {
                leading_margin: if position == 0 { 0.0 } else { f.leading_margin },
                ..f
            });
            stack.push(Visit {
                node: child,
                parent: Some(index),
                parent_origin: Some((bbox.x, bbox.y)),
                path: format!("{path}/{segment}"),
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
        self.diagnostics.push(Diagnostic {
            rule: rule::UNSUPPORTED,
            severity: Severity::Error,
            at: Location::Node(NodePath::new(index, path)),
            message: format!("{what} is not in the document vocabulary yet"),
        });
    }

    fn paint_of(&mut self, node: &Node, path: &str) -> Result<Option<DocPaint>, CompileError> {
        let entry = PaintEntry {
            fill: self.fill_of(node, path)?,
            stroke: self.stroke_of(node, path)?,
            corners: corners_of(node),
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

    fn fill_of(&mut self, node: &Node, path: &str) -> Result<Option<PaintKind>, CompileError> {
        let mut visible = node.fills.iter().filter(|p| p.visible != Some(false));
        let Some(fill) = visible.next() else {
            return Ok(None);
        };
        if visible.next().is_some() {
            // PaintEntry.fill is one Option<PaintKind>; Figma's fills is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4.
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "more than one visible fill".to_string(),
            });
        }
        self.paint_kind(fill, path).map(Some)
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
        let mut visible = node.strokes.iter().filter(|p| p.visible != Some(false));
        let Some(stroke) = visible.next() else {
            return Ok(None);
        };
        if visible.next().is_some() {
            // PaintEntry.stroke is one Option<Stroke>; Figma's strokes is an
            // array. Stacking is a Document expressiveness gap (debt #146), not
            // a triage gap — and a silent drop would violate P4. Same rule
            // as `fill_of`.
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "more than one visible stroke".to_string(),
            });
        }

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
}

/// The container-side flex intent of `node`, or `None` for a passthrough
/// (`layoutMode` absent or `NONE`).
///
/// The authored intent lowers — mode, gap, padding, alignment — and never
/// the solved boxes (P1, the surviving ground of
/// `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`). What the
/// runtime cannot solve yet is a blocker, not an approximation: `GRID` and
/// wrap land at v0.8 with the schema's enum appends, `BASELINE` with them
/// (Q-4).
fn container_of(node: &Node, blockers: &mut Vec<String>) -> Option<DocContainer> {
    let mode = match node.layout_mode.as_deref() {
        None | Some("NONE") => return None,
        Some("HORIZONTAL") => LayoutMode::Horizontal,
        Some("VERTICAL") => LayoutMode::Vertical,
        Some("GRID") => {
            blockers.push("grid auto-layout (GRID)".to_string());
            return None;
        }
        Some(other) => {
            blockers.push(format!("auto-layout ({other})"));
            return None;
        }
    };

    match node.layout_wrap.as_deref() {
        None | Some("NO_WRAP") => {}
        Some(wrap) => blockers.push(format!("wrapping auto-layout ({wrap})")),
    }

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
        // Baseline appends to the schema's CrossAxisAlign at v0.8 (Q-4);
        // aligning a baseline row to the cross start instead would move
        // every child (pinned by lowering-baseline.json).
        Some(other) => {
            blockers.push(format!("cross-axis alignment {other}"));
            CrossAxisAlign::Start
        }
    };

    Some(DocContainer {
        mode,
        gap: node.item_spacing.unwrap_or(0.0),
        padding: EdgeInsets {
            left: node.padding_left.unwrap_or(0.0),
            top: node.padding_top.unwrap_or(0.0),
            right: node.padding_right.unwrap_or(0.0),
            bottom: node.padding_bottom.unwrap_or(0.0),
        },
        main_align,
        cross_align,
    })
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
