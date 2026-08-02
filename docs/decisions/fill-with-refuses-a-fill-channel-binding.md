# `fill_with` beside a fill-channel binding is a named build-time refusal

    status   accepted (2026-08-01)
    scope    crates/dashlang (stage_live, the fill shadow); binds every
             producer that authors a non-solid fill on a bound node, and the
             loader-side attach_live path (debt #667)

## Context

The v0 paint vocabulary on `dashlang::Node`
(`docs/decisions/dashlang-paint-vocabulary.md`) made a combination reachable
that no producer could write before: a node carrying both a gradient or an
image fill through `fill_with(..)` and a reactive binding on one of the four
`Fill` channels.

The two cannot both survive. A fill-channel binding drives **one component of
a solid colour** through the node's per-node fill shadow, and every write it
makes — including the seed `build_live` stages before the first frame — is a
`Prop::Fill`, which replaces the node's whole fill slot.

Measured before the fix: a node authoring a linear gradient and
`bind(Channel::FillA, alpha)` committed a solid colour instead of its
gradient. The shadow seeds from the solid `fill` only, so with no solid fill
authored the seed is the transparent-black constant, and the bound alpha
component is written into it. The authored gradient was gone from the first
committed frame, and nothing reported it.

## Decision

`stage_live` panics by name when a node authors `fill_with(..)` and binds any
`Fill*` channel. The message names the node, the channel, and both ways out —
bind the fill channels of a solid `fill(..)` instead, or drop the fill-channel
binding and keep `fill_with(..)`.

This is P4: vocabulary is validated, never discovered. An out-of-profile
combination is a named diagnostic, never a silent drop. It joins the one
refusal `stage_live` already made — a `smooth()` whose channel has no matching
`bind()`, which would be silently inert (debt #194).

## Why not merge the two instead

There is no merge to perform. A gradient carries no four-component colour for
the channels to address, and an image carries none either, so a component
write has nothing to write into. Preferring one authoring over the other —
dropping the binding, or dropping the paint — would be a silent loss whichever
way it went, and neither is more defensible than the other. Refusing names the
conflict at the point the author can still fix it.

## Why a panic rather than a diagnostic

`stage_live` runs at build, in the producer's own process, on a value tree the
producer just wrote. It is not a document gate. The existing refusal at the
same seam is a panic for the same reason, and matching it keeps one behaviour
at one seam.

The loader side is different, and is **not** covered here. `attach_live` can
reach the same shape from a loaded `.dsb` — a document gradient plus a
fill-channel binding row — where a panic would fail a document at load time
rather than an author at build time. That is filed as debt #667, and it wants
a validator diagnostic rather than this panic. The asymmetry is known and
deliberate, not an oversight.

## Trace

- Proven by: `crates/dashlang/tests/reactive.rs`,
  `fill_with_plus_a_fill_channel_binding_is_refused_by_name`.
- As-built description: `docs/design/dashlang.md`, "Refused combinations".
- Related decisions: `docs/decisions/dashlang-paint-vocabulary.md` (the
  vocabulary that made the combination reachable);
  `docs/decisions/visible-is-layout-opacity-is-paint.md` and
  `docs/decisions/bindings-are-explicit-and-flat.md` (the channel model);
  `docs/decisions/binding-table-in-the-document.md` (the loader-side table
  debt #667 sits on).
- Open: debt #667 — the same silent-loss shape on `attach_live`, pre-existing
  and unresolved.
