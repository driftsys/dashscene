# com.driftsys.dashscene

The C# side of dashscene.

**This package draws a subset, and ships no native library.** The
`BatchRendererGroup` painter landed at story #1122; the library the host loads
is built by `just host-lib` and placed by whoever ships a release
([R-E3](../../docs/specification/07-embedding-and-distribution.md), still
unmet), so a fresh install meets a `DllNotFoundException` on its first call.

**What the painter draws:** fills, both solid and gradient; corner radii;
strokes; clips; per-node opacity and rotation. **What it does not:** shadows,
layer blurs, backdrop blurs, image fills, baked vector nodes, render-target
groups, and text — the atlas a glyph run samples does not cross the C ABI at
all. Every one of those is reported by name through `PackDiagnostic` rather than
skipped quietly. A backdrop blur is the one that is not merely unbuilt: it reads
what the painter itself composited, and a Unity host's target also holds the
engine's own scene.

What is here:

- `Runtime/BoundaryB.cs` — the value types that cross boundary B, declared so a
  painter written against them agrees with the Rust side byte for byte.
- `Runtime/Native.cs` — the C ABI's entry points, as
  `crates/dashscene-ffi/include/dashscene.h` declares them.
- `Runtime/DocumentRange.cs` — where a `.dsb` is, when it does not begin a file.
  On Android that is a byte range inside `base.apk`.
- `Runtime/DashsceneRuntime.cs`, `DashsceneException.cs`, `FrameLease.cs` — the
  host: version negotiation, runtime lifetime, document load, the tick, and the
  committed frame under a lease.
- `Runtime/CommitPacer.cs` — committing below the display rate without drifting
  off it. In `Runtime/` rather than in the sample because no CI job compiles a
  sample, and this carries a numeric claim worth gating.
- `Runtime/PaintHeap.cs`, `PaintProperties.cs`, `PaintBindings.cs`,
  `FramePacker.cs`, `PackDiagnostics.cs` — the half of the painter that decides
  what the picture is: the heap layout, the per-instance and global binding
  names, the packing of the committed tables into instances, and what was
  skipped. Engine-independent, so it compiles under netstandard2.1 like
  everything else at this level.
- `Runtime/Engine/BrgPainter.cs` — the `BatchRendererGroup` itself: the buffer
  target and its rung, the instance upload, the batches and the culling
  callback. **The only file under `Runtime/` that references `UnityEngine`**,
  which is why it is one directory down: `unity/package-compat` has no Unity
  reference assemblies, so R-E10's netstandard check cannot compile it and a
  Unity editor checks it instead
  ([r-e10-is-checked-in-two-halves](../../docs/decisions/r-e10-is-checked-in-two-halves.md)).
- `Runtime/Resources/Dashscene/` — the three material classes' `.shader` files,
  one per class. **They sit in a `Resources` folder because a player build
  strips a shader that no scene and no material references** — the painter loads
  each with `Resources.Load<Shader>` by the shader's own declared name, so the
  name doubles as the path and there is no second constant to drift. Issue #1313
  is the run that measured the alternative failing.
- `Runtime/Shaders/` — the shading those three include, and `Sdf.hlsl`. An
  `.hlsl` is not loaded at run time, so it needs no `Resources` folder.
  **`Sdf.hlsl` is generated** from the lean painter's WGSL shader library by
  `naga` and must not be edited: the point is that both painters evaluate one
  compiled module rather than two ports of one file.
- `Samples~/FrameLoop/` — a `MonoBehaviour` driving all of it from
  `Time.deltaTime`. It is a sample rather than `Runtime/` code because R-E10
  requires every type under `Runtime/` to compile against netstandard2.1 and
  names `unity/package-compat` as the check; that project has no Unity reference
  assemblies, so a `MonoBehaviour` there fails R-E10's own check.
  `Runtime/
  Engine/` is the other answer to the same problem, for code that is
  not a sample.

**The runtime is thread-affine and has no finalizer.** It is reachable only from
the thread that created it, and `Dispose` must run there too — a finalizer runs
on the GC's thread, where `ds_runtime_free` answers `DS_WRONG_THREAD` and the
runtime leaks with nothing reported.

## What checks the declarations

Each check asks a different question and none subsumes another. `just unity-abi`
runs the first two below, `just unity-ffi` the third and `just test` the fourth
— all four run on any pull request whose diff is not documentation-only, and
**none of those four needs a Unity editor**. Three do need one and therefore run
on no CI runner: `just unity-editor`, the only thing that compiles a Unity
`.shader` without building a player and the one whose purpose is to compile
`Runtime/Engine/`; `just unity-conformance`, which evaluates the committed
layer-2 probe table through the generated `Runtime/Shaders/Sdf.hlsl` on a real
graphics device (issue #1312) and is the only one here that reads a shader's own
computed values back and compares them against a committed table; and
`just unity-render`, which builds a player and draws a document through the
painter, so it runs shader code too and reads pixels.

`unity/package-compat` compiles `Runtime/` **minus `Runtime/Engine/`** against
**netstandard2.1**, which is what Unity's default API compatibility level
accepts — R-E10. `unity/abi-check` cannot stand in for it: that project targets
`net10.0`, a strict superset, so it accepts declarations Unity would refuse.

`unity/ffi-check` **loads a `dashscene-ffi` cdylib and calls it.** (`abi-check`
executes too, against `dashpaint-abi`; `package-compat` is the one that only
compiles.) It looks up every declared entry point (.NET binds a `DllImport`
lazily, so one nothing calls would otherwise be checked by nothing), performs
R-E16's version handshake, produces each status from a real call rather than
reading the header, and compares all nineteen of a frame's `DsSlice::stride`
values against this package's row sizes, which is R-E17.

It also builds more libraries beside it, each exporting less than this package
calls, and drives the package against each in its own `AssemblyLoadContext`.
That is the one failure a version number cannot report: adding an entry point
does not move `DS_ABI_VERSION`, so a package newer than the library it loads
agrees on the version and then fails where .NET binds the import. Every entry
point but `ds_abi_version` turns that into `DashsceneSymbolMissingException` —
an issue #1308 defect if it does not, and `Runtime/Native.cs` is where the
translation lives. That one is the version read itself, so its absence has no
version to report and is handed back as it arrived. `Dispose` is the one place
the refusal is recorded rather than thrown, because it runs during unwinding.

`unity/abi-check` compiles **this package's own `BoundaryB.cs`**, builds
`crates/dashpaint-abi` as a dynamic library, and compares every type against
what the Rust build reports — member by member, matched by name.

It catches anything wrong with **this file**: a member added, removed, renamed,
moved or widened, including two same-width members exchanged and an enum that
lost its `: byte`.

Two things it does not catch, both measured rather than reasoned about:

- A member whose C# type has the right size and the wrong meaning — `uint`
  declared as `float`.
- A member added to the **Rust** type that fits inside padding already there, so
  no size and no offset moves. `abi_surface!`'s member lists are hand-written,
  which is what leaves that open; issue #1252 carries it.

`unity/abi-check/Program.cs` states both.

## What a consumer must satisfy

Story #1125's spike settled the questions a consumer will ask — the distribution
form, the minimum Unity version, the render pipeline, the scripting backend, the
API compatibility level, and where the native library sits per platform. They
are requirements now, in
[`../../docs/specification/07-embedding-and-distribution.md`](../../docs/specification/07-embedding-and-distribution.md),
with the reasoning in the three records
[`../../docs/decisions/README.md`](../../docs/decisions/README.md) lists under
story #1125.

Story #1121 closed the ones about this package's own contents — the `.meta`
files, the `unity` field, `allowUnsafeCode`, and the two the host owes the ABI.
**What remains unmet is about a release rather than about this directory**: the
native library and the git tag that names it.

Read
[the requirements file](../../docs/specification/07-embedding-and-distribution.md)
for the current set. This is a pointer and not a census — an enumeration here is
a second place for the status to go stale, and it already had been.

## Licence

Apache-2.0. `LICENSE` and `NOTICE` travel with the package because §4 of that
licence requires both to.

**`LICENSE`, not `LICENSE.md`**, which is what UPM conventionally expects. The
repository formats every tracked Markdown file with `prim`, and on the `.md`
name it rewrites 325 lines of the licence text. The extensionless name is what
the Rust crates already carry for the same reason.
