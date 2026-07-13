//! The in-memory DSB document — what a producer lowers *into*, and what the
//! emitter writes *out of*.
//!
//! It is deliberately not a second vocabulary. The paint types are
//! `dashpaint`'s (boundary B), so the one paint vocabulary spans the
//! document, the runtime, and the painter, and a lowering cannot invent a
//! construct no painter can draw.
//!
//! What it adds over `dashpaint` is the *document's* shape: a flattened DFS
//! node list whose array index is the rect-table index (DESIGN §5), layout
//! intent (never results — P1), and the pools nodes reference by index.

use dashpaint::{ImageAsset, PaintEntry};

/// A node's authored box. Intent, not a result (P1): under a flex parent the
/// solver owns placement and these offsets are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Box2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One node of the document. `parent` is an index into [`Dsb::nodes`], and
/// the array is in DFS order, so a parent's index is always lower than its
/// children's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DsbNode {
    pub name: Option<String>,
    pub parent: Option<u32>,
    pub box2d: Box2D,
    /// The node's style. `None` draws nothing (a layout-only container).
    pub paint: Option<Paint>,
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

/// One DSB document, ready to emit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dsb {
    /// Flattened DFS node tree: array index = rect-table index (DESIGN §5).
    pub nodes: Vec<DsbNode>,
    /// The image assets an image fill references by index.
    pub images: Vec<ImageAsset>,
}

impl Dsb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a node and returns its index. The caller appends in DFS
    /// order; `emit` does not reorder.
    pub fn push(&mut self, node: DsbNode) -> u32 {
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
