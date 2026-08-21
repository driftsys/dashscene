# Embedding and distribution

    status  requirements, written by the spike of story #1125 (2026-08-21).
            They bind stories #1121, #1122, #1123 and #1124, none of which
            was built when these were written.
    source  docs/archive/2026-08-21-v021-unity-packaging-and-deployment.md,
            the spike's working memory, archived by the pull request that
            landed this file

`R-E1`-`R-E21` are what an engine host and its package must satisfy for a
dashscene document to draw inside a customer's project. They are stated so that
a reader can pass or fail a proposed package **without asking the author**,
which is what issue #1125 was opened to produce.

**Each is independently verifiable and names its own check.** Issue #170 exists
because several of `R1`-`R7` are not, and this file was written under an
instruction not to add to that population — so no requirement here uses a bare
adjective, and every quantitative claim carries its unit.

**Not all of them are met today.** A requirement is a statement about what must
hold, not a claim that it does; where the tree currently fails one, the entry
says so.

## The package

**R-E1** — `unity/com.driftsys.dashscene/package.json` shall declare
`"unity": "6000.3"`. _Check:_ read the field. It is absent today, deliberately,
because issue #1125 owned the number.

**R-E2** — every file and every folder under `unity/com.driftsys.dashscene/`
that Unity imports shall have a committed `.meta` file beside it. What Unity
imports excludes the four shapes its importer hides: a name beginning with `.`,
a name ending with `~`, a folder named `cvs`, and a file with the `.tmp`
extension. A check written from a shorter list demands a `.meta` for a path
Unity ignores, and goes red for the wrong reason. _Check:_ for each such path,
assert a sibling `.meta` exists.
`the_unity_package_meta_files_are_all_or_nothing` in
`demo/tests/registry_consistency.rs` asserts the **conditional** form — if any
`.meta` is shipped, all are — which is the strongest form that can be green
while this requirement is unmet, and which catches the partial adoption a story
committing them by hand will produce. **Unmet today — the package contains zero
`.meta` files**, and the record in
[`../decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md`](../decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md)
carries why that makes the package deliver nothing.

**R-E3** — the C# declarations and the native library a package release carries
shall be built from one commit. _Check:_ the tag R-E18 requires resolves to a
commit containing both. **Unmet today**: the package carries no native library.

**R-E18** — a package release shall be named by one git tag, matching
`.git-std.toml`'s `tag_prefix` followed by the version in
`unity/com.driftsys.dashscene/package.json`. _Check:_ compose the expected name
from those two files and assert **that** tag resolves — not merely that some tag
does, which any name would satisfy. **Unmet today**: `git tag` returns nothing
and the repository has no releases, so no release is nameable.

**R-E21** — each native library the package ships shall carry a `.meta`
declaring the platform and CPU that
[`../decisions/the-native-library-ships-inside-the-unity-package.md`](../decisions/the-native-library-ships-inside-the-unity-package.md)
D3 assigns to its target, using that table's exact casing. _Check:_ read each
`PluginImporter` block against D3's table and compare, byte for byte, every key
that row states — `OS` is stated for the desktop rows and not for the Android or
iOS ones, so the comparison is over the keys the row carries rather than over a
fixed pair. **Casing is the substance of this requirement, not pedantry**: Unity
parses the value through an enum converter and, on failure, substitutes the
default with a warning rather than an error, and for Android its own
documentation states it does not validate the setting — so a wrong value yields
a library silently absent from every player build. The path a library sits at
does **not** carry this: D2 records that a package's folder name reaches no
Unity path-inference rule, and that the fallback is Editor-only. **Unmet
today**: the package ships no native library and no `.meta`.

## The host project

**R-E4** — `UnityEngine.Rendering.GraphicsSettings.currentRenderPipeline` shall
be non-null in the host project before the painter constructs a
`BatchRendererGroup`. _Check:_ assert **both** limbs — that a group was
constructed, and that
`BatchRendererGroup requires the use of a ScriptableRenderPipeline` is absent
from the run. The second limb alone is fail-open, because that string is equally
absent from a run constructing no group at all. Story #1125's probe met the same
hazard from the other side: it read a plausible thread id off a group Unity had
refused. Measured — the probe produced that message under the Built-in Render
Pipeline and not under URP.

**R-E5** — the host project's active render pipeline asset shall set
`m_UseSRPBatcher` to `1`. _Check:_ read the field in the asset. Unity's refusal
message is `Please turn SRP Batcher ON to use the BatchRendererGroup API`, read
out of the editor binary's string table and **not observed in any run** — story
#1125's probe satisfied this requirement, so it never produced it.

**R-E6** — `ProjectSettings/GraphicsSettings.asset` shall set `m_BrgStripping`
to `2` (`BatchRendererGroupStrippingMode.KeepAll`). _Check:_ read the field. The
default is `0` (`KeepIfEntitiesGraphics`), which strips BRG shader variants in a
project that has no DOTS packages — which is this design's shape, since
`unity-painter-uses-brg.md` D1 uses BRG without Entities.

**R-E7** — the host project shall set the Android scripting backend to
`ScriptingImplementation.IL2CPP`. _Check:_ read `PlayerSettings`. Unity ships no
arm64 Mono runtime:
`PlaybackEngines/AndroidPlayer/Variations/mono/Release/Libs/` contains
`armeabi-v7a` only.

**R-E8** — the host project shall set `AndroidTargetArchitectures` to exactly
`AndroidArchitecture.ARM64`. _Check:_ read `PlayerSettings` and compare for
equality rather than membership — a value that also carries `ARMv7` fails.

**R-E9** — the host project shall set `AndroidMinSdkVersion` to the value of
`ANDROID_API` in the `justfile`, or higher. _Check:_ read `PlayerSettings` and
compare against that variable, which is `33` today. That variable is the floor
that binds the shipped artifact, because `_android-env` builds every Android
target through `aarch64-linux-android<ANDROID_API>-clang` — including the
`libdashscene_ffi.so` a Unity host loads, which is the only Android binary this
package ships. Issue #1235 argues the number can drop to 29; writing the
requirement against the variable rather than the literal means closing #1235
lowers the floor without amending this file.

**R-E10** — every C# type under `unity/com.driftsys.dashscene/Runtime/` shall
compile against `netstandard.dll` version 2.1.0, so the package builds under
`ApiCompatibilityLevel.NET_Standard`. _Check:_ `unity/package-compat`, which
`just unity-abi` runs. **Not `unity/abi-check`**, which targets `net10.0` — a
strict superset, so it accepts declarations Unity would refuse. Measured: a
`System.Half` in `BoundaryB.cs` builds clean under `net10.0` and fails under
`netstandard2.1` with CS0234. This requirement is met today.

## The painter's use of BatchRendererGroup

**R-E11** — every shader the painter passes to
`BatchRendererGroup.RegisterMaterial` shall declare `#pragma target 4.5` or a
higher target level. _Check:_ read each such shader's source, **and assert the
set of them is not empty** — no shader exists under `unity/` today, so a check
that only greps passes having read nothing. Target 4.5 is satisfied by GLES 3.1
and above, so this does not conflict with the GLES 3.2 fleet
`03-target-hardware-rules.md` names.

**R-E12** — every shader the painter passes to
`BatchRendererGroup.RegisterMaterial` shall declare
`#pragma multi_compile _ DOTS_INSTANCING_ON`. _Check:_ as R-E11, including its
non-empty assertion. Unity refuses a pass without the variant, naming it.

**R-E13** —
`unity/com.driftsys.dashscene/Runtime/Driftsys.Dashscene.Runtime.asmdef` shall
set `allowUnsafeCode` to `true`. _Check:_ read the field. It is `false` today.
`BatchCullingOutputDrawCommands` exposes raw pointer fields the culling callback
writes, so this is forced by the API rather than chosen.

**R-E14** — the painter shall read `BatchRendererGroup.BufferTarget` in a
process that has obtained a graphics device. _Check:_ assert the process holds a
device at the point of the read. `unity-painter-uses-brg.md` D4 rules that a
read taken without one is not a verdict, and story #1125 measured that hazard
producing a plausible answer rather than an obvious absence.

**R-E19** — the painter shall take the rung `unity-painter-uses-brg.md` D4's
table assigns to the value R-E14's read returns. _Check:_ that table, which is
the single home for the mapping; this requirement deliberately does not restate
it, because a second copy would drift. Note that
`UnsupportedByUnderlyingGraphicsApi` selects **rung 3** there — instanced draws
without BRG — rather than drawing nothing, and that descending below rung 1 is
D3's trigger to raise the R-T4 conflict rather than to proceed quietly. D4's
table assigns no rung to `Unknown`, which Unity documents as a value it never
returns; a painter that observes one shall report a named diagnostic and
construct no `BatchRendererGroup`, because an undocumented value is not a rung
selection.

**R-E15** — when `BatchRendererGroup.BufferTarget` is `ConstantBuffer`, the
painter shall bound each batch's window to the byte count
`BatchRendererGroup.GetConstantBufferMaxWindowSize()` returns, and shall align
each window offset to the byte count
`BatchRendererGroup.GetConstantBufferOffsetAlignment()` returns. _Check:_ assert
both against the values the **running device** reports, never against a literal.
On the one adapter measured — Apple M3, Metal — they were 16384 and 256, and
`BufferTarget` was `RawBuffer`, so that adapter does not exercise this
requirement at all.

**R-E20** — when `BatchRendererGroup.BufferTarget` is `ConstantBuffer`, the
painter shall emit at most 256 visible instances per `BatchDrawCommand`.
_Check:_ assert the count. **This bound is a literal and R-E15's are not**: 256
is fixed by the SRP core shader library, where `UnityDOTSInstancing.hlsl`
declares `DOTSVisibleData
unity_DOTSVisibleInstances[256]` against
`kBRGVisibilityUBOShaderArraySize`, so it is a property of the shader rather
than of the adapter.

## The ABI a host sits on

**R-E16** — the host shall call `ds_abi_version` once before any other
`ds_runtime_*` call and shall refuse to proceed when the value differs from the
`DS_ABI_VERSION` its C# was built against, reporting both numbers. _Check:_
build a host against a mismatched value and assert it refuses. Nothing in the
package does this today; `BoundaryB.cs` contains no `DllImport` at all.

**R-E17** — the host shall compare each `DsSlice::stride` against the `sizeof`
of its own row declaration before reading any row of that array, and shall
refuse to read the array when they differ. **Unmet today**: nothing in the
package reads a frame. _Check:_ mutate a row type's size and assert the host
refuses rather than drawing. `unity/abi-check` does not cover this: it compares
C# source against Rust source at one commit, and `stride` is what observes two
shipped artifacts having come from different ones.
