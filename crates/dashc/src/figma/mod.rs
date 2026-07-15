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

pub mod rest;
pub(crate) mod triage;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, ImageAsset, Mat23, PaintEntry,
    PaintKind, ScaleMode, Stroke, StrokeAlign, Vec2,
};
use dashscene_validator::{Diagnostic, NodePath, Profile, Report};

// `Node` and `Paint` collide with `rest`'s Figma-vocabulary types of the same
// name (imported below, unaliased, since they are what the rest of this
// module's signatures use); the document's types are aliased here instead.
// The rule: each file leaves its own subject bare and aliases the visitor.
// Here the Figma REST types are the subject, so the IR types are aliased;
// in `emit.rs` the IR is the subject, so the flatbuffer types are aliased
// (its `Node` stays bare and the flatbuffer's is `FbNode`).
use crate::document::{Box2D, Document, Node as DocNode, Paint as DocPaint};
use crate::figma::rest::{FigmaFile, Node, Paint, PaintTag};

/// Why a Figma file could not be compiled at all.
///
/// Distinct from a `Diagnostic`, which is a verdict *about* a document that
/// was understood. These are the cases where lowering cannot proceed.
#[derive(Debug)]
pub enum CompileError {
    /// The input was not the Figma REST JSON it claimed to be.
    Parse(serde_json::Error),
    /// A construct the v0.3 `Document` cannot express. It has no `Construct`
    /// variant, so it cannot be a diagnostic — and P4 forbids dropping it in
    /// silence, so it stops the compile instead. The named gaps this covers
    /// are tracked as debt: stacked fills/strokes (#146), node opacity/
    /// rotation/mask/hidden (#143), baked shadows (#144), auto-layout frames
    /// (#140), dashed and variable-width strokes (#145), and root-selection
    /// dropping canvas siblings (#147).
    Unsupported { path: String, what: String },
    /// An image fill whose `imageRef` the caller did not resolve. The load
    /// gate rejects a zero-byte asset, so no placeholder can be invented.
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
                write!(f, "{path}: {what} is not in the v0.3 vocabulary")
            }
            Self::UnresolvedImage { path, image_ref } => {
                write!(f, "{path}: no image supplied for imageRef {image_ref}")
            }
            Self::Diagnostics(report) => write!(f, "{report}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Lowers a parsed Figma file into a `Document` plus the diagnostics its
/// out-of-profile constructs earned.
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
    // The root has no parent origin: it is relative to itself, so it drops its
    // page position and lowers to (0, 0, w, h).
    walk.visit(root, None, None, "")?;

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
    fn walk(node: &Node, found: &mut BTreeSet<String>) {
        for paint in node.fills.iter().chain(node.strokes.iter()) {
            if paint.kind == PaintTag::Image
                && let Some(image_ref) = &paint.image_ref
            {
                found.insert(image_ref.clone());
            }
        }
        for child in &node.children {
            walk(child, found);
        }
    }

    let mut found = BTreeSet::new();
    walk(root_frame(&file.document)?, &mut found);
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

fn box_of(node: &Node, path: &str) -> Result<rest::Rect, CompileError> {
    node.absolute_bounding_box
        .ok_or_else(|| CompileError::Unsupported {
            path: path.to_string(),
            what: format!("node {} has no absoluteBoundingBox", node.name),
        })
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
    /// Depth-first, parent before child: `Document::push` order is the
    /// rect-table index, and `emit` does not reorder.
    ///
    /// `parent_origin` is the parent's *absolute* origin — what turns Figma's
    /// page-absolute box into the parent-relative intent `Document` wants.
    /// `None` at the root, which has no parent and so is relative to itself.
    fn visit(
        &mut self,
        node: &Node,
        parent: Option<u32>,
        parent_origin: Option<(f32, f32)>,
        prefix: &str,
    ) -> Result<(), CompileError> {
        let path = format!("{prefix}/{}", node.name);

        if node.kind != "FRAME" {
            return Err(CompileError::Unsupported {
                path,
                what: format!("node type {}", node.kind),
            });
        }
        // Document has no field for a hidden node, and no way to represent one
        // without shifting the DFS indices every later node depends on — so
        // it fails loudly rather than lowering as though it were visible
        // (P4). Hidden layers are routine in real Figma files (debt #143).
        if node.visible == Some(false) {
            return Err(CompileError::Unsupported {
                path,
                what: "a hidden node".to_string(),
            });
        }
        // Document carries no opacity vocabulary — no Construct fits, so opacity
        // fails loudly rather than lowering as though it were opaque (P4,
        // debt #143).
        if node.opacity.is_some_and(|o| o < 1.0) {
            return Err(CompileError::Unsupported {
                path,
                what: "node opacity".to_string(),
            });
        }
        // Document carries no rotation vocabulary — no Construct fits, so a
        // rotated node fails loudly rather than lowering as though it were
        // axis-aligned (P4, debt #143). Figma omits `rotation` entirely when
        // it is zero, so `None` and `Some(0.0)` both mean unrotated.
        if node.rotation.is_some_and(|r| r != 0.0) {
            return Err(CompileError::Unsupported {
                path,
                what: "node rotation".to_string(),
            });
        }
        // Document carries no mask vocabulary — no Construct fits, so a mask node
        // fails loudly rather than being painted as an ordinary frame (P4,
        // debt #143).
        if node.is_mask == Some(true) {
            return Err(CompileError::Unsupported {
                path,
                what: "a mask node".to_string(),
            });
        }
        // An auto-layout frame is refused for two reasons, and each one holds
        // on its own.
        //
        // Document has no flex vocabulary — no mode, no gap, no padding, no sizing
        // — so there is no field to lower the intent into and no Construct to
        // triage it onto. Dropping it would be the silent drop P4 forbids
        // (debt #140).
        //
        // And the boxes are not intent. Inside an auto-layout frame, Figma's
        // flex solver is what *computed* every child's absoluteBoundingBox, so
        // lowering those boxes as fixed rects would bake a layout result into
        // a document that carries only intent (P1) — and the result would look
        // right until the first resize.
        //
        // Figma writes `NONE` on a frame with auto-layout off, and omits the
        // field on a node that cannot have one.
        if let Some(mode) = node.layout_mode.as_deref()
            && mode != "NONE"
        {
            return Err(CompileError::Unsupported {
                path,
                what: format!("auto-layout ({mode})"),
            });
        }

        let bbox = box_of(node, &path)?;
        // Where a frame sits on the Figma page is a page-layout artifact, not
        // intent (P1). The root has no parent to be relative to, so it is
        // relative to itself and lowers to (0, 0, w, h).
        let origin = parent_origin.unwrap_or((bbox.x, bbox.y));
        // Built before the push: `paint_of` borrows `self` mutably (it interns
        // image assets), so it cannot run inside the `push` argument.
        let paint = self.paint_of(node, &path)?;
        let index = self.doc.push(DocNode {
            name: Some(node.name.clone()),
            parent,
            box2d: Box2D {
                x: bbox.x - origin.0,
                y: bbox.y - origin.1,
                width: bbox.width,
                height: bbox.height,
            },
            paint,
        });

        // The import gate: the producer maps, the validator decides (P5).
        let constructs = triage::constructs_of(node).map_err(|what| CompileError::Unsupported {
            path: path.clone(),
            what,
        })?;
        for construct in constructs {
            self.diagnostics.push(dashscene_validator::triage(
                construct,
                self.profile,
                NodePath::new(index, path.clone()),
            ));
        }

        for child in &node.children {
            self.visit(child, Some(index), Some((bbox.x, bbox.y)), &path)?;
        }
        Ok(())
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
        // so it fails loudly rather than repainting as a plain solid stroke of
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
