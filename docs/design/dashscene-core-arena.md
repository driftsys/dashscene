# dashscene-core: arena + staged-mutation API (v0.1, v0.4 variant resolution, v0.5 text intent, clip resolution)

`dashscene-core` is the semantic model: an arena holding a node tree
with layout and paint intent (`docs/design/dashbuf.md`), mutated
through the staged producer API (`open`/`set_prop`/`set_variant`/`commit`,
`docs/decisions/staged-mutation-v01-scope.md`), resolving on commit
into the committed output a painter consumes (boundary B,
`docs/design/architecture.md`). v0.1 scope: fixed-size layout, solid
fill, no Taffy, no variants — the walking skeleton
(`docs/roadmap.md`). v0.4 (story #20) added the variant table and
`set_variant`'s commit-time resolution — see "Variant resolution"
below. v0.5 (story #26) added text content and style as
intent, held on the node but not resolved into any committed output —
see "Text intent" below. Story #97 added clip and corner intent, and the
commit-time resolution of subtree clips into the clip regions boundary B
carries — see "Clip resolution" below.

Source: `crates/dashscene-core/src/lib.rs`, `src/arena.rs`,
`src/committed.rs`. Acceptance path: `crates/dashscene-core/tests/arena.rs`.

## Intent model

`Arena` holds `Vec<NodeData>` (one entry per node, indexed by arena
slot) plus a `roots: Vec<NodeId>` in creation order. Each `NodeData`
carries an optional name, an optional parent, its children in creation
order, an optional fill color, (story #97) per-corner radii and a
"clips its children" flag, (v0.5, story #26) an optional text
string and an optional text style, and a `Layout` — the authored
`x`/`y`/`width`/`height` fixed geometry plus, since v0.2 (story #8),
the flex vocabulary: mode NONE/H/V, gap, padding, alignment, per-axis
hug/fill/fixed sizing, optional min/max, and child margin (story #10).
All of it is a direct mirror
of the `dashbuf` schema shapes (`FixedSizeLayout`, `LayoutContainer`,
`LayoutConstraints`, `SolidFill`, `TextStyle`) without linking the
generated code
(the crate has no `dashbuf` dependency; see Scope boundaries below).
`Arena::layout(NodeId) -> Layout` exposes the intent by value
(padding as a named `EdgeInsets`, not a positional array), and
`Arena::roots()` / `Arena::children(NodeId)` expose the tree in
creation order — together the read seam the story #9 Taffy mapping
consumes. Since story #9, commit takes its
geometry from a `LayoutSolver` (`docs/decisions/layout-solver-seam.md`):
`commit()` uses core's internal fixed resolution (the mode-`None`
passthrough), and flex-aware producers use `commit_with` with the
engine's `TaffySolver`.

`NodeId` is a stable arena slot index (`u32`), returned by `add_node`
and never invalidated — v0.1 has no node removal. It is deliberately
distinct from document DFS order: DFS order (which doubles as the
rect-table index, matching `dashbuf`'s flattened tree,
`docs/design/dashbuf.md`)
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
(v0.1 vocabulary): `X`, `Y`, `Width`, `Height`, `Fill(Color)`; story #97
added `Corners { .. }` and `Clip(bool)` (see "Clip resolution" below);
v0.5
(story #26) added `Text(String)` and `TextStyle(TextStyle)` (see "Text
intent" below); since v0.2 (story #8) the flex vocabulary: `Mode`,
`Gap`, `Padding`, `MainAlign`, `CrossAlign`, `SizingH`, `SizingV`,
`MinWidth`, `MaxWidth`, `MinHeight`, `MaxHeight`
(`docs/decisions/flex-vocabulary-shape.md`), and `Margin` (story #10);
v0.4 (story #165) added `Visible(bool)` (see "Visibility" below).
`Txn::lower_negative_gaps` is a shared producer pass that rewrites a
negative container `gap` into child margins — the Figma≠CSS lowering
(`docs/decisions/negative-gap-lowering.md`). Node names are set at
`add_node`, not a mutable prop. Adding a `String`-carrying variant
means `Prop` can no longer derive `Copy` — it derives `Clone, Debug,
PartialEq` as of v0.5; nothing in `dashscene-core` or its `dashlang`
consumer depended on `Prop: Copy`.

Contract misuse (an out-of-range `NodeId`) panics with a message
naming the id and the arena. A `NodeId` from _another_ arena whose
index happens to be in range is not detected — ids carry no arena
identity. These are programmer-error panics, not part of the P4
named-diagnostics vocabulary. `Prop::Fill` can set a fill but never
clear one back to unfilled — a deliberate v0.1 gap
(`docs/decisions/staged-mutation-v01-scope.md`). `Prop::Clip(bool)` does
clear, because a bool has no absent state to lose.

Full rationale and the rejected alternative (op-log with
rollback-on-drop): `docs/decisions/staged-mutation-v01-scope.md`.

## Variant resolution (v0.4, story #20)

`Txn::add_variant_set(members: Vec<VariantMember>) -> VariantSetId`
declares a fixed, ordered list of members — Figma's "component SET"
(`docs/technotes/glossary.md`) — each an optional name plus sparse
`overrides: Vec<(NodeId, VariantValue)>` against the arena's base node
values; the first member (index 0) is active until
`Txn::set_variant(set, member)` switches it. `VariantValue` is the
narrow slice of `Prop`'s vocabulary the dashbuf variant table carries —
`X`, `Y`, `Width`, `Height`, `Fill(Color)` — not the full vocabulary;
`docs/decisions/variant-set-flat-index.md` records why selection is a
flat member index (not axis-keyed) and why the overridable-prop
vocabulary is this narrow slice rather than all of `Prop`.

`set_variant` is staged like `set_prop` (P3): it writes the active
member index immediately, and `Arena::active_variant(set)` reads it
back staged, the same immediate-visibility contract `Arena::text`
carries. `Arena::layout(node)` — the read seam every `LayoutSolver`
resolves geometry through, the internal `FixedSolver` included —
applies the active member's `X`/`Y`/`Width`/`Height` overrides on top
of the node's base layout before returning it, so a variant switch
reaches committed geometry through the _existing_ solver seam, with
neither `FixedSolver` nor `dashscene-engine`'s `TaffySolver` needing to
know variants exist. Commit's paint-interning step applies a `Fill`
override the same way, on top of the node's base fill. When two
variant sets both override the same node's same prop, the later-created
set wins (creation order, not commit order).

Because both application points feed the commit resolution pipeline's
existing rect/paint construction, the dirty-set diff (below) needs no
variant-specific logic: a variant-driven geometry or fill change is
indistinguishable, at the diff step, from the same change made through
`set_prop`.

`Arena::layout(node)` reaches through `arena.layout(id)` in
`FixedSolver::solve` (in place of the earlier raw field read), which is
the one change to a v0.1 code path this story makes — everywhere else,
variant resolution is additive.

## Text intent (v0.5, story #26)

`Prop::Text(String)` and `Prop::TextStyle(TextStyle)` set/replace a
node's text content and style; `TextStyle { family: String, size:
f32, weight: u16, color: Color }` mirrors the `dashbuf` `TextStyle`
table field-for-field. `Arena::text(NodeId) -> Option<&str>` and
`Arena::text_style(NodeId) -> Option<&TextStyle>` read them back —
`None` for a node without text or without a style.

Both accessors read the intent model directly, not `committed()`: a
staged (uncommitted) value is visible immediately, the same panic
contract as `Arena::name` for an out-of-range `NodeId`. This is
deliberate — it is the seam that story #28's standalone typeset
pipeline and story #29's measure callback are documented to read
from, ahead of either one existing.

The commit pipeline and the committed output are unchanged by this
story: text does not influence the v0.5 rect table (text-driven hug
sizing arrives with #29), and the committed output carries no glyph
data (P1 — boundary B gains positioned glyph runs at #28/#30). A
text-only change therefore produces no dirty entry — there is nothing
in `CommittedScene` for a text edit to change.

Interning styles into a committed style table at commit time (the
paint-pool precedent — see "Commit resolution pipeline" below) was
considered and deferred, not rejected outright: no consumer exists
until #28/#29 define what actually needs to cross boundary B, so
building the table now would be speculative. Storing text in the
committed output now was rejected for the same reason. The seam is
documented here so that whichever of #28/#29 lands first has a stated
contract to build against, and paint's committed-table shape is the
precedent to follow when a committed text/style table is finally
warranted.

## Clip resolution (story #97)

`Prop::Clip(bool)` marks a node as clipping its children to its own
(rounded) box (`Paint.clip`, `docs/design/architecture.md`); `Prop::Corners { .. }`
sets the per-corner radii that round both the node's own fill/stroke and
that clip box. Both are intent; commit resolves the clip into the
per-rect regions boundary B carries, because a flat rect table gives a
painter no ancestors to walk and P2 forbids it re-deriving them. The
contract and the rejected alternatives are
`docs/decisions/resolved-clip-regions-at-commit.md`; the shape is
`dashpaint`'s `ClipTable` / `ClipRegion` / `ClipBox`
(`docs/design/dashpaint.md`).

Resolution rides the DFS walk commit already does (parent before child):
a node's region is its parent's region, plus the parent's own box when
the parent clips. A clipping node therefore does not clip itself — only
its descendants. Regions intern on
`(parent's region index, parent's clip-box bits)`: equal ancestor chains
take equal keys by induction, so the whole subtree under one clipping
ancestor shares one region entry, at O(1) per node and with no
chain-shaped hash key. A node no ancestor clips references
`ClipIndex::UNCLIPPED`, the region `ClipTable::new()` reserves at index
0 — every rect resolves, as with paints.

## Visibility (story #165)

`Prop::Visible(bool)` (stored on `Layout`, default `true`) is layout-
affecting vocabulary the flex-aware solver seam consumes, like `Mode`
or `Gap`: `dashscene-engine`'s `TaffySolver` lowers `false` to Taffy
`Display::None` (`docs/design/dashscene-engine.md`), which hides the
node and every descendant from the flex flow so siblings reflow into
its space. `commit()`'s internal `FixedSolver` ignores it — the same
gap it already leaves for the rest of the flex vocabulary
(`docs/decisions/layout-solver-seam.md`).

A hidden node still resolves to a rect every commit (P4 — a solver
never omits a node) and keeps its rect-table index: the DFS walk and
index assignment in `Txn::commit_with` run unconditionally over every
node regardless of visibility, so nodes committed after a hidden one
never shift. This is the invariant the bounded-pool work depends on
(issue #166).

## Commit resolution pipeline

`Txn::commit` (in `arena.rs`) runs in one pass:

1. **DFS order** — an explicit stack seeded with `roots` (reversed, so
   pop order is creation order), pushing each node's children reversed
   for the same reason. Walk order is roots in creation order, then
   children in creation order under each parent, depth-first. This
   order is the committed rect table's index.
2. **Resolve + intern** — geometry comes from the `LayoutSolver`
   (story #9, `docs/decisions/layout-solver-seam.md`): `commit()`
   delegates to the internal `FixedSolver` (absolute position =
   parent's absolute + the node's authored offset, size = authored
   width/height), while `commit_with` takes any solver — the engine's
   `TaffySolver` for flex scenes. A solver that omits a node panics
   (P4). Paint = the fill color interned by exact bit pattern
   (`f32::to_bits` on each channel) in first-use order — an unfilled
   node interns the shared draws-nothing entry
   (`PaintEntry::default()`), so every rect resolves (story #4,
   `docs/decisions/boundary-b-unification.md`).
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
input, built from `dashpaint`'s types since the story #4 unification:
`rects() -> &[RectEntry]` (DFS-indexed, blittable
`{ x, y, w, h: f32, paint: PaintIndex, clip: ClipIndex }`),
`paints() -> &PaintTable` (deduplicated `PaintEntry` pool),
`clips() -> &ClipTable` (deduplicated `ClipRegion` pool, story #97),
`generation() -> u64`, `dirty() -> &[u32]`, plus the
NodeId↔rect-index correspondence for the commit that produced it
(`node_of(rect_index) -> NodeId`,
`rect_index_of(NodeId) -> Option<u32>` — `None` for a node added after
that commit). `Arena` holds two `CommittedScene` buffers and a `front`
index; `Arena::committed()` borrows the front buffer only.

The exact pinned shapes (byte sizes, paint dedup/ordering rule,
dirty-set definition) and the original reasoning for core's own mirror
types (superseded at story #4: core now depends on `dashpaint`, see
`docs/decisions/boundary-b-unification.md`) rather than reusing
`dashbuf`'s generated structs: `docs/decisions/core-committed-output-shape.md`.

## Schema change (`dashbuf`)

`FixedSizeLayout` in `crates/dashbuf/schema/dashbuf.fbs` gained
authored `x`/`y` fields alongside the existing `width`/`height`,
covered by `crates/dashbuf/tests/roundtrip.rs`. This is the document
side of the same decision as the arena's offset resolution:
`docs/decisions/fixed-position-authoring.md`.

v0.5 (story #26) added the schema's text vocabulary — `TextStyle`
table, `Document.strings`/`Document.text_styles` pools, and
`Node.text`/`Node.text_style` sentinel-indexed fields — that this
crate's `TextStyle` struct and `Prop::Text`/`Prop::TextStyle` mirror.
Full schema rationale: `docs/design/dashbuf.md`.

## Scope boundaries (v0.1)

- Depends on `dashpaint` for the boundary-B types since story #4's
  unification (`docs/decisions/boundary-b-unification.md`): the mirror
  types of the parallel-development phase are gone, and unfilled nodes
  resolve to the shared draws-nothing entry instead of a sentinel.
- No `dashbuf` dependency — the arena mirrors the schema's field
  shapes; nothing links the generated flatbuffer code.
- `set_variant` landed at v0.4 (story #20, "Variant resolution" above)
  — `docs/decisions/staged-mutation-v01-scope.md` scoped v0.1 to
  `open`/`set_prop`/`commit` only, deferring it until the variant table
  existed.
- No node removal, no reparenting, no fill clearing, no value
  validation (NaN, negative sizes). The validator crate enters at its
  own slice.
- Paint intent is solid fill + corner radii only: strokes, gradients and
  image fills exist at boundary B but no producer stages them yet (the
  goldens hand-build them). Clip boxes are axis-aligned — clip-on-rotated
  is a v0.8 construct (`docs/roadmap.md`).

## Module layout

    crates/dashscene-core/src/lib.rs        crate docs + re-exports
    crates/dashscene-core/src/arena.rs      Arena, NodeId, Prop,
                                            TextStyle, VariantValue,
                                            VariantMember, VariantSetId,
                                            Txn, commit resolution
    crates/dashscene-core/src/committed.rs  CommittedScene + re-exported
                                            dashpaint types
    crates/dashscene-core/src/load.rs       load_document: replays a
                                            validated .dsb document
                                            through the producer API,
                                            variant sets included
    crates/dashscene-core/tests/load.rs     load_document's variant-set
                                            replay (issue #20): default
                                            and non-default
                                            active_member at load time,
                                            a document with no variant
                                            sets
    crates/dashscene-core/tests/arena.rs    acceptance path (issue #2):
                                            empty-commit, single-root
                                            resolve, DFS nesting and
                                            interleaved creation order,
                                            paint interning/dedup,
                                            the shared draws-nothing
                                            entry, staged visibility,
                                            generation stamping,
                                            dirty-set diffing,
                                            NodeId↔rect-index
                                            round-trip; clip resolution
                                            (issue #97): corner intent
                                            reaching the paint entry,
                                            the ancestor chain, region
                                            dedup and pass-through, and
                                            the dirty-set clause for a
                                            resized or toggled clip
                                            ancestor; text intent
                                            (issue #26): set/read
                                            through the accessors,
                                            staged visibility, replace
                                            semantics, no-text default,
                                            no dirty entry from a
                                            text-only change; plus
                                            (story #8) flex-prop
                                            set/read-back, layout
                                            defaults, and flex props
                                            leaving committed output
                                            unchanged; plus (issue #165)
                                            a hidden node keeping its
                                            rect-table index with no
                                            shift to nodes committed
                                            after it; variant
                                            resolution (issue #20): a
                                            switch changing the
                                            resolved rect and paint,
                                            dirtying only the
                                            overridden rect and its
                                            descendants, staged
                                            visibility before commit,
                                            creation-order precedence
                                            across sets, the default
                                            active member, and the
                                            add_variant_set/set_variant
                                            panic contracts
