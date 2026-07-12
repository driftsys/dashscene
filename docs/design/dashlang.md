# dashlang — minimal builder DSL (v0.1)

    crate    crates/dashlang
    covers   v0.1 walking skeleton (story #5)

## Purpose

`dashlang` is the Rust DSL skin over `dashscene-core`'s staged-mutation
API (`DESIGN_1.md` §6.2, `SCOPE_DECISIONS.md` §9): it lets a scene be
written as a declaration — an inert value tree — instead of direct
`Arena`/`Txn` calls, then publishes that declaration in one commit.
Components are plain functions returning `Node` values (DESIGN §6.2:
"components are fns"); repetition is an iterator feeding
`Node::children` ("loops are repeaters").

## Value-tree surface

All types and functions live in `crates/dashlang/src/lib.rs`:

- `node(name: &str) -> Node` — a named node description; `anon() ->
  Node` for an unnamed one.
- `rgba(r, g, b, a: f32) -> Color` — a plain constructor for
  `dashscene_core::Color`, re-exported so DSL users need one import
  path.
- `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
  parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append
  one), `children(impl IntoIterator<Item = Node>)` (append from an
  iterator). Declaration order is document (DFS) order — core pins
  sibling order to creation order.
- `scene(roots: impl IntoIterator<Item = Node>) -> Scene` — collects a
  scene description from one or more root `Node`s, in order.

`Node` and `Scene` are inert: constructing and combining descriptions
stages nothing against an arena.

## Build/commit mapping

`Scene::build(&mut Arena) -> u64` is the DSL's only point of contact
with `dashscene-core`: it opens one `Txn`, walks the value tree
depth-first (a private recursive `add` — `add_node` then `set_prop`
for `X`/`Y`/`Width`/`Height` and, if set, `Fill`, then recurse into
children), and commits exactly once, returning the commit's
generation. `build` _adds_ its roots to whatever the arena already
holds — the DSL is a producer, not an owner — matching the one-commit
model the future C# describe-buffer skin will use across its FFI seam.

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
    crates/dashlang/tests/builder.rs  acceptance (issue #5): DSL output
                                       == hand-built output; repeater
                                       children; multi-root; append to
                                       an existing arena; unset-value
                                       defaults

One file: the v0.1 surface is small enough that splitting modules
would be structure without content.

## Trace

- Satisfies: `specs/DESIGN_1.md` §6.2 (Rust DSL skin); issue #5
  acceptance criteria.
- Blocks: #6 (golden harness); later DSL slices (the stress-corpus
  generator; v0.4 variants, once `dashcue` enters the graph).
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
  (this crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md`
  (the `open`/`set_prop`/`commit` API this crate consumes).
