# Driver prompt — wave 4, the device lane: Unity first, then Android

Issues, in order: **#1369, #1346, #1329, #1236, #1347**, then **#1304, #960,
#1215, #1270**.

**Revised 2026-08-28, after PR #1368 merged.** Two changes from the first
version: **#1345 left this lane** — it is carried to `v0.22` and needs the
SA8255P, not the phone — and **#1236 joined it**, ahead of #1347.

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `f495e1a1`, and everything in this revision re-checked at
`0e818315` after PR #1368 merged, both on 2026-08-28. Everything marked "the issue claims"
was not — check it yourself before acting on it.

## You hold the device, and you are the only lane that may

**Verified 2026-08-28:** one device is attached —
`11181FDD4002MY`, Pixel 5 (`redfin`), Android 14 / API 34, Adreno 620.

**Verified from PR #1368's body:** two attached targets make every `android-*`
recipe exit non-zero without a prefixed diagnosis, because the recipes do not
pass `adb -s`. Three other lanes (P, Q, R) are running and all three are
forbidden `adb`. **Do not start an emulator** — an emulator is a second target
and breaks the same recipes.

## PR #1368 has merged — this lane is unblocked

**Verified 2026-08-28:** `origin/main` is at `0e818315`, the merge of PR #1368,
which closed story #1367. Two things it landed that you depend on:

- **`just unity-android`** — build an Android player from the package as
  installed, put it on an attached device, report the read.
- **The editor pin moved to `6000.3.23f1`** (commit `3f682f34`). Only that
  editor is installed on this machine; every `unity-*` recipe defaulted to
  `6000.3.22f1` before that commit, so **any branch based before `0e818315`
  cannot run a Unity recipe at all**. Base on `0e818315` or later.


## One lane, two branches — and why it is one lane

Track A (Android: #960, #1215) and Track B (Unity: #1345, #1346, #1347) both
need the same single device, and the recipes cannot be pointed at one of two
attached targets. **Two lanes would fight over it**, so they are one lane that
runs the phases in order: Unity first, Android second.

One lane does not mean one pull request. **Ship each phase on its own branch**,
so neither review carries the other's diff:

    phase 1   story/v021-unity-device-runs    #1369, #1346, #1329, #1236, #1347
    phase 2   debt/v021-android-attach        #1304, #960, #1215, #1270

Open phase 1, get it reviewed and merged, then branch phase 2 from the new
`main`. The device is idle during phase 1's review — that is when you write
phase 2's failing test for #1215, which needs no device to fail.

## Setup

    git -C <worktrees>/dashscene-staging fetch origin
    git -C <worktrees>/dashscene-staging worktree add \
      -b story/v021-unity-device-runs \
      <worktrees>/dashscene-worktrees/v021-unity-device-runs \
      origin/main   # 0e818315 or later
    cd <worktrees>/dashscene-worktrees/v021-unity-device-runs && ./bootstrap

Phase 2 gets its own worktree the same way, from `origin/main` **after** phase 1
merges — not from phase 1's head.

Use absolute paths and `git -C` throughout — the shell working directory resets
between commands, and a relative-path edit has landed on `main` before.

## The device ruling, from the owner on 2026-08-28: **the Pixel 5 only**

The SA8255P / Adreno 663 automotive board is **not** in this lane. Two of your
issues carry a "target hardware" qualifier that a phone may or may not satisfy:

- **#1345** — its "Done when" says "a named target device", but PR #1368's body
  reads D4 as asking for the target board and says "this is a phone".
- **#960** — its owner ruling of 2026-08-23 says it closes on "one acquisition
  measurement on target hardware, both profiles".

**The owner ruled both closures on 2026-08-28, so you do not have to read the
word "target" for yourself:**

- **#1345 closes on `redfin`**, on the precedent #885 set when it discharged D3a
  on this same phone. D4 gets the value beside the named device **and** the
  "re-check per device class" line naming the automotive board as unread.
- **#960 closes on `redfin`**, both profiles. The confound the 2026-08-23 ruling
  named was the emulator's GPU mode, and a real Adreno 620 removes it. Record
  the automotive image's default GPU mode as an **unmeasured variable**, not as
  a blocker.

Name the device in every record either way. A number recorded against a named
device is never wasted.

Keep the standing rule from epic #1107: emulator results stay labelled as
emulator results. Its harder half — "nothing may describe Android as working
until #885 is measured" — **is discharged**: #885 closed on a Pixel 5 on
2026-08-17.

---

# Phase 1 — Unity

## #1345 has left this lane — do not take it

**Verified on `origin/main` at `0e818315`:** the read is already recorded. D4 in
`docs/decisions/unity-painter-uses-brg.md` carries
`BufferTarget = RawBuffer` under Vulkan on the Pixel 5, and says in normative
text that it "discharges nothing here for the same reason the M3 read does not:
a Pixel 5 is a phone".

The owner carried #1345 to **v0.22** on 2026-08-28 rather than closing it on the
phone. What remains on it is one run on an attached SA8255P — not a build, and
not this lane's. **Do not edit that D4 paragraph.**

## 1. #1369 — sharpen the recipe you are about to drive all day

**This is v0.23 debt, pulled in deliberately**, the same way #1304 is below.
Leave its milestone alone. It groups four findings the four-seat review of PR
#1368 judged real and did not fix, all in `just unity-android`:

- **No URP-pin precheck.** `unity-render` compares `package.json`'s
  `com.unity.render-pipelines.universal` pin against the editor's built-in URP
  before building; `unity-android` reads the built-in version and never
  compares. Both are `17.3.0` today, so it is latent — and it bites after the
  most expensive build here.
- **`waited` counts sleeps, not wall time**, so `timeout` is not the bound it
  names.
- **The poll discards adb's own diagnosis** — `device offline`, `unauthorized`,
  `no devices/emulators found` all become `adb logcat failed (exit N)`.
- **`adb install -r` discards stdout.**

**You will meet the third one first**, because you are the lane that drives a
cable. Fixing these before the long runs costs less than diagnosing a silent
`device offline` in the middle of #1346.


## 2. #1346 — Unity's Android lifecycle over the surface handshake

Rotation, backgrounding, split-screen — the three cases
`docs/decisions/host-integration-in-three-layers.md` D4 names as
use-after-free rather than visual defect.

Read `docs/decisions/the-frame-crosses-under-a-lease.md` first. The lease rules
are what make the failure modes asymmetric: nothing can tick until a lease ends,
so a lifecycle callback that stops a frame loop between acquire and release is a
**hang**, and one that tears the surface down under a held lease is the
**use-after-free**.

**#1267 is the same seam from the other side** and is **lane R's** this wave —
the C ABI is thread-affine and `DS_WRONG_THREAD` cannot tell a dead thread from
a foreign one. If your lifecycle work needs that ruling, say so on #1346 and in
lane R's issue rather than deciding it here.

## 3. #1329 — the third limb only: the shared frame-cost instrument

**Verified from #1329's comment of 2026-08-24:** two of its three limbs are
settled. The Unity app that draws documents and lets a person choose between
them was delivered by PR #1343; the showcase scenes moved to #1342 and landed on
2026-08-26. **What stays open is the third limb**: a per-frame figure whose
definition is stated against the instrument in `demo/src/shell.rs`, and the
device run that limb happens on.

**Verified in the tree:**

- `demo/src/shell.rs:519` — `const TIMING_VAR: &str = "DASHSCENE_FRAME_TIMING"`,
  the environment variable that turns the instrument on. Off by default, so an
  ordinary run pays nothing.
- `demo/src/shell.rs:527` — `const TIMING_SAMPLE: usize = 240`, "how many
  presents one report covers", chosen so it reads in the same units as
  `docs/technotes/frame-budget.md`.

**This is desk work, not device work**, and it is the prerequisite for #1347.
Do it while the device is idle.

## 4. #1236 — the frame table must name the extent, BEFORE #1347 measures

**This is a prerequisite for #1347, not a tidy-up.** Verified from the issue: the
first device bundle mixed three extents without the table saying so —
`surfaces` and `typography` at 2204x805 landscape, `layout` at **both**
1080x1984 and 1080x2050, orientation changing mid-capture. It was found only by
grepping `attached a WxH surface` out of the raw captures.

**Orientation changes the workload, not just the pixel count**: re-measured in
landscape, `typography` gave 14.6–15.1 ms against 3.8–4.3 ms in portrait. A
per-frame comparison between two painters, taken from a table that does not name
the extent, is not a comparison — it is the defect this issue describes, one
painter later.

So: `frames.md` names the extent per row, and a capture whose extent changes
part-way says so rather than averaging across it. Then #1347.

## 5. #1347 — the per-frame cost, Unity beside the lean painter

**The trap is stated on the issue and is the reason it is a story rather than a
command:** a Unity figure taken from `Time.deltaTime` or from the profiler
measures the engine's frame, not the painter's work. Read what
`demo/src/shell.rs` counts, and state in the record which parts of it the Unity
figure includes and which it cannot.

**Name the commit you measured.** Lane Q is changing the BRG painter's buffer
binding and possibly its repack (#1297, #1306). A painter change lands either
before your measurement or after it, never during — ask that lane when it lands,
and record the commit hash beside every number.

---

# Phase 2 — Android

## 6. #1304 first — it is a prerequisite, not a courtesy

**Verified:** #1304 is **open** and sits on the **v0.23** milestone, not this
one. #960's owner ruling of 2026-08-23 says: "Run #1304 first — two sibling
capture scripts still dump the whole logcat ring, and a measurement taken
through one is not evidence."

So a #960 measurement taken before #1304 is not evidence. Pull #1304 into this
branch, say in the PR that you did and why, and leave its milestone alone —
moving an issue between milestones is the owner's call.

## 7. #960 — the debug attach

**Read the issue's last comment before anything else.** Verified: the title and
the body describe framings that are **not** what this issue closes on. The owner
ruled the scope on 2026-08-23:

| framing | disposition |
| --- | --- |
| the **debug attach** — 0.74 s release against no observed completion in debug | **governing** |
| a painter that cannot get a device draws and reports nothing (**the title**) | split off, done by PR #1077 |
| the surface-destroy handshake entered and never returning (**the body**) | not this issue; #874's, measured at 27 ms |

**Closes on** one acquisition measurement on target hardware, **both profiles**,
from `measure/android/attach-timing.sh` — verified present and executable.
Neither emulator run settles it: the two disagree by a factor of fifteen.

`just android` builds **debug**, which is the path a developer meets first — so
the debug profile is the one whose absence of a completion is the finding.

**Then close it**, on the owner's ruling of 2026-08-28: both profiles on
`redfin` settle it, and the automotive image's default GPU mode is recorded as
an unmeasured variable rather than as a reason to hold the issue open.

## 8. #1215 — `HarnessActivity` reports a failed `nativeSurfaceCreated` as an ordinary wait

The file is
`crates/dashscene-android/harness/java/dev/driftsys/dashscene/HarnessActivity.java`.
The issue records that PR #1214 reproduced this defect and fixed **its own
copy**; the harness still has it. Read that fix before writing a second one —
the shape is already settled, and the same fix twice should look the same twice.

**Its failing test needs no device**, so write it while phase 1 is in review.

## 9. #1270 — what the target actually presents, read rather than recalled

**Verified:** #1270 is `owner-input` on this milestone and asks two things — the
deployment's CPU core count (raised as possibly a single-core VM) and what GPU a
virtualized target presents. `docs/specification/03-target-hardware-rules.md`
constrains GPUs and says nothing about CPU parallelism.

**The owner ruled on 2026-08-28 that it is derived, not recalled.** The read
goes into the measurement bundle, and **the GPU half is already there**:

- **Verified** — `measure/android/run.sh:104` runs `just android-probe` into
  `adapter-report.txt`, so the adapter the device actually presents is already
  captured on every bundle. #1270's "what GPU does a virtualized target
  present?" needs no new apparatus.
- **Verified** — `measure/android/run.sh:85`–`:94` writes `environment.md` from
  `getprop`: `ro.product.model`, `ro.product.device`, `ro.product.cpu.abi`,
  `ro.build.version.release`, `ro.build.version.sdk`,
  `ro.build.characteristics`, `ro.hardware`, `ro.kernel.qemu`, `ro.boot.qemu`.
  **There is no CPU core count in that list**, and that is the one thing #1270's
  first half needs.

So the work is small: add the core count to that block — `nproc` or
`/proc/cpuinfo`, whichever the device answers reliably — so one bundle answers
both halves. `ro.kernel.qemu` and `ro.boot.qemu` are already there, which is
what would say a target is virtualized.

That means the SA8255P board answers #1270 in one `measure/android/run.sh` run
whenever it is next attached — no owner recall, no second apparatus. **Do not
answer #1270 from the phone**: a handset is not the deployment the issue asks
about. Record the phone's reading, say which machine it came from, and leave the
issue open for the board.
---

## Files you own

    docs/decisions/unity-painter-uses-brg.md          (D4's device row)
    docs/design/android-toolchain.md                  ("What the device measured")
    unity/com.driftsys.dashscene/Samples~/**          (the frame-cost instrument)
    crates/dashscene-android/harness/java/dev/driftsys/dashscene/HarnessActivity.java
    measure/android/attach-timing.sh                  (and #1304's two siblings)
    demo/src/shell.rs                                 (read it; edit only if the
                                                       instrument's definition
                                                       must be shared)

## Files you must NOT touch

- `unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs`,
  `unity/com.driftsys.dashscene/Runtime/FramePacker.cs` — **lane Q**.
- `.github/workflows/ci.yml`, `unity/editor-compat/DashsceneEditorCompat.cs`,
  `unity/render-gate/RenderGateBuild.cs`,
  `docs/specification/07-embedding-and-distribution.md` — **lane P**.
- `crates/dashpaint-abi/src/lib.rs`, `crates/dashpaint/src/lib.rs`,
  `crates/dashscene-skia/src/lib.rs`, `crates/dashscene-gpu/src/**` — **lane R**.
- `unity/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl` and
  `crates/dashscene-gpu/src/shaders/sdf.wgsl` — generated, never hand-edited.

## Two files more than one lane appends to

- **`docs/design/unity-csharp-host.md`'s "Known gaps, named" list** — verified
  at `:1024`. One line per issue number; only lines naming **your** issues.
- **`justfile`** — append after the existing `unity-*` recipes; do not reorder.

## Every number you take, take twice

`docs/design/android-toolchain.md`'s Adreno 620 section is the precedent: three
independent sweeps agreeing to within 0.016 ms. PR #1299 is **on hold** for
exactly the opposite — a single hand-transcribed sweep with no raw artifact. Do
not repeat that here. Letter your sweeps, publish their minima side by side, and
keep the raw bundle.

## Done when

1. `just unity-android` reports adb's own diagnosis when a cable fails, and its
   timeout bounds wall time — #1369's four findings are dispositioned.
2. The three lifecycle cases have each been run on the device and their outcome
   recorded — including any that hangs rather than crashes.
3. `frames.md` names the extent of every row, and no measurement in this branch
   is taken from a table that does not.
4. The Unity frame-cost figure states its definition against
   `demo/src/shell.rs`'s instrument, names what it cannot include, and names the
   commit it was taken at.
5. The attach measurement exists for both profiles, taken through a capture that
   is not the whole logcat ring, on a named device — and #960 is closed with the
   automotive image recorded as an unmeasured variable.
6. `measure/android/run.sh`'s `environment.md` records the CPU core count beside
   the getprop set, and the Pixel 5's reading is recorded as a phone reading.
   #1270 stays open for the board.
7. Every issue whose reading you could not settle is left open with the number
   recorded and the question put to the owner.

## The rules this branch is under

`AGENTS.md`'s eight stages, and the **implementing-a-change**, **project-gates**
and **shipping-a-change** skills. Test first and confirm the RED — this applies
to #1215 as strictly as to a feature. Garden `docs/wip/` before `just build`;
open the PR ready, never a draft; run the review fan-out; record a disposition
against every finding; squash then rebase, never `reset --soft origin/main`; and
`git diff origin/main...HEAD --stat` before every push.

If a build fails oddly, run `pgrep -fl cargo` before believing it — three other
lanes are compiling.

**Merge last, and in two pieces.** This lane is the largest and it edits both
shared append files, which resolve more cheaply after P, Q and R have landed.
Phase 1 merges before phase 2 starts.
