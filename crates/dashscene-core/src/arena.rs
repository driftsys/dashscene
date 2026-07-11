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

use crate::committed::{Color, CommittedScene, NO_PAINT, Paint, RectEntry};

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
    /// Panics if `node` does not belong to this arena.
    pub fn name(&self, node: NodeId) -> Option<&str> {
        self.nodes[node.index()].name.as_deref()
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
    /// Panics if `parent` does not belong to this arena.
    pub fn add_node(&mut self, parent: Option<NodeId>, name: Option<&str>) -> NodeId {
        if let Some(p) = parent {
            assert!(
                p.index() < self.arena.nodes.len(),
                "parent {p:?} is not a node of this arena"
            );
        }
        let id = NodeId(u32::try_from(self.arena.nodes.len()).expect("node count exceeds u32"));
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
    /// Panics if `node` does not belong to this arena.
    pub fn set_prop(&mut self, node: NodeId, prop: Prop) {
        let data = &mut self.arena.nodes[node.index()];
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
    /// first-use order, dirty set = exact entry diff against the
    /// previous commit. Fully deterministic (R7).
    pub fn commit(self) -> u64 {
        let arena = self.arena;

        // DFS document order (rect-table index order).
        let mut order = Vec::with_capacity(arena.nodes.len());
        let mut stack: Vec<NodeId> = arena.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            order.push(id);
            stack.extend(arena.nodes[id.index()].children.iter().rev());
        }

        // Resolve rects + intern paints.
        let mut absolute = vec![(0.0f32, 0.0f32); arena.nodes.len()];
        let mut rects = Vec::with_capacity(order.len());
        let mut paints = Vec::new();
        let mut interned: HashMap<[u32; 4], u32> = HashMap::new();
        let mut rect_index = vec![u32::MAX; arena.nodes.len()];
        for (i, &id) in order.iter().enumerate() {
            let node = &arena.nodes[id.index()];
            let (parent_x, parent_y) = node.parent.map_or((0.0, 0.0), |p| absolute[p.index()]);
            let (x, y) = (parent_x + node.x, parent_y + node.y);
            absolute[id.index()] = (x, y);
            let paint = match node.fill {
                None => NO_PAINT,
                Some(color) => *interned.entry(color_key(color)).or_insert_with(|| {
                    paints.push(Paint { color });
                    u32::try_from(paints.len() - 1).expect("paint count exceeds u32")
                }),
            };
            rects.push(RectEntry {
                x,
                y,
                w: node.width,
                h: node.height,
                paint,
            });
            rect_index[id.index()] = u32::try_from(i).expect("node count exceeds u32");
        }

        // Dirty = exact diff against the previous commit, by index.
        let previous = &arena.buffers[arena.front];
        let dirty = rects
            .iter()
            .enumerate()
            .filter(|&(i, rect)| previous.rects.get(i) != Some(rect))
            .map(|(i, _)| u32::try_from(i).expect("node count exceeds u32"))
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
