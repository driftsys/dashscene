//! Runtime that resolves the model — the Taffy layout solve
//! (DESIGN_1.md §7.1; variants, FLIP, and the measure callback land at
//! their own slices).
//!
//! [`TaffySolver`] implements `dashscene-core`'s `LayoutSolver` seam:
//! it maps the arena's layout intent to a Taffy tree (one tree per
//! root — roots are independent coordinate islands), solves, and
//! returns absolute rects. Taffy is the sole solver (P2); layout mode
//! `None` is a passthrough expressed as absolutely-positioned children,
//! not a second engine.

use dashscene_core::{Arena, AxisSizing, Layout, LayoutMode, LayoutSolver, NodeId, SolvedRect};
use taffy::prelude::*;
use taffy::{AlignItems, AlignSelf, JustifyContent, Position};

/// The Taffy implementation of `dashscene-core`'s `LayoutSolver`.
///
/// Stateless today; `LayoutSolver` takes `&mut self` so this can hold
/// retained trees and measure-callback state at later slices.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TaffySolver;

impl TaffySolver {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LayoutSolver for TaffySolver {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        let mut out = Vec::new();
        for &root in arena.roots() {
            let mut tree: TaffyTree<()> = TaffyTree::new();
            // R7: the committed table is an f32 passthrough of the
            // solve — Taffy's default whole-pixel rounding is off.
            tree.disable_rounding();
            let mut pairs = Vec::new();
            let taffy_root = build(&mut tree, &mut pairs, arena, root, None);
            tree.compute_layout(taffy_root, Size::MAX_CONTENT)
                .expect("taffy tree built from the arena is always valid");
            // Roots are their own coordinate islands: the subtree
            // translates by the root's authored offset.
            let origin = arena.layout(root);
            let mut cursor = 0;
            read_back(
                &tree,
                &pairs,
                &mut cursor,
                arena,
                root,
                (origin.x, origin.y),
                &mut out,
            );
        }
        out
    }
}

/// Build the Taffy subtree for `node`; record the NodeId pairing.
fn build(
    tree: &mut TaffyTree<()>,
    pairs: &mut Vec<(NodeId, taffy::NodeId)>,
    arena: &Arena,
    node: NodeId,
    parent: Option<&Layout>,
) -> taffy::NodeId {
    let layout = arena.layout(node);
    let style = style_for(&layout, parent);
    let taffy_node = tree
        .new_leaf(style)
        .expect("taffy node allocation cannot fail");
    pairs.push((node, taffy_node));
    for &child in arena.children(node) {
        let taffy_child = build(tree, pairs, arena, child, Some(&layout));
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
    // Outer margin (negative allowed — expresses overlap, the target
    // of the negative-gap lowering).
    style.margin = Rect {
        left: LengthPercentageAuto::length(layout.margin.left),
        top: LengthPercentageAuto::length(layout.margin.top),
        right: LengthPercentageAuto::length(layout.margin.right),
        bottom: LengthPercentageAuto::length(layout.margin.bottom),
    };

    match parent.map(|p| p.mode) {
        // Root: nothing more to map (location handled at readback).
        None => {}
        // Passthrough parent: place by the authored offset. Fill has
        // no free-space axis under a None parent and behaves as Hug
        // (the validator diagnoses it at its own slice, P4).
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

    style
}

/// Accumulate parent origins to produce the absolute rects core's
/// table carries (Taffy reports parent-relative locations).
fn read_back(
    tree: &TaffyTree<()>,
    pairs: &[(NodeId, taffy::NodeId)],
    cursor: &mut usize,
    arena: &Arena,
    node: NodeId,
    parent_origin: (f32, f32),
    out: &mut Vec<(NodeId, SolvedRect)>,
) {
    // `pairs` is in build order — DFS pre-order — which is exactly the
    // order this walk visits, so the cursor advances in lockstep
    // instead of searching.
    let (paired, taffy_node) = pairs[*cursor];
    debug_assert_eq!(paired, node, "build order and readback order agree");
    *cursor += 1;
    let layout = tree
        .layout(taffy_node)
        .expect("layout was computed for the whole tree");
    let x = parent_origin.0 + layout.location.x;
    let y = parent_origin.1 + layout.location.y;
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
        read_back(tree, pairs, cursor, arena, child, (x, y), out);
    }
}
