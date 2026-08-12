# Staged-mutation API v0.1 scope: open/set_prop/commit, batched publish

    status   accepted (story #2, 2026-07-12)
    scope    dashscene-core producer API; dashscene-core vs. dashcue
             ownership

## Context

The canonical producer surface is `open` / `set_prop` / `set_variant` / `commit`
on the arena. `docs/design/architecture.md` describes this staged- mutation
contract as a property of the in-memory arena, but this project's early
crate-name map (`docs/decisions/crate-name-map.md`) had assigned it to `dashcue`
instead — a contradiction AGENTS.md flagged. Story #2 had to settle which crate
owns the API and decide how much of it exists at v0.1, and what "staged" means
precisely.

## The API lives in dashscene-core, not dashcue

**The arena wins: the API is `dashscene-core`'s.** The crate map's assignment to
`dashcue` was a mapping error, not a design decision, and has been corrected to
match.

- `docs/archive/2026-07-14-design-1-seed.md` §4 says it verbatim: "the in-memory
  arena + its staged mutation API (open/set_prop/set_variant/commit) is the real
  contract; `.scb` is one way to populate it" (`.scb` is the document's retired
  working name; see `docs/decisions/dsb-format-and-one-schema.md`). The API is
  defined as a property of the arena, and the arena is `dashscene-core`.
- `commit` is mechanically an arena operation — it swaps the double buffer,
  bumps the generation stamp, updates the dirty set, all state `dashscene-core`
  owns. Housing the API elsewhere means either another crate reaching into
  core's internals, or core exposing a lower-level mutation API anyway that the
  other crate merely wraps.
- Dependency graph: the v0.1 walking skeleton needs `open`/`set_prop`/`commit`
  but zero animation. With the API in core, v0.1 is
  `dashlang → dashscene-core → dashbuf` and `dashcue` doesn't exist until its
  slice (v0.4). The other way round, every producer drags in the animation crate
  to set a property.

What `dashcue` is, precisely: the descriptive animation vocabulary
(`docs/design/dashcue.md`) — transition specs (tween/spring/keyframes), stagger,
per-prop smoothing, loop tracks, keyframe tracks, enter/exit specs — plus their
runtime scheduling/interpolation. The seam: `set_variant` (the structural
switch) is core's; the transition spec describing how that switch animates is
`dashcue` data referenced by the commit. `dashcue` lands with slice v0.4
(variants + staged mutation + minimal FLIP), not before.

## Options

1. Implement `open`/`set_prop`/`commit` now; `set_variant` arrives with the
   variant table at v0.4.
2. Also ship a `set_variant` stub that always fails with a named "variants
   unsupported" error.
3. Full op-log transactions with rollback-on-drop.

## Choice

Option 1, with these semantics:

- `Arena::open(&mut self) -> Txn<'_>`: the `Txn` holds the arena's mutable
  borrow, so exactly one stage can be open and committed output cannot be read
  mid-stage — the borrow checker enforces the contract ("the type checker is the
  validator's first line", `docs/archive/2026-07-14-design-1-seed.md` §6.2).
- "Staged" means batched visibility (P3): mutations apply to the intent model
  immediately but publish to painters only at `commit`. Dropping a `Txn` without
  committing leaves the changes pending; they publish with the next commit. No
  rollback.
- Node construction (`add_node`) is part of the staged surface (raw node
  construction is an explicit part of the producer contract,
  `docs/archive/2026-07-14-design-1-seed.md` §6.2).
- `Prop::Fill` sets a fill but cannot clear one back to unfilled — v0.1 has no
  consumer that unfills a node, and a transparent fill is not the same committed
  output (a transparent solid interns a solid entry; an unfilled node interns
  the shared fill-less entry — `PaintEntry::default()` since story #4,
  `docs/decisions/boundary-b-unification.md`). A clear operation is additive
  when a consumer needs it.
- API misuse (an out-of-range `NodeId`) panics with a message naming the id; a
  `NodeId` from another arena whose index is in range is not detected. P4's
  named-diagnostics rule covers design vocabulary, not programmer error.

## Why

- `docs/archive/2026-07-14-scope-decisions.md` §9 itself scopes the walking
  skeleton: "the v0.1 walking skeleton needs `open`/`set_prop`/`commit` but zero
  animation." There is no variant table for `set_variant` to act on, so a stub
  (option 2) could only ever error — surface area with no behavior. Adding the
  method at v0.4 is additive and non-breaking.
- Rollback (option 3) is required by nothing in the design; it forces
  provisional node ids or slot rollback for `add_node`. The design's contract is
  about when mutations become visible to the frame loop, not about aborting
  them.
