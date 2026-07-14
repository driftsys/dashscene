# dashc — the dashscene compile pipeline

As-built after stories #16, #139, and #17 (v0.3). The requirements are in
`docs/specification/dashc-figma-lowering.md`. The rationale is in
`docs/decisions/`:

- [dashc-document-model-and-load-path.md](../decisions/dashc-document-model-and-load-path.md)
  — the `Document` model, the load path, and why the Figma front end waited.
- [unsupported-figma-constructs-refuse-the-compile.md](../decisions/unsupported-figma-constructs-refuse-the-compile.md)
  — a construct `Document` cannot express is refused loudly, never approximated.
- [figma-auto-layout-refused-on-two-grounds.md](../decisions/figma-auto-layout-refused-on-two-grounds.md)
  — and why one of those grounds outlives debt #140.
- [figma-image-refs-resolved-by-the-caller.md](../decisions/figma-image-refs-resolved-by-the-caller.md)
  — image bytes arrive as a caller-supplied map.
- [producer-assembles-its-own-diagnostics.md](../decisions/producer-assembles-its-own-diagnostics.md)
  — `Report` gains public assembly.
- [dashc-wasm-abi.md](../decisions/dashc-wasm-abi.md) — the wasm boundary
  the Deno importer calls this pipeline through is five hand-written
  exports over a length-prefixed wire format, not wasm-bindgen or a
  flatbuffers envelope.

The Figma REST field shapes the capture pinned — several of which contradict
what the documentation suggests — are in
`docs/technotes/figma-rest-shapes-the-capture-pinned.md`.

## The pipeline

    source              →  lower  →  Document  →  emit  →  validate  →  .dsb
    (Figma REST JSON)                     (in-memory document)

                                            ↓ (runtime)
    .dsb  →  root_as_document  →  validate_document  →  load_document  →  Arena  →  Painter

`lower` is the `figma` module (below). `compile_figma` runs the whole top
row — source through `.dsb` — in one call; `compile` runs everything from
`Document` onward, for a document built by hand or by any other producer.

## `Document` — the in-memory document

What a producer lowers _into_, and what `emit` writes _out of_.

    Document { nodes: Vec<Node>, images: Vec<ImageAsset> }
    Node     { name: Option<String>, parent: Option<u32>, box2d: Box2D, paint: Option<Paint> }
    Paint    { entry: dashpaint::PaintEntry, clip: bool }

Its paint types are **`dashpaint`'s** — boundary B's — not a third vocabulary.
One paint vocabulary spans the document, the runtime, and the painter, so a
lowering cannot invent a construct no painter can draw. What `Document` adds is
the document's own shape: a flattened DFS node list whose array index is the
rect-table index (DESIGN §5), and layout intent — never results (P1).

`clip` travels beside the paint entry because the _schema_ pools it there
(`Paint.clip`), while the _arena_ carries it as node intent (`Prop::Clip`,
story #97). The pool key therefore includes it: two nodes sharing a style but
differing in clip are two document entries.

## `emit` — deterministic (R7)

Same `Document` → byte-identical `.dsb`. Hashing, signing, and CI depend on it, so
nothing may depend on hash-map iteration order. The paint pool is the one place
that could, and it interns in **first-use DFS order** — the same rule
`dashscene-core`'s commit uses, so a document and the scene it loads into agree
on pool order too.

The interning key is a canonical bit encoding of the entry: `f32` goes in by
bit pattern, because `f32` is not `Eq`/`Hash` and NaN is not equal to itself, so
a value-keyed pool would emit a fresh entry per NaN and break reproducibility.

## `compile` — the emission gate (P4, R6)

    pub fn compile(doc: &Document) -> Result<Vec<u8>, Report>

Emits, then validates **as a document** — not as a `Document`. The load gate's
rules are about the serialized index model (a dangling `paint_entry`, an
unknown enum), so validating a shape the emitter has not produced yet would
check something other than what ships. An **error blocks the document**: the
bytes are discarded, never returned. A warning does not block (a strict build
refuses it; waivers are v0.7, #41).

## The `figma` module

The only Figma-aware code in the Rust tree (P5): nothing downstream of it
knows what a `FRAME` or an `imageRef` is. It is pinned to the v0.3 fixture at
`corpus/figma-fixtures/v03-paint.json` — every field shape is real, not
guessed.

    figma::rest         the Figma REST JSON shape (serde types)
    figma::triage       maps Figma constructs onto dashscene_validator::Construct
    figma::lower        the Figma REST JSON → Document walk
    figma::image_refs   the imageRefs a lowering of a file will demand

`lower` does no I/O: `dashc` compiles to `wasm32-unknown-unknown`, so it
cannot fetch, and Figma serializes an image fill as a bare `imageRef` with no
bytes. The caller — the Deno importer — resolves refs and passes them in as
`images: &BTreeMap<String, ImageAsset>`.

`image_refs` walks the same subtree `lower` does — both fills and strokes,
from `root_frame` down — and returns the sorted, deduplicated set of
`imageRef`s a lowering of the file will need. The Deno importer calls it,
across the wasm ABI, rather than walking the JSON itself, so there is
exactly one place that knows where an `imageRef` lives in Figma's shape (P5;
`docs/decisions/figma-image-refs-resolved-by-the-caller.md`). It is
deliberately a superset: a paint it names may still be refused by the
lowering (a stacked fill, an invisible one), so fetching an unused image
costs a download, while missing one costs a failed compile.

The walk is depth-first, parent before child, from the first `FRAME` under
the first `CANVAS` (`root_frame`). Every other sibling and every later
canvas is currently dropped without a diagnostic (debt #147); a declared-roots
plus reachability-closure rule (DESIGN §6.1) is the v0.7 story.

### `CompileError`

Why a Figma file could not be compiled at all — distinct from a
`Diagnostic`, which is a verdict about a document the lowering understood:

    Parse(serde_json::Error)             not the Figma REST JSON it claimed to be
    Unsupported { path, what }           a construct the v0.3 Document cannot express at all
    UnresolvedImage { path, image_ref }  an imageRef the caller did not resolve
    Diagnostics(Report)                  an error-severity diagnostic blocked emission (R6)

`Unsupported` is the loud-refusal side of P4: a construct with no
`Construct` variant cannot become a `Diagnostic`, so dropping it in silence
is the only alternative to stopping the compile. The named gaps behind it are
tracked as debt — see "Scope boundaries" below.

## `compile_figma` — two gates, one report

    pub fn compile_figma(json: &str, profile: Profile, images: &BTreeMap<String, ImageAsset>)
        -> Result<(Vec<u8>, Report), CompileError>

The headline entry point: it runs the whole pipeline, source through `.dsb`,
and merges both gates into one report before deciding whether to emit.

- The **import gate** (`triage`) runs while lowering, on constructs `Document`
  can express but that DESIGN §10.1 puts outside the NOW band.
- The **load gate** (`validate_document`) runs on the emitted document, same
  as `compile`.

An error from either gate blocks emission (R6): `compile_figma` returns
`Err(CompileError::Diagnostics(report))`, and the bytes are discarded. A
warning does not block, so on success it comes back **with** the bytes —
discarding it would be the silent drop P4 forbids. This is also why
`compile_figma` does not simply call `compile` and forward its `Result`:
`compile` discards the load-gate report on success, which is exactly that
silent drop.

## The wasm ABI

Five hand-written `extern "C"` exports on the `dashc_wasm.wasm` cdylib
(`crates/dashc/src/abi/`) are what let the Deno importer run this pipeline —
not reimplement it (P5; SCOPE_DECISIONS.md §4):

    dashc_abi_version() -> u32
    dashc_alloc(len: u32) -> *mut u8
    dashc_free(ptr: *mut u8, len: u32)
    dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8
    dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8

`dashc_compile_figma` frames `compile_figma` above: the request carries the
profile, the Figma JSON, and the caller-supplied image map; the response
carries the `.dsb` bytes and the report on success, or the tagged
`CompileError` on failure. `dashc_figma_image_refs` frames `figma::image_refs`
the same way, with no blob in the response. Why the ABI is hand-written
rather than wasm-bindgen or a flatbuffers envelope, the wire format, the
allocation ownership rules, and why the response is a length prefix rather
than a `(ptr, len)` pair packed into a `u64`, are recorded in
`docs/decisions/dashc-wasm-abi.md`.

`crates/dashc/tests/abi.rs` drives the five exports natively — allocate,
write, call, decode, free — exactly as `importers/figma/src/wasm.ts` does, so
the wire format is pinned by a native `cargo test` with no wasm runtime in the
loop. Nothing behind the exports may panic: a malformed request decodes to a
`status: 2` response, never a trap, because a panic on
`wasm32-unknown-unknown` traps and kills the whole module instance.

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

`compile_figma` has no CLI subcommand: the acceptance path for that entry
point is a library call, consumed by the Deno importer (#17) through the
`wasm32` target, not by this native binary.

## Scope boundaries (v0.3)

Out of scope by design: widening `Document` to a wider vocabulary (flex layout,
text — debt #140), moving the negative-gap lowering out of core's `Txn`, and
a native `dashc compile` CLI subcommand (see "The CLI" above).

Known gaps in the Figma lowering, each a loud `CompileError::Unsupported`
rather than a silent drop (P4), filed as debt because a real Figma file will
hit them even though the v0.3 fixture does not:

- **Stacked fills or strokes** — `PaintEntry.fill`/`.stroke` are each one
  `Option`; Figma's `fills`/`strokes` are arrays (debt #146).
- **Node opacity, rotation, mask nodes, and hidden nodes** — `Document` has no
  field for any of them, and no way to represent a hidden node without
  shifting the DFS indices every later node depends on. Hidden layers are
  routine in real Figma files, so this is likely to be the first one hit
  (debt #143).
- **Baked shadows** — DESIGN §10.1 puts them in the NOW band, but `Document`
  has no effects vocabulary, so there is no `Construct` to triage onto and no
  field to lower into. Effects enter the schema at v0.8 (debt #144).
- **Auto-layout frames** — a `layoutMode` other than `NONE` (`HORIZONTAL`,
  `VERTICAL`, `GRID`). Two reasons, each sufficient on its own. `Document` has
  no flex vocabulary, so the intent — mode, gap, padding, sizing — has no field
  to lower into and no `Construct` to triage onto (debt #140). And inside an
  auto-layout frame, `absoluteBoundingBox` is what Figma's own flex solver
  computed, so lowering a child as a fixed rect would write a layout result
  into a document that carries only intent (P1). This one is not
  hypothetical: the root frame of `effects-2025.json` is auto-layout.
- **Dashed and non-`BASIC` strokes** — `dashpaint::Stroke` is solid and
  uniform: one color, one width, one align. A
  `complexStrokeProperties.strokeType` other than `BASIC`, or a non-empty
  `strokeDashes`, has nothing to lower into, so it is refused rather than
  repainted as a plain solid stroke of the same color (debt #145).
- **Root selection drops canvas siblings silently** — `root_frame` takes the
  first `FRAME` under the first `CANVAS`; every other sibling and every
  later canvas vanishes with no diagnostic. A declared-roots plus
  reachability-closure rule (DESIGN §6.1) is the v0.7 story (debt #147).

One gap sits half outside the lowering: **variable-width stroke** is on
SCOPE_DECISIONS §8's REJECT list, but `dashscene_validator::Construct` has no
variant for it, so a producer cannot triage it into a named diagnostic. The
lowering refuses it as a non-`BASIC` `strokeType` (above), so it is no longer
a silent drop; what remains missing is the diagnostic (debt #145). It is
`pendingManual` in the fixture manifest and absent from `effects-2025.json`,
so no captured fixture exercises it.

## Known limits of the as-built front end

Not expressiveness gaps — the lowering handles these inputs, but handles them
less well than it should. Each is filed as debt rather than fixed here:

- **The diagnostics found before a refusal are lost** (debt #149). `lower`
  returns `Err(CompileError::Unsupported)`, and the warnings it had already
  collected go with it, so a file carrying both a warning and an unsupported
  construct reports only the second. A designer fixing the refusal then meets
  the warning on the next run rather than both at once.
- **Nesting is capped at roughly 61 frames** (debt #148). `serde_json`'s
  default recursion limit bounds how deep the Figma tree may nest before the
  parse fails. Deeper than that is a parse error, not a lowering error, so the
  message does not explain itself.
- **A `NodePath` cannot distinguish duplicate sibling names** (debt #150). The
  path is the slash-joined ancestor-name chain, which is what a designer sees —
  but two siblings sharing a name produce the same path. The DFS index in the
  `NodePath` still differs, so the diagnostic is unambiguous to a machine, and
  ambiguous only to a human reading the name chain.
- **Root selection drops canvas siblings** (debt #147, also listed above).
