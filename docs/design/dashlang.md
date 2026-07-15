# dashlang — minimal builder DSL (v0.1)

    crate    crates/dashlang
    covers   v0.1 walking skeleton (story #5), the v0.2 flex vocabulary
             (story #118), and the v0.4 reactive layer (story #166)

## Purpose

`dashlang` is the Rust DSL skin over `dashscene-core`'s staged-mutation
API (`docs/archive/2026-07-14-design-1-seed.md` §6.2,
`docs/decisions/staged-mutation-v01-scope.md`): it lets a scene be
written as a declaration — an inert value tree — instead of direct
`Arena`/`Txn` calls, then publishes that declaration in one commit.
Components are plain functions returning `Node` values
(`docs/archive/2026-07-14-design-1-seed.md` §6.2: "components are fns");
repetition is an iterator feeding
`Node::children` ("loops are repeaters").

## Value-tree surface

All types and functions live in `crates/dashlang/src/lib.rs`:

- `node(name: &str) -> Node` — a named node description; `anon() ->
  Node` for an unnamed one.
- `rgba(r, g, b, a: f32) -> Color` — a plain constructor for
  `dashscene_core::Color`. Both `Color` and `Arena` are re-exported
  (`pub use dashscene_core::{Arena, Color}`) so a DSL consumer needs
  one import path and no direct `dashscene-core` dependency.
- `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
  parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append
  one), `children(impl IntoIterator<Item = Node>)` (append from an
  iterator). Declaration order is document (DFS) order — core pins
  sibling order to creation order. The v0.2 flex vocabulary (issue
  #118): `mode(LayoutMode)`, `gap(f32)`, `padding(left, top, right,
  bottom: f32)`, `margin(left, top, right, bottom: f32)`,
  `main_align(MainAxisAlign)`, `cross_align(CrossAxisAlign)`,
  `sizing_h(AxisSizing)`, `sizing_v(AxisSizing)`, `min_width(f32)`,
  `max_width(f32)`, `min_height(f32)`, `max_height(f32)` — one method
  per `Prop` variant, mirroring `dashscene_core::Layout`, which `Node`
  embeds directly rather than duplicating its fields.
- `scene(roots: impl IntoIterator<Item = Node>) -> Scene` — collects a
  scene description from one or more root `Node`s, in order.

`Node` and `Scene` are inert: constructing and combining descriptions
stages nothing against an arena.

## Build/commit mapping

`Scene::build(&mut Arena) -> Built` and `Scene::build_with(&mut Arena,
&mut dyn LayoutSolver) -> Built` are the DSL's points of contact with
`dashscene-core`: both open one `Txn`, walk the value tree depth-first
(a private recursive `add` — `add_node` then `set_prop` for every
`Layout` field and, if set, `Fill`, then recurse into children), and
commit exactly once — `build` via `Txn::commit()` (the fixed solver,
flex intent ignored), `build_with` via `Txn::commit_with(solver)` (a
real solver, `dashscene-engine`'s `TaffySolver` being the product
case). Both _add_ their roots to whatever the arena already holds —
the DSL is a producer, not an owner — matching the one-commit model
the future C# describe-buffer skin will use across its FFI seam.

`Built` wraps the commit's generation (`Built::generation() -> u64`). It
is a named type rather than a bare `u64` so `build`/`build_with` have a
stable return type (`docs/decisions/dashlang-value-tree-builder.md`,
"Extension (issue #118)"). The v0.4 reactive layer (#166) did **not**
extend `Built`; `build` and `build_with` keep returning a plain
generation, and a live, bindable scene is a distinct entry point,
`Scene::build_live` returning a `LiveScene` — see "Reactive layer" below.
A live scene has to retain the solver, the scheduler, and the binding
tables, which a generation return cannot carry.

## Vocabulary, not semantics

Unset offset/size stay `0.0` and unset fill stays unfilled — exactly
`dashscene-core`'s own defaults. The DSL introduces no validation and
no defaults beyond core's: anything it can express is expressible by
hand against core, with identical committed output. The full rationale
and rejected alternatives (a closure/callback builder over a live
`Txn`; a `scene!{}` macro) are recorded in
`docs/decisions/dashlang-value-tree-builder.md` — not repeated here.

## Reactive layer (v0.4, story #166)

The reactive layer lets a producer drive a live scene at 60 Hz through one
commit per frame, without ever handling a `NodeId`. It lives entirely in
`crates/dashlang/src/reactive.rs`; `dashscene-core` is unchanged
(`docs/decisions/reactive-layer-home-and-staging.md`). The four load-bearing
decisions it realizes are recorded as decision records
(`reactive-layer-home-and-staging.md`, `bindings-are-explicit-and-flat.md`,
`scene-tree-is-static-lists-are-bounded-pools.md`,
`visible-is-layout-opacity-is-paint.md`); this section is the as-built
architecture.

**Authoring surface.** A producer declares signals on the `Scene`, binds
them on the `Node`, builds a `LiveScene`, and drives it per frame:

- `Scene::signal(initial) -> Signal<T>` — `T` is `f32` or `bool`. The
  builder owns the signal; identity without an owner would need an ambient
  allocator, which the design rejects.
- `Node::bind(Channel, expr)` / `Node::smooth(Channel, Spring)` /
  `Node::bind_text(expr)` / `Node::visible_when(Signal<bool>)` — bindings
  declared on the node, where a designer annotates them in Figma (v0.7).
- `Scene::roots(iter)` then `Scene::build_live(&mut Arena, Box<dyn
  LayoutSolver>) -> LiveScene` — assigns `NodeId`s, resolves each declared
  binding to its target, seeds each bound prop from its signal's initial
  value, commits once through the solver, and keeps the solver for reflows.
- `LiveScene::set(Signal, value)` marks the signal's bindings for the next
  flush (push-on-flush, never pull-on-paint, P3); `LiveScene::tick(dt, &mut
  Arena) -> u64` advances one frame.

**Channels and transforms.** A `Channel` is one scalar prop slot; at v0.4 it
is `X`/`Y`/`Width`/`Height` only — the paint channels (`Fill.r`, …) and
`Gap` the umbrella design also lists are deliberate follow-ups, not needed
by the v0.4 acceptance cases (text covers the paint-only path). A binding's
target is addressed by `dashcue`'s opaque `PropKey` (`node index ++ channel
code`), so `dashlang` and `dashcue` speak one `(PropKey, f32)` language
without depending on each other. The transform vocabulary is the declarative
`enum Transform` (`Identity`/`Scale`/`MapRange`/`Clamp`/`Format`/`Custom`);
`Custom(ClosureId)` holds a `dashlang`-only closure in a side table so the
enum itself stays serializable
(`docs/decisions/reactive-layer-home-and-staging.md`).

**The flush loop.** `LiveScene::tick` opens one `Txn`, then: flushes scalar
bindings whose signal changed (a smoothed one sets its spring's target, a
direct one writes now); flushes changed text bindings (`Prop::Text`, always
paint-only); flushes changed visibility bindings (`Prop::Visible`, always
layout-affecting); advances the `dashcue` scheduler and writes every live
track; and commits once. A frame that changed only paint props or only
contained scalars commits through retained geometry and never calls the real
solver; a frame that reflowed re-solves and refreshes the cache.

**The contained-write optimization (A1).** The live scene keeps the last
solved geometry (`cached_solve`, DFS order). A binding is `patchable` when
the node is ancestor-contained (every ancestor to the root is fixed or fill,
propagated through passthrough non-hug parents) **and** the write moves no
descendant (`write_is_single_rect`). A frame with only patchable writes
patches the cached rects and replays them through a private `CachedSolver`
fed to `commit_with`, so the real solver is never invoked and cost is
independent of scene size. The "no solve" decision lives entirely in
`dashlang`; core's `commit_with` is unchanged. A non-contained write, a
`Visible` flip, or a variant switch sets `layout_dirty` and forces the real
solve, after which the cache is refreshed from the committed buffer (DFS
order is invariant for a static tree, so the node-to-index map does not
change).

**Smoothing.** `Node::smooth(channel, Spring)` drives a bound channel
through `dashcue`'s `Scheduler`: the signal sets the spring's target, the
scheduler drives the value each frame, and a mid-flight retarget resumes
from the current sample (D4). `Spring` follows `dashcue`'s
stiffness + damping-ratio model, so a Compose `SpringSpec` maps onto it as
data. Variant-switch layout deltas animate through the engine's separate
FLIP path (`docs/design/dashscene-engine.md`), not through this per-prop
smoothing.

## Module layout

    crates/dashlang/src/lib.rs         crate docs + the value-tree DSL
                                       (node/anon/rgba/Node/scene/Scene)
    crates/dashlang/src/reactive.rs    the v0.4 reactive layer (#166):
                                       Signal/Channel/Transform/Spring,
                                       Node bind/smooth/bind_text/
                                       visible_when, Scene::build_live,
                                       LiveScene::set/tick
    crates/dashlang/tests/builder.rs   acceptance (issues #5, #118): DSL
                                       output == hand-built output;
                                       repeater children; multi-root;
                                       append to an existing arena;
                                       unset-value defaults; the flex
                                       vocabulary reaches the arena;
                                       build_with routes through an
                                       injected solver
    crates/dashlang/tests/reactive.rs  acceptance (issue #166): the A1-A4
                                       cases (contained scalar skips the
                                       solve; contained text is paint-only;
                                       a Visible flip reflows siblings; a
                                       bounded pool's hugging container
                                       collapses), plus a flex-container
                                       reflow, spring smoothing without
                                       solving, the declarative and format
                                       transforms, and one signal binding
                                       two nodes

The value-tree DSL is one file; the reactive layer is its own module
(`reactive.rs`) because it is a distinct subsystem — signal state,
resolved binding tables, the `dashcue` scheduler, and the flush loop.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §6.2 (Rust DSL
  skin) and §11 (the reactive layer); issue #5 acceptance criteria; issue
  #118 acceptance criteria (flex vocabulary, `build_with`, the SCOPE §23
  return-type seam); issue #166 acceptance criteria (signals, bindings,
  transforms, per-frame flush; `docs/archive/2026-07-14-scope-decisions.md`
  §23 D1-D4, D8).
- Blocks: #6 (golden harness, done); #46 (the DSL-generated stress
  corpus, unblocked by the flex vocabulary). The reactive layer (#166) is
  built; the v0.7 importer (#36) inherits the staged move of the binding
  table into `dashbuf` + core
  (`docs/decisions/reactive-layer-home-and-staging.md`).
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
  (this crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md`
  (the `open`/`set_prop`/`commit` API this crate consumes);
  `docs/decisions/flex-vocabulary-shape.md` (the core vocabulary this
  mirrors); `docs/decisions/negative-gap-lowering.md` D3 (the
  deferral #118 resolves); the four reactive-layer records
  (`reactive-layer-home-and-staging.md`,
  `bindings-are-explicit-and-flat.md`,
  `scene-tree-is-static-lists-are-bounded-pools.md`,
  `visible-is-layout-opacity-is-paint.md`).
