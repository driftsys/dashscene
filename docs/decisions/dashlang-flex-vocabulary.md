# dashlang mirrors the v0.2 flex vocabulary by embedding core's own `Layout`

    status   accepted (story/issue #118, 2026-07-15)
    scope    crates/dashlang (Node builder), goldens/tooling/tests/v02_flex.rs;
             resolves the deferral recorded in
             docs/decisions/negative-gap-lowering.md D3 and
             docs/decisions/v02-flex-goldens-per-construct.md

## Context

`dashlang`'s v0.1 builder exposed only `at`/`size`/`fill`/`child`
(`docs/decisions/dashlang-value-tree-builder.md`), and `Scene::build`
committed through `commit()` (the fixed solver, which ignores flex), so
no flex scene could be authored in the DSL. The v0.2 flex goldens
(`goldens/tooling/tests/v02_flex.rs`) authored their four scenes
directly against `dashscene-core`'s `Txn` and solved with
`TaffySolver`, bypassing `dashlang` entirely, and said so in their own
module doc — story #11 confirmed the gap and filed #118 against it.

## D1 — Flex fields live in an embedded `Layout`, not twelve new fields

`Node` stored `x`, `y`, `width`, `height` as separate `f32`s.
`dashscene_core::Layout` already unions those four fields with every
v0.2 flex field (`mode`, `gap`, `padding`, `margin`, `main_align`,
`cross_align`, `sizing_h`, `sizing_v`, and four `Option<f32>` min/max
fields), is `pub`, `Copy`, and `Default`. `Node` embeds one
`layout: Layout` field instead of duplicating those fields by hand;
`at`/`size` become thin field writes into it. This is the most literal
form of "the DSL adds vocabulary, never semantics" — it stores exactly
core's own struct.

## D2 — One builder method per `Prop` variant, except where a pairing already exists

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
the goldens demonstrate are set independently.

`min_width`/`max_width`/`min_height`/`max_height` stay conditionally
emitted (`Option<f32>` on `Node`, only `set_prop`'d when `Some`),
matching `Fill`'s existing no-clear precedent — core's min/max props
can set a bound but never clear one back to unconstrained
(`docs/decisions/flex-vocabulary-shape.md`).

`LayoutMode`, `AxisSizing`, `MainAxisAlign`, `CrossAxisAlign` join the
`pub use dashscene_core::{...}` re-export list, keeping `dashlang` a
one-import-path surface.

## D3 — Ported goldens are added in place, not a parallel file

The acceptance criterion requires `cargo tree -p dashlang` to show core,
not engine — confirmed as-built (no `dashscene-engine` entry).
`goldens/tooling` (package name `goldens`) is the only package in the
workspace that already dev-depends on both `dashlang` and
`dashscene-engine`, so `crates/dashlang/Cargo.toml` gains no new
dependency, dev or otherwise.

Each of `v02_flex.rs`'s four existing tests (nesting, sizing, clamping,
alignment —
`docs/decisions/v02-flex-goldens-per-construct.md`) gained a second,
DSL-built `Arena` right after its hand-built one, plus
`assert_eq!(dsl.committed().rects(), arena.committed().rects())` and
(added during code review, after the initial port) the matching
`paints()` assertion — reusing the hand-built scene's own
already-asserted rects/paints as the DSL side's expected values, rather
than duplicating either the hand-built `Txn` code or its numeric
expectations in a parallel file. This is the same DSL-equals-hand-built
pattern `crates/dashlang/tests/builder.rs` already established for
v0.1, inlined where each construct's full verification (hand-built and
DSL-built) can live together. It does not re-compare golden PNGs: the
goal is proving DSL output matches hand-built output, which the
existing golden image already covers once for the hand-built side. The
module doc comment's "dashlang is not used" line was corrected in the
same file.

## D4 — `lower_negative_gaps` stays out of scope

`build`/`build_with` do not call `Txn::lower_negative_gaps`
automatically. None of the four ported scenes use a negative gap,
issue #118 did not ask for it, and Taffy already solves an un-lowered
negative gap identically to the lowered form
(`docs/decisions/negative-gap-lowering.md`, "Known property") — the
lowering exists to keep a _serialized document_ CSS-representable, and
`dashlang` never serializes to `.dsb`. A producer that wants the
lowering can still call `txn.lower_negative_gaps()` directly against
the arena `dashlang` built into, same as before #118.

This resolves the deferral `docs/decisions/negative-gap-lowering.md` D3
recorded: giving `dashlang` a flex builder vocabulary, and deciding how
a `dashlang` scene reaches the engine solver, was explicitly left out
of that story's scope and filed as #118. #118 is that vocabulary and
that solver entry point (`Scene::build_with`); the negative-gap lowering
itself remains a separate, explicit producer step.

## Alternatives considered

- **Combine `sizing_h`/`sizing_v` into one `sizing(h, v)` call** (and
  `main_align`/`cross_align` into `align(main, cross)`), matching
  `at`/`size`. Rejected: the v0.2 goldens set these axes independently
  often enough that a combined call would force restating a default on
  every call site that only means to touch one axis.

## Trace

- Satisfies: issue #118 acceptance criteria (flex vocabulary on `Node`,
  the ported goldens, no new `dashlang` dependency).
- Unblocks: #46 (the DSL-generated stress corpus), which depends on the
  flex vocabulary this record adds.
- Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
  (this crate's original builder shape; separately extended by #118's
  `Built` return-type change, recorded there); `docs/decisions/
  flex-vocabulary-shape.md` (the core vocabulary this mirrors);
  `docs/decisions/negative-gap-lowering.md` D3 (the deferral this
  resolves); `docs/decisions/v02-flex-goldens-per-construct.md` (the
  per-construct golden convention these ported assertions extend).
