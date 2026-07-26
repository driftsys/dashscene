# vector MSDF baking — Figma VECTOR nodes → baked coverage fields

    crate    crates/dashc (module `figma::vector_field` — the generator),
             crates/dashbuf (schema), crates/dashpaint + crates/dashscene-skia
             (boundary B + sampling), crates/dashscene-validator (range gates),
             goldens/tooling (bake oracle)
    covers   v0.10 — real-file fidelity, story B1 (issue #340, epic #343)
    traces   docs/design/architecture.md (Vector baking row; dashc row),
             docs/decisions/baked-vector-msdf-field.md (the carrier decision),
             docs/decisions/paint-entry-composition.md,
             docs/decisions/image-assets-cross-boundary-b.md,
             docs/decisions/dsb-frozen-fixture-r7-guard.md (R7 append),
             docs/decisions/unsupported-figma-constructs-refuse-the-compile.md,
             docs/design/atlas-pipeline.md (the glyph-MSDF precedent reused),
             docs/technotes/msdf-arabic-atlas-spike.md,
             P1 (intent, not results), P2 (painters only color),
             P4 (validated vocabulary, named diagnostics)

## Purpose

A Figma `VECTOR` node lowers into the dashscene document as a baked
multi-channel signed-distance field (MSDF), carried on the paint entry as a
coverage mask. Before B1 a `VECTOR` node was a named skip-with-warning; after
B1 first-light's bolt and stroke arrows and the Landify hero's 148 vectors
render. The field is a resolution-independent shape source — the same kind of
intent the glyph atlas already carries — so P1 holds: the document carries the
baked field, not a rasterized pixel result. The painter only samples and
composes (P2), and a shape that cannot be fielded is a named diagnostic (P4).

## The carrier — shape-as-mask on the paint entry

The paint entry's shape channel is `Parametric | Field(shape_index)`. Before
B1 every node's shape was implicitly a (rounded) rectangle — `Paint { fill,
stroke, corners, clip, shadows }`, where `corners` rounds the box. That
implicit rounded-rect is the parametric case. B1 makes the channel explicit and
adds the `Field` alternative alongside it, additively.

For a `Field` paint entry the painter samples the baked field to a coverage
value in `[0,1]` and uses it as the alpha mask over the entry's existing fill
(solid, gradient, or image). The painter never reads a path — it samples a
field and colors it, exactly as it already samples an MSDF glyph. This is why
the hero's 12 gradient-filled vectors render on the first day: the gradient is
the ordinary `Gradient` paint; the field only masks it. The normative choice
and the rejected alternatives are in
`docs/decisions/baked-vector-msdf-field.md`.

## Schema additions (dashbuf, additive / R7-safe)

All appended at the tail of `crates/dashbuf/schema/dashbuf.fbs`; every existing
`.dsb` decodes unchanged, and the frozen `tests/fixtures/v0_5_document.dsb`
still round-trips (`docs/decisions/dsb-frozen-fixture-r7-guard.md`).

    table  VectorAtlas  { image: uint32;        // index into Document.assets (atlas PNG)
                          px_per_em: float32;
                          distance_range: float32; }
    struct AtlasRect     { x, y, width, height: uint32; }   // sub-rect in atlas pixels
    struct PlaneBounds   { left, top, right, bottom: float32; }  // padded field quad in shape space
    table  VectorShape   { atlas: uint32;                   // index into Document.vector_atlases
                          atlas_rect: AtlasRect;
                          plane_bounds: PlaneBounds; }
    Paint.shape_field: uint32 = 4294967295;   // NO_FIELD sentinel; else Document.vector_shapes[shape_field]
    Document.vector_atlases: [VectorAtlas];
    Document.vector_shapes:  [VectorShape];

- **Sentinel-index encoding.** `Paint.shape_field` defaults to the `NO_FIELD`
  sentinel (`uint32::MAX`); absent/sentinel means parametric (the implicit
  rounded box; `CornerRadii` is untouched). A valid index selects the `Field`
  case. This is the exact mirror of the `Node.paint_entry` / `Node.text`
  "index | sentinel" convention. A `Paint` with the sentinel serializes and
  loads byte-identically to a pre-B1 `Paint`, so a document with no vectors is
  unchanged end to end.
- **`VectorAtlas`** carries the atlas resolution (`px_per_em`, atlas pixels per
  shape em) and the MSDF spread (`distance_range`, in atlas pixels). Both feed
  the painter's screen-pixel-range computation
  (`distance_range * screen_px_per_em / px_per_em`), the same metric the glyph
  atlas uses (`docs/design/atlas-pipeline.md`).
- **`PlaneBounds`** is the padded quad — the field extends
  `distance_range / px_per_em` beyond the geometry edge, so these bounds are
  larger than the tight geometry box (msdfgen's `planeBounds` vs. the em box).
  The bounds come from the geometry's own extent, not the node's
  `absoluteBoundingBox`: first-light's arrows have a near-zero fill bbox but a
  real 3 px strokeGeometry extent.

Three named range-check rules in `dashscene-validator` refuse an out-of-range
`shape_field`, `VectorShape.atlas`, or `VectorAtlas.image` at the load gate
(P4).

## The generator (dashc, `figma::vector_field`)

`crates/dashc/src/figma/vector_field.rs` bakes at import time, so it runs
inside `dashc.wasm` and keeps the `fdsm` dependency contained to the one crate
that needs it. Generation is import-time; sampling is render-time — the same
offline-bake / runtime-sample split the glyph atlas uses.

- **fdsm, not msdfgen.** The generator is pure-Rust `fdsm` 0.8.0, a
  MIT-licensed MSDF implementation. It is required, not merely preferred: the
  import path is `dashc.wasm`, so a vendored C++ `msdfgen` cannot ride. fdsm and
  its whole transitive tree (image, nalgebra, num-traits) compile to
  `wasm32-unknown-unknown` (verified; `just wasm` green). Public entry points:
  `VectorAtlasBaker`, `bake_single`, `plan_field`; `DEFAULT_PX_PER_EM = 48`,
  `DISTANCE_RANGE = 4` (aligned with the glyph atlas's pxrange 4).
- **Bake steps.** Parse each contour's path string (`M`/`L`/`C`/`Z` only, the
  measured vocabulary) into fdsm Bézier segments, carrying the per-contour
  winding (`NONZERO`/`EVENODD`) so holes fill correctly →
  `edge_coloring_simple` → `generate_msdf` at `px_per_em` / `distance_range` →
  an RGB field buffer.
- **Dedup by path hash.** Identical normalized geometry (the hero repeats icon
  vectors) is baked once and shares a `VectorShape` (structural hashing).
- **Shelf packing.** Unique fields pack into one atlas sheet, the atlas PNG is
  emitted into `Document.assets`, and each shape records its `atlas_rect` +
  `plane_bounds`. (The bake oracle caught a real packer bug here: a lone field
  sized to its own width let the gutter push the blit off-edge; fixed by
  reserving `widest + 2 * gutter`.)

### The msdfgen weld

`msdfgen` (C++, `-sys` bindings) is an authoring-time reference oracle only,
never in the wasm path. A weld test compares fdsm against a committed reference
field — `crates/dashc/tests/fixtures/weld_star_msdf.png`, generated once by
pinned msdfgen v1.13.0 (the same tool the glyph atlas bakes with) — and asserts
field-equality within a per-texel tolerance. The value the painter actually
samples is the median-of-3 reconstructed distance; that reconstruction agrees
with the reference to a maximum of 0.0157 texel (mean 0.0019, about one
quantization step), against a `MEDIAN_MAX` tolerance of 0.047. Raw per-channel
distance diverges only at corners (~0.79 %), a coloring-heuristic difference
that the median-of-3 removes. The reference is regenerated only on a deliberate,
reviewed pin bump (`UPDATE_VECTOR_WELD_REFERENCE=1`), so CI stays C++-free — the
frozen-fixture discipline of `docs/decisions/dsb-frozen-fixture-r7-guard.md`.

## dashc lowering (the VECTOR arm)

The importer fetches geometry (`&geometry=paths` on the live fetch in
`importers/figma/src/fetch.ts` / `import.ts` and on `capture.ts`), and dashc's
`figma/rest.rs` parses `fillGeometry` / `strokeGeometry` (each a list of
`{ path, windingRule }`). The `figma/mod.rs` VECTOR arm applies a
measured-only field-input selection rule — widen by exactly what the two live
targets show:

| case                  | condition                             | field input                               | paint                                                                   |
| --------------------- | ------------------------------------- | ----------------------------------------- | ----------------------------------------------------------------------- |
| filled                | fills + fillGeometry non-empty        | fillGeometry contours                     | the node's fill (solid / linear-gradient)                               |
| stroke-only           | fills empty, strokeGeometry non-empty | strokeGeometry (Figma's expanded outline) | synthesized solid from the stroke color                                 |
| both, same color      | fill + stroke, colors equal           | union(fill, stroke) geometry              | the fill color                                                          |
| both, different color | fill + stroke, colors differ          | —                                         | named refusal (`figma.unsupported`), v0.11 candidate; in neither target |
| unfieldable           | no closed geometry / degenerate       | —                                         | named refusal (`figma.unsupported`); in neither target                  |

The two refusal rows are defensive (P4 completeness): the census found zero
different-color fill+stroke nodes and zero genuinely-unfieldable nodes in either
live target. Figma pre-expands strokes into a closed `strokeGeometry` outline,
so every measured `VECTOR` has fieldable closed geometry.

### Fixed-48 baking; escalation deferred

Production bakes every shape at the fixed `DEFAULT_PX_PER_EM` (48) and never
re-bakes — the census found zero shapes that need more, so the DoD is met at a
single resolution ("widen by exactly what is measured"). The per-shape
escalation ladder (re-bake at a higher `px_per_em` on a bake-band failure) and
the unfieldable-ceiling refusal exist **only** in the bake oracle
(`goldens/tooling/tests/v010_bake_oracle.rs`); wiring them into the production
lowering is deferred to debt **#357**. The named refusal is emitted through the
generic `figma.unsupported` rule, not a dedicated `figma.vector-unfieldable`
code. ThorVG-to-texture (`docs/decisions/runtime-vector-via-thorvg-to-texture.md`)
remains the v1 escape hatch for genuinely non-bakeable content; B1 does not
build it.

## The painter (dashpaint + dashscene-skia)

- **Boundary B (dashpaint).** The resolved paint entry mirrors the shape
  channel: a parametric box, or a resolved field reference (atlas texture
  handle + `atlas_rect` + `plane_bounds` + `px_per_em` + `distance_range`). The
  atlas PNG crosses boundary B on the existing `ImageTable` parameter of
  `Painter::paint` (`docs/decisions/image-assets-cross-boundary-b.md`).
- **Sampling reuses the glyph MSDF resolve.** dashscene-skia decodes the atlas
  PNG (the existing image path), samples median-of-3 channels → signed distance
  → coverage via a smoothstep over the screen-pixel range — the same
  reconstruction the msdf-text band already uses for glyphs. The only difference
  is what the coverage modulates: the paint entry's fill / gradient / image
  rather than a text color.

## The bake oracle (build-time, path-vs-field, us-vs-us)

`goldens/tooling/tests/v010_bake_oracle.rs` validates bake _quality_
independent of Figma, distinct from the import oracle and from the frozen
E7 render-oracle bands. Per shape it renders two ways at a test size: **truth**
= the original path filled by Skia's exact path rasterizer; **field** = the
baked MSDF rendered as a quad through the painter's field sampling. It diffs
coverage within its own footprint-relative tolerance (3 %, tighter than the
Figma-vs-us bands and never reusing or retuning them), and it exercises the
escalation ladder and the ceiling refusal so both stay executable. Census
shapes bake within tolerance at 48 px/em (star 0.033 %, others ≤ 0.013 %). This
oracle is what caught the packer bug noted above.

## As-built results

- **first-light** (`MRk9I5cYY6yJa8JhljzkBn`, root `2411:10795`): the bolt fill
  and the three stroke arrows render through the MSDF field — previously all
  four were skip-with-warning holes.
- **hero** (`S30AJmYfnDKGeSQmzuXEUk`, root `1973:6580`): all 148 vectors lower;
  the `.dsb` grows from ~189 KB to ~246 KB with the baked fields, and the file
  renders end to end (Landify logo, vector ribbons/circles/quote-mark, feature
  icons, app badges). Lowering the vectors also unmasked a pre-existing
  backdrop-blur node (previously hidden behind the VECTOR skip): a `VECTOR`
  carrying `BACKGROUND_BLUR` on the hero. That node was skipped whole under
  `EmitPolicy::Partial` until story #393 made backdrop blur core vocabulary; it
  now lowers and keeps its blur, and the baked-vector paint entry carries it
  (`docs/decisions/backdrop-blur-is-core-vocabulary.md`). The skip mechanism it
  used to rely on is recorded at
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`. Strict
  mode is unchanged.
- **import oracle**: the `vector-shapes` fixture (`f0nG7azeYELWb9KZ2tLnu9`,
  node `3:2`, four fill-only VECTORs) measures 1/73600 px (0.001 %, max Δ61) in
  the `msdf-text` band — effectively exact. The `msdf-text` band is reused
  read-only because a baked MSDF field rendered as a quad is the same
  reconstruction as an msdf-text glyph, so its residual vs Figma's exact path
  raster is glyph-edge-like.

## Consequences and seams

- **MSRV.** The fdsm dependency tree needs Rust ~1.88, so the declared
  workspace `rust-version` moved 1.85 → 1.88 (honest; the repo builds on a
  newer toolchain).
- **Geometry-extent correctness.** A `plane_bounds` right/bottom must use the
  ceil'd atlas extent divided by the scale, not the un-ceil'd geometry extent,
  or the field renders ~1 texel small anisotropically; fixed with a
  non-integer-width regression test.
- **Debts.** #356 (a skipped vector can leave an orphan atlas tile), #357
  (wire the escalation ladder into production, currently oracle-only), #358
  (review minors).

## Trace

- Satisfies: issue #340 (B1), epic #343; P1, P2, P4; the
  `docs/design/architecture.md` "Vector baking" row (compile-time bake).
- Decision: `docs/decisions/baked-vector-msdf-field.md`.
- Reuses: `docs/design/atlas-pipeline.md` (glyph-MSDF precedent),
  `docs/decisions/image-assets-cross-boundary-b.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/dsb-frozen-fixture-r7-guard.md`.
- Related: `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`
  (the backdrop-blur Partial-omit follow-up),
  `docs/decisions/runtime-vector-via-thorvg-to-texture.md` (the v1 escape
  hatch), `docs/technotes/2026-07-19-v010-real-file-fidelity.md`.
- Raw working memory: `docs/archive/2026-07-19-B1-vector-msdf-design.md`.
