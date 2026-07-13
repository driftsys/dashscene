# dashc — the SCD compile pipeline

As-built after story #16 (v0.3). The rationale is in
`docs/decisions/dashc-scd-model-and-load-path.md`.

## The pipeline

    source              →  lower  →  Scd  →  validate  →  emit  →  .dsb
    (Figma REST JSON)                (in-memory document)

                                            ↓ (runtime)
    .dsb  →  root_as_document  →  validate_document  →  load_document  →  Arena  →  Painter

The `lower` step is **not built yet** — see "What is deferred" below.

## `Scd` — the in-memory document

What a producer lowers _into_, and what `emit` writes _out of_.

    Scd     { nodes: Vec<ScdNode>, images: Vec<ImageAsset> }
    ScdNode { name, parent: Option<u32>, box2d: Box2D, paint: Option<Paint> }
    Paint   { entry: dashpaint::PaintEntry, clip: bool }

Its paint types are **`dashpaint`'s** — boundary B's — not a third vocabulary.
One paint vocabulary spans the document, the runtime, and the painter, so a
lowering cannot invent a construct no painter can draw. What `Scd` adds is the
document's own shape: a flattened DFS node list whose array index is the
rect-table index (DESIGN §5), and layout intent — never results (P1).

`clip` travels beside the paint entry because the _schema_ pools it there
(`Paint.clip`), while the _arena_ carries it as node intent (`Prop::Clip`,
story #97). The pool key therefore includes it: two nodes sharing a style but
differing in clip are two document entries.

## `emit` — deterministic (R7)

Same `Scd` → byte-identical `.dsb`. Hashing, signing, and CI depend on it, so
nothing may depend on hash-map iteration order. The paint pool is the one place
that could, and it interns in **first-use DFS order** — the same rule
`dashscene-core`'s commit uses, so a document and the scene it loads into agree
on pool order too.

The interning key is a canonical bit encoding of the entry: `f32` goes in by
bit pattern, because `f32` is not `Eq`/`Hash` and NaN is not equal to itself, so
a value-keyed pool would emit a fresh entry per NaN and break reproducibility.

## `compile` — the emission gate (P4, R6)

    pub fn compile(scd: &Scd) -> Result<Vec<u8>, Report>

Emits, then validates the **document** — not the `Scd`. The load gate's rules
are about the serialized index model (a dangling `paint_entry`, an unknown
enum), so validating a shape the emitter has not produced yet would check
something other than what ships. An **error blocks the document**: the bytes are
discarded, never returned. A warning does not block (a strict build refuses it;
waivers are v0.7, #41).

## The load path

`dashscene_core::load_document` replays a document through the ordinary producer
API. **Loading adds no semantics**: a loaded scene is indistinguishable from the
same scene staged by hand, and the round-trip test asserts it — same rects, same
paint pool, same clip table, same image table, same pixels.

It is infallible by contract (P4), like the painter: it panics on an index that
misses, and the caller runs the two gates first (the flatbuffer verifier for
structure, then `validate_document` for references). `dashscene-validator` is
published after `dashscene-core`, so core cannot call it — the contract is
stated at the loader rather than enforced inside it.

**Image indices are remapped on load.** A document's indices are `0..n`, but an
arena that already holds assets gives out different ones; assuming they coincide
would repaint one document's nodes with another document's assets.

## The CLI

    dashc check <file.dsb>    run the load gate; exit 1 if the document is blocked

## What is deferred

**The Figma lowering.** It needs a captured fixture, and none exists —
`corpus/figma-fixtures/` holds only its manifest, and capturing needs a Figma
account and PAT (SCOPE §11). Guessing the REST shape would build the lowering
against a fiction, and P5 makes Figma fidelity this producer's entire purpose.
The `v03-paint` fixture-author plugin command exists to produce the capture; the
lowering is a pure function into `Scd` and slots in without disturbing anything
downstream of it.
