//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (DESIGN_1.md §8).
//!
//! Vocabulary scope: the v0.3 slice (DESIGN_1.md §11, drawn from the
//! §10.1 NOW list) — solid fills,
//! the four gradient kinds, image fills with scale modes, stroke with
//! align, rounded corners, and clip. The rect-table index is the document
//! DFS node index (DESIGN_1.md §5); `RectEntry.paint` indexes the
//! [`PaintTable`].

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

/// An index into the [`PaintTable`] — the type of [`RectEntry::paint`]
/// and the return of [`PaintTable::push`].
///
/// `#[repr(transparent)]` over `u32`: [`RectEntry`] stays blittable and
/// its layout unchanged, while a node index or any other bare `u32`
/// cannot be passed where a paint index belongs without an explicit
/// wrap.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaintIndex(pub u32);

/// One resolved rectangle — boundary B's per-node unit (DESIGN_1.md §7.3).
///
/// The rect-table index of this entry is the document DFS node index, so
/// there is no id field. `paint` resolves in the [`PaintTable`].
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
    pub paint: PaintIndex,
}

/// A 2D point or vector, in the coordinate space its context names.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// A row-major 2×3 affine transform: maps (x, y) to
/// (a·x + b·y + tx, c·x + d·y + ty). Same shape as `dashbuf`'s `Mat23`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat23 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

/// One gradient color stop; `offset` is normalized 0..1 along the
/// gradient's primary axis.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

/// The four gradient kinds of DESIGN_1.md §10.1 (angular serves gauges).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    Angular,
    Diamond,
}

/// The most stops one gradient may carry.
///
/// A property of the paint vocabulary, not of any one backend: the lean
/// painter pays for stops in uniform slots, so the ceiling is what every
/// painter can be held to. It lives here, on boundary B, because the
/// painter that panics above it (`dashscene-skia`) and the validator that
/// rejects it upstream (`dashscene-validator`, P4) have to agree on the
/// number — two copies of an `8` that silently disagree would make the
/// validator's guarantee false.
pub const MAX_GRADIENT_STOPS: usize = 8;

/// A gradient fill. One geometry model serves all four kinds: three
/// normalized handle positions in the node's box — the gradient origin,
/// the primary-axis end, and the secondary-axis end (Figma's
/// gradientHandlePositions). Handles are intent; resolved geometry is
/// per-painter math (P1).
///
/// `stops` carries at least one and at most [`MAX_GRADIENT_STOPS`] entries
/// — validated upstream (P4), and painters may assume it.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub handle_origin: Vec2,
    pub handle_primary: Vec2,
    pub handle_secondary: Vec2,
    pub stops: Vec<GradientStop>,
}

/// Figma image-fill scale modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

/// Encoded image container formats a painter can decode. Mirrors
/// `dashbuf`'s `ImageFormat`; GPU-native containers (KTX2, DESIGN_1.md
/// §9) arrive as new variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
}

/// One encoded image asset — bytes plus their container format. Each
/// painter decodes with its own machinery.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAsset {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

/// The image-asset table (mirrors `dashbuf`'s `Document.images`):
/// dense, indexed by [`PaintKind::Image`]'s `image` field. Part of the
/// painter input since the v0.3 vocabulary
/// (`docs/decisions/image-assets-cross-boundary-b.md`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageTable {
    entries: Vec<ImageAsset>,
}

impl ImageTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an asset and returns its index — the value a
    /// [`PaintKind::Image`] `image` field holds to reference it.
    pub fn push(&mut self, asset: ImageAsset) -> u32 {
        let index =
            u32::try_from(self.entries.len()).expect("image table exceeds u32::MAX entries");
        self.entries.push(asset);
        index
    }

    pub fn get(&self, index: u32) -> Option<&ImageAsset> {
        self.entries.get(index as usize)
    }

    /// Resolves an image index. Panics on an out-of-range index —
    /// indices are validated upstream (P4), same contract as
    /// [`PaintTable::resolve`].
    pub fn resolve(&self, index: u32) -> &ImageAsset {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "image index {index} out of range ({} assets): image indices are validated upstream (P4)",
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

/// Stroke placement relative to the node's outline. Painters that only
/// stroke on center lower Inside/Outside by path expansion
/// (DESIGN_1.md §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

/// A stroke. v0.3 strokes are solid-only (see
/// `docs/decisions/paint-entry-composition.md`); the color widens to a
/// fill additively if a real file ever needs gradient strokes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub align: StrokeAlign,
    pub color: Color,
}

/// Per-corner radii in document units; all zero (the default) = sharp
/// corners.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

/// One way to fill a rect. Effects (shadows, masks) land at v0.8 as new
/// variants.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintKind {
    Solid {
        color: Color,
    },
    Gradient(Gradient),
    Image {
        /// Index into the [`ImageTable`].
        image: u32,
        scale_mode: ScaleMode,
        /// Normalized image-space transform for [`ScaleMode::Crop`];
        /// identity when `None`.
        transform: Option<Mat23>,
        /// Tile magnification for [`ScaleMode::Tile`].
        tile_scale: f32,
    },
}

/// One paint-table entry (DESIGN_1.md §5's paint-table row: paint-kind
/// enum plus fill/stroke params): what a rect is filled with, how its
/// outline is stroked, how its corners round, and whether it clips its
/// children.
///
/// `fill: None` is the paint-less node — a layout-only container draws
/// nothing but still occupies its rect-table slot (index = DFS node
/// index).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintEntry {
    pub fill: Option<PaintKind>,
    pub stroke: Option<Stroke>,
    pub corners: CornerRadii,
    pub clip: bool,
}

impl PaintEntry {
    /// The v0.1 walking-skeleton shorthand: a solid fill and nothing else.
    pub fn solid(color: Color) -> Self {
        Self {
            fill: Some(PaintKind::Solid { color }),
            ..Self::default()
        }
    }
}

/// The paint table (DESIGN_1.md §5): dense, indexed by `RectEntry.paint`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintTable {
    entries: Vec<PaintEntry>,
}

impl PaintTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an entry and returns its index — the value a
    /// [`RectEntry::paint`] field holds to reference it.
    pub fn push(&mut self, entry: PaintEntry) -> PaintIndex {
        let index =
            u32::try_from(self.entries.len()).expect("paint table exceeds u32::MAX entries");
        self.entries.push(entry);
        PaintIndex(index)
    }

    pub fn get(&self, index: PaintIndex) -> Option<&PaintEntry> {
        self.entries.get(index.0 as usize)
    }

    /// Resolves a rect's paint index. This is the lookup painters use.
    ///
    /// Panics on an out-of-range index: indices are validated upstream
    /// (P4), so a miss is a broken contract between crates, and the
    /// panic for that case is centralized here — a painter must never
    /// skip a rect silently.
    pub fn resolve(&self, index: PaintIndex) -> &PaintEntry {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "paint index {} out of range ({} entries): paint indices are validated upstream (P4)",
                index.0,
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
    /// against `paints` (use [`PaintTable::resolve`]); image fills
    /// resolve their asset in `images` (an empty table is valid input
    /// for image-less scenes).
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
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable, images: &ImageTable);
}
