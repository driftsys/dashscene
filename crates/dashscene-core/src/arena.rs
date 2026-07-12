//! The in-memory arena and its staged mutation API
//! (DESIGN_1.md §4/§5, SCOPE_DECISIONS.md §9).
//!
//! Producers mutate through a [`Txn`] obtained from [`Arena::open`];
//! nothing becomes visible to painters until [`Txn::commit`] resolves
//! the intent model into the committed output (P3 — producers mutate,
//! the runtime owns time). One `Txn` at a time, enforced by the borrow
//! checker. Dropping a `Txn` without committing leaves the staged
//! changes pending; they publish with the next commit ("staged" means
//! batched visibility, not rollback).

use std::collections::HashMap;

use crate::committed::{
    Color, CommittedScene, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry,
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

/// `Baseline` appends at v0.8 (DESIGN_1.md Q-4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
}

/// One node's layout intent — the authored fixed geometry plus the
/// v0.2 flex vocabulary. Mirrors the `dashbuf` schema shapes
/// (`FixedSizeLayout`, `LayoutContainer`, `LayoutConstraints`) without
/// linking the generated code. Stored intent: until story #9's Taffy
/// solve, `commit` resolves the fixed geometry only.
#[derive(Clone, Copy, Debug, Default)]
pub struct Layout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub mode: LayoutMode,
    pub gap: f32,
    /// Left, top, right, bottom.
    pub padding: [f32; 4],
    pub main_align: MainAxisAlign,
    pub cross_align: CrossAxisAlign,
    pub sizing_h: AxisSizing,
    pub sizing_v: AxisSizing,
    /// `None` = unconstrained (absence of intent, not a sentinel).
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
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
    Fill(Color),
    /// Set/replace the node's text content (DESIGN §5: strings, never
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
    MainAlign(MainAxisAlign),
    CrossAlign(CrossAxisAlign),
    SizingH(AxisSizing),
    SizingV(AxisSizing),
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
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
    fill: Option<Color>,
    text: Option<String>,
    text_style: Option<TextStyle>,
}

/// The semantic model: the node tree with layout + paint intent, and
/// the double-buffered committed output painters read.
#[derive(Debug, Default)]
pub struct Arena {
    nodes: Vec<NodeData>,
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

    fn node_data(&self, node: NodeId) -> &NodeData {
        self.nodes
            .get(node.index())
            .unwrap_or_else(|| panic!("{node:?} is not a node of this arena"))
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
            Prop::Fill(c) => data.fill = Some(c),
            Prop::Text(s) => data.text = Some(s),
            Prop::TextStyle(ts) => data.text_style = Some(ts),
            Prop::Mode(m) => data.layout.mode = m,
            Prop::Gap(v) => data.layout.gap = v,
            Prop::Padding {
                left,
                top,
                right,
                bottom,
            } => data.layout.padding = [left, top, right, bottom],
            Prop::MainAlign(a) => data.layout.main_align = a,
            Prop::CrossAlign(a) => data.layout.cross_align = a,
            Prop::SizingH(v) => data.layout.sizing_h = v,
            Prop::SizingV(v) => data.layout.sizing_v = v,
            Prop::MinWidth(v) => data.layout.min_width = Some(v),
            Prop::MaxWidth(v) => data.layout.max_width = Some(v),
            Prop::MinHeight(v) => data.layout.min_height = Some(v),
            Prop::MaxHeight(v) => data.layout.max_height = Some(v),
        }
    }

    /// Resolve the intent model into the back buffer, flip the double
    /// buffer, and return the new generation.
    ///
    /// Resolution: DFS walk (roots in creation order, children in
    /// creation order), absolute position = parent absolute + own
    /// authored offset, paints interned by exact color bit pattern in
    /// first-use order, dirty set diffed against the previous commit.
    /// Fully deterministic (R7).
    ///
    /// A rect is dirty when its entry bits changed (the bits a painter
    /// uploads, R-T4) or when its resolved fill color changed — the
    /// paint table is re-interned every commit, so an unchanged paint
    /// *index* can reference a different color and an index shift can
    /// leave the color unchanged; both cases count as dirty, and only
    /// a rect equal on both counts is clean.
    pub fn commit(self) -> u64 {
        let arena = self.arena;

        // DFS document order (rect-table index order).
        let mut order = Vec::with_capacity(arena.nodes.len());
        let mut stack: Vec<NodeId> = arena.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            order.push(id);
            stack.extend(arena.nodes[id.index()].children.iter().rev());
        }

        // Resolve rects + intern paints. Every rect resolves: an
        // unfilled node interns the shared draws-nothing entry
        // (`PaintEntry::default()`) keyed as `None`.
        let mut absolute = vec![(0.0f32, 0.0f32); arena.nodes.len()];
        let mut rects = Vec::with_capacity(order.len());
        let mut paints = PaintTable::new();
        let mut interned: HashMap<Option<[u32; 4]>, PaintIndex> = HashMap::new();
        let mut rect_index = vec![u32::MAX; arena.nodes.len()];
        for (i, &id) in order.iter().enumerate() {
            let node = &arena.nodes[id.index()];
            let (parent_x, parent_y) = node.parent.map_or((0.0, 0.0), |p| absolute[p.index()]);
            let (x, y) = (parent_x + node.layout.x, parent_y + node.layout.y);
            absolute[id.index()] = (x, y);
            let paint =
                *interned
                    .entry(node.fill.map(color_key))
                    .or_insert_with(|| match node.fill {
                        // Cannot truncate: the paint table never exceeds
                        // the node count (kept below u32::MAX by add_node)
                        // plus this one shared entry.
                        None => paints.push(PaintEntry::default()),
                        Some(color) => paints.push(PaintEntry::solid(color)),
                    });
            rects.push(RectEntry {
                x,
                y,
                w: node.layout.width,
                h: node.layout.height,
                paint,
            });
            // In range for u32 by the add_node guard.
            rect_index[id.index()] = i as u32;
        }

        // Dirty = diff against the previous commit, by index: entry
        // bits or resolved fill color changed (see the method docs).
        let previous = &arena.buffers[arena.front];
        let dirty = rects
            .iter()
            .enumerate()
            .filter(|&(i, rect)| {
                previous.rects.get(i).is_none_or(|old| {
                    entry_bits(old) != entry_bits(rect)
                        || resolved_color_bits(old, &previous.paints)
                            != resolved_color_bits(rect, &paints)
                })
            })
            .map(|(i, _)| i as u32)
            .collect();

        let generation = previous.generation + 1;
        let back = 1 - arena.front;
        arena.buffers[back] = CommittedScene {
            rects,
            paints,
            generation,
            dirty,
            node_ids: order,
            rect_index,
        };
        arena.front = back;
        generation
    }
}

fn color_key(color: Color) -> [u32; 4] {
    [
        color.r.to_bits(),
        color.g.to_bits(),
        color.b.to_bits(),
        color.a.to_bits(),
    ]
}

/// The bits a painter uploads for an entry (R-T4). Bit comparison keeps
/// the diff deterministic where `f32` equality is not (NaN never equals
/// itself and would mark a rect permanently dirty).
fn entry_bits(entry: &RectEntry) -> [u32; 5] {
    [
        entry.x.to_bits(),
        entry.y.to_bits(),
        entry.w.to_bits(),
        entry.h.to_bits(),
        entry.paint.0,
    ]
}

/// The fill color an entry resolves to in its own commit's paint table
/// (`None` for a fill-less entry), compared by bit pattern like the
/// interner.
fn resolved_color_bits(entry: &RectEntry, paints: &PaintTable) -> Option<[u32; 4]> {
    match paints.resolve(entry.paint).fill {
        Some(PaintKind::Solid { color }) => Some(color_key(color)),
        Some(_) => unreachable!(
            "widen resolved_color_bits when producers can stage non-solid fills (v0.3+ vocabulary)"
        ),
        None => None,
    }
}
