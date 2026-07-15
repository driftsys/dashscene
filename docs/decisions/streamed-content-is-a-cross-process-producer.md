# Streamed vocabulary content is a cross-process producer, not a special-cased runtime path (proposed)

    status   proposed — a direction, not yet ratified
    date     2026-07-13
    source   docs/technotes/runtime-content.md §3
    scope    the provisioned "Kotlin/remote" skin; v2 streaming, pulled
             forward to one node

## Context

A Compose/Glance-like DSL can compute a scene in real time and stream it
into a placeholder without pre-rendering, provided it streams _intent_
through the wire role of the schema, not the `.dsb` file role (one
schema, two roles — `docs/decisions/dsb-format-and-one-schema.md`). Glance already works this
way: it runs the Compose runtime but does not render; its composition
emits an abstract tree translated by another process into RemoteViews or
a protobuf.

## Leaning

Treat streamed content as an ordinary producer entering via one of two
placements:

- **In-process** — translate each (re)composition directly into arena
  staged mutations (`open`/`set_prop`/`set_variant`/`commit`), no
  serialization, like the Rust DSL.
- **Cross-process** (Glance's real situation: Compose on the JVM, runtime
  in Rust) — a "Kotlin/remote" skin: composition emits a describe buffer,
  one commit across the FFI/IPC seam per recomposition, ongoing changes as
  tiny commit deltas.

The streamed fragment must use only the target profile's validated
vocabulary, resident assets, and parametric primitives — it cannot
introduce content needing an offline bake. P3 holds throughout:
recomposition that changes structure/props is unrestricted (commits at
data/event rate), but Compose frame-loop escape hatches
(`withFrameNanos`, `Animatable`+`suspend`, arbitrary per-frame
`AnimationSpec`) do not survive — they must lower to `dashcue` descriptive
specs, input-rate mutations, or engine-side slot work.

## Why this is a leaning, not a decision

Swapping Glance's RemoteViews/protobuf translator for a "Compose tree →
dashscene fragment" translator needs no new mechanism, and streaming
intent (rather than pre-rendered pixels) renders identically on every
painter within the box (trimmed-Skia entry and Unity high-end alike). It
is not yet ratified because the admission policy for a remote or
untrusted producer (Q-5) is undecided — without it, "streamed content" and
"trusted content" are not yet distinguishable, and that gap has to close
before the mechanism can bind downstream work.

## What ratifies this

- Resolve the admission policy (Q-5) for remote/untrusted streamed
  fragments — open in `docs/technotes/runtime-content.md` §8.

## Consequences

- Whichever placement is used, the placeholder must be declared-size,
  never hug, so streamed content does not reflow the scene
  (`docs/technotes/runtime-content.md` §7, §10.2).
