//! Committed output — what a painter consumes (boundary B,
//! DESIGN_1.md §7.3).
//!
//! The shapes are pinned by the v0.1 boundary-B contract: a flat rect
//! table indexed by document DFS node index, blittable entries, and a
//! solid-fill paint table. `dashscene-core` owns these types; story #4
//! reconciles them with `dashpaint`'s painter-side declarations.

use crate::arena::NodeId;

/// Paint index of a node with no fill. Painters skip these entries.
/// Mirrors `dashbuf`'s `NO_PARENT` sentinel (`u32::MAX`).
pub const NO_PAINT: u32 = u32::MAX;

/// Solid-fill color, 4×f32 RGBA — the shape of `dashbuf`'s `Color`
/// struct.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// One resolved rect: absolute position + size, plus the paint-table
/// index (`NO_PAINT` for unfilled nodes). Blittable plain data.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: u32,
}

/// One paint-table entry. v0.1 has exactly one paint kind: solid fill.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub color: Color,
}

/// One committed buffer: the resolved rect table, the deduplicated
/// paint table, the generation stamp, the dirty set, and the
/// NodeId↔rect-index correspondence for the commit that produced it.
#[derive(Debug, Default)]
pub struct CommittedScene {
    pub(crate) rects: Vec<RectEntry>,
    pub(crate) paints: Vec<Paint>,
    pub(crate) generation: u64,
    pub(crate) dirty: Vec<u32>,
    /// Rect index → NodeId (DFS order of the commit).
    pub(crate) node_ids: Vec<NodeId>,
    /// NodeId slot → rect index.
    pub(crate) rect_index: Vec<u32>,
}

impl CommittedScene {
    /// Resolved rect table; index = document DFS node index.
    pub fn rects(&self) -> &[RectEntry] {
        &self.rects
    }

    /// Deduplicated solid-fill paint table, in first-use DFS order.
    pub fn paints(&self) -> &[Paint] {
        &self.paints
    }

    /// Commit counter: 0 before the first commit, +1 per commit —
    /// including commits that changed nothing (the dirty set says
    /// what changed; the generation says a commit happened).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Sorted rect indices whose entry differs from the previous
    /// commit (or did not exist in it).
    pub fn dirty(&self) -> &[u32] {
        &self.dirty
    }

    /// The node a rect entry was resolved from.
    ///
    /// # Panics
    ///
    /// Panics if `rect_index` is out of range for [`rects`](Self::rects).
    pub fn node_of(&self, rect_index: u32) -> NodeId {
        self.node_ids[rect_index as usize]
    }

    /// The rect index a node resolved to in this commit, or `None`
    /// for a node added after this commit.
    pub fn rect_index_of(&self, node: NodeId) -> Option<u32> {
        self.rect_index.get(node.index()).copied()
    }
}
