# dashscene-core v0.1 — arena + staged-mutation API (design)

    story    #2 (epic #1, v0.1 walking skeleton)
    branch   story/dashscene-core
    date     2026-07-12
    status   working memory — garden before the PR lands

## Goal

The in-memory semantic model for the v0.1 walking skeleton
(`DESIGN_1.md` §5, §7.3, §11): an arena holding a node tree with
fixed-size layout intent and solid-fill paint intent, mutated through
the staged producer API (`open` / `set_prop` / `commit`,
`SCOPE_DECISIONS.md` §9), resolving on commit into the committed
output a painter consumes (boundary B): a rect table, a paint table, a
generation stamp, and a dirty set — double-buffered.

Acceptance (issue #2): a scene can be built by hand via the mutation
API and read back as a resolved rect table + paint table; `just build`
green.

## Scope boundaries

- **No `dashpaint` dependency.** Story #3 defines the painter-side
  types in parallel. Core defines its own committed-output types with
  the pinned boundary-B shapes (below); story #4 reconciles the two
  crates when it wires the Skia painter.
- **No `dashbuf` dependency.** No v0.1 story loads a `.dsb` into the
  arena; `dashbuf`'s round-trip test covers the format side. The arena
  mirrors the schema's _shapes_ (field-for-field semantics), it does
  not link the generated code.
- **No `set_variant`.** `SCOPE_DECISIONS.md` §9: "the v0.1 walking
  skeleton needs `open`/`set_prop`/`commit` but zero animation."
  Variants land at v0.4 with the variant table.
- **No node removal, no reparenting, no validation** (NaN, negative
  sizes). The validator crate enters at its own slice (P4 concerns
  paint-vocabulary profiles, not producer API misuse).

## Pinned boundary-B contract (do not diverge)

- Rect table: flat array; index = document DFS node index.
- Rect entry: blittable plain data — `x, y, w, h` (`f32`) + paint
  index (`u32`).
- Paint: solid fill; color = 4×`f32` RGBA, exactly the shape of
  `dashbuf`'s `Color` struct.
- Double buffer, generation stamp, dirty set live in
  `dashscene-core`.

## Decisions (alternatives considered)

### D1 — Fixed positioning: authored x/y on `FixedSizeLayout`

`dashbuf`'s `FixedSizeLayout` has `width`/`height` but no position.
The walking skeleton needs more than one visible rect, so position
must be authorable.

- **Chosen:** extend the `FixedSizeLayout` struct in `dashbuf.fbs` to
  `x, y, width, height` — an authored, parent-relative offset. An
  authored offset is intent, not a resolved result, so P1 allows it.
  Child absolute position = parent absolute position + child `(x, y)`;
  a root's absolute position is its own `(x, y)`. Resolved absolutes
  appear only in the committed rect table (runtime output, not the
  document).
- Rejected — position as a new field on the `Node` table: the
  FlatBuffers-evolvable route (tables take new fields; structs do
  not), but `FixedSizeLayout` is not a long-lived evolution surface —
  the schema's own header says layout modes become a union when v0.2
  adds them — and no `.dsb` documents exist outside dashbuf's own
  round-trip test, so struct extension is safe today and keeps all
  fixed-layout intent in one place.
- Rejected — position only in the arena (not in the schema): the
  document could then never express position, breaking the v0.9
  same-scene-both-ways exit criterion (E1).

### D2 — Staging semantics: batched publish, not abortable transactions

- **Chosen:** `Arena::open(&mut self) -> Txn<'_>`. The `Txn` holds the
  mutable borrow (one open stage at a time, enforced by the borrow
  checker — "the type checker is the validator's first line",
  DESIGN §6.2) and applies mutations to the intent model immediately.
  `commit(self)` resolves and publishes atomically. Dropping a `Txn`
  without committing leaves the staged intent pending — it publishes
  with the next commit. "Staged" means batched visibility to painters
  (P3), not rollback; no design requirement asks for abort semantics.
- Rejected — op-log with rollback-on-drop: needs provisional node ids
  or slot rollback for `add_node`; complexity with no v0.1 consumer.
- API misuse (a `NodeId` from another arena, out-of-range) panics like
  slice indexing; it is not a named diagnostic (P4 is about design
  vocabulary, not programmer error).

### D3 — Committed output: own types, exact contract shapes

- `RectEntry { x, y, w, h: f32, paint: u32 }`, `#[repr(C)]`, `Copy` —
  blittable per the pinned contract.
- `Color { r, g, b, a: f32 }`, `#[repr(C)]`, `Copy` — the shape of
  `dashbuf`'s `Color`.
- A node with no fill gets paint index `NO_PAINT = u32::MAX`
  (mirroring `dashbuf`'s `NO_PARENT` sentinel); painters skip such
  entries. Flagged for reconciliation with story #3/#4.
- Paint table: deduplicated by exact color value (bit pattern),
  ordered by first use in DFS order — deterministic output (R7).
  Rebuilt at each commit (v0.1 scene sizes make incremental interning
  premature).
- Dirty set: exact diff of consecutive committed rect tables by index
  (an entry differs, or is new). Op-touched tracking was rejected: it
  misses descendants whose absolute position changed via a parent
  move; the exact diff is trivially correct at v0.1 scale.
- Generation stamp: `u64`, increments on every commit (including a
  no-change commit — the stamp says "a commit happened", the dirty set
  says what changed).
- Double buffer: two committed buffers inside the arena; commit
  resolves into the back buffer and flips. `Arena::committed()`
  borrows the front buffer. The concurrency payoff arrives with
  threading later; the mechanism is part of the pinned contract now.

### D4 — Node identity vs. document order

- **Chosen:** `NodeId` is a stable arena slot index, returned by
  `add_node` and never invalidated (no removal in v0.1). DFS document
  order (= rect-table index) is computed at commit: roots in creation
  order, children in creation order under each parent. The committed
  buffer carries the NodeId↔rect-index correspondence
  (`node_of(rect_index)`, `rect_index_of(NodeId)`).
- Rejected — keeping the arena `Vec` itself in DFS order: insertion
  splicing is O(n) and either invalidates ids or forces an id
  indirection table anyway.

## API sketch

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::Width(320.0));
    txn.set_prop(root, Prop::Height(240.0));
    txn.set_prop(root, Prop::Fill(Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
    let child = txn.add_node(Some(root), Some("badge"));
    txn.set_prop(child, Prop::X(10.0));
    txn.set_prop(child, Prop::Y(10.0));
    let generation = txn.commit();

    let scene = arena.committed();
    assert_eq!(scene.generation(), generation);
    assert_eq!(scene.rects().len(), 2);

`Prop` (v0.1): `X(f32)`, `Y(f32)`, `Width(f32)`, `Height(f32)`,
`Fill(Color)`. Node names are set at `add_node` (diagnostics aid, not
a mutable prop in v0.1).

## Module layout

    crates/dashscene-core/src/lib.rs        docs + re-exports
    crates/dashscene-core/src/arena.rs      Arena, NodeData, Txn, Prop
    crates/dashscene-core/src/committed.rs  Color, RectEntry, Paint,
                                            CommittedScene, NO_PAINT
    crates/dashscene-core/tests/arena.rs    integration tests (the
                                            acceptance path)

Plus the D1 schema change in `crates/dashbuf/schema/dashbuf.fbs` and
its round-trip test update (separate commit, `dashbuf` scope).

## Error handling

Nothing returns `Result` in v0.1. Contract violations (foreign or
out-of-range `NodeId`) panic with a clear message. No value
validation (see scope boundaries).

## Testing

TDD; the test list drives implementation order:

1. Empty arena: `committed()` is empty at generation 0; an empty
   commit still bumps the generation and yields an empty dirty set.
2. Single root with size + fill: one rect entry, authored values,
   paint table `[color]`, paint index 0.
3. Nested tree: DFS indices match creation structure; absolutes sum
   ancestor offsets.
4. Interleaved sibling creation: DFS order = roots/children in
   creation order, not global creation order.
5. Paint dedup: two nodes, same color → one paint entry, both rects
   index it; distinct colors → first-use order.
6. Unfilled node: `NO_PAINT`.
7. Staged visibility: mutations invisible in `committed()` until
   `commit()`; visible after.
8. Generation: increments per commit.
9. Dirty set: first commit marks all; a commit moving one parent
   marks the parent and its descendants (absolute positions changed)
   and nothing else; a no-op commit marks nothing.
10. NodeId↔rect-index correspondence round-trips.
11. `dashbuf` round-trip still green with `x`/`y` on
    `FixedSizeLayout`.
