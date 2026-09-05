# Technote — the Unity build environment, and the C ABI seam proven

Informative. **Measured 2026-08-17**, story #1230 under epic #1106, and amended
twice since: **2026-08-18**, when the target moved to Unity 6.5, and
**2026-08-20**, when it moved to Unity 6.3 LTS and the Unity CLI paragraph was
added. Neither amendment is story #1230's evidence; both say their own date.
Nothing depends on this note. It exists because epic #1106's entry conditions
are all the repository owner's — three on the day this was measured, two after
the ruling recorded below, and **none from 2026-08-18**, when the last two were
ruled — and they are cheaper to decide with evidence that a Unity project can
reach `dashscene-ffi` at all than without it.

**Written for someone deciding, not for someone repeating.** The commands are
here so a claim can be checked, but the audience is
`docs/decisions/unity-package-sited-in-this-repository.md` and issue #1125, the
packaging and deployment spike.

**The first of those two was decided later the same day, partly on this note.**
The C# package is sited in this repository under `unity/` rather than in a
separate repository, so the question described above as "still open on whether
that repository is created" is closed, and one of epic #1106's entry conditions
went with it. What this note contributed is below and is unchanged. Issue #1125
took this note as input and closed on 2026-08-21; its three records are named in
`../decisions/README.md`.

**What was proven is narrow, and the narrowness is the point.** A Unity project
called `ds_abi_version` and got back the value the committed header declares, in
the editor process on this machine and in an Android player on an emulator. That
entry point takes no arguments, returns a `const`, and is the one call in
`crates/dashscene-ffi/src/lib.rs` with no `catch_unwind` — so a failure would
have been a loader or marshalling failure and nothing else. Nothing else was
exercised: no runtime was created, no document was loaded, no surface was
attached, and no pixel was drawn. "What is still unknown" below is as much of
this note as "what worked", and issue #851's packaging findings are untouched by
any of it.

## The machine

|        |                                                           |
| ------ | --------------------------------------------------------- |
| host   | Apple Silicon, arm64, macOS 15.6.1 (Darwin 24.6.0, 24G90) |
| rustc  | 1.97.1 (8bab26f4f 2026-07-14)                             |
| device | **an emulator, not target hardware** — see below          |

## What is installed

**Unity Hub 3.20.0** was already on the machine, installed 2026-07-31, and this
story did not touch it. Its command-line entry point is inside the application
bundle and takes `--` before its own flags:

    "/Applications/Unity Hub.app/Contents/MacOS/Unity Hub" -- --headless editors --installed

**Unity Editor 6000.3.22f1** — Unity 6.3 LTS, changeset `1c726e1fb402`, released
2026-08-13, arm64. Installed by this story, headlessly and unattended:

    "/Applications/Unity Hub.app/Contents/MacOS/Unity Hub" -- --headless install \
      --version 6000.3.22f1 --changeset 1c726e1fb402 --architecture arm64 \
      --module android --childModules

`--childModules` is what makes that one line rather than seven: it pulls in the
JDK, the NDK and the SDK pieces below rather than requiring each to be named.

**The Hub does not say which releases are LTS**, which matters because the story
asked for an LTS release and picking one from the Hub's list is guesswork.
`editors --releases` prints five versions and no stream, and the installed
editor's own `metadata.hub.json` carries `"isLTS": null`. Unity's release
service does say, and is what this choice was made from:

    curl "https://services.api.unity.com/unity/editor/release/v1/releases?stream=LTS&platform=MAC_OS&architecture=ARM64"

Two LTS lines were live on that date — `6000.0.x` (Unity 6) and `6000.3.x`
(Unity 6.3). The newer was taken.

**Modules installed with it**, all of them Unity's own copies under the editor:

| module                 | version              |
| ---------------------- | -------------------- |
| Android Build Support  | —                    |
| OpenJDK                | Temurin 17.0.18+8    |
| Android NDK            | 27.2.12479018 (r27c) |
| CMake                  | 3.22.1               |
| SDK Build Tools        | 36.0.0               |
| SDK Platform Tools     | 36.0.0               |
| SDK Platforms          | 34, 35, 36, 37.0     |
| SDK Command Line Tools | 16.0                 |

**A second editor was already present and was left alone**: 6000.5.6f1 — Unity
6.5, which is a supported release rather than an LTS one — carrying Web Build
Support and Documentation only. It had no Android module and was not used here.
Two editors sat under `/Applications/Unity/Hub/Editor/` on the day this was
measured; one does now, as the paragraph below records.

**That second editor was made the target on 2026-08-18, and both halves of that
were overtaken on 2026-08-20.** The target was reversed to Unity 6.3 LTS — the
editor this story installed — by the owner's ruling recorded in
`../decisions/unity-painter-uses-brg.md` D2, which carries the reasons. And the
6.5 editor is no longer installed here: `/Applications/Unity/Hub/Editor/` holds
`6000.3.22f1` alone, by both the Hub's headless listing and the Unity CLI's.

Nothing measured below moves — it was all taken on `6000.3.22f1` and is labelled
as such. What the reversal changes for whoever builds next is that the Android
modules recorded above are the ones a Unity Android player uses, rather than a
set that a second editor would have had to install first.

**The editor moved to `6000.3.23f1` on 2026-08-28, and `6000.3.22f1` is no
longer installed.** Same LTS line, same modules — Android Build Support with the
bundled NDK, SDK and OpenJDK — installed through the Hub with
`--module android --childModules` as before. Its changeset is `09d2ecc7fb28`,
read from the editor's own `Unity.app/Contents/Info.plist` rather than from a
release listing.

**Two statements above are now false as statements about this machine**, and are
left as written because each is dated and each was true when taken: the Hub
holds `6000.3.23f1` alone rather than `6000.3.22f1`, and it is that version the
Unity CLI's `location` field marks as present. **R-E1 is unaffected** — it
requires `package.json` to declare `"unity": "6000.3"`, the minor version, which
both patches satisfy.

**Every `unity-*` recipe's default moved with it**, eleven sites in the
`justfile` at the time of that commit. One mention did not move and must not:
the comment recording why `-batchmode` is used without `-nographics`, which is a
dated measurement. **No ordinal is given for it here** — `unity-android` landed
in the same branch and added further sites, so "the twelfth" stopped identifying
it within one commit of being written. Derive the survivor instead:

    grep -n '6000\.3\.22f1' justfile Nothing else

measured in this note moves either, for the reason already stated: it is all
labelled with the version it was taken on.

**The licence is Unity Personal**, an entitlement issued 2026-08-17 to
`~/Library/Unity/licenses/UnityEntitlementLicense.xml`. It was already in place
when this story started, and no activation step was needed for any of the
batchmode runs below.

**Unity CLI 1.0.0-beta.5** was installed on 2026-08-20, after this story, with
`brew install --cask unity-cli`; the cask links a `unity` binary into
`/opt/homebrew/bin`. It is experimental, and it is in no `just` recipe and no CI
job — a convenience on this machine rather than a prerequisite of any gate. Two
of its properties bear on this note.

It does **not** report the LTS stream. `unity editors --releases --json` returns
an entry per release carrying some of `version`, `alias`, `architecture`,
`default` and `location`, and **no** stream field on any of them — so the
release-API query above stays the way an LTS line is identified here.

**Its listing is not a list of what is installed**, which matters when reading
the paragraph above off it. The same command returns five releases, one of them
`6000.5.9f1`; what marks an editor as present is the `location` field, which
only `6000.3.22f1` carries. `6000.5.9f1` is also a different patch from the
`6000.5.6f1` that was removed, so the CLI's output cannot answer "is 6.5 still
installed" at all. The Hub's `editors --installed` answers it directly. And its
`unity mcp` server and its `unity command` family reach a running Editor only
through `com.unity.pipeline`, a package installed into a project — with no
project and no Editor the server handshakes and answers `tools/list` with an
empty array. Issue #1121 carries that as a note for whoever creates the host
project.

## The two Android toolchains, which do not agree

The story predicted this and it is worth stating exactly, because a record
naming only one of them would be wrong in a way nothing catches.

|                 | Unity's                                      | what `just android` finds                      |
| --------------- | -------------------------------------------- | ---------------------------------------------- |
| NDK             | 27.2.12479018 (r27c)                         | **28.0.12674087-beta2 (r28-beta2)**            |
| location        | `<editor>/PlaybackEngines/AndroidPlayer/NDK` | `~/Library/Android/sdk/ndk/28.0.12674087`      |
| JDK             | Temurin 17.0.18+8                            | Homebrew OpenJDK 21.0.9                        |
| SDK build-tools | 36.0.0                                       | 30.0.3, 32.1.0-rc1, 34.0.0, 35.0.0, 36.0.0-rc3 |

Both NDKs ship `darwin-x86_64` prebuilts only, so both run under Rosetta on this
arm64 host. The repository's is a **beta**, which is a separate observation from
the disagreement and is the more surprising half of it.

**Nothing here required them to agree, and the reason is structural rather than
lucky.** Unity never compiled the library: `cargo` built it with the
repository's NDK, and Unity's Android build only packaged the finished `.so`.
The two would have to be reconciled if a Unity host ever built native code of
its own.

**There is also no C++ runtime to reconcile.** The cross-compiled
`libdashscene_ffi.so` declares exactly four `NEEDED` entries — `libandroid.so`,
`libdl.so`, `libm.so`, `libc.so` — and not `libc++_shared.so`. So the usual
collision between a plugin's C++ runtime and the engine's does not arise for
this library as it stands.

**The clause explaining what it would collide with was wrong and is corrected
here (2026-08-21, story #1125).** It said `libc++_shared.so` is "which Unity
ships its own copy of in the same APK". Unity ships no copy: the only
`libc++_shared.so` under this editor is inside the NDK sysroot, which is
toolchain input rather than player payload, and `libunity.so` for `arm64-v8a`
declares eight `NEEDED` entries — `libandroid.so`, `liblog.so`, `libz.so`,
`libEGL.so`, `libmediandk.so`, `libm.so`, `libdl.so`, `libc.so` — with the C++
runtime not among them. Unity 6.3 links libc++ statically. The conclusion is
unchanged and the reason is simpler than the one given: this library needs no
C++ runtime at all, so there is nothing to collide.

## The gap that was closed: `just host-lib`

The story named the host dynamic library as the first concrete gap. Measured
against the tree, the gap is narrower than "nothing builds one", and the precise
version is what the new recipe's comment records:

- `crates/dashscene-ffi/Cargo.toml` has declared a `cdylib` crate type since
  story #840, so the library has always been buildable.
- A **debug** `libdashscene_ffi.dylib` already falls out of two existing
  recipes. `assemble` is `cargo build --workspace`, which builds every declared
  crate type — measured by deleting the file and running `just assemble` alone,
  after which it came back. `c-abi` then links its C caller against exactly that
  file.
- What no recipe produced is the **release** library — the one a host actually
  loads — and no recipe named the path, so a host author read it out of
  someone's shell history.
- `just android` is the other half of the same seam and cross-compiles for
  `aarch64-linux-android` only. That is not a substitute: the editor probe below
  runs inside the editor process on this machine and resolves a library for this
  triple.

`just host-lib` closes that. It builds the release library and prints its path —
absolute, because the caller is a project outside this repository.

    $ just host-lib
    Finished `release` profile [optimized] target(s) in 1m 03s
    Finished `release` profile [optimized] target(s) in 0.09s
    host-lib: …/target/release/libdashscene_ffi.dylib

The second `Finished` line is the query described below, which runs cargo again
in JSON mode against the tree the first line just built.

3,085,408 bytes, `Mach-O 64-bit dynamically linked shared library arm64`, twelve
exported `ds_*` symbols.

**Where the path comes from is the part worth recording**, because the obvious
spelling is wrong. A recipe that maps `uname` to an extension and then tests
that the file exists is **fail-open**: `cargo build` does not delete the
artifacts of a crate type that has been removed, so after dropping `cdylib` from
`[lib] crate-type` and rebuilding, the previous `libdashscene_ffi.dylib` is
still on disk. That was measured on a throwaway crate, and then on this one —
with `cdylib` removed and the stale file left in place, the existence test
passes and would print the path of a library the run did not produce. It is the
shape of issue #1057, where a stale release `.so` was packaged into an APK and
announced as the release library. So the recipe reads
`compiler-artifact.filenames` out of `cargo build --message-format=json`, which
lists only what that invocation emitted; the mutation above now fails the recipe
instead of passing it.

**It is not in `check` or `build`, and that leaves a real gap.** `c-abi` links
this crate in _debug_ on every one of those runs, so nothing in any gate links
it in release, where `[profile.release]` turns on `lto = true` and
`codegen-units = 1`. A release-only failure in the one library a host is told to
load would reach nobody until someone ran this recipe by hand. Putting a
full-LTO link into every local `check` is a scheduling decision with a recurring
cost rather than one this story should take, so it is recorded as issue #1233
instead of decided here.

**The same gap was open for the Android triple when this was written, and it is
not any more.** As measured, `just android` carried no `--release`, so no recipe
produced a release `libdashscene_ffi.so` and the player below loaded the
**debug** one — which is still true of that run and is why its numbers are debug
numbers.

**The recipe gained a profile parameter fifteen minutes after this note landed**
(story #1229), and this paragraph asserted the opposite for four days and five
edits before story #1125 caught it. `just android release` maps to `--release`
and applies it to `dashscene-ffi`, so the artifact `DASHSCENE_ANDROID_PROFILE`
selects is built. It is **6,513,488 bytes**, against the 194,407,656 of the
debug library below.

## The editor half

The throwaway project is not committed, and what follows is the whole of what it
contains that matters — one declaration, which serves both halves:

```csharp
[DllImport("dashscene_ffi")]
private static extern uint ds_abi_version();
```

That one line resolves `libdashscene_ffi.dylib` in the editor and
`libdashscene_ffi.so` in the Android player. Nothing platform-specific was
needed on the C# side.

**The expected value is passed in rather than written in the probe**, so it
cannot agree with itself: the shell reads `#define DS_ABI_VERSION` out of
`crates/dashscene-ffi/include/dashscene.h` and hands it over as `-dsExpect`.

Two batchmode launches, because a native plugin's importer settings are read
when the library is first resolved and the second launch starts from a settled
asset database:

    Unity -projectPath <p> -batchmode -quit -nographics -executeMethod DsAbiSetup.Configure
    Unity -projectPath <p> -batchmode -quit -nographics -executeMethod DsAbiProbeEditor.Probe -dsExpect 1

The second takes **3.9 s** and answers:

    DS_ABI_PROBE result=match ds_abi_version=1 expected=1     (exit 0)

**That `1` is this run's value and is no longer the current one.** Story #1226
moved `DS_ABI_VERSION` to **2** when ten entry points changed signature for the
generational handle. The mechanism this proved is unaffected — the probe read
the header's value and compared — but the number is a measurement of the day,
not a fact to quote forward.

**It was proven falsifiable rather than assumed to be.** Two mutations, and the
restoration after them:

| mutation                | result                                                |
| ----------------------- | ----------------------------------------------------- |
| `-dsExpect 2`           | `result=mismatch ds_abi_version=1 expected=2`, exit 1 |
| the `.dylib` moved away | `result=load-failed`, `DllNotFoundException`, exit 2  |
| restored                | `result=match`, exit 0                                |

**What this is not:** the story said "a Unity editor play-mode test", and what
ran is an `-executeMethod` call in a headless editor process. It is the same
process and the same loader, and it is not literally play mode.

`EditorApplication.Exit(code)` is what makes the result an exit status; without
it a batchmode `-executeMethod` run reports success whatever the method
concluded.

## The device half

An APK was built from the same project — one scene, one `MonoBehaviour` that
calls the same `DsAbi.Check` on `Start`.

| setting            | value                                                               |
| ------------------ | ------------------------------------------------------------------- |
| scripting backend  | IL2CPP — Unity offers Mono for ARMv7 only, and the library is arm64 |
| architectures      | ARM64 only                                                          |
| `minSdkVersion`    | 33, matching `ANDROID_API` in the justfile                          |
| `targetSdkVersion` | 36                                                                  |
| build              | Release, managed stripping enabled                                  |
| wall time          | **95 s**, batchmode, cold                                           |
| APK                | 23,892,195 bytes                                                    |

**Unity stripped the library, and the size difference is large enough to be
worth naming.** `just android` produces a debug `libdashscene_ffi.so` of
**194,407,656 bytes**; the copy inside the APK, at
`lib/arm64-v8a/libdashscene_ffi.so`, is **19,670,048 bytes**. That is a strip
rather than a guess about one: the cargo output carries seven `.debug_*`
sections and a `.symtab`, the packaged copy carries neither, and
`ds_abi_version` is still in its `.dynsym`.

The run, on the emulator:

    DS_ABI_PROBE player-start abi=ARM64 FP ASIMD AES
    DS_ABI_PROBE result=match ds_abi_version=1 expected=1

**Falsified the same way.** Rebuilt with the plugin removed from the project,
the APK contained zero copies of the library and the player reported:

    DS_ABI_PROBE result=load-failed detail=Unable to load DLL 'dashscene_ffi' …
    dlopen failed: library "dashscene_ffi" not found

So the `1` in the passing run came from the library and not from anywhere else.

**The device was an emulator**, and no part of this ran on target hardware. It
was `dashscene-splitscreen` — a `medium_tablet` AVD, Google APIs playstore
image, API 35, `arm64-v8a`, reporting `Google sdk_gphone64_arm64` — started with
`-gpu host`. Target hardware is epic #1107's entry condition, not this story's,
and nothing here is a measurement of anything on a real device.

## Three things that were not obvious

**The player never reached `Start`, and nothing said so.** The first two device
runs produced a live process, a full Unity startup log, and no probe output at
all. The cause is that Android's immersive-mode confirmation overlay ("swipe up
to exit full screen") takes window focus, Unity pauses the player loop when it
loses focus, and `Start` runs on the first frame — which never came. It reads
exactly like a packaging failure and is not one. The fix is device state, not
build settings:

    adb shell settings put secure immersive_mode_confirmations confirmed

That was the **only** cause. While diagnosing it the probe was switched from
`Debug.Log` to `Debug.LogError` on the theory that a release build suppressed
the lower level; it does not, and the run above was re-done with `Debug.Log`
restored to confirm that rather than leaving two candidate causes in this note.

**`AndroidSdkVersions.AndroidApi33` does not exist**; the member is
`AndroidApiLevel33`. A wrong enum member fails the whole editor launch with
"Scripts have compiler errors" and no method runs, so it costs a full cycle to
find.

**The plugin importers were pinned explicitly** — `SetCompatibleWithEditor` with
`OS`/`CPU` for the host library,
`SetCompatibleWithPlatform(BuildTarget.Android)` with `CPU=ARM64` for the
Android one — rather than relying on what Unity infers from the path. Whether
the inference alone would have been enough was not measured, so this is a thing
that was done and not a thing that was required.

## The culling callback's thread — measured 2026-08-21, story #1125

**This section is a different measurement from the rest of this note**, taken
four days later by the packaging spike, and it is here because this note is
where the Unity measurements live. It is the one thing issue #1267's comment of
2026-08-19 left open: whether Unity invokes
`BatchRendererGroup.OnPerformCulling` on the main thread or on a worker, which
decides whether a host can bracket its job dispatch with
`ds_runtime_acquire_frame` / `ds_runtime_release_frame`.

**Unity documents no thread for this callback.** The
`UnityEngine.CoreModule.xml` shipped beside the assembly describes
`OnPerformCulling` and says nothing about which thread runs it, so the answer
had to be read rather than looked up.

### The result

|                                    |                                                   |
| ---------------------------------- | ------------------------------------------------- |
| editor                             | `6000.3.22f1` (Unity 6.3 LTS), the target         |
| render pipeline                    | URP 17.0.1, from the `3d-cross-platform` template |
| graphics device                    | **Metal, Apple M3** — a real device, windowed     |
| `BatchRendererGroup.BufferTarget`  | `RawBuffer`                                       |
| `GetConstantBufferMaxWindowSize`   | 16384                                             |
| `GetConstantBufferOffsetAlignment` | 256                                               |
| invocations                        | 58 `Camera` + 58 `Light` = **116**                |
| distinct managed thread ids        | **1**, equal to the id `Start` recorded           |

`OnPerformCulling` ran on the **main thread**, on every invocation, for both
view types. `IsThreadPoolThread` and `IsBackground` were both false. So a host
can **acquire** from inside the callback, and
`../decisions/the-frame-crosses-under-a-lease.md` D2's design is reachable
rather than merely coherent. **The release is a separate question this reading
does not answer**: `ds_runtime_acquire_frame` requires it after Unity completes
the `JobHandle` rather than on return from the callback, because the workers are
still reading when the callback returns.

### Two readings that were discarded, and why they matter

**A run under the Built-in Render Pipeline reported the same answer and is not
evidence.** Its log carries
`BatchRendererGroup requires the use of a
ScriptableRenderPipeline` at the
constructor, so the callback was firing on a group Unity had refused — 58
invocations on a thing that does not draw. It was caught by grepping the run log
for what Unity had complained about, not by reading the result line, which
looked correct. This is the same hazard `../decisions/unity-painter-uses-brg.md`
D4 names for a `-nographics` `BufferTarget` read, in a form that produces a
plausible number rather than an obvious absence.

**Zero `Light` invocations, in an earlier run, was a default and not a fact.**
Without a `SetEnabledViewTypes` call a group receives `Camera` views only, so a
shadow-casting light produced nothing. The 58 above appear only once both view
types are asked for.

### What it does not settle

- **One platform.** macOS, Metal, URP, Mono, one adapter. **The target is
  Android on a tiling GPU** and no reading has been taken there. The two
  constant-buffer figures are this adapter's, by `unity-painter-uses-brg.md`
  D4's own rule that the value is a property of the active graphics API.
- **It is a measurement, not a contract.** Unity promises no thread, so a host
  asserts the identity rather than assuming it — one `ManagedThreadId`
  comparison.
- **It says nothing about materials or shader variants.** The probe emits no
  draw commands, and `Shader.Find("Universal Render Pipeline/Unlit")` returned
  null in the player, so it fell back to `Sprites/Default`. Nothing was ever
  shaded.
- **Issue #1267's question 2 was outside this measurement's scope, and has since
  been ruled** (2026-08-23,
  `docs/decisions/ds-wrong-thread-stands-for-a-dead-thread-too.md`): it stands
  for a dead thread as well as a foreign one. What was true here, and stays
  true, is the scope claim: that question was an owner's ruling and not this
  measurement's to make.

## What is still unknown

**Nothing here settles packaging, and a working P/Invoke is not evidence about
it.** Issue #851's finding that you cannot memory-map through an AssetBundle
stands untouched. Two observations were made in passing and neither is a
conclusion: Unity packaged the native library as a **Deflate-compressed** zip
entry, and the manifest it generated carries `extractNativeLibs="true"`. Those
are Unity's defaults for a _plugin_, and the question that matters is about the
`.dsb` and its bank, which is issue #1124 for how the bytes reach memory and
issue #1125 for how the product reaches a customer's project — which it settled
on 2026-08-21 without answering this one. #851's second packaging path needs
_stored_ entries; nothing here tested whether Unity can be made to produce one.

**No data crossed the boundary.** `ds_abi_version` takes no arguments and
returns a `const`. Nothing here says a `#[repr(C)]` struct marshals correctly.
That was issue #859's data plane, which did not exist when this was written and
landed on 2026-08-20, and story #1121 gave it its first C# consumer:
`FrameLease` reads `DsFrame`'s nineteen arrays through the package's own
P/Invoke layer.

**No runtime was created.** `ds_runtime_new` was not called, no document was
loaded, no surface was attached, and `ds_runtime_draw` never ran. The lifecycle,
the tick and the surface handoff are all unexercised from C#.

**No target hardware.** See above; an arm64 emulator on an Apple Silicon host is
not the fleet.

**Whether the NDK disagreement ever matters.** It did not here because Unity
compiled nothing. It is unanswered for any future in which a Unity host carries
native code of its own.

**Whether Unity Personal is the right entitlement.** That is what is installed,
and no part of this story assessed whether the product's use of Unity requires a
different one.

**Which of the three layers a Unity host occupies** was untouched here — open
question 4 on issue #851, and entry condition 1 of epic #1106. This story
produced evidence and deliberately answered none of the three conditions. **It
was settled on 2026-08-18**: layer 0, in the host-draws form that ruling added
to `../decisions/host-integration-in-three-layers.md` D1. Everything else in
this section is still unknown.
