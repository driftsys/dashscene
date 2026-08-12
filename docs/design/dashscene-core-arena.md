# dashscene-core: arena + staged-mutation API (v0.1, v0.4 variant resolution + incremental commit, v0.5 text intent, clip resolution)

`dashscene-core` is the semantic model: an arena holding a node tree with layout
and paint intent (`docs/design/dashbuf.md`), mutated through the staged producer
API (`open`/`set_prop`/`set_variant`/`commit`,
`docs/decisions/staged-mutation-v01-scope.md`), resolving on commit into the
committed output a painter consumes (boundary B, `docs/design/architecture.md`).
v0.1 scope: fixed-size layout, solid fill, no Taffy, no variants — the walking
skeleton (`docs/roadmap.md`). v0.4 (story #20) added the variant table and
`set_variant`'s commit-time resolution — see "Variant resolution" below. v0.5
(story #26) added text content and style as intent, held on the node but not
resolved into any committed output — see "Text intent" below. Story #97 added
clip and corner intent, and the commit-time resolution of subtree clips into the
clip regions boundary B carries — see "Clip resolution" below. v0.4 (story #164)
made the commit **incremental** — retained interners, carry-forward of unchanged
rects, and a partial-solve contract, so per-frame update cost scales with the
change rather than the scene — see "Incremental commit" below.

Source: `crates/dashscene-core/src/lib.rs`, `src/arena.rs`, `src/committed.rs`.
Acceptance path: `crates/dashscene-core/tests/arena.rs`.

## Intent model

`Arena` holds `Vec<NodeData>` (one entry per node, indexed by arena slot) plus a
`roots: Vec<NodeId>` in creation order. Each `NodeData` carries an optional
name, an optional parent, its children in creation order, an optional fill
color, (story #97) per-corner radii and a "clips its children" flag, (v0.5,
story #26) an optional text string and an optional text style, and a `Layout` —
the authored `x`/`y`/`width`/`height` fixed geometry plus, since v0.2 (story
#8), the flex vocabulary: mode NONE/H/V, gap, padding, alignment, per-axis
hug/fill/fixed sizing, optional min/max, and child margin (story #10). All of it
is a direct mirror of the `dashbuf` schema shapes (`FixedSizeLayout`,
`LayoutContainer`, `LayoutConstraints`, `SolidFill`, `TextStyle`) without
linking the generated code (the crate has no `dashbuf` dependency; see Scope
boundaries below). `Arena::layout(NodeId) -> Layout` exposes the intent by value
(padding as a named `EdgeInsets`, not a positional array), and `Arena::roots()`
/ `Arena::children(NodeId)` expose the tree in creation order — together the
read seam the story #9 Taffy mapping consumes. Since story #9, commit takes its
geometry from a `LayoutSolver` (`docs/decisions/layout-solver-seam.md`):
`commit()` uses core's internal fixed resolution (the mode-`None` passthrough),
and flex-aware producers use `commit_with` with the engine's `TaffySolver`.

`NodeId` is a stable arena slot index (`u32`), returned by `add_node` and never
invalidated — v0.1 has no node removal. It is deliberately distinct from
document DFS order: DFS order (which doubles as the rect-table index, matching
`dashbuf`'s flattened tree, `docs/design/dashbuf.md`) is recomputed at every
commit, not maintained as the arena's storage order. Keeping the arena `Vec`
itself in DFS order was considered and rejected — insertion splicing to keep
siblings contiguous is O(n) per insert and either invalidates previously-issued
ids or forces an id indirection table anyway, for no v0.1 benefit.

## Staged mutation (`Txn`)

`Arena::open(&mut self) -> Txn<'_>` returns a `Txn` holding the arena's mutable
borrow, so exactly one stage can be open at a time and committed output cannot
be read mid-stage — the borrow checker enforces the contract.
`Txn::add_node(parent, name)` and `Txn::set_prop(node, prop)` mutate the intent
model immediately; nothing is visible to `Arena::committed()` until
`Txn::commit(self)` resolves and publishes. Dropping a `Txn` without committing
leaves the staged changes pending — they publish with the next commit. `Prop`
(v0.1 vocabulary): `X`, `Y`, `Width`, `Height`, `Fill(Color)`; story #97 added
`Corners { .. }` and `Clip(bool)` (see "Clip resolution" below); v0.5 (story
#26) added `Text(String)` and `TextStyle(TextStyle)` (see "Text intent" below);
since v0.2 (story #8) the flex vocabulary: `Mode`, `Gap`, `Padding`,
`MainAlign`, `CrossAlign`, `SizingH`, `SizingV`, `MinWidth`, `MaxWidth`,
`MinHeight`, `MaxHeight` (`docs/decisions/flex-vocabulary-shape.md`), and
`Margin` (story #10); v0.4 (story #165) added `Visible(bool)` (see "Visibility"
below). `Txn::lower_negative_gaps` is a shared producer pass that rewrites a
negative container `gap` into child margins — the Figma≠CSS lowering
(`docs/decisions/negative-gap-lowering.md`). Since v0.8 (story #43) it refuses a
`Wrap` container with a negative gap by named panic — a margin is only
gap-equivalent for a child that follows another child on the same line, and wrap
breaks its lines after the lowering — and skips `Grid` containers, whose gaps
are track spacing, not flex-flow spacing
(`docs/decisions/v08-layout-vocabulary-shape.md` D5). Node names are set at
`add_node`, not a mutable prop. Adding a `String`-carrying variant means `Prop`
can no longer derive `Copy` — it derives `Clone, Debug,
PartialEq` as of v0.5;
nothing in `dashscene-core` or its `dashlang` consumer depended on `Prop: Copy`.

Contract misuse (an out-of-range `NodeId`) panics with a message naming the id
and the arena. A `NodeId` from _another_ arena whose index happens to be in
range is not detected — ids carry no arena identity. These are programmer-error
panics, not part of the P4 named-diagnostics vocabulary. `Prop::Fill` can set a
fill but never clear one back to unfilled — a deliberate v0.1 gap
(`docs/decisions/staged-mutation-v01-scope.md`). `Prop::Clip(bool)` does clear,
because a bool has no absent state to lose.

Full rationale and the rejected alternative (op-log with rollback-on-drop):
`docs/decisions/staged-mutation-v01-scope.md`.

## Variant resolution (v0.4, story #20)

`Txn::add_variant_set(members: Vec<VariantMember>) -> VariantSetId` declares a
fixed, ordered list of members — Figma's "component SET"
(`docs/technotes/glossary.md`) — each an optional name plus sparse
`overrides: Vec<(NodeId, VariantValue)>` against the arena's base node values;
the first member (index 0) is active until `Txn::set_variant(set, member)`
switches it. `VariantValue` is the slice of `Prop`'s vocabulary the dashbuf
variant table carries — `X`, `Y`, `Width`, `Height`, `Fill(Color)`, and
`Visible(bool)` (story #283) — not the full vocabulary;
`docs/decisions/variant-set-flat-index.md` records why selection is a flat
member index (not axis-keyed) and why the overridable-prop vocabulary is this
slice rather than all of `Prop`.

`set_variant` is staged like `set_prop` (P3): it writes the active member index
immediately, and `Arena::active_variant(set)` reads it back staged, the same
immediate-visibility contract `Arena::text` carries. `Arena::layout(node)` — the
read seam every `LayoutSolver` resolves geometry through, the internal
`FixedSolver` included — applies the active member's
`X`/`Y`/`Width`/`Height`/`Visible` overrides on top of the node's base layout
before returning it, so a variant switch reaches committed geometry — and a
variant-hidden child's Taffy `Display::None` (story #283) — through the
_existing_ solver seam, with neither `FixedSolver` nor `dashscene-engine`'s
`TaffySolver` needing to know variants exist. Commit's paint-interning step
applies a `Fill` override the same way, on top of the node's base fill. When two
variant sets both override the same node's same prop, the later-created set wins
(creation order, not commit order).

Because both application points feed the commit resolution pipeline's existing
rect/paint construction, the dirty-set diff (below) needs no variant-specific
logic: a variant-driven geometry or fill change is indistinguishable, at the
diff step, from the same change made through `set_prop`.

`Arena::layout(node)` reaches through `arena.layout(id)` in `FixedSolver::solve`
(in place of the earlier raw field read), which is the one change to a v0.1 code
path this story makes — everywhere else, variant resolution is additive.

## Text intent (v0.5, story #26)

`Prop::Text(String)` and `Prop::TextStyle(TextStyle)` set/replace a node's text
content and style;
`TextStyle { family: String, size:
f32, weight: u16, color: Color }` mirrors the
`dashbuf` `TextStyle` table field-for-field.
`Arena::text(NodeId) -> Option<&str>` and
`Arena::text_style(NodeId) -> Option<&TextStyle>` read them back — `None` for a
node without text or without a style.

Both accessors read the intent model directly, not `committed()`: a staged
(uncommitted) value is visible immediately, the same panic contract as
`Arena::name` for an out-of-range `NodeId`. This is deliberate — it is the seam
that story #28's standalone typeset pipeline and story #29's measure callback
are documented to read from, ahead of either one existing.

The commit pipeline and the committed output are unchanged by this story: text
does not influence the v0.5 rect table (text-driven hug sizing arrives with
#29), and the committed output carries no glyph data (P1 — boundary B gains
positioned glyph runs at #28/#30). A text-only change therefore produces no
dirty entry — there is nothing in `CommittedScene` for a text edit to change.

**Superseded by story #542.** Both halves of that last paragraph are now false,
and deliberately so. `CommittedScene` carries a `GlyphRunTable` (`glyphs()`),
produced at commit through two defaulted `LayoutSolver` methods — `atlases` and
`stage_text` — and each run is stamped with the rect index of the text node it
was shaped from. P1 still holds: a run is committed _output_, the same category
as a `RectEntry`, and appears nowhere in the `.dsb`. A text-only change now does
produce a dirty entry: commit compares this commit's staged runs against the
previous commit's per anchor and dirties any anchor whose runs differ, because a
string that changes inside an unchanged box leaves the rect entry bits identical
and would otherwise leave a retained painter drawing stale glyphs. The seam, the
anchor field, and the measured consequences are
`docs/decisions/glyph-runs-cross-boundary-b.md`.

Interning styles into a committed style table at commit time (the paint-pool
precedent — see "Commit resolution pipeline" below) was considered and deferred,
not rejected outright: no consumer exists until #28/#29 define what actually
needs to cross boundary B, so building the table now would be speculative.
Storing text in the committed output now was rejected for the same reason. The
seam is documented here so that whichever of #28/#29 lands first has a stated
contract to build against, and paint's committed-table shape is the precedent to
follow when a committed text/style table is finally warranted.

## Clip resolution (story #97)

`Prop::Clip(bool)` marks a node as clipping its children to its own (rounded)
box (`Paint.clip`, `docs/design/architecture.md`); `Prop::Corners { .. }` sets
the per-corner radii that round both the node's own fill/stroke and that clip
box. Both are intent; commit resolves the clip into the per-rect regions
boundary B carries, because a flat rect table gives a painter no ancestors to
walk and P2 forbids it re-deriving them. The contract and the rejected
alternatives are `docs/decisions/resolved-clip-regions-at-commit.md`; the shape
is `dashpaint`'s `ClipTable` / `ClipRegion` / `ClipBox`
(`docs/design/dashpaint.md`).

Resolution rides the DFS walk commit already does (parent before child): a
node's region is its parent's region, plus the parent's own box when the parent
clips. A clipping node therefore does not clip itself — only its descendants.
Regions intern on `(parent's region index, parent's clip-box bits)`: equal
ancestor chains take equal keys by induction, so the whole subtree under one
clipping ancestor shares one region entry, at O(1) per node and with no
chain-shaped hash key. A node no ancestor clips references
`ClipIndex::UNCLIPPED`, the region `ClipTable::new()` reserves at index 0 —
every rect resolves, as with paints.

## Visibility (story #165)

`Prop::Visible(bool)` (stored on `Layout`, default `true`) is layout- affecting
vocabulary the flex-aware solver seam consumes, like `Mode` or `Gap`:
`dashscene-engine`'s `TaffySolver` lowers `false` to Taffy `Display::None`
(`docs/design/dashscene-engine.md`), which hides the node and every descendant
from the flex flow so siblings reflow into its space. `commit()`'s internal
`FixedSolver` ignores it — the same gap it already leaves for the rest of the
flex vocabulary (`docs/decisions/layout-solver-seam.md`).

A hidden node still resolves to a rect every commit (P4 — a solver never omits a
node) and keeps its rect-table index: the DFS walk and index assignment in
`Txn::commit_with` run unconditionally over every node regardless of visibility,
so nodes committed after a hidden one never shift. This is the invariant the
bounded-pool work depends on (issue #166).

## Masks and group opacity (story #44)

`Prop::Mask(bool)` (a `mask` node flag) marks a node as a mask: at commit it
adds its resolved (rounded) box to the clip region of every sibling that follows
it in the same parent, and to those siblings' subtrees, reusing the clip-region
machinery above. A new mask sibling replaces the active one; the mask node
itself resolves to the draws-nothing paint entry (a stencil, not paint). A mask
toggle marks the node paint-dirty and feeds the same region cascade a clip
toggle does.

`Prop::Opacity(f32)` (a `opacity` node value in `[0, 1]`, default `1.0`) is
paint-only — it never reaches the solver
(`docs/decisions/visible-is-layout-opacity-is-paint.md`). Commit resolves it in
two passes after the main walk: a post-order pass finds each subtree's
rect-index extent, then a pre-order pass carries a free-alpha product down the
tree. A node with opacity below 1 whose painted subtree is non-overlapping folds
its alpha into every subtree rect's `RectEntry.opacity` (the free path); an
overlapping one emits a `GroupComposite` whose layer composites at the node's
alpha times the carried product, and its subtree resets to a product of 1. The
overlap test is pairwise over the painting rects in the subtree. Opacity records
no change log — the walk reads it fresh every commit, and the rect entry's alpha
bits carry a change into the dirty set. Full model and alternatives:
`docs/decisions/masks-and-group-opacity.md`.

## Shadows (story #45)

`Prop::Shadows(Vec<Shadow>)` sets a node's drop and inner shadows (kind, offset,
blur, spread, color). Unlike a mask or a group opacity, a shadow has no
cross-node relation — it depends only on the node's own box and corners — so it
is plain paint intent, the corners case exactly: stored on the node, classified
paint-affecting, copied onto the committed `PaintEntry.shadows` and folded into
the paint-intern key, so two nodes sharing a style and shadows share one pool
entry and a shadow change re-interns. A mask or hidden node resolves to the
draws-nothing entry, which carries no shadows.
`docs/decisions/effects-vocabulary-shadows.md`.

## Binding tables (v0.7, story #167)

The document binding tables live on the arena as intent metadata:
`Txn::declare_signal(name, initial) -> SignalId` and
`Txn::bind(node,
channel, signal, transform)` stage them, `Arena::signals()` /
`Arena::bindings()` read them back, and commit ignores them — flushing a
signal's value through a binding is a producer-side runtime's job (`dashlang`'s
reactive layer; P3), and signal values never enter the arena (P1). The
vocabulary (`Channel` — the §23 set, whose discriminants are the wire codes
`dashbuf` and the engine's `PropKey` packing share — and the declarative
`ScalarTransform` with one shared `apply`) lives in
`crates/dashscene-core/src/bindings.rs`. `load_document` replays both tables
through the same producer API, indices resolved through the load's own mappings.
The intent accessors `Arena::parent` and `Arena::fill` are the read seam the
loader-side attach (`dashlang::attach_live`) derives tree structure and fill
seeds from. See `docs/decisions/binding-table-in-the-document.md`.

## Commit resolution pipeline

`Txn::commit` (in `arena.rs`) runs in one pass, and since story #164 it scales
with the change rather than the scene ("Incremental commit" below):

1. **DFS order** — an explicit stack seeded with `roots` (reversed, so pop order
   is creation order), pushing each node's children reversed for the same
   reason. Walk order is roots in creation order, then children in creation
   order under each parent, depth-first. This order is the committed rect
   table's index.
2. **Resolve + intern** — geometry comes from the `LayoutSolver` (story #9,
   `docs/decisions/layout-solver-seam.md`): `commit()` delegates to the internal
   `FixedSolver` (absolute position = parent's absolute + the node's authored
   offset, size = authored width/height), while `commit_with` takes any solver —
   the engine's `TaffySolver` for flex scenes. Since #164 a solver may return
   **only the rects that changed**; a node it omits keeps the rect the previous
   commit gave it (carry-forward). The "every node has a rect" invariant is
   re-expressed, not dropped: a node that is neither solved now nor present in
   the previous commit panics (P4 — never a silent skip). Paint = the fill color
   interned by exact bit pattern (`f32::to_bits` on each channel) — an unfilled
   node interns the shared draws-nothing entry (`PaintEntry::default()`), so
   every rect resolves (story #4, `docs/decisions/boundary-b-unification.md`).
   The paint interner (`paint_map`) and the clip interner (`clip_map`) — both
   `rustc_hash::FxHashMap` — are **retained on the `Arena` across commits**
   since #164, so an index means the same entry from one commit to the next.
3. **Dirty diff** — the published `dirty()` array is a per-index compare of the
   new rect table against the previous front buffer at the same index, dirty
   when the entry's bits differ (`entry_bits` =
   `[x, y, w, h, paint index, clip index]` as raw `u32`, so `f32::to_bits` makes
   NaN behave). Since #164 this is a plain bit compare with no resolved-color or
   resolved-region clause: because the interners are retained, a changed fill
   earns a _new_ paint index and a resized or toggled clip ancestor earns a
   _new_ clip region index, so the change is already in the entry bits (the old
   `resolved_paint_key` / `same_region_bits` helpers, needed only while the
   tables were re-interned every commit, are gone). This is still a per-index
   diff, not an op-touched set — a node whose _ancestor_ moved gets its own
   absolute recomputed and shows up as changed even though no `set_prop` touched
   it. _Which_ nodes are re-solved and re-interned, by contrast, is derived from
   what was written: `set_prop`/`set_variant` classify each write through
   `prop_class` into retained `layout_dirty` / `paint_dirty` / `clip_toggled`
   sets that drive the incremental work.
4. **Publish** — generation = previous generation + 1 (every commit increments,
   including a no-op commit); the new `CommittedScene` is written into the back
   buffer and `front` flips.

### Incremental commit (story #164)

Before #164 the commit cost `O(total nodes)` every frame — a full solve, two
fresh interner `HashMap`s, and a rect table rebuilt from scratch — regardless of
how few props changed. #164 makes each of those scale with the change, so the
per-frame update path (`dashlang`'s reactive layer, FLIP) is affordable at ~1000
live nodes:

- **Retained interners.** `paint_map` and `clip_map` persist on the `Arena`
  (taken with `mem::take` for the commit and put back by a scope guard, so a
  commit that panics part-way through leaves them describing the front buffer's
  tables rather than empty — issue #196), so paint and clip indices are stable
  across commits. This is what collapses the dirty check to a bit compare (step
  3).
- **Bounded pools.** Retaining an index means a changed entry earns a new one
  and leaves its old entry behind, which grew the paint and clip tables by one
  entry per commit under an animated fill or a resizing clip, without bound
  (issue #197). A commit whose table has grown past both a small floor and twice
  the rect count rebuilds that table from the entries its rects reference,
  renumbering them and re-keying the interner. Each rebuild at least halves the
  table, so its `O(scene)` cost amortizes to a constant per commit; the commit
  that rebuilds reports every renumbered rect dirty, which is what a painter
  needs to re-upload them.
- **Copy-on-write tables.** The back buffer's paint and clip tables, and the
  `node_of`/`rect_index` maps, start as `Arc` clones of the previous commit's
  and are `make_mut`-copied only when a genuinely new entry is pushed or the
  tree structure changed. Unchanged rect entries and the geometry of
  solver-omitted nodes are carried forward by `NodeId`.
- **Partial solve.** The `LayoutSolver::solve` contract now blesses returning
  only the movers (`docs/decisions/layout-solver-seam.md`); the engine's
  retained `TaffySolver` does exactly that (`docs/design/dashscene-engine.md`).

`rustc-hash` (`FxHashMap`) is the one crate #164 adds: an internal interner
keyed by color/geometry bits needs neither SipHash's DoS resistance nor its
cost. Acceptance is in `crates/dashscene-core/tests/arena.rs` (first-commit
dirties all; move-parent dirties it and its descendants only; no-op commit is
clean; a fill change earns a new stable paint index; swapped fills dirty both;
carry-forward of an omitted node's rect; the panic when a node has no rect this
solve or last) and `crates/dashscene-engine/tests/incremental.rs`.

Fixed-position authoring (why `x`/`y` exist as an offset on the document's
`FixedSizeLayout` rather than only in the arena) and the committed-output shapes
this pipeline produces are pinned decisions, not re-derived here:
`docs/decisions/fixed-position-authoring.md`,
`docs/decisions/core-committed-output-shape.md`.

## Committed output (boundary B)

`CommittedScene` (in `committed.rs`) is the double-buffered painter input, built
from `dashpaint`'s types since the story #4 unification:
`rects() -> &[RectEntry]` (DFS-indexed, blittable
`{ x, y, w, h: f32, paint: PaintIndex, clip: ClipIndex, opacity: f32 }`),
`paints() -> &PaintTable` (deduplicated `PaintEntry` pool),
`clips() -> &ClipTable` (deduplicated `ClipRegion` pool, story #97),
`groups() -> &[GroupComposite]` (the story #44 render-target group opacities),
`generation() -> u64`, `dirty() -> &[u32]`, plus the NodeId↔rect-index
correspondence for the commit that produced it (`node_of(rect_index) -> NodeId`,
`rect_index_of(NodeId) -> Option<u32>` — `None` for a node added after that
commit). `Arena` holds two `CommittedScene` buffers and a `front` index;
`Arena::committed()` borrows the front buffer only.

The exact pinned shapes (byte sizes, paint dedup/ordering rule, dirty-set
definition) and the original reasoning for core's own mirror types (superseded
at story #4: core now depends on `dashpaint`, see
`docs/decisions/boundary-b-unification.md`) rather than reusing `dashbuf`'s
generated structs: `docs/decisions/core-committed-output-shape.md`.

## Schema change (`dashbuf`)

`FixedSizeLayout` in `crates/dashbuf/schema/dashbuf.fbs` gained authored `x`/`y`
fields alongside the existing `width`/`height`, covered by
`crates/dashbuf/tests/roundtrip.rs`. This is the document side of the same
decision as the arena's offset resolution:
`docs/decisions/fixed-position-authoring.md`.

v0.5 (story #26) added the schema's text vocabulary — `TextStyle` table,
`Document.strings`/`Document.text_styles` pools, and
`Node.text`/`Node.text_style` sentinel-indexed fields — that this crate's
`TextStyle` struct and `Prop::Text`/`Prop::TextStyle` mirror. Full schema
rationale: `docs/design/dashbuf.md`.

## Scope boundaries (v0.1)

- Depends on `dashpaint` for the boundary-B types since story #4's unification
  (`docs/decisions/boundary-b-unification.md`): the mirror types of the
  parallel-development phase are gone, and unfilled nodes resolve to the shared
  draws-nothing entry instead of a sentinel.
- No `dashbuf` dependency — the arena mirrors the schema's field shapes; nothing
  links the generated flatbuffer code.
- `set_variant` landed at v0.4 (story #20, "Variant resolution" above) —
  `docs/decisions/staged-mutation-v01-scope.md` scoped v0.1 to
  `open`/`set_prop`/`commit` only, deferring it until the variant table existed.
- No node removal, no reparenting, no fill clearing, no value validation (NaN,
  negative sizes). The validator crate enters at its own slice.
- Paint intent is solid fill + corner radii only: strokes, gradients and image
  fills exist at boundary B but no producer stages them yet (the goldens
  hand-build them). Clip boxes are axis-aligned — clip-on-rotated is a v0.8
  construct (`docs/roadmap.md`).

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
