# The native library ships inside the Unity package, as a cdylib pinned by its `.meta`

    status   accepted (2026-08-21, story #1125's spike). Settles questions 2
             and 3 of issue #1125 — the native plugin's layout, and how the
             library is built and shipped.
    date     2026-08-21
    scope    crates/dashscene-ffi's crate types, and where its build output
             sits inside unity/com.driftsys.dashscene
    related  docs/decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md
             (why the .meta is the mechanism)
             docs/specification/07-embedding-and-distribution.md (R-E3)
             docs/technotes/unity-toolchain.md (what story #1230 measured)

## Context

Issue #1125 asked where the Rust library sits per platform and under what import
settings, and whether the binary is committed, built in CI, or fetched from a
release. It cited `crates/dashscene-ffi/Cargo.toml`'s crate-type comment as
stale. It is, and on a second count the issue did not name.

## Decision

**D1 — a Unity host takes the `cdylib`, on every platform in scope, and the
Cargo comment saying otherwise is corrected in place.**

The comment read, until the commit that landed this record:

    # `rlib` so the workspace's own tests can link it as a Rust crate; `cdylib` and
    # `staticlib` because a platform host loads it as a library and not as a crate.
    # JNI wants the first, and an iOS or Unity host in v1 wants the second.

"The first" and "the second" refer to the **pair** the preceding sentence
introduces, so the sentence assigns Unity the `staticlib`. That is wrong. Story
#1230 measured one `[DllImport("dashscene_ffi")]` resolving
`libdashscene_ffi.dylib` in the editor and `libdashscene_ffi.so` in the Android
player — both `cdylib`. "In v1" is stale for Unity, which moved to v0.21 on
2026-08-12, and correct for iOS, which has not moved.

**The sentence's real defect is that it treats iOS and Unity as one case when
they take different crate types**, which a date correction alone would have left
in place. `staticlib` remains right for iOS in v1, where a plug-in is linked
into the executable and reached as `DllImport("__Internal")` rather than by
library name.

**D2 — the library sits under `Runtime/Plugins/<platform>/`, and that path is
documentation for a human, not a mechanism.**

Unity's package layout page documents `Runtime/` and says in terms that it "is
only a convention and doesn't affect the asset import pipeline". Its path
inference table is rooted at `Assets/` in every row, so it reaches no package —
and it has **no Android row at all**, and cannot express macOS arm64, whose only
CPU options in that table are `x86`, `x86_64` and `x64`.

**The fallback matters more than the table.** A native library at a path
matching no pattern gets _Editor platform_ defaults. Inside a package that is
every library, so a plugin shipped without a correct `.meta` is Editor-only and
silently absent from every player build.

Unity's own bundled packages confirm the path does nothing:
`com.unity.rendering.denoising` uses `Runtime/Plugin/MacOS-arm64/` and
`com.unity.adaptiveperformance.google.android` uses
`Runtime/Plugins/Android/<name>/arm64-v8a/`. Neither matches a documented
pattern; both commit an explicit `.meta` per binary.

**D3 — the per-platform matrix.**

| target                           | crate type  | file                     | `.meta` must set                                    |
| -------------------------------- | ----------- | ------------------------ | --------------------------------------------------- |
| macOS editor + standalone, arm64 | `cdylib`    | `libdashscene_ffi.dylib` | Editor `OS=OSX` `CPU=ARM64`; Standalone `CPU=ARM64` |
| Windows editor + standalone, x64 | `cdylib`    | `dashscene_ffi.dll`      | Editor `OS=Windows` `CPU=x86_64`                    |
| Linux editor + standalone, x64   | `cdylib`    | `libdashscene_ffi.so`    | Editor `OS=Linux` `CPU=x86_64`                      |
| Android player, arm64            | `cdylib`    | `libdashscene_ffi.so`    | `Android` `CPU=ARM64`                               |
| iOS, v1                          | `staticlib` | `libdashscene_ffi.a`     | `iOS`, and the C# becomes `DllImport("__Internal")` |

**Casing is load-bearing and differs between platforms** — `x86_64` for Editor
and Standalone, `ARM64`/`X86_64` for Android. Unity parses the value through an
enum converter and, on failure, substitutes the default with a warning rather
than an error. And for Android specifically Unity's own documentation states it
"does not validate your settings", so a wrong CPU value produces a silently
mis-packaged library.

**D4 — the library is committed into the package, and this is chosen against a
better option that is currently unbuildable.**

The alternative that removes drift is to have CI build the binaries and attach
them to a release. It is blocked on four things that do not exist: any git tag,
any GitHub release, any release workflow, and — decisively — **any macOS or
Windows CI runner**. Every job in `.github/workflows/ci.yml` runs
`ubuntu-latest` except one `atlas-repro` leg on `ubuntu-24.04-arm`, so CI cannot
build the editor `.dylib`, which is the artifact a developer's editor actually
loads. UPM also has no post-install hook, and a file placed into
`Library/PackageCache` after install has no `.meta` and is ignored, so "fetch it
later" is not available either.

**The size objection is smaller than it looks and was re-measured.** The
often-quoted ~20 MB is Unity's strip of a **debug** build. The release Android
library is **6,513,488 bytes** and the release macOS one **3,085,408**, so the
two triples v0.21 needs are about 9.6 MB.

**What committing costs is permanent history in a public repository**, since
binaries do not delta-compress and the distribution record rejects LFS under its
"Alternatives considered". That is the reason this decision is scoped to the
Git-URL form and moves with it.

**D5 — nothing verifies a shipped binary against the declarations, under any of
these options, and this record does not pretend otherwise.**

`just unity-abi` builds `dashpaint-abi` from source and compares it against the
package's `BoundaryB.cs`. It never reads `dashscene-ffi` and never reads
anything under `Runtime/Plugins/`. So it proves C# source and Rust source agree
at one commit, and detects a stale committed binary in none of the three
shipping options. `DsSlice::stride` is what observes it at run time, which is
why `07-embedding-and-distribution.md` R-E17 makes that check mandatory rather
than advisory.

## Consequences

- **No recipe builds a Windows or Linux library**, and no CI job builds a macOS
  one. `just host-lib` builds the host's release cdylib and `just android` takes
  a profile, so the two triples v0.21 needs are producible on a developer's
  machine and nowhere else.
- **The `staticlib` has no consumer and is built anyway** on every
  `cargo build --workspace`, at 317 MB in debug. Not changed here — it is iOS's
  in v1 — but recorded so the cost is visible.
- **`libc++_shared.so` does not arise for this library.** It declares four
  `NEEDED` entries — `libandroid.so`, `libdl.so`, `libm.so`, `libc.so` — and not
  the C++ runtime. The technote's clause explaining what it _would_ collide with
  is corrected there: Unity 6.3's `libunity.so` does not declare
  `libc++_shared.so` either, and the AndroidPlayer ships no copy.

## Alternatives considered

**CI-built binaries attached to a release.** The right answer, and the one to
revisit: it makes the binary a function of a commit CI verified, rather than
something a human can forget to rebuild. Blocked on the four missing pieces in
D4, of which the CI runners are the substantive one.

**The consumer builds from source.** Zero repository growth and no staleness.
Rejected on who the consumer is: it requires a pinned Rust toolchain, a C
toolchain, `just`, `jq` and — for a player — the Android NDK, which `bootstrap`
does not install. A Unity customer typically has none of them.
