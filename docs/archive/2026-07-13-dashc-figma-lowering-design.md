# dashc — the Figma REST JSON lowering (design)

Story #139, the deferred half of #16. Epic #12 (v0.3 — basic paint +
importer).

## Context

PR #137 shipped everything downstream of the lowering: the `Scd`
in-memory document, the deterministic `.dsb` emitter, the emission gate,
and the `dashscene-core` load path with a rendering round-trip. The Figma
front end was deliberately left out, because no Figma fixture had ever
been captured and guessing the REST shape would have built the lowering
against a fiction. P5 makes Figma fidelity this producer's entire
purpose.

PR #142 (merged 2026-07-13) captured the tier-1 corpus. The block is
cleared: `corpus/figma-fixtures/v03-paint.json` is the designated input
for this story, and `corpus/figma-fixtures/effects-2025.json` is the
diagnostic fixture.

    Figma REST JSON  ->  lower  ->  Scd          <- this story
                                     |
                          validate -> emit -> .dsb    <- shipped in #137

Every lowering rule below is pinned by the captured fixture. Nothing here
is inferred from documentation.

## Decisions

### D1 — image bytes arrive as a caller-supplied map

Figma serialises an image fill as a bare `imageRef` hash. The file JSON
carries no image bytes and no top-level `images` map, so resolving a ref
needs a second REST call (`GET /images`) plus a download. `dashc` compiles
to `wasm32-unknown-unknown` (CI enforces it) and therefore cannot fetch.

`Scd.images` holds real PNG bytes, and the load gate rejects a zero-byte
asset (`asset.image-no-bytes`), so the bytes must exist before `compile`
runs. A two-phase "lower now, attach bytes later" contract would fail its
own gate.

**Decision.** The canonical JSON keeps Figma's raw shape — the `imageRef`
stays a ref — and the caller resolves refs and passes the bytes in
alongside:

    pub fn lower(
        file: &FigmaFile,
        profile: Profile,
        images: &BTreeMap<String, ImageAsset>,
    ) -> Result<(Scd, Vec<Diagnostic>), CompileError>;

The `imageRef` key is a Figma concept and `lower` is the Figma front end,
so the Figma-shaped key stays inside the Figma producer (P5). It never
reaches `Scd` or `compile`. The call is synchronous, so `dashc` stays
wasm-clean, and this does not prejudge the wasm ABI — `compileViaWasm` is
a v0.7 stub with an untyped parameter, and #17 can carry bytes across
however it likes.

In this story nobody fetches: the test supplies one synthetic PNG for the
fixture's single `imageRef`. Real resolution lands with #17.

**Alternatives considered.** Inlining base64 image bytes into the
canonical JSON: one input, but it inflates the payload by roughly a third,
adds a base64 decoder to `dashc`, mixes assets into the document JSON, and
pins the wasm ABI before #17 exists. Capturing the image bytes into the
corpus: highest record-and-replay fidelity, but it re-blocks the story on a
human capture step and adds assets that debt #141 says churn on every
re-capture. Skipping image fills: drops the one construct the fixture's
`image-fit` node was authored to cover, and contradicts the manifest's
`emits: true`.

### D2 — `Report` gains public assembly

`triage` returns a bare `Diagnostic`. `Report::push` is `pub(crate)`, so
`dashc` cannot assemble a `Report` from triage output, and there is no
public constructor, `FromIterator`, or `From<Vec<Diagnostic>>`.

This is a gap rather than a deliberate constraint: the validator's own
decision record (`docs/decisions/validator-three-gates.md`) assigns the
Figma-to-`Construct` mapping to `dashc`, then gives it no way to report the
result.

**Decision.** Add `impl FromIterator<Diagnostic> for Report` and
`impl Extend<Diagnostic> for Report` to `dashscene-validator`. `Report`
stays the single diagnostic container across both gates.

### D3 — fixed layout only; `Scd` is not widened

The `v03-paint` fixture is `layoutMode: NONE` throughout, so the paint
vocabulary lowers into `Scd` with no schema or `Scd` change. Debt #140
(`Scd` cannot express flex layout or text) stays open and untouched.

### D4 — the Figma-not-CSS lowerings stay in core's `Txn`

`Txn::lower_negative_gaps` lives on the arena in `dashscene-core`. #16's
scope note asked whether it moves into a lowering module here — the
negative-gap record's revisit trigger.

**Decision.** It does not move. The trigger fires on auto-layout, and
`v03-paint` contains none. Moving the pass now would be speculative: it
operates on the arena, while a document-side pass would have to operate on
`Scd`, which cannot express flex in the first place (D3). Revisit when
flex lowering lands.

## The lowering

### Tree walk

Root is the first `FRAME` child of the first `CANVAS`. DFS from there;
`Scd.push` order is the rect-table index, and `emit` does not reorder.

### Geometry

Figma's `absoluteBoundingBox` is page-absolute; `Scd.Box2D` is
parent-relative intent. The lowering owns the subtraction. The root drops
its page position and becomes `(0, 0, w, h)` — where a frame sits on the
Figma canvas is a page-layout artifact, not intent.

`absoluteRenderBounds` is **ignored**. It is a result, not intent (P1): in
the fixture it differs from the bounding box exactly by the stroke
expansion, and using it would bake a painter's output back into the
document.

### Paint

| Figma                                         | `Scd` / `dashpaint`                                                                                                                            |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `fills[0]` `SOLID`                            | `PaintKind::Solid`                                                                                                                             |
| `fills[0]` `GRADIENT_*`                       | `PaintKind::Gradient` — `gradientHandlePositions` copies across verbatim, since `dashpaint::Gradient` already stores Figma's handle convention |
| `fills[0]` `IMAGE`                            | map lookup on `imageRef` -> `push_image` -> `PaintKind::Image` with `scaleMode`                                                                |
| `strokes[0]` + `strokeWeight` + `strokeAlign` | `Stroke`                                                                                                                                       |
| `cornerRadius`                                | uniform `CornerRadii`                                                                                                                          |
| `rectangleCornerRadii`                        | per-corner `CornerRadii`                                                                                                                       |
| `clipsContent`                                | `Paint.clip`                                                                                                                                   |

Two shapes the fixture settles that a guess would plausibly have got
wrong:

- `cornerRadius` and `rectangleCornerRadii` are **mutually exclusive** —
  Figma nulls whichever does not apply. `rectangleCornerRadii` is
  `[top_left, top_right, bottom_right, bottom_left]`, which matches
  `dashpaint::CornerRadii`'s field order.
- `strokeWeight` and `strokeAlign` are present **even when `strokes` is
  empty**. The stroke lowering must gate on a non-empty `strokes` array,
  not on the presence of a weight.

### Triage — the import gate

The producer owns the mapping; the validator owns the verdict (P5).
`triage` is called only for constructs outside the NOW band; in-profile
constructs are simply lowered.

| Figma                                                        | `Construct`            | Verdict            |
| ------------------------------------------------------------ | ---------------------- | ------------------ |
| `effects[].type` `NOISE`, `TEXTURE`                          | `NoiseOrTextureEffect` | Error              |
| `effects[].type` `LAYER_BLUR` with `blurType: "PROGRESSIVE"` | `ProgressiveBlur`      | Error              |
| `effects[].type` `LAYER_BLUR` otherwise                      | `LayerBlur`            | Warning            |
| `effects[].type` `BACKGROUND_BLUR`                           | `BackdropBlur`         | Error under `Core` |
| `cornerSmoothing > 0`                                        | `CornerSmoothing`      | Warning            |
| `blendMode` not `NORMAL` or `PASS_THROUGH`                   | `AdvancedBlendMode`    | Error under `Core` |

Figma carries a `blendMode` on the node **and** on each paint. The rule
applies to both: a node whose `blendMode` is neither `NORMAL` nor
`PASS_THROUGH`, and a fill or stroke whose `blendMode` is not `NORMAL`,
each triage as `AdvancedBlendMode`. Frames in the fixture carry
`PASS_THROUGH` and every paint carries `NORMAL`, so nothing in `v03-paint`
triggers it — the rule exists so that a blend mode is never dropped in
silence (P4).

The `LAYER_BLUR` split is not a refinement invented here — it is what the
capture forced. `effects-2025.json` expresses progressive blur as a
`LAYER_BLUR` carrying `blurType: "PROGRESSIVE"`, so the type alone cannot
decide the band: plain layer blur warns, progressive blur rejects.

A `Diagnostic` carries a `NodePath` built from the node's DFS index and its
slash-joined ancestor name chain, which is what a designer sees.

### Emission gate

    pub enum CompileError {
        Parse(serde_json::Error),
        Unsupported { path: String, what: String },
        UnresolvedImage { path: String, image_ref: String },
        Diagnostics(Report),
    }

    pub fn compile_figma(
        json: &str,
        profile: Profile,
        images: &BTreeMap<String, ImageAsset>,
    ) -> Result<(Vec<u8>, Report), CompileError>;

It parses, lowers, then merges the import-gate diagnostics with the
load-gate `Report` before emitting. An error from either gate blocks
emission (R6). A single entry point means a caller cannot forget to check
the import diagnostics, which it could if it called `lower` and then
`compile` separately.

`CompileError` carries the three outcomes a bare `Report` cannot express: a
**parse failure** is not a diagnostic; an **unresolved `imageRef`** cannot be
faked, because the load gate rejects a zero-byte asset; and a construct the
v0.3 `Scd` **cannot express** (a stacked fill, a shadow, a non-`FRAME` node)
has no `Construct` variant, so it cannot become a `Diagnostic` at all — and
P4 forbids dropping it in silence, so it stops the compile.

Success returns the bytes **and** the report, rather than only the bytes.
Warnings do not block emission, but discarding them on the success path would
be exactly the silent drop P4 forbids.

## Known gaps, filed as debt rather than papered over

**Stacked fills.** `PaintEntry.fill` is one `Option<PaintKind>`; Figma's
`fills` is an array. Every node in the fixture carries exactly one fill, so
this story is unaffected — but a stacked-fill node would be a silent drop,
which P4 forbids. It is an `Scd` expressiveness gap of the same genre as
debt #140, not a triage gap, so it gets a debt issue rather than an
invented `Construct` variant. Node `opacity`, `rotation`, and
`visible: false` are the same case.

**Variable-width stroke** sits on SCOPE §8's REJECT list but has no
`Construct` variant. It is `pendingManual` in the fixture manifest and
genuinely absent from `effects-2025.json`, so this story cannot test it.
Debt.

## Scope boundaries

Out of scope: widening `Scd` (D3), moving the negative-gap pass (D4), a
`dashc compile` CLI subcommand (acceptance needs a library path, and #17
consumes it through wasm; only `main.rs`'s stale "fixture not captured"
help text gets fixed), and text or auto-layout of any kind.

## Acceptance

1. `v03-paint.json` -> `compile_figma` -> `.dsb` -> loads in
   `dashscene-core` -> renders through `SkiaPainter`, in a test.
2. Per-construct assertions: the three stroke aligns, both corner forms,
   the four gradient kinds, the clip frame, and the `(-60, -30)`
   parent-relative offset of `overflow-child`.
3. `effects-2025.json` is refused, and the report names all three REJECT
   constructs as errors.
4. Emission from the fixture stays byte-reproducible (R7).
5. `just wasm` and `just build` green.

## Documentation to update

The doc comments in `crates/dashc/src/lib.rs` and `crates/dashc/src/main.rs`
and the note in `docs/design/dashc.md` all still say the v0.3 fixture has
not been captured. PR #142 made that stale.
