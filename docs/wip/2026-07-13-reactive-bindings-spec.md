# Reactive bindings and incremental commit — design

    status   draft (brainstorm output, 2026-07-13)
    scope    dashlang (signals, bindings, transforms), dashscene-core
             (Visible prop, incremental commit), dashscene-engine
             (retained solver), dashpaint (dirty set across boundary B),
             importers/figma (bindings from Figma variables)
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
**both** the paint table and the clip table from scratch into two fresh
`HashMap`s, and then diffs every rect against the previous buffer to
discover what changed. The painter then redraws everything, because the
dirty set never crosses boundary B.

The target workload is roughly 1000 live nodes at 60 Hz with a hard CPU
budget: many independent components changing continuously and
independently, screen-variant transitions that reflow their neighbours,
and a stacking container that grows as its members activate.

## Constraints that shape the design

- **P1** — the document carries intent, never results. A binding ("this
  node's width depends on signal 3") is intent. A signal's _value_ is a
  result, and never enters the document.
- **P2** — one solver; painters only color. `dashscene-skia` depends on
  `dashpaint` and not on core, so a painter cannot reach the arena.
- **P3** — producers mutate, the runtime owns time. Nothing
  producer-side executes inside the frame loop. A reactive layer must
  therefore be **push-on-flush**, never pull-on-paint: a computation
  that lazily recomputes when the renderer reads it is exactly the
  frame-synchronous callback P3 bans.
- **P4** — vocabulary is validated, never discovered.
- **R4** (DESIGN §6.3) — interruptibility, and statically bounded cost:
  the frame budget must be provable from the document.
- **R-T1 / R-T4** (DESIGN §9) — one render pass per frame; per-frame CPU
  cost is the dirty-range instance-buffer upload from the rect table
  plus submission, and nothing else.
- **SCOPE §9** — core must not depend on `dashcue`, so that a producer
  setting one property does not link the animation crate.

## Non-goals

**The binding graph never models a relationship between two nodes.**
"When the alert list grows, the panel below it moves down" is a _layout_
relationship: the list and the panel are siblings in a flex column, the
list's content size changes, and the solver moves the panel. It is not a
dependency in the reactive graph.

Modelling it as a signal edge (alert count → panel's `y`) would
re-implement layout inside the binding table, which is what P2 forbids —
one solver, and it is Taffy. A binding therefore only ever connects
**data to a prop on one node**. Every "this pushes that" consequence
propagates through the solver.

This is the single most important simplification in this design. It is
what keeps the binding table flat and the reactive layer ignorant of the
tree.

## Decisions

### D1 — The reactive layer lives in dashlang. Core is unchanged

Two separate questions were conflated in an earlier draft, and they have
different answers.

**Should the reactive layer be its own crate? No.** The crates in this
workspace exist to make a boundary mechanical: a crate that depends on
nothing cannot violate the principle it enforces. `dashpaint` _is_
boundary B, so a painter physically cannot call into the arena (P2).
`dashcue` depends on nothing, so the scheduler physically cannot reach
producer state (P3). A reactive layer sits on no such boundary — it
would depend on core to write props and on `dashcue` to smooth them, and
its authoring surface belongs on `dashlang`'s `Node`. A separate crate
would add a wall with nothing on either side of it.

**Should the tables therefore go into core? Also no** — and that does
not follow from the first answer. The builder owns the signals
(`Scene::signal`, see "The authoring surface"), and `Scene` is
`dashlang`'s type. So:

- **`dashlang`** holds the signal table, the binding table, the
  transform closures, and the flush loop.
- **`dashscene-core`** is unchanged by the reactive layer. It gains a
  `Visible` prop (D7), but for layout reasons, not for bindings.
- **`dashcue`** and **`dashpaint`** are unchanged.

Why `dashlang` and not core:

- A signal's value is transient producer state. P1 keeps _results_ out of
  the document, and a signal value is much closer to a result than to
  intent.
- Core holds the arena and the staged-mutation API and no producer
  machinery at all; every producer sits above it. Putting a reactive
  graph, a dirty-binding list, and a flush loop inside core would make
  core own producer-side runtime state, which cuts against P3's split
  rather than supporting it.
- It satisfies SCOPE §9 by construction. `dashlang` depends on both core
  and `dashcue`; core never comes near the animation crate. An earlier
  draft put the tables in core and then needed an opaque handle to a
  smoothing spec that core must never interpret — a contortion invented
  purely to keep core away from `dashcue`. The contortion disappears
  when the layer sits in `dashlang`.
- Core needs **zero changes** for any of it. `dashlang`'s `tick` opens a
  `Txn`, flushes dirty bindings through `set_prop`, advances the
  `dashcue` scheduler, and commits. Every mechanism already exists.

D8 records the one condition under which part of this moves into core.

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

Both `dashlang` and `dashcue` then speak one language, `(PropKey, f32)`.
The opacity of the key is what keeps the two crates free of each other.

### D2 — Bindings are explicit and declarative, not implicitly tracked

SolidJS and `leptos_reactive` discover a computation's dependencies by
running it and recording signal reads on a thread-local stack. That
model is rejected here for three reasons, the third being decisive:

1. A dependency set discovered by running a computation can differ
   between runs, so the subscription list is rebuilt on every execution.
   That is per-frame bookkeeping and allocation, which R-T4 excludes.
2. It requires an ambient thread-local reactive context, which does not
   survive the FFI seam or multiple scenes cleanly.
3. It makes the binding graph knowable only at runtime. A prop cannot
   then be statically classified as layout-affecting or paint-only, so
   containment cannot be proven and the frame budget cannot be proven.
   This inverts P4 — the vocabulary would be discovered, not validated.

A binding therefore names its source signal, its target `PropKey`, and a
transform. The dependency graph is a flat table, known before anything
runs. Derived values are explicit combinators (`signal.map`,
`zip2(a, b).map`), each allocating one memo slot in the same flat table.
No `Rc`, no `RefCell`, no object graph: the reactive graph is
arena-shaped like everything else in this codebase.

### D3 — The scene tree is static; dynamic lists are bounded pools

Node ids are DFS positions, and the dirty diff compares rects **by
index**. Inserting one node into the middle of the tree shifts every
subsequent index, so every later rect is compared against the wrong
predecessor and the whole tail of the scene reports dirty. Structural
change does not cost a little in this architecture; it defeats the dirty
set entirely.

The tree is therefore static after build. Every node that can ever appear
is present, which is already what "variant closure is per component SET"
and "hidden nodes export as `visible:false`" imply. A list that varies in
length is a **bounded pool**: instances are materialized at build to a
declared maximum, and a length change is a `Visible` write plus a rebind.
Data longer than the pool is shown through a recycled window.

D7 confirms this works: a `Visible(false)` node still resolves to a
(degenerate) rect, because `commit_with` requires every node to resolve
(P4). The slot keeps its rect-table index, so no DFS index ever shifts,
and the container collapses because Taffy's `Display::None` removes the
node and its children from layout.

This is also the only option that keeps R4 honest. A tree that can grow
arbitrarily at runtime makes the frame budget unprovable from the
document by construction.

Fully dynamic surfaces (map view, settings) are not modelled as scene
nodes at all. They are `role=placeholder` handoffs to another renderer,
per DESIGN §10.2.

### D4 — One commit per frame carries both signals and animation

    app pushes data, at any time
        -> signals write; their bindings are marked dirty

    frame tick (dashlang::LiveScene::tick):
        txn = arena.open()
        flush dirty bindings      -> set_prop / set_variant
        scheduler.advance(dt)     -> set_prop for each live dashcue track
        txn.commit()              -> incremental resolve (D5)

    painter                       -> uploads the dirty rects (D6)

Signals are evaluated during the producer's flush and never read by the
renderer, which satisfies P3. `dashcue`'s scheduler is runtime code that
owns time, so it is entitled to write props; it needs no new mechanism
and uses `set_prop` like any other producer.

This closes the seam DESIGN §6.3 already named. "Per-prop smoothing —
declared spring/filter on a **bound prop** (gauges, live values)" now has
a referent: the signal sets the spring's target, the scheduler drives the
actual value, and a mid-flight retarget is a new target on a live track,
which `Scheduler::start` already supports.

Scalar channels (`Width`, `X`, `Fill.r`, …) are bindable and animatable,
because `dashcue` interpolates `f32`. Discrete props (`Text`,
`LayoutMode`, `Visible`, the variant index) are bindable but switch
instantly; their layout consequences animate through FLIP.

FLIP needs no new bookkeeping. `commit` writes the back buffer while the
previous generation's rects are still live in the front buffer, so the
"first" snapshot FLIP requires is already there. Before and after are the
two buffers.

### D5 — Commit becomes incremental

Three changes, each independently useful:

**Retain the Taffy tree.** `TaffySolver` is a zero-sized stateless struct
that builds a fresh `TaffyTree` on every solve and drops it, so Taffy's
per-node cache is born empty and dies unused every frame. Taffy already
provides the cache (nine measure slots plus a final-layout entry per
node, keyed by the sizing inputs) and `mark_dirty(node)`, whose doc line
is "Marks the layout of this node and its ancestors as outdated". A clean
subtree with unchanged constraints then returns from cache without being
re-descended. The `LayoutSolver` seam already takes `&mut self` for
exactly this reason.

**Prune the readback.** Taffy stores layouts relative to the parent;
`LayoutSolver::solve` returns absolute rects. Converting one to the other
naively is an `O(n)` walk that would consume the win. The prune: if a
node's relative layout is unchanged and its parent's absolute origin is
unchanged, nothing in its subtree moved, so the subtree is skipped. This
is the only genuinely new layout logic required.

**Retain the interners.** The paint table _and_ the clip table are both
rebuilt from scratch every commit, with two fresh `HashMap`s and SipHash.
That is per-frame allocation and hashing that R-T4 excludes, and it is
also why the dirty diff must compare the resolved paint key and the
resolved clip region in addition to the entry bits: indices shift between
commits, so an unchanged index can mean a different entry. Retaining the
interners makes both index spaces stable, collapses the dirty check to a
bit compare, and lets a painter delta-upload the paint buffer too. The
tables then grow monotonically and will eventually want refcounting or
compaction — an accepted trade for a bounded scene with a bounded
palette.

Two contracts have to change to allow any of this:

- `LayoutSolver::solve` must be able to return only the rects that
  changed. `commit_with` currently panics if the solver omits a node
  ("solver returned no rect"), which mandates a full solve. The invariant
  is re-expressed rather than deleted: every node has a rect, from this
  solve or from the previous one.
- The back buffer stops being rebuilt from scratch and starts as a copy
  of the front buffer, patched at the changed indices. Copying ~1000
  24-byte entries is a few microseconds and is cache-friendly.

The dirty set also stops being _discovered_. Today `commit_with` diffs
every rect against the previous buffer to learn that one gauge moved.
With bindings, `dashlang` already knows which nodes it wrote; the dirty
set becomes the union of two `O(changed)` sets — nodes whose paint props
were written, and nodes whose solved rect actually moved, which the
retained solver reports.

### D6 — The dirty set crosses boundary B, advisory, with an oracle

`Painter::paint` does not receive the dirty set today, so R-T4 is not
implementable by any painter: `CommittedScene::dirty()` is produced by
core and consumed by nobody. The trait gains it as a slice, preserving
the slice-input shape that `painter-trait-infallible-slice-input` chose
(so `dashpaint` still does not depend on core):

    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
             images: &ImageTable, clips: &ClipTable, dirty: Option<&[u32]>);

`Option` rather than a bare slice: most existing call sites hand the
painter hand-built tables with no `CommittedScene` behind them, so `None`
states "the caller has no dirty information" instead of forcing them to
fabricate a full set.

**Advisory** is the contract: ignoring `dirty` and redrawing everything
is always correct, and a painter that honours it must produce identical
output to one that does not.

Damage-region partial redraw is explicitly **not** the goal. On a tiling
GPU, restoring the previous framebuffer into tile memory to repaint part
of it is the flush-and-resolve that R-T1 forbids. The GPU redraws every
quad in one pass; what must not be repeated is the CPU work and the
upload (R-T4).

`SkiaPainter` gains a second mode that models the **instance buffer**,
not the canvas: it keeps a persistent copy of the rect table, refreshes
only the entries the dirty set names, and then redraws every quad from
that copy. That is R-T4's data flow exactly, and it is _not_ a
damage-region redraw — no pixels are selectively preserved.

**Its purpose is not speed — it is a test oracle for the dirty set.** A
missing entry in the dirty set shows up on the GPU as a stale
instance-buffer entry (a frozen gauge, an indicator that will not clear),
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
commit and their indices are unstable. When D5 retains the interners and
those indices become stable, the oracle must be extended to retain the
paint table too.

### D7 — Visible is a layout prop; Opacity is a paint prop. There is no third state

`Prop` has neither today. Both are needed, and the line between them is
not "visible versus invisible" — it is **which side of boundary B
consumes it**.

    Visible(bool)   layout participation. Lowers to Taffy Display::None.
                    false = not drawn AND out of layout; siblings reflow.
                    LAYOUT-AFFECTING: triggers a re-solve.

    Opacity(f32)    node/group alpha. Never reaches Taffy.
                    PAINT-ONLY: no solve.

**Taffy has exactly one lever.** Its `Display` enum is
`Block | Flex | Grid | None`, and `None` is documented as "The node is
hidden, and it's children will also be hidden". There is no visibility
concept anywhere in the crate. From a layout engine's point of view an
unpainted node is simply a normal node with a normal box — whether anyone
paints it is not its business. So the split falls exactly on P2: `Visible`
is the solver's, `Opacity` is the painter's.

**CSS's `visibility: hidden` is dropped.** CSS distinguishes it from
`opacity: 0` for three reasons, none of which has a referent here:
inheritance and descendant override (dashscene has no cascade),
hit-testing (input and hit-testing appear nowhere in `DESIGN_1.md` or
`SCOPE_DECISIONS.md` — input belongs to the host), and stacking contexts.
Without those, `visibility: hidden` and `opacity: 0` are the same thing:
occupies space, draws nothing. A third state that is a synonym for an
existing one is not a state. If hit-testing ever enters scope, the
distinction can be added then without breaking anything.

Figma, Unity, and Android all have exactly these two concepts (`visible`
plus opacity; `SetActive` plus `CanvasGroup.alpha`; `GONE` plus `alpha`).
CSS is the outlier. This is not a Figma-compatibility concession — it is
where the engines already are, and P5 is satisfied. The bonus is that
Figma's `visible: false` imports 1:1 with **no lowering**.

Two details:

- **Name it `Visible`, not `Display`.** `LayoutMode::None` already exists
  and means "passthrough — children place by their authored offsets".
  Two `None`s with opposite meanings would be a durable source of bugs.
  There is no collision underneath: the engine lowers `LayoutMode::None`
  to `Display::Flex` with `Position::Absolute` children, so
  `Display::None` is currently unused.
- **Group opacity is not free**, and DESIGN §10.1 already rules on it:
  "group opacity (compiler detects non-overlapping children → per-node
  opacity free; overlapping → budgeted RT)", with Q-6 open on the budget
  value. An overlapping subtree at 0 < α < 1 needs an offscreen composite,
  which is the mid-frame render-target switch R-T1 restricts. α = 0 needs
  no compositing at all — the subtree is simply not drawn.

`Opacity` lands with the v0.8 paint work per §10.1. `Visible` is needed at
v0.4 for bounded pools. In between, "hide but keep the space" for a _leaf_
node is already expressible as a fill with `Color { a: 0.0 }`, which is
per-node alpha and needs no group composite. None of the acceptance cases
below need it for a whole subtree.

### D8 — A binding is a document construct. Transforms must be declarative

D1 puts the tables in `dashlang`. That is correct while the Rust DSL is
the only producer of bindings. It stops being correct the moment a
designer can declare a binding in Figma (D9), because then the importer
emits bindings, the importer's output _is_ the document, and a document
construct must live in `dashbuf` and core.

So the home is staged, and the trigger is explicit:

- **Now (v0.4)** — signals, bindings, and transforms in `dashlang`. Core
  unchanged. This ships the Rust path and proves the machinery.
- **When the importer emits bindings, or the validator must prove
  containment (both v0.7)** — the signal-declaration table and the
  binding table move into `dashbuf`'s schema and core's arena.
  `dashlang` becomes one producer of bindings and `dashc` another, which
  is what would make issue #48 ("same screen both ways, bit-identical")
  mean something.
- **Always** — signal _values_ stay producer-side. They are results, and
  P1 keeps results out of the document.

**This constrains the record shape now, not later.** A Rust closure does
not serialize. If a designer will ever author a binding, the transform
must be a bounded declarative vocabulary:

    enum Transform {
        Identity,
        Scale(f32),
        MapRange { in_lo: f32, in_hi: f32, out_lo: f32, out_hi: f32 },
        Clamp { lo: f32, hi: f32 },
        Format(FormatSpec),
        Custom(ClosureId),   // dashlang-only; never serializes
    }

Arbitrary Rust closures stay available to the DSL through `Custom`, so no
ergonomics are lost. Everything a designer can express lives in the
declarative subset, the validator can reason about that subset, and
compiling a `Custom` binding to `.dsb` is a **named diagnostic** rather
than a silent drop — which is P4 working as intended.

Designing this in later would be a redesign. Designing it in now costs
one enum.

### D9 — Figma authors bindings through Variables, with the plugin as the sidecar

The mechanism exists and needs no new plumbing.

**Variables are the authoring surface.** Figma Variables are typed —
`COLOR`, `FLOAT`, `STRING`, `BOOLEAN` — and their bindable scopes line up
with the channel set: `WIDTH_HEIGHT`, `CORNER_RADIUS`, `OPACITY`,
`STROKE_FLOAT` (FLOAT); `ALL_FILLS` / `FRAME_FILL` / `SHAPE_FILL` /
`TEXT_FILL` / `STROKE_COLOR` (COLOR); `FONT_SIZE`, `FONT_WEIGHT`,
`LINE_HEIGHT`, `LETTER_SPACING` (FLOAT); STRING variables to text
content. A designer binds with Figma's own UI and never learns a
dashscene concept.

- **Variants**: confirmed — the plugin docs show a STRING variable bound
  to the variant property of an `InstanceNode`, via `setProperties()` and
  `figma.variables.createVariableAlias()`. That is `bind_variant`.
- **Visibility**: through a BOOLEAN **component property** that toggles a
  nested layer, which is the native Figma idiom. Direct binding of a
  variable to a node's `visible` field is not documented and is not
  assumed here.

**The Enterprise gate is on REST, not on the plugin.** The Variables REST
API (`GET /v1/files/:key/variables/local`) requires "a Full seat in an
Enterprise org" with scope `file_variables:read`, which is the gate
DESIGN §6.1 already records. `GET /file` returns `boundVariables` **IDs**
on any paid plan, but not variable names. The Plugin API, however, runs
_inside the file_ — so the existing annotator plugin
(`importers/figma/plugin/code.ts`, namespace `dashscene`, key `role`) can
export the variable ID → name map into `sharedPluginData`, which the
importer already fetches via `?plugin_data=shared`. The Enterprise
endpoint is never touched. This is the same sidecar shape DESIGN §6.1
describes for design tokens, with the plugin supplying the sidecar
instead of the gated endpoint.

**Typing is ours, not Figma's.** `setSharedPluginData` accepts **string
values only** ("encode it as a JSON string first via JSON.stringify"). So
a binding annotation is a JSON string whose schema the importer
validates, emitting a named diagnostic on a mismatch (P4). This is
already exactly how `role` works: a string constrained to four values,
validated in `importers/figma/src/trim.ts`.

**To verify before building on this**: whether the Plugin API's
`figma.variables.getLocalVariablesAsync()` is itself plan-gated. Figma
limits variable _modes_ by tier and the docs do not say whether reading
local variables from a plugin is restricted on lower plans. That single
fact decides whether the Variables path works outside Enterprise. It is
cheap to test against the fixture files in `corpus/figma-fixtures/`.

## Acceptance cases

Named by the mechanism they exercise, not by a product domain — the IR is
general, and a consumer's domain should not define the acceptance
criteria any more than a producer's limitations define the format (P5).
Between them the four span the axes that matter: scalar versus discrete
channels, contained versus propagating writes, and high versus low
frequency.

**A1 — a contained, high-frequency scalar write.** A bound scalar channel
changes every frame, and every ancestor to the root is fixed or fill, so
the write cannot escape its own subtree. Must skip the layout solve
entirely; cost must be independent of scene size.
_Instances_: an instrument gauge, a progress bar filling, an audio level
meter, a video scrubber head, a live sparkline cursor.
_Guards against_: a per-frame write that silently triggers a full solve
because it sits under a hug ancestor — the case the containment check
(below) exists to name.

**A2 — a contained, high-frequency discrete write.** A text string changes
every frame inside a fixed-size box. Must be paint-only: no solve, one
dirty rect.
_Instances_: a clock, a counter, a status label, a numeric readout.
_Guards against_: treating every discrete prop as layout-affecting. Text
_can_ affect layout when its node hugs; inside a fixed box it must not.

**A3 — a low-frequency discrete switch with sibling reflow.** A variant
index or a `Visible` flag flips, a component appears or disappears, and
its siblings move to make room. Rare, so a real re-solve is affordable;
the layout delta is what FLIP animates.
_Instances_: a screen-variant transition, a sidebar collapsing, a banner
appearing above content and pushing it down, an inline validation error
displacing a submit button, a media player switching between mini and
expanded.
_Guards against_: assuming the dirty set comes from the bindings. The
reflow moves nodes nobody wrote to, so those rects must come from the
solver.

**A4 — a bounded pool whose members appear independently, inside a
container that hugs its content.** Members of a fixed-capacity pool toggle
`Visible` one at a time; the container grows and collapses around them.
_Instances_: an indicator/telltale stack, a toast stack, a form's list of
validation errors, a chip list, a download queue.
_Guards against_: the temptation to implement real insertion and removal.
D3 exists to say no; this stays a `Visible` write into a pre-materialized
pool, which is what keeps DFS indices stable and the frame budget
provable.

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
            node("indicator")
                .visible_when(warning),
        ]),
    ]);

    let mut live = scene.build(&mut arena);   // ids and PropKeys resolved here

    // per frame
    live.set(speed, 87.5);
    live.tick(dt, &mut arena);

Bindings are declared **on the node**, which is where the information
belongs and where a designer annotates it in Figma (D9). `build` assigns
`NodeId`s and resolves each declared binding into a `PropKey`, so a
producer never handles a `NodeId` and cannot hold a stale one. Components
stay plain functions returning `Node`, as they are today.

`Scene::build` keeps taking `&mut Arena` rather than owning it, so
`dashlang`'s existing rule — the DSL is a producer, not an owner —
survives, and several producers can still build into one arena.

Signals cannot be free-standing values. A signal needs identity, two nodes
must be able to bind the same one, and identity without an owner requires
a global or thread-local allocator, which is the ambient state this design
rejects. The builder is that owner.

## Containment is provable, so R4 becomes a check

A prop write's cost is set by how far it propagates upward. A node's size
change escapes its own subtree only if an ancestor is content-sized (hug).
If every ancestor to the root is fixed or fill, the change is contained,
and the retained solver re-solves that subtree and nothing else.

That property is visible in the document before anything runs. Once the
binding table is in core (D8), the validator can report: _this channel is
bound, and it sits under N hug ancestors, so a write to it reflows M
nodes_. A 60 Hz binding under a hug ancestor becomes a named diagnostic.
This is P4 applied to performance, and it is how "frame budget provable
from the document" (R4) becomes something a machine proves rather than a
claim in a specification.

The same check names the `Visible`-versus-`Opacity` choice (D7): a
designer hiding a layer triggers a reflow, while setting its alpha to zero
does not, and the two look identical in Figma.

## The importer's trim rules are load-bearing for performance

Figma node count is not live node count, and the gap is where the frame
budget is won. A design file of ~5000 nodes plausibly lands at a few
hundred to ~1500 live nodes, through three mechanisms DESIGN already
specifies:

- **Trim layers** (§6.1) — root scoping, `_`-prefixed names as trim sugar,
  slot children auto-replaced (slot content in Figma is sample content by
  definition), and `sharedPluginData` roles as machine truth. This removes
  wrapper groups, redlines, spec annotations, and sample content.
- **Heavy decor bakes.** The `.dsb` format already reserves cold,
  page-aligned sections at the file tail for it. A 200-path vector
  illustration is one paint, not 200 nodes.
- **Placeholder stubs** (§10.2) — a `role=placeholder` component compiles
  to a stub with a declared box, its fallback subtree in a lazily-loaded
  fragment.

The consequence worth recording: **the trim rules are a performance
contract, not tidiness.** Only the live count is charged to the frame
budget, and R4's provability depends on it.

## Data structures

The codebase is already data-oriented, and needs no exotic crates. The
arena is a `Vec<NodeData>` addressed by `u32`; `CommittedScene` is a flat
`Vec<RectEntry>` in DFS order, so painting is a linear scan and siblings
are contiguous. The reactive tables must match that shape:
`Vec<SignalSlot>`, `Vec<Binding>`, `u32` indices, and a flush that is a
linear walk over a dirty list. No object graph.

In priority order:

1. **Remove the `HashMap`s from the commit path** (D5). Where a map
   survives, use `rustc-hash`'s `FxHashMap` — std's SipHash is
   DoS-resistant and slow, and an internal interner keyed by colour bits
   needs neither property. This is the one crate worth adding.
2. **Keep the dirty set a sparse `Vec<u32>` of indices.** A bitset only
   wins when most of the scene is dirty, which is the opposite of the
   target.
3. **A hot/cold split on `NodeData` is plausible but must follow a
   profile.** Each node currently inlines `Option<String>` name and text
   next to its `Layout`, so the solver walks a struct fattened by fields
   it never reads. Structure-of-arrays is the classic fix and is where a
   real cache win would come from at 1000+ nodes — but it is invasive, and
   nothing here justifies it before measurement.

## Alternatives considered

- **A separate `dashbind` crate.** Rejected: it sits on no boundary, and
  `dashlang` would depend on it anyway (D1).
- **Signal and binding tables in `dashscene-core` from the start.**
  Rejected for now: a signal value is a result, not intent; core holds no
  producer machinery; and it forces a contortion to keep core away from
  `dashcue`. The binding table alone moves to core when D8's trigger
  fires.
- **Implicit dependency tracking (Solid / leptos).** Rejected: it inverts
  P4 and forfeits the containment proof (D2).
- **A general reactive crate (`leptos_reactive`, `futures-signals`).**
  Rejected: they are `Rc`/`RefCell` object graphs with per-node
  allocation, which would import pointer-chasing into a flat,
  index-addressed arena.
- **Modelling "this pushes that" as a signal edge.** Rejected: it
  re-implements layout in the binding table and violates P2 (see
  Non-goals).
- **True insertion and removal with a keyed reconciler.** Rejected: it
  needs node removal and id recycling that core has never had, it
  allocates in the update path, it defeats the index-keyed dirty diff
  (D3), and it makes R4 unprovable. It would be reconsidered only for a
  genuinely unbounded rendered list, which this domain does not appear to
  have.
- **Immediate-mode rebuild and diff (the React model).** Rejected:
  `O(total nodes)` in the producer every frame with allocation, which is
  the cost fine-grained bindings exist to remove.
- **A CSS-style third `visibility: hidden` state.** Rejected: without a
  cascade or hit-testing it is a synonym for `Opacity(0.0)` (D7).
- **Damage-region partial redraw.** Rejected: slower on a tiling GPU, and
  contradicts R-T1 (D6).
- **Closure-only transforms.** Rejected: forecloses Figma-authored
  bindings, which would surface as a redesign at v0.7 (D8).

## Prerequisites and sequencing

Not yet built, and required:

1. **`set_variant` and the variant table** — story #20. `bind_variant` has
   no target without it, and #20's API shape (flat variant index versus
   axis-keyed selection) is still undecided.
2. **`Prop::Visible`** (D7), lowering to Taffy `Display::None`.
3. **The `LayoutSolver` partial-solve contract** (D5).

Independently valuable, and buildable before the reactive layer:

- The dirty set across boundary B and the dual-mode Skia oracle (D6).
  Planned in `docs/wip/2026-07-13-dirty-set-boundary-b-plan.md`.
- The retained Taffy tree and pruned readback (D5).
- The retained paint and clip interners (D5).

D6 should land **first**: it builds the differential oracle, and D5 is
precisely the change that makes the dirty set derived rather than
discovered. Build the net before the fall.

## Impact on queued work

- **#118 (`dashlang` flex builder vocabulary + `Scene::build_with`)** —
  this design adds binding vocabulary to the same `Node` builder. Doing
  both in one pass avoids reshaping the builder twice. #118 should be
  re-scoped or explicitly sequenced ahead of this work.
- **#20 (variant table + `set_variant`)** — gains a second consumer. Its
  undecided API shape should be settled with `bind_variant` in view.
- **#130 (core's `Prop` cannot express the v0.3 paint vocabulary)** — the
  `Channel` enum used by bindings and by `dashcue`'s `PropKey` is the same
  address space as `Prop`. Widening one should widen the other.

## Open questions

- Does `Channel` become the canonical prop id, superseding the ad-hoc
  `Prop` enum, or sit alongside it? The `id-model` record already reserves
  "property (`set_prop`) — field name — u16 wire id — the schema (future)".
- Is the Plugin API's variable access plan-gated (D9)? This decides
  whether the Variables path works outside Enterprise.
- Do the retained interners need refcounting, or is monotonic growth with
  an occasional compaction sufficient for a bounded scene?
- Q-6 (DESIGN §11) — the group-opacity render-target budget on target
  hardware. `Opacity` inherits it.
