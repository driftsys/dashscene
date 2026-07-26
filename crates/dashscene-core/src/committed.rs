//! Committed output — what a painter consumes (boundary B,
//! docs/design/architecture.md).
//!
//! The types are `dashpaint`'s — the boundary-B unification of story #4
//! (`docs/decisions/boundary-b-unification.md`): a flat rect table
//! indexed by document DFS node index, blittable entries, and the paint
//! table. Every rect resolves; an unfilled node references the shared
//! draws-nothing entry (`PaintEntry::default()`), not a sentinel.

pub use dashpaint::{
    Blur, BlurKind, ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CornerRadii, Gradient,
    GradientKind, GradientStop, GroupComposite, ImageAsset, ImageFormat, ImageTable, Mat23,
    PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry, ScaleMode, Shadow, ShadowKind,
    Stroke, StrokeAlign, Vec2, VectorField,
};

use std::sync::Arc;

use crate::arena::NodeId;

/// One committed buffer: the resolved rect table, the deduplicated
/// paint table, the resolved clip table, the generation stamp, the
/// dirty set, and the NodeId↔rect-index correspondence for the commit
/// that produced it.
///
/// The paint and clip tables, and the two NodeId↔index maps, are behind
/// `Arc`. An incremental commit builds the back buffer as a copy of the
/// front one patched at the changed indices (issue #164): the rect table
/// is cloned and patched, but the paint and clip tables are shared by
/// reference and grown only when a genuinely new entry appears (their
/// indices are stable across commits, so an unchanged entry keeps its
/// slot), and the two index maps are shared outright unless the tree
/// structure changed. A geometry-only or no-op commit therefore touches
/// no table allocation at all.
#[derive(Debug, Default)]
pub struct CommittedScene {
    pub(crate) rects: Vec<RectEntry>,
    pub(crate) paints: Arc<PaintTable>,
    pub(crate) images: Arc<ImageTable>,
    pub(crate) clips: Arc<ClipTable>,
    /// The render-target group opacities, in ascending `start` (DFS
    /// pre-order). Recomputed each commit — small and structural, unlike
    /// the pooled paint and clip tables.
    pub(crate) groups: Vec<GroupComposite>,
    pub(crate) generation: u64,
    pub(crate) dirty: Vec<u32>,
    /// Rect index → NodeId (DFS order of the commit).
    pub(crate) node_ids: Arc<Vec<NodeId>>,
    /// NodeId slot → rect index.
    pub(crate) rect_index: Arc<Vec<u32>>,
}

impl CommittedScene {
    /// Resolved rect table; index = document DFS node index.
    pub fn rects(&self) -> &[RectEntry] {
        &self.rects
    }

    /// Deduplicated paint table, in first-use DFS order.
    pub fn paints(&self) -> &PaintTable {
        self.paints.as_ref()
    }

    /// The image assets an image fill resolves against — the fourth table a
    /// painter is handed. Owned by the scene so a document loaded from a
    /// `.dsb` is self-contained.
    pub fn images(&self) -> &ImageTable {
        &self.images
    }

    /// Resolved clip table: the region each [`RectEntry::clip`]
    /// references, with the ancestor clips already walked (issue #97).
    /// Index 0 is the unclipped region; regions are deduplicated, so one
    /// clipping ancestor's whole subtree shares one entry.
    pub fn clips(&self) -> &ClipTable {
        self.clips.as_ref()
    }

    /// The render-target group opacities a painter composites (the fifth
    /// input crossing boundary B, `docs/decisions/masks-and-group-opacity.md`).
    /// Each names a rect subtree range and the alpha its offscreen layer
    /// composites at. The free-path (non-overlapping) opacity rides on
    /// [`RectEntry::opacity`] instead, so a scene with no overlapping
    /// group opacity has an empty slice here.
    pub fn groups(&self) -> &[GroupComposite] {
        &self.groups
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
