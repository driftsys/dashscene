# The Unity package is installed from a Git URL, and its `.meta` files are committed

    status   accepted (2026-08-21, story #1125's spike). Settles question 1
             of issue #1125 — package form and distribution — and the
             `.meta` property that issue's own comment of 2026-08-18 raised.
    date     2026-08-21
    scope    unity/com.driftsys.dashscene — how it reaches a customer's
             project, and what must be committed for it to deliver anything
    related  docs/decisions/unity-package-sited-in-this-repository.md (where
             the package lives; its install URL is corrected by this record)
             docs/specification/07-embedding-and-distribution.md (R-E1, R-E2,
             R-E3)
             docs/decisions/publishable-and-the-first-version.md (0.2.0)

## Context

`unity-package-sited-in-this-repository.md` put the package in this repository
and named a Git URL as how UPM consumes it. It did not settle the form as a
question, and it left the version, the minimum editor and the upgrade path to
this spike. Story #1239 built the package deliberately carrying none of them.

## Decision

**D1 — UPM, installed from a Git URL with `?path=`, for as long as the package
is declarations plus a native library.** It needs no registry, no account and no
Unity editor in CI, and it is the form the siting already assumed.

**D2 — the install URL in `unity-package-sited-in-this-repository.md` was wrong,
and is corrected there rather than restated here.** That record gave, until the
commit that landed this one:

    https://github.com/driftsys/dashscene.git?path=/unity#<tag>

`unity/` holds `README.md`, `abi-check/` and `com.driftsys.dashscene/` and **no
`package.json`**. Unity's manual on Git dependencies states that "the subfolder
you specify must contain the package manifest (`package.json` file)", so that
URL resolves to nothing. The path is `/unity/com.driftsys.dashscene`.

**D3 — every file and folder Unity imports carries a committed `.meta`, and this
is a correctness requirement rather than tidiness.**

A Git-URL package is **immutable**: Unity's glossary says so — "Most packages
are immutable, including packages downloaded from the package registry or by Git
URL" — and only Local and Embedded packages are mutable. In an immutable folder
Unity does **not** generate a missing `.meta`. Its own error string says what it
does instead, and it sits beside the source-file marker
`./Modules/PackageManager/Editor/PackageManagerImmutableAssets.cpp` in the
`6000.3.22f1` binary:

    Asset %s has no meta file, but it's in an immutable folder.
    The asset will be ignored.

So the package as story #1239 left it — **zero `.meta` files** — installs,
appears in the package list, and delivers nothing: the `.asmdef` is never
imported, no assembly is produced, and `BoundaryB.cs` never compiles.

**The issue comment that raised this named a different mechanism, and the
difference changes the remedy.** It reported that Unity would generate the
`.meta` files locally and that asset GUIDs would therefore be per-machine,
breaking anything referencing the package by GUID. That describes the `Assets/`
behaviour, not the package one. Nothing in the package uses a GUID reference
today — the `.asmdef` has no `references` array at all, and `BoundaryB.cs`
declares only enums and structs, so it can never be the target of a
`m_Script: {guid: …}` reference. The defect is **import**, not reference, and it
is larger than the comment supposed rather than smaller.

**D4 — a build-time fix-up script is not an alternative to shipping the files.**
Writing importer settings into an immutable package lands in
`Library/PackageCache`, and Unity restores it: the same binary carries "The
following asset(s) located in immutable packages were unexpectedly altered" and
"Restored immutable package asset {0} during package resolution." Story #1230's
`DsAbiSetup.Configure` worked because that throwaway project put the library
under `Assets/`, which is mutable. That route is not available to a package.

**D5 — generating them needs a Unity editor once, and that does not put one in
CI.** The `.meta` for a `.cs` and an `.asmdef` is a short stable block, but its
GUID must be a fresh 32-hex value and the `PluginImporter` block a native
library needs is long and per-platform. It is generated once by an editor and
committed; nothing re-runs it per build, so
`unity-package-sited-in-this-repository.md`'s property that the check needs no
editor is untouched.

**D6 — the package version tracks the Cargo workspace**, and the whole version
story — every line, the handshake, and what a customer sees on a mismatch — is
[`the-package-and-its-library-are-one-versioned-artifact.md`](the-package-and-its-library-are-one-versioned-artifact.md).
It is stated there once rather than summarised here as well.

## Consequences

- **R-E2 is unmet on `main`** and closing it is story #1121's, which is the
  first story that installs the package into a project and the first that adds a
  native library whose settings exist only in a `.meta`.
- **GUID stability becomes an obligation at story #1122**, when materials and
  shaders arrive — those genuinely are GUID-referenced. A regenerated GUID
  breaks every reference, so the committed values are permanent once published.
- **No tag exists.** `git tag` returns zero and there are no releases, so
  `#<tag>` names nothing today. `.git-std.toml` sets `tag_prefix = "v"`, so the
  first is `v0.2.0`.
- **A `?path=` install fetches the whole repository to deliver one subfolder.**
  The ratio is roughly three orders of magnitude, and it grows with the
  repository rather than with the package. **No byte count is given here on
  purpose**: the first draft of this bullet quoted figures measured mid-branch
  and they were stale by the next commit. Re-derive with
  `git archive HEAD | gzip -9 | wc -c` against
  `git archive HEAD unity/com.driftsys.dashscene | gzip -9 | wc -c`. UPM clones
  shallowly, so what crosses the wire is less than full history and is still the
  whole tree rather than the subfolder. That is a cost of this form, not a
  defect, and it is what D1's "for as long as" clause is watching.

## Alternatives considered

**A scoped registry.** It gives real semver resolution, an `Update` affordance
and conflict handling, and it lets CI pack the package without putting binaries
in git history. Rejected **for now** rather than on the merits: it needs a
registry, a publish job and an account, none of which exist, and Unity's own
text frames scoped registries as intra-organisation distribution. It is the
successor form if the payload or the customer count makes the Git URL hurt, and
D1's scope clause is written so that switching is a revision of this record
rather than a contradiction of it.

**`.unitypackage`.** It unpacks into `Assets/`, which is mutable, so D3's
problem does not arise at all — its one real advantage. Rejected: no manifest,
no version, no dependency declaration, no upgrade and no uninstall, and
producing one needs a Unity editor, which reintroduces the CI dependency that
`unity-package-sited-in-this-repository.md` was written to avoid.

**The Asset Store.** Rejected on a licence term before reaching the merits: its
submission guidelines exclude offerings carrying "any Creative Commons/Apache
2.0 license that requires attribution", and this package is Apache-2.0 and ships
LICENSE and NOTICE because §4 requires it. Whether that clause is meant to cover
a publisher's own licence or only third-party components is unresolved, which
makes the Asset Store a legal-review item rather than a route to pick here.

**Git LFS for the binaries.** Rejected on Unity's own warning: because UPM uses
shallow clones, "the Package Manager can't retrieve the files stored on the LFS
server and instead checks out the LFS pointer files without any error or warning
messages". A silent pointer file where a library should be is the worst
available failure.
