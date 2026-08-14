# A signal that drives text must also drive a write that solves

    status   RETIRED 2026-08-14 by the replay fix this record defers
             (issue #621). Kept for the reasoning, which is why the fix took
             the shape it did; the authoring rule below no longer binds.
             Was: accepted (story/demo-backend-badge, 2026-08-04)
    scope    every scene authored against dashlang's reactive layer, and every
             producer driving text through it — corpus/showcase today, the
             loader-side attach_live path on the same terms; retired by the
             dashlang rect-replay fix this record defers

## Context

Since story #542, `dashscene-core`'s commit is the producer of the glyph-run
table (`docs/decisions/glyph-runs-cross-boundary-b.md`, "The producer story,
decided"). Commit rebuilds that table every frame from whatever the
`LayoutSolver` it was handed stages through the seam's two defaulted methods,
`atlases` and `stage_text`. A solver that implements neither stages nothing, and
a commit through it publishes a run table with **no runs in it at all** — not
only for the node that changed, but for every text node in the scene.

`dashlang`'s contained-write optimization (A1, `docs/design/dashlang.md`) hands
commit exactly such a solver. When a frame's writes all patch or all skip, the
live scene replays its retained rect cache through a private `CachedSolver` fed
to `commit_with`, so the real solver — the one carrying the typesetter and the
atlas list — is never invoked. `CachedSolver` takes `LayoutSolver`'s defaults
for `atlases` and `stage_text`.

The two compose into a hazard exactly where the reactive layer is most useful.
`classify` (`crates/dashlang/src/reactive.rs`) sorts every scalar write into
`Patch`, `Solve` or `PaintOnly`; `bind_text` is always paint-only, and
`Channel::Opacity` and the four `Fill` channels are always paint-only. A signal
bound only to those channels produces a tick that never solves — so the frame
that changes a string is the frame that erases every glyph run in the scene, the
new string included.

This is not theoretical and it is not new. `corpus/showcase/README.md` records
it as "the defect these scenes are written around", and
`corpus/showcase/src/typography.rs` pairs its readout's text binding with a
width binding for exactly this reason. It cost this story a critical defect
anyway: the painter badge first bound only text and opacity, and announcing a
painter wiped the scene's own text along with its own label. The hazard was
recorded in one scene's prose, which is not where a person designing a new scene
looks.

## Retired, and what discharged each reason for deferring

**Option 1 landed at issue #621.** `CachedSolver` now borrows the solver
`LiveScene` already retains and forwards `atlases` and `stage_text` to it, so a
replaying commit re-stages text and **the authoring rule below is no longer
required**. A signal may drive `bind_text` alone.

The two reasons this record gave for deferring, and what became of them:

- **"`reactive.rs` was under change on another branch."** Discharged. Epic #951
  placed the fix in `dashlang` for the mirror-image reason: it is
  `dashscene-core`'s `arena.rs` that is contended now, by three open v0.19
  stories, and `reactive.rs` is free.
- **"A change to the cost model of every paint-only frame ... belongs to whoever
  owns that budget."** This is the reason that mattered, and it is **accepted
  rather than discharged**: a paint-only tick now re-stages text, at the ~1.5 µs
  per text node per commit that `glyph-runs-cross-boundary-b.md` measures. Epic
  #951 scheduled the fix knowing the alternative — carrying runs forward inside
  `commit_with`, which re-stages nothing — and chose this side on
  file-contention grounds. **The cheaper design is still the cheaper design**,
  and it is where a future frame-budget problem should be answered.

  One cost that would have been paid per frame was removed rather than accepted:
  `ShowcaseSolver::stage_text` deep-copied the whole atlas set, about 226 kB of
  PNG payload, on every call. It shares the `Arc` now, because this change is
  what moved that call into the frame loop.

## Options

1. **Fix the replay.** `CachedSolver` delegates `atlases` and `stage_text` to
   the solver `LiveScene` already retains, so a replaying commit re-stages text
   and no authoring rule is needed. This is the fix `corpus/showcase/README.md`
   names.
2. **Constrain the authoring.** Require every signal that drives a text binding
   to also drive a write that forces the solve, so the tick that changes a
   string is always a solving tick.

## Decision

**Option 2 now; option 1 is the fix and is not made here.**

A signal bound through `Node::bind_text` must also be bound, on the same signal,
to a channel whose write classifies as `WriteClass::Solve`. What qualifies is
decided by `classify` and `write_is_single_rect`
(`crates/dashlang/src/reactive.rs`) and is worth stating plainly, because the
answer depends on the target node **and on its ancestors**, not only on the
channel:

- `Channel::Opacity` and the four `Fill` channels never solve, on any node.
- `Channel::Gap` always solves — a gap redistributes children by definition.
- A `visible_when` binding always solves — the flush sets `layout_dirty`
  unconditionally for it.
- Every remaining channel — `X`, `Y`, `Width`, `Height` — solves **unless the
  write is a contained single-rect write**, and that requires both of the
  following at once:
  - **The node is contained.** Every one of its ancestors is a passthrough
    container (`LayoutMode::None`) that hugs on neither axis — the rule
    `child_contained` propagates down at build and `ancestor_contained`
    recomputes bottom-up for a loaded arena. One flex or grid ancestor, or one
    hugging ancestor, makes the node uncontained, and then **every** rect write
    on it solves, whatever the channel and whatever its own children. A root is
    contained by construction, having no ancestors at all.
  - **The write stays inside the node's own rect** (`write_is_single_rect`): `X`
    or `Y` on a node with **no** children; `Width` or `Height` on a node with no
    children, or on a passthrough node.

The containment term is the one that is easy to miss, and it is the term the
worked examples below turn on. Measured with a counting solver, one bound
`Channel::Width` write on a childless leaf, per tick:

    parent LayoutMode::Horizontal   1 extra solve    (uncontained: Solve)
    parent LayoutMode::None         0 extra solves   (contained: Patch)

Two clauses follow, and both are load-bearing.

**The solving write carries real content.** It is not a lever pulled for its
classification. A write whose only purpose is to force a solve is a write a
later reader deletes as dead, which restores the defect silently. The badge's
bound `Channel::Width` is the pill's own width, computed from the announced
label's character count (`corpus/showcase/src/badge.rs`, `pill_width`);
`typography`'s text binding shares its signal with a bar that reflows.

**The test asserts on the committed glyph-run count, never on the text prop.**
The prop updates whether or not the tick solved, so a prop-only assertion passes
while every run in the scene is being erased — which is precisely what happened
here, with tests green. The assertion that has teeth is a count relative to what
the scene staged at build: exactly one more run after announcing, losing none.
"Greater than zero" is not enough, because a scene carrying no text of its own
cannot fail it.

## Why option 1 is right and still deferred

The replay fix is correct and this record does not argue against it. It is
deferred on two grounds.

It is a change to the cost model of every paint-only frame, not a local repair.
A replaying commit would re-run the stager, and full re-staging costs about 1.5
µs per text node per commit with a warm shaping cache
(`docs/decisions/glyph-runs-cross-boundary-b.md`, "Per-frame cost, measured").
The whole point of A1 is that a contained write costs nothing proportional to
scene size; giving it back a per-text-node cost is a decision about the frame
budget, and it belongs to whoever owns that budget rather than to a story adding
an on-screen label.

And `crates/dashlang/src/reactive.rs` was under change on another branch when
the showcase landed, which is why the v0.14 work filed the fix rather than
making it. That reason has not been discharged.

## Consequences

- The rule is enforced by documentation and by review, not by the compiler or a
  diagnostic. This project declines that shape elsewhere (P4 — an out-of-profile
  construct is a named diagnostic, never a silent drop), and the exception is
  accepted here only because the enforcement that would replace it is option 1,
  which retires the rule outright rather than mechanising it.
- When option 1 lands, this record is retired, not relaxed: the authoring
  constraint disappears, and the scenes that pair a text binding with a
  layout-affecting one keep doing so because those pairings are real content.
- Until then, a new scene with signal-driven text is not reviewable by reading
  its bindings alone. The reviewer has to ask which channel the signal's other
  binding writes, on what kind of node, **and under which ancestors**: a
  `Channel::Width` on a childless leaf patches and does not solve when every
  ancestor of that leaf is a passthrough, non-hug container, and the identical
  write on the identical leaf solves as soon as one ancestor is a flex or grid
  container or hugs. `typography`'s `gauge-fill` is exactly that childless leaf,
  and it qualifies only through the ancestor half of the rule.

## Trace

- Mechanism: `classify` and `write_is_single_rect` in
  `crates/dashlang/src/reactive.rs`; the A1 paragraph of
  `docs/design/dashlang.md`.
- Pinned by: `corpus/showcase/tests/badge.rs`,
  `announcing_a_painter_adds_exactly_one_glyph_run_to_every_scene` — the
  build-count-plus-one assertion, over every scene in `showcase::SCENES`.
- Worked examples: `corpus/showcase/src/badge.rs` ("Why the pill's width is
  bound") and `corpus/showcase/src/typography.rs` ("Every signal here drives
  layout, deliberately"). Those two are the whole set — they are the only files
  under `corpus/showcase/src/` that call `Node::bind_text` at all.
  `corpus/showcase/src/surfaces.rs` is not among them: it has no text binding,
  and its signals drive `Channel::Width` and `Channel::X` only.
- Measurement: `corpus/showcase/README.md` ("The defect these scenes are written
  around"), which holds the run over 1,200 ticks at two extents.
- Related decisions: `docs/decisions/glyph-runs-cross-boundary-b.md` (commit as
  the run producer, and the per-node staging cost this defers paying);
  `docs/decisions/visible-is-layout-opacity-is-paint.md` (the layout/paint split
  `classify` reads); `docs/decisions/bindings-are-explicit-and-flat.md` (the
  flat table that makes a write statically classifiable at all).
