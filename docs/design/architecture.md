# System architecture

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §2, §4, §5, §6, §7, §8, §13

dashscene turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation (the dashscene
document, serialized as `.dsb`), one shared layout+text runtime, and
interchangeable paint backends.

This record carries what no other record does: the stack, the three-stage
pipeline and its two boundaries, and the map from crate to purpose to design
record. The document schema, the producers, the runtime internals, the painters,
and the project vocabulary each already have their own record — this page links
to them rather than restating them. The test applied to every paragraph below:
does this content exist somewhere else? If it does, it is a link here, not
prose.

## Stack

| Concern           | Choice                              | Why                                                                                                |
| ----------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------- |
| Layout solver     | Taffy (Rust)                        | only engine covering all four Figma modes; CSS Grid native; pure Rust; proven at scale — see below |
| Text shaping      | rustybuzz                           | HarfBuzz port, pure Rust; Arabic GSUB/GPOS                                                         |
| Bidi              | unicode-bidi                        | run splitting, RTL                                                                                 |
| Font metrics      | ttf-parser                          | same font tables as FreeType, same numbers                                                         |
| Glyph atlas       | msdf-atlas-gen                      | MSDF quality, keyed by glyph id (contextual forms included)                                        |
| Document format   | FlatBuffers                         | zero-copy mmap load, schema evolution, same schema as wire format                                  |
| Vector baking     | lyon (mesh) / SDF atlas             | arbitrary paths baked at compile time                                                              |
| Reference painter | skia-safe (Skia)                    | full 2D vocabulary; CPU raster = bit-exact deterministic goldens; GPU/GLES on target               |
| Wasm painter      | ~~tiny-skia~~ — retired at v0.15    | the lean painter reaches the browser from the same codebase as native                              |
| Engine painter    | Unity + SDF shader library          | lit world-space rendering; thin C# projection                                                      |
| Lean painter      | custom instanced SDF-quad renderer  | bandwidth-optimal for a quad vocabulary; ~5-10 KLOC                                                |
| Textures          | KTX2/Basis (UASTC/ETC1S) → ASTC/EAC | small at rest, GPU-native in VRAM                                                                  |
| Figma source      | @figma/rest-api-spec                | official OpenAPI 3.1 types; stable-additive                                                        |

Term-by-term detail: [glossary.md](../technotes/glossary.md).

**On Taffy being proven at scale.** Taffy's own README lists its users as
[Servo](https://github.com/servo/servo),
[Blitz](https://github.com/DioxusLabs/blitz), [Bevy](https://bevyengine.org/)
and the [Zed](https://zed.dev/) editor via
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (a moving
ref; the pinned claim above is Taffy's README, not Zed's tree)
([README.md](https://github.com/DioxusLabs/taffy/blob/0875874872e622131324e276dc77392517fd4bb1/README.md),
retrieved 2026-08-10, pinned to the commit current on that date). A browser
engine, a game engine and an editor exercise flexbox and grid harder than this
project will, which is the argument for adopting the solver rather than writing
one.

This is the claim that has circulated here as a "Taffy/Servo/Bevy/Slint/Zed
lineage" — a phrase that also named Slint, which does not use Taffy, and which
was sometimes read as dashscene's own ancestry rather than as Taffy's adopters.
It is recorded here, once and sourced, so it does not have to be restated
elsewhere.

## Pipeline

Three stages, two boundaries:

    STAGE 1 — build time (dashc, offline)
      Figma REST JSON --> dashc --> .dsb (sectioned container) + assets

    STAGE 2 — common runtime (Rust, one instance)
      arena + variants + text stack + Taffy + FLIP
        --> rect table + positioned glyph runs (double-buffered)

    STAGE 3 — painters (one per target, one trait)
      Skia (built)    lean GPU (built, v0.15)    Unity (planned, v0.21)

    boundary A = .dsb load gate (version + per-section hashes)
    boundary B = painter contract: rect table + glyph runs + paint indices.
                 A painter never measures, wraps, kerns, or moves anything.

Stage 1 does everything that can fail at compile time (validation, lowering,
atlas generation, shadow baking). Stage 2 does everything that must be identical
across backends, once. Stage 3 does only what is legitimately per-target: how a
rectangle gets colored.

Programmatic producers enter at stage 2 directly: the arena's staged-mutation
API (`open`/`set_prop`/`set_variant`/`commit`) is the real contract, and `.dsb`
is one way to populate it — see
[dashscene-core-arena.md](dashscene-core-arena.md).

- **Boundary A** is the `.dsb` load gate — schema and section format:
  [dashbuf.md](dashbuf.md).
- **Boundary B** is the painter contract, and a painter never measures, wraps,
  kerns, or moves anything (P2 —
  [02-principles.md](../specification/02-principles.md)) — types and trait:
  [dashpaint.md](dashpaint.md).

**Boundary B is a language-neutral data contract, not a Rust trait that happens
to have painters behind it.** G2 names the backends and one of them is Unity,
which is C#; a Rust trait cannot serve C#. So the interchangeability this
document claims — "a painter swap is a re-golden, not a redesign" — holds for
Rust painters by construction and for a non-Rust painter only if the tables
crossing the boundary have a C representation. That makes representability the
condition on a goal already written down, rather than provision for a
hypothetical backend. Once it holds, Unreal and Kanzi are the same problem as
Unity minus the C# adapter, since both consume a C header directly.

It is enforced rather than asserted: `crates/dashpaint-abi` declares an
`extern "C"` surface over the boundary-B value types under
`#![deny(improper_ctypes_definitions)]`, so making one of them non-representable
stops the workspace compiling (story #600). The surface is narrow today and
widens as story #578 flattens what is left. `PaintKind` carried payloads until
#578 gave it a tag and a row index, and it is on the surface now with
`Gradient`, `StopRange` and `ImageFill`. `PaintEntry` followed it, its `Option`s
and its one `Vec` replaced by ranges into the table's flat arrays, so it is on
the surface too at a pinned 64 bytes. `ImageAsset` is what remains, and is a
different problem: its `Vec<u8>` is a payload rather than a reference into a
table.

**What this does not buy.** It removes one obstacle, not the work. A non-Rust
engine still needs its own projection of the tables into its scene
representation, its own material library, its own atlas upload path, and its own
oracle calibration against the reference painter. Representability is what makes
those tractable; it does not do any of them.

## The document, producers, runtime, painters

Framing only; each of these has its own record.

### The document

The dashscene document is a FlatBuffers schema: a flattened DFS node tree,
interned strings, a deduplicated style/paint pool, and a section layout that
lets the loader mmap hot sections and verify them without touching cold pages
(R5). A named paint-vocabulary subset each painter honors (profile:full /
profile:core, P4) is enforced by the validator. Schema and section format:
[dashbuf.md](dashbuf.md). Validator:
[dashscene-validator.md](dashscene-validator.md). Naming: the IR is _the
dashscene document_; `.dsb` is only its file extension —
[dashscene-document-is-the-ir.md](../decisions/dashscene-document-is-the-ir.md).
Vocabulary: [glossary.md](../technotes/glossary.md).

### Producers

Two paths enter the document: the offline compile path (external JSON → `dashc`
lowering → validation → `.dsb`; Figma today) and the in-memory arena path
(producer code → the staged-mutation API directly; the Rust DSL today). Picking
the right path per producer — never inventing a new format — is the rule for
every future producer. Full treatment, including why no neutral IR sits above
dashscene and the Penpot/Slint calls:
[producers-and-ir.md](../technotes/producers-and-ir.md). As-built:
[dashc.md](dashc.md) (Figma), [dashlang.md](dashlang.md) (Rust DSL).

### Common runtime

One instance, run once per commit: Taffy solves layout for every backend
([dashscene-engine.md](dashscene-engine.md)); text is shaped once against a
glyph atlas built at compile time ([atlas-pipeline.md](atlas-pipeline.md),
[typeset-latin.md](typeset-latin.md)); the output is the generation-stamped
double buffer of rect entries plus glyph runs plus a dirty set — boundary B's
entire input.

### Painters

One trait behind boundary B: a painter swap is a re-golden, not a redesign,
because the same document plus the same commits produce bit-identical rect
tables regardless of which painter draws them. Skia is the shipped reference
(CPU raster = bit-exact goldens; GPU on GLES = the on-target entry path) —
[dashscene-skia.md](dashscene-skia.md). The SDF-quad model, the backend tiering,
and the Unity-painter internals:
[rendering-and-painters.md](../technotes/rendering-and-painters.md). The lean
painter is built, native and browser, at the v0.15 close —
[dashscene-gpu.md](dashscene-gpu.md). Unity is not built yet — see "Planned
components" below.

## Component map

What each crate is and where its record lives. This mirrors the real workspace
layout (`docs/archive/2026-07-14-design-1-seed.md` §13, repaired by PR #152
after its original `scd-*` suggestion was never adopted): one role from that
suggestion split in two on contact with the code — `dashscene-core` (the model a
producer mutates) and `dashscene-engine` (the runtime that solves it),
`docs/archive/2026-07-14-scope-decisions.md` §9.

| Path                          | Role                                                                                                                                                                                                                                                                    | Record                                                                                                                                                                                                                         |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/dashscene`            | umbrella / facade crate                                                                                                                                                                                                                                                 | — (thin re-export)                                                                                                                                                                                                             |
| `crates/dashscene-core`       | arena, node tree, layout+paint tables, staged-mutation API                                                                                                                                                                                                              | [dashscene-core-arena.md](dashscene-core-arena.md)                                                                                                                                                                             |
| `crates/dashscene-engine`     | Taffy solve, variants, FLIP, measure callback                                                                                                                                                                                                                           | [dashscene-engine.md](dashscene-engine.md)                                                                                                                                                                                     |
| `crates/dashscene-typeset`    | bidi, shaping, glyph atlas pipeline                                                                                                                                                                                                                                     | [atlas-pipeline.md](atlas-pipeline.md), [typeset-latin.md](typeset-latin.md)                                                                                                                                                   |
| `crates/dashscene-validator`  | profiles, diagnostics, waivers                                                                                                                                                                                                                                          | [dashscene-validator.md](dashscene-validator.md)                                                                                                                                                                               |
| `crates/dashpaint`            | paint table + painter trait (boundary B)                                                                                                                                                                                                                                | [dashpaint.md](dashpaint.md)                                                                                                                                                                                                   |
| `crates/dashscene-skia`       | Skia reference painter                                                                                                                                                                                                                                                  | [dashscene-skia.md](dashscene-skia.md)                                                                                                                                                                                         |
| `crates/dashcue`              | descriptive animation vocabulary + runtime scheduling                                                                                                                                                                                                                   | [dashcue.md](dashcue.md)                                                                                                                                                                                                       |
| `crates/dashlang`             | Rust DSL skin + stress-corpus generator                                                                                                                                                                                                                                 | [dashlang.md](dashlang.md)                                                                                                                                                                                                     |
| `crates/dashbuf`              | the `.dsb` flatbuffer schema                                                                                                                                                                                                                                            | [dashbuf.md](dashbuf.md)                                                                                                                                                                                                       |
| `crates/dashc`                | compiler CLI; also builds to wasm32 for the Deno importer                                                                                                                                                                                                               | [dashc.md](dashc.md), [vector-msdf-baking.md](vector-msdf-baking.md)                                                                                                                                                           |
| `crates/dashpack-astcenc-sys` | raw bindings to the vendored astcenc C++ sources — ASTC encode plus the in-process reference decode                                                                                                                                                                     | in progress (epic #345) — [native-astc-codec-table.md](../decisions/native-astc-codec-table.md)                                                                                                                                |
| `crates/dashpack`             | asset packer — per-profile derivations, cold-bank assembly, derivation manifest                                                                                                                                                                                         | in progress (epic #345) — [asset-quality-profile-bands.md](../decisions/asset-quality-profile-bands.md)                                                                                                                        |
| `crates/dashpaint-abi`        | the `extern "C"` surface that holds boundary B representable — a gate, not bindings, and not Unity's (named `dashscene-unity` until issue #1239)                                                                                                                        | the gate is built (story #600) and checked against C# declarations by `unity/abi-check` (story #1239); the painter is planned — see below                                                                                      |
| `crates/dashscene-web`        | web integration — canvas-to-surface handoff, the `requestAnimationFrame` loop, resize rebuild, byte-range `.dsb` load                                                                                                                                                   | [host-integration.md](host-integration.md) — built at v0.17 (story #741); the wasm/tiny-skia painter the name once described is retired, superseded by `dashscene-gpu`                                                         |
| `crates/dashscene-ffi`        | the C ABI every platform host sits on — runtime lifecycle, `.dsb` load, the tick, resize, the surface handoff, and the committed frame handed out under a lease for a host that paints it itself; no panic crosses it and no failure is a string only                   | [c-abi.md](c-abi.md) — built at v0.19 (story #840); the Android host of story #841 and the iOS and Unity hosts that follow all sit on it                                                                                       |
| `crates/dashscene-desktop`    | desktop integration — window-to-surface handoff, the `winit` frame loop, resize rebuild, the published `Present` seam, a mapped `.dsb` load bounded by the shown root                                                                                                   | [host-integration.md](host-integration.md) — built at v0.17 (story #794); `demo` keeps the demonstration and consumes it                                                                                                       |
| `crates/dashscene-android`    | Android integration — the `android.view.Surface` to `ANativeWindow` handoff, the `AChoreographer` frame loop on its own thread, and the `surfaceDestroyed` handshake that blocks until the surface is dropped; the first host to sit on the C ABI rather than beside it | [android-toolchain.md](android-toolchain.md) — built at v0.19 (story #841); layer 0 of `docs/decisions/host-integration-in-three-layers.md`, and the layer that record calls "the whole of _show a designed screen in my app_" |
| `crates/dashscene-gpu`        | the lean painter — instanced quads and analytic SDF over wgpu, native and web                                                                                                                                                                                           | [dashscene-gpu.md](dashscene-gpu.md)                                                                                                                                                                                           |
| `importers/figma/`            | Deno/TypeScript Figma REST importer + `sharedPluginData` annotator plugin                                                                                                                                                                                               | [dashc-wasm-abi.md](../decisions/dashc-wasm-abi.md) (the ABI it calls through)                                                                                                                                                 |
| `corpus/`                     | DSL-generated stress corpus + Figma fixture captures                                                                                                                                                                                                                    | —                                                                                                                                                                                                                              |
| `goldens/`                    | CI golden images + diff tooling (`goldens/tooling` workspace member)                                                                                                                                                                                                    | [goldens.md](goldens.md)                                                                                                                                                                                                       |

## Planned components (not yet built)

Unity, placeholders and node replacement, and remote streaming are unbuilt. Each
is listed here anyway, marked **planned**, and named against the requirement or
decision that binds it.

**The lean native painter and the web painter left this section at the v0.15
close** — they are one component, `dashscene-gpu`, and it is built for native
and for the browser from one codebase. Its as-built record is
[dashscene-gpu.md](dashscene-gpu.md). What bound it stays true and is recorded
there: R3 ("far less memory and CPU than the engine backend") and G2 ("wasm
(review)"), the 2026-07-13 sequencing that deferred it to on-target measurement
of a trimmed Skia entry tier, and the amendment in
[wgpu-is-the-lean-painter.md](../decisions/wgpu-is-the-lean-painter.md) that
overtook it, because the same painter was needed for the web regardless.
Skia-GPU is recorded there as **not planned**, and Skia remains the bit-exact
CPU oracle permanently. The **painter** `dashscene-web` once reserved a name for
is retired; the name itself is not — story #741 made it the web integration
crate at v0.17, which is a host concern rather than a painter. **Two things this
does not claim**: the entry tier has not switched, because no entry SoC has been
measured (epic #476), and the browser target is WebGPU only.

- **Unity painter (v0.21)**, plus its C# declarative producer front end — ships
  as a UPM package **in this repository, under `unity/`**, over the C ABI, with
  `dashpaint-abi` holding the gate that keeps boundary B C-representable. **The
  package exists and holds neither**: story #1239 created `unity/` with the C#
  declarations of the boundary-B value types and the check that holds them to
  the Rust layouts, story #1121 added the C# host on the C ABI and a second
  check that executes it, and no painter and no producer front end are written.
  Bound by G2 (multiple render backends) and R3 (GPU is the target's bottleneck)
  in
  [01-goals-and-requirements.md](../specification/01-goals-and-requirements.md);
  put in a separate repo by `docs/archive/2026-07-14-scope-decisions.md` §5,
  which deferred it to v1; it is v0.21 since 2026-08-12, and the separate-repo
  half was **reversed on 2026-08-17**
  ([unity-package-sited-in-this-repository.md](../decisions/unity-package-sited-in-this-repository.md)).
  A ruling of the same day renames the crate to `dashpaint-abi` —
  [crate-name-map.md](../decisions/crate-name-map.md), which is where crate
  names are decided. Story #1239 carried out both. Internals:
  [rendering-and-painters.md](../technotes/rendering-and-painters.md) §9-§10. It
  is instanced SDF quads too, so it shares `dashscene-gpu`'s instance struct and
  its layer-1 and layer-2 suites (R-T5).
- **Placeholders and node replacement** — **the schema surface exists;
  activation does not.** Story #1126 added `table Placeholder`
  (`contribution_id`/`fragment_ref`/`declared_size`/`interim_fill`) and one
  appended `Node.placeholder` field holding it, so an ordinary node writes no
  table and a pre-#1126 document encodes byte-identically. `dashc`'s emitter
  lowers a declared placeholder and `dashscene-core` reads it back through
  `Arena::placeholder`, but **no producer lowers one**: the `dashc` CLI's only
  subcommand is `check`, so authoring one today means building a
  `dashc::Document` in code, which is what the tests do. Figma is not missing
  the vocabulary — `dashscene/role = placeholder` is a known annotation the
  importer already recognises and whose sample children it trims
  ([importer-trim-layers.md](../decisions/importer-trim-layers.md)) — but the
  lowering drops it, so `crates/dashc/src/figma/mod.rs` sets
  `placeholder: None`. Connecting the two is story #1264. **Nothing resolves it
  either** — no measure callback reads `declared_size` and no host binds a
  contribution. What is now reported is the disagreement between the two:
  `dashscene_validator::validate_contributions` takes a document and the host's
  bound contribution ids and names a placeholder no host fills
  (`placeholder.unfilled`, suppressed on a `Core` target that binds nothing a
  host contribution can fill) and a binding no placeholder declares
  (`placeholder.undeclared-overload`, on both profiles) — story #1127, and
  [a-host-binds-a-contribution-by-id.md](../decisions/a-host-binds-a-contribution-by-id.md).
  **No caller in this repository passes it a binding list**: hosts are its
  callers, and no entry point takes one, so its tests are its only caller today.
  Story #859's data plane, which this line named as the seam that would carry
  one, is not it — that runs outward, handing a host the committed tables.
  Placeholder _activation_ stays in v1
  ([05-qualification.md](../specification/05-qualification.md)). Why a nested
  table rather than four loose fields, and why `declared_size` is a measure size
  rather than a second box:
  [a-placeholder-is-a-table-and-declares-its-measure-size.md](../decisions/a-placeholder-is-a-table-and-declares-its-measure-size.md).
  This line said "already carries" while none of the four existed, until the
  v0.19 phase-end revision checked it (issue #876), and read "still to be added"
  until story #1126 built it. Re-derive rather than trusting it, with a form
  that prints a count rather than exiting 1 on no match:

      for f in contribution_id fragment_ref declared_size interim_fill; do
        printf '%s %s\n' "$f" "$(git grep -l "$f" -- crates/ | wc -l)"
      done

  Bound by R5 (cold-start cost proportional to what is shown) in
  [01-goals-and-requirements.md](../specification/01-goals-and-requirements.md)
  and `docs/archive/2026-07-14-design-1-seed.md` §10.2. The contract that will
  fill a placeholder at runtime is already designed:
  [runtime-content.md](../technotes/runtime-content.md) §7. Node replacement is
  an engine-painter-only concept, so it binds to the Unity painter row above as
  well.
- **Remote streaming (v2)** — bound today, not just later:
  [remoting-two-transports.md](../decisions/remoting-two-transports.md) already
  constrains the current producer API (handles vs. indices) so v2 does not
  become an API break.

**This is a deliberate deviation from the `sdd-working-memory-lifecycle` rule**,
taken on purpose, not an oversight. That rule says shipped docs describe the
system as-built and forward-looking concepts stay in `docs/wip/`. But boundary B
exists specifically so a painter is interchangeable — "painter swap = re-golden,
not redesign" — and one of the two painters that prove that claim now exists,
with Unity still to come. The claim stopped being a promise at the v0.15 close:
a second painter draws the whole vocabulary behind the same seam and one band
set serves both. Deleting the unbuilt entries from this record would delete the
reason boundary A and boundary B, the profile system, and the file/wire schema
split are shaped the way they are. Each unbuilt item above is marked planned and
traced to what binds it, which satisfies the rule's actual concern (that an
unbuilt thing not be described as built) without erasing the "why" from the
record.
