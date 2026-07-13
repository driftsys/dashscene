# Auto-layout is refused on two grounds, and each one holds alone

    status   accepted (story #139, 2026-07-13)
    scope    crates/dashc (the figma module)
    binds    #140 (Dsb cannot express flex), the v0.7 flex lowering, and any
             future reader of absoluteBoundingBox

## Context

A Figma frame with `layoutMode` other than `NONE` (`HORIZONTAL`, `VERTICAL`,
or the newer `GRID`) is an auto-layout frame: Figma's own flex solver places
its children.

This is a specific case of
`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` — it
lowers to `CompileError::Unsupported`, like every other construct `Dsb` cannot
carry. It gets its own record because it is refused for **two independent
reasons**, and only one of them is an expressiveness gap. Conflating them
would let the second reason disappear the day the first is fixed.

## Choice

The walk refuses any `layoutMode` other than `NONE`:

    if let Some(mode) = node.layout_mode.as_deref()
        && mode != "NONE"
    {
        return Err(CompileError::Unsupported {
            path,
            what: format!("auto-layout ({mode})"),
        });
    }

## Why — reason one: there is no field to lower the intent into

`Dsb` has no flex vocabulary — no mode, no gap, no padding, no sizing
(debt #140). The intent has nowhere to go, and there is no `Construct`
variant to triage it onto either, so dropping it would be the silent drop
P4 forbids.

This reason expires. When `Dsb` gains the flex vocabulary, the fields appear
and the intent lowers.

## Why — reason two: inside an auto-layout frame, the boxes are results (P1)

Figma's `absoluteBoundingBox` for a node **inside** an auto-layout frame is
not authored geometry. It is what Figma's flex solver _computed_. Lowering it
as a fixed box would write a layout result into a document that, by P1, carries
intent and never results — and the output would look correct until the first
resize, at which point the frame would not reflow because nothing in the
document says it should.

This reason does **not** expire. It constrains how the flex lowering must be
written when #140 lands: it must lower the auto-layout _intent_ (mode, gap,
padding, sizing, alignment) and must still never read the solved box of a node
inside such a frame.

## Why record it, rather than fold it into the general refusal

Because the bug was live. `corpus/figma-fixtures/effects-2025.json` has a root
frame carrying `layoutMode: HORIZONTAL`, and before this refusal the walk
lowered it silently, as a fixed box, with its children's solved positions baked
in. It rendered. Nothing failed. Only reason two explains why that output was
wrong, and reason one alone would have been discharged by #140 while leaving
the same bug in place.

## Consequences

- **`effects-2025.json` cannot be compiled as captured.** Its effects are the
  point of that fixture, so the test exercises them through a derived document
  with `layoutMode` stripped from the root and nothing else changed, and pins
  the refusal of the captured root separately. The effects that reach the triage
  table are still the captured ones (P5).
- **Closing #140 does not by itself license reading the boxes.** The flex
  lowering must satisfy reason two on its own terms.
- **`v03-paint.json` is `layoutMode: NONE` throughout**, which is why the v0.3
  paint vocabulary lowers with no `Dsb` change at all.
