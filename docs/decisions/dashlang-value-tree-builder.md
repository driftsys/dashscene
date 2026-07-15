# dashlang v0.1 is an inert value-tree builder with one build step

    status   accepted (story #5, 2026-07-12)
             extended (story/issue #118, 2026-07-15) — build/build_with
             return Built, not a bare u64
    scope    dashlang DSL surface; binds the golden harness (#6) and
             the future corpus-generator work

## Context

Story #5 needed the minimal Rust DSL skin over `dashscene-core`'s
staged-mutation API (`docs/design/dashlang.md`). The shape of the v0.1 surface
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
- `scene(roots)` collects a `Scene`; `Scene::build(&mut Arena) -> Built`
  appends the roots to the arena (the DSL is a producer, not an
  owner) and publishes them in exactly one commit. `Built` wraps the
  commit generation (`Built::generation() -> u64`) — see "Extension"
  below for why it is a named type rather than the bare `u64` this
  record originally chose.
- Unset values keep core's defaults (zero offset/size, no fill). The
  DSL adds vocabulary, never semantics: anything it expresses is
  expressible by hand against core with identical committed output —
  the acceptance tests assert exactly that.

## Why

- `docs/design/dashlang.md` describes the DSL family this way: "components are fns"
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

## Extension (issue #118, 2026-07-15) — `build`/`build_with` return `Built`

Issue #118's own scope, as handed off, was only the v0.2 flex
vocabulary on `Node` plus `Scene::build_with(arena, solver)`. A
2026-07-13 issue comment ("SCOPE §23") re-scoped it: `build` must also
settle how `NodeId` survives the build, because issue #166's
reactive-bindings design (`docs/wip/2026-07-13-reactive-bindings-spec.md`
at the time; not yet gardened as its own record) resolves a bound
node's identity by declaring the binding on the node and resolving it
to a `PropKey` inside `build`, so a producer never handles a `NodeId`
at all — but only if `build`'s return type is something #166 can
extend without reshaping `build`'s signature a second time.

`dashscene_core::NodeId` was already private outside `dashlang::add()`'s
internal use before #118, and stays that way: `add()` already holds the
concrete `NodeId` at exactly the moment a future binding declaration
would need to resolve it, entirely inside `add()`. What actually needed
deciding was only the _return type_ of `build`/`build_with` — a bare
`u64` cannot grow `.set()`/`.tick()` methods later without breaking
every call site.

**Choice.** Both `Scene::build` and the new `Scene::build_with(&self,
arena: &mut Arena, solver: &mut dyn LayoutSolver)` return:

    pub struct Built { generation: u64 }
    impl Built { pub fn generation(self) -> u64 { self.generation } }

`Built` is deliberately minimal — no `PropKey`, no signal table, no
`LiveScene` name — because #118 does not implement bindings; #166
extends `Built` (new fields or methods, or a wrapper) when it lands
that machinery. `docs/design/dashlang.md`'s "Build/commit mapping"
section carries the as-built description of both methods.

**Alternatives considered.**

- **Keep `build` returning `u64`, let #166 reshape it later.** Rejected
  per the re-scope: this is exactly the "reshape twice" cost SCOPE §23
  identifies, and wrapping one integer in a struct now is cheap enough
  that deferring it has no advantage.
- **Resolve `PropKey`/bindings now, in #118.** Rejected: `PropKey`
  lives in `dashcue`, and #166's own design is what decides whether
  `dashlang` takes a `dashcue` dependency at all — pulling that forward
  into #118 would implement half of #166 against a design that had not
  been planned yet.
- **Name the return type `LiveScene` now.** Rejected: nothing is "live"
  without a signal table or a `tick`; naming it that now would promise
  behavior #118 does not deliver. `Built` is honest about what exists
  today and is #166's to rename or wrap.

Blast radius at the time: `crates/dashlang/tests/builder.rs` (five call
sites) and the crate-doc example in `lib.rs`, each changing
`generation == N` to `generation.generation() == N`.

Related: `docs/decisions/dashlang-flex-vocabulary.md` (the flex
vocabulary #118 also added, decided separately from this return-type
change).
