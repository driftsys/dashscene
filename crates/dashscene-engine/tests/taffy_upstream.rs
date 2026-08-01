//! The taffy 0.12 defect the negative-margin rebate exists to work around
//! (debt #236, debt #270, debt #269), reproduced in **plain taffy** with no
//! dashscene types in the way.
//!
//! Two roles, and both matter:
//!
//! 1. It is the minimal reproduction the upstream report carries
//!    (`docs/technotes/taffy-scaled-shrink-report.md`), kept in the repo so
//!    it cannot drift from the report.
//! 2. It is a **canary**. Every assertion here pins taffy's *current, wrong*
//!    answer, with the correct answer named beside it. When a taffy upgrade
//!    makes one of these fail, taffy has fixed the defect — and that is the
//!    signal to retire the rebate and the `flex_shrink` switch in
//!    `style_for`, and to delete this file
//!    (`docs/decisions/negative-margin-hug-rebate.md`).
//!
//! The defect, in one sentence: in `determine_container_main_size`'s
//! `MinContent | MaxContent` branch, an item whose content contribution is
//! *below* its flex basis has that difference divided by
//! `f32_max(1.0, flex_shrink * inner_flex_basis)` and then multiplied back
//! by `f32_max(1.0, flex_shrink) * inner_flex_basis`. For a `flex_shrink: 0`
//! item those are `1` and `inner_flex_basis`, so the difference — which a
//! negative main-axis margin is exactly — is amplified by the item's inner
//! flex basis.

use taffy::prelude::*;

/// A `flex_shrink: 0` child of authored `width`, carrying `margin_left`.
fn fixed_child(width: f32, margin_left: f32) -> Style {
    Style {
        size: Size {
            width: length(width),
            height: length(56.0_f32),
        },
        flex_basis: Dimension::length(width),
        flex_grow: 0.0,
        flex_shrink: 0.0,
        margin: Rect {
            left: LengthPercentageAuto::length(margin_left),
            ..Rect::zero()
        },
        ..Default::default()
    }
}

/// A hug-width row (`size.width: auto`) that lays its children out
/// horizontally — the container whose intrinsic main size mis-sums.
fn hug_row() -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        size: Size {
            width: Dimension::AUTO,
            height: length(56.0_f32),
        },
        ..Default::default()
    }
}

/// Solve `root` at max-content available space and return its width.
fn hug_width(tree: &mut TaffyTree<()>, root: taffy::NodeId) -> f32 {
    tree.disable_rounding();
    tree.compute_layout(root, Size::MAX_CONTENT)
        .expect("the tree is valid");
    tree.layout(root).expect("the root was laid out").size.width
}

#[test]
fn taffy_still_amplifies_a_shrink_zero_childs_negative_margin() {
    // Two 56-wide `flex_shrink: 0` children in a hug row; the second
    // overlaps the first. The row's hug width should be 56 + 56 + margin.
    //
    // The arithmetic taffy performs for the second child, at margin -16:
    //   flex_basis          = 56
    //   inner_flex_basis    = 56  (no padding, no border)
    //   content_contribution= 56 + (-16) = 40
    //   diff                = 40 - 56 = -16
    //   fraction            = -16 / f32_max(1, 0 * 56) = -16 / 1 = -16
    //   contribution        = f32_max(1, 0) * 56 * -16 = -896
    //   item size           = 56 + (-896) = -840   (clamped to 0 in the sum)
    // so the row hugs to 56 instead of 96.
    for (margin, correct, taffy_gives) in [
        (0.0_f32, 112.0_f32, 112.0_f32),
        (16.0, 128.0, 128.0),
        (-1.0, 111.0, 56.0),
        (-16.0, 96.0, 0.0),
    ] {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let a = tree.new_leaf(fixed_child(56.0, 0.0)).expect("leaf a");
        let b = tree.new_leaf(fixed_child(56.0, margin)).expect("leaf b");
        let root = tree.new_with_children(hug_row(), &[a, b]).expect("root");
        let width = hug_width(&mut tree, root);

        if margin >= 0.0 {
            assert_eq!(width, correct, "positive margins already sum correctly");
        } else {
            assert_eq!(
                width, taffy_gives,
                "taffy is expected to still mis-sum margin {margin} as {taffy_gives} \
                 rather than {correct}; if this now reads {correct}, taffy is fixed — \
                 retire the rebate (docs/decisions/negative-margin-hug-rebate.md)"
            );
        }
    }
}

#[test]
fn taffy_still_amplifies_a_hug_childs_negative_margin() {
    // The same defect reached through a content-sized (hug) child, which is
    // what debt #270 is: the child's flex basis is the content size taffy
    // measures during the very pass that mis-sums, so there is no authored
    // size to fold the margin into. The engine answers this one with
    // `flex_shrink: 1`, where the branch's two expressions agree.
    for (margin, correct, taffy_gives) in [(-1.0_f32, 111.0_f32, 56.0_f32), (-16.0, 96.0, 0.0)] {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let a = tree.new_leaf(fixed_child(56.0, 0.0)).expect("leaf a");
        let inner = tree.new_leaf(fixed_child(56.0, 0.0)).expect("inner leaf");
        // A hug-width child: no authored width, sized by its own content,
        // carrying the negative margin and taffy's own shrink-0 mapping.
        let hug_child = Style {
            flex_basis: Dimension::AUTO,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            margin: Rect {
                left: LengthPercentageAuto::length(margin),
                ..Rect::zero()
            },
            ..hug_row()
        };
        let b = tree
            .new_with_children(hug_child, &[inner])
            .expect("hug child");
        let root = tree.new_with_children(hug_row(), &[a, b]).expect("root");
        let width = hug_width(&mut tree, root);

        assert_eq!(
            width, taffy_gives,
            "taffy is expected to still mis-sum a hug child's margin {margin} as \
             {taffy_gives} rather than {correct}; if this now reads {correct}, taffy \
             is fixed — retire the rebate and the flex_shrink switch \
             (docs/decisions/negative-margin-hug-rebate.md)"
        );
    }
}

#[test]
fn a_shrink_one_child_already_sums_its_negative_margin_correctly() {
    // The agreement point the engine's #270 fix uses: at `flex_shrink: 1`
    // the divisor `f32_max(1, 1 * inner_basis)` and the multiplier
    // `f32_max(1, 1) * inner_basis` are the same number for any inner basis
    // of 1 or more, so the same scene sums exactly. This is the control that
    // makes the two assertions above a statement about the shrink factor
    // rather than about negative margins in general.
    for (margin, correct) in [(-1.0_f32, 111.0_f32), (-16.0, 96.0)] {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        let a = tree.new_leaf(fixed_child(56.0, 0.0)).expect("leaf a");
        let b = tree
            .new_leaf(Style {
                flex_shrink: 1.0,
                ..fixed_child(56.0, margin)
            })
            .expect("leaf b");
        let root = tree.new_with_children(hug_row(), &[a, b]).expect("root");

        assert_eq!(hug_width(&mut tree, root), correct, "margin {margin}");
    }
}
