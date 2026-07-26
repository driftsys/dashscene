# System architecture

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §2, §4, §5, §6, §7, §8, §13

dashscene turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation (the dashscene
document, serialized as `.dsb`), one shared layout+text runtime, and
interchangeable paint backends.

This record carries what no other record does: the stack, the three-stage
pipeline and its two boundaries, and the map from crate to purpose to design
record. The document schema, the producers, the runtime internals, the
painters, and the project vocabulary each already have their own record —
this page links to them rather than restating them. The test applied to every
paragraph below: does this content exist somewhere else? If it does, it is a
link here, not prose.

## Stack

| Concern           | Choice                              | Why                                                                                  |
| ----------------- | ----------------------------------- | ------------------------------------------------------------------------------------ |
| Layout solver     | Taffy (Rust)                        | only engine covering all four Figma modes; CSS Grid native; pure Rust                |
| Text shaping      | rustybuzz                           | HarfBuzz port, pure Rust; Arabic GSUB/GPOS                                           |
| Bidi              | unicode-bidi                        | run splitting, RTL                                                                   |
| Font metrics      | ttf-parser                          | same font tables as FreeType, same numbers                                           |
| Glyph atlas       | msdf-atlas-gen                      | MSDF quality, keyed by glyph id (contextual forms included)                          |
| Document format   | FlatBuffers                         | zero-copy mmap load, schema evolution, same schema as wire format                    |
| Vector baking     | lyon (mesh) / SDF atlas             | arbitrary paths baked at compile time                                                |
| Reference painter | skia-safe (Skia)                    | full 2D vocabulary; CPU raster = bit-exact deterministic goldens; GPU/GLES on target |
| Wasm painter      | tiny-skia                           | pure Rust, Skia algorithms, plain wasm32 (no emscripten)                             |
| Engine painter    | Unity + SDF shader library          | lit world-space rendering; thin C# projection                                        |
| Lean painter      | custom instanced SDF-quad renderer  | bandwidth-optimal for a quad vocabulary; ~5-10 KLOC                                  |
| Textures          | KTX2/Basis (UASTC/ETC1S) → ASTC/EAC | small at rest, GPU-native in VRAM                                                    |
| Figma source      | @figma/rest-api-spec                | official OpenAPI 3.1 types; stable-additive                                          |

Term-by-term detail: [glossary.md](../technotes/glossary.md).

## Pipeline

Three stages, two boundaries:

    STAGE 1 — build time (dashc, offline)
      Figma REST JSON --> dashc --> .dsb (sectioned container) + assets

    STAGE 2 — common runtime (Rust, one instance)
      arena + variants + text stack + Taffy + FLIP
        --> rect table + positioned glyph runs (double-buffered)

    STAGE 3 — painters (one per target, one trait)
      Skia (built)    Unity (planned, v1)    lean GPU (planned, later)

    boundary A = .dsb load gate (version + per-section hashes)
    boundary B = painter contract: rect table + glyph runs + paint indices.
                 A painter never measures, wraps, kerns, or moves anything.

Stage 1 does everything that can fail at compile time (validation, lowering,
atlas generation, shadow baking). Stage 2 does everything that must be
identical across backends, once. Stage 3 does only what is legitimately
per-target: how a rectangle gets colored.

Programmatic producers enter at stage 2 directly: the arena's staged-mutation
API (`open`/`set_prop`/`set_variant`/`commit`) is the real contract, and
`.dsb` is one way to populate it — see
[dashscene-core-arena.md](dashscene-core-arena.md).

- **Boundary A** is the `.dsb` load gate — schema and section format:
  [dashbuf.md](dashbuf.md).
- **Boundary B** is the painter contract, and a painter never measures, wraps,
  kerns, or moves anything (P2 —
  [02-principles.md](../specification/02-principles.md)) — types and trait:
  [dashpaint.md](dashpaint.md).

## The document, producers, runtime, painters

Framing only; each of these has its own record.

### The document

The dashscene document is a FlatBuffers schema: a flattened DFS node tree,
interned strings, a deduplicated style/paint pool, and a section layout that
lets the loader mmap hot sections and verify them without touching cold pages
(R5). A named paint-vocabulary subset each painter honors (profile:full /
profile:core, P4) is enforced by the validator. Schema and section format:
[dashbuf.md](dashbuf.md). Validator: [dashscene-validator.md](dashscene-validator.md).
Naming: the IR is _the dashscene document_; `.dsb` is only its file extension
— [dashscene-document-is-the-ir.md](../decisions/dashscene-document-is-the-ir.md).
Vocabulary: [glossary.md](../technotes/glossary.md).

### Producers

Two paths enter the document: the offline compile path (external JSON →
`dashc` lowering → validation → `.dsb`; Figma today) and the in-memory arena
path (producer code → the staged-mutation API directly; the Rust DSL today).
Picking the right path per producer — never inventing a new format — is the
rule for every future producer. Full treatment, including why no neutral IR
sits above dashscene and the Penpot/Slint calls:
[producers-and-ir.md](../technotes/producers-and-ir.md). As-built:
[dashc.md](dashc.md) (Figma), [dashlang.md](dashlang.md) (Rust DSL).

### Common runtime

One instance, run once per commit: Taffy solves layout for every backend
([dashscene-engine.md](dashscene-engine.md)); text is shaped once against a
glyph atlas built at compile time
([atlas-pipeline.md](atlas-pipeline.md),
[typeset-latin.md](typeset-latin.md)); the output is the generation-stamped
double buffer of rect entries plus glyph runs plus a dirty set — boundary
B's entire input.

### Painters

One trait behind boundary B: a painter swap is a re-golden, not a redesign,
because the same document plus the same commits produce bit-identical rect
tables regardless of which painter draws them. Skia is the shipped reference
(CPU raster = bit-exact goldens; GPU on GLES = the on-target entry path) —
[dashscene-skia.md](dashscene-skia.md). The SDF-quad model, the backend
tiering, and the Unity-painter internals:
[rendering-and-painters.md](../technotes/rendering-and-painters.md). Unity,
the lean native painter, and the web painter are not built yet — see
"Planned components" below.

## Component map

What each crate is and where its record lives. This mirrors the real
workspace layout (`docs/archive/2026-07-14-design-1-seed.md` §13, repaired by
PR #152 after its original `scd-*` suggestion was never adopted): one role
from that suggestion split in two on contact with the code —
`dashscene-core` (the model a producer mutates) and `dashscene-engine` (the
runtime that solves it),
`docs/archive/2026-07-14-scope-decisions.md` §9.

| Path                          | Role                                                                                                | Record                                                                                          |
| ----------------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `crates/dashscene`            | umbrella / facade crate                                                                             | — (thin re-export)                                                                              |
| `crates/dashscene-core`       | arena, node tree, layout+paint tables, staged-mutation API                                          | [dashscene-core-arena.md](dashscene-core-arena.md)                                              |
| `crates/dashscene-engine`     | Taffy solve, variants, FLIP, measure callback                                                       | [dashscene-engine.md](dashscene-engine.md)                                                      |
| `crates/dashscene-typeset`    | bidi, shaping, glyph atlas pipeline                                                                 | [atlas-pipeline.md](atlas-pipeline.md), [typeset-latin.md](typeset-latin.md)                    |
| `crates/dashscene-validator`  | profiles, diagnostics, waivers                                                                      | [dashscene-validator.md](dashscene-validator.md)                                                |
| `crates/dashpaint`            | paint table + painter trait (boundary B)                                                            | [dashpaint.md](dashpaint.md)                                                                    |
| `crates/dashscene-skia`       | Skia reference painter                                                                              | [dashscene-skia.md](dashscene-skia.md)                                                          |
| `crates/dashcue`              | descriptive animation vocabulary + runtime scheduling                                               | [dashcue.md](dashcue.md)                                                                        |
| `crates/dashlang`             | Rust DSL skin + stress-corpus generator                                                             | [dashlang.md](dashlang.md)                                                                      |
| `crates/dashbuf`              | the `.dsb` flatbuffer schema                                                                        | [dashbuf.md](dashbuf.md)                                                                        |
| `crates/dashc`                | compiler CLI; also builds to wasm32 for the Deno importer                                           | [dashc.md](dashc.md), [vector-msdf-baking.md](vector-msdf-baking.md)                            |
| `crates/dashpack-astcenc-sys` | raw bindings to the vendored astcenc C++ sources — ASTC encode plus the in-process reference decode | in progress (epic #345) — [native-astc-codec-table.md](../decisions/native-astc-codec-table.md) |
| `crates/dashpack`             | asset packer — per-profile derivations, cold-bank assembly, derivation manifest                     | in progress (epic #345) — [crate-name-map.md](../decisions/crate-name-map.md)                   |
| `crates/dashscene-unity`      | Rust-side FFI bindings for the Unity painter                                                        | planned — see below                                                                             |
| `crates/dashscene-web`        | wasm/tiny-skia painter                                                                              | planned — see below                                                                             |
| `importers/figma/`            | Deno/TypeScript Figma REST importer + `sharedPluginData` annotator plugin                           | [dashc-wasm-abi.md](../decisions/dashc-wasm-abi.md) (the ABI it calls through)                  |
| `corpus/`                     | DSL-generated stress corpus + Figma fixture captures                                                | —                                                                                               |
| `goldens/`                    | CI golden images + diff tooling (`goldens/tooling` workspace member)                                | [goldens.md](goldens.md)                                                                        |

## Planned components (not yet built)

Unity, the lean native painter, the web painter, placeholders and node
replacement, and remote streaming are all unbuilt. Each is listed here
anyway, marked **planned**, and named against the requirement or decision
that binds it:

- **Unity painter (v1)**, plus its C# declarative producer front end — ships
  as a separate, not-yet-created repo behind `dashscene-unity`'s Rust FFI
  bindings. Bound by G2 (multiple render backends) and R3 (GPU is the
  target's bottleneck) in
  [01-goals-and-requirements.md](../specification/01-goals-and-requirements.md);
  deferred to v1 in a separate repo by
  `docs/archive/2026-07-14-scope-decisions.md` §5. Internals:
  [rendering-and-painters.md](../technotes/rendering-and-painters.md) §9-§10.
- **Lean native painter** — no crate name is reserved yet. Bound by R3
  ("far less memory and CPU than the engine backend"); the decision to
  build it is deliberately deferred to on-target measurement of the trimmed
  Skia entry tier —
  [rendering-and-painters.md](../technotes/rendering-and-painters.md) §5.
- **Web painter** (`dashscene-web`, wasm + tiny-skia) — parked. Bound by G2
  ("wasm (review)"); the climb-only-when-pushed ladder toward it is
  `docs/archive/2026-07-14-design-1-seed.md` §8.4.
- **Placeholders and node replacement** — a reserved schema surface:
  `Node` already carries the fields (`contribution_id`/`fragment_ref`/
  `declared_size`/`interim_fill`), added append-only so existing loaders
  keep reading new documents. Bound by R5 (cold-start cost proportional to
  what is shown) in
  [01-goals-and-requirements.md](../specification/01-goals-and-requirements.md)
  and `docs/archive/2026-07-14-design-1-seed.md` §10.2. The contract that
  will fill a placeholder at runtime is already designed:
  [runtime-content.md](../technotes/runtime-content.md) §7. Node
  replacement is an engine-painter-only concept, so it binds to the Unity
  painter row above as well.
- **Remote streaming (v2)** — bound today, not just later:
  [remoting-two-transports.md](../decisions/remoting-two-transports.md)
  already constrains the current producer API (handles vs. indices) so v2
  does not become an API break.

**This is a deliberate deviation from the `sdd-working-memory-lifecycle`
rule**, taken on purpose, not an oversight. That rule says shipped docs
describe the system as-built and forward-looking concepts stay in
`docs/wip/`. But boundary B exists specifically so a painter is
interchangeable — "painter swap = re-golden, not redesign" — and the
painters that prove that claim mostly do not exist yet. Deleting them from
this record would delete the reason boundary A and boundary B, the profile
system, and the file/wire schema split are shaped the way they are. Each
unbuilt item above is marked planned and traced to what binds it, which
satisfies the rule's actual concern (that an unbuilt thing not be described
as built) without erasing the "why" from the record.
