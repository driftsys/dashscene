# Variant table: flat member index, narrow overridable-prop vocabulary

    status   accepted (story #20, 2026-07-15)
    scope    crates/dashbuf, crates/dashscene-core — the variant table
             and `set_variant` commit-time resolution

## Context

`docs/archive/2026-07-14-design-1-seed.md` §5 pins the shape of the
variant table itself: "sparse per-variant overrides, never duplicate
trees." What it does not pin is how a variant is _selected_, or which
props a variant is allowed to override. `docs/wip/2026-07-13-reactive-bindings-spec.md`
(a later, not-yet-built consumer) flags both as open: "#20's API shape
(flat variant index versus axis-keyed selection) is still undecided,"
and "#20 ... gains a second consumer [`bind_variant`]. Its undecided API
shape should be settled with `bind_variant` in view." This story is
where that settling happens.

`docs/technotes/glossary.md`: "variant closure is per component SET
(runtime can select any member)" — so selection is scoped to a named
group of alternatives (a "variant set," Figma's "component set"), not
one flat selection for the whole document: a Button's `State` variant
and a Toggle's `On/Off` variant switch independently.

## Options

Two independent choices this story has to make: how a variant is
selected, and which props a variant member is allowed to override.

**Selection shape:**

1. **Flat member index.** A `VariantSet` holds an ordered list of
   `VariantMember`s; `set_variant(set, member_index)` selects one by
   position. A component with two independent Figma properties (`State`
   × `Size`) is modeled as one set whose members are the full cross
   product (`Default/Small`, `Default/Large`, `Hover/Small`, …).
2. **Axis-keyed selection.** A `VariantSet` holds named axes, each with
   named values (`State: [Default, Hover, Pressed]`, `Size: [Small,
   Large]`); selection sets one axis at a time
   (`set_variant(set, "State", "Hover")`), and the runtime composes the
   overrides of every selected axis value.

**Overridable-prop vocabulary:**

1. Support the full `Prop` vocabulary (every settable prop) as a
   variant override from day one.
2. Support only the props needed to prove the acceptance criteria
   (resolved rect + paint tables): `X`, `Y`, `Width`, `Height`, and the
   solid-fill shorthand — the same five `Prop` started with at v0.1.

## Choice

Selection shape: option 1, flat member index. Overridable-prop
vocabulary: option 2, the narrow five-prop slice.

## Why

**Selection shape:**

- The design-1-seed contract is per-member overrides ("sparse
  per-variant overrides"), not per-axis-value overrides — option 1 maps
  directly onto that shape with no translation layer. Option 2 would
  need to decide how per-axis overrides _compose_ when two axes both
  touch the same prop (e.g. `State=Hover` and `Size=Large` both set
  `Width`), which the design doc does not address and no consumer in
  this repo has a stated need for yet.
- "Variant closure is per component SET" already gives option 1
  everything it needs: the full cross product is enumerable and finite
  by that same closure property, so flattening it costs nothing at
  import time and loses no expressiveness a v0.4 consumer needs.
- Option 2 is strictly more general — a flat scheme cannot represent
  "vary one axis independently while composing with another" without
  the producer pre-flattening the cross product itself, which is
  exactly what option 1 asks it to do. Nothing downstream needs that
  independence yet: `bind_variant` (the reactive-bindings spec's
  concrete consumer) binds a Figma STRING variable to "the variant
  property of an `InstanceNode`" — one string, i.e. one flat selection
  — not to an independent per-axis binding. This choice leaves the door
  open: `VariantSet` and `VariantMember` are both additive, named tables
  (`name: string` on `VariantMember`), so a future axis-keyed scheme can
  append alongside the flat one without breaking it (R7) — the flat
  scheme is not removed, a richer one is added.
- Keeping selection flat also keeps this story's schema surface small:
  one new union, one override table, one member table, one set table —
  matching the "walking skeleton" precedent every other vocabulary
  slice (v0.2 flex, v0.3 paint, v0.5 text) followed here.

**Overridable-prop vocabulary:**

- The story's acceptance criterion is "switching variants produces the
  correct resolved rect/paint tables." Nothing in scope exercises a
  text, flex, stroke, corner, or clip override, and `docs/design/dashscene-core-arena.md`
  already documents that text and flex props do not reach the v0.1–v0.5
  committed output at all — a variant override for either would have no
  observable effect to test.
- `Prop` itself grew this way: v0.1 shipped `X`/`Y`/`Width`/`Height`/
  `Fill` only; flex (v0.2), stroke/corners/clip (v0.3), and text (v0.5)
  each appended their own `Prop` variants in their own slice. Widening
  variant overrides is the same append-only move whenever a future slice
  needs it (a new `VariantX`-shaped table plus a new `VariantPropValue`
  union member — R7-safe).
- A dedicated `dashscene_core::VariantValue` enum (not a reuse of
  `Prop`) keeps the override vocabulary honest about what the schema
  actually carries: `Prop` also holds `Text(String)`, `Stroke(..)`,
  and the flex props, none of which this story's schema or loader
  understands as an override. Accepting the full `Prop` type in
  `add_variant_set` would let a caller stage an override the schema
  cannot round-trip.

## Resolution model (how overrides reach the committed output)

`Arena::layout(node)` (the public read seam every `LayoutSolver` —
including the internal `FixedSolver` and `dashscene-engine`'s
`TaffySolver` — resolves geometry through) applies the active member's
`X`/`Y`/`Width`/`Height` overrides on top of the node's base layout
before returning it. Commit's paint-interning step does the same for
`Fill`. Because both sites feed the _existing_ resolve-then-diff
pipeline (`docs/design/dashscene-core-arena.md`'s "Commit resolution
pipeline"), the dirty-set diff that already compares resolved rect bits
and resolved paint keys against the previous commit needs no change at
all to satisfy "dirty set covers switched props" — a variant-driven
geometry or fill change is indistinguishable, at the diff step, from
the same change made through `set_prop`.

`set_variant` is staged like any other mutation (P3: "producers mutate
structure, props, and variant switches whenever they like"): it writes
the arena's active-member index immediately, visible through
`Arena::layout`/`Arena::active_variant` before the next commit, and
published to painters only at `commit`.
