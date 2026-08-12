# The reactive layer lives in dashlang; a binding's home moves to core at v0.7

    status   accepted (story #166, 2026-07-15)
    scope    crates/dashlang (signals, bindings, transforms, the flush loop);
             the v0.7 importer (#36) and validator, which inherit the staged move
             into dashbuf + dashscene-core

## Context

A fine-grained reactive layer — signals a producer pushes into, bindings that
carry a signal's value to a node prop, and a once-per-frame flush — had to live
somewhere. Two questions were conflated in an earlier draft and have different
answers: should the reactive layer be its own crate, and should its tables live
in `dashscene-core`? (`docs/archive/2026-07-14-scope-decisions.md` §23 D1, D8;
`docs/archive/2026-07-14-design-1-seed.md` §11.)

A signal's _value_ is transient producer state. P1 keeps _results_ out of the
document, and a signal value is a result, not intent. A _binding_ — "this node's
width follows signal 3" — is intent, and becomes a document construct the moment
a designer can author it in Figma (v0.7, #36), at which point the importer emits
it and its schema must live in `dashbuf`.

## Options

1. A separate `dashbind` crate.
2. Signal and binding tables in `dashscene-core` from the start.
3. The whole layer in `dashlang` now, with the binding table (not the signal
   value) staged to move into `dashbuf` + core at v0.7.

## Choice

Option 3.

- **`dashlang`** holds the signal table, the binding table, the transform
  vocabulary, and the flush loop (`crates/dashlang/src/reactive.rs`).
  `dashscene-core` is unchanged by the reactive layer — `LiveScene::tick` opens
  a `Txn`, flushes dirty bindings through `set_prop`, advances the `dashcue`
  scheduler, and commits, all through mechanisms core already has.
- The transform vocabulary is a bounded, declarative `enum Transform`
  (`Identity`, `Scale`, `MapRange`, `Clamp`, `Format`, `Custom(ClosureId)`) from
  the start, because a Rust closure does not serialize. Everything a designer
  can express lives in the non-`Custom` subset; `Custom` is a `dashlang`-only
  escape hatch that keeps the arbitrary closure out of the serializable table,
  so compiling a `Custom` binding to `.dsb` is a named diagnostic (P4) rather
  than a silent drop.
- **At v0.7**, when the importer emits bindings or the validator must prove
  containment, the signal-declaration table and the binding table move into
  `dashbuf`'s schema and core's arena; `dashlang` becomes one producer of
  bindings and `dashc` another. Signal _values_ stay producer-side always.

## Why

- A crate here exists to make a boundary mechanical: `dashpaint` _is_ boundary
  B, so a painter physically cannot reach the arena (P2); `dashcue` depends on
  nothing, so the scheduler physically cannot reach producer state (P3). A
  reactive layer sits on no such boundary, and `dashlang` would depend on
  `dashbind` anyway. Option 1 adds a wall with nothing on either side of it.
- Putting the tables in core (option 2) makes core own producer-side runtime
  state — a reactive graph, a dirty-binding list, a flush loop — which cuts
  against P3's producer/runtime split. It also forces a contortion to keep core
  away from `dashcue` (an opaque smoothing handle core must never interpret).
  `dashlang` depends on both core and `dashcue`, so core never comes near the
  animation crate and SCOPE §9 holds by construction. The binding table alone
  moves to core when the v0.7 trigger fires — not the signal values, and not
  before there is a second producer to justify it.
- Designing the declarative transform vocabulary in later would be a redesign at
  v0.7 (the importer's output _is_ the document, and a closure cannot be in it).
  Designing it in now costs one enum, and loses no DSL ergonomics because
  `Custom` keeps arbitrary closures available.

The as-built architecture is described in `docs/design/dashlang.md` ("Reactive
layer"). The `dashcue` reuse (the binding address is `dashcue`'s opaque
`PropKey`, and smoothing drives props through its `Scheduler`) is what lets both
crates speak one `(PropKey, f32)` language without depending on each other.
