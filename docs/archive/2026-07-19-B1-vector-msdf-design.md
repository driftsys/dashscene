# B1 design — Figma VECTOR shapes lower into a baked MSDF field

```text
status   WIP working memory (design + plan). Grounding + design phase of
         story B1 (#340), epic #343 (v0.10 real-file fidelity). Archived
         at story close.
goal     A Figma VECTOR node lowers into the dashscene document as a baked
         multi-channel signed-distance field carried on the paint entry as
         a coverage mask, so first-light's bolt and arrows and the hero's
         148 vectors render instead of skip-with-warning. P1/P2/P4 hold:
         the document carries the baked field as intent (not a rasterized
         result the way a resolved pixel would be — the field is a
         resolution-independent shape source, like the glyph atlas), the
         painter only samples and composes, and an unfieldable shape is a
         named diagnostic.
refs     issue #340 (approved design gate, 2026-07-19); epic #343;
         docs/wip/2026-07-19-epic-v010-real-file-fidelity.md (story B1);
         docs/decisions/paint-entry-composition.md;
         docs/decisions/runtime-vector-via-thorvg-to-texture.md;
         docs/technotes/msdf-arabic-atlas-spike.md;
         crates/dashscene-typeset/src/atlas/ (the glyph MSDF precedent).
```

## Trace to the approved gate (do not re-litigate)

The carrier is pre-approved on #340 (2026-07-19, user-approved). This design
elaborates within it; it does not re-open it. The approved direction, verbatim
in effect:

- **Carrier = shape-as-mask on the paint entry.** dashbuf gains
  `VectorAtlas { image -> Image, px_per_em, distance_range }` +
  `VectorShape { atlas, atlas_rect, plane_bounds }`; a paint entry's shape
  channel becomes `Parametric | Field(shape_index)`. The painter samples the
  field as a coverage mask and composes it with the existing paint vocabulary
  (so the hero's gradient-filled vectors work day one; P2 composition holds).
- **Generator = pure-Rust `fdsm`** (required, not preferred: the import path
  runs `dashc.wasm`, so vendored C++ cannot ride). Welded to pinned `msdfgen`
  output by a field-equality test. Dedup by path hash. `px_per_em` default 48
  with band-driven escalation. Unfieldable shapes get a named refusal (P4) + a
  ThorVG note.
- Build uses a **bake oracle**: Skia-path-render truth vs MSDF-quad render,
  banded, with px-per-em escalation.

**Grounding did not contradict the carrier.** The two hard constraints both
resolved in its favor:

1. **fdsm compiles to `wasm32-unknown-unknown`.** Verified by a scratch build
   (see "fdsm suitability" below): fdsm 0.8.0 and its whole tree (image 0.25,
   nalgebra 0.34, num-traits, moxcms, simba, matrixmultiply) compiled clean for
   the target, exit 0. No `-sys` crates, no `cc`/`bindgen`, no `rayon`.
2. **The census fits the carrier.** Every VECTOR node in both live targets has
   fieldable closed geometry (Figma pre-expands strokes into a closed
   `strokeGeometry` outline). Zero genuinely-unfieldable nodes in either target
   — the named refusal is defensive, not load-bearing.

## The census (measured, both live targets)

Fetched via `GET /v1/files/<key>/nodes?ids=<root>&geometry=paths` (FIGMA_TOKEN
from the keychain; never committed — public files are live-only content).

### first-light (`MRk9I5cYY6yJa8JhljzkBn`, root `2411:10795`) — 4 VECTOR

| node       | fill  | stroke    | fillGeometry | strokeGeometry | notes                                                                                                                    |
| ---------- | ----- | --------- | ------------ | -------------- | ------------------------------------------------------------------------------------------------------------------------ |
| arrow (×3) | none  | SOLID 3px | 0            | 1 (closed)     | bbox width ~6e-6 — degenerate as a fill; the fieldable geometry is the **strokeGeometry** (Figma's expanded 3px outline) |
| Vector 82  | SOLID | none      | 1 (NONZERO)  | 0              | the "bolt" — a filled lightning-bolt path (M/L/C/Z)                                                                      |

### hero (`S30AJmYfnDKGeSQmzuXEUk`, root `1973:6580`) — 148 VECTOR

- Fills: **135 SOLID + 12 GRADIENT_LINEAR** (matches the plan's cited count),
  plus 1 stroke-only node = 148.
- Geometry per node: 144 fill-only, 1 stroke-only ("Icon", SOLID stroke 2px),
  3 both (fill + stroke), 0 neither.
- The 3 "both" nodes: **fill color == stroke color (white), 0.2px hairline**
  stroke — a single unioned field painted white covers them; no two-colour
  composition is needed by the measured data.
- Winding: 216 NONZERO + 13 EVENODD fill contours; **11 nodes are multi-contour
  (holes)**, max 13 contours on one node.
- Path command vocabulary across all geometry: **`M` `L` `C` `Z` only** — cubic
  Béziers, lines, moves, close. No arcs (`A`), no quadratics (`Q`). A clean,
  closed-contour input for fdsm.
- Field bbox range: widths 0.54–1440 px, heights 1–752 px — a wide size spread,
  which drives the px-per-em escalation case.

### The fixture (already authored, not yet captured)

`vector-shapes` (fileKey `f0nG7azeYELWb9KZ2tLnu9`, frame `1:2`), built by the
A0 `fixtureShapes` plugin command (`importers/figma/plugin/fixture-author/code.js`):
a 460×160 frame with 4 filled VECTOR nodes —

- **star-5-point** — NONZERO, 10 straight segments, solid orange.
- **arrow** — NONZERO, closed 7-vertex polygon, solid blue (note: this fixture
  arrow is _filled_, unlike first-light's stroke-only arrows).
- **organic-blob** — NONZERO, cubic Béziers, solid green.
- **square-with-hole** — **EVENODD**, outer + inner subpath in one path, solid
  rose — the fill-rule / hole case.

The fixture exercises fillGeometry lowering, NONZERO + EVENODD winding, straight

- cubic segments, and solid fills. It does not exercise gradients or stroke-only
  vectors — those are measured live (the hero's gradients, first-light's strokes).

### Unfieldable analysis

Genuinely unfieldable = a VECTOR with neither `fillGeometry` nor
`strokeGeometry`, or a degenerate/zero-area path Figma still reports. **Census:
zero such nodes in either target.** The named refusal (below) is therefore
defensive (P4 completeness), exercised by no live node. The one case adjacent to
"unfieldable" — a fill+stroke vector whose fill and stroke are _different_
colours — also does not occur (the only "both" nodes are same-colour); it is
refused by name for v0.10 and left as a v0.11 candidate.

## fdsm suitability (the critical constraint)

- **Repo/version:** `gitlab.com/Kyarei/fdsm`, v0.8.0, MIT. "A pure-Rust
  implementation of multi-channel signed distance field generation," following
  Chlumský's thesis.
- **Dependencies:** nalgebra, image, num-traits, oklab (optional). No C/C++, no
  `-sys`, no `cc`/`bindgen`, no `rayon`/threads, no filesystem in the compute
  path. Public API: `shape::Shape` (contours of Bézier segments), `Shape::
  edge_coloring_simple`, `Shape::prepare`, `generate::generate_msdf`,
  `correct_sign_msdf`, `render::render_msdf`.
- **wasm32 verdict — PASS.** A scratch crate listing only `fdsm = "0.8"`, built
  with `cargo build --lib --release --target wasm32-unknown-unknown`, compiled
  fdsm and its entire transitive tree for the target (exit 0). This is the
  decisive test: cargo codegens every dependency crate in the graph, so a clean
  build proves fdsm and image/nalgebra/etc. all target wasm32. This **confirms**
  the carrier; nothing to escalate.
  - Note (build task, not a blocker): default `image` 0.25 pulls a colour-
    management subtree (moxcms, pxfm). fdsm needs only image's buffer types, so
    the build should set `image = { default-features = false }` (or depend on
    fdsm with minimal features) to keep the `dashc.wasm` size down.

### The msdfgen weld

`msdfgen` (Rust) is `-sys` bindings to the C++ library; it and `msdf-atlas-gen`
(the C++ CLI the glyph atlas already bakes with — `crates/dashscene-typeset/
src/atlas/tool.rs`, MSDFgen 1.13.0) can only ever be **offline/authoring-time**
reference oracles, never in the wasm path. The weld test therefore compares
fdsm against a **committed reference field**, generated once at test-authoring
time by pinned msdfgen for a fixed canonical shape (a rounded triangle or the
fixture star), stored as a small PNG + its `px_per_em`/`distance_range` metadata.
The test bakes the same shape with fdsm and asserts field-equality within a
tight per-texel tolerance. This welds fdsm to the pinned msdfgen output and keeps
CI free of a C++ toolchain (the reference is regenerated only on a deliberate,
reviewed pin bump — the frozen-fixture discipline, mirroring
`docs/decisions/dsb-frozen-fixture-r7-guard.md`).

## The parametric-shape precedent

There is no explicit "shape" field on the paint entry today. A node's shape is
**implicitly a (rounded) rectangle**: `Paint { fill, stroke, corners: CornerRadii,
clip, shadows }` (`crates/dashbuf/schema/dashbuf.fbs`), where `corners` rounds
the box. That implicit rounded-rect **is** the parametric shape the gate names.
B1 makes the shape channel explicit and adds the `Field` alternative alongside it,
additively — the same append-only move as S1's axes and the v0.8 layout enums.

The image-carrying precedent is exact and reused wholesale: `ImageFill.image` is
a `uint32` index into `Document.images`; `Image { format: ImageFormat, bytes }`
stores encoded bytes; the Skia painter already decodes a PNG image fill
(`import-image-fill` oracle frame). The MSDF atlas is just another `Image` (PNG,
lossless — JPEG would destroy the distance encoding), referenced by index.

## Schema additions (dashbuf, additive / R7)

All appended at the tail; every existing `.dsb` decodes unchanged, and the frozen
`tests/fixtures/v0_5_document.dsb` (`tests/schema_evolution.rs`) still round-trips.

```fbs
// A packed MSDF atlas sheet: one PNG in Document.images holding one or more
// baked vector-shape fields. px_per_em is the field resolution (atlas pixels
// per shape em); distance_range is the MSDF spread in atlas pixels (msdfgen
// -pxrange). Both are needed by the painter for the screen-pixel range
// (distance_range_px * screen_px_per_em / px_per_em — the glyph-atlas metric,
// crates/dashscene-typeset/src/atlas/metrics.rs).
table VectorAtlas {
  image: uint32;            // index into Document.images (the atlas PNG)
  px_per_em: float32;
  distance_range: float32;
}

// The sub-rect of one baked shape inside its atlas, in atlas pixels.
struct AtlasRect { x: uint32; y: uint32; width: uint32; height: uint32; }

// Where the field quad sits in the node's local shape space (the fillGeometry
// coordinate space). The field extends distance_range/px_per_em beyond the
// geometry edge, so these bounds are the padded quad, not the tight geometry
// box (msdfgen's planeBounds vs. the em box).
struct PlaneBounds { left: float32; top: float32; right: float32; bottom: float32; }

// One baked shape: which atlas, which sub-rect, and how the quad maps into the
// node box.
table VectorShape {
  atlas: uint32;           // index into Document.vector_atlases
  atlas_rect: AtlasRect;
  plane_bounds: PlaneBounds;
}

// Document (appended at the tail, after `bindings`):
//   vector_atlases: [VectorAtlas];
//   vector_shapes:  [VectorShape];
```

### The paint shape channel

The channel is `Parametric | Field(shape_index)`. Two faithful encodings; the
build should adopt the **sentinel-index** (recommended) unless review prefers the
union:

- **Recommended — sentinel index.** Add one field to `Paint` at the tail:
  `shape_field: uint32 = 4294967295;` (a `NO_FIELD` sentinel). Absent/sentinel =
  Parametric (the implicit rounded box; `CornerRadii` stays exactly where it is).
  A valid index selects `Field` = `Document.vector_shapes[shape_field]`. This is
  the lightest additive change and is the exact mirror of `Node.paint_entry` /
  `Node.text`'s "index | sentinel" convention; the load gate range-checks the
  index and refuses an out-of-range one by name (P4).
- **Alternative — union.** `union PaintShape { FieldShape }` with
  `table FieldShape { shape: uint32; }`, added as `Paint.shape: PaintShape`;
  absent union = Parametric. More self-describing (matches the `Fill` /
  `BindingTransform` union style) but two table slots and an empty-case for the
  common node. Recommended only if review wants the channel spelled in the type.

Either way the conceptual channel is the approved `Parametric | Field`.

## The painter: field sampling + composition (dashscene-skia + dashpaint)

- **Boundary B (dashpaint):** the resolved paint entry mirrors the new shape
  channel — a paint entry carries either the parametric box or a resolved field
  reference (atlas texture handle + `atlas_rect` + `plane_bounds` + the
  `px_per_em`/`distance_range` scalars). Encoded assets already cross boundary B
  as an `ImageTable` parameter on `Painter::paint`
  (`docs/decisions/image-assets-cross-boundary-b.md`); the atlas PNG rides that
  same channel.
- **Sampling — reuse the glyph MSDF resolve.** dashscene-skia already samples an
  MSDF glyph atlas (median-of-3 channels → signed distance → coverage via a
  smoothstep over the screen-pixel range) for the msdf-text band. The vector
  field is the **same reconstruction**; the only difference is what the coverage
  modulates.
- **Composition — P2 holds.** For a `Field` paint entry, the painter samples the
  field to a coverage value in [0,1] and uses it as the **alpha mask** over the
  paint entry's existing fill (solid, gradient, or image). Concretely: draw the
  fill/gradient into the node box (the existing paint path), masked by the field
  coverage (an SkShader/SkMaskFilter or a masked layer). The painter never reads
  the path — it samples a field and colours it. This is why the hero's 12
  gradient-filled vectors work day one: the gradient is the ordinary `Gradient`
  paint; the field only masks it.

## dashc lowering (the VECTOR arm)

Today `crates/dashc/src/figma/mod.rs` (~L537) refuses VECTOR:
`node.kind != "FRAME" && != "TEXT" && … → self.unsupported(path, "node type VECTOR")`.
B1 replaces that arm for VECTOR with a field lowering. Two upstream fetch changes
are required first (neither the live import nor `capture.ts` requests geometry
today — `geometry=paths` returns nothing without it):

- **importers/figma** (`src/fetch.ts` / `import.ts` live path, and `capture.ts`
  for the committed fixture): add `&geometry=paths` so `fillGeometry` /
  `strokeGeometry` (path string + `windingRule`) reach dashc.
- **dashc `figma/rest.rs`:** parse `fillGeometry` / `strokeGeometry` (each a list
  of `{ path, windingRule }`).

### Field-input selection rule (measured, widen by exactly what the census shows)

| case                   | condition                               | field input                                        | paint                                                                                               |
| ---------------------- | --------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| filled                 | fills non-empty, fillGeometry non-empty | fillGeometry contours                              | the node's fill (solid / linear-gradient)                                                           |
| stroke-only            | fills empty, strokeGeometry non-empty   | strokeGeometry contours (Figma's expanded outline) | synthesized solid fill from the stroke's SOLID colour                                               |
| both, same colour      | fill + stroke, colours equal            | union(fillGeometry, strokeGeometry)                | the fill colour                                                                                     |
| both, different colour | fill + stroke, colours differ           | —                                                  | **named refusal** (via the generic `figma.unsupported` rule), v0.11 candidate; not in either target |
| unfieldable            | no closed geometry / degenerate         | —                                                  | **named refusal** (via the generic `figma.unsupported` rule) + ThorVG note; not in either target    |

The winding rule per contour (NONZERO / EVENODD) is carried into the fdsm shape
so holes (the 13 EVENODD hero contours, the fixture's square-with-hole) fill
correctly. The field quad's `plane_bounds` come from the **geometry's own extent**
(padded by the distance-range margin), not the node's `absoluteBoundingBox` — the
first-light arrows have a near-zero bbox width but a real 3px strokeGeometry
extent.

### The generator (where fdsm lives)

A new module in **dashc** (e.g. `crates/dashc/src/figma/vector_field.rs`), so the
bake runs inside `dashc.wasm` at import time and the fdsm dependency stays
contained to the one crate that needs it. It does:

1. Parse each contour's path string (`M/L/C/Z`) into fdsm Bézier segments; set
   the contour winding.
2. `edge_coloring_simple` → `generate_msdf` at `px_per_em` (default 48) and
   `distance_range` (default 4 px, aligned with the glyph atlas's pxrange 4) →
   an RGB field buffer.
3. **Dedup by path hash:** hash the normalized geometry; identical paths (the
   hero repeats icon vectors) bake once and share a `VectorShape`.
4. **Pack** the unique fields into one atlas sheet (a shelf/row packer, mirroring
   the glyph atlas's packing math in `crates/dashscene-typeset/src/atlas/`), emit
   the atlas PNG into `Document.images`, and record each shape's `atlas_rect` +
   `plane_bounds`.

Generation is import-time (dashc); sampling is render-time (painter) — a clean
split, the same as the glyph atlas (baked offline, sampled at runtime).

### px_per_em default + band escalation

> **v0.10 as-built (2026-07-19):** production bakes every shape at the fixed
> `DEFAULT_PX_PER_EM` (48) and never re-bakes — the census found zero shapes
> needing more. The per-shape escalation ladder and the unfieldable-ceiling
> refusal described below are exercised only by the bake oracle
> (`goldens/tooling/tests/v010_bake_oracle.rs`); wiring them into the production
> lowering is deferred (debt #357). The named refusal is emitted through the
> generic `figma.unsupported` rule, not a dedicated `figma.vector-unfieldable`
> code.

- Default `px_per_em = 48` (per the gate; the glyph atlas uses 32, vectors get
  more headroom). `distance_range = 4 px`.
- **Escalation** is driven by the bake oracle (below): a shape that fails its
  bake band at 48 is re-baked at a higher `px_per_em` (e.g. 64 → 96) until it
  passes or hits a ceiling. The thin first-light arrows (3px stroke) and the
  smallest hero shapes are the escalation candidates. The arabic-atlas spike
  (`docs/technotes/msdf-arabic-atlas-spike.md`) found diminishing returns above
  ~48 px/em for sub-14px content, so the ceiling is finite; a shape still failing
  at the ceiling becomes a named refusal (emitted through `figma.unsupported`).
- Escalation is **per-atlas resolution**: a shape needing a higher resolution
  moves into a higher-`px_per_em` atlas (a document may carry more than one
  `VectorAtlas`). This keeps each `VectorAtlas`'s `px_per_em` a single value.

### Unfieldable named refusal + the ThorVG note

An unfieldable static vector (no closed geometry, degenerate, or still-failing at
the escalation ceiling) is a **named diagnostic under partial-emit**
(emitted through the generic `figma.unsupported` rule) — skip-with-warning, never approximated (P4,
`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`). **ThorVG
note:** the decided escape hatch for genuinely non-bakeable vector content is
render-to-texture via ThorVG (`docs/decisions/runtime-vector-via-thorvg-to-
texture.md`), but that is a _runtime_ mechanism for arbitrary runtime SVG/Lottie;
for an import-time static shape that resists baking, the honest move is the named
refusal now, with ThorVG-to-texture as the v1 escape hatch if such shapes ever
appear in a real target (none do today). B1 does not build the ThorVG path.

## The bake oracle (build-time, path-vs-field, us-vs-us)

Distinct from the import oracle (below) and from the FROZEN Figma-comparison
bands. It validates bake _quality_ independent of Figma:

- For each baked shape, render two ways at a test size: **truth** = the original
  path filled by Skia's exact path rasterizer; **field** = the baked MSDF
  rendered as a quad through the painter's field sampling.
- Diff coverage within a **bake tolerance** (this is a self-comparison — path vs
  field — so it defines its own threshold, tighter than the Figma-vs-us bands;
  it does NOT reuse and does NOT retune the three frozen bands).
- On failure, **escalate** `px_per_em` and re-bake; at the ceiling, assert the
  shape is name-refused. This test is what makes the escalation policy and the
  refusal boundary executable and regression-guarded.
- Lives in the generator's tests (dashc) or goldens tooling; runs natively (Skia
  path render is native).

## The import-oracle frame (the committed regression + DoD)

The `vector-shapes` fixture lands as a new frame in the **import** oracle
(`goldens/oracle/import-manifest.json` + `import_oracle.rs` +
`import-design-source/`), never the frozen E7 `manifest.json`:

```json
{
  "frame": "vector-shapes",
  "fixture": "corpus/figma-fixtures/vector-shapes.json",
  "band": "msdf-text",
  "figmaFileKey": "f0nG7azeYELWb9KZ2tLnu9",
  "figmaNodeId": "1:2",
  "designSource": "oracle/import-design-source/vector-shapes.png",
  "status": "captured"
}
```

**Proposed band: `msdf-text`** (channel_delta 50, differing_fraction 0.03),
reused read-only (FROZEN — never retuned here). Reasoning:

- A baked MSDF field rendered as a quad is the **same reconstruction** as
  msdf-text glyphs — median-of-3, sharp high-contrast edges. Its residual vs
  Figma's exact path raster is glyph-edge-like: sub-pixel AA disagreement along
  arbitrary curved edges. The msdf-text band was tuned for exactly this residual
  (sharp MSDF edges, sparse ink, higher per-pixel threshold).
- `aa-edge` (Δ40, 2%) is for hard _rect/box_ edges vs Figma export resampling —
  a different, tighter residual that could clip the MSDF edge band.
- `blur-falloff` is for soft shadows — irrelevant.
- The fixture is sparse ink (4 small shapes on a 460×160 frame), matching
  msdf-text's sparse-area assumption.

The fixture (`corpus/figma-fixtures/vector-shapes.json`) and its design source
are captured by `deno task capture` + `deno task import-oracle-capture` — the
capture must run with the `geometry=paths` fetch change in place, or the
committed JSON carries no `fillGeometry`.

**DoD (from #340):** vector-shapes oracle frame measured in-band; first-light
bolt + arrows render (`just render MRk9I5cYY6yJa8JhljzkBn 2411:10795`); hero
VECTOR warnings gone (`just reprobe S30AJmYfnDKGeSQmzuXEUk 1973:6580`).

## Alternatives considered

- **Runtime vector engine (ThorVG) for all vectors instead of baking.**
  Rejected: `docs/decisions/runtime-vector-via-thorvg-to-texture.md` scopes
  ThorVG to genuinely-runtime, non-bakeable content; a static imported shape
  bakes faithfully and stays crisp-at-scale and free per frame, whereas ThorVG
  makes it a resize-bound bitmap. ThorVG remains the residual escape hatch.
- **Store the vector as an explicit path/contour vocabulary in the document
  (a real path table).** Rejected for B1: it would put a full path-rendering
  responsibility behind boundary B (every painter must rasterize arbitrary paths
  with winding rules and AA — a P2 violation, painters would do geometry). The
  field carrier keeps painters sampling-only and reuses the glyph MSDF resolve.
  A path vocabulary is a much larger, cross-painter commitment; not warranted by
  the measured need.
- **Vendored C++ msdfgen in the bake path.** Impossible: the import path is
  `dashc.wasm`; C++ cannot ride. This is what forces pure-Rust fdsm and is the
  gate's stated reason. msdfgen stays the offline weld reference only.
- **One `Image` per shape (no packing/dedup).** Rejected: the hero has 148
  vectors with repeats; one image each bloats the `.dsb`. Path-hash dedup +
  shelf-packing into shared atlases is the approved carrier and matches the glyph
  atlas model.
- **Union vs sentinel-index for the shape channel.** Both faithful to the
  approved `Parametric | Field`; the sentinel index is recommended for minimal
  additive weight and convention-match, the union noted as the more
  self-describing option for review to choose.
- **Two-field composition for fill+stroke vectors.** Deferred: the only measured
  "both" nodes are same-colour, covered by a single unioned field. A
  differently-coloured fill+stroke vector is name-refused now (v0.11 candidate) —
  widen by exactly what is measured.

---

## BUILD PLAN

Ordered, dependency-aware. Each step is a candidate build subagent with a verify
criterion. Wave-B ordering (epic doc): land the schema step **after A2's
`ImageFormat`** to avoid dashbuf collisions.

```text
B1.1 generator+weld ─┐
                     ├─► B1.3 painter ─┐
B1.2 schema+load  ───┘                 ├─► B1.4 lowering ─┐
                     └─────────────────┤                 ├─► B1.6 oracle frame + DoD
                                       └─► B1.5 bake oracle ┘
```

### B1.1 — Vector-field generator + msdfgen weld (foundation; no schema dep)

- Add `fdsm` (minimal image features) as a dashc dependency; new module
  `figma/vector_field.rs`: path-string (`M/L/C/Z`) + per-contour winding
  (NONZERO/EVENODD) → fdsm `Shape` → `edge_coloring_simple` → `generate_msdf` at
  px_per_em (48) / distance_range (4) → RGB field buffer. Path-hash dedup.
  Shelf-pack unique fields into one atlas PNG; emit per-shape `atlas_rect` +
  `plane_bounds`.
- Weld test: commit a reference field (pinned msdfgen / msdf-atlas-gen) for one
  canonical shape; assert fdsm ≈ reference within a tight per-texel tolerance.
- **Verify:** unit tests green (a star + a hole shape bake to expected coverage);
  weld test passes; `cargo build -p dashc --lib --release --target
  wasm32-unknown-unknown` succeeds (generator is wasm-clean).

### B1.2 — dashbuf schema + load gate (land after A2's ImageFormat)

- Append `VectorAtlas`, `VectorShape` tables, `AtlasRect` / `PlaneBounds`
  structs; add the `Paint` shape channel (recommended: `shape_field: uint32 =
  NO_FIELD`); append `Document.vector_atlases` / `vector_shapes`. Regenerate
  flatbuffer bindings. Load gate range-checks the field/atlas/shape indices and
  refuses out-of-range by name (P4).
- **Verify:** `just build` green; `tests/schema_evolution.rs` still decodes the
  frozen `v0_5_document.dsb` (R7); a new round-trip test writes and reads a
  document with a VectorAtlas + VectorShape + a Field paint entry.

### B1.3 — Painter field sampling + composition (needs B1.2)

- dashpaint: mirror the shape channel across boundary B (parametric box | field
  ref with atlas handle + rect + plane_bounds + px_per_em + distance_range).
- dashscene-skia: for a Field paint entry, decode the atlas PNG (existing image
  path), sample median-of-3 → coverage (reuse the glyph MSDF resolve), and mask
  the paint entry's fill/gradient/image by that coverage.
- **Verify:** a hand-built `.dsb` with one Field shape renders the shape
  (goldens/unit render); a gradient-masked field renders the gradient inside the
  shape; a rounded-rect (parametric, `shape_field` absent) still renders
  unchanged (no regression to the existing paint path).

### B1.4 — dashc figma VECTOR lowering (needs B1.1 + B1.2; render-verify uses B1.3)

- importers/figma: add `&geometry=paths` to the live fetch (`fetch.ts`/
  `import.ts`) and to `capture.ts`. dashc `rest.rs`: parse `fillGeometry` /
  `strokeGeometry` (`{ path, windingRule }`).
- dashc `mod.rs`: replace the VECTOR `unsupported` arm with the field lowering —
  apply the field-input selection rule (filled / stroke-only / same-colour-both /
  refuse different-colour-both / refuse unfieldable), bake via B1.1, emit
  VectorAtlas/VectorShape + a Paint entry with a Field shape and the fill /
  gradient / synthesized-stroke-colour paint.
- **Verify:** `just reprobe f0nG7azeYELWb9KZ2tLnu9 1:2` emits with no VECTOR
  warning; `just reprobe MRk9I5cYY6yJa8JhljzkBn 2411:10795` and `just reprobe
  S30AJmYfnDKGeSQmzuXEUk 1973:6580` show the VECTOR warnings gone; `just render`
  first-light shows the bolt + arrows drawn.

### B1.5 — Bake oracle (needs B1.1 + B1.3)

- New build-time test (dashc or goldens tooling): per shape, render Skia path
  truth vs MSDF-quad field, diff within the bake tolerance (its own threshold,
  not the frozen bands), escalate px_per_em on failure, assert a name-refusal at
  the ceiling.
- **Verify:** the fixture's 4 shapes + first-light's stroke arrows pass at or
  after escalation; a deliberately thin/degenerate shape exercises the escalation
  path and the ceiling refusal.

### B1.6 — vector-shapes import-oracle frame + DoD (needs B1.2, B1.3, B1.4)

- Capture the fixture: `deno task capture f0nG7azeYELWb9KZ2tLnu9` (with the
  geometry fetch in place) → `corpus/figma-fixtures/vector-shapes.json`;
  `deno task import-oracle-capture` → `import-design-source/vector-shapes.png`.
  Add the frame to `import-manifest.json` (band `msdf-text`).
- **Verify:** `cargo test -p goldens --test import_oracle` measures vector-shapes
  in the msdf-text band; the full DoD holds — first-light bolt + arrows render,
  hero VECTOR warnings gone, hero re-probe clean. Then the story DoD: `just
  build` green, PR draft + `/code-review`, findings captured.

## Open questions for the orchestrator (within the carrier; no gate re-open)

1. **Shape-channel encoding:** sentinel-index (recommended) vs union — a
   build-time schema choice, both faithful to the approved channel.
2. **Generator home:** module in dashc (recommended, contains the fdsm dep) vs a
   small new crate. A new dashscene crate would need a crate-name-map entry; the
   module avoids that.
3. **Bake tolerance value:** the bake oracle's own path-vs-field threshold is not
   pinned here — set it empirically at B1.5 build (tight, since it is
   us-vs-us).
