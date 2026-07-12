# dashscene-skia v0.3 painting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Paint the v0.3 vocabulary in the reference painter, with the image-asset boundary crossing, AA on, and per-kind render tests; the v0.1 golden must pass unchanged.

**Architecture:** Schema first (additive image fields), then the dashpaint types + trait widening (ripples through every `paint` call site), then the painter, then the golden.

**Tech Stack:** skia-safe =0.81.0 (gradient shaders, RuntimeEffect SkSL, rrect, image draw). Gate: `just build`.

## Global Constraints

- Additive schema only; `RectEntry`/`Color`/`PaintKind::Solid` untouched.
- Every failure of capability remains a named panic (P4) — after this
  story only `entry.clip` panics, naming the follow-up issue.
- The committed `v01-walking-skeleton.png` golden must not change.
- Commits: scopes `dashbuf`, `dashpaint`, `dashscene-skia`, `goldens`, `docs`.

---

### Task 1: dashbuf — ImageFill transform + tile_scale (RED: extend paint_roundtrip; GREEN: schema)

- `struct Mat23 { a, b, c, d, tx, ty: float32 }` (row-major 2×3 affine).
- `ImageFill` += `transform: Mat23` (optional struct field), `tile_scale: float32 = 1.0`.
- Round-trip test: transform fields + tile_scale + absent-defaults.
- Commit `feat(dashbuf): image-fill transform and tile scale (v0.3 crop/tile)`.

### Task 2: dashpaint — ImageTable + trait widening (RED: boundary_b tests; GREEN: types)

- `Mat23` (`#[repr(C)]`, Copy), `ImageFormat { Png }`, `ImageAsset { format, bytes: Vec<u8> }`,
  `ImageTable` (push → `ImageIndex`? No — `PaintKind::Image.image` stays `u32` in the schema mirror; table indexes by the same `u32`. Keep `u32` here; the typed-index question for image indices is debt #63's validator concern).
- `PaintKind::Image` += `transform: Option<Mat23>`, `tile_scale: f32`.
- `Painter::paint(&mut self, rects, paints, images: &ImageTable)`.
- Ripple: dashpaint tests (RecordingPainter + fixtures), dashscene-skia impl+tests pass `&ImageTable::new()`, goldens v01 test likewise.
- Commit `feat(dashpaint): image assets cross boundary B (ImageTable; paint() widened)`.

### Task 3: dashscene-skia — the vocabulary (RED: per-kind tests; GREEN: impl)

Implementation structure in `src/lib.rs` (single file still fine, ~300 lines):

- `paint()`: AA on everywhere; per entry: build the rrect from corners; fill (solid/gradient/image), then stroke; `clip` → `unimplemented!` naming the follow-up issue.
- `fn frame_matrix(rect, gradient) -> Matrix` (unit → handles → box).
- Gradients: linear/radial/sweep via skia shaders with local matrix; diamond via `RuntimeEffect` SkSL `t = clamp(abs(u.x)+abs(u.y),0,1)` sampling a stop ramp — implement the ramp inside the SkSL (mix over up to N stops is overkill: instead evaluate the diamond t, then reuse a 1D linear gradient shader along (0,0)→(1,0) composed with a coordinate shader? Simplest correct: build the diamond as SkSL producing t, then map t through stops CPU-side is impossible per-pixel — so: two-stop fast path in SkSL (mix(c0,c1,t)) and multi-stop via SkSL uniform arrays (up to 8 stops, loop). v0.3 corpus needs 2-3 stops; 8 is ample; >8 stops panics named (validator budget later).
- Stroke align: rrect inset/outset by ±width/2 (radii adjusted, clamped ≥ 0), center stroke.
- Images: decode via `images.resolve(index)` + `Image::from_encoded`; save/clip rrect; draw per mode (cover/contain math; Crop via Mat23 → skia Matrix in normalized image space; Tile via shader TileMode::Repeat scaled by tile_scale).
- Tests per kind (exact bytes at deterministic pixels): linear t=0/t=1 extremes; radial center/far corner; sweep quadrants; diamond center/edge-midpoint; stroke inside/center/outside band pixels on a square; corners: corner pixel transparent vs edge-center opaque at radius; images: quadrant colors per mode from a 2×2 asset PNG; multi-stop (3) linear midpoint stop exact; `#[should_panic]` clip; `#[should_panic]` >8-stop diamond.
- Commit `feat(dashscene-skia): paint the v0.3 vocabulary (gradients, stroke align, images, corners)`.

### Task 4: goldens — v03 golden + v0.1 unchanged proof

- `goldens/tooling/tests/v03.rs`: hand-built table covering every kind on one 96×96 canvas; `assert_matches_golden("v03-paint", …)`; generate, inspect visually, commit.
- Run the whole workspace: the v0.1 golden must pass byte-identical (AA-on proof).
- Commit `feat(goldens): golden-test the v0.3 paint vocabulary`.

### Task 5: records + follow-up issue

- File the follow-up issue: "dashscene-core: resolve clipsContent into painter-consumable clips at commit" (story-grade, epic #12/#42 revision input; the painter's clip panic names it).
- Update: `docs/design/dashscene-skia.md` (vocabulary, AA policy), `docs/design/dashpaint.md` + `docs/design/dashbuf.md` ("Open for story #14" sections close; ImageTable; new fields), `docs/decisions/paint-entry-composition.md` (image sub-decision closes), new records `docs/decisions/image-assets-cross-boundary-b.md` and `docs/decisions/reference-painter-antialiasing.md` (closes #85), decisions README index.
- Commit `docs(dashscene-skia): record the v0.3 painting decisions`.
