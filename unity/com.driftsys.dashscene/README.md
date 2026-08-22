# com.driftsys.dashscene

The C# side of dashscene.

**This package does not draw.** There is no painter here and no native library —
the `BatchRendererGroup` painter is story #1122, and the library the host loads
is built by `just host-lib` and placed by whoever ships a release
([R-E3](../../docs/specification/07-embedding-and-distribution.md), still
unmet). What is here:

- `Runtime/BoundaryB.cs` — the value types that cross boundary B, declared so a
  painter written against them agrees with the Rust side byte for byte.
- `Runtime/Native.cs` — the C ABI's fourteen entry points, as
  `crates/dashscene-ffi/include/dashscene.h` declares them.
- `Runtime/DashsceneRuntime.cs`, `DashsceneException.cs`, `FrameLease.cs` — the
  host: version negotiation, runtime lifetime, document load, the tick, and the
  committed frame under a lease.
- `Runtime/CommitPacer.cs` — committing below the display rate without drifting
  off it. In `Runtime/` rather than in the sample because nothing compiles a
  sample, and this carries a numeric claim worth gating.
- `Samples~/FrameLoop/` — a `MonoBehaviour` driving all of it from
  `Time.deltaTime`. It is a sample rather than `Runtime/` code because R-E10
  requires every type under `Runtime/` to compile against netstandard2.1 and
  names `unity/package-compat` as the check; that project has no Unity reference
  assemblies, so a `MonoBehaviour` there fails R-E10's own check.

**The runtime is thread-affine and has no finalizer.** It is reachable only from
the thread that created it, and `Dispose` must run there too — a finalizer runs
on the GC's thread, where `ds_runtime_free` answers `DS_WRONG_THREAD` and the
runtime leaks with nothing reported.

## What checks the declarations

Three projects, asking three questions. `just unity-abi` runs the first two and
`just unity-ffi` runs the third; all three run on any pull request whose diff is
not documentation-only, and **none needs a Unity editor**.

`unity/package-compat` compiles the whole of `Runtime/` against
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
