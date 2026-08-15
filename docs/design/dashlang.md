# dashlang — minimal builder DSL (v0.1)

    crate    crates/dashlang
    covers   v0.1 walking skeleton (story #5), the v0.2 flex vocabulary
             (story #118), the v0.4 reactive layer (story #166), the
             v0.7 document binding tables + loader-side attach (story #167),
             the v0.8 grid/wrap builder vocabulary + DSL-generated stress
             corpus (story #46), and the v0 paint vocabulary on `Node`
             (`paint.rs`), which let `corpus/showcase` collapse to one
             authoring pass

## Purpose

`dashlang` is the Rust DSL skin over `dashscene-core`'s staged-mutation API
(`docs/archive/2026-07-14-design-1-seed.md` §6.2,
`docs/decisions/staged-mutation-v01-scope.md`): it lets a scene be written as a
declaration — an inert value tree — instead of direct `Arena`/`Txn` calls, then
publishes that declaration in one commit. Components are plain functions
returning `Node` values (`docs/archive/2026-07-14-design-1-seed.md` §6.2:
"components are fns"); repetition is an iterator feeding `Node::children`
("loops are repeaters").

## Value-tree surface

All types and functions live in `crates/dashlang/src/lib.rs`:

- `node(name: &str) -> Node` — a named node description; `anon() ->
  Node` for
  an unnamed one.
- `rgba(r, g, b, a: f32) -> Color` — a plain constructor for
  `dashscene_core::Color`. Both `Color` and `Arena` are re-exported
  (`pub use dashscene_core::{Arena, Color}`) so a DSL consumer needs one import
  path and no direct `dashscene-core` dependency.
- `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
  parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append one),
  `children(impl IntoIterator<Item = Node>)` (append from an iterator).
  Declaration order is document (DFS) order — core pins sibling order to
  creation order. The v0.2 flex vocabulary (issue #118): `mode(LayoutMode)`,
  `gap(f32)`, `padding(left, top, right,
  bottom: f32)`,
  `margin(left, top, right, bottom: f32)`, `main_align(MainAxisAlign)`,
  `cross_align(CrossAxisAlign)`, `sizing_h(AxisSizing)`, `sizing_v(AxisSizing)`,
  `min_width(f32)`, `max_width(f32)`, `min_height(f32)`, `max_height(f32)` — one
  method per `Prop` variant, mirroring `dashscene_core::Layout`, which `Node`
  embeds directly rather than duplicating its fields. The v0.8 grid/wrap
  vocabulary (issue #46): `cross_gap(f32)`, `grid_row(u16)`, `grid_column(u16)`,
  `grid_row_span(u16)`, `grid_column_span(u16)` — more `Layout`-mirroring
  setters — plus
  `grid_rows`/`grid_columns(impl
  IntoIterator<Item = GridTrack>)`, whose track
  lists are held as separate `Node` fields because they are variable-length and
  `Layout` is `Copy` (the same split core makes). `GridTrack` is re-exported.
  Each new prop is staged only when authored, so a non-grid node reaches the
  arena exactly as it did before this vocabulary existed. Baseline
  cross-alignment needs no new setter — `cross_align(CrossAxisAlign::Baseline)`
  already reaches the arena. `visible(bool)` completes the set: it writes
  `layout.visible` like every other layout setter, and is layout vocabulary
  rather than paint (`docs/decisions/visible-is-layout-opacity-is-paint.md`)
  even though it landed with the paint vocabulary below. It stages
  `Prop::Visible` only when it differs from core's default, and it is the one
  layout prop that was reachable before only through the reactive
  `visible_when`. A node declaring both is not refused: `set_base_props` stages
  the static value first and `build_live` then seeds every bound prop from its
  signal's initial value, so the signal wins — the precedence every bound prop
  already has, now reachable for the first time and pinned by
  `crates/dashlang/tests/visible_precedence.rs`. Every one of core's 37 `Prop`
  variants now has a `Node` setter: 4 geometry, 20 layout, 13 paint.
- `scene(roots: impl IntoIterator<Item = Node>) -> Scene` — collects a scene
  description from one or more root `Node`s, in order.

`Node` and `Scene` are inert: constructing and combining descriptions stages
nothing against an arena.

## Paint surface

The paint vocabulary is a second set of `Node` setters, in
`crates/dashlang/src/paint.rs`. Twelve of them are **mirrors** — one setter per
`dashscene_core::Prop` paint variant, taking core's own type:

    corners_each(tl, tr, br, bl: f32)   Prop::Corners
    stroke(Stroke)                      Prop::Stroke
    fill_with(FillSpec)                 Prop::FillWith
    extra_fills(iter<FillSpec>)         Prop::ExtraFills
    opacity(f32)                        Prop::Opacity
    clip(bool)                          Prop::Clip
    mask(bool)                          Prop::Mask
    shadows(iter<Shadow>)               Prop::Shadows
    blurs(iter<Blur>)                   Prop::Blurs
    shape_field(VectorField)            Prop::ShapeField
    text(&str)                          Prop::Text
    text_style(TextStyle)               Prop::TextStyle

`fill(Color)` is the thirteenth and predates the rest: it is the solid
shorthand, the same split `Prop::Fill`/`Prop::FillWith` makes in core. Every
type these take is re-exported from `lib.rs`, so authoring a gradient or a
shadow needs no direct `dashpaint` dependency.

Four **sugar** methods expand to a mirror and add nothing else: `corners(r)` is
`corners_each` with the radius four times;
`drop_shadow(dx, dy, blur, spread, color)` and `inner_shadow(...)` are `shadows`
with one entry of the matching `ShadowKind`; `backdrop_blur(r)` is `blurs` with
one `BlurKind::Backdrop` entry. Each replaces the whole list, so a node needing
two shadows, or a mixed drop-and-inner list, calls the mirror. A `gradient(...)`
constructor is deliberately **not** among them: a gradient's geometry is three
handle points, and any sugar short of the full `Gradient` would have to invent
them — inventing defaults is the one thing "vocabulary, not semantics" forbids.
The showcase's own `gradient`/`diagonal_gradient` helpers are scene-local
conveniences over `FillSpec`, and they stay in the scene.

**Stages only when authored.** Each paint field on `Node` is `Option` or an
empty `Vec` until a setter writes it, and `stage_paint_props` emits a `Prop`
only for the ones that were. A node that sets none of them reaches the arena
with exactly the props it staged before this vocabulary existed — asserted by
`crates/dashlang/tests/paint.rs`'s unset-defaults case. The consequence to know:
`shadows([])` and `blurs([])` stage nothing, so neither clears a list the arena
already holds. Core has no clear operation for either, the same gap `fill` has.

**Image fills still need the arena.** `fill_with` takes any `FillSpec`,
including `FillSpec::Image`, but its `ImageFill.image` field is an index
`Txn::add_image` issues against an arena — and the value tree has no arena. So a
scene using an image fill builds the tree first and stages that one prop in a
short second pass (`corpus/showcase/README.md` describes the shape).

**`fill_with` and a fill-channel binding are mutually exclusive.** See "Refused
combinations" under the reactive layer below.

The module is separate from `lib.rs` for the same reason `reactive.rs` is: it is
a distinct subsystem — a whole prop family and its staging walk — not a second
`Node` type. All three files declare setters on the one `Node`, so the authoring
surface stays a single import path.

The vocabulary's shape, the sugar boundary, the re-export widening and the image
seam are recorded in `docs/decisions/dashlang-paint-vocabulary.md`; the charter
reading that admits sugar at all is the 2026-08-01 extension to
`docs/decisions/dashlang-value-tree-builder.md`.

## Build/commit mapping

`Scene::build(&mut Arena) -> Built` and
`Scene::build_with(&mut Arena,
&mut dyn LayoutSolver) -> Built` are the DSL's
points of contact with `dashscene-core`: both open one `Txn`, walk the value
tree depth-first (a private `add`, over an explicit stack rather than the call
stack — issue #79 — calling `add_node` then `set_base_props` for each node
before its children), and commit exactly once — `build` via `Txn::commit()` (the
fixed solver, flex intent ignored), `build_with` via `Txn::commit_with(solver)`
(a real solver, `dashscene-engine`'s `TaffySolver` being the product case). Both
_add_ their roots to whatever the arena already holds — the DSL is a producer,
not an owner — matching the one-commit model the future C# describe-buffer skin
will use across its FFI seam.

`set_base_props` is the one place a node's non-reactive props are staged: every
`Layout` field, the grid track lists and `Fill` when set, then
`paint::stage_paint_props` for the paint vocabulary. It is `pub(crate)` and
called from both walks — `add` here, and `stage_live` on the reactive path — so
the paint vocabulary reaches the arena through exactly one code path, and a
`Scene::build` node and a `Scene::build_live` node cannot stage different props.
Adding the vocabulary to one walk only was the drift this shape rules out.

### Roots do not compose, which is what makes a later root an overlay

`scene(roots)` and `Scene::roots(iter)` both take a list, and the roots in it
have **no layout relationship to each other**. Three properties follow, and
together they are the whole of how an overlay is authored:

- Each root is an independent coordinate island solved from its own origin
  (`docs/design/dashscene-engine.md`, "The solve"), so a second root honours its
  own `Node::at` offset and **overlaps** the first rather than stacking below
  it. Nothing in a root's geometry can move another root, in either direction.
- Roots stage and commit in declaration order
  (`docs/design/dashscene-core-arena.md`, "Commit resolution pipeline" step 1),
  so the **last** root's rects are last in the committed rect table, which is
  what draws it above every earlier root.
- A root added to a scene adds nothing to the scene's own layout, so content
  that must sit over a scene without disturbing it is authored as a later root
  and not as a child of the scene's tree.

`crates/dashlang/tests/builder.rs`'s `multiple_roots_keep_declaration_order`
pinned declaration order for `Scene::build` from v0.1, but nothing pinned what a
second root's geometry or paint order does, and no **live** scene had more than
one root before the painter badge (2026-08-04). The three properties above were
therefore measured rather than assumed. The worked example is
`corpus/showcase/src/badge.rs` — a pill naming the painter that drew the frame,
appended by each showcase scene as its second root — and
`corpus/showcase/tests/badge.rs` pins the placement, the root order and the
overlap.

`Built` wraps the commit's generation (`Built::generation() -> u64`). It is a
named type rather than a bare `u64` so `build`/`build_with` have a stable return
type (`docs/decisions/dashlang-value-tree-builder.md`, "Extension (issue
#118)"). The v0.4 reactive layer (#166) did **not** extend `Built`; `build` and
`build_with` keep returning a plain generation, and a live, bindable scene is a
distinct entry point, `Scene::build_live` returning a `LiveScene` — see
"Reactive layer" below. A live scene has to retain the solver, the scheduler,
and the binding tables, which a generation return cannot carry.

## Vocabulary, not semantics

Unset offset/size stay `0.0` and unset fill stays unfilled — exactly
`dashscene-core`'s own defaults. The DSL introduces no validation and no
defaults beyond core's: anything it can express is expressible by hand against
core, with identical committed output. The full rationale and rejected
alternatives (a closure/callback builder over a live `Txn`; a `scene!{}` macro)
are recorded in `docs/decisions/dashlang-value-tree-builder.md` — not repeated
here.

## Reactive layer (v0.4, story #166)

The reactive layer lets a producer drive a live scene at 60 Hz through one
commit per frame, without ever handling a `NodeId`. It lives in
`crates/dashlang/src/reactive.rs`; at v0.4 `dashscene-core` was unchanged, and
at v0.7 the staged move fired — the binding _table_ (never signal values) became
a document construct in `dashbuf` and core's arena, with this layer as one
producer of it (`docs/decisions/reactive-layer-home-and-staging.md`). The four
load-bearing decisions it realizes are recorded as decision records
(`reactive-layer-home-and-staging.md`, `bindings-are-explicit-and-flat.md`,
`scene-tree-is-static-lists-are-bounded-pools.md`,
`visible-is-layout-opacity-is-paint.md`); this section is the as-built
architecture.

**Authoring surface.** A producer declares signals on the `Scene`, binds them on
the `Node`, builds a `LiveScene`, and drives it per frame:

- `Scene::signal(initial) -> Signal<T>` — `T` is `f32` or `bool`. The builder
  owns the signal; identity without an owner would need an ambient allocator,
  which the design rejects.
- `Node::bind(Channel, expr)` / `Node::smooth(Channel, Spring)` /
  `Node::bind_text(expr)` / `Node::visible_when(Signal<bool>)` — bindings
  declared on the node, where a designer annotates them in Figma (v0.7).
- `Scene::roots(iter)` then
  `Scene::build_live(&mut Arena, Box<dyn
  LayoutSolver>) -> LiveScene` —
  assigns `NodeId`s, resolves each declared binding to its target, seeds each
  bound prop from its signal's initial value, commits once through the solver,
  and keeps the solver for reflows.
- `LiveScene::set(Signal, value)` marks the signal's bindings for the next flush
  (push-on-flush, never pull-on-paint, P3);
  `LiveScene::tick(dt, &mut
  Arena) -> u64` advances one frame.

**Channels and transforms.** A `Channel` is one scalar prop slot — since story
#167 it is `dashscene_core::Channel`, the document binding vocabulary,
re-exported: the full §23 set (`X`/`Y`/`Width`/`Height`, `Gap`, and the four
`Fill` channels — debt #201). A fill channel is paint-only and writes through a
per-node fill shadow (one channel writes one component of a four-component
color); `Gap` always solves. A binding's target is addressed by `dashcue`'s
opaque `PropKey`, packed by core's `dashscene_core::prop_key` — the one packing
and the one decoder everywhere, living beside `Channel` so no consumer needs the
engine to build a key (debt #208) — so `dashlang`, the engine, and `dashcue`
speak one `(PropKey, f32)` language. The transform vocabulary is the declarative
`enum Transform` (`Identity`/`Scale`/`MapRange`/`Clamp`/`Format`/`Custom`);
`Custom(ClosureId)` holds a `dashlang`-only closure in a side table so the enum
itself stays serializable (`docs/decisions/reactive-layer-home-and-staging.md`).

**Refused combinations.** Two authoring combinations are named build-time panics
in `stage_live` rather than silent losses (P4):

- A `smooth()` whose channel has no matching `bind()` — the spring would be
  silently inert, with no signal to take targets from (debt #194).
- A `fill_with(...)` on a node that also binds any `Fill*` channel. A fill
  channel drives one component of a solid color through the node's fill shadow,
  and every write it makes — including the seed `build_live` stages before the
  first frame — is a `Prop::Fill`, which replaces the node's whole fill slot.
  The authored gradient or image fill would be gone from the first committed
  frame with nothing reporting it. The shadow seeds from the solid `fill` only,
  because a gradient carries no four-component color for the channels to
  address, so there is no merge to perform and no defensible way to prefer one
  of the two authorings over the other
  (`docs/decisions/fill-with-refuses-a-fill-channel-binding.md`, which also
  records why the loader-side `attach_live` is deliberately not covered — debt
  #667).

**The document binding tables (v0.7, story #167).** `build_live` stages every
scalar signal (named via `Scene::signal_named`, or anonymous) and every
declarative scalar binding into the arena's binding tables
(`Txn::declare_signal`/`Txn::bind`), so a `dashlang` scene and a loaded `.dsb`
expose one table; `Custom` rows stay live-only (D8). The loader-side entry point
is `attach_live(arena, solver) -> LiveScene`: a document loaded by
`dashscene_core::load_document` attaches into a live scene whose signals are
addressable by their document names (`LiveScene::signal_named` — a Figma
variable's mode-qualified name), with the same write classification as authored
bindings (`docs/decisions/binding-table-in-the-document.md`).

**The flush loop.** `LiveScene::tick` opens one `Txn`, then: flushes scalar
bindings whose signal changed (a smoothed one sets its spring's target, a direct
one writes now); flushes changed text bindings (`Prop::Text`, always
paint-only); flushes changed visibility bindings (`Prop::Visible`, always
layout-affecting); advances the `dashcue` scheduler and writes every live track;
and commits once. A frame that changed only paint props or only contained
scalars commits through retained geometry and never calls the real solver; a
frame that reflowed re-solves and refreshes the cache.

**The contained-write optimization (A1).** The live scene keeps the last solved
geometry (`cached_solve`, DFS order). A binding is `patchable` when the node is
ancestor-contained (every ancestor to the root is fixed or fill, propagated
through passthrough non-hug parents) **and** the write moves no descendant
(`write_is_single_rect`). A frame with only patchable writes patches the cached
rects and replays them through a private `CachedSolver` fed to `commit_with`, so
the real solver's **solve** is never invoked and the layout cost is independent
of scene size. Since issue #621 the replay does forward the seam's other two
methods to that solver — see below — so text staging is not. The "no solve"
decision lives entirely in `dashlang`; core's `commit_with` is unchanged. A
non-contained write, a `Visible` flip, or a variant switch sets `layout_dirty`
and forces the real solve, after which the cache is refreshed from the committed
buffer (DFS order is invariant for a static tree, so the node-to-index map does
not change).

**The replay stages glyph runs, since issue #621.** `CachedSolver` borrows the
solver `LiveScene` retains and forwards `atlases` and `stage_text` to it, while
still answering `solve` from the patched cache. Commit rebuilds the glyph-run
table from what the solver it was handed stages
(`docs/decisions/glyph-runs-cross-boundary-b.md`), so forwarding is what keeps a
paint-only commit's text.

It used to take the trait's defaults for both, and a replaying commit then
published a run table with **no runs in it at all** — every text node in the
scene, not only the one that changed. Since `bind_text` and `Channel::Opacity`
are both paint-only, the frame that changed a string was the frame that erased
the scene's text. `docs/decisions/signal-driven-text-needs-a-solving-write.md`
carried an authoring rule that worked around it and is now retired.

**What that costs.** A paint-only tick re-stages text, at about 1.5 µs per text
node per commit with a warm shaping cache
(`docs/decisions/glyph-runs-cross-boundary-b.md`, "Per-frame cost, measured").
So A1's "independent of scene size" holds for layout and not for text. The
cheaper design — carrying runs forward inside `commit_with`, which re-stages
nothing — is recorded in the retired decision and is where a frame-budget
problem here should be answered.

**Smoothing.** `Node::smooth(channel, Spring)` drives a bound channel through
`dashcue`'s `Scheduler`: the signal sets the spring's target, the scheduler
drives the value each frame, and a mid-flight retarget resumes from the current
sample (D4). `Spring` follows `dashcue`'s stiffness + damping-ratio model, so a
Compose `SpringSpec` maps onto it as data. Variant-switch layout deltas animate
through the engine's separate FLIP path (`docs/design/dashscene-engine.md`), not
through this per-prop smoothing.

## Module layout

    crates/dashlang/src/lib.rs         crate docs + the value-tree DSL
                                       (node/anon/rgba/Node/scene/Scene),
                                       the Node paint fields, and
                                       set_base_props — the one staging
                                       walk both build paths call
    crates/dashlang/src/paint.rs       the paint vocabulary: 12 Prop
                                       mirrors and 4 sugar methods on
                                       Node, plus stage_paint_props,
                                       which stages only what was
                                       authored
    crates/dashlang/src/reactive.rs    the v0.4 reactive layer (#166):
                                       Signal/Channel/Transform/Spring,
                                       Node bind/smooth/bind_text/
                                       visible_when, Scene::build_live,
                                       LiveScene::set/tick; the v0.7
                                       additions (#167): signal_named,
                                       core-table staging, attach_live; the
                                       v0.17 frame policy (#810):
                                       MAX_FRAME_DELTA and the clamp tick
                                       applies, plus LiveScene::advanced
                                       and mark_shown, the generation gate
                                       both hosts used to hold privately;
                                       the renumbering gate (#945),
                                       take_renumbering, read by every loop
                                       that ticks a LiveScene
    crates/dashlang/tests/builder.rs   acceptance (issues #5, #118, #46):
                                       DSL output == hand-built output;
                                       repeater children; multi-root;
                                       append to an existing arena;
                                       unset-value defaults; the flex and
                                       the grid/wrap/baseline vocabulary
                                       reach the arena; build_with routes
                                       through an injected solver; and that
                                       the paint vocabulary is authorable
                                       through dashlang's own re-exports
                                       alone, with no dashpaint import
    crates/dashlang/tests/corpus.rs    the DSL-generated E3 stress corpus
                                       (issue #46): negative gap, hug-in-fill,
                                       wrap, grid spans, baseline, and a
                                       set_variant wrap-line topology change,
                                       plus the Vertical and min/max R2-coverage
                                       cases, each pinned to hand-computed rects
                                       by NodeId
    crates/dashlang/tests/reactive.rs  acceptance (issue #166): the A1-A4
                                       cases (contained scalar skips the
                                       solve; contained text is paint-only;
                                       a Visible flip reflows siblings; a
                                       bounded pool's hugging container
                                       collapses), plus a flex-container
                                       reflow, spring smoothing without
                                       solving, the declarative and format
                                       transforms, and one signal binding
                                       two nodes; plus the two refused
                                       combinations (an unbound smooth,
                                       and fill_with beside a fill-channel
                                       binding)
    crates/dashlang/tests/frame_policy.rs
                                       the frame policy tick owns (story
                                       #810): the delta clamp, including
                                       that an under-clamp delta passes
                                       through untouched and that a NaN
                                       one becomes zero rather than
                                       reaching dashcue's finite assert,
                                       and the generation gate, including
                                       that a rebuilt scene starts unshown.
                                       The **renumbering** gate's test
                                       (#945) is not here: it needs two
                                       bound roots to switch between, so
                                       it sits beside that fixture in
                                       reactive.rs as
                                       a_renumbering_is_reported_once_and_
                                       not_on_every_later_frame
    crates/dashlang/tests/paint.rs     acceptance for the paint
                                       vocabulary: each mirror reaches the
                                       arena, DSL output == hand-built
                                       output; each sugar method equals
                                       its mirror; clip(false)/mask(false)
                                       still stage; an unauthored node
                                       stages what it staged before the
                                       vocabulary existed
    crates/dashlang/tests/visible_precedence.rs
                                       a bound visible_when signal wins
                                       over a static visible(bool) on the
                                       same node — the collision the
                                       static setter made reachable

The value-tree DSL is one file; the paint vocabulary (`paint.rs`) and the
reactive layer (`reactive.rs`) are each their own module, because each is a
distinct subsystem — a prop family and its staging walk, and signal state with
the binding tables, the `dashcue` scheduler and the flush loop. Both declare
setters on the one `Node`, which stays in `lib.rs`.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §6.2 (Rust DSL skin) and
  §11 (the reactive layer); issue #5 acceptance criteria; issue #118 acceptance
  criteria (flex vocabulary, `build_with`, the SCOPE §23 return-type seam);
  issue #166 acceptance criteria (signals, bindings, transforms, per-frame
  flush; `docs/archive/2026-07-14-scope-decisions.md` §23 D1-D4, D8).
- Blocks: #6 (golden harness, done); #46 (the DSL-generated stress corpus, done
  — story #46 extended the builder with the v0.8 grid/wrap vocabulary and landed
  the corpus, `docs/decisions/dashlang-stress-corpus.md`; E3 is met — the
  variant child-count form landed via issue #283). The reactive layer (#166) is
  built, and the staged move of the binding table into `dashbuf` + core fired at
  story #167 (`docs/decisions/reactive-layer-home-and-staging.md`,
  `docs/decisions/binding-table-in-the-document.md`).
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md` (this
  crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md` (the
  `open`/`set_prop`/`commit` API this crate consumes);
  `docs/decisions/flex-vocabulary-shape.md` (the core vocabulary this mirrors);
  `docs/decisions/negative-gap-lowering.md` D3 (the deferral #118 resolves); the
  four reactive-layer records (`reactive-layer-home-and-staging.md`,
  `bindings-are-explicit-and-flat.md`,
  `scene-tree-is-static-lists-are-bounded-pools.md`,
  `visible-is-layout-opacity-is-paint.md`);
  `docs/decisions/dashlang-paint-vocabulary.md` (the v0 paint vocabulary this
  crate now carries, and the image index that bounds it);
  `docs/decisions/fill-with-refuses-a-fill-channel-binding.md` (the refusal that
  vocabulary made reachable);
  `docs/decisions/cross-arena-comparison-resolves-indices.md` (the rule the
  DSL-equals-hand-built assertions follow);
  `docs/decisions/signal-driven-text-needs-a-solving-write.md` (what the A1
  replay's missing text staging obliges of a scene author until it is fixed).
