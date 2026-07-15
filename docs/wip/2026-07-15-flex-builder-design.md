# dashlang flex builder + Scene::build_with(solver) — design

    status   draft (brainstorm output, 2026-07-15)
    scope    dashlang (Node builder, Scene::build/build_with)
    slice    v0.8 — fidelity (issue #118)

## Problem

`dashlang`'s builder exposes only `at`/`size`/`fill`/`child`, and
`Scene::build` commits through `commit()` (the fixed solver, which
ignores flex), so no flex scene can be authored in the DSL today. The
v0.2 flex goldens (`goldens/tooling/tests/v02_flex.rs`) author their four
scenes directly against `dashscene-core`'s `Txn` and solve with
`TaffySolver`, bypassing `dashlang` entirely, and say so in their own
module doc.

A re-scope comment on issue #118 (2026-07-13, "SCOPE §23") adds a second
problem: `Scene::build` returns only a `u64` generation, so no `NodeId`
escapes and no producer can mutate a scene it built. Issue #166 (the
reactive-bindings layer, `docs/wip/2026-07-13-reactive-bindings-spec.md`)
resolves node identity by declaring bindings on the node and resolving
them to a `PropKey` inside `build`, so a producer never handles a
`NodeId` at all — but that only works if `build`'s return type is
something #166 can extend. Landing the flex vocabulary and the return
type in the same pass avoids reshaping the `Node`/`Scene` builder twice.

## Decisions

### D1 — Flex fields live in an embedded `Layout`, not twelve new fields

`Node` currently stores `x`, `y`, `width`, `height` as separate `f32`s.
`dashscene_core::Layout` already unions those four fields with every
v0.2 flex field (`mode`, `gap`, `padding`, `margin`, `main_align`,
`cross_align`, `sizing_h`, `sizing_v`, and four `Option<f32>` min/max
fields), is `pub`, `Copy`, and `Default`. `Node` embeds one
`layout: Layout` field instead of duplicating those fields by hand;
`at`/`size` become thin field writes into it. This is the most literal
form of "the DSL adds vocabulary, never semantics" — it stores exactly
core's own struct.

### D2 — One builder method per `Prop` variant, except where a pairing already exists

New chainable `Node` setters: `mode`, `gap`, `padding(left, top, right,
bottom)`, `margin(left, top, right, bottom)`, `main_align`,
`cross_align`, `sizing_h`, `sizing_v`, `min_width`, `max_width`,
`min_height`, `max_height`.

`padding`/`margin` bundle all four edges in one call because
`Prop::Padding`/`Prop::Margin` already do — this mirrors `Prop`'s own
grain, it is not a new DSL-level pairing. `sizing_h`/`sizing_v` and
`main_align`/`cross_align` stay separate rather than bundling into
`sizing(h, v)` / `align(main, cross)`: the v0.2 goldens set them
independently as often as together (e.g. a `Hug` node sets only
`SizingH`), so a combined call would force every caller to restate an
axis it does not want to touch. This differs from `at(x, y)`/`size(w,
h)`, which bundle two `Prop` variants that are always meaningful
together (an offset, a size) — that precedent does not extend to axes
that the goldens demonstrate are set independently.

`min_width`/`max_width`/`min_height`/`max_height` stay conditionally
emitted (`Option<f32>` on `Node`, only `set_prop`'d when `Some`),
matching `Fill`'s existing no-clear precedent — core's min/max props
can set a bound but never clear one back to unconstrained
(`docs/decisions/flex-vocabulary-shape.md`).

`LayoutMode`, `AxisSizing`, `MainAxisAlign`, `CrossAxisAlign` join the
`pub use dashscene_core::{...}` re-export list, keeping `dashlang` a
one-import-path surface.

### D3 — `build`/`build_with` return `Built`, not a bare `u64`

Both `Scene::build` and the new `Scene::build_with(&self, arena: &mut
Arena, solver: &mut dyn LayoutSolver)` return a new struct:

    pub struct Built { generation: u64 }
    impl Built { pub fn generation(self) -> u64 { self.generation } }

This is the "how NodeId survives the build" answer SCOPE §23 asks for
— but not the whole mechanism. `dashscene_core::NodeId` is already
private outside `dashlang::add()`'s internal use (it never appears in
`dashlang`'s public API today), and `CommittedScene` already carries
`node_of`/`rect_index_of`. `add()` already holds the concrete `NodeId`
at exactly the moment a future binding declaration would need to
resolve it into a `PropKey` — #166 can do that resolution entirely
inside `add()` without a producer ever touching a `NodeId`. What
actually "constrains the builder's signature" is the _return type_:
a bare `u64` cannot grow `.set()`/`.tick()` methods without breaking
every call site, so #166 would have to reshape `build` a second time.
`Built` is deliberately minimal — no `PropKey`, no signal table, no
`LiveScene` name — because #118 does not implement bindings; #166
extends `Built` (by adding fields/methods, or wrapping it) when it
lands that machinery.

Blast radius: `crates/dashlang/tests/builder.rs` (five call sites) and
the crate-doc example in `lib.rs`, each changing `generation == N` to
`generation.generation() == N`.

### D4 — Ported goldens live in `goldens/tooling/tests/`, not `crates/dashlang/tests/`

The acceptance criterion requires `cargo tree -p dashlang` to show core,
not engine. `goldens/tooling` (package name `goldens`) is the only
package in the workspace that already dev-depends on both `dashlang`
and `dashscene-engine` — `crates/dashlang/Cargo.toml` gains no new
dependency, dev or otherwise. A new file,
`goldens/tooling/tests/v02_flex_dsl.rs`, builds each of `v02_flex.rs`'s
four scenes twice — once via the DSL through `build_with`, once by hand
against `Txn` as `v02_flex.rs` already does — and asserts identical
rects, the same DSL-equals-hand-built pattern
`crates/dashlang/tests/builder.rs` already established for v0.1. It
does not re-compare golden PNGs: the goal is proving DSL output matches
hand-built output, which the existing golden image already covers once
for the hand-built side.

The issue body says "port `v02_flex.rs`'s four scenes"; all four are
ported (nesting, sizing, clamping, alignment).

### D5 — `lower_negative_gaps` stays out of scope

`build`/`build_with` do not call `Txn::lower_negative_gaps`
automatically. None of the four ported scenes use a negative gap, the
issue does not ask for it, and Taffy already solves an un-lowered
negative gap identically to the lowered form
(`docs/decisions/negative-gap-lowering.md`, "Known property") — the
lowering exists to keep a _serialized document_ CSS-representable, and
`dashlang` never serializes to `.dsb`. A producer that wants the
lowering can still call `txn.lower_negative_gaps()` directly against
the arena `dashlang` built into, same as today.

## Acceptance criteria

- The four `v02_flex.rs` scenes (nesting, sizing, clamping, alignment),
  authored through the DSL and solved via injected `TaffySolver`,
  produce the same rects as their existing hand-built `Txn` equivalents.
- `dashlang` still has no `dashscene-engine` dependency (`cargo tree -p
  dashlang` shows core, not engine).
- `just build` green.

## Alternatives considered

- **Keep `build` returning `u64`, let #166 reshape it later.** Rejected
  per the re-scope: this is exactly the "reshape twice" cost SCOPE §23
  flags, and the fix (wrapping one integer in a struct) is cheap enough
  now that deferring it buys nothing.
- **Resolve `PropKey`/bindings now, in #118.** Rejected: `PropKey`
  lives in `dashcue`, and #166's own design (D1,
  `docs/wip/2026-07-13-reactive-bindings-spec.md`) is what decides
  `dashlang` takes a `dashcue` dependency — pulling that forward into
  #118 would implement half of #166 against a design that has not been
  planned yet.
- **Name the return type `LiveScene` now.** Rejected: nothing is "live"
  without a signal table or a `tick`; naming it that now would promise
  behavior #118 does not deliver. `Built` is honest about what exists
  today and is #166's to rename or wrap.
- **Combine `sizing_h`/`sizing_v` into one `sizing(h, v)` call** (and
  `main_align`/`cross_align` into `align(main, cross)`), matching
  `at`/`size`. Rejected: the v0.2 goldens set these axes independently
  often enough that a combined call would force restating a default on
  every call site that only means to touch one axis.

## Impact on queued work

- **#166** (reactive bindings) — extends `Built` rather than reshaping
  `build`'s return type; the design doc's own "Impact on queued work"
  section already anticipated this.
- **#46** (DSL-generated stress corpus) — unblocked: it depends on the
  flex vocabulary this story adds.

## Trace

- Satisfies: issue #118, including the SCOPE §23 re-scope comment.
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
  (the build/commit shape this extends), `docs/decisions/
  flex-vocabulary-shape.md` (the core vocabulary this mirrors),
  `docs/decisions/negative-gap-lowering.md` D3 (the deferral this
  resolves).
- Related spec: `docs/wip/2026-07-13-reactive-bindings-spec.md` (the
  binding-vocabulary pass this sequences ahead of).
