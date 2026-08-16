# dashscene-engine — the Taffy layout solve

    crate    crates/dashscene-engine
    covers   v0.2 flex core (story #9), the v0.5 measure callback
             (story #29 — text drives hug sizing), the v0.4 retained
             Taffy tree + pruned readback (story #164), the v0.4 Visible
             lowering (story #165), the v0.4 variant-switch FLIP
             (story #22), and the v0.8 layout fidelity — wrap, grid
             with spans, baseline, and the negative-margin hug rebate
             (story #43), the v0.11 weight-aware measure (story
             F1/#368 — the measure context carries the CSS weight), and
             the v0.13 correctness burn-down (#270 the hug-child
             negative margin, #322 the hug baseline row's cross size,
             #200 the taffy_of fill tripwire, #487 the FLIP animated-set
             contract)

## Purpose

`dashscene-engine` is the runtime that resolves the model
(`docs/archive/2026-07-14-design-1-seed.md` §7.1): the one Taffy solve every
backend shares (P2). It implements `dashscene-core`'s `LayoutSolver` seam
(`docs/decisions/layout-solver-seam.md`); producers commit flex scenes through
`txn.commit_with(&mut TaffySolver::new())`.

Source: `crates/dashscene-engine/src/lib.rs`. Acceptance path:
`crates/dashscene-engine/tests/solve.rs` (hand-computed rects).

`crates/dashscene-engine/tests/taffy_upstream.rs` is not an acceptance test: it
reproduces the taffy 0.12 defect the negative-margin workarounds exist for (debt
#269), in plain taffy with no dashscene types, and asserts taffy's current wrong
answers. A taffy upgrade that fixes the defect turns those assertions red, which
is the signal to retire both workarounds. The report text is
`docs/technotes/taffy-scaled-shrink.md`.

## The solve

One Taffy tree per arena root — roots are independent coordinate islands,
translated by their authored offsets at readback. The tree builds from the
arena's read seam (`roots`/`children`/`layout`), solves with max-content
available space, and reads back absolute rects by accumulating parent origins
(Taffy reports parent-relative locations). f32 passthrough: Taffy's default
whole-pixel rounding is disabled (`disable_rounding`), asserted by test on
fractional geometry (R7; deterministic given the same intent — exact bits follow
Taffy's evaluation order).

### Retained tree + pruned readback (v0.4, story #164)

`LayoutSolver` takes `&mut self` so the solver can retain state across solves;
#164 realizes it. The first solve builds the tree; every later solve reuses it.
`TaffySolver` holds a `TreeState` — the persistent `TaffyTree`, a
`taffy_of: Vec<taffy::NodeId>` map keyed by arena `NodeId` slot (so a `NodeId`
maps to a stable Taffy node across solves), `parent_of`, the roots, the previous
relative layouts and root origins for the readback prune, the #322 cross-size
floors, and two per-node vectors that are stamped rather than cleared:
`on_path`, which marks the ancestors of a dirty node for the pruned readback
(issue #1111), and `baseline_offsets`, which carries the #272 corrections (issue
#1153). Both exist to keep a per-frame allocation off the frame path, and each
pays a document-sized vector for it — `baseline_offsets` only once a solve has a
typesetter, so a solver built with `TaffySolver::new()` never sizes it. A
`solves` counter is exposed (`solves()`) so a test can assert that a paint-only
commit performed no solve.

`taffy_of` is seeded with `NodeId::new(u64::MAX)`, which names no node taffy can
allocate, and a `debug_assert` after the build says every slot was overwritten
(v0.13, debt #200). The seed used to be `NodeId::new(0)` — the id of the first
node built — so a structural bug that left a slot unwritten would read that
node's layout and report it as another node's rect. There is no such bug: every
arena node is reachable from a root. The point is that if one ever appears it
stops rather than answers.

`solve` dispatches on whether the tree is structurally current — a **grown**
arena node count forces a full `rebuild` returning every node; otherwise
`incremental` runs. Incremental invalidates only the nodes
`arena.layout_dirty()` names, through Taffy's `set_style` (which marks a node
and its ancestor chain dirty) plus `set_node_context` for their measure inputs;
a clean subtree returns from Taffy's cache without re-descent. An empty dirty
set is the paint-only fast path — `solve` returns no rects and never calls
Taffy.

The set spans every commit since the last one that **solved**, not since the
last commit (issue #1148). A producer that publishes geometry it resolved itself
— `dashlang`'s contained-write path, replaying its patched cache through
`CachedSolver` — commits without running this solver, and such a commit leaves
the set in place rather than draining it. So the nodes it dirtied are still here
to be restyled at the next real solve, which is what keeps this tree describing
the arena across a run of replayed frames.

The cost is that the set this reads can carry entries already styled, and it is
bounded rather than small: `commit_with` dedups a carried set once it passes
twice its own live content, so what arrives here is at most about twice the
distinct nodes dirtied since the last solve — bounded by **what changed**, not
by the document. Restyling one is idempotent and the alternative is a rect
resolved from a stale style, so the entries are paid for; the bound is what
stops the count above from being read as unbounded.

**Pruned readback** (`read_back_pruned`) is the only genuinely new layout logic.
Taffy stores layouts relative to the parent; converting to absolute naively is
an `O(n)` walk that would consume the win. Instead a node is emitted only when
its relative layout changed **or** its parent's absolute origin moved, and the
walk descends into a subtree only when the node moved or it lies on the path to
a dirty descendant (`on_path` = the dirty set plus its ancestors). A subtree
that neither moved nor guards a dirty descendant is skipped whole, so the
readback — like the solve — scales with the change. This is what lets
`commit_with` accept a partial solve (`docs/decisions/layout-solver-seam.md`).

## Style mapping (Layout → taffy::Style)

Container side (how a node lays out its children):

| intent             | taffy                                    |
| ------------------ | ---------------------------------------- |
| mode `None`        | `display: Block`; children positioned    |
|                    | `Absolute` at their authored offsets —   |
|                    | the passthrough, asserted equal (for     |
|                    | fixed-sized trees) to `commit()`         |
| mode `Horizontal`/ | `display: Flex` + `flex_direction`,      |
| `Vertical`         | `gap`, `padding`, `justify_content` from |
|                    | `MainAxisAlign`, `align_items` from      |
|                    | `CrossAxisAlign` (never `Stretch` at the |
|                    | container level; `Baseline` since v0.8)  |
| mode `Wrap` (v0.8) | `Horizontal`'s mapping plus wrapping     |
|                    | (`flex_wrap: Wrap`) with lines packed at |
|                    | the cross start (`align_content:         |
|                    | FlexStart` — Figma packs lines; taffy's  |
|                    | default behaves as stretch)              |
| mode `Grid` (v0.8) | `display: Grid`; `grid_template_rows`/   |
|                    | `grid_template_columns` from the node's  |
|                    | track lists (`Arena::grid_tracks`):      |
|                    | `Fixed(v)` → `length(v)`, `Fraction(w)`  |
|                    | → `minmax(0, fr(w))` — Figma's           |
|                    | serialized track form. `main_align`/     |
|                    | `cross_align` are not mapped; placement  |
|                    | is by cell                               |

The authored `gap` is the main-axis spacing (horizontal for every mode but
`Vertical`); `cross_gap` (v0.8) is the other axis's — wrap-line and grid-row
spacing — and follows `gap` when unset, which keeps the v0.2 both-axes mapping
for documents that never author it
(`docs/decisions/v08-layout-vocabulary-shape.md` D4).

Child side (axis-relative — the parent's direction decides which authored axis
is main; a `Wrap` parent's main axis is `Horizontal`'s):

| sizing  | main axis                           | cross axis          |
| ------- | ----------------------------------- | ------------------- |
| `Fixed` | `flex_basis: length`, grow/shrink 0 | `size: length`      |
| `Hug`   | `flex_basis: auto`, grow/shrink 0   | `size: auto`        |
| `Fill`  | `flex_basis: 0`, grow/shrink 1      | `align_self:        |
|         |                                     | Stretch`, size auto |

Under a `Grid` parent (v0.8) the child maps per axis, not per main/cross:
`grid_row`/`grid_column` anchors become taffy's 1-based start lines with
`span(grid_*_span)` ends (absent anchor = auto-placement in document order), and
in-cell alignment comes from the sizing intent — `Fill` → stretch over the cell
area, `Fixed`/`Hug` → the node's own size at the cell origin
(`justify_self`/`align_self`; taffy's default would stretch a hug child over its
cell). The conversion saturates: a ushort anchor caps at the solver's `i16` line
range and a zero span floors at 1, so no document value can panic — the honest
diagnosis is the load gate's (`docs/decisions/v08-layout-vocabulary-shape.md`
D6). A fixed child larger than its fraction cell keeps its size and overflows
the cell (a `Fraction` track is `minmax(0, fr)` and never grows for content) —
pinned behavior, named open question in the same record.

`min_width`/`max_width`/`min_height`/`max_height` map to `min_size`/`max_size`
(absent = auto). `margin` maps to `taffy::Style::margin` (a `Rect` of
`LengthPercentageAuto`); negative margins are legal and express overlap — the
target the negative-gap lowering rewrites to
(`docs/decisions/negative-gap-lowering.md`).

### The negative-margin hug rebate (v0.8, debt #236)

Taffy 0.12's intrinsic (hug) pass mis-sums a shrink-0 item whose main-axis
margin sum is negative: it divides the item's contribution diff by
`max(1, shrink × inner_basis)` (= 1) but multiplies it back by
`max(1, shrink) × inner_basis` (the item's basis minus its own padding),
amplifying the negative margin and collapsing the hug sum. For a `Fixed`
main-axis child with a negative main-axis margin sum, `style_for` therefore maps
`flex_basis = size + margin_sum`; when that falls below the child's own
main-axis padding sum — taffy floors every basis there — the basis anchors at
`padding + 1` instead, where the broken branch's two formulas agree exactly and
the reconstruction is `size + margin_sum` for any overlap depth. The main-axis
`min_size` floors at the authored size, clamped by an authored max, maxed with
an authored min, so the definite pass restores the real size — positions and
sizes are unchanged everywhere else.

A `Hug` child has no authored size to rebate into, so it takes the same branch's
other agreement point (v0.13, debt #270): at `flex_shrink = 1` the divisor
`max(1, shrink × inner_basis)` and the multiplier `max(1, shrink) × inner_basis`
are equal for every inner basis of 1 or more, and the item contributes exactly
`basis + margin_sum`. The switch needs a negative margin sum and a **parent that
hugs the same axis**, which keeps it inside the pass it repairs: taffy enters
the broken branch only for an indefinite container main size, and a hugging
container is sized to its own content sum, so the definite pass has no negative
free space for the shrink factor to act on. Under any other parent sizing the
child keeps `flex_shrink = 0`. Full arithmetic, alternatives, and the declared
corner cases: `docs/decisions/negative-margin-hug-rebate.md`.

End-to-end coverage: `goldens/images/v013-hug-negative-margin.png`
(`goldens/tooling/tests/v013_uncovered_shapes.rs`, issue #501) carries both the
negative-margin `Hug` rows and the fixed-parent guard row, so a regression fails
a committed frame rather than only the engine's own unit tests. No Figma fixture
has this shape.

Degenerate constructs, all pinned by test and named here for the validator slice
to diagnose (P4):

- A `Fill` child under a mode-`None` parent has no free-space axis and behaves
  as `Hug`.
- A `Fill` root has nothing to fill (no viewport concept yet) and collapses to
  content size.
- `Hug` keeps its content-wrapping meaning under a mode-`None` parent too (a hug
  group inside a plain frame is real vocabulary): a childless `Hug` node sizes
  to zero — authored width/height feed `Fixed` sizing only. The
  `commit()`-equivalence guarantee therefore applies to fixed-sized trees; trees
  using `Hug`/`Fill` are solver vocabulary the fixed resolve deliberately
  ignores.

The gap split resolved at v0.8: `cross_gap` is the second authored gap (see the
mapping above), and its absence reproduces the old both-axes-from-`gap` behavior
exactly.

## Visibility (v0.4, issue #165)

`Prop::Visible(false)` overrides both sides of the style mapping above:
`style_for` sets `Display::None` on the node's own style regardless of its
layout mode. Taffy's `Display::None` hides the node from its parent's flow — the
container's flex sizing (Hug, Fill splits) no longer accounts for it, so a
hidden child's share collapses and its siblings reflow — and recursively hides
every descendant during layout regardless of the descendant's own style,
computing a zeroed-out (degenerate) layout for the whole hidden subtree.
`commit()`'s `FixedSolver` (`dashscene-core`) ignores `Visible`, like the rest
of the flex vocabulary; the fixed-commit equivalence guarantee does not extend
to it.

## Measure callback — text drives hug sizing

Text enters the solve through Taffy's per-node measure callback
(`compute_layout_with_measure`), added at v0.5 (story #29). A node that carries
both text content and a text style (`Arena::text`/`Arena::text_style`, story
#26) becomes a Taffy leaf with a `TextContext` — the paragraph text, the render
size (px per em in document units), the shaping axes, and the node's CSS-scale
weight (story #368). Every other node is a context-free leaf whose measure is a
no-op, so a text-free scene solves exactly as before.

The weight is in the measure context because weight is a measure input, not only
a paint one: a heavier face has its own advances, so a bold run measured at
Regular's advances would size a box the text then overflows. `measure_text`
therefore lays out through `Typesetter::layout_weighted`, and the #272
post-solve baseline-correction pass resolves the same face for the same reason —
a bold child's first baseline sits at the bold face's ascent, not Regular's. A
cascade offering only weight 400 resolves every request there, so a box measures
exactly as it did before the field existed
(`docs/decisions/weight-selection-in-the-cascade.md`).

Taffy calls the measure function for each text leaf during the solve.
`measure_text` lays the text out through the typesetter and returns its box. The
wrap width is the width Taffy has already fixed for the node if there is one,
else a definite available width, else probe-dependent (debt #177, fixed at
v0.8): a max-content probe imposes no wrap, so an unconstrained hug node lays
its paragraph on one line and hugs that natural width; a min-content probe
measures at wrap width zero, which the greedy breaker turns into one word per
line — width = the widest word, the box wrappable text can never shrink below,
and the automatic minimum a shrinkable (`Fill`) text node stops at. A hug-sized
text node therefore solves to its shaped width and height; a width-constrained
one keeps its width and grows taller as the text wraps. A known axis is returned
unchanged, so measurement never overrides a dimension Taffy has already fixed.
The measure seam carries no glyph baseline, so under baseline alignment a text
leaf aligns by its box bottom (Q-4, `docs/technotes/open-questions.md`).

### The post-solve baseline pass (v0.8 #272, v0.13 #322)

The corrections live in `TreeState::baseline_offsets`, a dense table stamped per
collection. It has been three shapes: a `vec![None; node_count]` allocated per
solve, a sparse `FxHashMap` (issue #1111, which took the per-frame band's byte
term to 0), and this (issue #1153, which took the readback's hash off the frame
path without putting the allocation back). `docs/technotes/frame-budget.md`
carries what the middle one cost, measured.

Because the measure seam carries no baseline, Taffy aligns a
`CrossAxisAlign::Baseline` row on its children's box bottoms. `#272` corrects
that after the solve: `collect_baseline_offsets` walks the tree, and for every
`Horizontal` `Baseline` row holding at least one text child it re-places each
participating child so its first line's `baseline_y` — the placed baseline the
typesetter reports, half-leading included — meets one line. A non-text child
keeps its box bottom. A `Fill` cross-sized child is mapped
`align_self: STRETCH`, which taffy excludes from baseline alignment, so it is
excluded here too and keeps the place and the size taffy gave it.

The re-placed children can end lower than the row's own cross size, because that
size came from the box bottoms taffy aligned and the text now ends a descender
further down. A row that hugs its cross axis must hold them (v0.13, debt #322).
The pass therefore records the cross extent its own placement needs, injects it
as the row's Taffy `min_size` on the cross axis, and runs the solver a second
time. The re-solve — rather than a patch to the row's rect — is what makes the
row's ancestors, its following siblings and any hugging ancestor re-place around
it, and it keeps Taffy the one solver (P2). A row with an authored cross size is
never floored: an authored size is the author's decision, and a run that
overflows it clips as before.

The floors live on the retained tree and are recomputed every solve, so a row
that stops needing one has it removed rather than carrying a stale height — the
row itself is not restyled when only a text child changed. Exactly one extra
solve is ever run: the floor is the lowest re-placed child bottom, and neither a
child's baseline nor its cross size depends on the row's own cross size, so the
second solve settles on the floor the first one computed (a `debug_assert` pins
that).

The nested case stays open: a container inside a baseline text row is taken by
its box bottom, because Taffy's `Layout` does not expose the computed baseline
of a subtree.

End-to-end coverage: `goldens/images/v013-baseline-hug-cross.png`
(`goldens/tooling/tests/v013_uncovered_shapes.rs`, issue #501) is a HUG
cross-axis `Baseline` row holding a tall box, a text run and a `Fill`
cross-sized child, with a following sibling under it — so the floor, the `Fill`
exclusion, the re-solve and the sibling's re-placement all reach a committed
frame. The two Figma baseline fixtures are both `counterAxisSizingMode: FIXED`,
so none of this shape exists in the corpus.

### One cache, lent by default and held where it cannot be

The typesetter is passed in, never constructed here.
`TaffySolver::with_typesetter(&mut Typesetter)` borrows the caller's single
`Typesetter` for the solve; `TaffySolver::new()` carries none — the text-free
path, and what every non-text solve and the fixed-commit equivalence tests use.
`TaffySolver::with_text(&mut Typesetter, Vec<Atlas>)` is the third form: it
measures text _and_ stages it (see "The one text stager" below).

`TaffySolver::owning(TextResources)` is the fourth, added at story #863, and it
**holds** the typesetter rather than borrowing it. It exists for the caller that
has nothing to lend: `dashlang::attach_live` takes a `Box<dyn LayoutSolver>` and
keeps it for the life of the scene, so the solver in that box is `'static` and
outlives every local a document loader could lend it. Every `.dsb` load path
therefore built `TaffySolver::new()`, and a loaded document containing text drew
no glyphs and measured its text nodes as empty leaves. Every integration crate
that loads a document can now be handed the text it needs — `dashscene-desktop`
and `dashscene-web` take a `TextResources` directly, and `dashscene-ffi` reaches
one through `from_faces` below.

`TextResources::from_faces` is how a caller that holds no Rust type builds one
(story #947). It takes owned bytes per face — the family name, the CSS weight,
the font file, the index within a collection, and an optional committed sheet as
its PNG and metrics blob — and returns the typesetter and the atlas list
together. Owned rather than borrowed because the caller it was written for is a
C ABI, whose pointers are valid only for the length of the call.

**It is no longer only the C ABI's, and the owned bytes are what let it travel**
(issue #992). `corpus/showcase` has called it since PR #988, from two places and
keeping one half of each result, because a cascade and an atlas set have
different lifetimes there — the atlas set is converted once for the process
behind a `LazyLock`, and a typesetter is built per scene. Both facades now
re-export `FaceBytes`, `AtlasBytes` and `TextResourcesError`, so an embedder on
`dashscene-desktop` or `dashscene-web` reaches it without depending on this
crate. What that changes here is the audience, not the shape: owned bytes suit a
caller assembling a descriptor as much as one marshalling across an ABI, and the
font-slot ordering this function enforces is the reason it stays one call in
every case. `docs/design/host-integration.md` carries the facade half.

**Both lists come out of one walk, which is the whole point.** Faces are grouped
into families on `FontFamily::name_matches`, the same predicate that resolves a
document's `TextStyle::family`, so two spellings of one name are one family
rather than two — grouping by string equality instead would put a requested
weight in a family that does not hold it, because `Typesetter::probe_order`
promotes only the first family whose name matches.
`Typesetter::with_named_font_families` then flattens family-major over that
grouping, and the atlases are emitted in the same pass, so each face's sheet
lands at the slot its glyphs will carry however the caller ordered the argument.
Building the atlas list separately is what would let a caller mis-order it, and
a mis-ordered list samples the wrong face rather than failing.

What is refused rather than assembled, each error naming the descriptor it came
from: no faces at all, a family name that is empty once trimmed and so could
never be requested, bytes that are not a parseable face, metrics that do not
decode, a sheet whose PNG header does not parse or does not carry the extent its
metrics declare, a glyph in those metrics described by exactly one of its two
quads, and a set where some faces carry a sheet and some do not.
`TextResourcesError` carries `Display` and `Error` so that each of those reads
as a sentence: `dashscene-ffi` puts the string straight into
`ds_last_error_message`, where every other message on that path is prose.

The header is read through `dashpaint::image_id::identify` — the same reader
`dashc`'s compile gate and `dashscene-validator`'s load gate use, which verifies
the first chunk is `IHDR` and its length is 13 rather than trusting two fixed
offsets. It is read at all because `dashscene-gpu`'s decoder panics rather than
returning on a payload that does not decode, resting that on an upstream gate a
host's bytes never passed. **What it buys is the header and the extent, and no
more**: a correctly-headed PNG whose `IDAT` is truncated or CRC-corrupt still
passes here and still panics at the first draw, caught at the C boundary and
reported as `DS_PANIC`. Closing that would mean decoding the whole sheet at
load, which is a cost and a separate decision.

A glyph carrying neither quad is dropped rather than refused — that is an empty
outline, which the space is — and it is a drop rather than a diagnostic because
`Atlas::glyph` never needs to find one. Exactly one of the two is the
inconsistency: `AtlasMetrics::from_bytes` does not check the pair agrees, so a
dropped half-described glyph would leave the binary search missing that
character with the load reporting success.

Holding it inside the solver rather than in a wrapper is what keeps the retained
tree. A wrapper that owns the typesetter must build a `TaffySolver` inside every
call, so every solve starts with no tree and rebuilds it, which is #164's saving
paid back per frame. `corpus/showcase` carried the last such wrapper and was
moved off it at issue #950; the invariant that move had to satisfy — one solver
sees every commit into an arena, in order — is
`docs/decisions/one-solver-per-live-scene.md`.
`docs/decisions/measure-callback-typesetter-seam.md` carries why lending remains
the default and why the held arm satisfies its reason rather than waiving it.

The borrow is the single-source discipline: layout measures text against the
same shaped-run cache the painter reads at paint time (#30), so the two cannot
disagree about a glyph's size (P2 — one typesetter). The shaped-run cache stores
font-unit, unpositioned runs keyed by paragraph text within one shaping posture
(`docs/decisions/shaped-run-cache-font-units.md`), so one entry serves every
render size and re-measuring unchanged text costs a lookup, not a re-shape.

### The one text stager

`TaffySolver` also implements the text half of the solver seam — the two
defaulted `LayoutSolver` methods `atlases` and `stage_text` (story #542,
`docs/decisions/glyph-runs-cross-boundary-b.md`). Commit asks it for every text
node's placed glyphs exactly as it asks for every node's rect, and stamps each
returned run with its node's rect index. This crate is the right home for the
same reason it is the right home for the measure callback: it is the one crate
holding both `dashscene-core` and `dashscene-typeset`, and it already borrows
the caller's `Typesetter`.

Staging lays each node out under its **lowered text axes** — the same
`text_shape(style)` the measure callback uses — within the box _this_ commit
solved, and offsets the block by the vertical alignment's share of the box's
free space. Measure and paint therefore agree by construction: one typesetter,
one axis policy, one commit. The `geometry` closure commit supplies is what
makes "this commit's box" available; a stager reading `Arena::committed()` would
place glyphs at the previous front buffer's boxes.

`with_text` takes the atlases in the cascade's font-slot order, because a shaped
glyph carries the slot of the face that shaped it and that slot indexes the list
directly. A solver built with `with_typesetter` carries no atlases and therefore
stages nothing — it measures text without painting it, which is what a caller
wanting layout alone asks for. Runs are re-staged in full on every commit; the
measured cost is about 1.5 µs per text node, and the decision record carries the
sweep behind that figure.

The `TextContext` owns its text so the Taffy tree can outlive the arena borrow.
Shaping itself is not repeated across solves, because the cache sits in front of
it. The v0.4 retained tree (#164, "Retained tree + pruned readback" above)
rebased onto this measure seam and the `with_typesetter` signature: the
incremental solve refreshes a node's `TextContext` through `set_node_context`
for the nodes it re-styles, so a dirtied text/style node re-measures while a
clean one keeps Taffy's cached measurement — which is why the contract is
recorded here and in `docs/decisions/measure-callback-typesetter-seam.md` rather
than left as wiring.

## Variant-switch FLIP (v0.4, story #22)

`crates/dashscene-engine/src/flip.rs` animates the layout delta a variant switch
(or any re-solve) produces. It is a thin engine-side binder onto `dashcue`, not
standalone geometry math and not a `dashcue` producer:

- `prop_key(node, channel) -> dashcue::PropKey` exposes core's one packing
  (`dashscene_core::prop_key`: `(node slot << 8) | channel
  code`, beside
  `Channel` — the document binding vocabulary) as the typed `dashcue` key FLIP
  tracks carry; `decode_prop_key` wraps core's one canonical decoder. Since
  story #167 there is no other packing — `dashlang`'s reactive layer builds its
  keys from the same core math (debts #207/#208).
- `VariantFlip::start(before, after, &dashcue::VariantTransition)` takes the two
  solved layouts as `&[(NodeId, SolvedRect)]` slices and binds a caller-declared
  transition: it resolves each track's `from`/`to` from the before/after rects
  and hands them to `dashcue`'s `Scheduler`. `dashcue` carries no resolved
  values (P1), so the engine binds them at commit time. It refuses, by named
  panic, a track key that does not decode or that names a non-rect channel — a
  foreign packing can no longer silently mis-bind (debt #207); a raw key that
  happens to decode to a valid animated rect channel remains indistinguishable
  from a real one.
- `advance(dt)`, then `sample(node)` / `sampled_rects()` reassemble a full
  `SolvedRect` per node by overlaying the live per-channel scheduler samples on
  the `after` target.

**The animated set is the nodes that are animating, never the nodes a transition
declared** (v0.13, debt #487). Three cases put a node outside it, and they are
one rule: a node with no declared track, a node whose declared channels have all
finished (`advance` drops it), and a node whose every declared channel starts
and ends at the same value. In all three the node's rect is its `after` rect,
which is what a consumer composing the animated set over the `after` layout
already holds — so the set's membership is a statement about motion, not a table
of everything the author named.

That contract is what lets `start` decline a channel whose `from` equals its
`to`. `dashcue`'s binding callback may return `None` (issue #74), and it
computes a track's stagger delay from the track's **declared** index, so
declining one leaves every other track on the schedule the author wrote. A
switch that changes one channel of a many-channel declaration now pays for the
one channel, not for the declaration.

The two snapshots need no new bookkeeping: `commit` writes the back buffer while
the previous generation's rects are still live in the front buffer, so the
caller reads `before` and `after` straight from `arena.committed()` across the
switch (this is the previous-commit-geometry accessor
`docs/decisions/layout-solver-seam.md` anticipated for #22). A mid-flight
retarget resumes from the current sample, because `start` delegates interruption
to the scheduler's retarget rule. Acceptance is in
`crates/dashscene-engine/tests/flip.rs` (a linear tween between two layouts; a
second switch mid-flight that retargets without snapping; a spring FLIP that
replays bit-identically, E5).

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.1 (Taffy as sole
  solver, R2 vocabulary), §7.2 (the common runtime's measure callback — text
  drives hug sizing), and §6.3 (FLIP); `docs/roadmap.md`'s v0.2, v0.4, v0.5, and
  v0.8 slices; issue #9, issue #29, issue #165, issue #164, issue #22, and issue
  #43 acceptance criteria (with the folded debts #236, #177, #115).
- Blocks: #10 (negative-gap lowering), #11 (flex goldens). The v0.8 vocabulary
  (#43) blocks #264 (the dashc un-pin) and #46 (the stress corpus; #236's fix is
  a prerequisite of its negative-gap case). The measure seam blocks #30 (the
  hug-sizing text golden). The retained tree (#164) and Visible lowering (#165)
  serve #166 (the reactive layer's contained-write skip and bounded pools); FLIP
  (#22) serves #23 (the FLIP golden sampling).
- Related decisions: `docs/decisions/layout-solver-seam.md` (the partial-solve
  contract #164 extended, and the FLIP hook),
  `docs/decisions/flex-vocabulary-shape.md`,
  `docs/decisions/v08-layout-vocabulary-shape.md` (wrap/grid/baseline),
  `docs/decisions/negative-margin-hug-rebate.md` (#236),
  `docs/decisions/measure-callback-typesetter-seam.md`,
  `docs/decisions/shaped-run-cache-font-units.md`,
  `docs/decisions/weight-selection-in-the-cascade.md` (why the measure context
  carries the weight), `docs/decisions/visible-is-layout-opacity-is-paint.md`.
- Related design: `docs/design/typeset-latin.md` (the shaped-run cache the
  measure callback consumes); `docs/design/dashscene-core-arena.md`
  ("Incremental commit"); `docs/design/dashcue.md` (the scheduler and
  `VariantTransition` FLIP binds).
