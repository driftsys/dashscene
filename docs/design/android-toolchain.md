# The Android toolchain, and the D3a probe

    status  as-built at story #839 (v0.19, epic #833). The toolchain and the
            probe are built; **the D3a measurement on target hardware is not
            taken** — no device was available, and the deferral is recorded
            under "What is not measured" below.
    source  story #839
    why     [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
            carries the decisions; D3a is the one this record exists to serve.

Android was at zero before this: no target triple, no toolchain, no CI job, and
no FFI beyond `dashscene-unity`'s Unity-facing bindings.

## The toolchain

One target, `aarch64-linux-android`. Nothing else, because nothing needs it: an
Apple-silicon emulator is arm64 too, so one triple serves both a device and the
emulator, and a second is added when something asks for one.

**Plain `cargo build --target`, with the NDK's clang wired from the
environment.** Not `cargo-ndk`. Every other target in this repository is built
with plain cargo — `just wasm`, `wasm-painter`, `wasm-host` are all one cargo
line — and a wrapper would be the only one of its kind, to save three exported
variables.

**The NDK is a documented prerequisite, not something `bootstrap` installs.**
The same trade `web-build` makes for `wasm-bindgen-cli`: it is large, one target
needs it, and every clone paying for it would be the wrong cost. `just android`
discovers it and prints the `sdkmanager` line when it is missing.

Discovery order is `ANDROID_NDK_HOME`, then `ANDROID_NDK_ROOT` — the name
GitHub's runner images set, which is what lets one recipe serve CI and a
workstation — then the highest-versioned directory under the SDK's `ndk/`.
Highest rather than first, so a machine holding several does not silently pin
the oldest by sort order. The `llvm/prebuilt/*/bin` glob absorbs the host tag,
which is `darwin-x86_64` on Apple silicon and `linux-x86_64` on a runner.

**The three exported variables are written once**, in a `_android-env` recipe
that prints them for `android`, `android-lint` and `android-probe` to `eval`.
They were inlined in each of those three until issue #1101, so adding a fourth —
`RANLIB_aarch64_linux_android`, say — was three edits, and a partial one failed
only on `android-probe`, the recipe that needs a device and therefore runs least
often. `_android-env` also checks that the NDK actually ships a clang wrapper
for the API floor below, which `_android-ndk-bin` does not: it guarantees the
directory, not the per-level wrapper inside it.

### The API floor

`ANDROID_API` in the `justfile`, currently **33**. A floor rather than a target
— the oldest device the artifacts will load on.

It was **26** when this record was written, and this paragraph said so for two
stories after it stopped being true: story #862 raised it to 33 and changed only
the `justfile`, so the number here was stale from the moment it landed. Nothing
failed, because no test reads a sentence.

**The floor is 33 because of one function.** `AChoreographer_getInstance` is API
24, but `AChoreographer_postVsyncCallback` — the one carrying a frame timeline —
is `__INTRODUCED_IN(33)`, and D6 puts vsync on the native side. At this floor it
is reachable **unconditionally**: no runtime API guard, and no
`postFrameCallback64` fallback branch. That is the whole consequence of the
choice, and `crates/dashscene-android/src/host.rs` depends on it.

The floor was set against the **target fleet** rather than against Play, which
gates `targetSdk` and sets no minimum.

## What builds

`just android` cross-compiles `dashscene-gpu` — the painter, the whole of what a
host draws through. It compiled without a source change, pulling in `ndk-sys`,
`jni-sys`, `gpu-allocator` and `wgpu-core-deps-windows-linux-android`, so wgpu's
Android backend support needed nothing from this repository.

The CI job `android-build` runs that, the C ABI's conformance check,
`just android-apk`, and `just android-lint` — four steps, in that order.

`android-lint` is clippy on the same triple — and, since issue #1109, the
intra-doc-link pass for it as well, because a `cfg(target_os = "android")` item
does not exist in the host build that `just doc-links` documents. It was added
at issue #1086 because `just android` is `cargo build`: the platform half of
`dashscene-android` and `demo-android`'s JNI half compiled in this job without
ever being linted, the gap `just wasm-lint` closed for wasm32 at PR #907. It
runs **last** because it is `-D warnings` under an unpinned stable toolchain,
and a lint that goes red on a clippy release must not take the
header-conformance check down with it.

`just android-apk` packages both hosts' APKs and is the only **gate** that
compiles any Java in this repository. Before that recipe, nothing scheduled a
Java compile: the harness `build.sh` was reachable only through
`android-splitscreen`, which runs it before its device check but which nothing
runs automatically, and `demo-android`'s script had no caller at all, so its two
files had been compiled by no one (issue #1030). CodeQL's `java-kotlin` analysis
does run on every pull request, but it analyses without a full compile and does
not fail on an unresolved symbol.

It still measures nothing about a device. A runner has no device and no GPU, so
nothing there can measure D3a; a job that appeared to would be the
`t2-check-has-no-teeth` failure the v0.13 tiering exists to remove. Packaging an
APK is a compile check and is not a claim that it runs.

## The host, added at story #841

`crates/dashscene-android` is the Android integration surface, and the first
host to sit **on** `dashscene-ffi` rather than beside it: it drives the C ABI
through its own entry points as a C caller would, which is what D2 says every
platform host does. Driving it that way is also what established the ABI was
sufficient for layer 0 — and that it was not quite. `ds_runtime_detach_surface`
was added there, because D4 needs a call that drops the surface and keeps the
document, and freeing the whole runtime would drop the document with it.

Two threads, and the split is D4's and D6's between them. The **UI thread**
receives the lifecycle callbacks and is the only one that may call
`ANativeWindow_fromSurface`, which needs a `JNIEnv` and a live `jobject`. The
**render thread** prepares a looper, takes vsync from
`AChoreographer_postVsyncCallback`, and owns the runtime — so the thread that
draws is the thread that built the device.

**The destroy handshake is a type on no Android API**, deliberately. `Handshake`
uses two threads and a flag, and is compiled on every target, so `cargo test`
can assert the ordering `surfaceDestroyed` depends on. Everything else in the
crate is behind `cfg(target_os = "android")` and no test can reach it — the same
reason `dashscene-web` keeps its `fetch` and `shown` modules outside the
`wasm32` half.

### What ran, and on what

**On the automotive emulator, 2026-08-09. This is interim evidence and is not
the D3a measurement**, for the reason the section below gives: the only
painter-capable adapter there is a CPU rasteriser.

- A compiled `.dsb` drew, at `1408x483` and again at `792x1099` after a
  rotation. The vsync loop reported its first callback and its first frame.
- **Backgrounding** and **rotation** each ran the destroy handshake, and the
  thread ids in logcat show the ordering: the UI thread entered, the render
  thread detached and freed, and only then did the UI thread return. The UI
  thread was blocked for **80 ms** on a release build — 27 ms on the
  split-screen transition — and the first teardown of a debug build took 1.15 s.
  `crates/dashscene-android/src/handshake.rs` carries the same three figures
  beside the reporting interval they set. Neither crashed, and re-attach built a
  fresh render thread each time. That block is a whole runtime teardown rather
  than just a surface drop, which is issue #872.
- **Split-screen was not exercised on this image.** It declares no multi-window,
  freeform or split-screen feature at all — `pm list features` returns none and
  `ro.build.characteristics` is `automotive` — so the third of D4's three cases
  needs a different emulator image. It does not need different hardware, which
  this record claimed until 2026-08-15: the case was run on 2026-08-14 against a
  handheld image, and the harness entered the destroy handshake and never
  returned (issue #960, open). The recipe that runs it is `android-splitscreen`,
  and it asserts on the markers `HarnessActivity` logs. Since 2026-08-15 the
  completion marker is logged only inside the `handle != 0` guard, and a third
  marker names the case where no runtime handle was obtained; a device that
  could not be obtained is not that case, because it returns a non-zero handle.
  **`nativeIsRunning` does not answer it either**, which this record said until
  2026-08-15: it reports `Handshake::is_running`, true for `Starting` as well as
  `Running`, and the render thread reports `started()` only once its attach has
  returned — so a thread wedged inside an attach answers `true`, the same answer
  a drawing loop gives. What does answer it is the pair of markers around the
  attach **read together with the failure line**, which is a three-way reading
  rather than the two-way one this record carried until issue #1080:
  `attaching a WxH surface` is written before every acquisition, `attached`
  after every one that succeeded, and `attach failed:` or
  `could not rebuild the surface:` after one that finished and failed. Only
  `attaching` followed by none of the three is the wedge. Reading "no
  `attached`" on its own as a wedge calls every failed attach one, which is the
  same shape of wrong advice #1080 was filed to remove. The
  `android-splitscreen` recipe named `nativeIsRunning` in two comments until PR
  #1098 corrected them under #1080. The v0.19 driver prompt asserted the
  emulator could exercise all three cases; for this AVD that is false.

  **The measurement that explains it, taken 2026-08-15 (issue #960).** The
  attach never returned because the build was unoptimized, not because the
  emulator cannot run this path. Cold launch to first frame, same emulator:
  **0.74 s for a release build**, and **over 218 s for a debug one**, abandoned
  before it completed. `just android` builds debug and `android-splitscreen`
  packages what it built, so every run of that recipe has used the slow build.
  With a release library the split-screen case passes end to end and the
  handshake completes in **27 ms**.

  **That run also needs the emulator started with `-gpu host`** (issue #1158,
  measured 2026-08-16). Under the default GPU mode the painter cannot obtain a
  device — `Failed to open rendernode` — the harness draws a black frame, and
  the same release library fails at `assert-drew` instead. So the release build
  is necessary and not sufficient, and the sentence above holds only with the
  flag. `just android-probe` reports what the painter's own `request_device`
  gets on the attached adapter, which is the cheap way to check the mode before
  a ten-minute run.

A static document draws once and then stops, which is the idle skip working: the
generation does not advance, so no frame is worth drawing. Nothing about frame
_rate_ can be read from this, and nothing here says Android works.

### The harness that ran it

`crates/dashscene-android/harness/` — a manifest, two Java files and a
`build.sh`. **Not Gradle**, and the same trade this record already makes for
plain `cargo build --target` over `cargo-ndk`: Gradle would be a second build
system plus a Kotlin toolchain, to produce an APK whose whole content is one
manifest, two Java files and a shared library. `aapt2`, `javac`, `d8` and
`apksigner` do it directly.

**What that chain is pinned to, and what it is not** (issue #1058). The SDK's
build-tools are discovered rather than pinned — highest installed, filtered to
release versions, because `sort -V` puts `36.0.0-rc3` after `36.0.0` and a
release candidate was therefore preferred to every release. The **JDK is
pinned**, at temurin 21 in `android-build`: both scripts hard-code
`javac --release 17`, which is the class-file level `android.jar` and `d8`
accept, and that is a separate question from which JDK compiles them. The
image's default `JAVA_HOME` has moved repeatedly, and a gate that takes it from
the image is a gate whose toolchain changes without a commit.

**The library it packages is named, not searched for** (issue #1057).
`DASHSCENE_ANDROID_PROFILE`, defaulted to `debug` because that is what
`just android` builds. Both scripts used to prefer a `release` library when both
existed, so a machine that had ever built `--release` for this triple packaged
the older artifact — an APK shipping a library that predated the change under
test, reported as a successful build.

`javac` and `d8` are each given the file list through an `@argfile` rather than
through `find -exec ... +`, which batches by `ARG_MAX`: a second `d8` batch
re-invokes it with the same `--output` and only the last batch survives. Every
`classes*.dex` is staged, not the first, so a multidex build cannot lose one.
The APK is then checked to carry the dex and the shared library, and verified
with `apksigner`, before the intermediates are removed — packaging that dropped
an entry otherwise yields an APK that installs and throws
`ClassNotFoundException` at launch.

It is a **lifecycle harness and not the demonstration**. Story #842's
`demo-android` is that, and it cannot be reached by shipping the showcase as a
`.dsb`: the showcase animates by writing a named signal and, in one scene, by
switching a variant, and no committed `.dsb` carries a signal, a binding row or
a variant table (issue #617). The C ABI has no builder entry point either — that
is layer 2, D8 — so a host that wants a scene built in code links the crates
directly, as `demo` and `demo-web` do.

One trap the harness hit, recorded because the failure is silent: a debug
keystore regenerated on each build signs each build differently, and Android
then refuses the update with `INSTALL_FAILED_UPDATE_INCOMPATIBLE` while the
device goes on running the **previous** build. A test then reads as a working
build that ignores its own changes. The keystore lives outside the directory the
script wipes.

## The measurement apparatus, and the procedure at the device

    status  as-built at story #1229 (v0.21, epic #1107). The apparatus is built
            and verified on an emulator; **no measurement in this section is a
            device measurement**, and every one of #885, #960, #969, #842 and
            #1128 stays open until a device has run it.

**One command.** `just android-measure` runs everything below in an order that
requires no decision, and writes one directory a reader who was not there can
follow:

    target/android-measure/<device timestamp>/
      README.md            what this is, which issue each file belongs to, and
                           whether it is an emulator result
      environment.md       the device, its properties, and the commit
      adapter-report.txt   the D3a probe — #885's measurement in full
      layer-cost.txt       the render-target sweep — Q-6, #1128
      frames.md            one row per 240 drawn frames, with CPU beside it
      frames-<scene>.log   the raw logcat each row is derived from
      attach.md            cold launch to first frame, release against debug
      sf-timestats.txt     the compositor's own frame statistics over a window
      sf-latency.txt       superseded on Android 15 — see below
      gfxinfo.txt          HWUI's view of the same process — see the caveat
      perfetto-*.md/pbtx   the trace configuration and the command that uses it

**Start the emulator with `-gpu host`, and check the adapter before anything
else.** The bundle runs the adapter probe first for exactly that reason: under
the default GPU mode the painter obtains no device, every frame is black, and
the cost of discovering that from a frame capture is minutes rather than seconds
(issue #1158). The section above records what each mode reports.

### The five parts, and what each can and cannot say

**The adapter probe (#885)** is unchanged and is described below. It is the
first step because its verdict decides whether anything after it is a
measurement or an absence.

**The frame capture (#842)** launches the showcase host once per scene and reads
the lines `demo-android/src/timing.rs` has printed since 2026-08-09 and which
nothing read until now. One launch per scene is not a choice: `ShowcaseFrames`
takes its scene once, from the intent's `--es scene` extra, and nothing switches
it at run time. The capture asserts that the scene which drew is the scene that
was asked for, because `select` falls back to the first scene for an unknown
name rather than failing the launch — so a stale scene list would otherwise
produce three rows that all secretly measure `surfaces`.

Three properties of that table are easy to misread, and it states all three in
its own header:

- **`fps if unpaced` is not the frame rate.** The loop is paced by vsync;
  `Sample::fps_if_unpaced` is the rate the measured work alone would allow,
  which is what says how much headroom there is.
- **A sample of 240 drawn frames is not 240 vsyncs**, and on the showcase it is
  far more. The pulse advances every 2.5 s and the loop skips every frame that
  would draw nothing, so `advanced()` is false for most vsyncs — measured on
  2026-08-17, one sample spanned between 10 s and 57 s of wall time. The
  `wall s` column is what exposes it.
- **The first sample of a scene carries pipeline warm-up.** Measured, a first
  sample reported `max 349 ms` against a `p50` of 19 ms. Rows are numbered and
  never averaged, so the reader drops what they judge to be warm-up rather than
  having it folded in.

**CPU (#842's other half)** is `utime + stime` from `/proc/<pid>/stat`, over the
interval **each sample covers** rather than over the session. The alignment is
what makes the column mean anything: `Timing` clears its buffers on every
report, so consecutive sample lines partition the drawn frames exactly. The
sampler writes its readings **into logcat**, through the device's own `log`
command, so both line kinds carry one clock and one ordering — the alternative
needs the device epoch mapped onto the host's, and `date +%s` on the device is
whole seconds, a ±1 s error on an interval of a few seconds.

**GPU, vendor-neutral.** `dumpsys SurfaceFlinger --timestats` is the source that
describes this painter's frames, because the painter's output _is_ a composited
layer: total frames, missed frames, a present-to-present histogram, a
frame-duration histogram, and per-layer jank payloads. It is enabled,
**cleared**, collected over a window the script names, and then dumped. The
clear is what bounds the window and it is not optional: `-enable` on an
already-enabled SurfaceFlinger resets nothing, and a dump taken without it
reported a `statsStart`/`statsEnd` 161 s apart for a 12 s collection — an
unbounded interval that looks exactly like a bounded one. `statsStart` and
`statsEnd` are in the bundle, so the window can be checked rather than assumed.

**Two other sources are captured and neither is the painter's frames.** Both are
stated here rather than left to be discovered at the device:

- `dumpsys SurfaceFlinger --latency` **returns nothing on Android 15.** Measured
  on 2026-08-17 against all four layers this process has — the
  `SurfaceView[...]` container, its `(BLAST)` child that actually receives the
  buffers, a background layer and an input sink — it gives the refresh period
  and zero frame rows. The timeline sources superseded it. It is still captured,
  because the API floor is 33 and it does work on older releases, and because a
  recorded empty result is what stops the next person trying it.
- `dumpsys gfxinfo <pkg> framestats` reports HWUI's rendering of the **View
  hierarchy**, and this host's hierarchy is one `SurfaceView` that draws nothing
  after layout — the painter draws into that surface directly through wgpu, so
  its frames never enter that pipeline. On the same run it reported
  `Total frames rendered: 2` while the compositor counted 192 frames of the same
  process over 12 s. That contrast is why it is captured at all.

Vendor counters are **deliberately absent** from the committed Perfetto
configuration. The counter ids differ between Adreno, Mali and PowerVR, and the
adapter is unknown until the probe reports it on first contact; a guessed id
yields a silently empty track, which is worse than a configuration that does not
claim to hold counters. `adb shell perfetto --query` on the device names what it
actually offers, and that is the second pass.

**The attach procedure (#960)** times a cold launch to its first drawn frame,
per profile, **with a timeout** — so "no completion observed within N seconds"
is a recorded outcome rather than a developer waiting. Two intervals are
reported from `machine.rs`'s own markers: `attaching` to `attached` is the
acquisition, which is what #960 says is unmeasured, and `attaching` to
`first frame` adds the first tick and draw. `am start -W`'s `TotalTime` is
recorded beside them and is a different quantity: a window can be displayed with
nothing drawn in it, which is exactly what issue #1158 produces.

Four outcomes are distinguished and not two, on the reading issue #1080
established: `attached` is a finished acquisition; `attach failed:` or
`could not rebuild the surface:` is one that finished and failed; `attaching`
with none of those after it is the wedge; and no `attaching` at all means the
loop never started. Reading "no `attached`" as a wedge on its own calls every
failed attach one.

`just android release` exists for this, and is new at this story: the profile is
a parameter on `android`, `android-apk` and both `_apk-*` recipes, defaulted to
`debug` so every existing caller is unchanged. Before it there was no recipe for
the release half of #960's comparison at all, which is why the figure below it
was taken by hand.

### The witness, and which window it judges (issue #1191)

`assert-drew.py` is the only evidence that the Android painter drew anything,
and until story #1229 it surveyed the **whole display** minus a fraction of the
top and bottom. That is roughly the painter's area in the fullscreen phase and
it is not in multi-window, where the painter owns about half the screen and
another window owns the rest — so the verdict was partly about the neighbour.

It now takes `--rect X,Y,W,H`, and `HarnessActivity` logs exactly that on every
`surfaceChanged`, from `View.getLocationOnScreen`:

    I dashscene: harness: window bounds 0,176 2560x1360

`just android-splitscreen` reads the **last** such line — the cold launch and
the Settings launch each resize the window — and passes it. With no line the
script surveys the display exactly as before, so a run that cannot read the
bounds is degraded rather than broken.

**The chrome fractions do not apply inside a rect**, and that is the part the
issue did not anticipate. They exist to remove the status bar, the title bar,
the multi-window caption and the gesture-navigation bar, every one of which is
outside the painter's window — so applying them again would discard 26% of the
pane, and on this emulator it discarded the wrong 26%: an 18% top fraction of
the window above starts the survey at row 420, and the harness's document is
drawn in rows 176 to 300. The whole of what the painter drew fell inside the
exclusion, and the script reported "the painter drew nothing" about a frame that
had drawn.

**Two corrections to what that issue says**, both established by construction
rather than argued:

- Its stated defect is a false **PASS**, with a colourful neighbour supplying
  the colours and the light ground while the painter's pane is black. That
  mechanism is not reachable in the code PR #1188 shipped: the issue names
  `MIN_LIGHT_FRACTION`, which that pull request's final revision replaced with
  `MAX_INK_FRACTION`, and a black pane filling half the survey is about half its
  ink — five times over a 10% ceiling.
- What is real is the mirror image, and for a gate it is worse: the painter
  draws correctly, the neighbour is dark, and the display survey **fails**. A
  false FAIL in the one check that witnesses the painter reads as a painter
  regression.

The rect closes both directions, because the verdict stops depending on the
neighbour at all. `assert-drew-test.py` carries all four combinations.

**What it does not close is the calibration, and that needs a device (issue
#1232).** The three thresholds are derived from a host render of the fixture
filling the frame. The harness draws its document at the document's own size, so
on this emulator's 2560x1360 surface the drawn content is about 1% of the pane —
79 distinct colours, and 98.8% of the pane dark. Both bounds lose their meaning
together: the ceiling refuses a frame that did draw, and the floor, which exists
to say the glyphs drew, is satisfied by the emptiness alone.

So `just android-splitscreen` does not pass on this AVD today. **That is not a
regression from story #1229**: `origin/main`'s own script, run on the same
screenshot, gives the same FAIL with a worse diagnosis. It does contradict the
2026-08-16 line above recording an end-to-end pass on the same AVD, and this
record does not claim to know which run was anomalous.

### What an emulator run showed, and what it does not settle

**All of these are emulator results on the API 35 `medium_tablet` image with
`-gpu host`, on 2026-08-17. None of them is a device measurement, and none of
them closes any issue.** They are recorded because two of them contradict what
this file said, which is a fact about the records rather than about a device.

**The debug attach completed.** 2.37 s to first frame in debug against 1.50 s in
release — a ratio of about 1.6, not a hang. This file and issue #960 both say
"over 218 s for a debug one, abandoned before it completed", and that figure was
taken on the **automotive** image in its **default GPU mode**. So "a debug
attach never completes" is not a property of the debug build on its own. Which
of the two variables explains the earlier result is **not settled by this run**
— one measurement in one configuration cannot refute a class — and #960's device
half is what settles it.

**The render-target sweep resolves on this GPU.** `just android-layer-cost` fits
a line over 0 to 12 layers at 1920x1080 and reported **+0.198 ms per layer, with
a standard error of 0.005 ms**, on the host's Metal backend. An earlier run of
the same sweep on the same machine gave **+0.427 ms** — the difference is what
else the machine was doing, which is precisely why the figure is reported with
its own uncertainty and why neither number is a fact about anything but that
host. Q-6 is about a tiling GPU, so both are quoted only as evidence that the
probe produces a resolvable answer rather than noise.

**It produced noise first, twice, and both fixes are the interesting part.** At
30 frames per point the marginal column swung ±1.3 ms with no trend in it;
taking the **minimum** per point fixed that, since every source of error in this
measurement makes a frame slower and none makes one faster. Then the test
deciding whether the slope meant anything compared it against the residual
spread over the sweep — which reads as honest and is a **1.12-sigma** test.
Simulated over 20000 sweeps at a true slope of zero, that form declared **32%**
of pure noise resolved, at every noise level, since it is scale-free. It is now
three standard errors of the slope, which the same simulation puts at 1.1%, and
the probe prints "BELOW THIS PROBE'S RESOLUTION" with both figures rather than a
number. A six-point sweep on this host demonstrates it: `+0.0717 ms` against a
0.165 ms threshold is refused, where the old rule would have printed it as a
per-layer cost.

**The sweep is the one step of the bundle with no timeout**, because it is a
plain executable under `adb shell` with no host-side loop to bound it.
`DS_LAYER_MAX` and `DS_LAYER_FRAMES` are what bound it instead, and a first run
on unknown hardware should lower both — a shorter sweep is a weaker measurement
rather than a broken one, and the resolution test above reports the weakness
instead of hiding it.

## The probe

`crates/dashscene-gpu/examples/adapter_report.rs`, run by `just android-probe`,
which cross-compiles it, pushes it to `/data/local/tmp` and runs it through
`adb`. A plain executable rather than an APK: adapter enumeration needs no
window and no Java, which is what makes the probe available before any of the
Android host exists.

**It replicates the painter's own `request_device`** rather than comparing two
numbers. `Renderer::on_adapter` asks for
`wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())`, and a
device that cannot meet it fails **at the device request** — earlier than
pipeline creation. So the verdict the probe prints is the painter's own.

It is an example rather than a new workspace member because it measures
`dashscene-gpu`'s own requirement against a device, and because a crate would
cost the registries a new crate has to be added to — thirteen of them when story
#794 added `dashscene-desktop`, enumerated in
[`../decisions/crate-name-map.md`](../decisions/crate-name-map.md).

**A passing probe covers the device request and no more (issue #890); what it
does not cover is enumerated under "What is not measured" below.** The largest
of those is a second requirement the probe cannot reach:
`SurfaceRenderer::new_async` calls `surface.get_capabilities(&adapter)` and
refuses with `RendererError::NoLinearFormat` when no offered format is either
`TARGET_FORMAT` or otherwise free of an sRGB conversion on write, which
[`../decisions/pipelines-and-layer-3.md`](../decisions/pipelines-and-layer-3.md)
D3 makes a term of the contract rather than a preference. That check runs
**before** `Renderer::on_adapter`, so on a real host it is reached first.

Surface formats belong to a surface and a surface needs a window. The same
property that makes this probe available before any of the Android host exists —
no window, no Java — is what stops it asking. So an adapter that satisfies the
device request and offers no linear swapchain format reports as passing here and
then fails at `for_android_ndk` on the device, which is exactly the outcome the
probe exists to predict. **Read "device request OK" as the device request
succeeding, never as the painter running.** The probe prints this caveat on
every run, passing or failing.

Closing it properly needs a throwaway window on the target, which is a larger
piece of work than the probe is; stating the limit is what is done instead.

## What was measured, and what it does not say

**Host, for comparison** — `cargo run -p dashscene-gpu --example adapter_report`
on the development machine:

    backend Metal, Apple M3, IntegratedGpu
    max_storage_buffers_per_shader_stage 29
    device request OK

**Automotive emulator** —
`Automotive_1408p_landscape_with_Google_Play_API_34-ext9`, API 34, on that same
machine. **This is an emulator result and is not the D3a measurement**; both
adapters below describe the host machine's GPU and its translation layer, not a
target device.

    adapter 0  Vulkan, SwiftShader Device (LLVM 10.0.0), Cpu
               max_storage_buffers_per_shader_stage 10
               device request OK

    adapter 1  Gl, Android Emulator OpenGL ES Translator (Apple M3), IntegratedGpu
               driver OpenGL ES 3.0 (4.1 Metal - 89.4)
               max_storage_buffers_per_shader_stage 0
               device request FAILED

**The GLES adapter reporting zero is D3a's mechanism, demonstrated.** D3a says a
device without Vulkan meets the same wall that makes WebGL2 unbuildable for this
painter, where `downlevel_webgl2_defaults` allows zero storage buffers. That
adapter translates OpenGL ES 3.0, and shader storage buffers arrived in GLES 3.1
— so zero is correct rather than a reporting gap, and the painter cannot run on
it.

One precision about the failure line: wgpu names the first limit that fails,
which here was `max_compute_workgroups_per_dimension`, not the storage-buffer
one. The GLES 3.0 adapter reports zero for a whole family of limits. The request
failed and the storage-buffer limit is zero; the request did not fail _because
of_ the storage-buffer limit specifically, and this record does not claim it
did.

### The automotive emulator cannot be made to use the host GPU for Vulkan

**This heading said "the emulator" until 2026-08-17, and as a general claim that
is false.** What is recorded below holds for the automotive image launched in
its default GPU mode, and it does not generalise: on the API 35 `medium_tablet`
image started with **`-gpu host`**, `just android-probe` reports the guest's
Vulkan adapter as the host GPU behind MoltenVK, and the painter's device request
succeeds on it. Measured on 2026-08-17, story #1229:

    adapter 0  Vulkan, Apple M3, IntegratedGpu
               driver MoltenVK 1.4.0
               max_storage_buffers_per_shader_stage 31
               device request OK

    adapter 1  Gl, Android Emulator OpenGL ES Translator (Apple M3), IntegratedGpu
               driver OpenGL ES 3.0 (4.1 Metal - 89.4)
               max_storage_buffers_per_shader_stage 0
               device request FAILED — max_compute_workgroups_per_dimension

That is still an **emulator** result and still describes this machine's GPU
rather than a device, so it changes nothing about D3a or #885. What it does
change is the reading two paragraphs below: the emulator's only painter-capable
adapter is a CPU rasteriser **in that configuration**, and with `-gpu host` on a
handheld image it is not. It is also the mechanism behind issue #1158 — the flag
does not merely make the emulator faster, it is what gets the painter an adapter
it can use at all.

The rest of this section is the automotive-image measurement, unchanged. The
emulator ships `Vulkan = off` in its own `lib/advancedFeatures.ini`, which reads
like the reason its Vulkan adapter is a CPU rasteriser. It is not. Setting
`Vulkan = on` in `~/.android/advancedFeatures.ini` and restarting applies the
flag —

    Feature 'Vulkan' (21) is overridden to 'enabled'

— and the emulator then selects SwiftShader anyway, because it sets the ICD to
SwiftShader explicitly rather than looking for a host driver:

    initIcdPaths: ICD set to 'swiftshader', using Swiftshader ICD
    Selecting Vulkan device: SwiftShader Device (LLVM 10.0.0), Version: 1.3.0
    useVulkanComposition: false
    useVulkanNativeSwapchain: false

The probe reports byte-identical output with the flag on and off. macOS exposes
no native Vulkan ICD, and this emulator build does not route to MoltenVK, so
there is nothing for the flag to select. The override was reverted.

**The consequence is the one that matters for planning.** On this emulator the
only adapter the painter can use is a CPU rasteriser, and the only adapter on
the host GPU exposes zero storage buffers. So the emulator is usable for
checking that something _works_ and is useless for checking how fast it is —
which is a second, independent reason story #842's frame-rate measurement waits
for hardware rather than being approximated here.

### What is not measured

**Whether the target device class exposes Vulkan.** That is the D3a question and
it needs hardware, which was not available (expected roughly 2026-08-23).

**Which adapter the host would actually pick (issue #890).** The probe passes if
**any** enumerated adapter satisfies the device request.
`SurfaceRenderer::new_async` picks exactly one, through `request_adapter` with
`PowerPreference::default()` and a `compatible_surface` — and it need not be the
one that passed. The emulator run recorded above is precisely that shape:
adapter 0 (Vulkan, SwiftShader, a CPU device) passes and adapter 1 (the GLES
translator, an integrated GPU) fails, while `PowerPreference::default()` is
`LowPower`, which ranks an integrated GPU above a CPU one. Read the summary line
as the at-least-one claim it makes.

**Whether a surface would offer a format the painter can blend in (issue
#890).** `SurfaceRenderer::new_async` refuses with
`RendererError::NoLinearFormat` when `linear_format` finds none, and that check
runs **before** `Renderer::on_adapter` — so a passing device request does not
even mean a host got as far as requesting a device. Surface formats belong to a
surface and a surface needs a window; this probe has none, which is the same
property that makes it available before any of the Android host exists.

**Anything after the device request.** `Renderer::on_adapter` builds the shader
module, the bind group layouts and the pipelines, and `new_async` then calls
`check_extent`. Issue #714 is a recorded failure of that last one — a host
aborted on the first resize past 2048 on a device reporting 16384.

The first three are stated rather than closed: closing any of them needs a
surface, and therefore a window on the target, which is a larger piece of work
than the probe is. **The probe prints all of this on every run**, passing or
failing, so a transcript carries it.

**It is carried as debt (#885) rather than as a slice gate** (2026-08-09). The
epic made it a gate because a lot would be built on an unverified assumption;
what discharged most of that is that nothing built is backend-specific — wgpu
selects the backend itself — and that a device which cannot meet the limit fails
at the device request rather than subtly. The painter has also now been run
through Vulkan end to end, pipelines and text and blur, rather than only
`request_device` (stories #841 and #842). Blocking a slice on a measurement
nobody can take stalls work the answer does not invalidate.

**What is not relaxed:** nothing describes Android as working until #885 is
measured, and emulator results stay labelled as emulator results.

### The emulator numbers depend on how the emulator was launched

Recorded here so a re-run is not read as a contradiction. The two adapter sets
above came from a **default-GPU** launch. A launch with
`-gpu swiftshader_indirect`, on 2026-08-09, reported adapter 1 as ANGLE over
SwiftShader at **GLES 3.1** with `max_storage_buffers_per_shader_stage` of
**18**, passing the device request — where this record holds a Metal-backed
translator at **GLES 3.0** reporting **0** and failing.

Both are correct for their configuration: shader storage buffers arrived in GLES
3.1, so a 3.0 translator reports zero and a 3.1 one does not. The consequence is
that **"the GLES adapter reporting zero is D3a's mechanism, demonstrated" is a
statement about a configuration**, not about the emulator — and any emulator
number quoted here should name the `-gpu` mode that produced it.

Until the hardware measurement exists, **nothing may describe Android as
working** — not this record, not a design document, not an issue. D3a's status
in
[`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
stays "a risk to check", and story #839 carries the step that changes it.
