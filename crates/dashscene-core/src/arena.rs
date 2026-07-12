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

/// One settable node property. v0.1 vocabulary: the authored
/// parent-relative offset, the fixed size, and the solid fill.
///
/// `Fill` sets a fill but cannot clear one back to unfilled (the
/// shared draws-nothing pool entry) — a deliberate v0.1 gap, recorded
/// in `docs/decisions/staged-mutation-v01-scope.md`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prop {
    X(f32),
    Y(f32),
    Width(f32),
    Height(f32),
    Fill(Color),
}

/// Intent for one node — mirrors the `dashbuf` schema shapes
/// (`FixedSizeLayout`, `SolidFill`) without linking the generated code.
#[derive(Debug)]
struct NodeData {
    name: Option<String>,
    parent: Option<NodeId>,
    /// Creation order; DFS child order at commit.
    children: Vec<NodeId>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill: Option<Color>,
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
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            fill: None,
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
            Prop::X(v) => data.x = v,
            Prop::Y(v) => data.y = v,
            Prop::Width(v) => data.width = v,
            Prop::Height(v) => data.height = v,
            Prop::Fill(c) => data.fill = Some(c),
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
            let (x, y) = (parent_x + node.x, parent_y + node.y);
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
                w: node.width,
                h: node.height,
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
