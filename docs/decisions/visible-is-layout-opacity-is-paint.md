# Visible is a layout prop, Opacity is a paint prop, and there is no third state

    status   accepted (story #165, 2026-07-15). Prop::Visible is built at v0.4;
             Prop::Opacity is scope for the v0.8 paint work (#42), not yet built.
    scope    dashscene-core's Prop vocabulary; dashscene-engine's style mapping;
             the v0.8 group-opacity work

## Context

`Prop` had neither a visibility nor an opacity concept, and the update path
needs to hide a node (bounded pools, variant switches). The line between
the two is not "visible versus invisible" — it is which side of boundary B
consumes it. (`docs/archive/2026-07-14-scope-decisions.md` §23 D7;
`docs/archive/2026-07-14-design-1-seed.md` §11, §10.1.)

## Options

1. One `Visible(bool)` that hides both from layout and from paint.
2. Two props: `Visible(bool)` (layout participation) and `Opacity(f32)`
   (node/group alpha), plus a CSS-style third `visibility: hidden` state
   (draws nothing, keeps its box).
3. Two props only — `Visible` layout-affecting, `Opacity` paint-only — and
   no third state.

## Choice

Option 3.

- `Prop::Visible(bool)` (stored on `Layout`, default `true`) is
  layout-affecting vocabulary the flex-aware solver consumes.
  `dashscene-engine`'s `TaffySolver` lowers `false` to Taffy
  `Display::None`, which hides the node and every descendant from the flex
  flow so siblings reflow into its space. `commit()`'s `FixedSolver`
  ignores it, like the rest of the flex vocabulary. A hidden node still
  resolves to a rect and keeps its rect-table index (P4 — a solver never
  omits a node), which is the invariant bounded pools depend on
  (`docs/decisions/scene-tree-is-static-lists-are-bounded-pools.md`).
- `Prop::Opacity(f32)` is node/group alpha that never reaches Taffy and
  triggers no solve. It is decided here but lands with the v0.8 paint work
  (§10.1), which already owns group opacity; `Visible` (v0.4) and `Opacity`
  (v0.8) split across two slices.

## Why

- Taffy has exactly one lever — its `Display` enum is
  `Block | Flex | Grid | None`, and `None` hides the node and its children.
  There is no visibility concept anywhere in the crate. From a layout
  engine's point of view an unpainted node is a normal node with a normal
  box; whether anyone paints it is not its business. So the split falls
  exactly on P2: `Visible` is the solver's, `Opacity` is the painter's.
- CSS's `visibility: hidden` (option 2's third state) exists for
  inheritance/descendant-override, hit-testing, and stacking contexts —
  dashscene has none of the three (there is no cascade, and input and
  hit-testing belong to the host). Without them, `visibility: hidden` and
  `opacity: 0` are the same thing: occupies space, draws nothing. A state
  that is a synonym for an existing one is not a state. If hit-testing ever
  enters scope, the distinction can be added then without breaking anything.
- The prop is named `Visible`, not `Display`, because `LayoutMode::None`
  already exists and means "passthrough". Two `None`s with opposite
  meanings would be a durable source of bugs; `Display::None` is currently
  unused, since the engine lowers `LayoutMode::None` to `Display::Flex` with
  absolutely-positioned children.
- Figma, Unity, and Android all have exactly these two concepts (`visible`
  plus opacity; `SetActive` plus `CanvasGroup.alpha`; `GONE` plus `alpha`);
  CSS is the outlier. Figma's `visible: false` therefore imports 1:1 with no
  lowering, and P5 is satisfied — this is where the engines already are, not
  a Figma-compatibility concession.

Group opacity is not free: an overlapping subtree at 0 < α < 1 needs an
offscreen composite, which is the mid-frame render-target switch R-T1
restricts (Q-6 in `docs/technotes/open-questions.md` holds the budget
value). α = 0 needs no compositing — the subtree is simply not drawn. The
as-built `Visible` behavior is in `docs/design/dashscene-core-arena.md`
("Visibility") and `docs/design/dashscene-engine.md` ("Visibility").
