# Changelog

All notable changes to this package are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version tracks
the Cargo workspace rather than moving on its own.

## [Unreleased]

### Added

- **The Showcase sample asks for 60 fps before its first frame** (issue #1408).
  `Samples~/Showcase/DashsceneFramePacing.cs` sets
  `Application.targetFrameRate = 60` at `SubsystemRegistration` — process-wide,
  in every player and Editor play session that compiles the sample, before the
  first scene: a project that imports the sample and sets its own target
  elsewhere gets this one first. On Android, which honours the target, a panel
  above 60 Hz is capped by it; on desktop the target is ignored while vsync is
  on, which is Unity's default for a player; the Editor's Game view has it off
  by default, so a play session there is capped. Unity's Android default paces
  at 30 fps whatever the panel does, and a player asked for 60 with Unity's
  default pacing presented on every other vsync, so the repository's own demo
  build script also turns optimized frame pacing on — measured on a Pixel 5,
  `docs/design/android-toolchain.md`.
- **The showcase reports what a frame cost, and states what the figure is.**
  `Samples~/Showcase/DashsceneFrameCost.cs` reports one line per 240 drawn
  frames: the runtime tick, and the drawing this package executes — the frame
  lease, `BrgPainter.Draw` and the release. It **excludes** the GPU's execution
  of the batches, the render pipeline's passes, culling and the swapchain
  present, because Unity runs all of them after `Update` returns, so the figure
  is a floor on the painter's per-frame cost and not the whole of it. The header
  states the definition term by term against `demo/src/shell.rs`, which is the
  instrument the other three hosts report through. Each line names the extent it
  was drawn at, and a sample is discarded when the entry or the extent changes
  part-way. Armed by default; `-no-frame-cost` on the command line turns it off.
- **The showcase rotates on the up arrow**, so a player can be put through an
  orientation change on a device that will not rotate for a script — a build
  allowing all four orientations follows the sensor, and a handset lying flat
  reports portrait whatever the window manager is told.

### Fixed

- **A sorted draw command now names exactly one visible instance.** Unity's
  sorted-transparent `BatchRendererGroup` path was measured dropping a
  contiguous subset of draw commands for single frames when a command carrying
  `BatchDrawCommandFlags.HasSortingPosition` named more than one: the affected
  region of that frame renders as bare backdrop, nothing is logged, no exception
  is raised, and the painter's own culling emission is byte-identical on it.
  `BrgPainter` now emits one draw command per instance, the shape
  `docs/technotes/batch-renderer-group.md` §3 attributes to Unity's own GPU
  Resident Drawer and the only one measured free of the defect. On macOS/Metal,
  Apple M3, Unity 6000.3.23f1, the showcase typography scene: 410 affected
  frames per 20,000 before, 0 per 20,000 after. The showcase's reported frame
  cost held its mean at 0.19 ms, though the command count rose from 11 to 381
  per view — the small movements in the median, 95th percentile and maximum are
  run-to-run noise, and the figure excludes Unity's culling callback;
  `docs/design/unity-csharp-host.md` states what it covers.
  `docs/technotes/batch-renderer-group.md` §5d carries the tables. Issue #1401.
- **The painter draws its text.** `BrgPainter` drew every surface and no glyph,
  in every player build on every platform, through ten green configurations —
  `BatchRendererGroup` groups draw commands by material before drawing them, so
  the painter's emission order did not survive and the document's backdrop was
  drawn over the glyphs. The painter now sets
  `BatchDrawCommandFlags.HasSortingPosition` on every command and writes one
  sorting key per command, which is what makes the glyphs reach the screen
  (issue #1389). **The order it draws them in is the emission order**, measured
  under one instance per command on one graphics API (issue #1402): the keys'
  rank is the draw order, farthest first, on the flagged path — the fixture
  composites in order with the flag off as well, which the record lists as
  unexplained — and `just unity-render`'s order phase reads seven composite
  pixels of a fixture built for it on every run;
  `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` D1 carries the
  claim and its limits.
- **Every batch after the first is registered on the RawBuffer rung.**
  `AddBatches` passed a non-zero window offset there, where Unity requires both
  window parameters to be zero and refuses the batch otherwise — through a log
  line rather than an exception, so the loss was silent. The batch's byte offset
  is folded into the metadata offsets instead. Unreachable through this
  package's own API today, because the rung's per-batch capacity doubles until
  one batch covers the whole document. Issue #1389.
- **A frame the painter cuts short says so.** `OnPerformCulling` now reports,
  once, when it emitted fewer instances than the frame packed — which happens
  when `Draw` throws part-way — instead of handing over a partial picture in
  silence.

- **The manifest no longer says the painter draws no text.** `package.json`'s
  `description` — what a UPM registry listing shows — still said the painter
  draws "no shadows, blurs, images or text" after story #1123 had landed the
  text seam. It now points at this package's README and at `PackDiagnostic`
  instead of carrying a list of its own (issue #1325).
- **The R-E5 warning no longer fires on a correctly configured host.**
  `BrgPainter` read `GraphicsSettings.useScriptableRenderPipelineBatching` in
  its constructor, and URP assigns that global inside its own pipeline
  instance's constructor — which Unity runs at the first render, after the
  `Awake` of the first frame where a host builds a painter. The global was
  therefore `false` in every process that had not yet rendered, whatever the
  project was set to. The read now happens in `Draw`, guarded on
  `RenderPipelineManager.currentPipeline`, and is decided once per pipeline
  instance rather than once per painter — so a host that switches to an asset
  with the batcher off is told (issue #1317).
- **Rung 3 is reported rather than selected silently.** Where
  `BatchRendererGroup.BufferTarget` answers
  `UnsupportedByUnderlyingGraphicsApi`, the painter took
  `BrgRung.InstancedWithoutBrg`, built no group and drew nothing, logging
  nothing — while R-E6's default produces a blank frame that Unity itself names
  on every frame. The two were indistinguishable from the console, and reading
  `Rung` was the only way to tell them apart. The constructor now warns on that
  arm (issue #1326).

- **The showcase reads its manifest inside an APK.**
  `Application.streamingAssetsPath` on Android is a `jar:` URL into the APK, so
  `File.Exists` answered false for a `showcase.json` that was present: the
  sample reported it missing, `Awake` ended, and the showcase scenes — which
  need no manifest at all — never loaded either. The read now goes through
  `Runtime/Engine/StreamingAssetDocument.Resolve`, which asks the APK's own
  asset manager where the entry is, and a short read is refused rather than
  returned as a partial document.

- **The showcase scenes draw in the demonstration, with their motion.**
  `Samples~/Showcase` now walks the three `corpus/showcase` scenes — the ones
  `demo`, `demo-web` and `demo-android` draw — ahead of the committed documents,
  with the scripted pulse on `demo/src/shell.rs`'s own 2500 ms cadence and the
  scene's own variant switch on the space bar where it declares one — of the
  three, only `layout` does. `Runtime/` gains the `ds_demo_*` declarations for
  it, behind `DASHSCENE_DEMO_PRODUCER`, which no shipped configuration defines:
  the entry points are exported by `unity/demo-producer`, a demonstration
  library that is `dashscene-ffi` plus seven calls, and the shipped C ABI still
  has no producer-side entry point. When layers 1 and 2 land the demonstration
  moves to C# and all of this goes away (story #1342,
  `docs/decisions/the-demo-producer-links-the-abi-rather-than-shipping-in-it.md`).
- **A showcase sample, and `just unity-demo` to build and run it.**
  `Samples~/Showcase` reads a manifest of documents from `StreamingAssets`,
  switches on the page keys or on a `-cycle <seconds>` argument, and reports the
  rung, the instance count and every construct the painter refused. The recipe
  stages the committed documents, the font cascade and — because the package
  ships no binary — the native library itself, which is why it demonstrates the
  package's C# and shaders as installed but says nothing about a released plugin
  layout (issue #1334). It is a demonstration rather than a gate: its `cycle`
  action asserts that every entry reached the painter, and `unity/render-gate`
  is what asserts anything about the picture (issue #1329).
- **Text.** The MSDF glyph atlas a run samples crosses the C ABI on its own call
  — `ds_runtime_atlas_count` and `ds_runtime_atlas`, keyed by a `GlyphRun`'s
  atlas index and carrying the sheet as well as the per-glyph placement, because
  an atlas index is the typesetter's font slot and not the index of the face a
  host passed (story #1123). `DashsceneRuntime.ReadAtlases` wraps it and
  `BrgPainter.SetAtlases` consumes it, one linear texture and one material over
  `Dashscene/Text` per sheet. Read the atlases after each load, not each frame:
  the set is installed by a load and is not part of a commit.
- `DashsceneRuntime.LoadDocumentWithText` and `TextFontFace` — the loader that
  takes a font cascade, which the package had left unwrapped, and without which
  a document's text shapes to nothing (story #1123).
- The C# declaration of boundary B — the value types `crates/dashpaint-abi`
  holds to a C representation (story #1239).
- The C# host on the C ABI: a P/Invoke declaration for every entry point, a
  thread-affine managed lifetime, the `ds_last_error_message` channel on every
  failure a `DsStatus` describes, and the committed frame under a lease that
  checks each array's stride before a row is read (story #1121).
- **A library that does not export an entry point this package calls is reported
  as `DashsceneSymbolMissingException`** — the R-E16 type — rather than as the
  `EntryPointNotFoundException` .NET raises where it binds an import, which is
  neither a `DashsceneException` nor a `DashsceneAbiMismatchException` and so
  escapes every catch a host is told to write. A package built after a symbol
  arrived and loaded against a library from before passes the version handshake,
  because adding a symbol does not move `DS_ABI_VERSION`, and then fails at the
  first call to it (issue #1308). **Catch `DashsceneAbiMismatchException`
  wherever you call**, not only around the constructor: the tick and the frame
  acquire raise it too. `Dispose` is the exception and does not throw — it
  records the refusal on `LastDisposeDetail` and leaves `LastDisposeStatus` at
  `Ok`, because no call answered a status. **Read the pair rather than either
  property**: a status is what a call answered, a detail is why, and neither on
  its own says whether the runtime was freed.
- `DocumentRange`, and `LoadDocumentMapped(DocumentRange, uint)` over it: a
  `.dsb` held as a byte range inside a larger file is mapped where it lies. That
  is what makes a document in `StreamingAssets` loadable on Android, where the
  path resolves to a `jar:file://…!/assets` URI inside the APK and the
  whole-file mapped loader answers `DsStatus.Map`. No copy to
  `Application.persistentDataPath`, so demand paging survives the first run
  (story #1124, issue #1288).
- `CommitPacer`, for committing below the display rate without drifting off it.
- A `Frame Loop` sample — a `MonoBehaviour` that loads a `.dsb`, ticks it, hands
  each committed frame to `BrgPainter` and marks it drawn (story #1121, issue
  #1298). It needs a host project meeting R-E4, R-E5 and R-E6.
- The `BatchRendererGroup` painter, in three material classes — unlit-overlay,
  lit-opaque and lit-cutout (story #1122). It draws fills, both solid and
  gradient, corner radii, strokes, clips, per-node opacity and rotation. What it
  does **not** draw is reported by name through `PackDiagnostic` rather than
  listed here — P4 forbids a silent drop, and the list belongs where it cannot
  go stale. It was written out here until this entry, and text sat in it until
  story #1123, the first entry above.
- The three material classes' `.shader` files sit under
  `Runtime/Resources/Dashscene/`, one per class, and `BrgPainter` loads each
  with `Resources.Load<Shader>` by the shader's own declared name. They were
  under `Runtime/Shaders/` and resolved with `Shader.Find` until issue #1313,
  which measured that a player build strips a shader no scene and no material
  references — so the painter threw its R-E2 diagnostic in the one configuration
  a consumer ships. A host referencing one of these by asset path has to follow
  the move.
- `Runtime/Shaders/Sdf.hlsl`, generated from
  `crates/dashscene-gpu/src/shaders/sdf.wgsl` by `naga` rather than ported by
  hand. The signed-distance, coverage and gradient math a Unity shader evaluates
  is the same compiled module the lean painter evaluates
  (`docs/specification/03-target-hardware-rules.md` R-T5). Do not edit it.
- A dependency on `com.unity.render-pipelines.universal`. The painter's shaders
  include URP's `Core.hlsl`, which is what reaches the DOTS instancing
  declarations a `BatchRendererGroup` needs.
- `.meta` files for every path Unity imports, without which a Git-URL package
  delivers nothing (R-E2), and a `unity` field declaring `6000.3` (R-E1).

### Changed

- **`PaintGlobals` is gone; its five names are now members of
  `PaintMaterialProperties`.** Nothing the painter binds is process-wide any
  more, so the class that named the global buffers has no subject. A host
  referencing `PaintGlobals.Paints` renames it to
  `PaintMaterialProperties.Paints`; the string values are unchanged.

### Fixed

- **Two painters in one process no longer shade from one paint heap.**
  `BrgPainter` bound `_DsPaints`, `_DsClipBoxes`, `_DsStrokes`, `_DsGlyphs` and
  the `_DsGlobals` scalars with `Shader.SetGlobalBuffer` and
  `Shader.SetGlobalVector`, both process-wide, so the last painter to draw
  supplied the gradients, strokes and clip boxes every painter's fragments read.
  It binds them on the materials it registered itself instead, and the
  constructor warning that told a host to "draw one document per process" is
  gone with the constraint it reported — **a second painter in one process is
  now a supported configuration** (issue #1297). All four shipped shaders gained
  a `_DsGlobals` entry in their `Properties` blocks, and the three that lacked
  one gained `_DsCutoff`: a `UnityPerMaterial` member a pass reads and the
  property section does not declare makes the pass SRP-Batcher-incompatible, and
  a `BatchRendererGroup` refuses every draw command that uses it. `_DsGlobals`
  is read by every class and was measured doing exactly that; `_DsCutoff` is
  read by one and drew all the same without its entry, and is declared
  everywhere rather than resting on an explanation no run measured.
- **The manifest no longer says the painter draws no text.** `package.json`'s
  `description` — what a UPM registry listing shows — still said the painter
  draws "no shadows, blurs, images or text" after story #1123 had landed the
  text seam. It now points at this package's README and at `PackDiagnostic`
  instead of carrying a list of its own (issue #1325).
- **The R-E5 warning no longer fires on a correctly configured host.**
  `BrgPainter` read `GraphicsSettings.useScriptableRenderPipelineBatching` in
  its constructor, and URP assigns that global inside its own pipeline
  instance's constructor — which Unity runs at the first render, after the
  `Awake` of the first frame where a host builds a painter. The global was
  therefore `false` in every process that had not yet rendered, whatever the
  project was set to. The read now happens in `Draw`, guarded on
  `RenderPipelineManager.currentPipeline`, and is decided once per pipeline
  instance rather than once per painter — so a host that switches to an asset
  with the batcher off is told (issue #1317).
- **Rung 3 is reported rather than selected silently.** Where
  `BatchRendererGroup.BufferTarget` answers
  `UnsupportedByUnderlyingGraphicsApi`, the painter took
  `BrgRung.InstancedWithoutBrg`, built no group and drew nothing, logging
  nothing — while R-E6's default produces a blank frame that Unity itself names
  on every frame. The two were indistinguishable from the console, and reading
  `Rung` was the only way to tell them apart. The constructor now warns on that
  arm (issue #1326).
