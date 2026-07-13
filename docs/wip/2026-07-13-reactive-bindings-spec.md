# Reactive bindings and incremental commit — design

    status   draft (brainstorm output, 2026-07-13)
    scope    dashscene-core (signal + binding tables, incremental commit),
             dashlang (binding vocabulary), dashscene-engine (retained
             solver), dashpaint (dirty set across boundary B)
    slice    v0.4 and later — depends on #20 (variant table + set_variant)

## Problem

A producer today has two ways to update a live scene, and neither one
holds at 60 Hz on target hardware.

`dashlang` cannot update a scene at all: `Scene::build` appends its
roots to the arena and commits once, and it returns only a generation
number, so a producer cannot keep the `NodeId`s it would need in order
to mutate anything later. Rebuilding the value tree appends a second
copy of the scene rather than updating the first.

Hand-written `set_prop` calls do work, but every commit costs
`O(total nodes)` regardless of how few props changed. `commit_with`
walks the full DFS, asks the solver for every node's rect, re-interns
the entire paint table from scratch into a fresh `HashMap`, and then
diffs every rect against the previous buffer to discover what changed.
The painter then redraws everything, because the dirty set is never
passed across boundary B.

The target workload is roughly 1000 live nodes at 60 Hz with a hard
CPU budget: many independent components (gauges, telltales, live
readouts) changing continuously and independently, screen-variant
transitions that reflow their neighbours, and a stacking container
that grows as telltales activate.

## Constraints that shape the design

- **P1** — the document carries intent, never results. A binding
  ("this node's width depends on signal 3") is intent. A resolved
  value is not.
- **P2** — painters only color. `dashscene-skia` depends on
  `dashpaint` and not on core, so a painter cannot reach the arena.
- **P3** — producers mutate, the runtime owns time. Nothing
  producer-side executes inside the frame loop. A reactive layer must
  therefore be **push-on-flush**, never pull-on-paint: a computation
  that lazily recomputes when the renderer reads it is exactly the
  frame-synchronous callback P3 bans.
- **P4** — vocabulary is validated, never discovered.
- **R4** (DESIGN §6.3) — interruptibility, and statically bounded cost:
  the frame budget must be provable from the document.
- **R-T1 / R-T4** (DESIGN §9) — one render pass per frame; per-frame
  CPU cost is the dirty-range instance-buffer upload from the rect
  table plus submission, and nothing else.
- **SCOPE §9** — core must not depend on `dashcue`, so that a producer
  setting one property does not link the animation crate.

## Decisions

### D1 — No new crate. Tables in core, vocabulary in dashlang

The crates in this workspace exist to make a boundary mechanical: a
crate that depends on nothing cannot violate the principle it
enforces. `dashpaint` is boundary B, so a painter physically cannot
call into the arena. `dashcue` depends on nothing, so the scheduler
physically cannot reach producer state.

A reactive layer sits on no such boundary. It would depend on core (to
write props) and on `dashcue` (to smooth them), and its authoring
surface belongs on `dashlang`'s `Node`. A separate crate would add a
wall with nothing on either side of it. Therefore:

- **`dashscene-core`** gains a signal table and a binding table. A
  binding is intent, and core is the intent model.
- **`dashlang`** gains the authoring vocabulary (`scene.signal`,
  `.bind`, `.smooth`, `.visible_when`, `.bind_variant`) and holds the
  transform closures in a side table.
- **`dashcue`** and **`dashpaint`** are unchanged.

A binding record in core is `(SignalId, PropKey)` and nothing more. It
carries no smoothing spec, because that spec is `dashcue` data and core
must not depend on `dashcue` (SCOPE §9). Where core must reference one,
it holds an opaque handle it never interprets — the same technique
`dashcue` already uses in the other direction with `PropKey`.

#### The address space

`PropKey` already exists, in `dashcue`, as an opaque `u64` whose doc
comment reads: "the caller encodes prop identity into it (the engine
packs node index and channel); `dashcue` only compares it". That is
exactly the address a binding targets, so bindings reuse it rather than
inventing a second one:

    PropKey  =  node index (u32)  ++  channel (u32)

A **channel** is one scalar prop slot — `Width`, `Height`, `X`, `Y`,
`Gap`, `Fill.r`, `Fill.g`, and so on. A `Color` is four channels. This
is the "property (`set_prop`) — field name — u16 wire id — the schema"
row the `id-model-strings-compile-to-indices` record already reserves.

Both crates then speak one language, `(PropKey, f32)`: `dashcue` keys
its animation tracks by it, and core keys its bindings by it. The
opacity is what keeps them free of each other — neither crate needs the
other's types to agree on an address.

### D2 — Bindings are explicit and declarative, not implicitly tracked

SolidJS and `leptos_reactive` discover a computation's dependencies by
running it and recording signal reads on a thread-local stack. That
model is rejected here for three reasons, the third being decisive:

1. A dependency set discovered by running a computation can differ
   between runs, so the subscription list is rebuilt on every
   execution. That is per-frame bookkeeping and allocation, which R-T4
   excludes.
2. It requires an ambient thread-local reactive context, which does not
   survive the FFI seam or multiple scenes cleanly.
3. It makes the binding graph knowable only at runtime. A prop cannot
   then be statically classified as layout-affecting or paint-only, so
   containment cannot be proven and the frame budget cannot be proven.
   This inverts P4 — the vocabulary would be discovered, not validated.

A binding therefore names its source signal, its target `PropKey`, and
a pure transform. The dependency graph is a flat table, known before
anything runs. Derived values are explicit combinators (`signal.map`,
`zip2(a, b).map`), each allocating one memo slot in the same flat
table. No `Rc`, no `RefCell`, no object graph: the reactive graph is
arena-shaped like everything else in this codebase.

### D3 — The scene tree is static; dynamic lists are bounded pools

Node ids are DFS positions, and the dirty diff compares rects **by
index**. Inserting one node into the middle of the tree shifts every
subsequent index, so every later rect is compared against the wrong
predecessor and the whole tail of the scene reports dirty. Structural
change does not cost a little in this architecture; it defeats the
dirty set entirely.

The tree is therefore static after build. Every node that can ever
appear is present, which is already what "variant closure is per
component SET" and "hidden nodes export as `visible:false`" imply. A
list that varies in length is a **bounded pool**: instances are
materialized at build to a declared maximum, and a length change is a
visibility write plus a rebind. Data longer than the pool is shown
through a recycled window.

This is also the only option that keeps R4 honest. A tree that can grow
arbitrarily at runtime makes the frame budget unprovable from the
document by construction.

Fully dynamic surfaces (map view, settings) are not modelled as scene
nodes at all. They are `role=placeholder` handoffs to another renderer,
per DESIGN §10.2.

### D4 — One commit per frame carries both signals and animation

    app pushes data, at any time
        -> signals write; their bindings are marked dirty

    frame tick:
        txn = arena.open()
        flush dirty bindings      -> set_prop / set_variant
        scheduler.advance(dt)     -> set_prop for each live dashcue track
        txn.commit()              -> incremental resolve (D5)

    painter                       -> uploads the dirty rects (D6)

Signals are evaluated during the producer's flush and never read by the
renderer, which satisfies P3. `dashcue`'s scheduler is runtime code
that owns time, so it is entitled to write props; it needs no new
mechanism and uses `set_prop` like any other producer.

This closes the seam DESIGN §6.3 already named. "Per-prop smoothing —
declared spring/filter on a **bound prop** (gauges, live values)" now
has a referent: the signal sets the spring's target, the scheduler
drives the actual value, and a mid-flight retarget is a new target on a
live track, which `Scheduler::start` already supports.

Scalar channels (`Width`, `X`, `Fill.r`, …) are bindable and
animatable, because `dashcue` interpolates `f32`. Discrete props
(`Text`, `LayoutMode`, `Visible`, the variant index) are bindable but
switch instantly; their layout consequences animate through FLIP.

### D5 — Commit becomes incremental

Three changes, each independently useful:

**Retain the Taffy tree.** `TaffySolver` is a zero-sized stateless
struct that builds a fresh `TaffyTree` on every solve and drops it, so
Taffy's per-node cache is born empty and dies unused every frame. Taffy
already provides the cache (nine measure slots plus a final-layout
entry per node, keyed by the sizing inputs) and `mark_dirty(node)`,
which clears a node and its ancestors. A clean subtree with unchanged
constraints then returns from cache without being re-descended. The
`LayoutSolver` seam already takes `&mut self` for exactly this reason.

**Prune the readback.** Taffy stores layouts relative to the parent;
`LayoutSolver::solve` returns absolute rects. Converting one to the
other naively is an `O(n)` walk that would consume the win. The prune:
if a node's relative layout is unchanged and its parent's absolute
origin is unchanged, nothing in its subtree moved, so the subtree is
skipped. This is the only genuinely new layout logic required.

**Retain the paint interner.** The paint table is currently rebuilt
from scratch every commit, with a fresh `HashMap` and SipHash. That is
per-frame allocation and hashing that R-T4 excludes, and it is also why
the dirty diff must compare resolved colours as well as entry bits:
indices shift between commits. Retaining the interner makes paint
indices stable, collapses the dirty check to a bit compare, and lets a
painter delta-upload the paint buffer too. The table then grows
monotonically and will eventually want refcounting or compaction — an
accepted trade for a bounded scene with a bounded palette.

Two contracts have to change to allow any of this:

- `LayoutSolver::solve` must be able to return only the rects that
  changed. `commit_with` currently panics if the solver omits a node
  ("solver returned no rect"), which mandates a full solve. The
  invariant is re-expressed rather than deleted: every node has a rect,
  from this solve or from the previous one.
- The back buffer stops being rebuilt from scratch and starts as a copy
  of the front buffer, patched at the changed indices. Copying ~1000
  24-byte entries is a few microseconds and is cache-friendly.

The dirty set also stops being _discovered_. Today `commit_with` diffs
every rect against the previous buffer to learn that one gauge moved.
With bindings, core already knows which nodes were written; the dirty
set becomes the union of two `O(changed)` sets — nodes whose paint
props were written, and nodes whose solved rect actually moved, which
the retained solver reports.

### D6 — The dirty set crosses boundary B, advisory, with an oracle

`Painter::paint` does not receive the dirty set today, so R-T4 is not
implementable by any painter: `CommittedScene::dirty()` is produced by
core and consumed by nobody. The trait gains it as a slice, preserving
the slice-input shape that `painter-trait-infallible-slice-input`
chose (so `dashpaint` still does not depend on core):

    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
             images: &ImageTable, clips: &ClipTable, dirty: &[u32]);

**Advisory** is the contract: ignoring `dirty` and redrawing everything
is always correct, and a painter that honours it must produce identical
output to one that does not.

Damage-region partial redraw is explicitly **not** the goal. On a
tiling GPU, restoring the previous framebuffer into tile memory to
repaint part of it is the flush-and-resolve that R-T1 forbids. The GPU
redraws every quad in one pass; what must not be repeated is the CPU
work and the upload (R-T4).

`SkiaPainter` gains a second mode that models the **instance buffer**,
not the canvas: it keeps a persistent copy of the rect table, refreshes
only the entries the dirty set names, and then redraws every quad from
that copy. That is R-T4's data flow exactly, and it is _not_ a
damage-region redraw — no pixels are selectively preserved.

**Its purpose is not speed — it is a test oracle for the dirty set.** A
missing entry in the dirty set shows up on the GPU as a stale
instance-buffer entry (a frozen gauge, a telltale that will not clear),
which is intermittent and hard to diagnose on target hardware. The same
bug shows up in the CPU painter as a stale pixel, which CI can diff
deterministically without a GPU.

The oracle is a dedicated test over a _sequence_ of mutate-commit-paint
steps, not a second rendering of the existing goldens: staleness only
exists across frames, and the goldens are single-frame renders of
hand-built tables with no dirty set behind them, so running them in both
modes would pass vacuously.

What the oracle catches is bounded, and the bound is worth stating. The
retained buffer holds `RectEntry` values, which carry paint and clip
_indices_; the paint and clip tables themselves are handed to the painter
fresh each frame. So a stale entry renders wrong pixels only when the
entry's **bits** changed — geometry, the paint index, or the clip index.
This is not a gap in the simulation: R-T4 names the rect table as the
thing that delta-uploads, and the small paint and clip tables must
re-upload wholesale today anyway, because both are re-interned every
commit and their indices are unstable. When D5 retains the interner and
those indices become stable, the oracle must be extended to retain the
paint table too.

## The authoring surface

    let mut scene = Scene::new();

    let speed   = scene.signal(0.0f32);
    let warning = scene.signal(false);

    scene.roots([
        node("cluster").mode(V).gap(8.0).children([
            node("bar")
                .size(0.0, 12.0)
                .bind(Channel::Width, speed.map(|v| v * 2.0))
                .smooth(Channel::Width, Spring::critically_damped(0.15)),
            node("readout")
                .bind_text(speed.map(|v| format!("{v:.0} km/h"))),
            node("telltale")
                .visible_when(warning),
        ]),
    ]);

    let mut live = scene.build(&mut arena);   // ids and PropKeys resolved here

    // per frame
    live.set(speed, 87.5);
    live.tick(dt, &mut arena);

Bindings are declared **on the node**, which is where the information
belongs and where a designer would annotate it in Figma. `build`
assigns `NodeId`s and resolves each declared binding into a `PropKey`,
so a producer never handles a `NodeId` and cannot hold a stale one.
Components stay plain functions returning `Node`, as they are today.

`Scene::build` keeps taking `&mut Arena` rather than owning it, so
`dashlang`'s existing rule — the DSL is a producer, not an owner —
survives, and several producers can still build into one arena.

Signals cannot be free-standing values. A signal needs identity, two
nodes must be able to bind the same one, and identity without an owner
requires a global or thread-local allocator, which is the ambient state
this design rejects. The builder is that owner.

## Containment is provable, so R4 becomes a check

A prop write's cost is set by how far it propagates upward. A node's
size change escapes its own subtree only if an ancestor is
content-sized (hug). If every ancestor to the root is fixed or fill,
the change is contained, and the retained solver re-solves that subtree
and nothing else.

That property is visible in the document before anything runs. With the
binding table in core, the validator can therefore report: _this
channel is bound, and it sits under N hug ancestors, so a write to it
reflows M nodes_. A 60 Hz binding under a hug ancestor becomes a named
diagnostic. This is P4 applied to performance, and it is how "frame
budget provable from the document" (R4) becomes something a machine
proves rather than a claim in a specification.

## Alternatives considered

- **A separate `dashbind` crate.** Rejected: it sits on no boundary,
  and `dashlang` would depend on it anyway (see D1).
- **Implicit dependency tracking (Solid / leptos).** Rejected: it
  inverts P4 and forfeits the containment proof (see D2).
- **A general reactive crate (`leptos_reactive`, `futures-signals`).**
  Rejected: they are `Rc`/`RefCell` object graphs with per-node
  allocation, which would import pointer-chasing into a flat,
  index-addressed arena.
- **True insertion and removal with a keyed reconciler.** Rejected: it
  needs node removal and id recycling that core has never had, it
  allocates in the update path, it defeats the index-keyed dirty diff
  (see D3), and it makes R4 unprovable. It would be reconsidered only
  for a genuinely unbounded rendered list, which this domain does not
  appear to have.
- **Immediate-mode rebuild and diff (the React model).** Rejected:
  `O(total nodes)` in the producer every frame with allocation, which
  is the cost fine-grained bindings exist to remove.
- **Damage-region partial redraw.** Rejected: it is slower on a tiling
  GPU and contradicts R-T1 (see D6).

## Prerequisites and sequencing

Not yet built, and required:

1. **`set_variant` and the variant table** — story #20. The variant
   binding (`bind_variant`) has no target without it, and #20's API
   shape (flat variant index versus axis-keyed selection) is still
   undecided.
2. **A visibility / display prop.** `Prop` has none. Bounded pools and
   the telltale stack both need it, and it must be Taffy's
   `Display::None` flavour (out of layout), not merely unpainted.
3. **The `LayoutSolver` partial-solve contract** (D5).

Independently valuable, and buildable before the reactive layer:

- The retained Taffy tree and pruned readback (D5).
- The retained paint interner (D5).
- The dirty set across boundary B and the dual-mode Skia oracle (D6).

## Impact on queued work

- **#118 (`dashlang` flex builder vocabulary + `Scene::build_with`)** —
  this design adds binding vocabulary to the same `Node` builder. Doing
  both in one pass avoids reshaping the builder twice. #118 should be
  re-scoped or explicitly sequenced ahead of this work.
- **#20 (variant table + `set_variant`)** — gains a second consumer.
  Its undecided API shape should be settled with `bind_variant` in
  view.
- **#130 (core's `Prop` cannot express the v0.3 paint vocabulary)** —
  the `Channel` enum used by bindings and by `dashcue`'s `PropKey` is
  the same address space as `Prop`. Widening one should widen the
  other.

## Open questions

- Does `Channel` (the scalar address used by bindings and `dashcue`)
  become the canonical prop id, superseding the ad-hoc `Prop` enum, or
  does it sit alongside it? The `id-model` record already reserves
  "property (`set_prop`) — field name — u16 wire id — the schema
  (future)".
- Does the binding table eventually serialize into `.dsb`? A binding is
  intent, so P1 permits it, and it would let the Figma importer emit
  bindings and let a C# producer push a few signal values per frame
  instead of marshalling hundreds of `set_prop` calls. The obstacle is
  the transform, which is code. A bounded declarative expression
  vocabulary (scale, clamp, format) would solve it. This is out of
  scope here and is recorded only so the binding record's shape does
  not foreclose it.
- Does the paint interner need refcounting, or is monotonic growth with
  an occasional compaction sufficient for a bounded scene?
