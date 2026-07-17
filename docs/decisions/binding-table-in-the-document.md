# The binding table in the document: named scalar signals, mode-qualified

    status   accepted (story #167, 2026-07-17)
    scope    dashbuf (Document.signals/bindings), dashscene-core (the arena
             tables), dashc (the lowering + the ABI v2 request), dashlang
             (staging + attach_live), importers/figma (the join)
    binds    the serialized binding-table shape; the signal naming contract
             a runtime looks document signals up by; the ABI v2 request

## Context

At v0.7 a binding becomes a document construct: the importer emits
bindings, and the importer's output is the document
(`docs/decisions/reactive-layer-home-and-staging.md`). Three shape
questions had to be answered before anything serialized: what a
serialized binding row is, how a Figma variable becomes signals, and
where the join between the phase-1 sidecar and the plugin-exported
vartable runs (`docs/decisions/token-resolution-phase-split.md`).

## Choice

### 1. The document carries signal declarations plus flat binding rows; the resolved literals stay

`Document.signals` is `[{ name?, initial: f32 }]`; `Document.bindings`
is `[{ signal, node, channel, transform }]` — the §23 model, verbatim
(`docs/decisions/bindings-are-explicit-and-flat.md`). The phase-1
resolved literals are untouched: a `.dsb` with binding tables still
renders correctly with no runtime that understands them, because every
bound channel's literal is the binding's transform applied to the
signal's initial. The transform is the identity everywhere except a
bound fill's alpha under paint opacity: the lowering folds the paint's
`opacity` into the shipped literal (`color.a * opacity`), so the FillA
row captures the same multiply as a `Scale(opacity)` transform over the
raw variable alpha — the seeded scene matches the literal, and a
producer pushing a new variable value stays folded. Signal _values_
never serialize (P1); a signal declaration's `initial` is authored
intent — the value the document displays until a producer pushes.

The alternative — replacing literals with token _references_ resolved
against a runtime theme table — was rejected: it makes every consumer
theme-aware to render at all, and it invents a second value-delivery
mechanism beside the reactive layer. Theme switching falls out of the
binding table instead: the vartable carries every mode's values, so a
theme flip is the producer setting each variable's signals to the other
mode's values through the one mechanism that already exists.

### 2. A variable is scalar signals: a COLOR is four, suffixed `.r/.g/.b/.a`

Signals are `f32` — the §23 decision that a color is four channels. A
COLOR variable `color/bg` declares `color/bg.r`, `color/bg.g`,
`color/bg.b`, `color/bg.a`, bound to the four `Fill` channels. A FLOAT
variable is one signal. STRING and BOOLEAN variables (text, variant,
visibility bindings) are later slices; the join names them
(`figma.bindings.unsupported-type`, warning) rather than dropping them.

### 3. A non-default mode qualifies the signal name: `size/gap@dark`

One variable resolved in two mode contexts is two signals. A subtree
pinned dark (`explicitVariableModes`) binds `size/gap@dark` with the
dark value as its initial; an unpinned subtree binds `size/gap` with the
default mode's value. The pin is authored intent — the designer chose
that subtree's mode — so a runtime driving `size/gap` must not move the
dark-pinned subtree. Two different variables (or modes) yielding one
name is a named join error (`figma.bindings.ambiguous-signal`), and the
load gate refuses duplicate declared names (`signal.name-duplicate`),
so a by-name lookup is never ambiguous.

### 4. The join splits at the ABI: Deno owns variables and modes, dashc owns channels

The Deno importer joins sidecar rows against the vartable
(`importers/figma/src/bindings.ts`): staleness, id resolution, per-node
mode, value extraction, signal naming. What crosses the wasm ABI
(version 2 — one appended request section, the framing unchanged,
`docs/decisions/dashc-wasm-abi.md`) is one row per site: node id,
property path, signal name, resolved value. `dashc`'s Figma-aware half
(`crates/dashc/src/figma/bindings.rs`) maps property paths onto binding
channels — `itemSpacing` → `Gap`, a solid fill's `fills[i].color` → the
four `Fill` channels — and names every path without a channel
(`figma.bindings.unsupported-property`, warning: the literal ships).
Join errors block the export (`BindingsBlocked`), matching
`TokensBlocked`.

### 5. `Custom` transforms never serialize; declarative rows stage everywhere

`dashlang`'s `build_live` stages every declarative binding into the
arena tables, so a `dashlang` scene and a loaded document expose one
table — the acceptance that makes "same screen both ways" (#48) mean
something is `crates/dashc/tests/bindings_lowering.rs`, which pins
row-identical tables from both producers. A `Custom(ClosureId)` binding
stays in `dashlang`'s live tables only; a `Custom` transform reaching
`dashc::compile` is the named error `binding.custom-transform` (D8, P4).

### 6. `attach_live` is the loader-side consumer

`dashlang::attach_live(arena, solver)` builds a `LiveScene` from a
loaded arena's tables: document signals become live signals addressable
by name (`LiveScene::signal_named`), rows become scalar bindings with
the same write classification as authored ones (fill channels
paint-only, `Gap` layout-affecting, contained rect channels patched).
One mechanism, two producers.

## Consequences

- The scheduler address is one packing everywhere: the math
  (`prop_key`/`decode_prop_key`) lives in `dashscene-core` beside
  `Channel`, over core types (a bare `u64` — core cannot depend on
  `dashcue`); `dashscene-engine` exposes it as the typed
  `dashcue::PropKey` and `VariantFlip` validates track keys through the
  one decoder; `dashlang` deleted its private packing and builds keys
  from core's (debts #207/#208, which said "engine-owned" — refined
  here to core-owned math with an engine-typed surface, so `dashlang`'s
  library keeps its core-plus-dashcue dependency set,
  `docs/decisions/dashlang-flex-vocabulary.md` D3).
- The `Channel` vocabulary lives in `dashscene-core`, completed with
  `Fill.*` and `Gap` (debt #201).
- The wasm ABI version is 2; a stale `dashc_wasm.wasm` fails the version
  handshake with a sentence, never a misdecode.
- The `.dsb` frozen fixture was regenerated in the same change that
  appended the schema fields (the legitimate append case,
  `docs/decisions/dsb-frozen-fixture-r7-guard.md`).

## Alternatives considered

- **Token refs in the `.dsb`, literals removed** — rejected (choice 1):
  every consumer becomes theme-aware, and the runtime gains a second
  value path beside signals.
- **One signal per variable with a color-typed value** — rejected
  (choice 2): the reactive layer's scalar `(PropKey, f32)` language is
  what lets `dashcue` animate any bound channel; a color-typed signal
  would need a parallel vocabulary.
- **A global mode selector instead of mode-qualified names** — rejected
  (choice 3): a document-level mode switch would move dark-pinned
  subtrees the designer pinned deliberately.
- **The packing owned by the engine** — rejected: it would put
  `dashlang`'s library on `dashscene-engine` for one pure function,
  contradicting `docs/decisions/dashlang-flex-vocabulary.md` D3 (the
  solver is injected; `cargo tree -p dashlang` shows core, not the
  engine) and the crate map's dashlang-builds-on-core statement. The
  math is a pure function over core types, so it lives in core.
- **The join in dashc (vartable across the ABI)** — rejected (choice 4):
  the sidecar is the decided join input ("one mechanism, two consumers",
  `docs/decisions/token-resolution-phase-split.md`), the vartable
  machinery (parse, staleness) already lives importer-side, and the
  split keeps mode knowledge out of the Rust tree and channel knowledge
  out of Deno.

## Trace

- Satisfies: issue #167 acceptance criteria; §23 D8/D9
  (`docs/archive/2026-07-14-scope-decisions.md`); P1 (initials are
  intent, values never serialize), P4 (every unjoined or uncarried site
  is named), P5 (property→channel mapping lives in the one Figma-aware
  producer half).
- Verified by: `crates/dashc/tests/bindings_lowering.rs` (end to end:
  capture + rows → `.dsb` → arena → `attach_live`; producer parity;
  the Custom refusal), `importers/figma/src/bindings_test.ts` (the join
  on the fixture pair, both mode contexts, every named verdict),
  `crates/dashbuf/tests/bindings_roundtrip.rs` and
  `tests/schema_evolution.rs` (the serialized shape).
- Related: `docs/decisions/reactive-layer-home-and-staging.md`,
  `docs/decisions/bindings-are-explicit-and-flat.md`,
  `docs/decisions/token-resolution-phase-split.md`,
  `docs/decisions/dashc-wasm-abi.md`,
  `docs/decisions/dsb-frozen-fixture-r7-guard.md`.
