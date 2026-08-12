# Remoting rides two transports: UI snapshots + commit deltas, and asset fetch

    status   accepted direction (design session, 2026-07-12) —
             implementation is v1+; the rules under "Binds now"
             constrain current producer-API and schema work
    scope    the .dsb wire role, dashscene-core producer API,
             crates/dashbuf message schema (future)

## Context

`docs/archive/2026-07-14-design-1-seed.md` §4 states the real producer contract
is the arena's staged mutation API; `.dsb` is one way to populate it.
`docs/decisions/dsb-format-and-one-schema.md` pins one schema for the file and
wire roles, with framing as the only difference. The question was how remote UI
streams progressively: what a snapshot is, what a delta is, how they address
nodes, and how asset bytes travel.

## Options

1. Two transports: an ordered, reliable channel for UI snapshots plus commit
   deltas (snapshots address by doc index, deltas by producer handle), and a
   pull channel fetching assets by content hash.
2. Positional deltas: one ordered channel, ops addressing doc indices against a
   generation.
3. One transport for everything, assets included, in stream order.
4. Concurrent-editing machinery (operational transforms / CRDTs) so multiple
   producers can mutate one document.

## Choice

Option 1 — two transports, two message kinds on the first:

- **Transport 1 (ordered, reliable): snapshots + commit deltas.**
  - A **snapshot** is materialized committed state at a generation — the same
    tables as the file's hot sections — streamed as `SnapshotSlice` messages:
    column slices of the parallel tables over a contiguous doc-index range.
    Because the tree is flattened in DFS order, a subtree is a contiguous range
    and parents precede children, so every prefix of the stream is a well-formed
    partial tree; the producer picks slice boundaries at visually coherent
    subtrees.
  - A **delta** is a serialized commit:
    `{base_generation,
    new_generation, ops}` mirroring the staged mutation
    surface — as-built today `add_node` / `set_prop` / `commit` (story #2),
    `set_variant` at v0.4, and subtree removal when the API grows it; ops are
    batched struct-of-arrays. A subtree-creating op carries its new nodes in the
    same slice vocabulary — "a remote update is structurally a small document"
    made literal. A commit may reference `dashcue` transition specs
    (`docs/decisions/staged-mutation-v01-scope.md`); scheduling stays
    runtime-side (P3).
  - **Snapshots speak doc indices; deltas speak producer handles** (see the
    id-model record). The receiving arena owns handle→index mapping and
    re-flattens at commit, exactly as for local producers — remote and local
    producers are indistinguishable.
  - Framing: size-prefixed flatbuffer messages with a union payload, sharing the
    schema's `file_identifier`.
  - Lifecycle is the keyframe model: late join, reconnect, or a delta backlog
    larger than the document all resolve to `RequestSnapshot`, then deltas from
    that generation.
- **Transport 2 (pull, unordered): content-addressed asset fetch.**
  `request(hash, offset?, len?)` → chunk frames; payloads are the same raw blob
  bytes as the file's blob sections (see the asset-model record). Fetch priority
  is driven by what arrived on transport 1 ("referenced by the visible variant,
  first"), with producer prefetch hints as ordering advice on the same queue.

The file role is the two transports materialized: the envelope is the file's
framing exactly as the length prefix is the wire's, and a `.dsb` is a snapshot
whose asset fetches are already resolved into cold sections. The envelope itself
never crosses the wire.

## Why

- **Option 2 (positional deltas)**: structural inserts shift every later index;
  a small logical diff becomes a large physical one, and ordering dependencies
  inside one batch make application fragile.
- **Option 3 (one transport)**: asset bytes queued behind UI messages block
  interaction on downloads (head-of-line blocking); the two flows have opposite
  ordering, priority, and reliability needs.
- **Option 4 (OT/CRDT)**: out of scope — one producer owns a document's mutation
  stream (P3); conflict resolution has no source of conflicts.
- Left open until the wire schema story exists: whether a `SnapshotSlice` can
  reuse the exact generated table types of the file's hot sections, or only the
  same vocabulary — either answer preserves
  `docs/decisions/dsb-format-and-one-schema.md`.

## Receiving-side semantics

- Slices and commits apply atomically through the existing double buffer between
  frames; a half-arrived message is never observable; the generation stamp is
  the sync token.
- Re-layout on arrival is the model: a newly arrived subtree dirties its
  ancestors and the solver re-solves. The producer's slice granularity controls
  visual pop-in; FLIP smooths what moves, fed by the commit pair it already
  receives.
- Interned pools (strings, styles, assets) are append-only within a generation
  stream; appends arrive with or before the first reference; compaction happens
  only at snapshot boundaries.

## Binds now (v0 work)

1. **Handles ≠ indices in the producer API** (dashscene-core): the
   staged-mutation surface must not conflate "node 17" with "slot 17", or
   remoting later becomes an API break. The as-built API conforms (`NodeId` is a
   creation-order newtype with an explicit rect-index translation in the
   committed output).
2. **Pools must become append-stable before deltas exist.** As-built, the
   committed paint pool is re-interned from scratch on every commit — the arena
   documents that an unchanged paint index can reference a different color
   across commits. That is sound for local painters (the dirty set accounts for
   it) but incompatible with deltas that reference pool indices across
   generations. The migration to a stable, append-only interner is named here;
   it is not a property the code has today. The `dashbuf` string/style pools are
   plain index-referenced vectors and are append-compatible as-is.
3. **Subtree-shaped operations reuse the document vocabulary** rather than
   inventing a second encoding.
