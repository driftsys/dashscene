# dashc — the dashscene compile pipeline

As-built after stories #16, #139, #17 (v0.3), #140 (v0.7 — the flex
lowering), #167 (v0.7 — the binding tables), and #264 (v0.8 — the
grid/wrap/baseline layout lowering). The requirements are in
`docs/specification/06-dashc-figma-lowering.md`. The rationale is in
`docs/decisions/`:

- [dashc-document-model-and-load-path.md](../decisions/dashc-document-model-and-load-path.md)
  — the `Document` model, the load path, and why the Figma front end waited.
- [unsupported-figma-constructs-refuse-the-compile.md](../decisions/unsupported-figma-constructs-refuse-the-compile.md)
  — a construct `Document` cannot express is refused loudly, never approximated.
- [figma-auto-layout-refused-on-two-grounds.md](../decisions/figma-auto-layout-refused-on-two-grounds.md)
  — the v0.3 refusal #140 lifted, and the P1 ground that outlives it.
- [figma-flex-lowering.md](../decisions/figma-flex-lowering.md) — the #140
  lowering's shape: per-axis intent, the walk-side negative-gap rewrite,
  and (D5, un-pinned at #264) the grid/wrap/baseline lowering onto story
  #43's schema fields.
- [v08-layout-vocabulary-shape.md](../decisions/v08-layout-vocabulary-shape.md)
  — the v0.8 layout schema fields #264 lowers onto: grid tracks as
  `Fixed`/`Fraction` vectors, per-child placement, the one cross gap.
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

    source              →  lower  →  Document  →  emit  →  validate  →  package  →  .dsb
    (Figma REST JSON)                     (in-memory document)                (envelope)

                                            ↓ (runtime)
    .dsb  →  ui_document  →  root_as_document  →  validate_document  →  load_document  →  Arena  →  Painter
             (envelope)

`lower` is the `figma` module (below). `compile_figma` runs the whole top
row — source through `.dsb` — in one call; `compile` runs everything from
`Document` onward, for a document built by hand or by any other producer.

`package` and `ui_document` are the two ends of the file envelope, added at
v0.11 (`docs/design/dsb-container-format.md`). Everything between them works on
the ui document as a bare flatbuffer, which is what a structured section
carries; only the bytes crossing in or out of a file go through the envelope.
`emit` returns a section payload, `compile` returns a file, and
`root_as_document` is never called on `.dsb` file bytes directly.

## `Document` — the in-memory document

What a producer lowers _into_, and what `emit` writes _out of_.

    Document { nodes: Vec<Node>, images: Vec<ImageAsset> }
    Node     { name: Option<String>, parent: Option<u32>, box2d: Box2D, paint: Option<Paint>,
               container: Option<LayoutContainer>, constraints: Option<LayoutConstraints> }
    Paint    { entry: dashpaint::PaintEntry, clip: bool }

`container` and `constraints` (story #140) mirror the schema's two v0.2
flex tables (`docs/decisions/flex-vocabulary-shape.md`) as plain types, the
way `dashscene-core`'s arena mirrors them: `None` is the schema's absent
table, so a fixed-layout document emits byte-identically to before the
vocabulary was carried (R7 — the frozen goldens pin it). `Box2D` is
per-axis intent: under a flex parent the offsets are solver-owned and lower
as zeros, and the extents are the datum only a `Fixed` axis reads
(`docs/decisions/figma-flex-lowering.md`).

Its paint types are **`dashpaint`'s** — boundary B's — not a third vocabulary.
One paint vocabulary spans the document, the runtime, and the painter, so a
lowering cannot invent a construct no painter can draw. What `Document` adds is
the document's own shape: a flattened DFS node list whose array index is the
rect-table index (`docs/design/dashbuf.md`), and layout intent — never
results (P1).

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
    figma::bindings     the joined variable-binding rows → binding tables
                        (story #167)
    figma::image_refs   the imageRefs a lowering of a file will demand

`lower` does no I/O: `dashc` compiles to `wasm32-unknown-unknown`, so it
cannot fetch, and Figma serializes an image fill as a bare `imageRef` with no
bytes. The caller — the Deno importer — resolves refs and passes them in as
`images: &BTreeMap<String, ImageAsset>`.

`image_refs` walks the same subtrees `lower` does — both fills and strokes,
every top-level node's subtree with component definitions included — and
returns the sorted, deduplicated set of `imageRef`s a lowering of the file will
need. The Deno importer calls it, across the wasm ABI, rather than walking the
JSON itself, so there is exactly one place that knows where an `imageRef` lives
in Figma's shape (P5; `docs/decisions/figma-image-refs-resolved-by-the-caller.md`).
It is deliberately a superset: a paint it names may still be refused by the
lowering (a stacked fill, an invisible one) or sit in a definition the lowering
resolves but does not paint, so fetching an unused image costs a download, while
missing one costs a failed compile.

The walk is depth-first, parent before child. Every top-level node under every
`CANVAS` is a document root (`top_level_nodes`): a declared-roots export
computes exactly the set to pass, so the walk no longer selects one
positionally, and a component definition among the roots resolves but does not
paint (`docs/decisions/figma-component-lowering.md`, #242). This deleted the
v0.3 first-`FRAME`-under-the-first-`CANVAS` selection (debt #147) that dropped
every sibling and every later canvas without a diagnostic; declared roots plus
the reachability closure (`importers/figma/src/closure.ts`) are the
importer-side half of that debt. The walk is
iterative — an explicit stack, not recursion — so tree depth costs heap, never
call stack (debt #148); the matching parse-side bound is `MAX_JSON_DEPTH`, a
documented 256-level limit enforced by a linear pre-scan whose refusal names
both depths, in place of serde_json's opaque default.

A construct the document vocabulary cannot express is an error-severity
diagnostic under `dashc`'s own rule id `figma.unsupported`, and the walk
continues — the node's subtree is skipped, never lowered approximately, and
one pass reports every finding (debt #149; the mechanism revision is in
`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`). A
diagnostic's path disambiguates duplicate sibling names with the node's
Figma id — `Frame 1 (1:23)` — or its child position when a synthetic node
has no id (debt #150).

### The image gate — `image_id`

Since v0.11 (story #400) every image entering through the `images` map is
identified before it becomes a document asset. `crates/dashc/src/image_id.rs`
matches the PNG, JPEG, and GIF signatures and parses just the header for the
intrinsic width and height. It **never decodes** — no inflate, no entropy
decode, no LZW — and that boundary is permanent, because pixel reconstruction is
the part of a codec that carries the CVEs
(`docs/decisions/dashc-identifies-images-never-decodes.md`).

Before this, the producer's format tag was verified only on the Deno path and
not at all through the native compile API, so a mistagged payload reached a
painter's decoder before anything noticed. Four error-severity rules close it,
each naming the `imageRef`:

    figma.image-unknown-signature   the bytes match no known signature
    figma.image-format-mismatch     the signature contradicts the producer's tag
    figma.image-header-malformed    truncated, inconsistent, or a refused JPEG frame marker
    figma.image-zero-dimension      the header parses and reports a zero extent

All four are errors under both emit policies: an image that cannot be identified
has no approximation to degrade to, so there is nothing for
`EmitPolicy::Partial` to soften. The importer's own `isPng`/`isJpeg`/`isGif` stay
as a courtesy pre-flight.

One path into `Document.images` is exempt, and says so at its call site: the
MSDF vector atlas PNG the compiler generates itself. Nobody asserts its format,
so there is no tag to verify.

### `CompileError`

Why a Figma file could not be compiled at all — distinct from a
`Diagnostic`, which is a verdict about a document the lowering understood:

    Parse(serde_json::Error)             not the Figma REST JSON it claimed to be, or past MAX_JSON_DEPTH
    Unsupported { path, what }           a file shape the walk cannot start on (no root FRAME)
    UnresolvedImage { path, image_ref }  an imageRef the caller did not resolve
    Diagnostics(Report)                  an error-severity diagnostic blocked emission (R6)

Since #140 an unsupported _construct_ is a `figma.unsupported` diagnostic,
not a `CompileError` — see the walk description above. The named gaps are
tracked as debt; see "Scope boundaries" below.

## `compile_figma` — two gates, one report

    pub fn compile_figma(json: &str, profile: Profile, images: &BTreeMap<String, ImageAsset>)
        -> Result<(Vec<u8>, Report), CompileError>

The headline entry point: it runs the whole pipeline, source through `.dsb`,
and merges both gates into one report before deciding whether to emit.

- The **import gate** (`triage`) runs while lowering, on constructs `Document`
  can express but that `docs/specification/04-figma-vocabulary-profile.md`
  puts outside the NOW band.
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
not reimplement it (P5;
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`):

    dashc_abi_version() -> u32
    dashc_alloc(len: u32) -> *mut u8
    dashc_free(ptr: *mut u8, len: u32)
    dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8
    dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8

`dashc_compile_figma` frames `compile_figma_with_bindings` above: the
request (wire version 2) carries the profile, the Figma JSON, the
caller-supplied image map, and the joined binding rows; the response
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

## Bindings (v0.7, story #167)

`compile_figma_with_bindings` takes the importer's joined
variable-binding rows beside the JSON and images (the compile request's
ABI v2 section). `figma::bindings::apply` maps each row's Figma node id
onto the document node the walk lowered it to, and its property path
onto a binding channel: `itemSpacing` becomes a `Gap` row, a solid
fill's `fills[i].color` becomes four `Fill` channel rows with `.r/.g/
.b/.a` signal suffixes. Signals intern by name in first-use row order
(R7). A path with no channel yet, or a row targeting an unlowered node,
is a named warning — the resolved literal still ships (P4); an
inconsistent signal declaration is a named error. `Document` mirrors the
schema's tables as plain types (`SignalDecl`, `Binding`,
`BindingChannel`, `BindingTransform`), and `BindingTransform::Custom`
reaching `compile` is the named error `binding.custom-transform` —
a closure does not serialize, so the document is refused, never emitted
approximately (`docs/decisions/binding-table-in-the-document.md`).

## Scope boundaries (v0.7, after #140)

Out of scope by design: text lowering (story #160), moving the negative-gap
lowering out of core's `Txn` (re-checked at #140 —
`docs/decisions/negative-gap-lowering.md`), and a native `dashc compile` CLI
subcommand (see "The CLI" above).

Known gaps in the Figma lowering, each a named `figma.unsupported` error
diagnostic rather than a silent drop (P4), filed as debt or scheduled where
noted, because a real Figma file will hit them:

- **Stacked strokes** — `PaintEntry.stroke` is one `Option`; Figma's
  `strokes` is an array (debt #146 remainder). Stacked _fills_ lower since
  story C1 (epic #343): `paint_of` reads every visible entry of a plain
  frame/rectangle's `fills` array in order onto `PaintEntry.fill` (the
  bottom layer) and `PaintEntry.extra_fills` (the rest), each already
  carrying its own baked-in opacity. An ellipse, a baked vector, and a text
  glyph color keep the single-fill refusal — the measured need was a plain
  frame/rectangle.
- **Node rotation** — `Document` has no rotation vocabulary, so a rotated
  node is a named refusal (debt #143 remainder). Node opacity, mask nodes,
  and hidden nodes were un-pinned at v0.8 (story #44): they lower into
  `Node.opacity` / `Node.mask` / `Node.visible`. A box outline mask lowers;
  a soft alpha or luminance mask, and a text-shaped mask, refuse by name
  (`docs/decisions/masks-and-group-opacity.md`).
- **Baked shadows** — drop and inner shadows lower at v0.8 (story #45,
  debt #144 resolved): `shadows_of` reads a node's visible
  `DROP_SHADOW`/`INNER_SHADOW` effects (color, offset, radius, spread) onto
  the paint entry's `shadows` list, in Figma's effect order. A hidden effect
  is skipped; a non-`NORMAL` shadow blend mode is an advanced-blend
  diagnostic; a shadow with no color refuses by name. Noise, texture, and
  progressive blur stay REJECT (`docs/decisions/effects-vocabulary-shadows.md`).
- **Wrap line distribution and a negative wrap gap** —
  `counterAxisAlignContent: SPACE_BETWEEN` has no `align_content`
  vocabulary yet, and a negative `itemSpacing` on a `WRAP` frame has no
  margin encoding (wrap breaks its lines after the lowering). Each is a
  named refusal, both appearing with the v0.8 grid/wrap/baseline un-pin
  (story #264, `docs/decisions/v08-layout-vocabulary-shape.md` D5). A grid
  track token the `Fixed`/`Fraction` vocabulary cannot express (an `auto`,
  a `minmax` with a non-zero minimum) is likewise refused by name.
- **A `Fill` child on an axis its parent hugs** — Figma and CSS resolve the
  sizing cycle differently, so it is refused rather than solved to a
  picture Figma never rendered (`docs/decisions/figma-flex-lowering.md`
  D5; pinned by `variables-bound.json`).
- **Absolutely-positioned flex children, layout-consuming strokes, and
  reversed paint order** — `layoutPositioning: ABSOLUTE`,
  `strokesIncludedInLayout: true`, `itemReverseZIndex: true` have no
  vocabulary; each would silently reflow or repaint siblings if defaulted.
- **Dashed and non-`BASIC` strokes** — `dashpaint::Stroke` is solid and
  uniform: one color, one width, one align. A
  `complexStrokeProperties.strokeType` other than `BASIC`, or a non-empty
  `strokeDashes`, has nothing to lower into, so it is refused rather than
  repainted as a plain solid stroke of the same color (debt #145).
- **Non-`FRAME` nodes** — `TEXT` is story #160, `ELLIPSE`→circle is #239,
  `INSTANCE` is #242, and `RECTANGLE` (a paint-bearing leaf beside `ELLIPSE`)
  plus `SECTION`/`GROUP` (absolute containers, admitted through the same
  container branch a `layoutMode`-less `FRAME` already takes) are #309
  (`docs/decisions/figma-rectangle-and-structural-containers.md`). The
  remaining shape nodes — `VECTOR`, `LINE`, `STAR`, `REGULAR_POLYGON`,
  `BOOLEAN_OPERATION` — carry bezier/path geometry the `.dsb` schema does not
  model yet, so they stay refused by name; admitting them is a distinct,
  larger vocabulary effort.
- **A `SECTION` with hidden contents** — `sectionContentsHidden: true` has no
  vocabulary (the document cannot express "this container's children are
  hidden"), so it is a named refusal (#309) rather than a silent render of
  content Figma hides.

One gap sits half outside the lowering: **variable-width stroke** is on
`docs/specification/04-figma-vocabulary-profile.md`'s REJECT list, but `dashscene_validator::Construct` has no
variant for it, so a producer cannot triage it into a named diagnostic. The
lowering refuses it as a non-`BASIC` `strokeType` (above), so it is no longer
a silent drop; what remains missing is the diagnostic (debt #145). It is
`pendingManual` in the fixture manifest and absent from `effects-2025.json`,
so no captured fixture exercises it.

## Known limits of the as-built front end

Not expressiveness gaps — the lowering handles these inputs, but handles them
less well than it should:

- **A hug container over a lowered negative gap solves collapsed** (engine
  debt #236). The lowering's output is correct; Taffy 0.12's intrinsic
  sizing mis-sums negative margins. Found verifying #105 at story #140.
- **A `BASELINE` text row does not solve to Figma's captured boxes** (debt
  #273). The lowering is exact (`counterAxisAlignItems: BASELINE` ->
  `CrossAxisAlign::Baseline`, story #264), but the engine's leaf baseline
  is the box bottom, not the glyph baseline, so a baseline row of
  mixed-size text aligns on box bottoms instead. `lowering-baseline.json`'s
  lowered intent is pinned; its solved rects are not asserted against the
  capture, and the divergence is named rather than concealed. Found
  un-pinning `BASELINE` at story #264.

Resolved at story #140, folded in from the v0.3 debt list: the
diagnostics-lost-on-refusal limit (#149 — one pass now reports every
finding), the 61-frame nesting cap (#148 — iterative walk plus the
documented `MAX_JSON_DEPTH` pre-scan), and the ambiguous duplicate-sibling
path (#150 — the Figma id disambiguates).
