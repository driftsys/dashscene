# The Android toolchain, and the D3a probe

    status  as-built at story #839 (v0.19, epic #833), and **the D3a
            measurement is taken**: 2026-08-17, on a Pixel 5, under "What the
            device measured" below. That section is the evidence #885, #842 and
            #1128 close against; whether they are closed is the tracker's to
            say, and this record does not assert it. The emulator figures
            elsewhere in this file are kept as emulator figures and are
            labelled as such.
    source  story #839, and story #1229 for the apparatus that took the
            measurement
    why     [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
            carries the decisions; D3a is the one this record exists to serve.

Android was at zero before this: no target triple, no toolchain, no CI job, and
no FFI beyond `dashpaint-abi`'s boundary-B gate.

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
runs **last** because it is `-D warnings`, and a lint that goes red must not
take the header-conformance check down with it. That used to mean a lint
arriving unannounced from a stable release; since `rust-toolchain.toml` pinned
the compiler it means one arriving at a deliberate bump, which is when the
ordering earns its keep.

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
sufficient for layer 0 **in its runtime-draws form** — the form D1 states this
host in, and since 2026-08-18 not the only one — and that it was not quite.
`ds_runtime_detach_surface` was added there, because D4 needs a call that drops
the surface and keeps the document, and freeing the whole runtime would drop the
document with it.

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

## What the device measured

    status  taken 2026-08-17 with `just android-measure` (story #1229's
            apparatus). **This is a device measurement**, and it is the first
            in this project. It is the evidence #885, #842 and #1128 asked
            for; a record does not close a ticket.

**The device.** Google Pixel 5 (`redfin`), Android 14 / API 34, `arm64-v8a`,
over USB. Adreno 620 — a **tiling** GPU, which is the class R-T1's tile-memory
argument is about, and the reason the render-target figure below could not have
come from a desktop.

It is one mid-range 2020 handset and not the target fleet. Every number here is
a number about **this** device: the shape of the answers is what generalises,
not the values.

### D3a — the Vulkan measurement (#885)

`just android-probe`, which is the painter's own `request_device` replicated:

    adapter 0  Vulkan, Adreno (TM) 620, IntegratedGpu
               driver Qualcomm Technologies Inc. Adreno Vulkan Driver
                      (build 4783c89, 2020-11-30, EV031.31.04.01, QPR2)
               max_storage_buffers_per_shader_stage 32
               device request OK

    adapter 1  Gl, Adreno (TM) 620, IntegratedGpu
               driver OpenGL ES 3.2 V@0490.0 (GIT@4783c89)
               max_storage_buffers_per_shader_stage 4
               device request OK

**D3a's risk does not materialise on this device, and the GLES row is the more
interesting half.** The painter asks for `downlevel_defaults`, which allows four
storage buffers per stage, and it binds four in the fragment stage — so it has
no headroom there by construction. Vulkan offers **32**, eight times what is
asked. The GLES 3.2 adapter offers **exactly 4** and therefore also passes: the
painter fits it with nothing to spare, and one more fragment-stage storage
buffer would put this device's GLES path outside the contract.

That is the opposite of the emulator, where the GLES translator was 3.0 and
reported **0**. Both are correct: shader storage buffers arrived in GLES 3.1.

**What this still does not cover** is unchanged and is the list under "What is
not measured": which adapter a host picks, whether a surface offers a format the
painter can blend in, and everything after the device request. What the frame
measurements below add is that on this device a real host got past all three —
they were taken through `SurfaceRenderer::for_android_ndk` on a live surface.

### Frame costs (#842)

`demo-android` running each showcase scene under its own `pulse`, **release**
build, three reported samples of 240 drawn frames per scene, at **2340x805**:

    scene       tick   draw mean   p50    p95    max    fps if unpaced   cpu
    surfaces    1.05   26.4-27.6   24.5-26.4   47-60   75    ~35         37%
    typography  1.50   14.6-15.1   11.6        ~19.6   54    ~62         35%
    layout      1.08    5.4- 8.4    4.6- 5.9   11.5-14 31    105-155     15%

`max` excludes each scene's first sample, whose maximum is pipeline warm-up —
227 ms for `surfaces` and 160 ms for `typography`, against p50s of 25 and 12.
The instrument reports per sample and the table is not averaged precisely so
that warm-up is visible rather than folded in.

**Read against a 16.67 ms budget** — which is a 60 Hz budget and **not** a
requirement this project has set; nothing in the specification pins display
geometry, and #549 is open against exactly that:

- `surfaces` spends about 25 ms at p50 and reaches 60 at p95.
- `typography` spends 11.6 at p50 and 19.6 at p95.
- `layout` spends 4.6.

**None of those is a statement about whether the device can hold 60 Hz, and an
earlier revision of this section said they were.** It argued that `surfaces`
"does not hold it" because `fps_if_unpaced` of ~35 showed "real work rather than
the idle skip". That argument does not survive the split below: `fps_if_unpaced`
sums tick, paint and present, and present is mostly **waiting** on the swapchain
— so the figure counts blocking as work, and a scene that presents in 2.5 ms
when unblocked can report 35. The compositor's own count over the same host is 5
missed frames in 532.

What the figures above are is **wall-clock per drawn frame from the combined
instrument**, useful for ranking the three scenes against each other and not for
judging a budget.

**The solve is not where the time goes**, which does survive: `tick` is 1.0-1.7
ms against 5-27 ms of everything else. Where the rest goes needed the split to
answer, and the answer is not the paint path — an earlier revision said "the
paint path is the whole question" and the split measured paint at 0.01-0.10 ms.

**Orientation changes the workload materially**, which is worth recording
because it is not a fill-rate story. The same three scenes in portrait at
1080x~2000 — _more_ pixels than 2340x805 — measured `typography` at 3.8-4.3 ms
against 14.6-15.1 in landscape, a 3.5x difference on fewer pixels. A wider box
lays out more text. Any frame figure from this host has to name its extent, and
a set taken across mixed orientations cannot be compared.

### Where a frame's time actually goes (2026-08-17, later the same day)

**The figures above come from an instrument that timed `paint` and `present`
together, and that turned out to hide the answer.** `demo-android`'s instrument
now times them apart, and the numbers below are from the split one — at
1280x445, release, on the same Pixel 5. Both sets are real; they are reported
separately because the line format differs and mixing them would be comparing
two instruments.

    scene       tick   paint mean/p50   present mean/p50/p95/max   glyphs   cpu
    surfaces    0.25   0.03  0.03       11.7  11.9  12.8  21.7     32       18%
    typography  0.36   0.09  0.09        7.2   9.4  10.7  14.0     446      14%
    layout      0.18   0.01  0.01        7.1   9.6  10.9  17.5     0         8%

**Three findings, and two of them retract an earlier reading in this record.**

**Text is not expensive.** `typography` draws **446 glyphs** and `layout` draws
none, at the same extent in the same run: 0.09 ms of paint against 0.01, and no
measurable difference in present. An earlier reading here — that `typography`
cost 7 ms more than `layout` and that text was therefore the dominant
per-element cost — was an artifact of one combined timer, and it is withdrawn.

**This project's own instance packing is not expensive either.** `paint` is
**0.01 to 0.10 ms** across all three scenes. Whatever a frame costs, it is not
the packing.

**`present` is mostly pacing, and a second run proved it.** Across scenes it
looked like a flat floor — 9.6 ms for thirty rects and no text, 9.4 ms for 446
glyphs, 11.9 ms for the stress scene — and a cost that barely moves across
frames that different is not content cost. Its **mean sits below its median**,
which is a bimodal distribution rather than a floor.

The confirmation came from repeating the run: `layout` reported **present p50 of
2.86 and 2.52 ms** in two of three samples and 10.08 in the third, in one
capture, at one extent, on one process. **The same scene presents in 2.5 ms when
it does not block and 10 ms when it does**, so the 9-10 ms is not work — it is
waiting, and `layout`'s real cost is at most 2.5 ms.

The compositor agrees from the other side: over a 15 s window it counted **532
frames and 5 missed — 0.94%**. A frame path that misses one deadline in a
hundred is not one that is short of GPU time.

**The split between GPU work and waiting was open when this section was written,
and it is still open for the frames in the table above.** Wall-clock around
`present` cannot separate them. The section below prices GPU work on this
adapter, but it does not decompose these rows: it runs at a different extent
(1280x720 against 2340x805, on a record whose own finding is that cost is
fill-rate-bound), on a different scene (solid quads rather than the showcase's
text, which that section says it does not price), and offscreen rather than to a
live surface. What it lifts is the earlier bar on saying anything at all about
GPU cost on this device; what it does not do is subtract a term from these
measurements. The route this paragraph named at the time — the vendor-neutral
Perfetto configuration plus Adreno counters — **was tried and does not work on
this device**; what replaced it is timestamp queries inside the painter. "What
the GPU costs" below carries both the result and why the prescribed route was
abandoned.

**Read against a CPU budget**, which is the one thing these numbers do support:
the app's own per-frame CPU is `tick + paint`, so **0.19 ms for `layout`, 0.45
ms for `typography` and 0.28 ms for `surfaces`** — the sums of the table's own
two columns, which an earlier revision got wrong for `typography` by carrying
run 1's terms against run 2's table.

### What the GPU costs (2026-08-18)

`crates/dashscene-gpu/examples/gpu_time.rs`, built with the `gpu-timing` feature
and run by `just android-gpu-time`, brackets the frame's command encoder with a
wgpu `QuerySet` and converts the two timestamps with `get_timestamp_period()`.
The Adreno 620 offers `TIMESTAMP_QUERY`, `TIMESTAMP_QUERY_INSIDE_ENCODERS`,
`TIMESTAMP_QUERY_INSIDE_PASSES` and `PIPELINE_STATISTICS_QUERY` —
`just android-probe` prints all four — so the figures below are the device's own
GPU clock rather than wall-clock around a submit.

Offscreen at 1280x720, release, 60 frames per row after 10 discarded:

    rects  layers    gpu min ms   gpu p50 ms   gpu max ms
        0       0         1.805        4.083       19.906
        8       0         3.144        6.714       43.168
       32       0         7.105        7.480       25.525
       32       1         7.983       10.086       38.359
       32       4        10.632       10.879       74.527
       32       8        14.172       17.816      106.760

**Only the minimum reproduces, which is why every reading below uses it.** The
sweep has been taken three times, from three builds of the probe. They are
lettered **A**, **B** and **C** here, because this record already uses "run 1"
and "run 2" further down for the two archived capture bundles — neither of which
holds any GPU output.

- **Sweep A** produced the table above, from the first build that measured.
- **Sweep B** followed a refactor later in the same session.
- **Sweep C** was built from the tree at `747e1093`, the commit that introduced
  this section.

Their minima, in milliseconds, and the worst spread on each row:

    row     sweep A   sweep B   sweep C   worst spread
    0/0       1.805     1.804     1.805          0.001
    8/0       3.144     3.144     3.145          0.001
    32/0      7.105     7.104     7.105          0.001
    32/1      7.983     7.980     7.983          0.003
    32/4     10.632    10.641    10.629          0.012
    32/8     14.172    14.156    14.162          0.016

**Within 0.016 ms on every row.** Sweep C, the one from the tree named above,
sits closer still: its largest deviation from the table is 0.010 ms, on the last
row, and three of its six rows are identical.

**Neither the p50 nor the max column reproduces, and nothing here is drawn from
either.** Row one's p50 reads 4.083, 2.814 and 2.254 ms across A, B and C, and
row five's 10.879, 23.759 and 16.960 — a factor of 2.2. The max column moves
further: row one is 19.906, 4.086 and 4.086. Both record what else the device
was doing. They are printed because a row whose minimum and maximum differ by a
factor of 11, as row one's do in sweep A, is a row worth distrusting.

**Provenance, stated exactly.** The table is sweep A's, and sweep A predates the
revision that added the `DS_GPU_FRAMES` and `DS_GPU_WARMUP` overrides and made a
failed map retire the instrument rather than abort the process. Neither change
touches the path a successful frame takes — the same encode, the same wait, the
same strict `>` on the pair — and the defaults are unchanged at 60 and 10.
**Sweep C reproduces the table from the tree named above; it did not produce
it**, and this record claims only the former.

Sweeps A and B were built from revisions that exist in no pushed ref, because
the pull request carrying them landed squashed. Sweep C is the only one
reproducible from history.

**No raw output is committed for any of the three**, which every other device
measurement in this record has — issue #1254. The figures above are hand
transcriptions until that is fixed, and `measure/android/run.sh` already writes
`gpu-time.txt` into the evidence bundle, so the next device contact can capture
one with no code change.

**The recipe's forwarding is verified.** A separate invocation with
`DS_GPU_FRAMES=5 DS_GPU_WARMUP=2` printed "5 frames per row after 2 discarded",
so both variables reach the device through `adb shell` — a path that had landed
on `main` verified only by `bash -n`. With neither set the recipe passes empty
strings, and `read_usize` in `gpu_time.rs` returns the defaults; the shell does
not do that, and the same guard is what makes an explicit `0` fall back rather
than measure nothing.

**One path is still unexercised on hardware** — issue #1255. The marker on a
partial row, printed as `(N of M frames read back)`, has never fired, because no
sweep has read back fewer frames than it asked for.

**The `0 rects` row is not an empty frame.** The probe's scene always lays a
full-screen background quad, so row one is one 1280x720 quad and the `rects`
column counts what is drawn on top of it.

**The cost is fill rate.** Reading the minima against the area each row shades —
the background quad is 0.9216 Mpx, each content rect is 300x300 px, and every
rect is fully inside the target at both counts:

    row                     shaded area    gpu min    ms per Mpx
    one full-screen quad     0.9216 Mpx    1.805 ms          1.96
    plus 8 rects             1.6416 Mpx    3.144 ms          1.92
    plus 32 rects            3.8016 Mpx    7.105 ms          1.87

**1.87 to 1.96 ms per megapixel over a four-fold range of shaded area**, and
that is the rate of **one pipeline**, not of the GPU — see the layers below,
which shade through a cheaper one.

The instance count moves 1 to 9 to 33 across those rows while the per-megapixel
figure does not, so what a frame pays for is fragments shaded rather than
instances packed. **The draw-call count does not move at all**: with solid paint
and no groups, `draw_runs` takes its early return and each of these three rows
encodes exactly one `pass.draw`, as `Renderer::last_draw_runs` documents ("One
for an ordinary frame"). An earlier revision of this paragraph claimed draw
calls moved with the rest, which was wrong and, if it had been true, would have
left the reading ambiguous between per-call and per-fragment cost. One draw call
throughout is what removes that ambiguity.

**A render-target layer costs 0.88 ms, and that figure does not vary with the
layer count**: 0.878 ms for one layer, 0.882 for each of four, 0.883 for each of
eight. Two 320x320 quads account for 0.2048 Mpx of it, about 0.39 ms at the rate
above; **the remaining 0.49 ms is a residual over three terms, not a blit rate**
— and it is the reason the fill rate above is a property of one pipeline rather
than of the device. A full-target composite shaded at the SDF pipeline's 1.9 ms
per megapixel would cost about 1.75 ms on its own, which is twice the whole
per-layer figure. It does not, because the composite is a different and much
cheaper pipeline: `self.composite_pipeline`, one textured quad, against
`self.pipeline`'s analytic SDF evaluation. Each layer pass is a `LoadOp::Clear`
and a `StoreOp::Store` over a full-extent target, so that 0.49 ms covers a
full-target clear, a full-target store — the tile-memory traffic R-T1 and Q-6
exist to price — and the composite draw. Divided by the target's 0.9216 Mpx it
is 0.53 ms per megapixel for the three together, which is usable for planning
and is **not** a measurement of the composite alone. Separating them needs a
sweep that varies one at a time.

What is established rather than inferred is that the pass is full-extent:
`crates/dashscene-gpu/src/render.rs` draws one quad over the whole target per
layer, and its own "Why full extent, and one per layer" records the choice.

**Why not Adreno counters, which this record previously prescribed.** Three
routes were tried on this device and all three are closed to a retail build with
no `su`:

- `adb shell perfetto --query` lists the registered data sources, and
  **`gpu.counters` is not among them**. This device's `traced_probes` exposes no
  GPU counter producer.
- The `kgsl` and `dma_fence` ftrace tracepoints **exist and do not enable**.
  `measure/android/perfetto-attribution.pbtx` requests them by name, and a 20 s
  trace taken while the painter drew recorded **zero** events from either group
  against about 75 000 `sched_switch`.
- `/sys/class/kgsl` is **refused to the `shell` user**.

Timestamp queries are what remains, and they sit inside the painter rather than
beside it — which is why the `gpu-timing` feature exists rather than a second
Perfetto configuration.

**What this excludes: the swapchain.** The probe is offscreen and windowless, so
no image acquire and no present fall inside the timestamps. That is the point of
it. Those terms are what the windowed measurements earlier in this section
already contained, and this is the term they did not.

**What it still does not settle.** The 1.9 ms per megapixel is measured on rows
that shade through the SDF fragment path with solid paint and no texture
sampling, so that rate prices neither the glyph atlas path nor a gradient. The
three layer rows are not among them: each adds one composite pass per layer that
binds the layer texture and samples it, through a different pipeline — which is
why the per-layer figure is quoted separately and never at the SDF rate. It is
one adapter. And because it is offscreen, it cannot be added to the windowed
figures above to produce a frame total — the two share no common frame.

### Q-6 — the render-target budget (#1128)

`just android-layer-cost`, sweeping 0 to 12 mid-frame render-target switches at
1920x1080 offscreen, 120 frames per point after 5 discarded:

    run 1   +1.9526 ms ± 0.2919 ms (1 s.e.), 13 points, residual 3.6228 ms
    run 2   +1.7676 ms ± 0.3801 ms (1 s.e.), 13 points

**Two independent runs, agreeing inside their own error bars**, which is what
makes this figure worth quoting at all — the probe reports an uncertainty
precisely so a second run can be checked against the first rather than replacing
it.

The slope is **6.7 standard errors** in run 1 and **4.7** in run 2, against a
threshold of three, so both are measurements rather than noise. An earlier
revision said "a factor of about seven", which conflated the slope's size in
standard errors with its ratio to the threshold — that ratio is 2.2 and 1.6 —
and quoted only the better of the two runs. The same probe on an Apple M3
reports **0.20-0.43 ms**, so a tile-based deferred renderer pays roughly five to
ten times what an immediate-mode desktop GPU does for the same switch — which is
the R-T1 argument, measured for the first time.

**The constant does not change, and this is why.**
`dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` is a **count**, and what
was measured is a **cost**. Turning one into the other needs a frame budget, and
this project deliberately has none: no display geometry is pinned (#549), so any
count written here would be derived from an invented budget. What the
measurement does settle is the shape of the answer:

- **A fixed count cannot be right at any value.** 1.95 ms is a cost per switch
  at one resolution on one GPU; the affordable number of switches is
  `budget / cost`, and both terms are properties of the target rather than of
  the document.
- **Eight is not conservative.** At this cost eight switches is about **15.6
  ms** — a whole 60 Hz frame before anything else is drawn. If the placeholder
  was ever read as "eight is fine", that reading is now measured to be wrong.
- **`paint.render-target-budget` stays a warning**, for the same reason: an
  error would enforce a threshold nobody has derived. It becomes an error when a
  frame budget exists to derive one from.

### The attach, and what it does and does not say

Recorded because the apparatus takes it and it is the first such figure from
hardware. **It is not the answer to #960**, whose subject is a different thing —
see the note below. (Phrased without the keyword deliberately: #960 is open, and
a sentence saying an issue was _not_ closed fires exactly as well as one saying
it was, in any pull-request body that quotes it.)

    run   profile   acquire   to first frame   am start -W TotalTime
    1     release   0.27 s    0.31 s           188 ms
    1     debug     1.85 s    14.57 s          227 ms
    2     release   0.31 s    0.34 s           222 ms
    2     debug     0.69 s     0.93 s          183 ms

`acquire` is `attaching a WxH surface` to `attached a WxH surface` — the
adapter, the device and the pipeline set. **A debug build completes**, which is
the part both runs agree on.

**They disagree about how much it costs, by a factor of fifteen, so the figure
is not stable.** Run 1 was the first launch after installing that build and run
2 a later one, which makes one-time cost the likeliest reading — ART optimising
a freshly installed package — rather than steady-state debug slowness. That
would put the steady-state penalty at about **2.7x** rather than 47x. This
record does not settle it: separating the two needs runs deliberately labelled
first-launch-after-install and later, which neither of these was.

**`just android` builds debug**, so this is the path a developer meets first,
and a first launch after install can cost many seconds — 14.6 s in run 1.

### The text path (#969), and D4's two remaining cases

Taken on the same device later the same day, with the harness host — the one
that drives the C ABI and calls `nativeSurfaceCreatedWithText`.
`just android-measure` does not cover these: it exercises the showcase host,
which builds scenes in code and never takes the text path.

**The text drew.** The harness staged its cascade (`Inter 400`, font 258 992 B,
sheet 63 940 B, metrics 4 448 B), took the `1 face(s)` entry point, attached a
2340x805 surface in 0.32 s and reached its first frame 20 ms later. The
screenshot shows `hug inside fill` rendered as dark glyphs on the orange chip
inside the lavender panel — correct shaping, correct colour, legible. **That is
the JNI text entry point, the face cascade and the committed MSDF sheet working
on hardware**, which is the half of #969 that only a device could give.

**Its automated witness said the opposite, and was wrong** — the false FAIL
#1232 predicted, now confirmed on hardware:

    assert-drew: FAIL — 83 distinct colour(s) in the whole of the painter's
    window at 0,209 2340x805 … but 97.8% of it is dark, above the 10% ceiling

Both halves of that sentence are true and the verdict does not follow from them.
The document is a fixed-size `.dsb` drawn at its own size, so on a 2340x805
surface it occupies about 2%; the ceiling was calibrated against a host render
where the fixture fills the frame. **#969 is measured by looking at the frame,
not by the gate**, and the gate stays wrong until #1232 is settled.

**D4's backgrounding case, on hardware.** Home, then relaunch:
`surfaceDestroyed — entering the handshake` to `handshake complete, returning`
took **27 ms**, and the re-attach reached a new frame 0.27 s later. Against 80
ms on the emulator for a release build and 1.15 s for a debug one, the UI-thread
block D4 bounds is smaller on the device than anywhere it had been measured.

**D4's split-screen case, on hardware, and it passes.** A cold
`--windowingMode 6` launch landed in `mWindowingMode=multi-window`; putting
Settings in the other half destroyed one surface and every entry returned —
`entering=1, complete=1`, no wedge. **That is the transition #960 was originally
filed against**, as a 150 s deadlock on an emulator. It corroborates that
issue's own 2026-08-14 correction: the emulator's painter had no GPU device, and
`surfaceDestroyed` blocks for a frame loop that never started. With a device
that draws, the handshake returns.

### What this run does not settle

- **#874's third case is exercised but not closed here**: one clean split
  transition is evidence, not the repeated run that recipe performs, and
  `android-splitscreen` itself cannot pass while #1232 stands.
- **#960** is, since its own 2026-08-14 comment, "a painter that cannot obtain a
  device must say so" — a silent-failure defect, not a measurement. It is
  untouched by this run, and in fact this device obtains a device, so the
  failure path was not exercised at all. The epic's one-line summary of #960 as
  "whether a debug attach ever completes" does not match the issue.
- **No frame budget is established.** The 16.67 ms above is a reference point
  for reading the table, not a requirement.
- **One device.** Nothing here says what a different Adreno, a Mali or a PowerVR
  does, and the GLES row's zero headroom is the figure most likely to differ.

### What the first device contact cost, recorded so it is not re-paid

**Rotation does not stay where it is put.**
`settings put system user_rotation 1` applies only while an app that permits
rotation is in front, and the capture force-stops between scenes — which returns
to the portrait-locked launcher and **resets the setting to 0**. The first
bundle therefore measured two scenes in landscape and one in portrait, with that
scene rotating part-way through its own capture. `adb shell wm size 2340x1080`
is the lever that survives, because it overrides the logical display rather than
asking the window manager for a rotation; `adb shell wm size reset` restores the
device afterwards.

**`frames.md` does not record the extent**, which is how the above went
unnoticed until the raw captures were grepped. The table names the device, the
build profile and whether the source is an emulator, and a reader comparing rows
cannot see that they describe different geometries. That is a gap in story
#1229's apparatus rather than in this measurement, and it is issue **#1236**:
the `attached a WxH surface` line sits in the same capture the samples come
from, so the extent can be attributed per row by the join the parser already
does for CPU.

**The device clock read 2023-12-29** — 2.6 years behind — so the bundle
directory is stamped `20231229T060616Z`. Every interval in it is device-clock to
device-clock and is therefore correct; only the provenance line is misleading.

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
actually offers.

**That query was run, and it closed the route rather than opening it** — see
"What the GPU costs" above, which supersedes this paragraph for the Pixel 5 and
records what replaced it. The paragraph stands only for an adapter whose vendor
does register the producer.

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

**Whether the target device class exposes Vulkan** — **answered on 2026-08-17
for one device**, and the answer is above: an Adreno 620 exposes Vulkan with 32
storage buffers per stage. That is the D3a question settled for a Pixel 5 and
not for a class; the rest of this list is unchanged, because a device figure
does not extend the probe's reach.

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

**That condition is now discharged** (2026-08-17): #885 is measured, on a Pixel
5, under "What the device measured" above. The rule it stated was "nothing
describes Android as working until #885 is measured", and what the measurement
licenses is narrow — the painter obtains a Vulkan device on that device and
draws the showcase at the rates recorded there. **Emulator results stay labelled
as emulator results**, which was always the other half of the rule and is not
affected by having a device figure to sit beside them.

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
