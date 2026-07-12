# Remoting rides two transports: UI snapshots + commit deltas, and asset fetch

    status   accepted direction (design session, 2026-07-12) —
             implementation is v1+; the rules under "Binds now"
             constrain current producer-API and schema work
    scope    the .dsb wire role, dashscene-core producer API,
             crates/dashbuf message schema (future)

## Context

DESIGN §4 states the real producer contract is the arena's staged
mutation API; `.dsb` is one way to populate it. SCOPE_DECISIONS §3
pins one schema for the file and wire roles, with framing as the only
difference. The question was how remote UI streams progressively:
what a snapshot is, what a delta is, how they address nodes, and how
asset bytes travel.

## Choice

Two transports, two message kinds on the first:

- **Transport 1 (ordered, reliable): snapshots + commit deltas.**
  - A **snapshot** is materialized committed state at a generation —
    the same tables as the file's hot sections — streamed as
    `SnapshotSlice` messages: column slices of the parallel tables
    over a contiguous doc-index range. Because the tree is flattened
    in DFS order, a subtree is a contiguous range and parents precede
    children, so every prefix of the stream is a well-formed partial
    tree; the producer picks slice boundaries at visually coherent
    subtrees.
  - A **delta** is a serialized commit: `{base_generation,
    new_generation, ops}` mirroring the staged API (set_prop /
    set_variant / insert / remove), batched struct-of-arrays.
    An insert carries its subtree in the same slice vocabulary — "a
    remote update is structurally a small document" made literal.
    A commit may reference `dashcue` transition specs (SCOPE §9);
    scheduling stays runtime-side (P3).
  - **Snapshots speak doc indices; deltas speak producer handles**
    (see the id-model record). The receiving arena owns handle→index
    mapping and re-flattens at commit, exactly as for local
    producers — remote and local producers are indistinguishable.
  - Framing: size-prefixed flatbuffer messages with a union payload,
    sharing the schema's `file_identifier`.
  - Lifecycle is the keyframe model: late join, reconnect, or a delta
    backlog larger than the document all resolve to `RequestSnapshot`,
    then deltas from that generation.
- **Transport 2 (pull, unordered): content-addressed asset fetch.**
  `request(hash, offset?, len?)` → chunk frames; payloads are the same
  raw blob bytes as the file's blob sections (see the asset-model
  record). Fetch priority is driven by what arrived on transport 1
  ("referenced by the visible variant, first"), with producer prefetch
  hints as ordering advice on the same queue.

The file role is the two transports materialized: the envelope is the
file's framing exactly as the length prefix is the wire's, and a
`.dsb` is a snapshot whose asset fetches are already resolved into
cold sections. The envelope itself never crosses the wire.

## Receiving-side semantics

- Slices and commits apply atomically through the existing double
  buffer between frames; a half-arrived message is never observable;
  the generation stamp is the sync token.
- Re-layout on arrival is the model: a newly arrived subtree dirties
  its ancestors and the solver re-solves. The producer's slice
  granularity controls visual pop-in; FLIP smooths what moves, fed by
  the commit pair it already receives.
- Interned pools (strings, styles, assets) are append-only within a
  generation stream; appends arrive with or before the first
  reference; compaction happens only at snapshot boundaries.

## Binds now (v0 work)

1. **Handles ≠ indices in the producer API** (dashscene-core): the
   staged-mutation surface must not conflate "node 17" with "slot 17",
   or remoting later becomes an API break.
2. **Pools stay append-friendly** (strings, styles, future asset
   entries).
3. **Subtree-shaped operations reuse the document vocabulary** rather
   than inventing a second encoding.

## Alternatives considered

- **Positional deltas (index-addressed ops)** — rejected: structural
  inserts shift every later index; a small logical diff becomes a
  large physical one.
- **One transport for everything** — rejected: asset bytes behind UI
  messages block interaction on downloads (head-of-line blocking);
  the two flows have opposite ordering and reliability needs.
- **Concurrent-editing machinery (operational transforms / CRDTs)** —
  out of scope: one producer owns a document's mutation stream (P3);
  conflict resolution has no source of conflicts.
- **Whether a `SnapshotSlice` can reuse the exact generated table
  types of the file's hot sections, or only the same vocabulary** —
  left open until the wire schema story exists; either answer
  preserves SCOPE §3.
