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
//! let built = scene([
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
//! assert_eq!(arena.committed().generation(), built.generation());
//! assert_eq!(arena.committed().rects().len(), 4);
//! assert_eq!(arena.committed().paints().len(), 2);
//! ```

use dashscene_core::{EdgeInsets, Layout, LayoutSolver, NodeId, Prop, Txn};

mod reactive;

// The reactive layer (issue #166): signals, bindings, transforms, and
// the per-frame flush. Declared on this crate's `Node`/`Scene`, so the
// authoring surface is one import path. `attach_live` (story #167) is
// the loader-side entry point: it builds a `LiveScene` from the binding
// tables a loaded document staged into the arena.
pub use reactive::{
    Channel, ClosureId, FormatSpec, LiveScene, Mapped, ScalarExpr, Signal, SignalValue, Spring,
    TextExpr, Transform, attach_live,
};

// A DSL consumer needs an `Arena` to build into, a `Color` to fill
// with, and the v0.2 flex vocabulary's enums; re-exporting all of them
// keeps authoring and solving a scene (via `build`/`build_with` with an
// existing solver, e.g. `dashscene-engine`'s `TaffySolver`) a
// one-import-path surface, with no direct `dashscene-core` dependency
// required downstream. Implementing a *custom* `LayoutSolver` still
// needs `dashscene_core::{LayoutSolver, NodeId, SolvedRect}` directly —
// deliberately not re-exported, so `NodeId` stays a type no `dashlang`
// producer ever names (see `crates/dashlang/tests/builder.rs`'s
// `DoubleWidthSolver` for exactly this case).
pub use dashscene_core::{
    Arena, AxisSizing, Color, CrossAxisAlign, GridTrack, LayoutMode, MainAxisAlign,
};

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
        ..Scene::default()
    }
}

/// One node's description: authored offset, fixed size, optional
/// solid fill, children in declaration order, and the v0.2 flex
/// vocabulary (issue #118) — `mode`, `gap`, `padding`, `margin`,
/// `main_align`, `cross_align`, `sizing_h`, `sizing_v`, `min_width`,
/// `max_width`, `min_height`, `max_height`. Inert until
/// [`Scene::build`] — constructing and combining descriptions stages
/// nothing. Each setter's own doc comment covers what it sets; the full
/// vocabulary, including the later v0.8 grid/wrap additions, is also
/// listed in `docs/design/dashlang.md`'s "Value-tree surface" section.
///
/// Unset values keep `dashscene-core`'s defaults: zero offset and
/// size, no fill.
#[derive(Debug, Default)]
pub struct Node {
    name: Option<String>,
    layout: Layout,
    // Grid track templates (story #43). They live beside `Layout` in the
    // arena because they are variable-length and `Layout` is `Copy`, so
    // they are separate fields here too. Empty = no grid tracks authored.
    grid_rows: Vec<GridTrack>,
    grid_columns: Vec<GridTrack>,
    fill: Option<Color>,
    children: Children,
    // Reactive declarations (issue #166), resolved to targets at build.
    // Inert for the non-live `build`/`build_with` paths.
    scalar_bindings: Vec<(Channel, ScalarExpr)>,
    smoothing: Vec<(Channel, Spring)>,
    text_binding: Option<TextExpr>,
    visible_binding: Option<u32>,
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

    /// Spacing between wrap lines and between grid rows (story #43).
    /// Unset follows `gap`, core's default.
    pub fn cross_gap(mut self, gap: f32) -> Self {
        self.layout.cross_gap = Some(gap);
        self
    }

    /// The row track template of a `Grid` container (story #43).
    pub fn grid_rows(mut self, tracks: impl IntoIterator<Item = GridTrack>) -> Self {
        self.grid_rows = tracks.into_iter().collect();
        self
    }

    /// The column track template of a `Grid` container (story #43).
    pub fn grid_columns(mut self, tracks: impl IntoIterator<Item = GridTrack>) -> Self {
        self.grid_columns = tracks.into_iter().collect();
        self
    }

    /// The 0-based grid row cell this child anchors to (story #43).
    /// Unset auto-places in document order.
    pub fn grid_row(mut self, anchor: u16) -> Self {
        self.layout.grid_row = Some(anchor);
        self
    }

    /// The 0-based grid column cell this child anchors to (story #43).
    /// Unset auto-places in document order.
    pub fn grid_column(mut self, anchor: u16) -> Self {
        self.layout.grid_column = Some(anchor);
        self
    }

    /// How many grid rows this child spans (story #43). Core default 1.
    pub fn grid_row_span(mut self, span: u16) -> Self {
        self.layout.grid_row_span = span;
        self
    }

    /// How many grid columns this child spans (story #43). Core default 1.
    pub fn grid_column_span(mut self, span: u16) -> Self {
        self.layout.grid_column_span = span;
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

/// A node's owned children, wrapped so [`Node`] itself stays free of a
/// custom `Drop` impl. Rust forbids moving a field out of a value whose
/// own type implements `Drop`, and two existing call sites rely on
/// moving `Node`'s fields: [`node`]'s `..Node::default()` struct update,
/// and the reactive build path's field-move destructuring
/// (`stage_live` in `reactive.rs`). Neither needs to change, because the
/// type that needs the iterative drop (issue #79) is this wrapper, not
/// `Node`.
#[derive(Debug, Default)]
struct Children(Vec<Node>);

impl std::ops::Deref for Children {
    type Target = Vec<Node>;

    fn deref(&self) -> &Vec<Node> {
        &self.0
    }
}

impl std::ops::DerefMut for Children {
    fn deref_mut(&mut self) -> &mut Vec<Node> {
        &mut self.0
    }
}

impl Children {
    /// Takes the child vector, leaving an empty one behind. A plain
    /// `self.0` move is not available here: `Children` implements
    /// `Drop`, and Rust forbids moving a field out of a value that does.
    /// Going through `&mut self` instead is allowed, because `self`
    /// itself never becomes partially moved.
    fn take(&mut self) -> Vec<Node> {
        std::mem::take(&mut self.0)
    }
}

impl IntoIterator for Children {
    type Item = Node;
    type IntoIter = std::vec::IntoIter<Node>;

    fn into_iter(mut self) -> Self::IntoIter {
        self.take().into_iter()
    }
}

/// Drops a node's children without recursing per tree level (issue
/// #79). The derived drop of `Vec<Node>` recurses once per level, which
/// overflows the stack on a deep enough chain (a corpus-generator shape,
/// `docs/design/architecture.md` §6.2). This moves every child onto one
/// heap work-stack before it is allowed to drop, so no `Node` ever
/// reaches its own field drop with a non-empty `children`: each pop
/// drops in O(1) stack depth, in the same order the derived drop would
/// have used.
impl Drop for Children {
    fn drop(&mut self) {
        let mut pending: Vec<Node> = self.take();
        while let Some(mut node) = pending.pop() {
            pending.extend(node.children.take());
            // `node` drops here with an empty `children`, so the `Drop`
            // above runs on an already-empty vector — no recursion.
        }
    }
}

/// A scene description, built from [`scene`] or the [`Scene::new`]
/// builder. Carries the signal declarations the reactive layer (issue
/// #166) resolves at [`Scene::build_live`]; empty for the non-live
/// paths.
#[derive(Debug, Default)]
pub struct Scene {
    roots: Vec<Node>,
    // Signal initial values, in declaration order. `SignalValue::declare`
    // pushes here; `build_live` moves them into the `LiveScene`.
    scalar_inits: Vec<f32>,
    // The scalar signals' runtime lookup names, parallel to
    // `scalar_inits`. `None` for a signal declared through
    // `Scene::signal`; `Some` through `Scene::signal_named` (story #167,
    // the name staged into the document binding table).
    scalar_names: Vec<Option<String>>,
    bool_inits: Vec<bool>,
}

impl Scene {
    /// An empty scene builder — declare signals with [`Scene::signal`],
    /// then set roots with [`Scene::roots`].
    pub fn new() -> Self {
        Self::default()
    }
}

/// The result of one [`Scene::build`]/[`Scene::build_with`] commit. A
/// thin wrapper around the commit generation today — the seam issue
/// #166's reactive layer is designed to extend into a live, bindable
/// scene handle by wrapping a `Built` value, without a second change to
/// `build`'s return type itself
/// (`docs/decisions/dashlang-flex-vocabulary.md` D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Built {
    generation: u64,
}

impl Built {
    /// The commit's generation (`CommittedScene::generation`).
    pub fn generation(self) -> u64 {
        self.generation
    }
}

impl Scene {
    /// Adds this description's roots to `arena` (appending to whatever
    /// the arena already holds — the DSL is a producer, not an owner)
    /// and publishes them in exactly one commit, using core's internal
    /// fixed-geometry resolution (flex intent ignored). A scene with
    /// flex intent commits through [`Scene::build_with`] and a real
    /// solver.
    ///
    /// An empty scene still commits: the generation increments, and
    /// changes staged by a previously dropped `Txn` publish (core's
    /// batched-publish staging).
    pub fn build(&self, arena: &mut Arena) -> Built {
        Built {
            generation: self.stage(arena).commit(),
        }
    }

    /// Adds this description's roots to `arena` and publishes them in
    /// exactly one commit, using `solver` for every node's geometry —
    /// the entry point a flex scene needs (`dashscene-engine`'s
    /// `TaffySolver` being the product case). The solver stays injected:
    /// the caller chooses it, and this crate never constructs one.
    pub fn build_with(&self, arena: &mut Arena, solver: &mut dyn LayoutSolver) -> Built {
        Built {
            generation: self.stage(arena).commit_with(solver),
        }
    }

    fn stage<'a>(&self, arena: &'a mut Arena) -> Txn<'a> {
        let mut txn = arena.open();
        for root in &self.roots {
            add(&mut txn, None, root);
        }
        txn
    }
}

// Explicit-stack depth-first walk (issue #79): the equivalent recursive
// form calls itself once per tree level, which overflows the stack on a
// deep enough chain (a corpus-generator shape, `docs/design/
// architecture.md` §6.2). `pending` holds the same (parent, node) pairs
// a recursive call's stack frames would, so this stages nodes in the
// identical document (DFS, declaration) order: pushing a node's children
// in reverse means the next pop is always its first child, so a whole
// subtree is staged before the next sibling, exactly as the recursive
// form would.
fn add(txn: &mut Txn<'_>, parent: Option<NodeId>, node: &Node) {
    let mut pending: Vec<(Option<NodeId>, &Node)> = vec![(parent, node)];
    while let Some((parent, node)) = pending.pop() {
        let id = txn.add_node(parent, node.name.as_deref());
        set_base_props(txn, id, node);
        for child in node.children.iter().rev() {
            pending.push((Some(id), child));
        }
    }
}

/// Stage the base (non-reactive) props for one already-added node — the
/// authored geometry, flex vocabulary, and fill. Shared by the non-live
/// [`Scene::build`] path ([`add`]) and the reactive `build_live` path,
/// so a node's base props are set one way only.
pub(crate) fn set_base_props(txn: &mut Txn<'_>, id: NodeId, node: &Node) {
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
    // The v0.8 grid/wrap vocabulary (story #43). Emitted only when
    // authored, so a non-grid node stages exactly the props it did before
    // this vocabulary existed (the unset-defaults acceptance tests).
    if let Some(v) = node.layout.cross_gap {
        txn.set_prop(id, Prop::CrossGap(v));
    }
    if !node.grid_rows.is_empty() {
        txn.set_prop(id, Prop::GridRows(node.grid_rows.clone()));
    }
    if !node.grid_columns.is_empty() {
        txn.set_prop(id, Prop::GridColumns(node.grid_columns.clone()));
    }
    if let Some(v) = node.layout.grid_row {
        txn.set_prop(id, Prop::GridRow(v));
    }
    if let Some(v) = node.layout.grid_column {
        txn.set_prop(id, Prop::GridColumn(v));
    }
    if node.layout.grid_row_span != Layout::default().grid_row_span {
        txn.set_prop(id, Prop::GridRowSpan(node.layout.grid_row_span));
    }
    if node.layout.grid_column_span != Layout::default().grid_column_span {
        txn.set_prop(id, Prop::GridColumnSpan(node.layout.grid_column_span));
    }
    if let Some(color) = node.fill {
        txn.set_prop(id, Prop::Fill(color));
    }
}
