//! The in-memory dashscene document — what a producer lowers *into*, and
//! what the emitter writes *out of*.
//!
//! It is deliberately not a second vocabulary. The paint types are
//! `dashpaint`'s (boundary B), so the one paint vocabulary spans the
//! document, the runtime, and the painter, and a lowering cannot invent a
//! construct no painter can draw.
//!
//! What it adds over `dashpaint` is the *document's* shape: a flattened DFS
//! node list whose array index is the rect-table index (docs/design/dashbuf.md), layout
//! intent (never results — P1), and the pools nodes reference by index.

use dashpaint::{ImageAsset, PaintEntry};

/// A node's authored box. Intent, not a result (P1): under a flex parent the
/// solver owns placement and these offsets are ignored, and the width/height
/// are the datum only an axis sized [`AxisSizing::Fixed`] reads.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Box2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A flex container's direction. Mode `None` — the schema's
/// `LayoutMode::None` — is spelled as `Node::container: None`: the absence
/// the schema encodes as an absent table, `Option` encodes as `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// `Baseline` appends at v0.8 (Q-4); until then a producer refuses it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
}

/// How a node sizes itself along one axis. `Fixed` reads the [`Box2D`]
/// width/height as its datum; `Hug` wraps content; `Fill` stretches into
/// the parent's free space.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AxisSizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}

/// Insets named per edge, mirroring the schema's `EdgeInsets` struct.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Container-side flex intent — the schema's `LayoutContainer` table.
/// Present only on a node that lays its children out (mode H/V).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutContainer {
    pub mode: LayoutMode,
    /// Never negative in an emitted document: a negative authored gap is
    /// lowered to child margins before it gets here
    /// (`docs/decisions/negative-gap-lowering.md`).
    pub gap: f32,
    pub padding: EdgeInsets,
    pub main_align: MainAxisAlign,
    pub cross_align: CrossAxisAlign,
}

/// Child-side flex intent — the schema's `LayoutConstraints` table. `None`
/// on [`Node`] means fully default: `Fixed` sizing, unconstrained min/max,
/// zero margin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayoutConstraints {
    pub sizing_h: AxisSizing,
    pub sizing_v: AxisSizing,
    /// `None` = unconstrained — absence of intent, not a sentinel (P1).
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    /// Outer margin in the parent's flex flow. Negative values express
    /// overlap — the negative-gap lowering's target.
    pub margin: EdgeInsets,
}

/// One node of the document. `parent` is an index into [`Document::nodes`], and
/// the array is in DFS order, so a parent's index is always lower than its
/// children's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: Option<String>,
    pub parent: Option<u32>,
    pub box2d: Box2D,
    /// The node's style. `None` draws nothing (a layout-only container).
    pub paint: Option<Paint>,
    /// Container-side flex intent. `None` = mode `None` (a passthrough:
    /// children place by their authored offsets).
    pub container: Option<LayoutContainer>,
    /// Child-side flex intent. `None` = fully default constraints.
    pub constraints: Option<LayoutConstraints>,
}

/// A node's style: the boundary-B paint entry, plus the clip intent the
/// document pools alongside it.
///
/// Clip travels with the paint entry because the schema pools it there
/// (`Paint.clip`). The arena instead carries clip as *node* intent
/// (`Prop::Clip`, issue #97) — so two nodes sharing a style but differing in
/// clip need two pool entries here, which is why the pool key below includes
/// it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paint {
    pub entry: PaintEntry,
    pub clip: bool,
}

/// One dashscene document, ready to emit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    /// Flattened DFS node tree: array index = rect-table index (docs/design/dashbuf.md).
    pub nodes: Vec<Node>,
    /// The image assets an image fill references by index.
    pub images: Vec<ImageAsset>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a node and returns its index. The caller appends in DFS
    /// order; `emit` does not reorder.
    pub fn push(&mut self, node: Node) -> u32 {
        let index = u32::try_from(self.nodes.len()).expect("document exceeds u32::MAX nodes");
        self.nodes.push(node);
        index
    }

    /// Appends an image asset and returns its index.
    pub fn push_image(&mut self, asset: ImageAsset) -> u32 {
        let index = u32::try_from(self.images.len()).expect("document exceeds u32::MAX images");
        self.images.push(asset);
        index
    }
}
