# The taffy scaled-shrink report (debt #269)

    status   informative — the text to file upstream, plus the verified
             minimal reproduction it carries
    covers   taffy 0.12.0 through 0.12.2 (0.12.2 is what this repo locks)
    related  docs/decisions/negative-margin-hug-rebate.md (the workaround this
             report is the exit route for), debt #236, debt #270, debt #269
    repro    crates/dashscene-engine/tests/taffy_upstream.rs — plain taffy, no
             dashscene types, and a canary: its assertions pin taffy's current
             wrong answers, so they fail the day taffy is fixed

This note is informative. It records what to file and what the reproduction
proves; it binds nothing. Filing the issue on the taffy repository is the
repository owner's act, not this repo's — the text below is ready to paste.

## Why the report matters here

`dashscene-engine` carries a workaround for this defect in two places: the
negative-margin flex-basis rebate for `Fixed` children (debt #236) and the
`flex_shrink: 1` switch for `Hug` children (debt #270). Both exist only
because of the arithmetic below. When a fixed taffy is released and adopted,
both come out and
`crates/dashscene-engine/tests/taffy_upstream.rs` goes with them.

## The report

### Title

Intrinsic main size amplifies a negative main-axis margin on a
`flex_shrink: 0` item

### Body

A flex container whose main size is indefinite (`size: auto`, solved under
`MinContent` or `MaxContent` available space) computes a wrong intrinsic main
size when a `flex_shrink: 0` item carries a negative main-axis margin. The
error scales with the item's inner flex basis, so it is not a rounding
difference: a 56-wide item with `margin-left: -16` makes a two-item row hug
to 0 instead of 96.

Affects 0.12.0, 0.12.1 and 0.12.2 (checked against the 0.12.2 source).

#### Where

`src/compute/flexbox.rs`, `determine_container_main_size`, the
`AvailableSpace::MinContent | AvailableSpace::MaxContent` arm.

The item's flex fraction is computed as:

    let diff = content_contribution - item.flex_basis;
    if diff > 0.0 {
        diff / f32_max(1.0, item.flex_grow)
    } else if diff < 0.0 {
        let scaled_shrink_factor = f32_max(1.0, item.flex_shrink * item.inner_flex_basis);
        diff / scaled_shrink_factor
    } else {
        0.0
    }

and the item's size is then reconstructed as:

    let flex_contribution = if item.content_flex_fraction > 0.0 {
        f32_max(1.0, item.flex_grow) * flex_fraction
    } else if item.content_flex_fraction < 0.0 {
        let scaled_shrink_factor = f32_max(1.0, item.flex_shrink) * item.inner_flex_basis;
        scaled_shrink_factor * flex_fraction
    } else {
        0.0
    };
    let size = item.flex_basis + flex_contribution;

The `diff < 0.0` path divides by `f32_max(1.0, flex_shrink * inner_flex_basis)`
and multiplies back by `f32_max(1.0, flex_shrink) * inner_flex_basis`. Those
two expressions are only equal when `flex_shrink * inner_flex_basis` and
`f32_max(1.0, flex_shrink) * inner_flex_basis` agree — which holds at
`flex_shrink = 1` for any inner basis of 1 or more, and fails at
`flex_shrink = 0`, where the divisor floors at `1` while the multiplier is the
whole inner flex basis.

A negative main-axis margin is exactly what makes `diff` negative for an item
whose preferred size is definite: `content_contribution` is the clamped
preferred size **plus** the margins, and `flex_basis` is the preferred size
alone, so `diff` is the margin sum. The reconstruction then returns
`flex_basis + inner_flex_basis * margin_sum` instead of
`flex_basis + margin_sum`.

The `diff > 0.0` path divides by `f32_max(1.0, flex_grow)` and multiplies back
by `f32_max(1.0, flex_grow)` — the same expression — which is why a positive
margin sums correctly and only a negative one is affected.

#### Reproduction

    use taffy::prelude::*;

    fn child(margin_left: f32) -> Style {
        Style {
            size: Size { width: length(56.0), height: length(56.0) },
            flex_basis: Dimension::length(56.0),
            flex_grow: 0.0,
            flex_shrink: 0.0,
            margin: Rect { left: LengthPercentageAuto::length(margin_left), ..Rect::zero() },
            ..Default::default()
        }
    }

    fn main() {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        tree.disable_rounding();
        let a = tree.new_leaf(child(0.0)).unwrap();
        let b = tree.new_leaf(child(-16.0)).unwrap();
        let root = tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    size: Size { width: Dimension::AUTO, height: length(56.0) },
                    ..Default::default()
                },
                &[a, b],
            )
            .unwrap();

        tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
        // expected 96 (56 + 56 - 16); taffy 0.12.2 gives 0
        println!("{}", tree.layout(root).unwrap().size.width);
    }

Sweeping the margin gives the whole shape of the error:

| `margin-left` | expected row width | taffy 0.12.2 |
| ------------- | ------------------ | ------------ |
| `0`           | 112                | 112          |
| `+16`         | 128                | 128          |
| `-1`          | 111                | 56           |
| `-16`         | 96                 | 0            |

Setting `flex_shrink: 1` on the second child — changing nothing else — makes
every row in that table correct, which isolates the fault to the
`flex_shrink = 0` case of the two scaled-shrink expressions.

The same error is reachable through a content-sized child: replace the second
leaf with a nested `size.width: auto` row holding one 56-wide leaf, and the
outer row hugs to 0 in exactly the same way. That form has no authored size to
work around the defect with, which is why it is worth naming in the report.

#### Suggested fix

Make the two expressions the same. The reconstruction's
`f32_max(1.0, item.flex_shrink) * item.inner_flex_basis` should be the
`f32_max(1.0, item.flex_shrink * item.inner_flex_basis)` the fraction was
divided by, so that `flex_basis + flex_contribution` returns
`content_contribution` for any shrink factor, as it already does on the
`diff > 0.0` path.

## What this repo does until then

`docs/decisions/negative-margin-hug-rebate.md` records both workarounds and
the corners each leaves. Neither is removed on this note's say-so: the
retirement is conditional on a fixed taffy being released and adopted, and
`crates/dashscene-engine/tests/taffy_upstream.rs` is the thing that will
say so — it asserts taffy's current wrong answers, so a taffy upgrade that
fixes the defect turns those assertions red and names the work.
