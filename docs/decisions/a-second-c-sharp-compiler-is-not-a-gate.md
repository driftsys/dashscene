# A second C# compiler is not a gate for the editor-only sources

    status   accepted (2026-09-05, owner's ruling on issue #1350). It refuses
             ONE shape — Roslyn as a library, parsed on a pull request — and
             rules nothing about NuGet in general, nor about the other two
             shapes that issue names
    scope    the eight editor-only C# files under unity/android-probe/,
             unity/demo/, unity/editor-compat/, unity/hlsl-conformance/ and
             unity/render-gate/, which no .csproj compiles. Not the package's
             own Runtime/ or Samples~/, which are issue #1331's
    related  docs/decisions/the-native-library-ships-inside-the-unity-package.md
             (D4 — no Unity editor on a CI runner, which is why shape 1 cannot
             run on a pull request)
             docs/decisions/cargo-lock-is-committed.md (the pinning policy the
             Rust half has and this half has never needed)

Eight C# files carry this repository's editor-side gates and **no `.csproj`
compiles any of them**, so a file that stops compiling is found by a developer
minutes into an editor run rather than on the pull request that broke it. Issue
#1350 names three shapes that could close that. This record refuses the third.

## The refusal

**No `Microsoft.CodeAnalysis.CSharp` as a `PackageReference`, called for
`CSharpSyntaxTree.ParseText` over these files on a pull request.**

## Why — the compiler would be the wrong one

This is the reason that decides it, and it is measured rather than argued.
**Unity ships its own Roslyn**, and it is four language versions behind the one
a `PackageReference` would resolve:

    $EDITOR/Unity.app/Contents/Resources/Scripting/DotNetSdkRoslyn/csc.dll
      csc.deps.json  Microsoft.CodeAnalysis.CSharp/4.3.1-3.22526.13, net6.0
      -langversion:? highest is 10.0 (default)

    <sdk>/Roslyn/bincore/csc.dll
      csc.deps.json  Microsoft.CodeAnalysis.CSharp/5.9.0-1.26379.115, net10.0
      -langversion:? highest is 14.0 (default)

Editor `6000.3.23f1` and SDK `10.0.400`. Derive both with
`dotnet <csc.dll> -langversion:?` rather than trusting the numbers here.

A newer compiler accepts a **superset**: C# 11, 12, 13 and 14 syntax parses
clean and the editor rejects it. So the gate would be green on the pull request
and broken the moment someone opens Unity — issue #1350's own failure mode,
reached from the other side, and in the direction that matters. A gate that
fails closed would merely be noisy; this one fails open.

**Matching the versions is a synchronisation, not a setting.** A library parse
takes its level from `CSharpParseOptions.LanguageVersion`, not from the hosting
project's MSBuild `LangVersion`, so it would be a constant in the gate's own
source that has to move whenever the editor does — a second `ANDROID_API`, which
`docs/design/android-toolchain.md` records went stale for two stories the last
time such a number moved.

## Why — the precedent, which is the smaller half

It would be the first `PackageReference` in this repository. There are three
`.csproj` — `unity/abi-check` and `unity/ffi-check` on `net10.0`, and
`unity/package-compat` on `netstandard2.1` — and none has one, nor is there a
`nuget.config`, a `packages.lock.json` or a `Directory.Build.props`. So it opens
a supply chain together with the policy that governs it: pinning, lock files,
restore sources, and whether an offline build still works. The Rust half settled
its version of that in `cargo-lock-is-committed.md`; this half has never had to.

Nothing pins the SDK either — there is no `global.json`, and CI asks for
`10.0.x` — so the resolved Roslyn is a property of whichever machine runs the
build. That widens the gap above rather than narrowing it.

## What this does not rule

**Not NuGet in general.** A package for some other purpose is a separate
question, and this record is not an argument against one.

**Not the other two shapes.** Referencing an editor's own managed assemblies and
vendoring a facade of Unity's API are both still open on issue #1350, with the
costs that issue measures.

## What it leaves, stated plainly

**No shape that satisfies issue #1350's "Done when" remains unruled and cheap.**
That criterion is a job on _every pull request_, and:

- Shape 1 needs an installed editor, which D4 rules out for a CI runner.
- Shape 2, a vendored facade, could run on a pull request. It is the only
  remaining candidate that could, and it is neither ruled nor small: these files
  reach `UnityEngine`, `UnityEditor`, `UnityEditor.Build`,
  `UnityEditor.Build.Reporting`, `UnityEditor.SceneManagement`,
  `UnityEngine.Rendering`, URP's `UnityEngine.Rendering.Universal` and the
  package's own `Runtime/Engine/` types.
- Shape 3 is this refusal.

So issue #1350 stays open and the gap stands. **Issue #1316's factoring of the
throwaway-project scaffolding does not close it** — a job that compiles these
files inside a real Unity project uses the right compiler and catches binding
errors, which a parser never would, but it runs locally and not on a pull
request. It is a real improvement on nothing and it is not the criterion.

## What would reopen this

An editor whose bundled Roslyn and the SDK's agree on a language version, or a
Unity that publishes its compiler as a package this repository could pin to the
editor it targets. Either removes the reason above; the precedent question would
then be the whole of it.
