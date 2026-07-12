# dashscene-core: arena + staged-mutation API (v0.1)

`dashscene-core` is the semantic model: an arena holding a node tree
with layout and paint intent (DESIGN_1.md §5), mutated through the
staged producer API (`open`/`set_prop`/`commit`, SCOPE_DECISIONS.md
§9), resolving on commit into the committed output a painter consumes
(boundary B, DESIGN_1.md §7.3). v0.1 scope: fixed-size layout, solid
fill, no Taffy, no variants — the walking skeleton
(DESIGN_1.md §11).

Source: `crates/dashscene-core/src/lib.rs`, `src/arena.rs`,
`src/committed.rs`. Acceptance path: `crates/dashscene-core/tests/arena.rs`.

## Intent model

`Arena` holds `Vec<NodeData>` (one entry per node, indexed by arena
slot) plus a `roots: Vec<NodeId>` in creation order. Each `NodeData`
carries an optional name, an optional parent, its children in creation
order, the authored `x`/`y`/`width`/`height` offset, and an optional
fill color — a direct mirror of the `dashbuf` schema shapes
(`FixedSizeLayout`, `SolidFill`) without linking the generated code
(the crate has no `dashbuf` dependency; see Scope boundaries below).

`NodeId` is a stable arena slot index (`u32`), returned by `add_node`
and never invalidated — v0.1 has no node removal. It is deliberately
distinct from document DFS order: DFS order (which doubles as the
rect-table index, matching `dashbuf`'s flattened tree, DESIGN_1.md §5)
is recomputed at every commit, not maintained as the arena's storage
order. Keeping the arena `Vec` itself in DFS order was considered and
rejected — insertion splicing to keep siblings contiguous is O(n) per
insert and either invalidates previously-issued ids or forces an id
indirection table anyway, for no v0.1 benefit.

## Staged mutation (`Txn`)

`Arena::open(&mut self) -> Txn<'_>` returns a `Txn` holding the
arena's mutable borrow, so exactly one stage can be open at a time and
committed output cannot be read mid-stage — the borrow checker
enforces the contract. `Txn::add_node(parent, name)` and
`Txn::set_prop(node, prop)` mutate the intent model immediately;
nothing is visible to `Arena::committed()` until `Txn::commit(self)`
resolves and publishes. Dropping a `Txn` without committing leaves the
staged changes pending — they publish with the next commit. `Prop`
(v0.1 vocabulary): `X`, `Y`, `Width`, `Height`, `Fill(Color)`; node
names are set at `add_node`, not a mutable prop.

Contract misuse (an out-of-range `NodeId`) panics with a message
naming the id and the arena. A `NodeId` from _another_ arena whose
index happens to be in range is not detected — ids carry no arena
identity. These are programmer-error panics, not part of the P4
named-diagnostics vocabulary. `Prop::Fill` can set a fill but never
clear one back to `NO_PAINT` — a deliberate v0.1 gap
(`docs/decisions/staged-mutation-v01-scope.md`).

Full rationale and the rejected alternative (op-log with
rollback-on-drop): `docs/decisions/staged-mutation-v01-scope.md`.

## Commit resolution pipeline

`Txn::commit` (in `arena.rs`) runs in one pass:

1. **DFS order** — an explicit stack seeded with `roots` (reversed, so
   pop order is creation order), pushing each node's children reversed
   for the same reason. Walk order is roots in creation order, then
   children in creation order under each parent, depth-first. This
   order is the committed rect table's index.
2. **Resolve + intern** — for each node in DFS order: absolute
   position = parent's already-resolved absolute (0,0 for a root) +
   the node's own authored offset; paint = the fill color interned by
   exact bit pattern (`f32::to_bits` on each channel) in first-use
   order, or `NO_PAINT` (`u32::MAX`) if the node has no fill.
3. **Dirty diff** — after building the new rect table, each index is
   compared against the same index in the previous front buffer; an
   entry is dirty when its bits changed (compared via `f32::to_bits`,
   so NaN does not self-compare unequal forever) **or** when its
   resolved fill color changed. The second clause exists because the
   paint table is re-interned every commit: a stable paint index can
   reference a different color (a fill change on the only filled node
   keeps index 0), and an index shift can leave the color unchanged —
   entry-bit equality alone would miss real repaints. This is a
   per-index diff, not an op-touched set — a node whose _ancestor_
   moved gets its own absolute recomputed and so shows up as changed
   even though no `set_prop` touched it directly.
4. **Publish** — generation = previous generation + 1 (every commit
   increments, including a no-op commit); the new `CommittedScene` is
   written into the back buffer and `front` flips.

Fixed-position authoring (why `x`/`y` exist as an offset on the
document's `FixedSizeLayout` rather than only in the arena) and the
committed-output shapes this pipeline produces are pinned decisions,
not re-derived here:
`docs/decisions/fixed-position-authoring.md`,
`docs/decisions/core-committed-output-shape.md`.

## Committed output (boundary B)

`CommittedScene` (in `committed.rs`) is the double-buffered painter
input: `rects() -> &[RectEntry]` (DFS-indexed, blittable
`{ x, y, w, h: f32, paint: u32 }`), `paints() -> &[Paint]`
(deduplicated `{ color: Color }`, `Color` = 4×`f32` RGBA),
`generation() -> u64`, `dirty() -> &[u32]`, plus the
NodeId↔rect-index correspondence for the commit that produced it
(`node_of(rect_index) -> NodeId`,
`rect_index_of(NodeId) -> Option<u32>` — `None` for a node added after
that commit). `Arena` holds two `CommittedScene` buffers and a `front`
index; `Arena::committed()` borrows the front buffer only.

The exact pinned shapes (byte sizes, the `NO_PAINT` sentinel, paint
dedup/ordering rule, dirty-set definition) and why `dashscene-core`
owns these types rather than depending on `dashpaint` or reusing
`dashbuf`'s generated structs: `docs/decisions/core-committed-output-shape.md`.

## Schema change (`dashbuf`)

`FixedSizeLayout` in `crates/dashbuf/schema/dashbuf.fbs` gained
authored `x`/`y` fields alongside the existing `width`/`height`,
covered by `crates/dashbuf/tests/roundtrip.rs`. This is the document
side of the same decision as the arena's offset resolution:
`docs/decisions/fixed-position-authoring.md`.

## Scope boundaries (v0.1)

- No `dashpaint` dependency — story #3 defined the painter-side types
  in parallel (now on `main`: an identical `RectEntry` shape and a
  `PaintTable` whose `resolve` panics on an out-of-range index);
  story #4 reconciles the two crates. Known reconciliation point:
  core emits `NO_PAINT` (`u32::MAX`) for unfilled nodes, while
  `dashpaint`'s `Painter` contract paints every rect and treats an
  unresolvable index as a broken contract — story #4 must decide how
  an unfilled node crosses boundary B
  (`docs/decisions/core-committed-output-shape.md`).
- No `dashbuf` dependency — the arena mirrors the schema's field
  shapes; nothing links the generated flatbuffer code.
- No `set_variant` — `SCOPE_DECISIONS.md` §9 scopes v0.1 to
  `open`/`set_prop`/`commit`. Variants land at v0.4 with the variant
  table.
- No node removal, no reparenting, no fill clearing, no value
  validation (NaN, negative sizes). The validator crate enters at its
  own slice.

## Module layout

    crates/dashscene-core/src/lib.rs        crate docs + re-exports
    crates/dashscene-core/src/arena.rs      Arena, NodeId, Prop, Txn,
                                            commit resolution
    crates/dashscene-core/src/committed.rs  Color, RectEntry, Paint,
                                            CommittedScene, NO_PAINT
    crates/dashscene-core/tests/arena.rs    acceptance path (issue #2):
                                            empty-commit, single-root
                                            resolve, DFS nesting and
                                            interleaved creation order,
                                            paint interning/dedup,
                                            NO_PAINT, staged visibility,
                                            generation stamping,
                                            dirty-set diffing,
                                            NodeId↔rect-index round-trip
