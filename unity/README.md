# unity/

The Unity C# package, and the checks that hold it to what the rest of this
repository builds and records.

    com.driftsys.dashscene/   the UPM package — boundary B's types, the C#
                              host, and the BatchRendererGroup painter; no
                              native library
    abi-check/                a plain .NET check, no Unity editor needed
    package-compat/           a netstandard2.1 compile of the package's
                              Runtime/ MINUS Runtime/Engine/, which holds the
                              engine-referencing half. abi-check cannot do this
                              — it targets net10.0, a superset of what Unity
                              accepts — and this project cannot do the engine
                              half, because it has no Unity reference
                              assemblies (issue #1286)
    ffi-check/                the package's P/Invoke declarations, executed
                              against a dashscene-ffi cdylib. Same exclusion,
                              for its own reason: it compiles Runtime/ only
                              because that is where the declarations live.
                              `older-library.c` beside it is built into
                              several libraries that export LESS than the
                              package calls, so the gate can provoke a package
                              newer than its library rather than describe it —
                              which is why this one also needs a C compiler.
                              That file enumerates the builds
    package-gate/             a Rust workspace member, in the sanity test tier:
                              the HLSL derived from the WGSL shader library,
                              the shader pragmas R-E11 and R-E12 require, and
                              the R-E10 split above. The only check here with
                              no .NET and no editor prerequisite, so the only
                              one with no .NET SDK and no editor prerequisite,
                              so the only one that runs on any pull request
                              whose diff touches code
    editor-compat/            an editor script `just unity-editor` copies into a
                              throwaway Unity project: the WHOLE package
                              compiled, shaders included. Needs an editor, so it
                              runs on no CI runner
    hlsl-conformance/         a compute shader and an editor script
                              `just unity-conformance` copies into a throwaway
                              Unity project: the committed layer-2 probe table
                              evaluated through the generated Sdf.hlsl on a real
                              graphics device (issue #1312). The only check here
                              that reads a shader's own computed VALUES back and
                              compares them against a committed table — the one
                              below runs shader code too, and reads PIXELS.
                              Needs an editor, so it runs on no CI runner
    render-gate/              two scripts `just unity-render` copies into
                              another throwaway project: it builds a PLAYER,
                              runs it, draws a document into a RenderTexture
                              and reads that back. The only thing here that
                              draws. A player because a player is where Unity
                              strips a shader nothing references, which is the
                              class no check above can see (issue #1313) — so
                              this project deliberately adds nothing to Always
                              Included Shaders. Needs an editor, so it runs on
                              no CI runner

Sited in this repository rather than in a separate one by the owner's ruling of
2026-08-17, recorded in
[`../docs/decisions/unity-package-sited-in-this-repository.md`](../docs/decisions/unity-package-sited-in-this-repository.md).
UPM installs from a Git URL with `?path=`, so a subfolder is directly
consumable.

**Sharing a repository gains nothing on its own** — that record says so, and the
checks here are what give it value. `just unity-abi` runs `abi-check` and
`package-compat`, `just unity-ffi` runs `ffi-check`, `just test` runs
`package-gate`, `just unity-editor` runs `editor-compat`,
`just unity-conformance` runs `hlsl-conformance` and `just unity-render` runs
`render-gate`.

They ask different questions and none subsumes another. `abi-check` compares
boundary B's value types against a `dashpaint-abi` build, member by member;
`package-compat` asks whether Unity could compile the engine-free half at
netstandard2.1, and executes nothing; `ffi-check` loads `dashscene-ffi` and
calls it, and loads deliberately incomplete libraries beside it, each in its own
`AssemblyLoadContext`, to watch a missing entry point become the R-E16 type
(issue #1308); `package-gate` reads sources and re-derives the generated HLSL;
`editor-compat` is the only one that compiles a Unity `.shader` without building
a player; `hlsl-conformance` is the only one that dispatches shader code and
compares the values it computed against a committed table; `render-gate` is the
only one that builds a player and draws. Until story #1121 nothing compiled a C#
P/Invoke against `crates/dashscene-ffi/include/dashscene.h`, which is item 2 of
issue #1266 — but `abi-check` has always executed: it declares sixty
`[DllImport]`s and round-trips structs by value through the library.

None of them reads a shipped binary. Those that build a Rust half build both
halves from one tree, so they observe only a disagreement this repository
already contains — a stale committed library is what `DsSlice::stride` catches
at run time, which is why `07-embedding-and-distribution.md` R-E17 makes that
check mandatory in the host rather than advisory.

This directory is **almost** outside the Cargo workspace: `package-gate` is a
workspace member (`publish = false`, added by story #1122) and everything else
here is outside it, as `importers/` is. It carries its own `unity` commit scope
in `.git-std.toml`.
