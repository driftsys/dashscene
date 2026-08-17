# v0.21 driver prompt — the Android measurement apparatus

    status  written 2026-08-17, at the opening of v0.21's preparation, while the
            v0.20 phase-end revision has not yet run and epic #951 is still
            open. Neither blocks this work.
    scope   story **#1229** alone. It builds the apparatus; it takes no
            measurement.
    epic    #1107 (v0.21 — Android and Unity on target hardware), Track A
    branch  `story/v021-android-measurement`, cut from `main`
    leaves  archived to `docs/archive/` when #1229's pull request merges

## Why this story exists

**The target device was expected roughly 2026-08-23, and a device window is
finite.** Track A of #1107 is five items — #885, #960, #969, #842 and #1128 —
and every one of them is a run on hardware. Until this story, each existed only
as a procedure described in prose inside its own issue. The failure this story
prevents is arriving at the device, finding that nothing captures the
frame-timing lines the showcase already prints, and spending the window writing
a parser.

**Nothing here needs the device.** Everything is verifiable on an emulator, and
the verification instructions below say how.

## A correction to read before planning, because it changes the shape

Epic #1107 and [`../roadmap.md`](../roadmap.md) both say that #842 "writes that
reachability into `demo-android` before any device measures anything", and both
call it the one Track A item that is not a pure device run. **That was true on
2026-08-09 and stopped being true the same day.**

`demo-android/src/timing.rs` is the instrument, 236 lines, and
`demo-android/src/host.rs` calls it — the sample is pushed and its line goes to
logcat. Commit `1360c928` states it in its own message: "Story #842's
deliverable, minus the measurement that needs hardware."

So **all five Track A items are device runs**, and this story is what gives them
something to run. Do not re-derive the instrument, and do not write a second
one.

Correcting the two records is **not** this story's job — it is a documentation
fix that belongs to whoever runs the v0.20 phase-end revision, and filing it is
enough. Say so in the pull request rather than editing `docs/roadmap.md` here.

## Read first

- Issue **#1229** — this story, and the definition of done that governs.
- Epic **#1107** — Track A's five items, and why they sit on this slice.
- [`../design/android-toolchain.md`](../design/android-toolchain.md) — the
  toolchain, the AVD, what the probe found and what is **not** measured. The
  procedure this story writes lands here, so read what is already there first.
- [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
  — D3a is the risk `just android-probe` checks.
- `demo-android/src/timing.rs` — the instrument's module documentation says why
  it is a second copy rather than a shared one, and what is deliberately shared.
- `crates/dashscene-android/harness/assert-drew.py` — its docstring is the
  contract the capture half has to keep.

## What is already built, so none of it is rebuilt

- **Both APKs, packaged from committed inputs with no device.**
  `just android-apk` runs both halves and reports both;
  `DASHSCENE_ANDROID_PROFILE` selects the profile and defaults to `debug`. The
  packages are `dev.driftsys.dashscene.demo` (the showcase) and
  `dev.driftsys.dashscene.harness` (the lifecycle harness).
- **The D3a probe, which is #885's measurement in full.** `just android-probe`
  cross-builds `dashscene-gpu --example adapter_report`, pushes it to
  `/data/local/tmp` and runs it. It is windowless and needs nothing else built.
  This story does not reimplement it; it makes its output part of the bundle,
  because the adapter it reports is what decides the vendor tooling step below.
- **The frame-cost instrument**, described above. It reports once per 240
  frames, in the units `demo/src/shell.rs` uses so a device number and a desktop
  number can be read side by side: scene name, frame count, mean tick, and mean,
  p50, p95 and max draw, plus the rate the measured work alone would allow.
  `Sample::fps_if_unpaced` is documented as **not** the frame rate — the loop is
  paced by vsync — and any table this story writes has to carry that distinction
  rather than flatten it.
- **The text entry point.** `HarnessActivity` calls
  `nativeSurfaceCreatedWithText`, and
  `crates/dashscene-android/harness/build.sh` stages the font, the atlas sheet,
  its metrics and the cascade. #969 owes the device run, not harness code.
- **`assert-drew.py`, and it is no longer the script the older issues
  describe.** All five defects filed as #1029 were fixed in PR #1188, with 28
  committed test cases in `assert-drew-test.py` and a CI step that runs them. A
  black frame fails in the fullscreen phase. Read the current file, not #1029's
  body.

## What this story builds

Five deliverables. Take them in this order — each later one consumes the
earlier.

1. **Capture and parse the frame samples.** Nothing today reads the lines the
   instrument prints. Launch the showcase host, capture logcat, extract one row
   per scene, write a table to a file. This is the smallest piece and it is the
   one that makes the frame-rate half of #842 a five-minute run instead of an
   improvisation.
2. **CPU over the same window.** Deltas from `/proc/<pid>/stat` across the
   sample period, so a number is attributable to a scene rather than to a whole
   session. The instrument already discards a sample when the scene changes
   part-way through; the CPU half has to align to the same boundaries or the two
   columns describe different things.
3. **GPU, vendor-neutral first.** `dumpsys gfxinfo <package> framestats` and a
   committed Perfetto trace configuration, so a trace can be taken on the device
   and read afterwards. **Do not guess the vendor.** Counter tooling differs
   between Adreno, Mali and PowerVR, and the adapter is unknown until
   `just android-probe` reports it on first contact. Name the follow-up step and
   what it will need; file it if it is more than a paragraph.
4. **A bounded procedure for #960.** Release against debug, timed from launch to
   first drawn frame, with a **timeout**, so that "no completion observed" is a
   recorded outcome rather than a developer waiting. The standing measurement is
   0.74 s in release against no observed completion in debug, and `just android`
   builds debug, so this is the path a developer meets first.
5. **One run script over all of it**, writing a single evidence bundle. This is
   the deliverable the other four exist for. It runs the probe, the showcase
   capture, the CPU sampler, the gfxinfo pass and the attach procedure, in an
   order that does not require the operator to decide anything, and it writes
   one directory that can be read afterwards by someone who was not there.

## Two items to scope before building, and report on

- **#1191** — `screencap` captures the whole display, and in multi-window the
  painter owns about half of it, so a second window supplies the colours, the
  light ground and the ink while the painter's own pane is black. **No exclusion
  fraction closes this**, because the region to exclude is wherever the window
  manager put the other window. Its body carries three options and a
  measurement. Choose one, and have it ready to verify on first contact with the
  device — the point is that the device window is not spent deciding.
- **#1128 (Q-6)** — `RENDER_TARGET_BUDGET_PLACEHOLDER` is `8` in
  `crates/dashscene-validator/src/lib.rs`, and what it stands in for is the
  repaint cost of one cached surface on the target GPU. **No scene in the
  workspace forces a mid-frame render-target switch**, so unlike the other four
  items there is nothing to run at all. Scope the probe it needs **and report
  before building it**: if it is larger than the rest of this story combined, it
  is its own story under #1107, and saying so is the right outcome rather than a
  failure.

## How to verify without a device

**Start the emulator with `-gpu host`.** Under the default GPU mode the painter
obtains no device, the harness draws a black frame, and the run fails at
`assert-drew` after about ten minutes. That cost was paid twice before issue
#1158 named the flag; `AGENTS.md` and
[`../design/android-toolchain.md`](../design/android-toolchain.md) both carry it
now.

An emulator result describes the host machine's GPU. **Every script this story
writes states in its own output that an emulator result is an emulator result**,
and the bundle records which it holds. That is not politeness: the rule #885
states is that nothing describes Android as working until the measurement is
taken on target hardware, and a bundle that does not say which one it is is
exactly how that rule gets broken by accident.

## What this story does not do

- **It takes no measurement**, and it closes none of #885, #960, #969, #842 or
  #1128. Those close when a device has run this apparatus.
- **It writes no number into any record.** If a script produces an emulator
  number while being tested, that number is test output, not evidence.
- **It does not edit `docs/roadmap.md`** — see the correction section above.

## Before the pull request

- `just build` green, and name the tier in the pull request body.
- **A sibling lane is open on the same day**: `story/v021-unity-toolchain`
  (story #1230) also adds a driver prompt to `docs/wip/`, so it edits
  `docs/wip/README.md` too. Whichever lands second takes a small conflict there.
  **Re-derive the file count rather than incrementing the one you read** — that
  ledger has been wrong twice by exactly that mistake, and it carries the count
  in more than one paragraph. Count with the directory, not with arithmetic.
- A driver prompt has **no row** in `docs/wip/README.md` — captures have the
  table, prompts have that file's prose. Update the prose, in the same commit
  that adds this file.
- Open the pull request as an ordinary pull request, never a draft, and run
  `/code-review` on the number while CI runs.
- `Refs #1229` in the commit message. **A closing keyword next to any other
  number closes that issue**, including inside a sentence saying it was not
  fixed, so write `Refs #N` for every issue named except the one this pull
  request completes.
