//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (DESIGN_1.md §8).
//!
//! v0.1 walking-skeleton scope: solid fills only. The rect-table index is
//! the document DFS node index (DESIGN_1.md §5); `RectEntry.paint` indexes
//! the [`PaintTable`].

/// An RGBA color, 4×f32 — the same shape as `dashbuf`'s `Color` struct.
///
/// `#[repr(C)]` fixes the layout now: solid-fill colors are per-frame
/// painter input, and DESIGN_1.md §9 (R-T4) plans instance-buffer uploads
/// of that input, even though nothing uploads it yet.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// One resolved rectangle — boundary B's per-node unit (DESIGN_1.md §7.3).
///
/// The rect-table index of this entry is the document DFS node index, so
/// there is no id field. `paint` indexes the [`PaintTable`].
///
/// `#[repr(C)]`: DESIGN_1.md §7.3 calls rect entries blittable, and R-T4
/// plans dirty-range instance-buffer uploads straight from the rect table.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: u32,
}

/// One paint-table entry. v0.1 knows solid fills only; further paint
/// kinds land as new variants at their slices (gradients, images, and
/// stroke handling at v0.3; effects such as shadows and masks at v0.8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintKind {
    Solid { color: Color },
}

/// The paint table (DESIGN_1.md §5): dense, indexed by `RectEntry.paint`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintTable {
    entries: Vec<PaintKind>,
}

impl PaintTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an entry and returns its index — the value a
    /// [`RectEntry::paint`] field holds to reference it.
    pub fn push(&mut self, kind: PaintKind) -> u32 {
        let index =
            u32::try_from(self.entries.len()).expect("paint table exceeds u32::MAX entries");
        self.entries.push(kind);
        index
    }

    pub fn get(&self, index: u32) -> Option<&PaintKind> {
        self.entries.get(index as usize)
    }

    /// Resolves a rect's paint index. This is the lookup painters use.
    ///
    /// Panics on an out-of-range index: indices are validated upstream
    /// (P4), so a miss is a broken contract between crates, and the
    /// panic for that case is centralized here — a painter must never
    /// skip a rect silently.
    pub fn resolve(&self, index: u32) -> &PaintKind {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "paint index {index} out of range ({} entries): paint indices are validated upstream (P4)",
                self.entries.len()
            )
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Boundary B (DESIGN_1.md §4, §8): the one trait every paint backend
/// implements. A painter only colors — it never measures, wraps, kerns,
/// or moves anything (P2).
pub trait Painter {
    /// Paints every rect, resolving each [`RectEntry::paint`] index
    /// against `paints` (use [`PaintTable::resolve`]).
    ///
    /// Slice order defines stacking: a later entry composites over an
    /// earlier one (DFS order encodes document stacking). The composited
    /// result is the contract; iteration order is the implementation's
    /// choice (the lean painter draws opaque cores front-to-back,
    /// DESIGN_1.md §9 R-T2).
    ///
    /// Infallible by design: vocabulary and indices are validated upstream
    /// (P4), so there is no legitimate runtime failure. An out-of-range
    /// `paint` index is a broken contract between crates;
    /// [`PaintTable::resolve`] centralizes the panic for that case.
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
}
