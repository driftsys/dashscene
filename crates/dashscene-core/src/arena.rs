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
use std::collections::HashMap;
use std::sync::Arc;

use crate::committed::{
    ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CommittedScene, CornerRadii, ImageAsset,
    ImageTable, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry, Stroke, Vec2,
};

/// Stable handle to a node in one [`Arena`]. Returned by
/// [`Txn::add_node`] and never invalidated (v0.1 has no node removal).
/// Only meaningful for the arena that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub(crate) fn index(self) -> usize {
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
    /// Resolve every node of `arena` to an absolute rect. Omitting a
    /// node is a broken contract: [`Txn::commit_with`] panics rather
    /// than skipping the node silently (P4).
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)>;
}

/// Layout mode of a container node. `None` = passthrough (children
/// place by their authored offsets); `Horizontal`/`Vertical` = flex
/// (the solver owns placement — story #9). Wrap and Grid append at
/// v0.8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    #[default]
    None,
    Horizontal,
    Vertical,
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

/// `Baseline` appends at v0.8 (docs/technotes/open-questions.md, Q-4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
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
    /// Whether the node is drawn and takes part in layout. `false`
    /// lowers to Taffy `Display::None` (`docs/design/dashscene-engine.md`,
    /// issue #165): the node and its descendants are not drawn and take
    /// no space, so siblings reflow. Layout-affecting, like the rest of
    /// the flex vocabulary — ignored by `commit()`'s fixed resolution.
    /// Defaults to `true`.
    Visible(bool),
}

/// Text style intent — mirrors the `dashbuf` `TextStyle` table
/// (family, em size in document units, CSS-scale weight, color)
/// without linking the generated code.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    /// Em size in document units.
    pub size: f32,
    /// CSS-scale weight, 100 to 900 inclusive.
    pub weight: u16,
    pub color: Color,
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
    /// The node's fill, in the full boundary-B vocabulary. `Prop::Fill`
    /// stays as the solid shorthand v0.1 producers use; `Prop::FillWith`
    /// stages a gradient or an image fill.
    fill: Option<PaintKind>,
    stroke: Option<Stroke>,
    corners: CornerRadii,
    /// "This node clips its children to its own box" — intent, resolved
    /// at commit (issue #97).
    clip: bool,
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
    /// The arena owns them because a `.dsb` carries them (`Document.images`),
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
    buffers: [CommittedScene; 2],
    front: usize,
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
    /// vocabulary), by value.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena. A `NodeId` from
    /// another arena whose index happens to be in range is not detected
    /// — ids are only meaningful for the arena that produced them.
    pub fn layout(&self, node: NodeId) -> Layout {
        self.node_data(node).layout
    }

    /// Root nodes in creation order (document DFS root order).
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
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
        // is resolved before its children read it.
        let mut out = Vec::with_capacity(arena.nodes.len());
        let mut absolute = vec![(0.0f32, 0.0f32); arena.nodes.len()];
        for id in arena.dfs_order() {
            let node = &arena.nodes[id.index()];
            let (parent_x, parent_y) = node.parent.map_or((0.0, 0.0), |p| absolute[p.index()]);
            let (x, y) = (parent_x + node.layout.x, parent_y + node.layout.y);
            absolute[id.index()] = (x, y);
            out.push((
                id,
                SolvedRect {
                    x,
                    y,
                    w: node.layout.width,
                    h: node.layout.height,
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
    /// paint table never exceeds the node count plus the one shared
    /// draws-nothing entry).
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
            fill: None,
            stroke: None,
            corners: CornerRadii::default(),
            clip: false,
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

    pub fn set_prop(&mut self, node: NodeId, prop: Prop) {
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
            Prop::Visible(v) => data.layout.visible = v,
        }
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
    pub fn lower_negative_gaps(&mut self) {
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
                // A mode-None container ignores gap entirely; nothing
                // to lower.
                LayoutMode::None => continue,
            };
            let gap = nodes[i].layout.gap;
            nodes[i].layout.gap = 0.0;
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
            }
        }
    }

    /// Resolve the intent model into the back buffer, flip the double
    /// buffer, and return the new generation — using core's internal
    /// fixed-geometry resolution (authored offset + fixed size; flex
    /// intent ignored). Product code with flex layout commits through
    /// [`commit_with`](Txn::commit_with) and a real solver.
    pub fn commit(self) -> u64 {
        self.commit_with(&mut FixedSolver)
    }

    /// Resolve the intent model into the back buffer using `solver`
    /// for every node's geometry, flip the double buffer, and return
    /// the new generation.
    ///
    /// Resolution: DFS walk (roots in creation order, children in
    /// creation order) fixes the rect-table order; geometry comes from
    /// the solver; paints intern by exact bit pattern in first-use
    /// order; clip intent resolves into the per-rect clip regions
    /// boundary B carries (issue #97); the dirty set diffs against the
    /// previous commit. Fully deterministic given a deterministic solver
    /// (R7).
    ///
    /// A rect is dirty when its entry bits changed (the bits a painter
    /// uploads, R-T4), when its resolved paint changed, or when its
    /// resolved clip region changed. The paint and clip tables are both
    /// re-interned every commit, so an unchanged *index* can reference a
    /// different entry (a fill change on the only filled node keeps
    /// index 0; resizing a clipping frame leaves its subtree's clip
    /// index alone), and an index shift can leave the resolved value
    /// unchanged. Only a rect equal on all three counts is clean.
    ///
    /// # Panics
    ///
    /// Panics if the solver omits a node (P4 — a missing rect is a
    /// broken contract, never a silent skip).
    pub fn commit_with(self, solver: &mut dyn LayoutSolver) -> u64 {
        let arena = self.arena;

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

        // Build rects + intern paints and clip regions. Every rect
        // resolves: an unfilled node interns the shared draws-nothing
        // entry (`PaintEntry::default()`) keyed as `None`, and a node no
        // ancestor clips references the reserved unclipped region.
        //
        // The DFS order is parent-before-child, so a node's clip region
        // is known by the time its children are visited.
        let mut rects = Vec::with_capacity(order.len());
        let mut paints = PaintTable::new();
        let mut interned: HashMap<PaintKey, PaintIndex> = HashMap::new();
        let mut clips = ClipTable::new();
        let mut clip_interned: HashMap<ClipKey, ClipIndex> = HashMap::new();
        // `None` until the node is visited, not UNCLIPPED: the clip walk's
        // correctness rests on the DFS order being parent-before-child, and a
        // default of UNCLIPPED would turn a violation of that into a silently
        // unclipped subtree instead of a panic. The parent geometry lookup
        // below already fails loudly for the same reason.
        let mut region_of: Vec<Option<ClipIndex>> = vec![None; arena.nodes.len()];
        let mut rect_index = vec![u32::MAX; arena.nodes.len()];
        for (i, &id) in order.iter().enumerate() {
            let node = &arena.nodes[id.index()];
            let geometry =
                solved[id.index()].unwrap_or_else(|| panic!("solver returned no rect for {id:?}"));
            let entry = PaintEntry {
                fill: node.fill.clone(),
                stroke: node.stroke,
                corners: node.corners,
            };
            let paint = *interned.entry(paint_key(&entry)).or_insert_with(|| {
                // Cannot truncate: the paint table never exceeds the
                // node count (kept below u32::MAX by add_node) plus
                // this one shared entry.
                paints.push(entry.clone())
            });

            // The clip that applies to this node is its parent's, plus
            // the parent's own box when the parent clips. A clipping
            // node does not clip itself — only its descendants.
            let clip = match node.parent {
                Some(parent) if arena.nodes[parent.index()].clip => {
                    let parent_data = &arena.nodes[parent.index()];
                    let parent_geometry = solved[parent.index()]
                        .expect("the parent's rect resolved earlier in the DFS walk");
                    let parent_box = ClipBox {
                        x: parent_geometry.x,
                        y: parent_geometry.y,
                        w: parent_geometry.w,
                        h: parent_geometry.h,
                        corners: parent_data.corners,
                    };
                    intern_region(
                        &mut clips,
                        &mut clip_interned,
                        region_of[parent.index()]
                            .expect("the parent's clip region resolved earlier in the DFS walk"),
                        parent_box,
                    )
                }
                // A non-clipping parent passes its own region through.
                Some(parent) => region_of[parent.index()]
                    .expect("the parent's clip region resolved earlier in the DFS walk"),
                None => ClipIndex::UNCLIPPED,
            };
            region_of[id.index()] = Some(clip);

            rects.push(RectEntry {
                x: geometry.x,
                y: geometry.y,
                w: geometry.w,
                h: geometry.h,
                paint,
                clip,
            });
            // In range for u32 by the add_node guard.
            rect_index[id.index()] = i as u32;
        }

        // Dirty = diff against the previous commit, by index: entry
        // bits, resolved paint, or resolved clip region changed (see the
        // method docs).
        let previous = &arena.buffers[arena.front];
        let dirty = rects
            .iter()
            .enumerate()
            .filter(|&(i, rect)| {
                previous.rects.get(i).is_none_or(|old| {
                    entry_bits(old) != entry_bits(rect)
                        || resolved_paint_key(old, &previous.paints)
                            != resolved_paint_key(rect, &paints)
                        || !same_region_bits(
                            previous.clips.resolve(old.clip),
                            clips.resolve(rect.clip),
                        )
                })
            })
            .map(|(i, _)| i as u32)
            .collect();

        let generation = previous.generation + 1;
        let back = 1 - arena.front;
        arena.buffers[back] = CommittedScene {
            rects,
            paints,
            images: Arc::clone(&arena.images),
            clips,
            generation,
            dirty,
            node_ids: order,
            rect_index,
        };
        arena.front = back;
        generation
    }
}

/// The interning key of one node's clip region: the region its parent
/// resolved to, plus the parent's clip box. Equal ancestor chains take
/// equal keys by induction (the parent's index already stands for its
/// whole chain), so this dedups regions by value at O(1) per node —
/// without hashing a chain-shaped key.
type ClipKey = (u32, [u32; 8]);

fn intern_region(
    clips: &mut ClipTable,
    interned: &mut HashMap<ClipKey, ClipIndex>,
    parent_region: ClipIndex,
    parent_box: ClipBox,
) -> ClipIndex {
    let key = (parent_region.0, clip_box_bits(&parent_box));
    if let Some(&index) = interned.get(&key) {
        return index;
    }
    let mut boxes = clips.resolve(parent_region).boxes().to_vec();
    boxes.push(parent_box);
    let index = clips.push(ClipRegion::new(boxes));
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

    match &entry.fill {
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
    key
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
/// itself and would mark a rect permanently dirty).
fn entry_bits(entry: &RectEntry) -> [u32; 6] {
    [
        entry.x.to_bits(),
        entry.y.to_bits(),
        entry.w.to_bits(),
        entry.h.to_bits(),
        entry.paint.0,
        entry.clip.0,
    ]
}

/// The paint an entry resolves to in its own commit's paint table. Both
/// tables are re-interned every commit, so an unchanged index can resolve
/// to a different entry.
fn resolved_paint_key(entry: &RectEntry, paints: &PaintTable) -> PaintKey {
    paint_key(paints.resolve(entry.paint))
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

/// Whether two resolved clip regions are the same box-for-box, by bit
/// pattern. The clip table is re-interned every commit, so a stable clip
/// index can reference a moved or resized ancestor box — the case a rect
/// whose own entry bits did not change still has to repaint.
fn same_region_bits(a: &ClipRegion, b: &ClipRegion) -> bool {
    a.boxes().len() == b.boxes().len()
        && a.boxes()
            .iter()
            .zip(b.boxes())
            .all(|(x, y)| clip_box_bits(x) == clip_box_bits(y))
}
