//! Rust DSL skin over the `dashscene-core` producer surface, and the
//! future stress-corpus generator (docs/design/architecture.md,
//! docs/decisions/staged-mutation-v01-scope.md).
//!
//! The DSL builds an inert value tree ([`Node`]) and publishes it in
//! one staged commit via [`Scene::build`] — components are plain
//! functions returning [`Node`] values, loops are iterators feeding
//! [`Node::children`]. The DSL adds vocabulary, never semantics:
//! anything it expresses is expressible by hand against
//! `dashscene-core`, with identical committed output.
//!
//! ```
//! use dashlang::{Arena, node, rgba, scene};
//!
//! let mut arena = Arena::new();
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

use dashscene_core::{EdgeInsets, Layout, NodeId, Prop, Txn};

// A DSL consumer needs an `Arena` to build into, a `Color` to fill
// with, and the v0.2 flex vocabulary's enums; re-exporting all of
// them keeps `dashlang` a one-import-path surface (no direct
// `dashscene-core` dependency required downstream).
pub use dashscene_core::{Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign};

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
#[derive(Debug, Default)]
pub struct Node {
    name: Option<String>,
    layout: Layout,
    fill: Option<Color>,
    children: Vec<Node>,
}

impl Node {
    /// Authored offset relative to the parent (canvas origin for a
    /// root).
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.layout.x = x;
        self.layout.y = y;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.layout.width = width;
        self.layout.height = height;
        self
    }

    /// Container layout mode: `None` (passthrough — children place by
    /// their authored offsets), or `Horizontal`/`Vertical` flex.
    pub fn mode(mut self, mode: LayoutMode) -> Self {
        self.layout.mode = mode;
        self
    }

    /// Gap between children along the main axis, under a flex mode.
    pub fn gap(mut self, gap: f32) -> Self {
        self.layout.gap = gap;
        self
    }

    /// Inner padding, all four edges.
    pub fn padding(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout.padding = EdgeInsets {
            left,
            top,
            right,
            bottom,
        };
        self
    }

    /// Outer margin in the parent's flow, all four edges. Flex-flow
    /// vocabulary only: inert on a root or under a mode-`None` parent.
    pub fn margin(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.layout.margin = EdgeInsets {
            left,
            top,
            right,
            bottom,
        };
        self
    }

    /// Alignment along the container's main axis.
    pub fn main_align(mut self, align: MainAxisAlign) -> Self {
        self.layout.main_align = align;
        self
    }

    /// Alignment along the container's cross axis.
    pub fn cross_align(mut self, align: CrossAxisAlign) -> Self {
        self.layout.cross_align = align;
        self
    }

    /// How this node sizes itself horizontally under a flex parent.
    pub fn sizing_h(mut self, sizing: AxisSizing) -> Self {
        self.layout.sizing_h = sizing;
        self
    }

    /// How this node sizes itself vertically under a flex parent.
    pub fn sizing_v(mut self, sizing: AxisSizing) -> Self {
        self.layout.sizing_v = sizing;
        self
    }

    /// Minimum width clamp. Cannot be unset once set — core has no clear
    /// operation for it (same gap as `fill`).
    pub fn min_width(mut self, v: f32) -> Self {
        self.layout.min_width = Some(v);
        self
    }

    /// Maximum width clamp. Cannot be unset once set.
    pub fn max_width(mut self, v: f32) -> Self {
        self.layout.max_width = Some(v);
        self
    }

    /// Minimum height clamp. Cannot be unset once set.
    pub fn min_height(mut self, v: f32) -> Self {
        self.layout.min_height = Some(v);
        self
    }

    /// Maximum height clamp. Cannot be unset once set.
    pub fn max_height(mut self, v: f32) -> Self {
        self.layout.max_height = Some(v);
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
#[derive(Debug)]
pub struct Scene {
    roots: Vec<Node>,
}

impl Scene {
    /// Adds this description's roots to `arena` (appending to whatever
    /// the arena already holds — the DSL is a producer, not an owner)
    /// and publishes them in exactly one commit. Returns the commit's
    /// generation.
    ///
    /// An empty scene still commits: the generation increments, and
    /// changes staged by a previously dropped `Txn` publish (core's
    /// batched-publish staging).
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
    txn.set_prop(id, Prop::X(node.layout.x));
    txn.set_prop(id, Prop::Y(node.layout.y));
    txn.set_prop(id, Prop::Width(node.layout.width));
    txn.set_prop(id, Prop::Height(node.layout.height));
    txn.set_prop(id, Prop::Mode(node.layout.mode));
    txn.set_prop(id, Prop::Gap(node.layout.gap));
    txn.set_prop(
        id,
        Prop::Padding {
            left: node.layout.padding.left,
            top: node.layout.padding.top,
            right: node.layout.padding.right,
            bottom: node.layout.padding.bottom,
        },
    );
    txn.set_prop(
        id,
        Prop::Margin {
            left: node.layout.margin.left,
            top: node.layout.margin.top,
            right: node.layout.margin.right,
            bottom: node.layout.margin.bottom,
        },
    );
    txn.set_prop(id, Prop::MainAlign(node.layout.main_align));
    txn.set_prop(id, Prop::CrossAlign(node.layout.cross_align));
    txn.set_prop(id, Prop::SizingH(node.layout.sizing_h));
    txn.set_prop(id, Prop::SizingV(node.layout.sizing_v));
    if let Some(v) = node.layout.min_width {
        txn.set_prop(id, Prop::MinWidth(v));
    }
    if let Some(v) = node.layout.max_width {
        txn.set_prop(id, Prop::MaxWidth(v));
    }
    if let Some(v) = node.layout.min_height {
        txn.set_prop(id, Prop::MinHeight(v));
    }
    if let Some(v) = node.layout.max_height {
        txn.set_prop(id, Prop::MaxHeight(v));
    }
    if let Some(color) = node.fill {
        txn.set_prop(id, Prop::Fill(color));
    }
    for child in &node.children {
        add(txn, Some(id), child);
    }
}
