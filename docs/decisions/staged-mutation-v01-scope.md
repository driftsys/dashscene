# Staged-mutation API v0.1 scope: open/set_prop/commit, batched publish

    status   accepted (story #2, 2026-07-12)
    scope    dashscene-core producer API

## Context

The canonical producer surface is `open` / `set_prop` / `set_variant` /
`commit` on the arena (SCOPE_DECISIONS.md §9). Story #2 had to decide
how much of it exists at v0.1 and what "staged" means precisely.

## Options

1. Implement `open`/`set_prop`/`commit` now; `set_variant` arrives with
   the variant table at v0.4.
2. Also ship a `set_variant` stub that always fails with a named
   "variants unsupported" error.
3. Full op-log transactions with rollback-on-drop.

## Choice

Option 1, with these semantics:

- `Arena::open(&mut self) -> Txn<'_>`: the `Txn` holds the arena's
  mutable borrow, so exactly one stage can be open and committed
  output cannot be read mid-stage — the borrow checker enforces the
  contract ("the type checker is the validator's first line",
  DESIGN §6.2).
- "Staged" means batched visibility (P3): mutations apply to the
  intent model immediately but publish to painters only at `commit`.
  Dropping a `Txn` without committing leaves the changes pending; they
  publish with the next commit. No rollback.
- Node construction (`add_node`) is part of the staged surface (raw
  node construction is an explicit part of the producer contract,
  DESIGN §6.2).
- API misuse (a `NodeId` from another arena) panics like slice
  indexing; P4's named-diagnostics rule covers design vocabulary, not
  programmer error.

## Why

- SCOPE_DECISIONS.md §9 itself scopes the walking skeleton: "the v0.1
  walking skeleton needs `open`/`set_prop`/`commit` but zero
  animation." There is no variant table for `set_variant` to act on,
  so a stub (option 2) could only ever error — surface area with no
  behavior. Adding the method at v0.4 is additive and non-breaking.
- Rollback (option 3) is required by nothing in the design; it forces
  provisional node ids or slot rollback for `add_node`. The design's
  contract is about when mutations become visible to the frame loop,
  not about aborting them.
