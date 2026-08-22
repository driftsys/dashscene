# unity/

The Unity C# package, and the three checks that hold it to the Rust build.

    com.driftsys.dashscene/   the UPM package — boundary B's types and the C#
                              host; no painter, no native library
    abi-check/                a plain .NET check, no Unity editor needed
    package-compat/           a netstandard2.1 compile of the package's
                              Runtime/, which abi-check cannot do — it targets
                              net10.0, a superset of what Unity accepts
    ffi-check/                the package's P/Invoke declarations, executed
                              against a dashscene-ffi cdylib

Sited in this repository rather than in a separate one by the owner's ruling of
2026-08-17, recorded in
[`../docs/decisions/unity-package-sited-in-this-repository.md`](../docs/decisions/unity-package-sited-in-this-repository.md).
UPM installs from a Git URL with `?path=`, so a subfolder is directly
consumable.

**Sharing a repository gains nothing on its own** — that record says so, and the
checks here are what give it value. `just unity-abi` runs the first two;
`just unity-ffi` runs the third.

The three ask different questions and none subsumes another. `abi-check`
compares boundary B's value types against a `dashpaint-abi` build, member by
member; `package-compat` asks whether Unity could compile the package at all,
and is the only one of the three that executes nothing; `ffi-check` loads
`dashscene-ffi` and calls it. Until story #1121 nothing compiled a C# P/Invoke
against `crates/dashscene-ffi/include/dashscene.h`, which is item 2 of issue
#1266 — but `abi-check` has always executed: it declares sixty `[DllImport]`s
and round-trips structs by value through the library.

None of them reads a shipped binary. All three build both halves from one tree,
so they observe only a disagreement this repository already contains — a stale
committed library is what `DsSlice::stride` catches at run time, which is why
`07-embedding-and-distribution.md` R-E17 makes that check mandatory in the host
rather than advisory.

This directory is outside the Cargo workspace, as `importers/` is, and carries
its own `unity` commit scope in `.git-std.toml`.
