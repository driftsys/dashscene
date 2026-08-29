# unity/

The Unity C# package, and the checks that hold it to what the rest of this
repository builds and records.

    com.driftsys.dashscene/   the UPM package — boundary B's types, the C#
                              host, the BatchRendererGroup painter, and the
                              native libraries for macOS arm64 and
                              Android arm64
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
                              the shader pragmas R-E11 and R-E12 require, the
                              R-E10 split above, R-E21's platform data over
                              each shipped native library, BrgPainter's two
                              diagnostics — where R-E5's read sits and what
                              guards it, and that the rung-3 arm reports the
                              rung — and, since PR #1372, that the paint heap
                              binds per material rather than globally: no
                              `SetGlobal…` setter in the compiled half, every
                              bound name reaching a setter through a static
                              property id, the bindings inside `Draw`'s own call
                              and after the upload that can replace the buffers,
                              the glyph rows on the text materials alone, every
                              heap buffer freed after the materials naming it,
                              and every `UnityPerMaterial` member declared by
                              every shader that reads it. Text over a file no CI job compiles. Needs
                              no .NET SDK and no editor, so it runs on any pull
                              request whose diff touches code — as, since story
                              #1342, does `demo-producer/`'s `just
                              demo-exports`. It is no longer the only one
    editor-compat/            an editor script `just unity-editor` copies into a
                              throwaway Unity project: the WHOLE package
                              compiled, shaders included. Since issue #1322's Mono
                              reading it also declares two `[DllImport]`s of its own
                              against the shipped library — one exported, one
                              no library exports — and requires the second to
                              raise `EntryPointNotFoundException` — or SKIPS,
                              in the summary line, on a host where D3 ships no
                              library an editor can load, which is every host
                              but the one its editor-compatible row names.
                              That is the
                              runtime behaviour every forwarder in `Native.cs`
                              rests on, observed on Mono rather than on CoreCLR
                              (issue #1322). Needs an editor, so it runs on no
                              CI runner
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
                              and reads that back. The only CHECK here that
                              draws — `demo/` below draws too, and asserts
                              nothing. A player because a player is where Unity
                              strips a shader nothing references, which is the
                              class no check above can see (issue #1313) — so
                              this project deliberately adds nothing to Always
                              Included Shaders. Needs an editor, so it runs on
                              no CI runner
    android-probe/            two scripts `just unity-android` copies into a
                              throwaway project, plus the mutation table
                              `just unity-android-negative` applies to a COPY of
                              them under target/. The recipe configures that
                              project the way R-E7, R-E8 and R-E9 require,
                              builds an Android PLAYER and runs it on an
                              attached device to read
                              `BatchRendererGroup.BufferTarget` there — D4's
                              rung, which story #1125 had read on Metal only.
                              One of the two things here that build a player
                              for anything but macOS, and one of the two that
                              need a device as well as an editor —
                              `unity-demo-android` is the other, and its own
                              negative control needs both as well. It does NOT check
                              R-E7, R-E8 or R-E9: it writes the values it reads
                              back, and those three bind the shipping project
                              rather than one regenerated under target/ (issue
                              #1353). What it does read off an artifact is the
                              APK's ABI directories
    demo/                     an editor script `just unity-demo` and
                              `just unity-demo-android` both copy into a
                              throwaway project: it configures the project the
                              way R-E4, R-E5 and R-E6 require, then builds a
                              windowed macOS PLAYER over the package's Showcase
                              sample and runs it — or, when
                              `DASHSCENE_DEMO_TARGET=android` is set, an arm64
                              APK through IL2CPP for a device. A demonstration rather than a
                              check: its `cycle` action asserts that every
                              entry reached the painter, and a person
                              decides whether the picture is right (issue
                              #1329). Needs an editor,
                              so it runs on no CI runner
    demo-producer/            the native producer that player draws the
                              showcase scenes through: `dashscene-ffi` linked
                              as an rlib plus seven `ds_demo_*` entry points, so
                              it carries ONE runtime table and a handle minted
                              by `ds_runtime_new` resolves in `ds_demo_build`.
                              A separate crate and not a feature of the shipped
                              one because that crate is published and
                              `corpus/showcase` is unpackageable in principle
                              (story #1342). `just demo-exports` holds it to
                              the shipped seventeen plus a set carrying only
                              the `ds_demo_` prefix — `unity/ffi-check`'s
                              demonstration pass is what names the six. Needs
                              neither an editor nor the .NET SDK, so CI's
                              `demo-build` job runs it on every code diff

Sited in this repository rather than in a separate one by the owner's ruling of
2026-08-17, recorded in
[`../docs/decisions/unity-package-sited-in-this-repository.md`](../docs/decisions/unity-package-sited-in-this-repository.md).
UPM installs from a Git URL with `?path=`, so a subfolder is directly
consumable.

**Sharing a repository gains nothing on its own** — that record says so, and the
checks here are what give it value. `just unity-abi` runs `abi-check` and
`package-compat`, `just unity-ffi` runs `ffi-check`, `just test` runs
`package-gate`, `just unity-editor` runs `editor-compat`,
`just unity-conformance` runs `hlsl-conformance`, `just unity-render` runs
`render-gate`, `just unity-android` runs `android-probe` — with
`just unity-android-negative` as its negative control, on
`unity-conformance-negative`'s shape — and `just unity-demo` runs `demo` over
`demo-producer`, as does `just unity-demo-android`, for Android.

They ask different questions and none subsumes another. `abi-check` compares
boundary B's value types against a `dashpaint-abi` build, member by member;
`package-compat` asks whether Unity could compile the engine-free half at
netstandard2.1, and executes nothing; `ffi-check` loads `dashscene-ffi` and
calls it, and loads deliberately incomplete libraries beside it, each in its own
`AssemblyLoadContext`, to watch a missing entry point become the R-E16 type
(issue #1308); `package-gate` re-derives the generated HLSL, holds each shipped
library's `.meta` and header to D3, and holds the prose over the editor-only
gates to the code in them — the recorded status of R-E7, R-E8 and R-E9,
`unity-android-negative`'s mutation table, and what `just unity-editor` is said
to ask; `editor-compat` is the only one that compiles a Unity `.shader` without
building a player, and the only one that watches a missing entry point become
`EntryPointNotFoundException` on a runtime Unity ships; `hlsl-conformance` is
the only one that dispatches shader code and compares the values it computed
against a committed table; `render-gate` is the only one that builds a player
and asserts what it drew — `demo` builds one and asserts only that every entry
reached the painter, and `android-probe` builds one for a second platform and
asserts a value read on the device rather than a picture. Until story #1121
nothing compiled a C# P/Invoke against
`crates/dashscene-ffi/include/dashscene.h`, which is item 2 of issue #1266 — but
`abi-check` has always executed: it declares sixty `[DllImport]`s and
round-trips structs by value through the library.

**Two of them read a shipped binary, both since story #1334.** `render-gate` was
de-staged, so the player it builds loads the library the package ships and runs
the `ds_abi_version` handshake and the per-array `DsSlice::stride` comparison
against it; and `package-gate` reads each shipped library's header far enough to
check its architecture and container against D3. The rest build both halves from
one tree, so they observe only a disagreement this repository already contains.

**No check compares a shipped binary against the sources of the commit that
carries it**, which is the different question: an architecture match is not a
freshness check, and a stale library of the right architecture passes every one
of them. That is what `DsSlice::stride` catches at run time, which is why
`07-embedding-and-distribution.md` R-E17 makes that check mandatory in the host
rather than advisory.

This directory is **almost** outside the Cargo workspace: `package-gate` is a
workspace member (`publish = false`, added by story #1122) and everything else
here is outside it, as `importers/` is. It carries its own `unity` commit scope
in `.git-std.toml`.
