//! Committed output — what a painter consumes (boundary B,
//! DESIGN_1.md §7.3).
//!
//! The types are `dashpaint`'s — the boundary-B unification of story #4
//! (`docs/decisions/boundary-b-unification.md`): a flat rect table
//! indexed by document DFS node index, blittable entries, and the paint
//! table. Every rect resolves; an unfilled node references the shared
//! draws-nothing entry (`PaintEntry::default()`), not a sentinel.

pub use dashpaint::{
    ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CornerRadii, PaintEntry, PaintIndex,
    PaintKind, PaintTable, RectEntry,
};

use crate::arena::NodeId;

/// One committed buffer: the resolved rect table, the deduplicated
/// paint table, the resolved clip table, the generation stamp, the
/// dirty set, and the NodeId↔rect-index correspondence for the commit
/// that produced it.
#[derive(Debug, Default)]
pub struct CommittedScene {
    pub(crate) rects: Vec<RectEntry>,
    pub(crate) paints: PaintTable,
    pub(crate) clips: ClipTable,
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

    /// Deduplicated paint table, in first-use DFS order.
    pub fn paints(&self) -> &PaintTable {
        &self.paints
    }

    /// Resolved clip table: the region each [`RectEntry::clip`]
    /// references, with the ancestor clips already walked (issue #97).
    /// Index 0 is the unclipped region; regions are deduplicated, so one
    /// clipping ancestor's whole subtree shares one entry.
    pub fn clips(&self) -> &ClipTable {
        &self.clips
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
