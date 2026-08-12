# dashc emits from an in-memory document; the loader lives in dashscene-core

    status   accepted (story #16, 2026-07-13); the deferral in §1 was
             discharged by story #139
    scope    crates/dashc, crates/dashscene-core, crates/dashpaint
    binds    #17 (the Deno importer calls this pipeline through wasm),
             #41, #107

## Context

Story #16 asked for "Figma REST JSON (fixture) in → validate → lower →
deterministic `.dsb` out", with the acceptance criterion "fixture JSON → `.dsb`
→ loads in dashscene-core → renders via the Skia painter".

Three things had to be settled: what a producer lowers _into_, where the
`.dsb`→runtime loader lives, and what to do about the fact that **no Figma
fixture has ever been captured** — `corpus/figma-fixtures/` holds only its
manifest, and capturing needs a Figma account and PAT
(`docs/decisions/figma-access-plan-and-pat-policy.md`).

## Choice

### 1. The Figma front end is deferred, deliberately (discharged at #139)

**Discharged.** PR #142 captured the tier-1 corpus, and story #139 built the
lowering against it. The reasoning below is why it waited, and it held: every
field shape the lowering reads was pinned by the capture, and several of them
(the mutually exclusive corner-radius fields, `strokeWeight` present with an
empty `strokes` array, a dashed stroke that still reports `strokeType: "BASIC"`)
contradict what a careful reading of the documentation would have produced. See
`docs/technotes/figma-rest-shapes.md`.

The lowering is **not** built in this slice. Guessing the REST JSON shape —
`absoluteBoundingBox` vs `absoluteRenderBounds`, when `cornerRadius` collapses
to `rectangleCornerRadii`, how `strokeAlign`/`strokeWeight` serialize, the
gradient-handle convention, image fills being an `imageRef` that needs a
separate `GET /images` call — would build the lowering against a fiction, and P5
makes Figma fidelity this producer's entire purpose. A lowering verified against
invented input is worth less than no lowering.

So this slice ships everything _downstream_ of the lowering, which is most of
the engineering and all of the shared machinery: the document model, the
deterministic emitter, the emission gate, and the validated round trip through
`dashscene-core` and the reference painter. The lowering is a pure function into
`Document` and slots in against a real fixture without disturbing any of it.

The `v03-paint` fixture-author plugin command exists to produce that capture.

### 2. `Document` — the in-memory document — uses `dashpaint`'s paint types

A producer lowers into `Document`, and `emit` writes `.dsb` out of it. Its paint
types are **boundary B's**, not a third set: one paint vocabulary spans the
document, the runtime, and the painter, so a lowering cannot invent a construct
no painter can draw. What `Document` adds is the _document's_ shape — the
flattened DFS node list whose index is the rect-table index, layout intent
(never results, P1), and the pools.

### 3. The loader lives in `dashscene-core`, and loading adds no semantics

`load_document` replays a document through the ordinary producer API (`add_node`
/ `set_prop` / `commit`). It is not a second way to build a scene: a loaded
scene is **indistinguishable** from the same scene staged by hand, and the
round-trip test asserts exactly that — same rects, same paint pool, same clip
table, same image table, same pixels.

`dashscene-core` gains a `dashbuf` dependency for it. The publish order already
allows this (`dashbuf → dashpaint → dashscene-core`), and it is the right home:
`docs/design/architecture.md` makes `.dsb` the format the _runtime_ mmaps, so
reading it is a core runtime capability, not a compiler-only one. Putting the
loader in `dashc` would force every runtime that wants to load a document to
depend on the compiler CLI.

### 4. The loader is infallible by contract (P4), like the painter

It does not re-check referential integrity and it panics on an index that misses
— the same contract as `PaintTable::resolve` and the `Painter` trait. The caller
runs the gates first, and there are two:

    root_as_document(bytes)?      // flatbuffer verifier: structure
    validate_document(&doc)       // load gate: references, enums, vocabulary
    load_document(&doc, &mut arena)   // safe iff the gate passed

`dashscene-validator` is published _after_ `dashscene-core`, so core cannot call
it — which is why the contract is stated at the loader rather than enforced
inside it. `dashc::compile` runs both gates, and an **error blocks the
document** (R6): the bytes are discarded, never returned.

### 5. The arena owns the image table

A `.dsb` carries its assets (`Document.images`), so a loaded scene has to be
self-contained. `Txn::add_image` stages an asset; `CommittedScene::images()`
hands it to the painter alongside `paints()` and `clips()`. There was nowhere
else for a loaded document's assets to live.

**Image indices are remapped on load.** A document's indices are `0..n`, but an
arena that already holds assets gives out different ones. Assuming they coincide
would repaint one document's nodes with another document's assets — a test pins
the remapping.

### 6. `Prop` widened, and the paint-interning key with it

Core's producer API carried only `Fill(Color)`, so no producer could express a
gradient, a stroke, or an image fill (issue #130) — the loader needed all three.
`Prop` gains `FillWith(PaintKind)` and `Stroke(Stroke)`; `Fill(Color)` stays as
the solid shorthand, and because both write **one** field they cannot disagree
the way the document's `paint`/`paint_entry` pair could (issue #63).

The commit's paint-interning key was `(fill color, corners)` with an
`unreachable!` on any other fill (issue #131). That would have panicked in the
dirty-set diff — one commit away from the producer that staged the gradient —
the moment this widening landed. The key is now a canonical bit encoding of the
**whole** `PaintEntry`, so the next vocabulary widening costs one match arm and
nothing else.

## Consequences

- **The Figma lowering was the remaining half of #16**, and story #139 built it
  against the `v03-paint` capture. `Document`, `emit`, and `compile` were
  unchanged by it, which is the claim §1 made: the lowering slotted in as a pure
  function into `Document`. Its own decisions are in
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`,
  `figma-auto-layout-refused-on-two-grounds.md`,
  `figma-image-refs-resolved-by-the-caller.md`, and
  `producer-assembles-its-own-diagnostics.md`.
- **#17 (the Deno importer)** calls this pipeline through `dashc.wasm`. Its
  "byte-identical to dashc-native output" criterion rests on the emitter's
  determinism (R7), which is tested here.
- The document pools clip with the paint entry (`Paint.clip`) while the arena
  carries clip as node intent (`Prop::Clip`, issue #97). The emitter's pool key
  therefore includes the clip flag: two nodes sharing a style but differing in
  clip are two document entries. Reconciling the two representations — moving
  clip to `Node` in the schema — is a schema change, deliberately not made here.
