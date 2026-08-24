# Changelog

All notable changes to this package are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version tracks
the Cargo workspace rather than moving on its own.

## [Unreleased]

### Added

- **A showcase sample, and `just unity-demo` to build and run it.**
  `Samples~/Showcase` reads a manifest of documents from `StreamingAssets`,
  switches on the arrow keys or on a `-cycle <seconds>` argument, and reports
  the rung, the instance count and every construct the painter refused. The
  recipe stages the committed documents, the font cascade and — because the
  package ships no binary — the native library itself, which is why it
  demonstrates the package's C# and shaders as installed but says nothing about
  a released plugin layout (issue #1334). It is a demonstration rather than a
  gate: its `cycle` action asserts that every document reached the painter, and
  `unity/render-gate` is what asserts anything about the picture (issue #1329).
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

### Fixed

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
