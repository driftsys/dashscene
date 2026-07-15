# The scene tree is static after build; dynamic lists are bounded pools

    status   accepted (story #166, 2026-07-15)
    scope    crates/dashlang reactive layer; the dirty diff in
             crates/dashscene-core; the v0.7 importer's list lowering

## Context

Node ids are DFS positions, and the commit's dirty diff compares rects
**by index** (`docs/design/dashscene-core-arena.md`). Inserting one node
into the middle of the tree shifts every subsequent index, so every later
rect is compared against the wrong predecessor and the whole tail of the
scene reports dirty. Structural change does not cost a little in this
architecture — it defeats the dirty set entirely, and a tree that can grow
arbitrarily at runtime makes the frame budget (R4) unprovable from the
document by construction. (`docs/archive/2026-07-14-scope-decisions.md` §23
D3; `docs/archive/2026-07-14-design-1-seed.md` §11.)

## Options

1. True insertion and removal with a keyed reconciler (React/Solid list
   reconciliation, node id recycling).
2. The tree is static after build; a variable-length list is a bounded
   pool of pre-materialized instances, and a length change is a `Visible`
   write plus a rebind. Data longer than the pool shows through a recycled
   window.

## Choice

Option 2. Every node that can ever appear is present after build. A list
that varies in length is a bounded pool sized to a declared maximum; a
member appearing or disappearing is a `Prop::Visible` write, not an
insertion. Genuinely unbounded, fully dynamic surfaces (a map view, a
settings screen) are not modelled as scene nodes at all — they are
`role=placeholder` handoffs to another renderer (DESIGN §10.2).

## Why

- True insertion (option 1) needs node removal and id recycling core has
  never had, it allocates in the update path, it defeats the index-keyed
  dirty diff, and it makes R4 unprovable. It would be reconsidered only for
  a genuinely unbounded rendered list, which this domain does not appear to
  have.
- A bounded pool keeps every DFS index stable, which is the invariant the
  whole update path rests on. `Prop::Visible(false)` lowers to Taffy
  `Display::None` (`docs/decisions/visible-is-layout-opacity-is-paint.md`),
  so a hidden member still resolves to a (degenerate) rect and keeps its
  rect-table index — no index ever shifts — while its container collapses
  because Taffy removes it and its children from the flex flow. This is
  already what "variant closure is per component SET" and "hidden nodes
  export as `visible:false`" imply, so it costs the importer nothing new.

The A4 acceptance case (`a4_bounded_pool_hugging_container_collapses` in
`crates/dashlang/tests/reactive.rs`) exercises exactly this: pool members
toggle `Visible` one at a time and a hugging container grows and collapses
around them, with no insertion and no index shift.
