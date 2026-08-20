# Technote — the Unity build environment, and the C ABI seam proven

Informative. **Measured 2026-08-17**, story #1230 under epic #1106. Nothing
depends on this note. It exists because epic #1106's entry conditions are all
the repository owner's — three on the day this was measured, two after the
ruling recorded below, and **none from 2026-08-18**, when the last two were
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
is still open and this note is still its input.

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
Support and Documentation only. It has no Android module and was not used here.
Two editors now sit under `/Applications/Unity/Hub/Editor/`.

**That second editor is now the target** (the owner's ruling of 2026-08-18,
`../decisions/unity-painter-uses-brg.md` D2). Nothing measured below moves — it
was all taken on `6000.3.22f1` and is labelled as such — but two things follow
for whoever builds next. **The Android modules are on 6.3 and not on 6.5**, as
the paragraph above records, so the first Unity Android player on 6.5 installs
them. And **6.5 is not on Unity's LTS stream**: the release API returned
`6000.3` and `6000.0` there on 2026-08-18, which is the same query this note
uses above to identify an LTS line. The departure from this story's LTS
requirement is deliberate and the record carries it.

**The licence is Unity Personal**, an entitlement issued 2026-08-17 to
`~/Library/Unity/licenses/UnityEntitlementLicense.xml`. It was already in place
when this story started, and no activation step was needed for any of the
batchmode runs below.

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
`libdl.so`, `libm.so`, `libc.so` — and not `libc++_shared.so`, which Unity ships
its own copy of in the same APK. So the usual collision between a plugin's C++
runtime and the engine's does not arise for this library as it stands.

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

**The same gap is still open for the Android triple, and this story did not
close it.** `just android` carries no `--release`, and `android-probe` — the one
release build for that triple — builds `dashscene-gpu`'s `adapter_report`
example rather than this crate. So no recipe produces a release
`libdashscene_ffi.so`, and the player below loaded the **debug** one. The
consuming half already exists: `android-apk` reads `DASHSCENE_ANDROID_PROFILE`,
defaulted to `debug` "because that is what `android` builds" (issue #1057).
Nothing builds the release artifact that variable would select.

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

## What is still unknown

**Nothing here settles packaging, and a working P/Invoke is not evidence about
it.** Issue #851's finding that you cannot memory-map through an AssetBundle
stands untouched. Two observations were made in passing and neither is a
conclusion: Unity packaged the native library as a **Deflate-compressed** zip
entry, and the manifest it generated carries `extractNativeLibs="true"`. Those
are Unity's defaults for a _plugin_, and the question that matters is about the
`.dsb` and its bank, which is issue #1124 for how the bytes reach memory and
issue #1125 for how the product reaches a customer's project. #851's second
packaging path needs _stored_ entries; nothing here tested whether Unity can be
made to produce one.

**No data crossed the boundary.** `ds_abi_version` takes no arguments and
returns a `const`. Nothing here says a `#[repr(C)]` struct marshals correctly.
That was issue #859's data plane, which did not exist when this was written and
landed on 2026-08-20 — it still has no C# consumer, which is story #1121's.

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
