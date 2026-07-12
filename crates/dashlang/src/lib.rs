//! Rust DSL skin over the `dashscene-core` producer surface, and the
//! future stress-corpus generator (DESIGN_1.md §6.2,
//! SCOPE_DECISIONS.md §9).
//!
//! The DSL builds an inert value tree ([`Node`]) and publishes it in
//! one staged commit via [`Scene::build`] — components are plain
//! functions returning [`Node`] values, loops are iterators feeding
//! [`Node::children`]. The DSL adds vocabulary, never semantics:
//! anything it expresses is expressible by hand against
//! `dashscene-core`, with identical committed output.
//!
//! ```
//! use dashlang::{node, rgba, scene};
//!
//! let mut arena = dashscene_core::Arena::new();
//! let generation = scene([
//!     node("bg")
//!         .size(320.0, 240.0)
//!         .fill(rgba(0.1, 0.2, 0.3, 1.0))
//!         .children((0..3).map(|i| {
//!             node("badge")
//!                 .at(10.0 + 30.0 * i as f32, 10.0)
//!                 .size(24.0, 24.0)
//!                 .fill(rgba(1.0, 0.0, 0.0, 1.0))
//!         })),
//! ])
//! .build(&mut arena);
//!
//! assert_eq!(arena.committed().generation(), generation);
//! assert_eq!(arena.committed().rects().len(), 4);
//! assert_eq!(arena.committed().paints().len(), 2);
//! ```

use dashscene_core::{Arena, Color, NodeId, Prop, Txn};

/// A named node description. See [`anon`] for unnamed nodes.
pub fn node(name: &str) -> Node {
    Node {
        name: Some(name.to_owned()),
        ..Node::default()
    }
}

/// An unnamed node description.
pub fn anon() -> Node {
    Node::default()
}

/// A solid-fill color (plain constructor for `dashscene_core::Color`).
pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

/// A scene description: the roots to add to an arena, in order.
pub fn scene(roots: impl IntoIterator<Item = Node>) -> Scene {
    Scene {
        roots: roots.into_iter().collect(),
    }
}

/// One node's description: authored offset, fixed size, optional
/// solid fill, children in declaration order. Inert until
/// [`Scene::build`] — constructing and combining descriptions stages
/// nothing.
///
/// Unset values keep `dashscene-core`'s defaults: zero offset and
/// size, no fill.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    name: Option<String>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill: Option<Color>,
    children: Vec<Node>,
}

impl Node {
    /// Authored offset relative to the parent (canvas origin for a
    /// root).
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Appends one child. Declaration order is document (DFS) order.
    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// Appends children from an iterator (the DSL's repeater),
    /// preserving iteration order.
    pub fn children(mut self, children: impl IntoIterator<Item = Node>) -> Self {
        self.children.extend(children);
        self
    }
}

/// A scene description, built from [`scene`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    roots: Vec<Node>,
}

impl Scene {
    /// Adds this description's roots to `arena` (appending to whatever
    /// the arena already holds — the DSL is a producer, not an owner)
    /// and publishes them in exactly one commit. Returns the commit's
    /// generation.
    pub fn build(&self, arena: &mut Arena) -> u64 {
        let mut txn = arena.open();
        for root in &self.roots {
            add(&mut txn, None, root);
        }
        txn.commit()
    }
}

fn add(txn: &mut Txn<'_>, parent: Option<NodeId>, node: &Node) {
    let id = txn.add_node(parent, node.name.as_deref());
    txn.set_prop(id, Prop::X(node.x));
    txn.set_prop(id, Prop::Y(node.y));
    txn.set_prop(id, Prop::Width(node.width));
    txn.set_prop(id, Prop::Height(node.height));
    if let Some(color) = node.fill {
        txn.set_prop(id, Prop::Fill(color));
    }
    for child in &node.children {
        add(txn, Some(id), child);
    }
}
