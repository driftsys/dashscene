# R-E10 is checked in two halves: netstandard2.1 in CI, an editor for the engine half

    status   accepted (story #1122, 2026-08-23); resolves debt #1286
    scope    unity/package-compat, unity/com.driftsys.dashscene/Runtime/, and
             R-E10's wording in
             docs/specification/07-embedding-and-distribution.md. It does not
             touch R-E11, R-E12 or R-E13, which are about the painter's shaders
             and its assembly definition
    related  docs/decisions/unity-package-sited-in-this-repository.md (why the
             package is here at all),
             docs/decisions/the-native-library-ships-inside-the-unity-package.md
             (D4, which records that no CI runner here can host a Unity
             install),
             docs/decisions/unity-painter-uses-brg.md (D1, the painter that
             forced this)

## Context

`docs/specification/07-embedding-and-distribution.md` **R-E10** requires every
C# type under `unity/com.driftsys.dashscene/Runtime/` to compile against
`netstandard.dll` 2.1.0, so the package builds under
`ApiCompatibilityLevel.NET_Standard`. It named one check:
`unity/package-compat`, a plain `netstandard2.1` library that globs
`Runtime/**/*.cs`.

**That project has no Unity reference assemblies.** So a type referencing
`UnityEngine` fails there with `CS0246`, whatever its API compatibility level
actually is. The requirement is about which BCL surface the package uses; the
check is about what one reference-less project can compile. Those are different
questions, and until story #1122 nothing separated them because the package
happened to contain no Unity-facing type — story #1121 met R-E10 by keeping
every type it added engine-independent and putting its `MonoBehaviour` frame
loop in `Samples~/`, which the `~` suffix hides from both Unity's importer and
the glob.

**Story #1122 has no such option.** The `BatchRendererGroup` painter must
reference `UnityEngine`, and it is runtime code rather than a sample — R-E13
already anticipates it, requiring `allowUnsafeCode` because
`BatchCullingOutputDrawCommands` exposes raw pointer fields. Issue #1286 was
filed from story #1121 predicting exactly this, and predicting the repair as
well: the first commit that adds the painter turns `just unity-abi` red, and the
cheap-looking fix is to exclude the file from the glob, which silently narrows
R-E10 to whatever is left.

**Reproduced before deciding.** Adding `Runtime/Engine/BrgPainter.cs` and
running `dotnet build unity/package-compat` produced 26 `CS0246` errors on the
first pass and 52 on the second, naming `Mesh`, `GraphicsBuffer`, `BatchID`,
`BatchMeshID` and the rest. So this is the measured failure rather than the
predicted one.

## Options

Issue #1286 named three, and none was obviously right.

1. **Give `package-compat` Unity reference assemblies.** Answers the real
   question exactly — the compiler would see `netstandard.dll` 2.1.0 _and_
   `UnityEngine.dll`, which is what Unity itself compiles the package against.
   It needs `UnityEngine.dll` from an editor install, and
   `the-native-library-ships-inside-the-unity-package.md` D4 already records
   that no CI runner here can host one. Unity's reference assemblies are also
   not ours to commit.
2. **Split `Runtime/` by whether a file references the engine**, gate the
   engine-free half at netstandard2.1, and amend R-E10 to say so. Honest about
   what is checked, and narrows the requirement.
3. **Check the painter's API level a different way** — the `.asmdef`'s own
   constraints, or a Unity batchmode compile in a job that is not CI.

## Decision

**D1 — R-E10 keeps its scope and gains a second check.** The requirement still
binds every C# type under `Runtime/`. What changes is that it names two checks
instead of one, because no single project can ask the whole question. This is
options 2 and 3 together: the split is what makes the CI half honest, and the
editor compile is what stops the split from narrowing the requirement.

**D2 — `Runtime/Engine/` holds every engine-referencing file, and every project
that globs `Runtime/` excludes exactly that directory.** Everything else under
`Runtime/` stays engine-free and is compiled against `netstandard.dll` 2.1.0 on
every pull request, as before. Each project prints what it skipped on every run,
because the narrowing this exclusion permits is one nobody would otherwise see.

**The class, not `unity/package-compat` alone**, and that wording is the result
of getting it wrong: story #1122 excluded the directory there and left
`unity/ffi-check`, which carries the same glob for an entirely different reason
— its question is whether the package's P/Invoke declarations match the library,
and it compiles the whole of `Runtime/` only because that is where those
declarations live. `just unity-ffi` then failed with the same `CS0246`, reading
as an unrelated defect. `unity/abi-check` is unaffected because it names
`BoundaryB.cs` alone.

**D3 — `just unity-editor` is R-E10's second check**, and it compiles the whole
package — engine half included — in a Unity editor, asserting the project's API
compatibility level is `NET_Standard` rather than assuming it. It needs an
editor, so it is outside CI and outside `just check`, and it says so. It is a
developer's gate, run before opening a pull request that touches
`Runtime/Engine/` or `Runtime/Shaders/`.

**D4 — the split is guarded by three assertions, not by review.** They are in
`unity/package-gate`, a Rust crate in the sanity test tier, so they run on every
pull request with no editor and no .NET SDK:

- **every** project under `unity/` whose sources glob
  `../com.driftsys.dashscene/Runtime/**/*.cs` carries an `Exclude` of
  **exactly** `../com.driftsys.dashscene/Runtime/Engine/**/*.cs`, and exactly
  one `Exclude`; and the set of such projects is not empty. This is the
  assertion #1286's "cheap-looking repair" fails, and it is stated over the
  class because a version stated over one project passed while the sibling was
  broken.
- every file under `Runtime/Engine/` references the engine, so the exclusion
  cannot be used to move an engine-free file out of the checked half.
- the count of engine-referencing files is **printed**, so a reader of a green
  run knows whether the second assertion ran over anything.

**D5 — what is put where is a design rule, not a filing convention.** The half
of the painter that decides what the picture is — reading the committed tables,
resolving kinds and rows, packing the paint heap — is engine-independent and
lives in `Runtime/`, where a check with no editor compiles it and where
`unity/ffi-check` **does** execute it — against a real committed frame, and
against synthetic frames that vary one property at a time. This clause said
"could" when it was written, and was true of nothing until the review of story
#1122 observed that the split's stated benefit was unrealised. `Runtime/Engine/`
holds the `BatchRendererGroup` lifecycle, the buffer upload and the culling
callback, and decides nothing about the picture. A file that could be written
without the engine and is not is a defect against this rule, and D4's second
assertion is its weakest form.

## Consequences

**R-E10's engine half is not checked on any pull request.** That is the cost,
and it is stated rather than hidden: CI compiles the engine-free half, and the
engine half's API compatibility level is confirmed by a developer running
`just unity-editor`. The alternative — option 1 — is blocked on a CI runner that
can host a Unity install, which D4 of
`the-native-library-ships-inside-the-unity-package.md` records this repository
does not have. **If that ever changes, option 1 supersedes this record**: one
project compiling `Runtime/**` against `netstandard.dll` 2.1.0 _and_ Unity's
reference assemblies answers the whole question in CI and makes D2, D3 and D4
unnecessary.

**The editor gate is not only R-E10's.** It compiles the package's shaders as
well, which is the half `unity/package-gate` cannot reach: that crate reads the
`#pragma` lines R-E11 and R-E12 require, and a pragma that is present while an
include path is wrong passes it. Nothing else in this repository compiles a
shader.

**It also writes the `.meta` files R-E2 requires.** A `file:` dependency is a
mutable package, so the editor writes a `.meta` beside every asset it imports —
in the working tree, which is how story #1121's were made and what R-E2 records
as the only acceptable way to make them. Whoever adds a file under the package
runs this recipe and commits what it wrote.

**Two names now have to stay in step by hand**, and each has a test rather than
a convention: the per-instance shader property names, declared in
`Runtime/PaintProperties.cs` and in every shader's `Properties` block, and the
shader names, declared in `Runtime/PaintHeap.cs` and by each `.shader`. A
BatchRendererGroup binds a property by name, so a name present on one side and
absent on the other is neither a compile error nor a run-time error — the shader
reads the property's default and draws a plausible wrong picture. That is why
`unity/package-gate` asserts both sets in both directions, and why the package
keeps one file per binding kind: with per-instance, per-material and global
names in one file, the gate would have to guess which is which.
