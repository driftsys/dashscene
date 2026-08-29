# A platform is product or development, and its scripting runtime follows

    status   **accepted 2026-08-25**, by the owner, after story #1334 shipped
             the first two native libraries and the question of which
             platforms follow them became concrete. **One question inside it is
             deliberately not taken** and is marked as such: which Android
             ABI a farm emulator runs (P6, issue #1354).
    date     2026-08-25
    scope    which platforms the Unity package ships a native library for,
             which scripting runtime each of them uses, and which requirements
             bind which host project
    related  docs/decisions/the-native-library-ships-inside-the-unity-package.md
             (D3's matrix, and D4's committed binaries)
             docs/specification/07-embedding-and-distribution.md
             (R-E7, R-E8, R-E9 — the requirements this scopes)
             docs/decisions/host-integration-in-three-layers.md
             (a Unity host occupies layer 0 in its host-draws form)

## Context

D3 of
[`the-native-library-ships-inside-the-unity-package.md`](the-native-library-ships-inside-the-unity-package.md)
is a table of five rows, and story #1334 shipped two of them — macOS arm64 and
Android arm64 — on the rule that a row ships when it has a consumer. That rule
was written for two rows and never stated as a policy.

It now has to carry more weight. The product target is embedded Unity on arm64;
development happens on a Mac and on Linux x86_64; and an emulator farm is
expected. Those are three different kinds of machine with three different
reasons to exist, and D3's table does not distinguish them — it lists platforms,
so filling it in reads like completing a set rather than serving a consumer.

The scripting runtime has the same problem one level down. R-E7 requires IL2CPP
for Android and says nothing about desktop, which is correct; but **nothing
enforces it** (issue #1353). Since story #1367 `unity/android-probe` sets the
backend for the probe project it builds and reports it; every other player built
here runs whatever Unity's default is for its target.

## Decision

**The decisions here are numbered `P` rather than `D` on purpose.** The
native-library record's `D1`-`D5` are cited by bare number in `plugin_meta.rs`,
`RenderGateBuild.cs`, `DashsceneEditorCompat.cs` and the `justfile` — "D3's
macOS row" appears in the tests alone about a hundred times — and that record
also has a `D3` and a `D4` about these same platforms. A second `D3` here would
have made every one of those citations ambiguous.

**P1 — a platform row is either product or development, and the two are governed
differently.**

**Every row in that table is one or the other, so all five are named here.**
Product: Android arm64, and **iOS when it arrives** — AGENTS.md plans an iOS
host, and Unity offers no Mono there either, so P5's rule reaches it unchanged.
Development: macOS arm64, and Linux x86_64 when it ships. **Windows x64 is
neither, because no machine in the loop runs it** — under P2 it is a row the
table describes and nothing has a consumer for, which is exactly the case P2
exists to name rather than fill in.

A **product** row is a platform the shipped artifact runs on. A **development**
row is a platform that exists so a person or a machine can do work. Both get a
native library by the same mechanism; what differs is what may be required of
them. A requirement written for the product must not be applied to a development
platform by default, and a convenience taken for development must not be assumed
by the product.

**P2 — a row exists when a machine that runs it exists, not when the table has a
gap.**

D3's table describes what a row would look like; it is not a list of work. A row
ships when there is a machine in the loop that runs it. This is the rule story
#1334 already followed for two rows, stated so that a later reader does not fill
in Windows because the table has a Windows line.

**P3 — the product platform is embedded Android arm64, and it is IL2CPP.**

Not a preference. R-E7's own rationale is that Unity ships no arm64 Mono runtime
— `PlaybackEngines/AndroidPlayer/Variations/mono/Release/Libs/` contains
`armeabi-v7a` only — so an arm64 Android player cannot be Mono whatever anyone
decides. Anything that asserts something about the shipping runtime therefore
runs IL2CPP.

**P4 — development platforms are macOS arm64 and Linux x86_64, and macOS ships
arm64 only.**

macOS arm64 is a developer's editor. **This settles issue #1348**: the macOS row
ships arm64 and no universal binary. The editor installed for this project is
itself `Mach-O 64-bit executable arm64` with no x86_64 slice, so the row's
actual consumer is served exactly; a universal binary would buy two things and
neither is a target: shipping a macOS application to Intel machines, and making
Unity's **default** player architecture work without the pin below. **An Intel
Mac would also fail in the editor, not only in a player** — under P2 such a
machine would add a row rather than reopen this.

**The consequence has to be written where a consumer reads it, because Unity's
default produces it**: a macOS player built for the universal `x64ARM64`
architecture gets no library at all — Unity copies nothing rather than failing
the build, and the player raises `DllNotFoundException` on its first call.
`unity/render-gate` pins itself to `OSArchitecture.ARM64` for that reason, and
that pin is a supported configuration rather than a workaround.

Linux x86_64 is an editor row that D3 already describes and that nothing ships
yet; issue #1355 builds it.

**P5 — a desktop demonstration runs Mono; anything asserting something about the
shipping runtime runs IL2CPP.**

The showcase and the demo exist to be looked at and iterated on, and an IL2CPP
transpile buys them nothing. They **shall** run Mono, set explicitly rather than
left to a default, so the choice cannot drift. **One build script does this
now** — `unity/android-probe/AndroidProbeBuild.cs`, added 2026-08-28, sets
IL2CPP explicitly and reports it, because R-E7 requires it for Android and
because Unity ships no arm64 Mono runtime. That is a **product**-side target, so
it does not discharge the rule above, which binds the development-side recipes:
`unity-editor`, `unity-render`, `unity-conformance` and `unity-demo` still take
Unity's default and still report nothing. Issue #1360 is what makes the rule
true for them. Until it lands every player built here takes Unity's default for
its target, and no gate reports which runtime produced its result.

`unity/render-gate` is a demonstration for this purpose too and takes the same
rule: its question — did ink land where the committed tables place a node — is
backend-independent, and it already proxies on macOS and Metal, which nothing
ships either. **Which runtime it has been running is not recorded**, and
[`../design/unity-csharp-host.md`](../design/unity-csharp-host.md) says so;
#1360 is where that stops being assumed.

**What this leaves uncovered is the P/Invoke boundary**, and that is where the
IL2CPP gate belongs: a minimal player that constructs a runtime, performs the
`ds_abi_version` handshake, loads, ticks, acquires and releases a frame, and
drives the missing-symbol path. It does not draw. Issue #1322 is that subject,
and story #1334 is what made it reachable — a player now loads a shipped
library.

**The exposure is narrow across the C ABI, which is why one small gate covers
that surface.** The package declares no reverse P/Invoke over boundary B or the
C ABI: no `[MonoPInvokeCallback]`, no `GetFunctionPointerForDelegate`, no
`UnmanagedFunctionPointer`, and `include/dashscene.h` declares no
function-pointer parameter.

**One delegate does reach native code, and it is not on that surface.**
`BrgPainter` hands `OnPerformCulling` to `new BatchRendererGroup(...)` and
Unity's renderer invokes it every frame; its body writes raw pointer fields,
which is why R-E13 forces `allowUnsafeCode`. That is Unity's own binding layer
rather than a `[DllImport]` boundary, so it sits outside what the gate above is
for — **and outside what that gate would exercise, because that gate does not
draw.** It is named so "the exposure is narrow" is not read as "there is nothing
else": a second surface exists, it runs only when a player draws, and #1322 does
not cover it.

What remains on the C ABI surface is the missing-symbol translation, managed
stripping — the package ships no `link.xml` — and blittable-struct marshalling,
which boundary B already constrains.

**P6 — R-E8 binds the shipping host project, and a farm project is a different
project.** _(the ABI itself is open — see below)_

R-E8 requires `AndroidTargetArchitectures` to be exactly
`AndroidArchitecture.ARM64`, compared for equality rather than membership. That
is correct for the artifact that ships: it is what stops an APK silently gaining
a second ABI that doubles its size and never runs. A project built to drive an
emulator is not that artifact. If such a project needs another ABI it is a
separate host project with its own settings — **not a relaxation of R-E8**,
which keeps its meaning for the thing that ships.

**Open: which ABI the farm emulator runs — issue #1354.** Two answers, and they
differ in what they cost and in what they prove:

- **arm64 images on arm64 hosts** (Graviton, Ampere, Apple silicon) — no new
  library row, no new directory layout, and the farm exercises the ABI the
  product actually runs. Preferred where the hosts can be arm64.
- **x86_64 images on x86 hosts** — arm64 images on an x86 host run under full
  CPU emulation with no KVM acceleration, which is why this is the usual choice.
  It needs an Android `X86_64` row that **the native-library record's D3 table
  does not have**, a second host project under this decision, and it validates
  an ABI no target executes.

**P7 — a second Android ABI requires per-ABI directories, and the move happens
with it.**

That record's D3 carries one Android row today, and a second would be named
`libdashscene_ffi.so` exactly as the first is, so the two would collide in one
directory. `Runtime/Plugins/Android/` becomes
`Runtime/Plugins/Android/arm64-v8a/` and `Runtime/Plugins/Android/x86_64/`.
**The `.meta` must move with its library**: the guid is what an asset reference
resolves through, and R-E2 records that nothing can mint one later in an
immutable package.

**Each new folder needs its own `.meta` as well.** R-E2 is stated over "every
file **and every folder**" Unity imports, which is why
`Runtime/Plugins/Android.meta` and `Runtime/Plugins/macOS.meta` are already
committed. A move that ships the libraries and not the folders makes the package
deliver nothing, by R-E2's own argument.

**And four places pin the current path**, none of which a move updates by
itself: `Row::dir` and the row lookup in
`unity/package-gate/tests/plugin_meta.rs`, the `plugins=` assignment in
`just unity-plugins`, the package prefix in `RenderGateBuild.cs`, and
`ShippedPlugins()` in `DashsceneEditorCompat.cs`. The last two are also the
lists a new ABI must be added to, and nothing cross-checks them against each
other.

## Consequences

**No single machine can build the matrix this record plans.** One Mac with the
NDK builds both rows that ship today, in one `just unity-plugins` invocation —
so this is a statement about where the matrix is going, not a change this record
makes. macOS arm64 needs Apple's linker; Linux x86_64 needs a Linux toolchain;
Android needs the NDK. `just unity-plugins` builds both rows in one invocation,
which is the assumption that ends here; its refusal to run off macOS is a
separate matter, and the recipe gives the reason — linking a Mach-O dylib needs
Apple's linker. Issue #1355 is the row that ends it. Rows will be produced on
different machines, which makes R-E3's "built from one commit" weaker than it
already was — issue #1351 carries that.

**This is the threshold D4 of the native-library record was waiting for.** That
decision committed binaries because there was no release, no tag and no runner
for some platforms; it is worth re-deriving rather than inheriting. CI already
runs on `ubuntu-latest`, so a machine that can **build** that row exists today —
which is not what P2 asks. P2 asks for a machine that **runs** it, and no Linux
Unity editor is in the loop: no CI runner here can host an editor at all, which
is why every `just unity-*` recipe that needs an editor is outside CI. The count
is deliberately not written here: it has grown three times, and
`.claude/skills/project-gates/SKILL.md` is where each one is enumerated. The row
ships when a Linux editor is in the loop, not when a Linux compiler is, and
GitHub offers macOS runners for a public repository. Each committed refresh
writes a full copy into permanent history — these binaries do not deltify — so
the cost grows with rows multiplied by refreshes, not with rows alone. Two rows
at 9,660,216 bytes is comfortable; four rows refreshed often is where a tagged
release starts to win, and that is R-E3 and R-E18.

**R-E7, R-E8 and R-E9 remain unenforced.** Not because nothing reads
`PlayerSettings` — `unity/android-probe/AndroidProbeBuild.cs` sets all three and
reads each back — but because a check that writes the value it then reads cannot
fail for the requirement, and because these three bind the shipping project.
That is issue #1353, and `07-embedding-and-distribution.md` carries the same
distinction against each of the three. This decision gives them a scope; it does
not give them a check.

**`just unity-android` does not change that, and says so itself.** It sets all
three for the probe project it builds, and asserts R-E8 on the built APK — one
ABI directory, `arm64-v8a`. But it configures the project it then reads, which
this repository already rules out as a check, and the project it configures is
regenerated under `target/` on every run rather than being the shipping artifact
this record scopes those three to. The APK assertion is the one half that is a
check of an artifact rather than of an assignment.

**One question is closed by this record rather than by work**: issue #1348 asked
whether the macOS row should ship a universal binary. P4 answers no, on the
measurement that the editor has no x86_64 slice.

## Alternatives considered

**One matrix, filled in as platforms are encountered.** Rejected: it is what
produces a Windows row nobody runs. The distinction that matters is not which
platforms exist but which ones have a machine in the loop, and that is what P1
and P2 name.

**IL2CPP everywhere, including demonstrations.** Rejected on iteration cost for
no gain: a demo asserts nothing, and the transpile is minutes per change. The
risk it would cover is real but narrow, and P5 covers it with one gate over the
boundary rather than by slowing every build.

**Mono everywhere, including the emulator.** Rejected because it is not
available where it matters: Unity ships no arm64 Mono runtime, so the product's
own ABI cannot run it. A Mono emulator would test a runtime the product cannot
use.

**Relaxing R-E8 to membership so one project serves both.** Rejected: the
requirement's value is the equality comparison, which is what catches an APK
gaining `ARMv7` or `X86_64` by accident. A second project costs a scaffold and
keeps the guarantee.

**A universal macOS binary.** Rejected under P4, and recorded on issue #1348
with the measurement that settles it — the editor has no x86_64 slice, so the
row's consumer does not need one.
