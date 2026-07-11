//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (DESIGN_1.md §8).
//!
//! v0.1 walking-skeleton scope: solid fills only. The rect-table index is
//! the document DFS node index (DESIGN_1.md §5); `RectEntry.paint` indexes
//! the [`PaintTable`].

/// An RGBA color, 4×f32 — the same shape as `dashbuf`'s `Color` struct.
///
/// `#[repr(C)]`: rect/paint data is blittable by design (DESIGN_1.md §7.3),
/// so the layout is fixed now even though nothing uploads it yet.
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
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: u32,
}

/// One paint-table entry. v0.1 knows solid fills only; gradients, images,
/// strokes, and effects land at v0.3 as new variants.
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
    /// Paints every rect in slice order (back-to-front: DFS order encodes
    /// document stacking), resolving each [`RectEntry::paint`] index
    /// against `paints`.
    ///
    /// Infallible by design: vocabulary and indices are validated upstream
    /// (P4), so an out-of-range `paint` index is a broken contract between
    /// crates — implementations may panic on it.
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
}
