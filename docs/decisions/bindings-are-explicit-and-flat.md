# Bindings are explicit and flat; a binding connects data to one prop on one node

    status   accepted (story #166, 2026-07-15)
    scope    crates/dashlang reactive layer; the v0.7 validator's
             containment proof, which depends on the graph being static

## Context

A reactive layer has to decide how a binding's dependencies are known and what a
binding may connect. SolidJS and `leptos_reactive` _discover_ a computation's
dependencies by running it and recording signal reads on a thread-local stack. A
separate temptation is to model layout consequences ("when the alert list grows,
the panel below moves down") as edges in the same graph. Both are rejected.
(`docs/archive/2026-07-14-scope-decisions.md` §23 D2 and Non-goals;
`docs/archive/2026-07-14-design-1-seed.md` §11.)

## Options

Two axes, each with two options.

1. **Dependency tracking, implicit** — discover a binding's source signals by
   executing it (SolidJS, `leptos_reactive`).
2. **Dependency tracking, explicit** — each binding names its source signal, its
   target channel, and a transform, in a flat table known before anything runs.
3. **Node coupling, allowed** — a binding may relate two nodes (a signal edge
   from one node's state to another node's prop).
4. **Node coupling, forbidden** — a binding connects data to one prop on one
   node only; every "this pushes that" consequence propagates through the
   solver.

## Choice

Options 2 and 4.

- A binding names its source `Signal`, its target `Channel`
  (`X`/`Y`/`Width`/`Height`) or text or visibility, and a `Transform`. The
  dependency graph is a flat table (`Vec<ScalarBinding>` / `Vec<TextBinding>` /
  `Vec<VisibleBinding>` in `crates/dashlang/src/reactive.rs`), resolved to
  `NodeId` targets at build, so a producer never handles a `NodeId` and cannot
  hold a stale one. Derived values are explicit combinators (`Signal::map`,
  `scale`, `map_range`, `clamp`, `format`), each storing one slot in the same
  flat table — no `Rc`, no `RefCell`, no object graph.
- A binding never connects two nodes. "When the list grows, the panel below
  moves" is a _layout_ relationship: the list and panel are siblings in a flex
  column, the list's content size changes, and the solver moves the panel.

## Why

- An implicitly discovered dependency set can differ between runs, so the
  subscription list is rebuilt on every execution — per-frame bookkeeping and
  allocation that R-T4 excludes. It needs an ambient thread-local context that
  does not survive the FFI seam. Decisively, it makes the graph knowable only at
  runtime, so a prop cannot be statically classified as layout-affecting or
  paint-only — which inverts P4 (the vocabulary would be discovered, not
  validated) and forfeits the containment proof that lets a contained scalar
  write skip the solve (the A1 acceptance case,
  `a1_contained_scalar_write_performs_no_layout_solve`). A flat, declarative
  table is statically classifiable, so `write_is_single_rect` can decide at
  build time whether a write stays in one rect.
- Modelling a node-to-node consequence as a signal edge re-implements layout
  inside the binding table, which is exactly what P2 forbids — one solver, and
  it is Taffy. Keeping a binding to data-into-one-prop is the single most
  important simplification: it keeps the binding table flat and the reactive
  layer ignorant of the tree.

This is what makes containment provable and R4 a check a machine can run (once
the binding table is in core, v0.7): a channel is bound, it sits under N hug
ancestors, so a write to it reflows M nodes, and a 60 Hz binding under a hug
ancestor becomes a named diagnostic.

## As built — the containment check (issue #257)

The load gate carries the check, as `binding.reflow-not-contained`
(`crates/dashscene-validator/src/document.rs`). For every binding row whose
channel reaches the solver it walks the target node's parent chain and names any
ancestor that hugs. It runs in the validator rather than the engine because the
binding loop and the hug predicate both already live there, and one
implementation of containment is worth more than a second that can disagree with
it.

Three things the promise above left open, decided here.

- **Which channels count.** `X`, `Y`, `Width`, `Height` and `Gap` reach the
  solver; `FillR`–`FillA` and `Opacity` are paint-only
  (`docs/decisions/visible-is-layout-opacity-is-paint.md`). That is the same
  split `dashlang::reactive::classify` already makes for
  `WriteClass::PaintOnly`, so the gate and the runtime read one line rather than
  two. A channel appended to `BindingChannel` that the split does not name fails
  `every_binding_channel_is_classified` instead of being treated as paint-only
  in silence (P4).
- **The threshold is one hug ancestor, not a depth.** A hug ancestor resizes
  with its content, so the write travels up to the nearest fixed ancestor and
  back down through everything under it — the reflow leaves the bound node's
  subtree at the first hug, and every further hug only widens it. A depth
  threshold would have to name a number nothing has measured.
- **A hug on either axis breaks containment, not only the bound axis.** A width
  change becomes a height change wherever text rewraps or a `Wrap` container
  relines, so the two axes are not independent and a per-axis test would
  under-report. This is the same either-axis predicate
  `dashlang::reactive::ancestor_contained` already applies to decide whether a
  write may skip the solve.

The diagnostic is a warning, not an error: the document renders correctly and
the reflow is authored intent, so a target that accepts the cost declares a
waiver. An error is never waivable, which would leave no way to accept it.

What the check does not prove. It answers "does this write's reflow leave the
bound node's subtree", not "how many nodes reflow" — the M in the promise above
is still uncounted, and counting it needs the solver, not the document. It says
nothing about how often a binding is written: the flat table records that a
signal drives a channel, never the write rate, so a "60 Hz binding" is not a
document construct and the check treats every binding alike. And it is a static
check on authored structure, so it cannot see a reflow that only a particular
signal value produces.
