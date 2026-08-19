# A placeholder is a nested table, and declares its measure size

    status   accepted (story #1126, 2026-08-18)
    scope    crates/dashbuf — table Placeholder and Node.placeholder;
             crates/dashc lowering; crates/dashscene-core's read surface

## Context

Node replacement is where a host's own content meets a dashscene document: the
premise the Unity design was worked out against (issue #851) is that
continuously-changing content is engine geometry the host owns, and dashscene
owns the 2D layout, typography and chrome around it. Without it an embedded host
draws a document _beside_ engine content rather than _with_ it.

`docs/design/architecture.md` described `Node` as carrying four fields for this
— `contribution_id`, `fragment_ref`, `declared_size`, `interim_fill` — and none
existed; the record said "already carries" until the v0.19 phase-end revision
corrected it (issue #876). Story #1126 builds the surface. Two questions the
prose did not settle had to be settled to build it, because a `.dsb` field is
append-only and cannot be withdrawn.

`docs/archive/2026-07-14-design-1-seed.md` §10.2 calls it a "placeholder node
**kind**"; `architecture.md` says `Node` **carries the fields**. Those are
different schemas.

## Decision 1 — one nested table, and its presence is the predicate

`Node` gains a single field, `placeholder: Placeholder`, holding all four.

**A node carrying the table is a declared placeholder; a node without one is
not.** Story #1127's diagnostic distinguishes four states — filled, unfilled,
undeclared overload, ordinary — and presence of a table answers the
placeholder/not axis exactly.

**Amended 2026-08-19 by story #1127**, which built that diagnostic: this
paragraph said its two warnings "both turn on that predicate", and neither turns
on it alone. `placeholder.undeclared-overload` turns on no placeholder at all —
its subject is a binding the document does not declare — and
`placeholder.unfilled` reads `contribution_id` and `fragment_ref` after the
predicate has classified the node, to decide whether a host was ever owed a
contribution for it
([`a-host-binds-a-contribution-by-id.md`](a-host-binds-a-contribution-by-id.md),
Decision 3). The decision below is unchanged, and so is its reasoning: what
presence buys is that placeholder-ness is not a convention over field values.
Four loose fields would make the predicate a convention ("`contribution_id` is
set") that #1127 would then depend on, and would spend four `Node` field ids
rather than one.

Nesting is also what `Node` already does for a grouped concern: `layout`,
`flex`, `constraints` and `paint` are all nested tables.

## Decision 2 — `declared_size` is the measure size, not a second box

`Node` already states its box (`layout.width`/`height`) and its sizing mode
(`constraints.sizing_h`/`sizing_v` over `Fixed`/`Hug`/`Fill`). A `declared_size`
restating the box would be a second source of truth that can disagree with the
first.

It is not that. `declared_size` is **what a measure callback reports while no
contribution is bound** — an intrinsic size for a node whose content has not
arrived.

This is what makes the `runtime-content.md` §7 contract — "a declared-size box
(never hug — lazy content must not reflow the scene)" — hold **by
construction**. Because the measured size is declared rather than derived from
content, a contribution arriving at a different size does not change what was
measured, so nothing reflows. The alternative reading, pinning the node to
`Fixed` and banning `Hug`, enforces the same rule by restriction and leaves a
`Hug` parent unable to size around a placeholder at all.

## Decision 3 — `interim_fill` is an inline `Fill`, not a paint-pool index

The other pooled node fields (`paint_entry`, `text`, `text_style`) are indices.
`interim_fill` is the `Fill` union inline instead, as `Paint.fill` and
`FillLayer.fill` already are.

A pool index would point at a whole `Paint` — stroke, corners, shadows, blurs —
of which only the fill would ever be read here, so the read would be lossy in a
way nothing in the schema declared. An interim appearance is one fill, and the
union maps losslessly onto `dashpaint::FillSpec` in both directions.

## The surface only

`docs/specification/05-qualification.md` puts placeholder **activation** in v1.
This story carries the surface and nothing reads it: `Prop::Placeholder` stages
onto the arena and `Arena::placeholder` reads it back, with no effect on
committed output. That is the posture `Prop::Text` shipped with at v0.5 for the
same reason — what consumes it did not exist yet — and it is why
`Prop::Placeholder` is classed `PropClass::Layout` beside `Text`/`TextStyle`:
`declared_size` is measured-size intent, whatever reads it later.

## Consequences

- `table Node` ends at `placeholder`. An ordinary node writes no table, so a
  document from before this vocabulary encodes byte-identically —
  `a_node_with_no_placeholder_writes_no_placeholder_table` asserts both halves,
  the absence and that a declared placeholder differs.
- The frozen fixture `v0_5_document.dsb` still decodes with all 15 assertions
  passing, which is the append's proof that no existing field id shifted.
  **`Placeholder`'s own field ids are frozen by nothing**: no fixture predates
  them, and one written by today's bindings would guard nothing until a future
  edit. Filed as debt rather than built here.
- A placeholder's two strings intern into `Document.strings` after the node's
  own text, in the same first-use DFS order. The node's text interning is left
  inline deliberately — routing it through the new helper would touch the pool's
  first-use order, which is what R7's byte-identity rests on.
- The Figma producer lowers no placeholder — but **not because the vocabulary is
  missing**. `dashscene/role = placeholder` is a known annotation the importer
  already recognises, whose sample children it trims
  (`docs/decisions/importer-trim-layers.md`). The lowering in
  `crates/dashc/src/figma/mod.rs` drops it and sets `placeholder: None`.
  Connecting the two is story #1264.
- Two load-gate rules came with the surface, because the loader resolves both
  placeholder strings through `Document.strings` and reads `interim_fill` as a
  `Fill` union: `placeholder.string-out-of-range` and
  `placeholder.declared-size-invalid`. Without them a document the gate accepted
  panicked in `dashscene-core` — `flatbuffers::Vector::get` asserts, and so does
  the arena's own finiteness guard. `dashbuf::prefetch::assets_of_root` walks
  the interim fill for the same reason: a placeholder need not carry a paint
  entry, so an asset reachable only through it would never be prefetched or
  verified (R5).
- Story #1127 can now be built — and was, on 2026-08-19. It needed one concept
  this record did not anticipate: `Location::Contribution`, the first `Location`
  variant naming something outside the document, because the undeclared-overload
  warning's subject is a host binding rather than a node.
  [`a-host-binds-a-contribution-by-id.md`](a-host-binds-a-contribution-by-id.md)
  carries what it settled, and `validator-three-gates.md` is amended for the
  gate it added.

## Trace

- Satisfies: story #1126. Refs issue #876 (the prose this corrects), issue #851
  (the design discussion), story #1127 (the consumer).
- Bound by: `docs/specification/01-goals-and-requirements.md` R7;
  `docs/specification/05-qualification.md` (activation stays in v1).
- Related: `docs/decisions/dsb-frozen-fixture-r7-guard.md`,
  `docs/technotes/runtime-content.md` §7,
  `docs/archive/2026-08-18-placeholder-surface.md` (this work's raw spec).
