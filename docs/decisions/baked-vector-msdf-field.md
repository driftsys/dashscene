# A Figma VECTOR lowers into a baked MSDF field carried as a paint-entry coverage mask

    status   accepted (story B1, issue #340, epic #343, 2026-07-19)
    scope    dashbuf schema, dashpaint (boundary B), dashscene-skia,
             dashc (figma lowering + the fdsm generator), dashscene-validator,
             goldens (bake oracle)

## Context

v0.10 (real-file fidelity) must render Figma `VECTOR` nodes, which the importer
had refused by name — 4 on first-light, 148 on the Landify hero. Two facts
constrain the design:

- **P2** — a painter only colors; it must not rasterize arbitrary paths with
  winding rules and anti-aliasing, which is geometry work. So a `VECTOR` cannot
  reach the painter as a path.
- **The import path is `dashc.wasm`.** Any bake that runs at import time must
  compile to `wasm32-unknown-unknown`, so vendored C++ (`msdfgen`,
  `msdf-atlas-gen`) cannot ride the compile path.

The census of both live targets: every `VECTOR` has fieldable closed geometry
(Figma pre-expands strokes into a closed `strokeGeometry` outline); path
commands are `M`/`L`/`C`/`Z` only; 135 SOLID + 12 GRADIENT_LINEAR fills on the
hero; 11 nodes have holes (EVENODD); zero genuinely-unfieldable nodes and zero
different-color fill+stroke nodes.

## Decision

**A `VECTOR` node lowers into a baked multi-channel signed-distance field
(MSDF), carried on the paint entry as a coverage mask.** The paint entry's
shape channel is `Parametric | Field(shape_index)`: absent/sentinel is the
existing implicit rounded-rect (parametric), a valid index selects a baked
`VectorShape`. The painter samples the field to a coverage value in `[0,1]` and
masks the entry's existing fill (solid, gradient, or image) by it — it never
reads a path. The field is resolution-independent shape _intent_, the same kind
the glyph atlas already carries, so P1 holds (the document carries no rasterized
pixel result).

**The generator is pure-Rust `fdsm`, welded to pinned `msdfgen`.** fdsm bakes
inside `dashc.wasm` at import time; a committed reference field generated once by
pinned msdfgen v1.13.0 welds fdsm's output (the median-of-3 reconstructed
distance the painter samples) within a per-texel tolerance, so CI stays
C++-free. The schema additions are append-only and R7-safe: `VectorAtlas`,
`VectorShape`, `AtlasRect`, `PlaneBounds`, a `Paint.shape_field: uint32`
sentinel (`NO_FIELD = uint32::MAX`), and `Document.vector_atlases` /
`vector_shapes` — a document with no vectors serializes byte-identically to a
pre-B1 document.

**Baking is fixed at 48 px/em for v0.10; escalation is deferred (debt #357).**
Production bakes every shape once at `DEFAULT_PX_PER_EM = 48` — the census found
zero shapes needing more, so the fidelity bar is met at a single resolution
("widen by exactly what is measured"). A per-shape escalation ladder and an
unfieldable-ceiling refusal are defined and executed only by the bake oracle
(`goldens/tooling/tests/v010_bake_oracle.rs`); wiring them into the production
lowering is deferred to issue #357.

**Unfieldable shapes and different-color fill+stroke vectors are named
refusals** through the generic `figma.unsupported` rule (P4), not a dedicated
code. Both are defensive — neither occurs in either live target.

The as-built mechanism (schema, generator, weld, painter, bake oracle) is
`docs/design/vector-msdf-baking.md`.

## Alternatives considered

- **A runtime vector engine (ThorVG) for all vectors, instead of baking.**
  Rejected. `docs/decisions/runtime-vector-via-thorvg-to-texture.md` scopes
  ThorVG to genuinely-runtime, non-bakeable content; a static imported shape
  bakes faithfully and stays crisp at any scale and free per frame, whereas
  ThorVG makes it a resize-bound bitmap. ThorVG stays the v1 residual escape
  hatch if a real target ever carries an unfieldable shape (none does today).
- **Store the vector as an explicit path/contour vocabulary in the document.**
  Rejected for B1. It would put full path rasterization (arbitrary contours,
  winding rules, AA) behind boundary B, a direct P2 violation, and is a much
  larger cross-painter commitment than the measured need warrants. The field
  carrier keeps every painter sampling-only and reuses the glyph MSDF resolve.
  A path vocabulary remains the deferred v1 option.
- **Vendored C++ `msdfgen` in the bake path.** Impossible — the import path is
  `dashc.wasm`; C++ cannot ride. This is what forces pure-Rust fdsm; msdfgen
  stays the offline weld reference only.
- **A union (`PaintShape { FieldShape }`) instead of a sentinel index for the
  shape channel.** Both are faithful to the approved `Parametric | Field`
  channel. The sentinel index was chosen for minimal additive weight and to
  match the existing `Node.paint_entry` / `Node.text` "index | sentinel"
  convention; the union is more self-describing but adds table slots and an
  empty case on the common node.
- **One `Image` per shape (no packing or dedup).** Rejected. The hero has 148
  vectors with repeats; one image each bloats the `.dsb`. Path-hash dedup plus
  shelf-packing into shared atlases mirrors the glyph-atlas model.
- **Two-field composition for fill+stroke vectors.** Deferred. The only
  measured "both" nodes are same-color, covered by a single unioned field; a
  different-color fill+stroke vector is name-refused now, a v0.11 candidate.

## Consequences

- **Byte-identity for vector-free documents.** `Paint.shape_field` defaults to
  the sentinel and the new tables/vectors are tail-appended, so the frozen
  `v0_5_document.dsb` still round-trips (R7) and a document with no vectors is
  unchanged end to end.
- **`Painter::paint` gains no new parameter.** The atlas PNG rides the existing
  `ImageTable` channel; sampling reuses the median-of-3 MSDF resolve the
  msdf-text band already runs.
- **MSRV moved 1.85 → 1.88.** The fdsm dependency tree requires it.
- **Lowering the hero's vectors unmasked a backdrop-blur node.** A `VECTOR`
  carrying `BACKGROUND_BLUR` was previously hidden behind the VECTOR skip; once
  vectors lower it triages to an Error verdict. Under `EmitPolicy::Partial`
  only, a backdrop blur then joined the node's blockers and the node was skipped
  whole with a named warning — a per-construct follow-up the refusal decision
  pre-named, recorded in place in
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`. Story
  #393 then made backdrop blur core vocabulary, so that node no longer skips —
  it lowers and keeps its blur, and this remains the record of why it was
  visible in the first place
  (`docs/decisions/backdrop-blur-is-core-vocabulary.md`).
- **The escalation ladder is oracle-only until #357.** Production trusts the
  fixed 48 px/em census result; a future real file with a shape that fails its
  bake band at 48 needs #357's wiring, otherwise it bakes low-fidelity rather
  than escalating.
- **Debts.** #356 (a skipped vector can leave an orphan atlas tile), #357
  (production escalation wiring), #358 (review minors).

## Trace

- Satisfies: issue #340 (B1) acceptance criteria; epic #343; P1, P2, P4.
- Files: `docs/design/vector-msdf-baking.md`, `docs/design/architecture.md`
  (Vector baking row).
- Related: `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/image-assets-cross-boundary-b.md`,
  `docs/decisions/dsb-frozen-fixture-r7-guard.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`,
  `docs/decisions/runtime-vector-via-thorvg-to-texture.md`,
  `docs/design/atlas-pipeline.md`.
- Raw working memory: `docs/archive/2026-07-19-B1-vector-msdf-design.md`.
