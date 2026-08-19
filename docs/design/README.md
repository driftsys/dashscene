# design

Architecture: interfaces and components, gardened from `docs/wip/` sessions into
durable, as-built records.

The system-wide record comes first below; per-component records follow it
directly:

- [architecture.md](architecture.md) — the system-wide record: stack, pipeline,
  boundaries A and B, the component map, and the planned-but-unbuilt components.
- [dashpaint.md](dashpaint.md) — boundary B: the paint table + clip table +
  painter trait (v0.1 walking skeleton, story #3; v0.3 paint vocabulary, story
  #13; resolved subtree clips, story #97).
- [dashscene-core-arena.md](dashscene-core-arena.md) — the arena's intent model,
  staged mutation, commit resolution pipeline, and committed output (story #2);
  text content and style as intent, v0.5 (story #26); clip and corner intent,
  and commit-time clip resolution (story #97).
- [dashscene-engine.md](dashscene-engine.md) — the Taffy solve behind the
  `LayoutSolver` seam: per-root trees, the axis-relative style mapping (story
  #9).
- [dashbuf.md](dashbuf.md) — boundary A: the `.dsb` document schema (v0.1
  walking skeleton; v0.3 paint vocabulary, story #13; v0.5 text vocabulary,
  story #26).
- [dsb-container-format.md](dsb-container-format.md) — the `.dsb` file envelope:
  header, section table, hashes, alignment, and the loading model (v0.11, story
  #399).
- [dashlang.md](dashlang.md) — the value-tree builder surface and its one-commit
  mapping onto `dashscene-core` (story #5).
- [dashcue.md](dashcue.md) — the descriptive animation vocabulary and its
  runtime scheduling: transitions, springs, keyframes (v0.4, story #21).
- [atlas-pipeline.md](atlas-pipeline.md) — the build-time font → MSDF glyph
  atlas + metrics blob pipeline (v0.5, story #27); one committed atlas directory
  per (script, weight) (v0.11, story F1/#368).
- [typeset-latin.md](typeset-latin.md) — the runtime text pipeline: bidi split →
  per-run shaping → greedy break → per-line display reorder and baseline
  positioning, with a font-unit shaped-run cache (v0.5, story #28; v0.6 bidi and
  Arabic + digit shapes, stories #32/#33); the weighted cascade — coverage picks
  the family, the CSS weight picks the face (v0.11, story F1/#368).
- [dashscene-skia.md](dashscene-skia.md) — the Skia CPU-raster reference
  painter, the first `Painter` implementation (story #4); the v0.3 paint
  vocabulary (story #14) and resolved subtree clips (story #97).
- [dashscene-gpu.md](dashscene-gpu.md) — the lean painter: instanced quads and
  analytic SDF over wgpu, covering native and web from one codebase; the
  instance buffer as the painter's output, the four-storage-buffer wall that
  shaped the paint heap, atlas residency, layers and the backdrop blur, and the
  four-layer verification net (v0.15, epic #569).
- [host-integration.md](host-integration.md) — the two integration crates,
  `dashscene-web` and `dashscene-desktop`: the five pieces an embedder must
  have, the byte-range and mapped load paths, the two frame loops, the published
  `Present` seam, and the checks that keep the surface out of the demonstrations
  (v0.17, stories #741, #810, #794, #792).
- [c-abi.md](c-abi.md) — `dashscene-ffi` as built: the entry points and the
  lifecycle they form, why there are three loaders, the versioning rule that has
  kept `DS_ABI_VERSION` unmoved across every status variant added since, and
  moved it to 2 exactly once, for story #1226's ten changed signatures — and the
  gap in it that `SurfaceLost` exposed — what a caller must guarantee, and the
  gaps — including that it is a control plane and not a data plane (v0.19, story
  #840).
- [android-toolchain.md](android-toolchain.md) — the `aarch64-linux-android`
  target, the discovered NDK and the API floor, the `android-build` job, and the
  D3a probe: what the painter's own device request reports on an adapter,
  measured on the host and on an emulator. **The target-hardware measurement is
  not taken** (v0.19, story #839).
- [goldens.md](goldens.md) — the golden-image diff tooling and the v0.1
  walking-skeleton golden scene, the v0.1 slice's closing component (story #6);
  the v0.2 flex-vocabulary goldens, closing epic #7 (story #11); the v0.3 paint
  and clip goldens (stories #14, #18, #97).
- [dashscene-validator.md](dashscene-validator.md) — the validation gates, the
  diagnostic shape, and the v0.1–v0.3 rule set (story #15); producer-assembled
  reports (story #139); the contribution gate, which reads a document and a
  host's bindings together (story #1127).
- [dashc.md](dashc.md) — the dashscene compile pipeline: the in-memory
  `Document` model, the deterministic `.dsb` emitter, the emission gate, and the
  `dashscene-core` load path (story #16); the Figma REST front end — the
  lowering walk, the import gate, and `compile_figma` (story #139); the wasm ABI
  the Deno importer calls it through (story #17).
- [vector-msdf-baking.md](vector-msdf-baking.md) — Figma `VECTOR` nodes bake
  into multi-channel signed-distance fields carried on the paint entry as
  coverage masks: the pure-Rust `fdsm` generator inside `dashc.wasm` (welded to
  pinned msdfgen), the additive `VectorAtlas`/`VectorShape` schema +
  `shape_field` sentinel, the boundary-B field sampling, and the bake oracle
  (v0.10, story B1/#340).

See the `sdd-working-memory-lifecycle` rule.
