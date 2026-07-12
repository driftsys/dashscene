# dashlang v0.1 is an inert value-tree builder with one build step

    status   accepted (story #5, 2026-07-12)
    scope    dashlang DSL surface; binds the golden harness (#6) and
             the future corpus-generator work

## Context

Story #5 needed the minimal Rust DSL skin over `dashscene-core`'s
staged-mutation API (DESIGN_1.md §6.2). The shape of the v0.1 surface
fixes how components, repetition, and commit boundaries look for every
later DSL slice.

## Options

1. An inert value tree (`Node` descriptions composed by plain
   functions) published by one `Scene::build(&mut Arena)` call.
2. A closure/callback builder mutating a live `Txn`
   (`scene(&mut arena, |s| { s.node("bg") ... })`).
3. A `scene! {}` macro.

## Choice

Option 1:

- `node(name)` / `anon()` return `Node` values; consuming chainable
  setters (`at`, `size`, `fill`, `child`, `children`); declaration
  order is document (DFS) order.
- `scene(roots)` collects a `Scene`; `Scene::build(&mut Arena) -> u64`
  appends the roots to the arena (the DSL is a producer, not an
  owner) and publishes them in exactly one commit.
- Unset values keep core's defaults (zero offset/size, no fill). The
  DSL adds vocabulary, never semantics: anything it expresses is
  expressible by hand against core with identical committed output —
  the acceptance tests assert exactly that.

## Why

- DESIGN §6.2 describes the DSL family this way: "components are fns"
  (plain functions returning `Node` values compose without lifetimes),
  "loops are repeaters" (iterators feeding `children`), and the C#
  skin later builds a describe buffer with one commit across the FFI
  seam — one `build` = one commit gives the Rust skin the same
  commit-boundary model now.
- A live-`Txn` closure builder (option 2) couples user code to
  transaction lifetimes, and a panic mid-closure leaves half-staged
  intent that core's no-rollback staging publishes with the next
  commit; the inert tree stages nothing until `build`.
- A macro (option 3) adds surface syntax but requires implementation
  and diagnostics work; nothing in v0.1 needs it, and a macro can wrap
  the value tree later without breaking any caller.
