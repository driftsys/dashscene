//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (docs/design/architecture.md).
//!
//! Vocabulary scope: the v0.3 slice (docs/roadmap.md, drawn from the
//! docs/specification/04-figma-vocabulary-profile.md NOW list) — solid fills,
//! the four gradient kinds, image fills with scale modes, stroke with
//! align, rounded corners, and clip. The rect-table index is the document
//! DFS node index (docs/design/dashbuf.md); `RectEntry.paint` indexes the
//! [`PaintTable`] and `RectEntry.clip` indexes the [`ClipTable`].
//!
//! Clipping crosses this boundary already resolved: `dashscene-core`
//! turns "this node clips its children" into a per-rect [`ClipRegion`]
//! at commit (issue #97), because a flat rect table carries no ancestors
//! for a painter to walk (P2).

/// An RGBA color, 4×f32 — the same shape as `dashbuf`'s `Color` struct.
///
/// `#[repr(C)]` fixes the layout now: solid-fill colors are per-frame
/// painter input, and docs/specification/03-target-hardware-rules.md (R-T4) plans instance-buffer uploads
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

/// An index into the [`ClipTable`] — the type of [`RectEntry::clip`]
/// and the return of [`ClipTable::push`].
///
/// `#[repr(transparent)]` over `u32`, for the same reason as
/// [`PaintIndex`]: [`RectEntry`] stays blittable, and no bare `u32` can
/// pass for a clip index without an explicit wrap.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipIndex(pub u32);

impl ClipIndex {
    /// The region every unclipped rect references. [`ClipTable::new`]
    /// reserves index 0 for it, so an unclipped rect still resolves —
    /// this is a real entry, not a sentinel
    /// (`docs/decisions/boundary-b-unification.md`).
    pub const UNCLIPPED: ClipIndex = ClipIndex(0);
}

/// One resolved rectangle — boundary B's per-node unit (docs/design/architecture.md).
///
/// The rect-table index of this entry is the document DFS node index, so
/// there is no id field. `paint` resolves in the [`PaintTable`], `clip`
/// in the [`ClipTable`].
///
/// `#[repr(C)]`: docs/design/architecture.md calls rect entries blittable, and R-T4
/// plans dirty-range instance-buffer uploads straight from the rect table.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: PaintIndex,
    /// The clip that applies to this rect, resolved from its clipping
    /// ancestors by `dashscene-core` at commit — a painter never
    /// re-derives the tree (P2). [`ClipIndex::UNCLIPPED`] when no
    /// ancestor clips.
    pub clip: ClipIndex,
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

/// The four gradient kinds of docs/specification/04-figma-vocabulary-profile.md (angular serves gauges).
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
/// `dashbuf`'s `ImageFormat`; GPU-native containers (KTX2,
/// docs/specification/03-target-hardware-rules.md) arrive as new variants.
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
/// (docs/technotes/rendering-and-painters.md).
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

/// One clipping box of a resolved [`ClipRegion`]: an axis-aligned box in
/// the same absolute space as [`RectEntry`], rounded by `corners` (all
/// zero = a sharp rect).
///
/// This is a clipping *ancestor*'s box, already resolved — not the
/// clipped rect's own geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub corners: CornerRadii,
}

/// The clip that applies to one rect: the **intersection** of its boxes,
/// which are the boxes of its clipping ancestors, outermost first. No
/// boxes = unclipped.
///
/// `dashscene-core` resolves this at commit (docs/design/architecture.md
/// `Paint.clip` — "clips its children to its box"): boundary B is a flat
/// rect table, so a painter has no ancestors to walk and P2 forbids it
/// re-deriving them. The box list is kept rather than pre-intersected
/// because the intersection of two rounded rects is not a rounded rect.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipRegion {
    boxes: Vec<ClipBox>,
}

impl ClipRegion {
    /// The region of a rect no ancestor clips — the one [`ClipTable`]
    /// reserves at [`ClipIndex::UNCLIPPED`].
    pub fn unclipped() -> Self {
        Self::default()
    }

    /// A region clipped by every box, outermost ancestor first.
    pub fn new(boxes: Vec<ClipBox>) -> Self {
        Self { boxes }
    }

    /// The boxes to intersect, outermost ancestor first.
    pub fn boxes(&self) -> &[ClipBox] {
        &self.boxes
    }

    /// True when no ancestor clips this rect (no boxes).
    pub fn is_unclipped(&self) -> bool {
        self.boxes.is_empty()
    }
}

/// The clip table: dense, indexed by [`RectEntry::clip`]. Index 0 is
/// always the unclipped region ([`ClipIndex::UNCLIPPED`]), so every rect
/// resolves; regions are deduplicated by `dashscene-core`, so the
/// subtree under one clipping ancestor shares one entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipTable {
    entries: Vec<ClipRegion>,
}

impl Default for ClipTable {
    fn default() -> Self {
        Self {
            entries: vec![ClipRegion::unclipped()],
        }
    }
}

// A `ClipTable` is never empty — index 0 is the reserved unclipped
// region — so an `is_empty` that always answers false would be a trap.
#[allow(clippy::len_without_is_empty)]
impl ClipTable {
    /// A table holding only the reserved unclipped region.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a region and returns its index — the value a
    /// [`RectEntry::clip`] field holds to reference it.
    pub fn push(&mut self, region: ClipRegion) -> ClipIndex {
        let index = u32::try_from(self.entries.len()).expect("clip table exceeds u32::MAX entries");
        self.entries.push(region);
        ClipIndex(index)
    }

    pub fn get(&self, index: ClipIndex) -> Option<&ClipRegion> {
        self.entries.get(index.0 as usize)
    }

    /// Resolves a rect's clip index. This is the lookup painters use.
    ///
    /// Panics on an out-of-range index, for the same reason as
    /// [`PaintTable::resolve`]: a miss is a broken contract between
    /// crates, and a painter must never skip a rect's clip silently
    /// (P4).
    pub fn resolve(&self, index: ClipIndex) -> &ClipRegion {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "clip index {} out of range ({} regions): clip indices are validated upstream (P4)",
                index.0,
                self.entries.len()
            )
        })
    }

    /// Region count, including the reserved unclipped region at index 0.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
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

/// One paint-table entry (docs/design/dashbuf.md's paint-table row: paint-kind
/// enum plus fill/stroke params): what a rect is filled with, how its
/// outline is stroked, and how its corners round.
///
/// `fill: None` is the paint-less node — a layout-only container draws
/// nothing but still occupies its rect-table slot (index = DFS node
/// index).
///
/// Whether a node clips its children (`Paint.clip`, docs/design/architecture.md)
/// is *intent*, and does not appear here: `dashscene-core` resolves it
/// at commit into the [`ClipTable`] each [`RectEntry::clip`] references
/// (issue #97). The intent itself lives in the document (`dashbuf`'s
/// `Paint.clip`) and in the arena (`Prop::Clip`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintEntry {
    pub fill: Option<PaintKind>,
    pub stroke: Option<Stroke>,
    pub corners: CornerRadii,
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

/// The paint table (docs/design/dashbuf.md): dense, indexed by `RectEntry.paint`.
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

/// One glyph placed in absolute document space (y-down, layout origin
/// at the top-left): the pen position on its line's baseline, with the
/// shaping offsets already applied.
///
/// Plain mirror of `dashscene-typeset`'s `PositionedGlyph` (dashpaint
/// depends on no crate — the same reason [`Color`] mirrors `dashbuf`'s
/// `Color`). Whoever stages the run adds the text node's resolved box
/// origin, so positions reach the painter absolute and the painter never
/// moves anything (P2). The painter combines each position with the
/// atlas glyph's y-up `plane_em` quad to place the textured quad.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
}

/// One atlas glyph's placement geometry, keyed by glyph id. Only glyphs
/// that paint appear: an empty outline (a space) has no quad and is
/// omitted, so a `glyph_id` absent from an [`Atlas`] draws nothing.
///
/// Plain mirror of `dashscene-typeset`'s `GlyphEntry` bounds. Both
/// rectangles are `[left, bottom, right, top]`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasGlyph {
    pub glyph_id: u16,
    /// Quad bounds in ems, y-up, baseline origin (the metrics blob's
    /// `plane_em`).
    pub plane_em: [f32; 4],
    /// Texel bounds in the atlas image, bottom-left origin (the metrics
    /// blob's `atlas_px`).
    pub atlas_px: [f32; 4],
}

/// An index into a [`GlyphRunTable`]'s atlas list — the type of
/// [`GlyphRun::atlas`] and the return of [`GlyphRunTable::push_atlas`].
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasIndex(pub u32);

/// The MSDF glyph atlas a painter samples: the image, its parameters,
/// and per-glyph placement.
///
/// Plain mirror of the `dashscene-typeset` atlas metrics blob (dashpaint
/// depends on no crate). The build-time pipeline produces the metrics; a
/// stager converts them into this boundary-B shape, the same way an
/// image fill's bytes reach the painter as an [`ImageAsset`].
#[derive(Debug, Clone, PartialEq)]
pub struct Atlas {
    /// The MSDF atlas image (RGB distance channels), an encoded asset.
    pub image: ImageAsset,
    /// Atlas image size in texels.
    pub width: u32,
    pub height: u32,
    /// The size, in texels per em, the atlas was rendered at.
    pub px_per_em: u16,
    /// The MSDF distance range in atlas texels. The painter's
    /// screen-pixel range is `distance_range_px * render_size /
    /// px_per_em` (`plane_em` and `atlas_px` bake the range into the
    /// bounds, so this scales the sharpness of the edge, not the size).
    pub distance_range_px: f32,
    /// Placement per glyph, sorted and unique by `glyph_id` (the metrics
    /// blob's own invariant — painters may binary-search it).
    glyphs: Vec<AtlasGlyph>,
}

impl Atlas {
    /// An atlas over `glyphs`, which must be sorted and unique by
    /// `glyph_id` — the metrics blob guarantees it, so [`glyph`](Self::glyph)
    /// binary-searches.
    pub fn new(
        image: ImageAsset,
        width: u32,
        height: u32,
        px_per_em: u16,
        distance_range_px: f32,
        glyphs: Vec<AtlasGlyph>,
    ) -> Self {
        debug_assert!(
            glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id),
            "atlas glyphs must be sorted and unique by glyph id"
        );
        Self {
            image,
            width,
            height,
            px_per_em,
            distance_range_px,
            glyphs,
        }
    }

    /// The placement for `glyph_id`, or `None` when the atlas has no quad
    /// for it (an empty-outline glyph such as a space, or a glyph outside
    /// the atlas's charset — which paints nothing).
    pub fn glyph(&self, glyph_id: u16) -> Option<&AtlasGlyph> {
        self.glyphs
            .binary_search_by_key(&glyph_id, |g| g.glyph_id)
            .ok()
            .map(|i| &self.glyphs[i])
    }
}

/// One positioned glyph run: a sequence of placed glyphs that share a
/// render size, a fill color, and an atlas (one style per text node in
/// the v0.5 Latin subset — `docs/design/architecture.md` §7.2).
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    /// The atlas the glyphs are sampled from.
    pub atlas: AtlasIndex,
    /// Render size in document units (px per em).
    pub size: f32,
    /// The text fill; the MSDF coverage modulates this color.
    pub color: Color,
    /// The placed glyphs, in draw order.
    pub glyphs: Vec<GlyphQuad>,
}

/// The glyph-run table (`docs/design/architecture.md` §7.3): the
/// positioned glyph runs plus the atlases they reference — the text half
/// of the painter input, a sibling of the rect table. Empty for a
/// text-free scene, so every existing caller passes
/// [`GlyphRunTable::new`].
///
/// Runs cross boundary B already shaped, wrapped, and positioned: the
/// one typesetter did that once, and the painter only draws the quads
/// (P2). The atlases are carried with the runs because a run's glyph ids
/// are meaningless without the atlas that places them, the same way an
/// image fill needs its [`ImageTable`] entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphRunTable {
    atlases: Vec<Atlas>,
    runs: Vec<GlyphRun>,
}

impl GlyphRunTable {
    /// An empty table — no runs, no atlases.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an atlas and returns its index — the value a
    /// [`GlyphRun::atlas`] field holds to reference it.
    pub fn push_atlas(&mut self, atlas: Atlas) -> AtlasIndex {
        let index =
            u32::try_from(self.atlases.len()).expect("glyph-run table exceeds u32::MAX atlases");
        self.atlases.push(atlas);
        AtlasIndex(index)
    }

    /// Appends a positioned run.
    pub fn push_run(&mut self, run: GlyphRun) {
        self.runs.push(run);
    }

    /// The runs to draw, in order.
    pub fn runs(&self) -> &[GlyphRun] {
        &self.runs
    }

    /// Resolves a run's atlas index. Panics on an out-of-range index,
    /// the same contract as [`PaintTable::resolve`]: a miss is a broken
    /// contract between crates, validated upstream (P4).
    pub fn atlas(&self, index: AtlasIndex) -> &Atlas {
        self.atlases.get(index.0 as usize).unwrap_or_else(|| {
            panic!(
                "atlas index {} out of range ({} atlases): atlas indices are validated upstream (P4)",
                index.0,
                self.atlases.len()
            )
        })
    }

    /// The atlases the runs reference, in [`AtlasIndex`] order — so a
    /// painter can prepare each atlas (decode, upload) once, rather than
    /// once per run that samples it.
    pub fn atlases(&self) -> &[Atlas] {
        &self.atlases
    }

    /// True when the table carries no runs.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

/// Boundary B (docs/design/architecture.md): the one trait every paint backend
/// implements. A painter only colors — it never measures, wraps, kerns,
/// or moves anything (P2).
pub trait Painter {
    /// Paints every rect, resolving each [`RectEntry::paint`] index
    /// against `paints` (use [`PaintTable::resolve`]) and each
    /// [`RectEntry::clip`] index against `clips` (use
    /// [`ClipTable::resolve`]); image fills resolve their asset in
    /// `images` (an empty table is valid input for image-less scenes).
    ///
    /// A rect draws only inside its resolved [`ClipRegion`] — the
    /// intersection of the region's boxes. The region is already
    /// ancestor-resolved: a painter clips against the boxes it is given
    /// and never asks which node they came from (P2).
    ///
    /// Slice order defines stacking: a later entry composites over an
    /// earlier one (DFS order encodes document stacking). The composited
    /// result is the contract; iteration order is the implementation's
    /// choice (the lean painter draws opaque cores front-to-back,
    /// docs/specification/03-target-hardware-rules.md R-T2).
    ///
    /// `glyphs` is the glyph-run table: positioned glyph runs and the
    /// atlases they sample, staged already shaped and placed. The v0.5
    /// Latin subset composites every run over all rects (text is
    /// foreground); a full z-interleave of runs with rects is later work.
    /// An empty table ([`GlyphRunTable::new`]) is valid input for a
    /// text-free scene.
    ///
    /// `dirty` is the rect indices whose entry changed since the commit
    /// that produced the previous `rects` — **advisory**. `None` means
    /// the caller has no dirty information (hand-built tables, or a first
    /// frame). Ignoring it and redrawing everything is always correct,
    /// and a painter that honors it MUST produce output identical to one
    /// that does not. It exists so a painter can meet R-T4
    /// (docs/specification/03-target-hardware-rules.md): per-frame CPU
    /// cost is the dirty-range instance-buffer upload from the rect table
    /// plus submission, and nothing else. It is not a license for
    /// damage-region partial redraw, which R-T1 forbids on a tiling GPU.
    ///
    /// Infallible by design: vocabulary and indices are validated upstream
    /// (P4), so there is no legitimate runtime failure. An out-of-range
    /// `paint` or `clip` index is a broken contract between crates;
    /// [`PaintTable::resolve`] and [`ClipTable::resolve`] centralize the
    /// panic for that case.
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        dirty: Option<&[u32]>,
    );
}
