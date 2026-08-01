//! The in-memory arena and its staged mutation API
//! (docs/design/architecture.md, docs/decisions/staged-mutation-v01-scope.md).
//!
//! Producers mutate through a [`Txn`] obtained from [`Arena::open`];
//! nothing becomes visible to painters until [`Txn::commit`] resolves
//! the intent model into the committed output (P3 — producers mutate,
//! the runtime owns time). One `Txn` at a time, enforced by the borrow
//! checker. Dropping a `Txn` without committing leaves the staged
//! changes pending; they publish with the next commit ("staged" means
//! batched visibility, not rollback).

use std::cmp::Ordering;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::bindings::{Binding, Channel, ScalarTransform, SignalDecl, SignalId};
use crate::committed::{
    Atlas, Blur, ClipBox, ClipIndex, ClipTable, ClipView, Color, CommittedScene, CornerRadii,
    GlyphQuad, GlyphRun, GlyphRunTable, GroupComposite, ImageAsset, ImageTable, PaintEntry,
    PaintIndex, PaintKind, PaintTable, RectEntry, Shadow, Stroke, StrokeAlign, Vec2, VectorField,
};

/// Stable handle to a node in one [`Arena`]. Returned by
/// [`Txn::add_node`] and never invalidated (v0.1 has no node removal).
/// Only meaningful for the arena that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// The node's dense slot — its position in the arena's node array,
    /// assigned at [`Txn::add_node`] and stable for the node's life. A
    /// retained [`LayoutSolver`] keys its per-node state (Taffy nodes,
    /// parents, previous layouts) by this slot; `0..arena.node_count()`
    /// covers every node (issue #164).
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One node's resolved rectangle in absolute coordinates — the
/// geometry a [`LayoutSolver`] returns per node and the committed rect
/// table carries.
#[derive(Clone, Copy, Debug)]
pub struct SolvedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The geometry seam (P2 — one solver, docs/design/architecture.md): commit asks
/// exactly one solver for every node's rect and computes no geometry
/// of its own. `dashscene-engine`'s `TaffySolver` is the product
/// implementation; [`Txn::commit`] uses core's internal fixed-geometry
/// resolution (mode-`None` passthrough semantics).
pub trait LayoutSolver {
    /// Resolve the nodes of `arena` whose absolute rect changed since the
    /// previous solve to their new rects. A solver may return every node
    /// (the internal [`FixedSolver`] always does), or only the ones that
    /// moved or resized — an incremental solver reports just those and
    /// leaves the rest for [`Txn::commit_with`] to carry forward from the
    /// previous commit (issue #164). The one hard rule is index integrity
    /// (P4): every returned id must be a node of this arena, and no id may
    /// appear twice. A node that is neither returned here nor present in
    /// the previous commit has no rect at all, which
    /// [`Txn::commit_with`] rejects loudly rather than resolving to a
    /// degenerate default.
    ///
    /// The `&mut self` receiver lets a solver retain state across solves —
    /// `dashscene-engine`'s `TaffySolver` keeps its Taffy tree so an
    /// unchanged frame costs no re-solve.
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)>;

    /// The atlases every run [`stage_text`](Self::stage_text) returns
    /// samples, in [`AtlasIndex`] order — the set commit hands the painter
    /// alongside the runs, because a run's glyph ids mean nothing without
    /// the atlas that places them.
    ///
    /// Shared rather than copied: commit rebuilds the run table each
    /// frame, but the atlas set behind it is a build artifact that does
    /// not change, and copying it per commit would be per-frame cost R-T4
    /// bounds away.
    ///
    /// The default is the empty set, which is what a text-free scene and
    /// every solver that stages no text need.
    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        Arc::new(Vec::new())
    }

    /// Stage the glyph runs for every text node of `arena` — one or more
    /// runs per node — placed against `geometry`.
    ///
    /// This is the text half of the geometry seam, and it exists for the
    /// same reason (P2 — one typesetter): commit asks exactly one stager
    /// for every text node's placed glyphs and shapes nothing itself.
    /// `dashscene-core` has no typesetter, no fonts and no atlas, and
    /// needs none — it *stamps* runs rather than building them.
    ///
    /// `geometry` resolves a node to the rect **this commit just solved**,
    /// not the previous commit's. A stager that reads
    /// [`Arena::committed`] instead would place glyphs at last frame's
    /// boxes, which is only correct for a stager that runs after the
    /// commit has published.
    ///
    /// [`GlyphRun::rect`] on a returned run is ignored: commit stamps it
    /// from the [`NodeId`] beside it, so a stager cannot get the anchor
    /// wrong and no run can disagree with the rect table it is read
    /// against.
    ///
    /// [`GlyphRun::glyphs`] is likewise not the stager's to fill. A staged
    /// run carries its quads beside it in [`StagedRun`] and its range as
    /// [`GlyphRange::UNASSIGNED`]; commit sorts the runs by anchor before
    /// pushing them, so no offset a stager could compute would survive the
    /// reorder anyway (story #578).
    ///
    /// The default stages nothing, so every existing implementer keeps
    /// compiling untouched and a text-free scene costs nothing.
    ///
    /// # Panics
    ///
    /// [`Txn::commit_with`] panics if a returned id is not a node of this
    /// arena — the same index-integrity contract [`solve`](Self::solve)
    /// carries (P4).
    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        let _ = (arena, geometry);
        Vec::new()
    }
}

/// One run a stager produced, with the quads it draws.
///
/// The quads travel beside the run rather than inside it because
/// [`GlyphRun::glyphs`] is a range into the table's flat array (story
/// #578), and a stager has no table to index. Commit sorts these by anchor
/// and then pushes each through [`GlyphRunTable::push_run`], which is what
/// assigns the range — so a stager could not compute a surviving offset
/// even if it had one.
#[derive(Debug, Clone, PartialEq)]
pub struct StagedRun {
    /// The node this run was shaped from. Commit turns it into
    /// [`GlyphRun::rect`].
    pub node: NodeId,
    /// The run, carrying [`GlyphRange::UNASSIGNED`].
    pub run: GlyphRun,
    /// Its quads, in draw order.
    pub quads: Vec<GlyphQuad>,
}

/// Layout mode of a container node. `None` = passthrough (children
/// place by their authored offsets); `Horizontal`/`Vertical` = flex
/// (the solver owns placement — story #9). `Wrap` (v0.8, story #43) is
/// a horizontal wrapping row — Figma's `layoutWrap` exists for
/// horizontal auto-layout only. `Grid` (v0.8) places children in the
/// container's track lists ([`Prop::GridRows`]/[`Prop::GridColumns`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    #[default]
    None,
    Horizontal,
    Vertical,
    Wrap,
    Grid,
}

/// How a node sizes itself along one axis: `Fixed` uses the authored
/// width/height as the datum, `Hug` wraps content, `Fill` stretches
/// into the parent's free space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisSizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// `Baseline` (v0.8, story #43, Q-4) aligns a horizontal row's children
/// on their flex baselines: a leaf's baseline is its bottom edge, a
/// nested row propagates its first line's baseline. In a `Vertical`
/// container it degrades to start alignment (Taffy computes baselines
/// for rows only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Baseline,
}

/// Padding insets, named to make edge order unmistakable — solver
/// mappings (story #9: Taffy's rect is left, right, top, bottom) must
/// not depend on positional convention.
#[derive(Clone, Copy, Debug, Default)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// One grid row or column track (v0.8, story #43) — mirrors the
/// `dashbuf` `GridTrack` table. `Fixed` is a document-unit length;
/// `Fraction` is a flexible weight over the free space (Figma's
/// `minmax(0, Nfr)` serialized track).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridTrack {
    Fixed(f32),
    Fraction(f32),
}

/// One node's layout intent — the authored fixed geometry plus the
/// v0.2 flex vocabulary. Mirrors the `dashbuf` schema shapes
/// (`FixedSizeLayout`, `LayoutContainer`, `LayoutConstraints`) without
/// linking the generated code. Stored intent: until story #9's Taffy
/// solve, `commit` resolves the fixed geometry only.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub mode: LayoutMode,
    pub gap: f32,
    /// The cross-axis gap (v0.8, story #43): the spacing between wrap
    /// lines and between grid rows — `gap` stays the main-axis spacing,
    /// which for `Wrap` and `Grid` is the horizontal one. `None` =
    /// follows `gap` (the v0.2 both-axes mapping).
    pub cross_gap: Option<f32>,
    pub padding: EdgeInsets,
    /// Outer margin in the parent's flow. Negative values express
    /// overlap and are what [`Txn::lower_negative_gaps`] rewrites a
    /// negative container `gap` into.
    pub margin: EdgeInsets,
    pub main_align: MainAxisAlign,
    pub cross_align: CrossAxisAlign,
    pub sizing_h: AxisSizing,
    pub sizing_v: AxisSizing,
    /// `None` = unconstrained (absence of intent, not a sentinel).
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    /// Grid placement under a mode-`Grid` parent (v0.8, story #43): the
    /// 0-based anchor cell. `None` = auto-placed in document order.
    pub grid_row: Option<u16>,
    pub grid_column: Option<u16>,
    /// How many tracks the node spans from its anchor. Defaults to 1.
    pub grid_row_span: u16,
    pub grid_column_span: u16,
    /// `false` lowers to Taffy `Display::None` (issue #165): not drawn
    /// and out of layout, siblings reflow. Defaults to `true`.
    pub visible: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            mode: LayoutMode::default(),
            gap: 0.0,
            cross_gap: None,
            padding: EdgeInsets::default(),
            margin: EdgeInsets::default(),
            main_align: MainAxisAlign::default(),
            cross_align: CrossAxisAlign::default(),
            sizing_h: AxisSizing::default(),
            sizing_v: AxisSizing::default(),
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            grid_row: None,
            grid_column: None,
            grid_row_span: 1,
            grid_column_span: 1,
            visible: true,
        }
    }
}

/// One settable node property: the authored parent-relative offset
/// and fixed size, the solid fill (v0.1), the v0.2 flex vocabulary,
/// and the text content and style (v0.5).
///
/// `Fill`, `Text`, and `TextStyle` set a value but cannot clear one
/// back to absent — the same deliberate gap `Fill` opened at v0.1
/// (`docs/decisions/staged-mutation-v01-scope.md`); a clear operation
/// lands with the first producer that needs one. The min/max
/// constraint props share the gap: they set a bound but cannot clear
/// one back to unconstrained
/// (`docs/decisions/flex-vocabulary-shape.md`).
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    X(f32),
    Y(f32),
    Width(f32),
    Height(f32),
    /// The v0.1 solid-fill shorthand. Equivalent to
    /// `FillWith(PaintKind::Solid { color })` — one field, two setters, so
    /// unlike the document's `paint`/`paint_entry` pair (issue #63) the two
    /// cannot disagree.
    Fill(Color),
    /// The node's fill in the full v0.3 vocabulary: a gradient, or an image
    /// fill referencing an asset staged with [`Txn::add_image`].
    FillWith(PaintKind),
    /// The node's outline stroke: width, align, and solid color.
    Stroke(Stroke),
    /// Per-corner radii, in document units. They round the node's own
    /// fill and stroke, and — when the node clips — the clip box its
    /// descendants are clipped against.
    Corners {
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    },
    /// The node's drop and inner shadows, in paint order (v0.8, story
    /// #45, `docs/decisions/effects-vocabulary-shadows.md`). Replaces the
    /// whole list — an empty vec clears the node's shadows. Paint intent
    /// like fill/stroke/corners: a shadow depends only on the node's own
    /// box and corners, so commit copies it straight onto the paint-pool
    /// entry with no cross-node resolution (unlike a mask or group
    /// opacity).
    Shadows(Vec<Shadow>),
    /// The node's blurs (v0.11, story #393,
    /// `docs/decisions/backdrop-blur-is-core-vocabulary.md`). Replaces the
    /// whole list — an empty vec clears them. Paint intent like
    /// [`Prop::Shadows`]: a blur depends only on the node itself, so commit
    /// copies it straight onto the paint-pool entry with no cross-node
    /// resolution, even though a backdrop blur's *painting* does depend on
    /// what lies beneath. That dependency is the painter's contract at
    /// boundary B, not a resolution this arena performs (P1/P2).
    Blurs(Vec<Blur>),
    /// Fills stacked over the node's `Fill`/`FillWith` fill, bottom to top
    /// (story C1, debt #146). Replaces the whole list — an empty vec clears
    /// the node's stacked layers back to a single fill. Paint intent like
    /// `Shadows`: not variant-overridable, copied straight onto the
    /// paint-pool entry at commit with no cross-node resolution.
    ExtraFills(Vec<PaintKind>),
    /// The node's baked-vector coverage mask (story B1). Sets the resolved
    /// [`VectorField`] a Figma VECTOR node lowered into; the painter masks
    /// the node's fill by it. Paint intent like [`Prop::Shadows`] — commit
    /// copies it onto the paint-pool entry with no cross-node resolution.
    /// Set-only, no clear (the same gap as `Fill`), which the loader never
    /// needs (a document either carries a shape channel or it does not).
    ShapeField(VectorField),
    /// Whether the node clips its children to its own (rounded) box
    /// (`Paint.clip`, docs/design/architecture.md). Intent only: commit resolves it
    /// into the per-rect clip regions boundary B carries, because a flat
    /// rect table gives a painter no ancestors to walk (P2, issue #97).
    ///
    /// Unlike `Fill`, this prop clears: `Clip(false)` turns clipping
    /// back off (a bool has no absent state to lose).
    Clip(bool),
    /// Set/replace the node's text content (docs/design/dashbuf.md: strings, never
    /// glyph positions — P1). v0.5: no effect on committed output;
    /// text-driven hug sizing arrives with the measure-callback story.
    Text(String),
    /// Set/replace the node's text style.
    TextStyle(TextStyle),
    Mode(LayoutMode),
    Gap(f32),
    Padding {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    Margin {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    MainAlign(MainAxisAlign),
    CrossAlign(CrossAxisAlign),
    SizingH(AxisSizing),
    SizingV(AxisSizing),
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    /// The cross-axis gap (v0.8, story #43): wrap-line / grid-row
    /// spacing. Sets a value but cannot clear one back to
    /// follows-`gap`, the same gap as the min/max props.
    CrossGap(f32),
    /// The container's grid row tracks, top to bottom (v0.8, story
    /// #43). Replaces the whole list; meaningful when the mode is
    /// [`LayoutMode::Grid`].
    GridRows(Vec<GridTrack>),
    /// The container's grid column tracks, left to right.
    GridColumns(Vec<GridTrack>),
    /// The node's 0-based grid anchor row under a mode-`Grid` parent.
    GridRow(u16),
    /// The node's 0-based grid anchor column.
    GridColumn(u16),
    /// How many row tracks the node spans from its anchor (default 1).
    GridRowSpan(u16),
    /// How many column tracks the node spans from its anchor (default 1).
    GridColumnSpan(u16),
    /// Whether the node is drawn and takes part in layout. `false`
    /// lowers to Taffy `Display::None` (`docs/design/dashscene-engine.md`,
    /// issue #165): the node and its descendants are not drawn and take
    /// no space, so siblings reflow. Layout-affecting, like the rest of
    /// the flex vocabulary — ignored by `commit()`'s fixed resolution.
    /// Defaults to `true`.
    Visible(bool),
    /// Node/group alpha in `[0, 1]` (`Paint.opacity`, §23). Paint-only:
    /// it never reaches Taffy and triggers no solve
    /// (`docs/decisions/visible-is-layout-opacity-is-paint.md`). Commit
    /// resolves it into the per-rect free alpha boundary B carries and,
    /// for an overlapping subtree, a render-target group
    /// (`docs/decisions/masks-and-group-opacity.md`). Its pair
    /// [`Prop::Visible`] is the layout half. Defaults to `1.0`.
    ///
    /// Unlike `Fill`, this prop clears: a later `Opacity(1.0)` restores
    /// full opacity (a scalar has no absent state to lose).
    Opacity(f32),
    /// Whether this node is a mask: it stencils the siblings that follow
    /// it within the same parent (until the next mask sibling or the end
    /// of the parent) to its own (rounded) box, and draws nothing itself
    /// (`docs/decisions/masks-and-group-opacity.md`). Intent only: commit
    /// resolves it into the following siblings' clip regions, reusing the
    /// resolved-clip-region machinery (issue #97), because a flat rect
    /// table gives a painter no siblings to walk (P2).
    ///
    /// Like [`Prop::Clip`] this prop clears: `Mask(false)` turns masking
    /// back off.
    Mask(bool),
}

/// Horizontal text alignment within the node box — mirrors the `dashbuf`
/// `TextAlign` enum (story #310). `Left` is the default: the runtime flushes
/// an LTR paragraph left and an RTL one right by direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment of the text block within the node box — mirrors the
/// `dashbuf` `TextAlignV` enum (story #310). `Top` is the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlignV {
    #[default]
    Top,
    Center,
    Bottom,
}

/// Text style intent — mirrors the `dashbuf` `TextStyle` table
/// (family, em size in document units, CSS-scale weight, color, the four
/// v0.9 axes — a fixed line height, letter spacing, horizontal and vertical
/// alignment — and the v0.10 standard-ligatures-off bit) without linking the
/// generated code.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    /// Em size in document units.
    pub size: f32,
    /// CSS-scale weight, 100 to 900 inclusive.
    pub weight: u16,
    pub color: Color,
    /// A fixed line height in document units, or `None` for auto (the font's
    /// natural line advance). Story #310.
    pub line_height_px: Option<f32>,
    /// Letter spacing (tracking) in document units; zero is the default.
    pub letter_spacing: f32,
    /// Horizontal alignment; `Left` is the default.
    pub text_align: TextAlign,
    /// Vertical alignment within the box; `Top` is the default.
    pub text_align_v: TextAlignV,
    /// Standard ligatures forced off (story #341: Figma's OpenType
    /// `LIGA: 0`). `false` is the default.
    pub ligatures_off: bool,
}

/// One prop value a variant member can override — the slice of `Prop`'s
/// vocabulary the dashbuf variant table carries (X, Y, Width, Height, the
/// solid-fill shorthand, and visibility): the props needed to prove
/// resolved rect/paint correctness plus the topology change a switch that
/// hides or shows a child makes (story #283). Widening to the rest of
/// `Prop` is additive future work
/// (`docs/decisions/variant-set-flat-index.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VariantValue {
    X(f32),
    Y(f32),
    Width(f32),
    Height(f32),
    Fill(Color),
    /// Whether a variant member shows or hides the target node. `false`
    /// takes the child out of the laid-out set (Taffy `Display::None`) and
    /// its siblings reflow, exactly as [`Prop::Visible`] does through
    /// `set_prop` — the variant-driven "different child counts" topology
    /// change (story #283).
    Visible(bool),
}

/// One selectable state of a [`VariantSetId`]: an optional name and its
/// sparse overrides against the arena's base node values. Overrides
/// that name the same node's same prop more than once are legal; the
/// last one in the list wins (`Vec` order, not a map), the same
/// last-write convention `Txn::set_prop` already carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VariantMember {
    pub name: Option<String>,
    pub overrides: Vec<(NodeId, VariantValue)>,
}

/// Stable handle to a variant set in one [`Arena`], returned by
/// [`Txn::add_variant_set`]. Only meaningful for the arena that
/// produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VariantSetId(u32);

/// A staged variant set: its fixed member list and which one is active.
#[derive(Debug)]
struct VariantSetData {
    members: Vec<VariantMember>,
    active: usize,
}

/// The variant overrides active for one node, gathered across every
/// variant set in creation order (a later set's override for the same
/// prop wins — see [`Arena::overlay`]).
#[derive(Clone, Debug, Default)]
struct NodeOverlay {
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    fill: Option<PaintKind>,
    /// `Some(false)` hides the node (and its subtree) — the same effect as
    /// [`Prop::Visible(false)`](Prop::Visible), reached through a variant
    /// switch (story #283). `None` = the member does not override
    /// visibility, so the node's base `layout.visible` stands.
    visible: Option<bool>,
}

/// Intent for one node — layout intent plus paint and text intent and
/// tree links.
#[derive(Debug)]
struct NodeData {
    name: Option<String>,
    parent: Option<NodeId>,
    /// Creation order; DFS child order at commit.
    children: Vec<NodeId>,
    layout: Layout,
    /// The container's grid track lists (v0.8, story #43). Beside
    /// `layout` rather than inside it — they are variable-length, and
    /// `Layout` is `Copy` — the same split as `text`. Empty = no tracks
    /// authored (implicit auto tracks under mode `Grid`).
    grid_rows: Vec<GridTrack>,
    grid_columns: Vec<GridTrack>,
    /// The node's fill, in the full boundary-B vocabulary. `Prop::Fill`
    /// stays as the solid shorthand v0.1 producers use; `Prop::FillWith`
    /// stages a gradient or an image fill.
    fill: Option<PaintKind>,
    /// Fills stacked over `fill`, bottom to top (story C1, debt #146). Beside
    /// the scalar paint fields rather than inside them — it is
    /// variable-length — the same split as `shadows`. Empty = a single fill
    /// (or no fill), not variant-overridable (the variant vocabulary is
    /// X/Y/W/H/Fill).
    extra_fills: Vec<PaintKind>,
    stroke: Option<Stroke>,
    corners: CornerRadii,
    /// The node's drop and inner shadows, in paint order (v0.8, story
    /// #45). Beside the scalar paint fields rather than inside them — it
    /// is variable-length — the same split as `text`. Empty = no shadows.
    shadows: Vec<Shadow>,
    /// The node's blurs (v0.11, story #393). Empty = no blur — the same
    /// variable-length split as `shadows`.
    blurs: Vec<Blur>,
    /// The node's baked-vector coverage mask (story B1). `Some` for a Figma
    /// VECTOR node: the fill is masked by the resolved field. `None` (the
    /// default) is the implicit parametric shape. Resolved at load
    /// (`load::load_document`) from the document's vector pools, then copied
    /// straight onto the paint-pool entry at commit — paint intent like
    /// fill/stroke, with no cross-node resolution.
    shape: Option<VectorField>,
    /// "This node clips its children to its own box" — intent, resolved
    /// at commit (issue #97).
    clip: bool,
    /// Node/group alpha in `[0, 1]`, default `1.0` — intent, resolved at
    /// commit into per-rect free alpha and render-target groups
    /// (`docs/decisions/masks-and-group-opacity.md`).
    opacity: f32,
    /// "This node masks the siblings that follow it" — intent, resolved
    /// at commit into those siblings' clip regions
    /// (`docs/decisions/masks-and-group-opacity.md`).
    mask: bool,
    text: Option<String>,
    text_style: Option<TextStyle>,
}

/// The semantic model: the node tree with layout + paint intent, and
/// the double-buffered committed output painters read.
#[derive(Debug, Default)]
pub struct Arena {
    nodes: Vec<NodeData>,
    /// The image assets `PaintKind::Image` fills reference by index.
    ///
    /// The arena owns them because a `.dsb` names them (`Document.assets`, whose
    /// payloads the loader binds from the file's blob sections),
    /// so a loaded scene must be self-contained — a painter is handed
    /// `scene.images()` alongside `scene.paints()` and `scene.clips()`, and
    /// there is nowhere else for a loaded document's assets to live.
    ///
    /// Behind an `Arc` because every commit hands the table to the committed
    /// buffer, and the buffer is double-buffered: cloning it would memcpy
    /// every asset's encoded bytes twice per frame, on the path R-T4 budgets
    /// at "dirty-range upload + submit, nothing else". Assets are immutable
    /// once staged, so the commit takes a refcount rather than a copy;
    /// `add_image` pays the copy instead, and only when the table is actually
    /// shared.
    images: Arc<ImageTable>,
    /// Creation order; DFS root order at commit.
    roots: Vec<NodeId>,
    /// Declared by [`Txn::add_variant_set`], in creation order — the
    /// order a later set's override wins ties in
    /// (`docs/decisions/variant-set-flat-index.md`).
    variant_sets: Vec<VariantSetData>,
    /// Signal declarations, in declaration order (story #167). Intent
    /// metadata: commit never reads them — flushing a signal value
    /// through a binding is a producer-side runtime's job (P3).
    signals: Vec<SignalDecl>,
    /// Binding rows, in declaration order (story #167). Intent metadata,
    /// like `signals`.
    bindings: Vec<Binding>,
    buffers: [CommittedScene; 2],
    front: usize,
    /// Retained paint interner: a paint entry's canonical bits → the
    /// stable table index it was first assigned. Kept across commits
    /// (issue #164) so an unchanged entry keeps its index and the dirty
    /// check is a bit compare. The table lives in the committed buffers;
    /// this records only the key→index assignment. A changed entry earns
    /// a new index and the old one stays, so the table grows with the
    /// distinct entries seen; a commit that finds most of the table
    /// unreachable rebuilds it and re-keys this map onto the new indices
    /// (issue #197, [`compact_paints`]).
    paint_map: FxHashMap<PaintKey, PaintIndex>,
    /// Retained clip interner, the clip-region analogue of `paint_map`.
    clip_map: FxHashMap<ClipKey, ClipIndex>,
    /// Nodes whose layout-affecting intent changed since the last commit.
    /// The retained solver reads this (via [`Arena::layout_dirty`]) to
    /// mark exactly those nodes dirty in its tree; drained each commit.
    /// May carry duplicates — consumers dedup.
    layout_dirty: Vec<NodeId>,
    /// Nodes whose paint intent (fill, stroke, corners) or clip flag
    /// changed since the last commit — what commit re-interns. Drained
    /// each commit; may carry duplicates.
    paint_dirty: Vec<NodeId>,
    /// Nodes whose clip flag toggled since the last commit. A toggle
    /// changes whether the node contributes a clip box to its subtree
    /// even when its own geometry did not move. Drained each commit.
    clip_toggled: Vec<NodeId>,
    /// Nodes whose mask flag toggled since the last commit. A toggle
    /// changes whether the node stencils its following siblings even when
    /// its own geometry did not move, so those siblings must re-resolve
    /// their clip regions. Drained each commit
    /// (`docs/decisions/masks-and-group-opacity.md`).
    mask_toggled: Vec<NodeId>,
    /// Nodes whose visibility toggled since the last commit. A hidden node
    /// (and its subtree) resolves to the draws-nothing paint entry under
    /// the fixed solver, and stops masking if it was a mask, so a toggle
    /// re-interns the affected subtree's paint even when no geometry moved
    /// (`docs/decisions/masks-and-group-opacity.md`, story #44 M5). Drained
    /// each commit.
    visible_toggled: Vec<NodeId>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin staging mutations. The returned [`Txn`] holds the arena's
    /// mutable borrow, so committed output cannot be read (and no
    /// second stage can open) until it commits or drops.
    pub fn open(&mut self) -> Txn<'_> {
        Txn { arena: self }
    }

    /// The front committed buffer — the painter input (boundary B).
    /// Generation 0 and empty before the first commit.
    pub fn committed(&self) -> &CommittedScene {
        &self.buffers[self.front]
    }

    /// The node's authored name, if any (a diagnostics aid).
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena. A `NodeId` from
    /// another arena whose index happens to be in range is not detected
    /// — ids are only meaningful for the arena that produced them.
    pub fn name(&self, node: NodeId) -> Option<&str> {
        self.node_data(node).name.as_deref()
    }

    /// The node's text content, or `None` for a node without text.
    ///
    /// Reads the intent model: staged (uncommitted) values are visible
    /// immediately, unlike [`Arena::committed`]. This is the seam the
    /// typeset pipeline and the measure callback read from.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena (same contract
    /// as [`Arena::name`]).
    pub fn text(&self, node: NodeId) -> Option<&str> {
        self.node_data(node).text.as_deref()
    }

    /// The node's text style, or `None` when unstyled. Intent-side,
    /// like [`Arena::text`].
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn text_style(&self, node: NodeId) -> Option<&TextStyle> {
        self.node_data(node).text_style.as_ref()
    }

    /// The node's layout intent (authored fixed geometry + flex
    /// vocabulary), by value — with any active variant override
    /// (`X`/`Y`/`Width`/`Height`/`Visible`) applied on top of the base
    /// value. This
    /// is the read seam every [`LayoutSolver`] resolves geometry
    /// through (the internal [`FixedSolver`] included), so a variant
    /// switch reaches committed geometry without either solver knowing
    /// variants exist (`docs/decisions/variant-set-flat-index.md`).
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena. A `NodeId` from
    /// another arena whose index happens to be in range is not detected
    /// — ids are only meaningful for the arena that produced them.
    pub fn layout(&self, node: NodeId) -> Layout {
        let mut layout = self.base_layout(node);
        let overlay = self.overlay(node);
        if let Some(x) = overlay.x {
            layout.x = x;
        }
        if let Some(y) = overlay.y {
            layout.y = y;
        }
        if let Some(width) = overlay.width {
            layout.width = width;
        }
        if let Some(height) = overlay.height {
            layout.height = height;
        }
        if let Some(visible) = overlay.visible {
            layout.visible = visible;
        }
        layout
    }

    /// The node's authored layout intent with **no** variant overlay
    /// applied — the base value [`Arena::layout`] folds the active
    /// variant's `X`/`Y`/`Width`/`Height`/`Visible` override on top of
    /// (issue #185).
    ///
    /// Read this only when the base authored geometry is the thing
    /// wanted independently of variant state: a `.dsb` re-exporter
    /// diffing an override against the base value, an inspector, or a
    /// test pinning authored geometry. Every consumer that resolves
    /// geometry — the solvers included — reads [`Arena::layout`]
    /// instead, so that a variant switch reaches committed geometry
    /// (`docs/decisions/variant-set-flat-index.md`).
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena (same contract
    /// as [`Arena::layout`]).
    pub fn base_layout(&self, node: NodeId) -> Layout {
        self.node_data(node).layout
    }

    /// The node's grid track lists (v0.8, story #43) — row tracks top
    /// to bottom, column tracks left to right. Beside [`Arena::layout`]
    /// rather than inside it because the lists are variable-length and
    /// `Layout` is `Copy` (the same split as [`Arena::text`]). Both
    /// empty for a node that authored no tracks — under a mode-`Grid`
    /// container that means implicit auto tracks.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn grid_tracks(&self, node: NodeId) -> (&[GridTrack], &[GridTrack]) {
        let data = self.node_data(node);
        (&data.grid_rows, &data.grid_columns)
    }

    /// The currently active member index of `set` — staged intent, like
    /// [`Arena::text`]: a member switched by [`Txn::set_variant`] but
    /// not yet committed still reads back here.
    ///
    /// # Panics
    ///
    /// Panics if `set` is out of range for this arena.
    pub fn active_variant(&self, set: VariantSetId) -> usize {
        self.variant_sets
            .get(set.0 as usize)
            .unwrap_or_else(|| panic!("{set:?} is not a variant set of this arena"))
            .active
    }

    /// The variant overrides active for `node`, gathered across every
    /// variant set's active member, in set-creation order — a later
    /// set's override of the same prop on the same node wins. Empty for
    /// a node no variant set touches.
    ///
    /// O(total override entries across all sets): the "walking
    /// skeleton" scale this story targets has no need for a
    /// per-node index, and building one on every read would trade a
    /// simple scan for cache invalidation this API's staged-visibility
    /// contract (immediate, on every read) does not otherwise need.
    fn overlay(&self, node: NodeId) -> NodeOverlay {
        let mut overlay = NodeOverlay::default();
        for set in &self.variant_sets {
            let member = &set.members[set.active];
            for (target, value) in &member.overrides {
                if *target != node {
                    continue;
                }
                match *value {
                    VariantValue::X(v) => overlay.x = Some(v),
                    VariantValue::Y(v) => overlay.y = Some(v),
                    VariantValue::Width(v) => overlay.width = Some(v),
                    VariantValue::Height(v) => overlay.height = Some(v),
                    VariantValue::Fill(color) => {
                        overlay.fill = Some(PaintKind::Solid { color });
                    }
                    VariantValue::Visible(v) => overlay.visible = Some(v),
                }
            }
        }
        overlay
    }

    /// Root nodes in creation order (document DFS root order).
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// The node's parent, or `None` for a root. Intent-side, like
    /// [`Arena::children`] — the read seam a loader-side consumer (the
    /// reactive attach, story #167) derives tree structure from.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena (same contract
    /// as [`Arena::name`]).
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.node_data(node).parent
    }

    /// The node's fill intent, or `None` for an unfilled node.
    /// Intent-side, like [`Arena::text`]: staged values are visible
    /// immediately. The base value only — a variant override's fill is
    /// commit-time overlay, not base intent.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn fill(&self, node: NodeId) -> Option<&PaintKind> {
        self.node_data(node).fill.as_ref()
    }

    /// The node's opacity intent in `[0, 1]` (default `1.0`). Intent-side,
    /// like [`Arena::fill`]: a staged [`Prop::Opacity`] is visible
    /// immediately, before the next commit resolves it.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn opacity(&self, node: NodeId) -> f32 {
        self.node_data(node).opacity
    }

    /// Whether the node is a mask (stencils its following siblings).
    /// Intent-side, like [`Arena::fill`].
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn is_mask(&self, node: NodeId) -> bool {
        self.node_data(node).mask
    }

    /// The staged signal declarations, in declaration order — the table
    /// [`SignalId`] indexes (story #167).
    pub fn signals(&self) -> &[SignalDecl] {
        &self.signals
    }

    /// The staged binding rows, in declaration order (story #167).
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Total node count, roots and descendants alike. A [`LayoutSolver`]
    /// compares this to the node count of its retained tree to detect a
    /// structural change (v0.4 grows the tree by appending; it never
    /// removes), which forces a rebuild rather than an incremental solve
    /// (issue #164).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Nodes whose layout-affecting intent changed since the last commit,
    /// in the order the changes were staged (with possible duplicates).
    /// This is the seam a retained [`LayoutSolver`] reads to mark exactly
    /// those nodes dirty in its tree instead of re-solving the whole scene
    /// (issue #164). Empty right after a commit; a `set_prop` of a
    /// paint-only property adds nothing here, which is what lets a
    /// paint-only frame skip the solve entirely.
    pub fn layout_dirty(&self) -> &[NodeId] {
        &self.layout_dirty
    }

    /// A node's children in creation order (document DFS child
    /// order).
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena. A `NodeId` from
    /// another arena whose index happens to be in range is not detected
    /// — ids are only meaningful for the arena that produced them.
    pub fn children(&self, node: NodeId) -> &[NodeId] {
        &self.node_data(node).children
    }

    /// Document DFS order (the rect-table index order): roots in
    /// creation order, children in creation order under each parent,
    /// depth-first. The one traversal both the rect table and the
    /// solvers agree on — change it here or nowhere.
    fn dfs_order(&self) -> Vec<NodeId> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            order.push(id);
            stack.extend(self.nodes[id.index()].children.iter().rev());
        }
        order
    }

    fn node_data(&self, node: NodeId) -> &NodeData {
        self.nodes
            .get(node.index())
            .unwrap_or_else(|| panic!("{node:?} is not a node of this arena"))
    }
}

/// The v0.1 fixed-geometry resolution as a [`LayoutSolver`]: absolute
/// position = parent absolute + authored offset, size = authored
/// width/height. Flex intent is ignored — this is the mode-`None`
/// passthrough, and what [`Txn::commit`] uses.
struct FixedSolver;

impl LayoutSolver for FixedSolver {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        // dfs_order is parent-before-child, so every parent's absolute
        // is resolved before its children read it. Reading through
        // `arena.layout(id)` rather than the node's raw field means a
        // variant-overridden X/Y/Width/Height reaches this solve the
        // same way a `set_prop` one does (`docs/decisions/variant-set-flat-index.md`).
        let mut out = Vec::with_capacity(arena.nodes.len());
        let mut absolute = vec![(0.0f32, 0.0f32); arena.nodes.len()];
        for id in arena.dfs_order() {
            let parent = arena.nodes[id.index()].parent;
            let layout = arena.layout(id);
            let (parent_x, parent_y) = parent.map_or((0.0, 0.0), |p| absolute[p.index()]);
            let (x, y) = (parent_x + layout.x, parent_y + layout.y);
            absolute[id.index()] = (x, y);
            out.push((
                id,
                SolvedRect {
                    x,
                    y,
                    w: layout.width,
                    h: layout.height,
                },
            ));
        }
        out
    }
}

/// A staged mutation. Obtained from [`Arena::open`]; publishes via
/// [`commit`](Txn::commit).
#[derive(Debug)]
pub struct Txn<'a> {
    arena: &'a mut Arena,
}

impl Txn<'_> {
    /// Add a node under `parent` (or as a root). Siblings keep
    /// creation order in the document DFS order.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is out of range for this arena (a `NodeId`
    /// from another arena whose index happens to be in range is not
    /// detected), or if the arena already holds `u32::MAX` nodes —
    /// node ids stay below the `u32::MAX` sentinel (`dashbuf`'s
    /// `NO_PARENT`), and every paint index stays representable (the
    /// retained interner lets the paint table outgrow the node count
    /// between rebuilds, but a rebuild brings it back to at most one
    /// entry per rect, so it stays a small multiple of the node count —
    /// issue #197).
    pub fn add_node(&mut self, parent: Option<NodeId>, name: Option<&str>) -> NodeId {
        if let Some(p) = parent {
            assert!(
                p.index() < self.arena.nodes.len(),
                "parent {p:?} is not a node of this arena"
            );
        }
        // This guard is the single point where the node count grows, so
        // every id, DFS index, and paint index stays < u32::MAX and the
        // plain `as u32` casts in `commit` cannot truncate.
        assert!(
            self.arena.nodes.len() < u32::MAX as usize,
            "arena is full: u32::MAX is reserved as a sentinel"
        );
        let id = NodeId(self.arena.nodes.len() as u32);
        self.arena.nodes.push(NodeData {
            name: name.map(String::from),
            parent,
            children: Vec::new(),
            layout: Layout::default(),
            grid_rows: Vec::new(),
            grid_columns: Vec::new(),
            fill: None,
            extra_fills: Vec::new(),
            stroke: None,
            corners: CornerRadii::default(),
            shadows: Vec::new(),
            blurs: Vec::new(),
            shape: None,
            clip: false,
            opacity: 1.0,
            mask: false,
            text: None,
            text_style: None,
        });
        match parent {
            Some(p) => self.arena.nodes[p.index()].children.push(id),
            None => self.arena.roots.push(id),
        }
        id
    }

    /// Set one property on a node.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena. A `NodeId` from
    /// another arena whose index happens to be in range is not detected
    /// — ids are only meaningful for the arena that produced them.
    /// Stages an image asset and returns its index — the value a
    /// [`PaintKind::Image`] fill references.
    ///
    /// Assets are append-only within an arena and are not deduplicated here:
    /// a document's asset table arrives already deduplicated by its producer
    /// (the content-addressed asset model is v0.7, issue #107).
    pub fn add_image(&mut self, asset: ImageAsset) -> u32 {
        Arc::make_mut(&mut self.arena.images).push(asset)
    }

    /// Declare a variant set: a fixed list of named, sparsely-overriding
    /// members (`docs/decisions/variant-set-flat-index.md`). The first
    /// member (index 0) is active until [`Txn::set_variant`] switches
    /// it.
    ///
    /// # Panics
    ///
    /// Panics if `members` is empty (there is nothing to select), or if
    /// any override names a `NodeId` out of range for this arena.
    pub fn add_variant_set(&mut self, members: Vec<VariantMember>) -> VariantSetId {
        assert!(
            !members.is_empty(),
            "a variant set needs at least one member"
        );
        for member in &members {
            for (node, _) in &member.overrides {
                // The bounds check and its message live in `node_data`,
                // which every other node read goes through (issue #186).
                let _ = self.arena.node_data(*node);
            }
        }
        // Cannot truncate: add_node's guard already keeps the arena
        // below u32::MAX nodes, and a variant set needs at least one
        // node to be useful, so variant sets never outnumber nodes.
        let id = VariantSetId(self.arena.variant_sets.len() as u32);
        self.arena
            .variant_sets
            .push(VariantSetData { members, active: 0 });
        id
    }

    /// Switch `set`'s active member. Staged like any other mutation
    /// (P3): visible immediately through [`Arena::layout`] and
    /// [`Arena::active_variant`], published to painters at the next
    /// commit.
    ///
    /// # Panics
    ///
    /// Panics if `set` is out of range for this arena, or `member` is
    /// out of range for `set`'s member list.
    pub fn set_variant(&mut self, set: VariantSetId, member: usize) {
        let data = self
            .arena
            .variant_sets
            .get_mut(set.0 as usize)
            .unwrap_or_else(|| panic!("{set:?} is not a variant set of this arena"));
        assert!(
            member < data.members.len(),
            "member {member} is out of range for {set:?} ({} members)",
            data.members.len()
        );
        data.active = member;
        // Switching the active member changes the effective value of every
        // node this set overrides — both the ones the old member touched
        // (reverting) and the ones the new member touches (applying). Mark
        // them all layout- and paint-dirty (a variant override carries
        // geometry and fill, docs/decisions/variant-set-flat-index.md), so
        // the next commit re-solves and re-interns exactly those nodes
        // (issue #164). A Visible override additionally toggles the node's
        // draws-nothing state, so its subtree re-interns paint through the
        // hidden_changed cascade — mark it visible_toggled too (story #283).
        // Bounded by the set's override count.
        let targets: Vec<(NodeId, bool)> = self.arena.variant_sets[set.0 as usize]
            .members
            .iter()
            .flat_map(|m| {
                m.overrides
                    .iter()
                    .map(|(node, value)| (*node, matches!(value, VariantValue::Visible(_))))
            })
            .collect();
        for (node, toggles_visibility) in targets {
            self.arena.layout_dirty.push(node);
            self.arena.paint_dirty.push(node);
            if toggles_visibility {
                self.arena.visible_toggled.push(node);
            }
        }
    }

    pub fn set_prop(&mut self, node: NodeId, prop: Prop) {
        // Classify the change before consuming `prop`, so the next commit
        // re-does only the affected work (issue #164): a layout-affecting
        // prop feeds the retained solver's dirty set, a paint prop feeds
        // commit's paint re-intern, and a clip-flag change feeds the
        // clip-region cascade. A prop that changes neither geometry nor
        // paint is not recorded, which is what lets a paint-only frame
        // skip the layout solve. Recorded after the node is validated
        // below, never on the panic path.
        let class = prop_class(&prop);
        let data = self
            .arena
            .nodes
            .get_mut(node.index())
            .unwrap_or_else(|| panic!("{node:?} is not a node of this arena"));
        match prop {
            Prop::X(v) => data.layout.x = v,
            Prop::Y(v) => data.layout.y = v,
            Prop::Width(v) => data.layout.width = v,
            Prop::Height(v) => data.layout.height = v,
            Prop::Fill(c) => data.fill = Some(PaintKind::Solid { color: c }),
            Prop::FillWith(kind) => data.fill = Some(kind),
            Prop::Stroke(s) => data.stroke = Some(s),
            Prop::Corners {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            } => {
                data.corners = CornerRadii {
                    top_left,
                    top_right,
                    bottom_right,
                    bottom_left,
                }
            }
            Prop::Shadows(shadows) => data.shadows = shadows,
            Prop::Blurs(blurs) => data.blurs = blurs,
            Prop::ExtraFills(fills) => data.extra_fills = fills,
            Prop::ShapeField(field) => data.shape = Some(field),
            Prop::Clip(v) => data.clip = v,
            Prop::Text(s) => data.text = Some(s),
            Prop::TextStyle(ts) => data.text_style = Some(ts),
            Prop::Mode(m) => data.layout.mode = m,
            Prop::Gap(v) => data.layout.gap = v,
            Prop::Padding {
                left,
                top,
                right,
                bottom,
            } => {
                data.layout.padding = EdgeInsets {
                    left,
                    top,
                    right,
                    bottom,
                }
            }
            Prop::Margin {
                left,
                top,
                right,
                bottom,
            } => {
                data.layout.margin = EdgeInsets {
                    left,
                    top,
                    right,
                    bottom,
                }
            }
            Prop::MainAlign(a) => data.layout.main_align = a,
            Prop::CrossAlign(a) => data.layout.cross_align = a,
            Prop::SizingH(v) => data.layout.sizing_h = v,
            Prop::SizingV(v) => data.layout.sizing_v = v,
            Prop::MinWidth(v) => data.layout.min_width = Some(v),
            Prop::MaxWidth(v) => data.layout.max_width = Some(v),
            Prop::MinHeight(v) => data.layout.min_height = Some(v),
            Prop::MaxHeight(v) => data.layout.max_height = Some(v),
            Prop::CrossGap(v) => data.layout.cross_gap = Some(v),
            Prop::GridRows(tracks) => data.grid_rows = tracks,
            Prop::GridColumns(tracks) => data.grid_columns = tracks,
            Prop::GridRow(v) => data.layout.grid_row = Some(v),
            Prop::GridColumn(v) => data.layout.grid_column = Some(v),
            Prop::GridRowSpan(v) => data.layout.grid_row_span = v,
            Prop::GridColumnSpan(v) => data.layout.grid_column_span = v,
            Prop::Visible(v) => data.layout.visible = v,
            // A non-finite opacity is a producer error the painter cannot
            // honor. Refuse it by name rather than clamp `NaN` to a
            // value that reads back as fully opaque (story #44 M7).
            Prop::Opacity(v) => {
                assert!(
                    v.is_finite(),
                    "{node:?}: Prop::Opacity({v}) is not finite; opacity must be in [0, 1]"
                );
                data.opacity = v.clamp(0.0, 1.0);
            }
            Prop::Mask(v) => data.mask = v,
        }
        // `data`'s borrow ends with the match above.
        match class {
            PropClass::Layout => self.arena.layout_dirty.push(node),
            PropClass::Paint => self.arena.paint_dirty.push(node),
            PropClass::ClipFlag => self.arena.clip_toggled.push(node),
            // A mask toggle changes both the node's own paint (it now
            // draws nothing, or paints again) and its following siblings'
            // clip regions, so it feeds both cascades.
            PropClass::MaskFlag => {
                self.arena.paint_dirty.push(node);
                self.arena.mask_toggled.push(node);
            }
            // A visibility toggle is layout-affecting (the solver hides the
            // node) and also re-interns the node's — and its subtree's —
            // paint under the fixed solver, which does not hide it (M5).
            PropClass::VisibleFlag => {
                self.arena.layout_dirty.push(node);
                self.arena.visible_toggled.push(node);
            }
            // Opacity is recomputed from `node.opacity` on every commit's
            // walk, so it needs no change log — the walk always picks up
            // the staged value, and the rect entry's alpha bits carry the
            // change into the dirty set.
            PropClass::OpacityOnly => {}
        }
    }

    /// Declare a signal: an optional runtime lookup name and the initial
    /// value its bindings seed from (story #167). Declarations are
    /// append-only, like nodes.
    ///
    /// # Panics
    ///
    /// Panics if the arena already holds `u32::MAX` signal declarations
    /// (the same sentinel headroom rule as [`Txn::add_node`]).
    pub fn declare_signal(&mut self, name: Option<&str>, initial: f32) -> SignalId {
        assert!(
            self.arena.signals.len() < u32::MAX as usize,
            "arena is full: u32::MAX signal declarations"
        );
        let id = SignalId(self.arena.signals.len() as u32);
        self.arena.signals.push(SignalDecl {
            name: name.map(String::from),
            initial,
        });
        id
    }

    /// Bind one channel of one node to a declared signal through a
    /// declarative transform (story #167). Rows are append-only; two rows
    /// on the same `(node, channel)` are legal and both flush, last
    /// writer wins — the same last-write convention as `set_prop`.
    ///
    /// Intent metadata only: `commit` never reads the table. The commit
    /// that publishes a signal's value is the producer-side flush that
    /// calls `set_prop` (P3).
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a node of this arena or `signal` is not a
    /// declaration of this arena — a broken producer contract, named
    /// loudly (P4), matching [`Txn::add_variant_set`].
    pub fn bind(
        &mut self,
        node: NodeId,
        channel: Channel,
        signal: SignalId,
        transform: ScalarTransform,
    ) {
        assert!(
            node.index() < self.arena.nodes.len(),
            "{node:?} is not a node of this arena"
        );
        assert!(
            signal.index() < self.arena.signals.len(),
            "{signal:?} is not a signal declaration of this arena"
        );
        self.arena.bindings.push(Binding {
            signal,
            node,
            channel,
            transform,
        });
    }

    /// Lower every negative flex gap to child margins (the Figma≠CSS
    /// lowering, docs/design/dashbuf.md).
    ///
    /// Figma auto-layout allows a negative item spacing so children
    /// overlap; CSS/Taffy `gap` cannot go negative. For each container
    /// node whose mode is `Horizontal`/`Vertical` and whose `gap` is
    /// negative, this sets the `gap` to `0` and adds the gap to the
    /// leading main-axis margin of every child after the first
    /// (`margin.left` for `Horizontal`, `margin.top` for `Vertical`) —
    /// the same overlap, expressed in vocabulary the solver accepts.
    /// Positive and zero gaps are untouched; the pass adds to an
    /// existing child margin rather than replacing it, and is
    /// idempotent (after it runs no negative gaps remain).
    ///
    /// A shared producer step: the DSL/commit path calls it before
    /// committing, and the Figma importer (`dashc`) reuses it. It
    /// stages like any other mutation — the rewrite publishes with the
    /// next commit (P3).
    ///
    /// # Panics
    ///
    /// Panics on a `Wrap` container with a negative gap. The margin
    /// rewrite is only gap-equivalent for a child that follows another
    /// child on the same line, and wrap decides its line breaks after
    /// the lowering — a lowered wrap scene pulls every later line's
    /// leading child into the padding band and distorts the breaks.
    /// There is no margin encoding of a negative wrap gap, so the
    /// construct is refused by name (P4), never lowered wrong
    /// (story #43, review finding R4).
    pub fn lower_negative_gaps(&mut self) {
        let mut dirtied: Vec<NodeId> = Vec::new();
        let nodes = &mut self.arena.nodes;
        for i in 0..nodes.len() {
            // Only genuinely-negative gaps lower. `partial_cmp` returns
            // `None` for NaN, so a NaN gap is skipped, never treated as
            // negative and never sprayed into child margins.
            if nodes[i].layout.gap.partial_cmp(&0.0) != Some(Ordering::Less) {
                continue;
            }
            let horizontal = match nodes[i].layout.mode {
                LayoutMode::Horizontal => true,
                LayoutMode::Vertical => false,
                // A margin is only gap-equivalent for a child that
                // follows another child on the same line, and wrap
                // breaks its lines after the lowering — refused by
                // name, never lowered wrong (P4; see the method docs).
                LayoutMode::Wrap => panic!(
                    "negative gap on a Wrap container has no margin lowering \
                     (line breaks are decided after the lowering); the \
                     construct is refused (story #43, P4)"
                ),
                // A mode-None container ignores gap entirely; nothing
                // to lower. Grid gaps are track spacing, not flex-flow
                // spacing — a leading margin would shift cell content,
                // not overlap tracks — so they do not lower here.
                LayoutMode::None | LayoutMode::Grid => continue,
            };
            let gap = nodes[i].layout.gap;
            nodes[i].layout.gap = 0.0;
            dirtied.push(NodeId(i as u32));
            // Every child after the first (in main-axis order) gains
            // the negative gap as a leading margin. `NodeId` is `Copy`,
            // so indexing by position ends the read borrow before the
            // mutable one — no children clone needed.
            for k in 1..nodes[i].children.len() {
                let child = nodes[i].children[k];
                let margin = &mut nodes[child.index()].layout.margin;
                if horizontal {
                    margin.left += gap;
                } else {
                    margin.top += gap;
                }
                dirtied.push(child);
            }
        }
        // The rewrite changes gap and child margins — both layout intent,
        // so the next commit re-solves these nodes (issue #164). Pushed
        // after the `nodes` borrow above ends.
        self.arena.layout_dirty.extend(dirtied);
    }

    /// Resolve the intent model into the back buffer, flip the double
    /// buffer, and return the new generation — using core's internal
    /// fixed-geometry resolution (authored offset + fixed size; flex
    /// intent ignored). Product code with flex layout commits through
    /// [`commit_with`](Txn::commit_with) and a real solver.
    pub fn commit(self) -> u64 {
        self.commit_with(&mut FixedSolver)
    }

    /// Resolve the intent model into the back buffer using `solver`,
    /// flip the double buffer, and return the new generation.
    ///
    /// The commit is incremental: it carries the previous buffer forward
    /// and re-does only the work the change forced (issue #164).
    ///
    /// - Geometry comes from the solver. A solver may report only the
    ///   nodes that moved or resized; every other node keeps the rect it
    ///   resolved to last commit, found by [`NodeId`] so a structural
    ///   shift still finds it.
    /// - Paints and clip regions intern through the arena's *retained*
    ///   interners, so an unchanged entry keeps its index across commits
    ///   and only a changed entry earns a new one. A node whose paint
    ///   intent did not change reuses its previous index outright; a node
    ///   whose clip context (an ancestor's box or clip flag) did not
    ///   change reuses its previous clip index.
    /// - The back buffer starts as the front buffer patched at the
    ///   changed indices: the rect table is rebuilt entry by entry, but
    ///   an unchanged node's entry is copied from the previous commit, and
    ///   the paint/clip tables and the two index maps are shared by
    ///   reference until a genuinely new entry (or a structural change)
    ///   forces a copy.
    ///
    /// Because interned indices are stable, a rect is dirty exactly when
    /// its entry bits (the bits a painter uploads, R-T4) differ from the
    /// previous commit at the same index — a fill change earns a new paint
    /// index, a clip-box change earns a new clip index, and either shows
    /// up in the bit compare. Fully deterministic given a deterministic
    /// solver (R7).
    ///
    /// # Panics
    ///
    /// Panics if the solver returns a rect for a node that is not this
    /// arena's, or two rects for one node (P4). Panics if a node has no
    /// rect at all — neither returned by this solve nor resolved by a
    /// previous commit (the re-expressed "every node has a rect"
    /// invariant: a missing rect is a broken contract, never a silent
    /// default).
    pub fn commit_with(self, solver: &mut dyn LayoutSolver) -> u64 {
        // Borrow the retained interners for the walk through a guard that
        // puts them back however the walk ends (issue #196). They are
        // taken out of the arena so the arena stays immutably borrowable
        // (for `overlay`) while the maps are mutated.
        let mut guard = InternerGuard::open(self.arena);
        let InternerGuard {
            arena,
            paint_map,
            clip_map,
            paints_at_open,
            clips_at_open,
            ..
        } = &mut guard;
        // Reborrow the guard's arena field as the plain `&mut Arena` the
        // rest of the walk uses. Borrowing the maps and the arena as
        // separate fields of the guard is what keeps them usable at once.
        let arena: &mut Arena = arena;

        // DFS document order (rect-table index order).
        let order = arena.dfs_order();

        // Geometry from the solver, keyed by arena slot. Malformed
        // solver output is a broken contract, named loudly (P4):
        // duplicates and foreign ids never commit silently.
        let mut solved: Vec<Option<SolvedRect>> = vec![None; arena.nodes.len()];
        for (id, rect) in solver.solve(arena) {
            let slot = solved.get_mut(id.index()).unwrap_or_else(|| {
                panic!("solver returned a rect for {id:?}, which is not a node of this arena")
            });
            assert!(
                slot.replace(rect).is_none(),
                "solver returned two rects for {id:?}"
            );
        }
        // Carry forward the previous commit's rect for every node the
        // solver did not report — by NodeId, so a structural change that
        // shifted the DFS index still finds the right previous rect. A
        // node that is neither solved now nor present in a previous commit
        // stays `None` and trips the invariant below.
        {
            let previous = &arena.buffers[arena.front];
            for (slot, geom) in solved.iter_mut().enumerate() {
                if geom.is_none()
                    && let Some(&ri) = previous.rect_index.get(slot)
                {
                    let r = previous.rects[ri as usize];
                    *geom = Some(SolvedRect {
                        x: r.x,
                        y: r.y,
                        w: r.w,
                        h: r.h,
                    });
                }
            }
        }

        // Which nodes changed paint intent / toggled their clip flag this
        // commit — O(1) lookup for the walk below.
        let paint_dirty_set: FxHashSet<usize> =
            arena.paint_dirty.iter().map(|n| n.index()).collect();
        let clip_toggled_set: FxHashSet<usize> =
            arena.clip_toggled.iter().map(|n| n.index()).collect();
        let mask_toggled_set: FxHashSet<usize> =
            arena.mask_toggled.iter().map(|n| n.index()).collect();

        let previous = &arena.buffers[arena.front];
        // A node was added since the previous commit iff the node count
        // grew (v0.4 never removes). A structural change re-indexes the
        // rect table, so the previous index maps cannot be reused.
        let structural = order.len() != previous.node_ids.len();

        // The paint and clip tables start shared with the previous
        // commit; `intern_*` copies-on-write only when a new entry is
        // pushed. `region_out_index`/`region_out_changed` carry, per node,
        // the clip region it hands its children and whether that region
        // changed — the parent-before-child DFS lets a child read them.
        let mut back_paints = Arc::clone(&previous.paints);
        let mut back_clips = Arc::clone(&previous.clips);
        let mut rects: Vec<RectEntry> = Vec::with_capacity(order.len());
        let n = arena.nodes.len();
        // `None` until the node is visited, so a child that reads its
        // parent's region before the parent resolved it fails by name
        // rather than reading a silent `UNCLIPPED` (issue #198).
        let mut region_out_index: Vec<Option<ClipIndex>> = vec![None; n];
        let mut region_out_changed: Vec<bool> = vec![false; n];
        let visible_toggled_set: FxHashSet<usize> =
            arena.visible_toggled.iter().map(|n| n.index()).collect();

        // Mask resolution (`docs/decisions/masks-and-group-opacity.md`).
        // `mask_region[parent slot]` is the region a mask child hands the
        // siblings that follow it: the parent's own region with every mask
        // child seen so far chained on (successive masks intersect, not
        // replace — M3). `None` = no mask child yet, so a following sibling
        // reads the parent's `region_out` directly. `mask_changed` marks
        // that the masking a following sibling receives changed this commit
        // (a mask added, removed, moved, or made in/visible), so the
        // sibling re-resolves even in the mask-off direction (M1). Keyed by
        // parent slot; the DFS never writes a parent's slot between its
        // children, so the value a later sibling reads is stable.
        let mut mask_region: Vec<Option<ClipIndex>> = vec![None; n];
        let mut mask_changed: Vec<bool> = vec![false; n];
        // `eff_hidden[slot]` is whether the node, or any ancestor, is
        // `Visible(false)`. A hidden subtree draws nothing under the fixed
        // solver (which does not hide it), so its rects resolve to the
        // draws-nothing entry (M5). `hidden_changed[slot]` propagates a
        // visibility toggle down the subtree so a descendant re-interns its
        // paint. Parent-before-child DFS fills both.
        let mut eff_hidden: Vec<bool> = vec![false; n];
        let mut hidden_changed: Vec<bool> = vec![false; n];
        // Per rect index: slot lookup, and the painted extent of the rect
        // (its box grown by any stroke outset) for the overlap test — `None`
        // when the rect paints nothing (M10). The exclusive subtree end is
        // filled post-order below.
        let mut rect_of_slot: Vec<u32> = vec![0; n];
        let mut painted_extent: Vec<Option<Extent>> = vec![None; order.len()];

        for (i, &id) in order.iter().enumerate() {
            rect_of_slot[id.index()] = i as u32;
            let node = &arena.nodes[id.index()];
            // Effective visibility folds the active variant override on top
            // of the base field, the same overlay-on-read the geometry and
            // fill use (story #283): a member that sets Visible(false) hides
            // the node here too, not only through the TaffySolver, so the
            // fixed-solver commit resolves it to draws-nothing (M5) and stops
            // it masking.
            let node_visible = arena.overlay(id).visible.unwrap_or(node.layout.visible);
            let geometry = solved[id.index()].unwrap_or_else(|| {
                panic!(
                    "no rect for {id:?}: the solver did not resolve it and no previous commit did \
                     either (P4)"
                )
            });
            // The node's entry from the previous commit, by NodeId. `None`
            // for a node added since (it has no previous rect).
            let prev_entry: Option<RectEntry> = previous
                .rect_index
                .get(id.index())
                .map(|&ri| previous.rects[ri as usize]);

            // The region reaching this node is the one its parent's masks
            // hand it: the parent's own `region_out` with every earlier
            // mask child of the parent chained on (M3), or the parent's
            // `region_out` directly when no mask precedes this node. A root
            // is unclipped. (A clipping or masking node does not clip
            // itself, only its descendants / following siblings.)
            let (region_in_index, region_in_changed) = match node.parent {
                Some(parent) => {
                    // Read the parent's outgoing region through the guard
                    // in both arms, so an order violation is caught even
                    // when a mask supplies the region actually used
                    // (issue #198).
                    let parent_out = parent_region_out(&region_out_index, parent);
                    let changed =
                        region_out_changed[parent.index()] || mask_changed[parent.index()];
                    match mask_region[parent.index()] {
                        Some(masked) => (masked, changed),
                        None => (parent_out, changed),
                    }
                }
                None => (ClipIndex::UNCLIPPED, false),
            };

            let geo_changed = prev_entry.is_none_or(|p| {
                p.x.to_bits() != geometry.x.to_bits()
                    || p.y.to_bits() != geometry.y.to_bits()
                    || p.w.to_bits() != geometry.w.to_bits()
                    || p.h.to_bits() != geometry.h.to_bits()
            });

            // Effective visibility: hidden if this node or any ancestor is
            // `Visible(false)` (M5). `hidden_changed` propagates a
            // visibility toggle down so a descendant re-interns its paint.
            let parent_hidden = node.parent.is_some_and(|p| eff_hidden[p.index()]);
            let parent_hidden_changed = node.parent.is_some_and(|p| hidden_changed[p.index()]);
            eff_hidden[id.index()] = parent_hidden || !node_visible;
            hidden_changed[id.index()] =
                parent_hidden_changed || visible_toggled_set.contains(&id.index());

            // A mask node and a hidden node both draw nothing — a mask is a
            // stencil (`docs/decisions/masks-and-group-opacity.md`), a
            // hidden node is not drawn (M5). Neither contributes to overlap.
            let draws_nothing = node.mask || eff_hidden[id.index()];

            // Paint: reuse the stable previous index unless the node's paint
            // intent changed, its mask flag toggled (paint-dirty), or its
            // visibility toggled (draws-nothing changes). `contributes[i]`
            // reads the fill's presence without cloning; the clone stays in
            // the cache-miss arm so a change-scaled commit does not
            // heap-clone every gradient's stops every frame (M9, P3).
            let paint = match prev_entry
                .filter(|_| !paint_dirty_set.contains(&id.index()) && !hidden_changed[id.index()])
            {
                Some(prev) => prev.paint,
                None => {
                    let entry = if draws_nothing {
                        PaintEntry::default()
                    } else {
                        let fill = arena.overlay(id).fill.or_else(|| node.fill.clone());
                        PaintEntry {
                            fill,
                            stroke: node.stroke,
                            corners: node.corners,
                            // Shadows are not variant-overridable (the
                            // variant vocabulary is X/Y/W/H/Fill), so they
                            // come straight from the node. Cloned in the
                            // cache-miss arm only, like the fill's stops.
                            shadows: node.shadows.clone(),
                            blurs: node.blurs.clone(),
                            // The baked-vector coverage mask (story B1), not
                            // variant-overridable either — straight from the
                            // node. `VectorField` is `Copy`, so no clone.
                            shape: node.shape,
                            // Stacked fills (story C1), not variant-overridable
                            // either — same posture as shadows, straight from
                            // the node.
                            extra_fills: node.extra_fills.clone(),
                        }
                    };
                    intern_paint(&mut back_paints, paint_map, entry)
                }
            };

            // Clip: reuse the previous index unless the region reaching
            // this node changed (or it is new).
            let clip = match prev_entry {
                Some(prev) if !region_in_changed => prev.clip,
                _ => region_in_index,
            };

            // The painted extent for the overlap test: the box grown by the
            // stroke's outset (an outside or center stroke paints past the
            // box), then cut down to where the rect's clip region lets it
            // paint (issue #276). `None` when the node draws nothing (M10),
            // or when the clip leaves it no area at all.
            if !draws_nothing
                && (arena.overlay(id).fill.is_some()
                    || node.fill.is_some()
                    || node.stroke.is_some())
            {
                painted_extent[i] = clipped_extent(
                    stroke_extent(geometry, node.stroke.as_ref()),
                    back_clips.resolve(clip),
                );
            }

            // `opacity` is a placeholder here; the group-opacity pass below
            // fills every rect's free-path alpha once subtree overlap is
            // known.
            let entry = RectEntry {
                x: geometry.x,
                y: geometry.y,
                w: geometry.w,
                h: geometry.h,
                paint,
                clip,
                opacity: 1.0,
            };

            // The region this node hands its children: its own incoming
            // region plus its box when it clips. `paint_dirty` stands in
            // for a corners/box change (corners are paint intent);
            // over-marking only re-resolves a descendant to the same index,
            // never a wrong one.
            let box_changed = geo_changed || paint_dirty_set.contains(&id.index());
            if node.clip {
                let node_box = ClipBox {
                    x: geometry.x,
                    y: geometry.y,
                    w: geometry.w,
                    h: geometry.h,
                    corners: node.corners,
                };
                region_out_index[id.index()] = Some(intern_region(
                    &mut back_clips,
                    clip_map,
                    region_in_index,
                    node_box,
                ));
                region_out_changed[id.index()] =
                    region_in_changed || clip_toggled_set.contains(&id.index()) || box_changed;
            } else {
                region_out_index[id.index()] = Some(region_in_index);
                region_out_changed[id.index()] =
                    region_in_changed || clip_toggled_set.contains(&id.index());
            }

            // A visible mask node stencils the siblings that follow it in
            // the same parent: chain its box onto the parent's mask region
            // so following siblings intersect every mask (M3). A hidden mask
            // does not mask (M2). Any change to this node's masking — flag
            // toggled, geometry moved, or visibility changed — re-resolves
            // the regions its following siblings receive (M1).
            if let Some(parent) = node.parent {
                let node_masks = node.mask && node_visible;
                if node_masks {
                    let node_box = ClipBox {
                        x: geometry.x,
                        y: geometry.y,
                        w: geometry.w,
                        h: geometry.h,
                        corners: node.corners,
                    };
                    mask_region[parent.index()] = Some(intern_region(
                        &mut back_clips,
                        clip_map,
                        region_in_index,
                        node_box,
                    ));
                }
                let mask_state_changed = mask_toggled_set.contains(&id.index())
                    || visible_toggled_set.contains(&id.index())
                    || (node.mask && box_changed);
                if mask_state_changed {
                    mask_changed[parent.index()] = true;
                }
            }

            rects.push(entry);
        }

        // Group opacity (`docs/decisions/masks-and-group-opacity.md`).
        // First the exclusive end of every rect's subtree in rect-index
        // order: a subtree is contiguous in DFS, so a post-order max from
        // each child into its parent's slot suffices.
        let mut subtree_end: Vec<u32> = (1..=order.len() as u32).collect();
        for i in (0..order.len()).rev() {
            if let Some(parent) = arena.nodes[order[i].index()].parent {
                let pi = rect_of_slot[parent.index()] as usize;
                subtree_end[pi] = subtree_end[pi].max(subtree_end[i]);
            }
        }

        // Then a pre-order pass carrying the free-path alpha product down
        // the tree. A node with opacity below 1 whose painted subtree is
        // mutually non-overlapping folds its alpha into every subtree rect
        // (the free path); an overlapping one becomes a render-target
        // group whose layer composites at the node's alpha times the
        // carried product, and its subtree draws into that layer at a
        // reset product of 1.
        let mut groups: Vec<GroupComposite> = Vec::new();
        let mut carried_out: Vec<f32> = vec![1.0; n];
        for (i, &id) in order.iter().enumerate() {
            let node = &arena.nodes[id.index()];
            let base = match node.parent {
                Some(parent) => carried_out[parent.index()],
                None => 1.0,
            };
            let opacity = node.opacity.clamp(0.0, 1.0);
            let end = subtree_end[i];
            // `opacity == 0` needs no compositing at all — the subtree is
            // simply not drawn (`visible-is-layout-opacity-is-paint.md`), so
            // it stays on the free path even when its children overlap
            // (story #44 M14). Only `0 < opacity < 1` over an overlapping
            // subtree needs a render target.
            if opacity > 0.0 && opacity < 1.0 && subtree_overlaps(i as u32, end, &painted_extent) {
                groups.push(GroupComposite {
                    start: i as u32,
                    end,
                    alpha: base * opacity,
                });
                // The node's own rect and its subtree draw into the layer
                // at full alpha; the group's alpha applies once, at the
                // composite.
                rects[i].opacity = 1.0;
                carried_out[id.index()] = 1.0;
            } else {
                let alpha = if opacity < 1.0 { base * opacity } else { base };
                rects[i].opacity = alpha;
                carried_out[id.index()] = alpha;
            }
        }

        // Reclaim the pooled entries nothing references any more (issue
        // #197). Retaining an entry's index for the life of the arena is
        // what makes the dirty check a bit compare (issue #164), but
        // nothing ever released a slot: a changed paint or clip earned a
        // new index and its old entry stayed. An animated fill therefore
        // grew the paint table by one entry per frame, without bound, and
        // a per-frame-resizing clip did the same to the clip table.
        // Rebuilding a table from the entries this commit's rects
        // reference bounds it again. It renumbers, so it runs only when
        // most of what the table holds is already unreachable.
        //
        // A rebuild renumbers, so the watermark the guard's unwind path
        // uses to tell entries that survive a failed walk from entries the
        // walk added stops meaning anything: after a rebuild, no index in
        // the map names a slot of the front buffer's table. Dropping the
        // watermark to zero makes that path clear the map instead, which
        // only costs a re-intern (issues #196 and #197 together).
        if should_compact(back_paints.len(), rects.len()) {
            *paints_at_open = 0;
            compact_paints(&mut back_paints, paint_map, &mut rects);
        }
        if should_compact(back_clips.len(), rects.len()) {
            *clips_at_open = 0;
            compact_clips(&mut back_clips, clip_map, &mut rects);
        }

        // Text staging — the text half of the geometry seam (P2: one
        // typesetter, asked exactly once, computing nothing here). The
        // rect table is final at this point, so the `geometry` a stager
        // places glyphs against is a lookup into what this commit solved
        // rather than new work, and the anchor each run is stamped with is
        // the index that run will be read against.
        //
        // Core stamps runs; it does not build them. It has no typesetter,
        // no fonts and no atlas, and needs none — the stager supplies the
        // placed glyphs exactly as the solver supplies the rects.
        let glyphs = {
            let geometry = |id: NodeId| {
                let i = rect_of_slot_checked(&rect_of_slot, id, "asked for the geometry of");
                let r = rects[i as usize];
                SolvedRect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: r.h,
                }
            };
            let mut staged = solver.stage_text(arena, &geometry);
            // Stamp each run with its node's rect index, then order the
            // table by anchor. A stager walking DFS already returns them in
            // this order; sorting makes the invariant true by construction
            // rather than by contract, which is what lets a painter walk
            // runs and rects together with one cursor. The sort is stable,
            // so the run order *within* one text node — the font-fallback
            // split — is preserved.
            for staged in &mut staged {
                staged.run.rect =
                    rect_of_slot_checked(&rect_of_slot, staged.node, "returned a run for");
            }
            staged.sort_by_key(|staged| staged.run.rect);
            let mut table = GlyphRunTable::with_atlases(solver.atlases());
            for staged in staged {
                table.push_run(staged.run, &staged.quads);
            }
            table
        };

        // Dirty is a bit compare against the previous commit at each index
        // (what a painter refreshes), computed once the rect entries —
        // opacity included, and any renumbering done — are final. A
        // shifted tail after a structural change reports dirty, as
        // intended.
        let mut dirty_set: FxHashSet<u32> = FxHashSet::default();
        for (i, entry) in rects.iter().enumerate() {
            if previous
                .rects
                .get(i)
                .is_none_or(|old| entry_bits(old) != entry_bits(entry))
            {
                dirty_set.insert(i as u32);
            }
        }
        // A render-target group's alpha lives outside the rect entry bits,
        // so a group forming, dissolving, or changing alpha would otherwise
        // leave its subtree's rects clean while the composited pixels move
        // (M8). Every rect covered by a group present on exactly one side of
        // this commit is dirtied. Groups are few; the scan is cheap.
        for group in groups.iter().chain(previous.groups.iter()) {
            let on_both = groups.contains(group) && previous.groups.contains(group);
            if !on_both {
                dirty_set.extend(group.start..group.end.min(rects.len() as u32));
            }
        }
        // A text node's glyph runs live outside its rect entry bits, the
        // same way a render-target group's alpha does. A node whose string
        // or text style changed inside a box that did not move or resize
        // produces identical bits, so the compare above would report it
        // clean and a retained painter would redraw nothing and keep last
        // frame's glyphs. Every anchor whose staged runs differ from the
        // previous commit's is dirtied here, which covers a changed string,
        // a changed style, a variant switch, and a fallback that picked a
        // different font — none of which need their own rule, because the
        // runs are what actually reached the painter.
        for anchor in changed_anchors(&glyphs, &previous.glyphs) {
            if anchor < rects.len() as u32 {
                dirty_set.insert(anchor);
            }
        }
        let mut dirty: Vec<u32> = dirty_set.into_iter().collect();
        dirty.sort_unstable();

        let generation = previous.generation + 1;
        // The index maps change only on a structural change; otherwise the
        // previous commit's maps still describe this DFS order, so share
        // them by reference.
        let (node_ids, rect_index) = if structural {
            let mut rect_index = vec![0u32; n];
            for (i, &id) in order.iter().enumerate() {
                // In range for u32 by the add_node guard.
                rect_index[id.index()] = i as u32;
            }
            (Arc::new(order), Arc::new(rect_index))
        } else {
            (
                Arc::clone(&previous.node_ids),
                Arc::clone(&previous.rect_index),
            )
        };
        let images = Arc::clone(&arena.images);
        let back_scene = CommittedScene {
            rects,
            paints: back_paints,
            images,
            clips: back_clips,
            groups,
            glyphs,
            generation,
            dirty,
            node_ids,
            rect_index,
        };

        // Publish the buffer and drain the change log.
        let back = 1 - arena.front;
        arena.buffers[back] = back_scene;
        arena.front = back;
        arena.layout_dirty.clear();
        arena.paint_dirty.clear();
        arena.clip_toggled.clear();
        arena.mask_toggled.clear();
        arena.visible_toggled.clear();
        // The tables the walk interned into are now the front buffer's, so
        // every index the maps hold resolves. Until this point the guard
        // rolls those entries back on the way out (issue #196).
        guard.published = true;
        generation
    }
}

/// The rect index a node resolved to in the walk that built `rect_of_slot`.
///
/// # Panics
///
/// Panics if `id` is not a node of the arena being committed — the same
/// index-integrity contract malformed [`LayoutSolver::solve`] output is
/// held to (P4). `did` is the whole verb clause, so the message names
/// which of the stager's two calls was wrong rather than reading as one
/// generic failure.
fn rect_of_slot_checked(rect_of_slot: &[u32], id: NodeId, did: &str) -> u32 {
    *rect_of_slot
        .get(id.index())
        .unwrap_or_else(|| panic!("the stager {did} {id:?}, which is not a node of this arena"))
}

/// The anchors whose runs differ between two commits' glyph tables —
/// present in one and not the other, or present in both with different
/// runs.
///
/// Both tables are ordered by anchor, so each anchor's runs form one
/// contiguous slice and the comparison is a merge walk over the two.
fn changed_anchors(new: &GlyphRunTable, old: &GlyphRunTable) -> Vec<u32> {
    let mut changed = Vec::new();
    let (new, old) = (new.runs(), old.runs());
    let (mut i, mut j) = (0usize, 0usize);
    while i < new.len() || j < old.len() {
        // Settle the lower of the two leading anchors; when one side is
        // exhausted, the other's anchor is the one to settle.
        let anchor = match (new.get(i).map(|r| r.rect), old.get(j).map(|r| r.rect)) {
            (Some(n), Some(o)) => n.min(o),
            (Some(n), None) => n,
            (None, Some(o)) => o,
            (None, None) => break,
        };
        let ni = i;
        while new.get(i).is_some_and(|r| r.rect == anchor) {
            i += 1;
        }
        let oj = j;
        while old.get(j).is_some_and(|r| r.rect == anchor) {
            j += 1;
        }
        // An anchor present on only one side has an empty slice on the
        // other, so "appeared" and "disappeared" fall out of the same
        // comparison as "changed".
        if new[ni..i] != old[oj..j] {
            changed.push(anchor);
        }
    }
    changed
}

/// Holds the retained interners for one commit walk and puts them back on
/// the arena however the walk ends — normally or by unwinding (issue
/// #196).
///
/// The walk mutates the two maps while the arena itself is borrowed
/// immutably (for [`Arena::overlay`]), so the maps are moved out of the
/// arena for its duration. Restoring them only on the success path left an
/// arena that had caught a mid-commit panic holding two empty maps: the
/// next commit would re-intern from index 0 while the committed buffers
/// still referenced the entries at the old indices.
///
/// A failed walk drops the paint and clip tables it was building, so the
/// entries it interned exist nowhere afterwards. Restoring the maps
/// unchanged would therefore leave keys naming indices past the end of the
/// live tables — worse than the empty maps, because such an index reaches
/// a painter, which refuses an out-of-range index by panicking (P4). The
/// rollback removes exactly the entries a failed walk added: indices are
/// assigned by appending, so those are the ones at or past the table
/// lengths recorded when the walk opened.
struct InternerGuard<'a> {
    arena: &'a mut Arena,
    paint_map: FxHashMap<PaintKey, PaintIndex>,
    clip_map: FxHashMap<ClipKey, ClipIndex>,
    /// Paint and clip table lengths of the front buffer when the walk
    /// opened — the boundary between entries that survive a failed walk
    /// and entries that do not.
    paints_at_open: usize,
    clips_at_open: usize,
    /// Set once the commit has published its buffer. Until then the guard
    /// treats the walk as failed and rolls back.
    published: bool,
}

impl<'a> InternerGuard<'a> {
    fn open(arena: &'a mut Arena) -> Self {
        let front = &arena.buffers[arena.front];
        let paints_at_open = front.paints.len();
        let clips_at_open = front.clips.len();
        let paint_map = std::mem::take(&mut arena.paint_map);
        let clip_map = std::mem::take(&mut arena.clip_map);
        Self {
            arena,
            paint_map,
            clip_map,
            paints_at_open,
            clips_at_open,
            published: false,
        }
    }
}

impl Drop for InternerGuard<'_> {
    fn drop(&mut self) {
        if !self.published {
            let paints_at_open = self.paints_at_open;
            let clips_at_open = self.clips_at_open;
            self.paint_map
                .retain(|_, index| (index.0 as usize) < paints_at_open);
            self.clip_map
                .retain(|_, index| (index.0 as usize) < clips_at_open);
        }
        self.arena.paint_map = std::mem::take(&mut self.paint_map);
        self.arena.clip_map = std::mem::take(&mut self.clip_map);
    }
}

/// How a `Prop` change reaches the committed output — the classification
/// that lets a commit re-do only the affected work (issue #164).
enum PropClass {
    /// Geometry / measured size: feeds the solver's dirty set.
    Layout,
    /// Fill, stroke, or corners: feeds commit's paint re-intern.
    Paint,
    /// The clip flag: feeds the clip-region cascade.
    ClipFlag,
    /// The mask flag: re-interns the node's own (now draws-nothing) paint
    /// and feeds the clip-region cascade for its following siblings.
    MaskFlag,
    /// The visibility flag: layout-affecting (the solver hides the node)
    /// and paint-affecting under the fixed solver (a hidden subtree draws
    /// nothing).
    VisibleFlag,
    /// Node/group opacity: recomputed on every commit walk, so it records
    /// nothing (the walk reads `node.opacity` fresh, and the rect entry's
    /// alpha carries the change to the dirty set).
    OpacityOnly,
}

fn prop_class(prop: &Prop) -> PropClass {
    match prop {
        Prop::Fill(_)
        | Prop::FillWith(_)
        | Prop::Stroke(_)
        | Prop::Corners { .. }
        | Prop::Shadows(_)
        | Prop::Blurs(_)
        | Prop::ExtraFills(_)
        | Prop::ShapeField(_) => PropClass::Paint,
        Prop::Clip(_) => PropClass::ClipFlag,
        Prop::Mask(_) => PropClass::MaskFlag,
        Prop::Visible(_) => PropClass::VisibleFlag,
        Prop::Opacity(_) => PropClass::OpacityOnly,
        // Everything else is layout or measured-size intent. Text and
        // TextStyle change the shaped run a measuring solver sizes to, so
        // they are layout-affecting even though they touch no rect field
        // directly.
        Prop::X(_)
        | Prop::Y(_)
        | Prop::Width(_)
        | Prop::Height(_)
        | Prop::Text(_)
        | Prop::TextStyle(_)
        | Prop::Mode(_)
        | Prop::Gap(_)
        | Prop::Padding { .. }
        | Prop::Margin { .. }
        | Prop::MainAlign(_)
        | Prop::CrossAlign(_)
        | Prop::SizingH(_)
        | Prop::SizingV(_)
        | Prop::MinWidth(_)
        | Prop::MaxWidth(_)
        | Prop::MinHeight(_)
        | Prop::MaxHeight(_)
        | Prop::CrossGap(_)
        | Prop::GridRows(_)
        | Prop::GridColumns(_)
        | Prop::GridRow(_)
        | Prop::GridColumn(_)
        | Prop::GridRowSpan(_)
        | Prop::GridColumnSpan(_) => PropClass::Layout,
    }
}

/// Intern a paint entry through the retained interner: reuse the stable
/// index if the entry was seen before, else push it (copying the table on
/// write) and record the new index (issue #164).
fn intern_paint(
    paints: &mut Arc<PaintTable>,
    interned: &mut FxHashMap<PaintKey, PaintIndex>,
    entry: PaintEntry,
) -> PaintIndex {
    let key = paint_key(&entry);
    if let Some(&index) = interned.get(&key) {
        return index;
    }
    // Cannot truncate: the paint table stays below u32::MAX. `add_node`
    // bounds the node count, and the rebuild in `compact_paints` bounds
    // the table at a small multiple of the rect count (issue #197).
    let index = Arc::make_mut(paints).push(entry);
    interned.insert(key, index);
    index
}

/// Table size below which a rebuild is not worth its bookkeeping. Small
/// scenes stay untouched: a table this size costs less than the rebuild
/// that would shrink it (issue #197).
const COMPACT_FLOOR: usize = 256;

/// Whether a pooled table holds enough unreachable entries to be worth
/// rebuilding (issue #197).
///
/// Each rect references exactly one paint entry and one clip region, so
/// the rect count bounds how many entries can still be reachable. A table
/// more than twice that size therefore holds at least as many dead entries
/// as live ones, and a rebuild at least halves it — which is what makes
/// the rebuild's O(scene) cost amortize to a constant per commit.
fn should_compact(table_len: usize, rect_count: usize) -> bool {
    table_len > COMPACT_FLOOR && table_len > 2 * rect_count
}

/// Rebuild the paint table from the entries `rects` reference, renumber
/// those rects, and re-key the retained interner onto the new indices
/// (issue #197).
///
/// Every rect is renumbered, so the commit that runs this reports its
/// whole rect table dirty and a painter re-uploads it. That is the price
/// of reclaiming the slots, and why the caller runs this rarely.
fn compact_paints(
    paints: &mut Arc<PaintTable>,
    interned: &mut FxHashMap<PaintKey, PaintIndex>,
    rects: &mut [RectEntry],
) {
    let mut table = PaintTable::new();
    let mut moved: FxHashMap<u32, PaintIndex> = FxHashMap::default();
    for rect in rects.iter_mut() {
        rect.paint = match moved.get(&rect.paint.0) {
            Some(&index) => index,
            None => {
                let index = table.push(paints.resolve(rect.paint).clone());
                moved.insert(rect.paint.0, index);
                index
            }
        };
    }
    // Re-key from the rebuilt table rather than by translating the old
    // map: the surviving entries are exactly the table's, and a key is
    // derived from its entry, so there is nothing to carry over.
    interned.clear();
    for i in 0..table.len() {
        // In range for u32: the table holds at most one entry per rect,
        // and `add_node` keeps the node count below u32::MAX.
        let index = PaintIndex(i as u32);
        interned.insert(paint_key(table.resolve(index)), index);
    }
    *paints = Arc::new(table);
}

/// Rebuild the clip table the same way (issue #197).
///
/// A region is a list of clipping ancestor boxes and the interner names it
/// by its prefix's index plus its last box, so the rebuild works in
/// box-list space: every region a rect references contributes itself and
/// each of its prefixes, and the list is rebuilt shortest first, so a
/// region's prefix already has its new index when the region is pushed.
fn compact_clips(
    clips: &mut Arc<ClipTable>,
    interned: &mut FxHashMap<ClipKey, ClipIndex>,
    rects: &mut [RectEntry],
) {
    let bits_of =
        |boxes: &[ClipBox]| -> Vec<[u32; 8]> { boxes.iter().map(clip_box_bits).collect() };

    let mut live: Vec<Vec<ClipBox>> = Vec::new();
    let mut seen: FxHashSet<Vec<[u32; 8]>> = FxHashSet::default();
    for rect in rects.iter() {
        let boxes = clips.resolve(rect.clip).boxes();
        // The empty list is the unclipped region, which `ClipTable::new`
        // already reserves at index 0, so the prefixes start at length 1.
        for length in 1..=boxes.len() {
            let prefix = &boxes[..length];
            if seen.insert(bits_of(prefix)) {
                live.push(prefix.to_vec());
            }
        }
    }
    live.sort_by_key(Vec::len);

    let mut table = ClipTable::new();
    let mut index_of: FxHashMap<Vec<[u32; 8]>, ClipIndex> = FxHashMap::default();
    interned.clear();
    for boxes in &live {
        let bits = bits_of(boxes);
        let (last, head) = bits.split_last().expect("a live region carries a box");
        let parent = match head {
            [] => ClipIndex::UNCLIPPED,
            _ => *index_of
                .get(head)
                .expect("shorter prefixes are rebuilt before the regions that extend them"),
        };
        let index = table.push(boxes);
        interned.insert((parent.0, *last), index);
        index_of.insert(bits, index);
    }

    for rect in rects.iter_mut() {
        let bits = bits_of(clips.resolve(rect.clip).boxes());
        rect.clip = match bits.is_empty() {
            true => ClipIndex::UNCLIPPED,
            false => *index_of
                .get(&bits)
                .expect("every region a rect references was rebuilt"),
        };
    }
    *clips = Arc::new(table);
}

/// The clip region a parent hands its children, read while the child is
/// visited. `None` means the commit walk reached a child before its
/// parent: [`Arena::dfs_order`] is parent-before-child, so an unset slot
/// is a broken traversal invariant, not a recoverable state.
///
/// The pre-#164 commit path stored these as `Option` for exactly this
/// reason and panicked here; the incremental rewrite replaced them with a
/// vector defaulting to `UNCLIPPED`, which would mis-clip the whole
/// subtree in silence instead (issue #198). P4 — a violated invariant is
/// a named failure, never a silent degrade.
fn parent_region_out(region_out: &[Option<ClipIndex>], parent: NodeId) -> ClipIndex {
    region_out[parent.index()].unwrap_or_else(|| {
        panic!(
            "commit reached a child of {parent:?} before {parent:?} itself: the clip cascade \
             requires parent-before-child document order (P4)"
        )
    })
}

/// The interning key of one node's clip region: the region its parent
/// resolved to, plus the parent's clip box. Equal ancestor chains take
/// equal keys by induction (the parent's index already stands for its
/// whole chain), so this dedups regions by value at O(1) per node —
/// without hashing a chain-shaped key.
type ClipKey = (u32, [u32; 8]);

fn intern_region(
    clips: &mut Arc<ClipTable>,
    interned: &mut FxHashMap<ClipKey, ClipIndex>,
    parent_region: ClipIndex,
    parent_box: ClipBox,
) -> ClipIndex {
    let key = (parent_region.0, clip_box_bits(&parent_box));
    if let Some(&index) = interned.get(&key) {
        return index;
    }
    // Copy-on-write: the table is shared with the previous commit until a
    // genuinely new region forces a copy (issue #164).
    let table = Arc::make_mut(clips);
    let mut boxes = table.resolve(parent_region).boxes().to_vec();
    boxes.push(parent_box);
    let index = table.push(&boxes);
    interned.insert(key, index);
    index
}

/// The interning key of one paint entry: a canonical bit encoding of the
/// entry in full.
///
/// It encodes the *whole* entry rather than a per-field tuple, because a
/// key that names each field has to grow an arm for every vocabulary
/// widening — and the previous one did not. It carried only
/// `(fill color, corners)` and answered a non-solid fill with
/// `unreachable!`, which would have panicked in the dirty-set diff, one
/// commit away from the producer that staged the gradient (issue #131).
/// Encoding the entry means the next fill kind costs one match arm here
/// and nothing anywhere else.
///
/// `f32`s go in by bit pattern, like the rest of the commit path: `f32`
/// equality is not reflexive for NaN, which would make a rect permanently
/// dirty and its paint entry never dedup.
type PaintKey = Vec<u32>;

fn paint_key(entry: &PaintEntry) -> PaintKey {
    let mut key = Vec::new();

    push_fill_key(&mut key, entry.fill.as_ref());

    // Stacked fills (story C1, debt #146). Omitted from this key until debt
    // #395: `commit` copies `extra_fills` onto the entry, so two nodes sharing
    // a base fill and differing only in their stacked layers interned to ONE
    // pool entry and the overlay was lost on load — silently, with no
    // diagnostic. Measured on the Landify hero: the document carried one entry
    // with one extra layer and the arena kept none. Same "count then each
    // element's bits" framing as the sections below, so the encoding stays
    // prefix-free and no two distinct entries can collide.
    key.push(entry.extra_fills.len() as u32);
    for layer in &entry.extra_fills {
        push_fill_key(&mut key, Some(layer));
    }

    match &entry.stroke {
        None => key.push(0),
        Some(stroke) => {
            key.push(1);
            key.push(stroke.width.to_bits());
            key.push(stroke.align as u32);
            key.extend(color_key(stroke.color));
        }
    }

    key.extend(corner_key(entry.corners));

    key.push(entry.blurs.len() as u32);
    for b in &entry.blurs {
        key.push(b.kind as u32);
        key.push(b.radius.to_bits());
    }
    key.push(entry.shadows.len() as u32);
    for shadow in &entry.shadows {
        key.push(shadow.kind as u32);
        key.extend(vec2_key(shadow.offset));
        key.push(shadow.blur.to_bits());
        key.push(shadow.spread.to_bits());
        key.extend(color_key(shadow.color));
    }

    // The baked-vector coverage mask (story B1). Absent = the parametric
    // shape (a leading 0). A present field encodes its full resolved
    // reference, so two nodes with the same fill but different shapes — or a
    // shape vs. the parametric box — take distinct pool entries.
    match &entry.shape {
        None => key.push(0),
        Some(field) => {
            key.push(1);
            key.push(field.image);
            key.extend(field.atlas_rect);
            key.extend(field.plane_bounds.iter().map(|v| v.to_bits()));
            key.push(field.distance_range.to_bits());
        }
    }
    key
}

/// One fill's bits, tag-dispatched so the encoding is self-delimiting: the
/// leading tag determines how many words follow, which is what lets the base
/// fill, each stacked layer, and every section after them concatenate without
/// ambiguity.
///
/// `None` is the fill-less entry (tag 0). A stacked layer is always `Some`.
fn push_fill_key(key: &mut Vec<u32>, fill: Option<&PaintKind>) {
    match fill {
        None => key.push(0),
        Some(PaintKind::Solid { color }) => {
            key.push(1);
            key.extend(color_key(*color));
        }
        Some(PaintKind::Gradient(gradient)) => {
            key.push(2);
            key.push(gradient.kind as u32);
            key.extend(vec2_key(gradient.handle_origin));
            key.extend(vec2_key(gradient.handle_primary));
            key.extend(vec2_key(gradient.handle_secondary));
            key.push(gradient.stops.len() as u32);
            for stop in &gradient.stops {
                key.push(stop.offset.to_bits());
                key.extend(color_key(stop.color));
            }
        }
        Some(PaintKind::Image {
            image,
            scale_mode,
            transform,
            tile_scale,
        }) => {
            key.push(3);
            key.push(*image);
            key.push(*scale_mode as u32);
            key.push(tile_scale.to_bits());
            match transform {
                None => key.push(0),
                Some(m) => {
                    key.push(1);
                    key.extend([
                        m.a.to_bits(),
                        m.b.to_bits(),
                        m.c.to_bits(),
                        m.d.to_bits(),
                        m.tx.to_bits(),
                        m.ty.to_bits(),
                    ]);
                }
            }
        }
    }
}

fn vec2_key(v: Vec2) -> [u32; 2] {
    [v.x.to_bits(), v.y.to_bits()]
}

fn color_key(color: Color) -> [u32; 4] {
    [
        color.r.to_bits(),
        color.g.to_bits(),
        color.b.to_bits(),
        color.a.to_bits(),
    ]
}

fn corner_key(corners: CornerRadii) -> [u32; 4] {
    [
        corners.top_left.to_bits(),
        corners.top_right.to_bits(),
        corners.bottom_right.to_bits(),
        corners.bottom_left.to_bits(),
    ]
}

/// The bits a painter uploads for an entry (R-T4). Bit comparison keeps
/// the diff deterministic where `f32` equality is not (NaN never equals
/// itself and would mark a rect permanently dirty). The free-path group
/// alpha is part of the entry, so a group-opacity change on the free path
/// reaches the dirty set (`docs/decisions/masks-and-group-opacity.md`).
fn entry_bits(entry: &RectEntry) -> [u32; 7] {
    [
        entry.x.to_bits(),
        entry.y.to_bits(),
        entry.w.to_bits(),
        entry.h.to_bits(),
        entry.paint.0,
        entry.clip.0,
        entry.opacity.to_bits(),
    ]
}

/// A painting rect's device extent (top-left origin plus size), grown from
/// the layout box by any stroke outset so the overlap test sees the pixels
/// the stroke actually covers (`docs/decisions/masks-and-group-opacity.md`
/// M10).
#[derive(Clone, Copy)]
struct Extent {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// A node's painted extent: its resolved box grown by the stroke's outset.
/// An `Outside` stroke paints a full width past the box; a `Center` stroke
/// half a width; an `Inside` stroke stays within the box. A gradient or
/// image fill under a partial-opacity group is why the box alone is not
/// enough — a center/outside stroke band otherwise escapes the overlap test
/// and seams at ~0.75 alpha under the free path (M10).
fn stroke_extent(geometry: SolvedRect, stroke: Option<&Stroke>) -> Extent {
    let outset = match stroke {
        None => 0.0,
        Some(s) => match s.align {
            StrokeAlign::Inside => 0.0,
            StrokeAlign::Center => s.width / 2.0,
            StrokeAlign::Outside => s.width,
        },
    };
    Extent {
        x: geometry.x - outset,
        y: geometry.y - outset,
        w: geometry.w + 2.0 * outset,
        h: geometry.h + 2.0 * outset,
    }
}

/// A painting rect's extent cut down to where its clip region lets it
/// paint: the extent intersected with every box of the region. `None` when
/// nothing is left — a rect the clip removes entirely paints no pixels and
/// so cannot overlap anything (issue #276).
///
/// Corners are ignored, so a rounded clip box contributes its rectangle.
/// That keeps the result a superset of the pixels the rect really covers,
/// which is the direction the overlap test has to err in: judging two rects
/// disjoint when they share a pixel would under-composite, a visible bug,
/// while judging two disjoint rects overlapping only costs a render target.
fn clipped_extent(extent: Extent, region: ClipView<'_>) -> Option<Extent> {
    let mut left = extent.x;
    let mut top = extent.y;
    let mut right = extent.x + extent.w;
    let mut bottom = extent.y + extent.h;
    for clip in region.boxes() {
        left = left.max(clip.x);
        top = top.max(clip.y);
        right = right.min(clip.x + clip.w);
        bottom = bottom.min(clip.y + clip.h);
    }
    (right > left && bottom > top).then_some(Extent {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}

/// Whether two painting extents in the range `[start, end)` overlap — the
/// group-opacity overlap test
/// (`docs/decisions/masks-and-group-opacity.md`). Non-overlap of the
/// painted extents is exactly the condition that lets a group opacity fold
/// into per-rect alpha without double-blending, so the test is over the
/// nodes that actually paint (a mask, hidden, or layout-only node
/// contributes `None`). Zero-area extents never overlap (strict comparison).
///
/// Each extent arrives already cut down to its rect's clip region
/// ([`clipped_extent`]), so content that two disjoint clips separate is no
/// longer judged overlapping (issue #276). The region is reduced to its
/// bounding box, so the test still errs towards reporting an overlap and
/// never under-composites.
fn subtree_overlaps(start: u32, end: u32, painted: &[Option<Extent>]) -> bool {
    let extents: Vec<&Extent> = (start..end)
        .filter_map(|i| painted[i as usize].as_ref())
        .collect();
    for (a_i, a) in extents.iter().enumerate() {
        for b in &extents[a_i + 1..] {
            let x_overlap = a.x < b.x + b.w && b.x < a.x + a.w;
            let y_overlap = a.y < b.y + b.h && b.y < a.y + a.h;
            if x_overlap && y_overlap {
                return true;
            }
        }
    }
    false
}

fn clip_box_bits(clip: &ClipBox) -> [u32; 8] {
    let corners = corner_key(clip.corners);
    [
        clip.x.to_bits(),
        clip.y.to_bits(),
        clip.w.to_bits(),
        clip.h.to_bits(),
        corners[0],
        corners[1],
        corners[2],
        corners[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // The parent-before-child tripwire of the clip cascade (issue #198).
    // `dfs_order` is parent-before-child, so no public API can present the
    // walk with a violated order — the guard is unit-tested directly, on
    // the state a violation would produce.

    #[test]
    fn parent_region_out_returns_a_resolved_parents_region() {
        let region_out = vec![Some(ClipIndex(7)), None];
        assert_eq!(parent_region_out(&region_out, NodeId(0)), ClipIndex(7));
    }

    #[test]
    #[should_panic(expected = "requires parent-before-child document order")]
    fn parent_region_out_panics_when_the_parent_is_unresolved() {
        // What a child-before-parent traversal would hand the cascade: the
        // parent's slot still unset. Reading `UNCLIPPED` there instead
        // would mis-clip the whole subtree in silence.
        let region_out = vec![None, Some(ClipIndex(1))];
        let _ = parent_region_out(&region_out, NodeId(0));
    }

    // The interner guard's rollback rule (issue #196), exercised on the
    // guard itself. The commit-level behaviour is covered from the public
    // API in `tests/arena.rs`; these pin the watermark rule the rollback
    // is built on, including the value `commit_with` sets after a pooled
    // table is rebuilt (issue #197).

    #[test]
    fn a_failed_walk_keeps_the_interner_entries_below_the_watermark() {
        let mut arena = Arena::new();
        {
            let mut guard = InternerGuard::open(&mut arena);
            guard.paints_at_open = 1;
            guard.clips_at_open = 2;
            guard.paint_map.insert(vec![0], PaintIndex(0));
            guard.paint_map.insert(vec![1], PaintIndex(1));
            guard.clip_map.insert((0, [0; 8]), ClipIndex(1));
            guard.clip_map.insert((1, [0; 8]), ClipIndex(2));
        }
        assert_eq!(
            arena.paint_map.get(&vec![0]),
            Some(&PaintIndex(0)),
            "an entry the front buffer's table still holds survives",
        );
        assert_eq!(
            arena.paint_map.len(),
            1,
            "the entry the failed walk added is gone",
        );
        assert_eq!(arena.clip_map.len(), 1);
    }

    #[test]
    fn a_failed_walk_after_a_rebuild_clears_the_interner() {
        // `commit_with` drops the watermark to zero when it rebuilds a
        // pooled table: a rebuild renumbers, so no index in the map names a
        // slot of the front buffer's table any more and none can be kept.
        let mut arena = Arena::new();
        {
            let mut guard = InternerGuard::open(&mut arena);
            guard.paints_at_open = 0;
            guard.clips_at_open = 0;
            guard.paint_map.insert(vec![0], PaintIndex(0));
            guard.clip_map.insert((0, [0; 8]), ClipIndex(1));
        }
        assert!(arena.paint_map.is_empty());
        assert!(arena.clip_map.is_empty());
    }
}
