# dashlang — minimal builder DSL (v0.1)

    crate    crates/dashlang
    covers   v0.1 walking skeleton (story #5)

## Purpose

`dashlang` is the Rust DSL skin over `dashscene-core`'s staged-mutation
API (`docs/archive/2026-07-14-design-1-seed.md` §6.2,
`docs/decisions/staged-mutation-v01-scope.md`): it lets a scene be
written as a declaration — an inert value tree — instead of direct
`Arena`/`Txn` calls, then publishes that declaration in one commit.
Components are plain functions returning `Node` values
(`docs/archive/2026-07-14-design-1-seed.md` §6.2: "components are fns");
repetition is an iterator feeding
`Node::children` ("loops are repeaters").

## Value-tree surface

All types and functions live in `crates/dashlang/src/lib.rs`:

- `node(name: &str) -> Node` — a named node description; `anon() ->
  Node` for an unnamed one.
- `rgba(r, g, b, a: f32) -> Color` — a plain constructor for
  `dashscene_core::Color`. Both `Color` and `Arena` are re-exported
  (`pub use dashscene_core::{Arena, Color}`) so a DSL consumer needs
  one import path and no direct `dashscene-core` dependency.
- `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
  parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append
  one), `children(impl IntoIterator<Item = Node>)` (append from an
  iterator). Declaration order is document (DFS) order — core pins
  sibling order to creation order. The v0.2 flex vocabulary (issue
  #118): `mode(LayoutMode)`, `gap(f32)`, `padding(left, top, right,
  bottom: f32)`, `margin(left, top, right, bottom: f32)`,
  `main_align(MainAxisAlign)`, `cross_align(CrossAxisAlign)`,
  `sizing_h(AxisSizing)`, `sizing_v(AxisSizing)`, `min_width(f32)`,
  `max_width(f32)`, `min_height(f32)`, `max_height(f32)` — one method
  per `Prop` variant, mirroring `dashscene_core::Layout`, which `Node`
  embeds directly rather than duplicating its fields.
- `scene(roots: impl IntoIterator<Item = Node>) -> Scene` — collects a
  scene description from one or more root `Node`s, in order.

`Node` and `Scene` are inert: constructing and combining descriptions
stages nothing against an arena.

## Build/commit mapping

`Scene::build(&mut Arena) -> Built` and `Scene::build_with(&mut Arena,
&mut dyn LayoutSolver) -> Built` are the DSL's points of contact with
`dashscene-core`: both open one `Txn`, walk the value tree depth-first
(a private recursive `add` — `add_node` then `set_prop` for every
`Layout` field and, if set, `Fill`, then recurse into children), and
commit exactly once — `build` via `Txn::commit()` (the fixed solver,
flex intent ignored), `build_with` via `Txn::commit_with(solver)` (a
real solver, `dashscene-engine`'s `TaffySolver` being the product
case). Both _add_ their roots to whatever the arena already holds —
the DSL is a producer, not an owner — matching the one-commit model
the future C# describe-buffer skin will use across its FFI seam.

`Built` wraps the commit's generation (`Built::generation() -> u64`).
It is deliberately a named type rather than a bare `u64`: issue #166's
reactive layer is designed to extend it into a live, bindable scene
handle, when it lands, without a second change to `build`'s signature
(`docs/decisions/dashlang-value-tree-builder.md`, "Extension (issue #118)").

## Vocabulary, not semantics

Unset offset/size stay `0.0` and unset fill stays unfilled — exactly
`dashscene-core`'s own defaults. The DSL introduces no validation and
no defaults beyond core's: anything it can express is expressible by
hand against core, with identical committed output. The full rationale
and rejected alternatives (a closure/callback builder over a live
`Txn`; a `scene!{}` macro) are recorded in
`docs/decisions/dashlang-value-tree-builder.md` — not repeated here.

## Module layout

    crates/dashlang/src/lib.rs        crate docs + the whole DSL
                                       (node/anon/rgba/Node/scene/Scene)
    crates/dashlang/tests/builder.rs  acceptance (issues #5, #118): DSL
                                       output == hand-built output;
                                       repeater children; multi-root;
                                       append to an existing arena;
                                       unset-value defaults; the flex
                                       vocabulary reaches the arena;
                                       build_with routes through an
                                       injected solver

One file: the v0.1 surface is small enough that splitting modules
would be structure without content.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §6.2 (Rust DSL
  skin); issue #5 acceptance criteria; issue #118 acceptance criteria
  (flex vocabulary, `build_with`, the SCOPE §23 return-type seam).
- Blocks: #6 (golden harness, done); #46 (the DSL-generated stress
  corpus, unblocked by the flex vocabulary); #166 (reactive bindings,
  which extends `Built` rather than reshaping `build`'s signature).
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
  (this crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md`
  (the `open`/`set_prop`/`commit` API this crate consumes);
  `docs/decisions/flex-vocabulary-shape.md` (the core vocabulary this
  mirrors); `docs/decisions/negative-gap-lowering.md` D3 (the
  deferral #118 resolves).
