//! Runtime that resolves the model — the Taffy layout solve
//! (docs/design/architecture.md; variants, FLIP, and the measure callback land at
//! their own slices).
//!
//! [`TaffySolver`] implements `dashscene-core`'s `LayoutSolver` seam:
//! it maps the arena's layout intent to a Taffy tree (one tree per
//! root — roots are independent coordinate islands), solves, and
//! returns absolute rects. Taffy is the sole solver (P2); layout mode
//! `None` is a passthrough expressed as absolutely-positioned children,
//! not a second engine.
//!
//! The tree is **retained** across solves (issue #164). The first solve
//! builds it and reports every node; a later solve marks only the nodes
//! whose layout intent changed (via `set_style`, which clears a node and
//! its ancestors), lets Taffy recompute just those subtrees, and reads
//! back only the rects whose absolute position or size actually moved.
//! A commit whose changes are all paint-only marks nothing and performs
//! no solve at all. A structural change — the node count grew — rebuilds
//! the tree, since the arena's DFS indices have shifted underneath it.

use dashscene_core::{Arena, AxisSizing, Layout, LayoutMode, LayoutSolver, NodeId, SolvedRect};
use dashscene_typeset::text::Typesetter;
use rustc_hash::FxHashSet;
use taffy::prelude::*;
use taffy::{AlignItems, AlignSelf, JustifyContent, Position};

/// The Taffy implementation of `dashscene-core`'s `LayoutSolver`.
///
/// The typesetter is borrowed, never owned: the caller keeps one
/// [`Typesetter`] for the whole runtime and lends it here for the
/// solve, so the measure callback and the painter (#30) read one
/// shaped-run cache and cannot disagree about a glyph's size. A solver
/// built with [`new`](TaffySolver::new) carries no typesetter and
/// solves a text-free scene exactly as before; text nodes in such a
/// scene are simply not measured.
///
/// A solver retains its Taffy tree across solves (issue #164), so it is
/// bound to one arena for its lifetime: reusing a solver against a
/// different arena would read a mismatched tree. The runtime keeps one
/// solver per arena, the same way it keeps one typesetter.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TaffySolver<'a> {
    typesetter: Option<&'a mut Typesetter>,
    /// The retained tree and its per-node bookkeeping. `None` until the
    /// first solve builds it.
    state: Option<TreeState>,
    /// How many Taffy layout computations this solver has run. A commit
    /// whose changes are all paint-only leaves this unchanged — the whole
    /// point of the retained tree (issue #164). Read via
    /// [`solves`](TaffySolver::solves).
    solves: u64,
}

/// The retained Taffy tree plus the maps that let an incremental solve
/// find each arena node, walk to its ancestors, and tell whether its
/// resolved rect moved since last time. All the per-node vectors are
/// indexed by [`NodeId`] slot; `roots` and `prev_root_origin` follow
/// arena root order.
#[derive(Debug)]
struct TreeState {
    tree: TaffyTree<TextContext>,
    /// The Taffy node standing for each arena node.
    taffy_of: Vec<taffy::NodeId>,
    /// Each arena node's parent (its root has `None`).
    parent_of: Vec<Option<NodeId>>,
    /// The Taffy roots, in arena root order.
    roots: Vec<taffy::NodeId>,
    /// The previous solve's Taffy-relative layout per node, as bit
    /// patterns: `[location.x, location.y, size.width, size.height]`. Bits
    /// so the compare stays deterministic where `f32` equality is not.
    prev_rel: Vec<[u32; 4]>,
    /// The previous solve's authored root origin per root — the offset the
    /// readback adds, which Taffy does not model, so a root move is
    /// detected here rather than in the tree.
    prev_root_origin: Vec<[u32; 2]>,
    /// The node count when the tree was built. A mismatch is a structural
    /// change and forces a rebuild.
    node_count: usize,
}

impl<'a> TaffySolver<'a> {
    /// A solver with no typesetter — for scenes without text-driven
    /// sizing. A hug-sized text node solved this way is not measured
    /// (it has no font to shape with) and sizes as an empty leaf.
    pub fn new() -> Self {
        Self {
            typesetter: None,
            state: None,
            solves: 0,
        }
    }

    /// A solver that measures text nodes against `typesetter`'s
    /// shaped-run cache. The borrow keeps the cache single-sourced: the
    /// same `Typesetter` the caller lends here is the one the painter
    /// reads at paint time (#30).
    pub fn with_typesetter(typesetter: &'a mut Typesetter) -> Self {
        Self {
            typesetter: Some(typesetter),
            state: None,
            solves: 0,
        }
    }

    /// How many Taffy layout computations this solver has run. It stays
    /// put across a paint-only commit — the retained tree is not
    /// recomputed when no layout intent changed (issue #164) — so a test
    /// can assert a paint-only frame did no solve.
    pub fn solves(&self) -> u64 {
        self.solves
    }
}

impl LayoutSolver for TaffySolver<'_> {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        let TaffySolver {
            typesetter,
            state,
            solves,
        } = self;
        // A grown node count means the arena's DFS indices shifted under
        // the retained tree; rebuild rather than patch.
        let structural = state
            .as_ref()
            .is_none_or(|s| s.node_count != arena.node_count());
        if structural {
            let (new_state, out) = rebuild(typesetter.as_deref_mut(), arena, solves);
            *state = Some(new_state);
            out
        } else {
            let state = state.as_mut().expect("non-structural implies a built tree");
            incremental(state, typesetter.as_deref_mut(), arena, solves)
        }
    }
}

/// Build the whole tree from scratch, solve it, and report every node —
/// the first solve, or one after a structural change (issue #164).
fn rebuild(
    typesetter: Option<&mut Typesetter>,
    arena: &Arena,
    solves: &mut u64,
) -> (TreeState, Vec<(NodeId, SolvedRect)>) {
    let n = arena.node_count();
    let mut tree: TaffyTree<TextContext> = TaffyTree::new();
    // R7: the committed table is an f32 passthrough of the solve —
    // Taffy's default whole-pixel rounding is off.
    tree.disable_rounding();

    // A placeholder for every slot; `build` overwrites each, since every
    // node is reachable from a root.
    let placeholder = taffy::NodeId::new(0);
    let mut taffy_of: Vec<taffy::NodeId> = vec![placeholder; n];
    let mut parent_of: Vec<Option<NodeId>> = vec![None; n];
    let mut roots = Vec::with_capacity(arena.roots().len());
    for &root in arena.roots() {
        let taffy_root = build(
            &mut tree,
            &mut taffy_of,
            &mut parent_of,
            arena,
            root,
            None,
            None,
        );
        roots.push(taffy_root);
    }

    compute_all(&mut tree, &roots, typesetter, solves);

    let mut prev_rel = vec![[0u32; 4]; n];
    let mut prev_root_origin = Vec::with_capacity(roots.len());
    let mut out = Vec::new();
    for &root in arena.roots() {
        // Roots are their own coordinate islands: the subtree translates
        // by the root's authored offset.
        let origin = arena.layout(root);
        prev_root_origin.push([origin.x.to_bits(), origin.y.to_bits()]);
        read_back_full(
            &tree,
            &taffy_of,
            &mut prev_rel,
            arena,
            root,
            (origin.x, origin.y),
            &mut out,
        );
    }

    let state = TreeState {
        tree,
        taffy_of,
        parent_of,
        roots,
        prev_rel,
        prev_root_origin,
        node_count: n,
    };
    (state, out)
}

/// Re-solve only what the change forced: restyle the nodes whose layout
/// intent changed (and their children, for a mode change), recompute the
/// dirtied subtrees, and read back only the rects that moved (issue #164).
fn incremental(
    state: &mut TreeState,
    typesetter: Option<&mut Typesetter>,
    arena: &Arena,
    solves: &mut u64,
) -> Vec<(NodeId, SolvedRect)> {
    // The nodes whose layout intent changed since the last commit.
    let dirty: FxHashSet<NodeId> = arena.layout_dirty().iter().copied().collect();
    // The paint-only fast path: nothing to solve. A root move is layout
    // intent (an X/Y change), so an empty set means no geometry changed
    // anywhere, and every rect carries forward unchanged.
    if dirty.is_empty() {
        return Vec::new();
    }

    // Restyle each dirty node. A node's child-side style depends on its
    // parent's mode, so a mode change on the parent restyles its children
    // too; recompute them unconditionally (bounded by the change).
    for &node in &dirty {
        let taffy_node = state.taffy_of[node.index()];
        let parent_layout = state.parent_of[node.index()].map(|p| arena.layout(p));
        let node_layout = arena.layout(node);
        state
            .tree
            .set_style(taffy_node, style_for(&node_layout, parent_layout.as_ref()))
            .expect("restyling a retained node cannot fail");
        state
            .tree
            .set_node_context(taffy_node, text_context(arena, node))
            .expect("setting a retained node's context cannot fail");
        for &child in arena.children(node) {
            let taffy_child = state.taffy_of[child.index()];
            state
                .tree
                .set_style(
                    taffy_child,
                    style_for(&arena.layout(child), Some(&node_layout)),
                )
                .expect("restyling a retained child cannot fail");
        }
    }

    // `roots` is cloned so the loop does not hold a borrow of `state`
    // while `state.tree` is recomputed (the roots list is small).
    let roots = state.roots.clone();
    compute_all(&mut state.tree, &roots, typesetter, solves);

    // A subtree can hold a changed node without moving at its own root
    // (a fixed-size frame with a shifted child): mark every dirty node and
    // its ancestors so the readback descends to reach them, on top of
    // descending wherever a rect actually moved.
    let mut on_path = vec![false; state.node_count];
    for &node in &dirty {
        let mut cursor = Some(node);
        while let Some(current) = cursor {
            if on_path[current.index()] {
                break;
            }
            on_path[current.index()] = true;
            cursor = state.parent_of[current.index()];
        }
    }

    let mut out = Vec::new();
    for (root_i, &root) in arena.roots().iter().enumerate() {
        let origin = arena.layout(root);
        let cur_origin = [origin.x.to_bits(), origin.y.to_bits()];
        let root_moved = state.prev_root_origin[root_i] != cur_origin;
        state.prev_root_origin[root_i] = cur_origin;
        read_back_pruned(
            &state.tree,
            &state.taffy_of,
            &mut state.prev_rel,
            &on_path,
            arena,
            root,
            (origin.x, origin.y),
            root_moved,
            &mut out,
        );
    }
    out
}

/// Compute the layout of every root, counting each as one solve. Text
/// nodes size to their shaped runs; the typesetter is reborrowed per root
/// so its one shaped-run cache serves every root (and, at #30, the
/// painter). A solver with no typesetter measures every text node to zero
/// — the same result Taffy's default (no-op) measure gives.
fn compute_all(
    tree: &mut TaffyTree<TextContext>,
    roots: &[taffy::NodeId],
    typesetter: Option<&mut Typesetter>,
    solves: &mut u64,
) {
    let mut typesetter = typesetter;
    for &taffy_root in roots {
        tree.compute_layout_with_measure(
            taffy_root,
            Size::MAX_CONTENT,
            |known, available, _node, context, _style| match (context, typesetter.as_deref_mut()) {
                (Some(text), Some(ts)) => measure_text(known, available, text, ts),
                _ => Size::ZERO,
            },
        )
        .expect("taffy tree built from the arena is always valid");
        *solves += 1;
    }
}

/// A text node's measure input, attached to its Taffy leaf: the
/// paragraph text and the render size (px per em in document units).
/// The text is owned so the tree outlives the arena borrow; shaping
/// itself is not repeated, because `measure_text` reads the
/// typesetter's shaped-run cache
/// (`docs/decisions/shaped-run-cache-font-units.md`).
#[derive(Debug)]
struct TextContext {
    text: String,
    size: f32,
}

/// The measure context for a node, present only when the node carries
/// both text content and a text style — the well-formed text node. A
/// node missing either is a plain leaf, not a measured text node (a
/// text node with no style has no size to shape at).
fn text_context(arena: &Arena, node: NodeId) -> Option<TextContext> {
    let text = arena.text(node)?;
    let style = arena.text_style(node)?;
    Some(TextContext {
        text: text.to_string(),
        size: style.size,
    })
}

/// Measure a text node against the shaped-run cache. `known` is what
/// Taffy has already fixed for the node; `available` is the space it
/// offers. The wrap width is the fixed width if Taffy set one, else a
/// definite available width, else none — a min/max-content probe
/// imposes no wrap, so the paragraph lays out on one line and the node
/// hugs its natural width. A known axis is returned unchanged; only an
/// unfixed axis takes the shaped measurement.
fn measure_text(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    context: &TextContext,
    typesetter: &mut Typesetter,
) -> Size<f32> {
    let max_width = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
    });
    let laid = typesetter.layout(&context.text, context.size, max_width);
    Size {
        width: known.width.unwrap_or(laid.width),
        height: known.height.unwrap_or(laid.height),
    }
}

/// Build the Taffy subtree for `node`; record its Taffy id and parent.
fn build(
    tree: &mut TaffyTree<TextContext>,
    taffy_of: &mut [taffy::NodeId],
    parent_of: &mut [Option<NodeId>],
    arena: &Arena,
    node: NodeId,
    parent: Option<&Layout>,
    parent_id: Option<NodeId>,
) -> taffy::NodeId {
    let layout = arena.layout(node);
    let style = style_for(&layout, parent);
    // A text node carries a measure context so Taffy sizes it from its
    // shaped runs; every other node is a plain leaf whose measure is a
    // no-op.
    let taffy_node = match text_context(arena, node) {
        Some(context) => tree.new_leaf_with_context(style, context),
        None => tree.new_leaf(style),
    }
    .expect("taffy node allocation cannot fail");
    taffy_of[node.index()] = taffy_node;
    parent_of[node.index()] = parent_id;
    for &child in arena.children(node) {
        let taffy_child = build(
            tree,
            taffy_of,
            parent_of,
            arena,
            child,
            Some(&layout),
            Some(node),
        );
        tree.add_child(taffy_node, taffy_child)
            .expect("taffy child insertion cannot fail");
    }
    taffy_node
}

/// Map one node's layout intent to a Taffy style, in the context of
/// its parent's layout (child sizing is axis-relative).
fn style_for(layout: &Layout, parent: Option<&Layout>) -> Style {
    let mut style = Style::default();

    // Container side: how this node lays out its own children.
    match layout.mode {
        LayoutMode::None => {
            // Children are absolutely positioned by their authored
            // offsets (the passthrough); Block is the inert display.
            style.display = Display::Block;
        }
        LayoutMode::Horizontal | LayoutMode::Vertical => {
            style.display = Display::Flex;
            style.flex_direction = if layout.mode == LayoutMode::Horizontal {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
            // The vocabulary has one authored gap; the cross-axis
            // half is inert until wrap (v0.8), which decides whether
            // row and column gaps split into separate properties.
            style.gap = Size {
                width: length(layout.gap),
                height: length(layout.gap),
            };
            style.padding = Rect {
                left: length(layout.padding.left),
                top: length(layout.padding.top),
                right: length(layout.padding.right),
                bottom: length(layout.padding.bottom),
            };
            style.justify_content = Some(match layout.main_align {
                dashscene_core::MainAxisAlign::Start => JustifyContent::FLEX_START,
                dashscene_core::MainAxisAlign::Center => JustifyContent::CENTER,
                dashscene_core::MainAxisAlign::End => JustifyContent::FLEX_END,
                dashscene_core::MainAxisAlign::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            });
            // Never Stretch at the container level: Fill children opt
            // into stretching via align_self; Fixed/Hug children keep
            // their own cross size under any alignment.
            style.align_items = Some(match layout.cross_align {
                dashscene_core::CrossAxisAlign::Start => AlignItems::FLEX_START,
                dashscene_core::CrossAxisAlign::Center => AlignItems::CENTER,
                dashscene_core::CrossAxisAlign::End => AlignItems::FLEX_END,
            });
        }
    }

    // Child side: how this node sizes within its parent.
    let dimension = |sizing: AxisSizing, size: f32| match sizing {
        AxisSizing::Fixed => Dimension::length(size),
        // Fill's main-axis growth is expressed via flex_basis/grow
        // below; its size stays auto on both axes.
        AxisSizing::Hug | AxisSizing::Fill => Dimension::AUTO,
    };
    style.size = Size {
        width: dimension(layout.sizing_h, layout.width),
        height: dimension(layout.sizing_v, layout.height),
    };
    let bound = |v: Option<f32>| v.map_or(Dimension::AUTO, Dimension::length);
    style.min_size = Size {
        width: bound(layout.min_width),
        height: bound(layout.min_height),
    };
    style.max_size = Size {
        width: bound(layout.max_width),
        height: bound(layout.max_height),
    };

    match parent.map(|p| p.mode) {
        // Root: nothing more to map (location handled at readback).
        // Margin is flex-flow vocabulary with no meaning here, and
        // Taffy ignores a root's own margin regardless.
        None => {}
        // Passthrough parent: place by the authored offset. Fill has
        // no free-space axis under a None parent and behaves as Hug
        // (the validator diagnoses it at its own slice, P4). Margin is
        // inert — placement is the authored offset, matching
        // `commit()`'s fixed resolution, which ignores margin.
        Some(LayoutMode::None) => {
            style.position = Position::Absolute;
            style.inset = Rect {
                left: LengthPercentageAuto::length(layout.x),
                top: LengthPercentageAuto::length(layout.y),
                right: LengthPercentageAuto::AUTO,
                bottom: LengthPercentageAuto::AUTO,
            };
        }
        Some(mode @ (LayoutMode::Horizontal | LayoutMode::Vertical)) => {
            // Outer margin applies only in flex flow (negative allowed
            // — it expresses overlap, the target of the negative-gap
            // lowering).
            style.margin = Rect {
                left: LengthPercentageAuto::length(layout.margin.left),
                top: LengthPercentageAuto::length(layout.margin.top),
                right: LengthPercentageAuto::length(layout.margin.right),
                bottom: LengthPercentageAuto::length(layout.margin.bottom),
            };
            // Axis-relative sizing: the parent's main axis maps to
            // flex_basis/grow/shrink; the cross axis maps to size (set
            // above) and align_self.
            let (main_sizing, main_size, cross_sizing) = if mode == LayoutMode::Horizontal {
                (layout.sizing_h, layout.width, layout.sizing_v)
            } else {
                (layout.sizing_v, layout.height, layout.sizing_h)
            };
            match main_sizing {
                AxisSizing::Fixed => {
                    style.flex_basis = Dimension::length(main_size);
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
                AxisSizing::Hug => {
                    style.flex_basis = Dimension::AUTO;
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
                AxisSizing::Fill => {
                    style.flex_basis = Dimension::length(0.0);
                    style.flex_grow = 1.0;
                    style.flex_shrink = 1.0;
                }
            }
            if cross_sizing == AxisSizing::Fill {
                style.align_self = Some(AlignSelf::STRETCH);
            }
        }
    }

    // Overrides both sides above: Taffy's Display::None hides the node
    // from its parent's flow and hides its whole subtree regardless of
    // any descendant's own style (issue #165).
    if !layout.visible {
        style.display = Display::None;
    }

    style
}

/// Emit every node's absolute rect and record its relative layout — the
/// full readback a rebuild uses (issue #164).
fn read_back_full(
    tree: &TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    prev_rel: &mut [[u32; 4]],
    arena: &Arena,
    node: NodeId,
    parent_origin: (f32, f32),
    out: &mut Vec<(NodeId, SolvedRect)>,
) {
    let layout = tree
        .layout(taffy_of[node.index()])
        .expect("layout was computed for the whole tree");
    let x = parent_origin.0 + layout.location.x;
    let y = parent_origin.1 + layout.location.y;
    prev_rel[node.index()] = rel_bits(layout);
    out.push((
        node,
        SolvedRect {
            x,
            y,
            w: layout.size.width,
            h: layout.size.height,
        },
    ));
    for &child in arena.children(node) {
        read_back_full(tree, taffy_of, prev_rel, arena, child, (x, y), out);
    }
}

/// Emit only the rects that moved or resized since the previous solve,
/// pruning subtrees that neither shifted nor hold a changed node
/// (issue #164). `parent_moved` is whether this node's parent origin
/// changed — if so, this node shifts even when its own relative layout
/// did not.
#[allow(clippy::too_many_arguments)]
fn read_back_pruned(
    tree: &TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    prev_rel: &mut [[u32; 4]],
    on_path: &[bool],
    arena: &Arena,
    node: NodeId,
    parent_origin: (f32, f32),
    parent_moved: bool,
    out: &mut Vec<(NodeId, SolvedRect)>,
) {
    let layout = tree
        .layout(taffy_of[node.index()])
        .expect("layout was computed for the whole tree");
    let x = parent_origin.0 + layout.location.x;
    let y = parent_origin.1 + layout.location.y;
    let cur = rel_bits(layout);
    let prev = prev_rel[node.index()];
    prev_rel[node.index()] = cur;

    let rel_changed = cur != prev;
    // The absolute rect changed if the parent shifted or the node's own
    // relative layout (position or size) changed.
    let rect_changed = parent_moved || rel_changed;
    if rect_changed {
        out.push((
            node,
            SolvedRect {
                x,
                y,
                w: layout.size.width,
                h: layout.size.height,
            },
        ));
    }

    // This node's own origin moved if the parent shifted or its relative
    // position changed (a pure resize leaves the origin put).
    let origin_moved = parent_moved || cur[0] != prev[0] || cur[1] != prev[1];
    // Descend when the subtree could hold a change: this node shifted or
    // resized, or it is on the path to a node whose intent changed. A
    // node that neither moved nor guards a dirty descendant has an
    // unchanged subtree, and Taffy left its layouts untouched.
    if rect_changed || on_path[node.index()] {
        for &child in arena.children(node) {
            read_back_pruned(
                tree,
                taffy_of,
                prev_rel,
                on_path,
                arena,
                child,
                (x, y),
                origin_moved,
                out,
            );
        }
    }
}

/// One node's Taffy-relative layout as bit patterns — position and size,
/// the values a readback compares to decide whether a rect moved.
fn rel_bits(layout: &taffy::Layout) -> [u32; 4] {
    [
        layout.location.x.to_bits(),
        layout.location.y.to_bits(),
        layout.size.width.to_bits(),
        layout.size.height.to_bits(),
    ]
}
