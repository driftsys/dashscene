# Driver prompt — lane G: the Android host and the C ABI

Run this with **Opus**. Everything below marked "Verified" was checked against
the tree on 2026-08-15 with `origin/main` at `557179b`. Everything marked "the
issue claims" was not — check it yourself before acting on it.

## Setup

    git worktree add <worktrees>/wt-lane-g-android -b debt/v020-android-recovery origin/main
    cd <worktrees>/wt-lane-g-android
    ./bootstrap

**You need an Android NDK, which `bootstrap` does not install.** `just android`
is your compile gate and the only one `dashscene-android` and `demo-android`
have — their JNI halves compile on no other target. Confirm `just android`
works before you write anything.

## What you own

Six issues. This is the slice's named Android recovery path — the milestone is
"hardening: the critical findings **and the Android recovery path**".

    #940  the rebuild bound never fires: the give-up branch is unreachable
    #888  the Android frame loop's state machine has no tests
    #960  surfaceDestroyed enters the handshake and never returns (split-screen)
    #884  the C ABI cannot say which frame failures are recoverable
    #969  the harness does not exercise the text-carrying entry point
    #981  the JNI text entry point cannot name a face inside a collection

Read each with `gh issue view <n>` before editing.

**#960's title is wrong.** It reads "The runtime draws nothing and reports
nothing when it cannot get a GPU device"; its body is the split-screen
`surfaceDestroyed` deadlock, measured on an emulator on 2026-08-14, with a
reproducer and `Refs #874. Refs #872.` Retitle it or split it — but do not work
the title.

## Do #940 and #888 as one PR

They are the same file and the same defect from two directions. #888 asks for
the state machine to be liftable and testable; #940 is a live bug that such a
test would have caught. Doing #940 alone leaves the next one invisible.

**#940 re-verified on `557179b`** (its body verified against `e4cfc9b`, an older
main — I re-checked, and the mechanism is intact):

    crates/dashscene-android/src/loop_.rs:97    rebuilds: u32
    crates/dashscene-android/src/loop_.rs:106   const MAX_CONSECUTIVE_REBUILDS: u32 = 3
    crates/dashscene-android/src/loop_.rs:423   Step::Rebuild => {
    crates/dashscene-android/src/loop_.rs:443       state.rebuilds += 1;
    crates/dashscene-android/src/loop_.rs:444       if state.rebuilds > MAX_CONSECUTIVE_REBUILDS {
    crates/dashscene-android/src/loop_.rs:449       // **Falls through to the re-post below.**
    crates/dashscene-android/src/loop_.rs:473   state.rebuilds = 0;

Line 473 is reached by both the `Continue` and the `Rebuild` paths, so the
counter runs 0 → 1 → 0 and never reaches 2. Verified. The give-up branch is
dead code.

There is **no `fn step`** in that file today, so #888's proposed shape does not
exist yet — you are writing it, not finding it.

`crates/dashscene-android/src/handshake.rs` already sits **outside** the
`cfg(target_os = "android")` gate with tests, and #888 names it as the pattern
to copy. Read it before designing the extraction.

## The trap this lane exists because of

`crates/dashscene-android/src/loop_.rs` is entirely behind
`cfg(target_os = "android")`. So `just test`, `just test-regression` and the
`android-build` CI job (a **compile only**) all pass with the loop stalled.
**#940 is the third consecutive change to this recovery path that broke it**,
and each was invisible for the same reason.

A related trap, from an earlier Android session: a `cfg` naming a *platform*
rather than a *condition* made a device hang silently at 0% CPU. When you gate
something here, gate on the capability, not on the OS name.

So: **anything you can decide without an NDK symbol must be tested on the
host.** That is the whole point of #888, and it is the only thing that makes
the #940 fix falsifiable.

## Verified facts — do not re-derive

- `crates/dashscene-android/src/` holds exactly: `frames.rs`, `handshake.rs`,
  `host.rs`, `lib.rs`, `logging.rs`, `loop_.rs`.
- `DocumentFrames` is at `crates/dashscene-android/src/host.rs:66`, and
  `impl Frames for DocumentFrames` at 86. That is what #884 says has to guess.
- `FrameError::is_recoverable` is at **`crates/dashscene-gpu/src/surface.rs:127`**
  — not in `render.rs`. Lane D is rewriting `render.rs` heavily but does not own
  `surface.rs`; if you need to touch it, say so in your PR body so the rebase
  order is visible.
- `ds_runtime_draw` is at `crates/dashscene-ffi/src/lib.rs:645`.
  `DsFontFace` is at `crates/dashscene-ffi/src/lib.rs:365` **and** in the
  committed header `crates/dashscene-ffi/include/dashscene.h:101`.
- **#981 verified**: `crates/dashscene-android/src/host.rs:125` writes
  `face_index: 0`. `dashscene_engine`'s own `face_index: u32`
  (`crates/dashscene-engine/src/lib.rs:124`) is the field it cannot reach.
- The harness Java is two files:
  `crates/dashscene-android/harness/java/dev/driftsys/dashscene/HarnessActivity.java`
  and `DashsceneNative.java`. `nativeSurfaceCreated` is declared at
  `DashsceneNative.java:37` and `nativeSurfaceCreatedWithText` at 78 — so the
  binding #969 needs already exists on the Java side and is simply not called.
- `crates/dashscene-android/src/frames.rs` documents why the atlas crosses the
  boundary the way it does. Read it before redesigning #981's descriptor.

## Territory — another session is in this directory

A session is working **#1006** right now, and with it #1007, #1008 and #1029.
Its files are the `android-splitscreen` recipe in the `justfile` and
`crates/dashscene-android/harness/assert-drew.py`. **Do not touch either.**

**PR #1032 is open and it edits `HarnessActivity.java`** — section 1 of #1006,
"the handshake marker is logged only when the handshake ran". Its two files are
that Java file and the `justfile`.

So **start on #940 and #888 only.** They are in
`crates/dashscene-android/src/loop_.rs`, which PR #1032 does not touch, and they
are the pair worth doing together anyway.

**Hold #960 and #969 until PR #1032 merges**, then rebase and re-read #960
before working it. #1032 rewrites `surfaceDestroyed`'s marker logic, and #960's
entire evidence is those markers — "entering the handshake" logged, "handshake
complete, returning" never logged, 150 s later. The two point in opposite
directions (#1032 is a marker printed when no handshake ran; #960 is a handshake
that entered and never returned), so #1032 does not obviously fix #960 — but it
changes the code the observation was made against. **Re-derive the symptom on
post-#1032 `main` before you diagnose it.**

`DashsceneNative.java` is untouched by #1032. Announce it if you need
`build.sh`.

**#1030** — nothing compiles the harness Java, so a broken `HarnessActivity`
lands green — is not assigned to this wave. It is the gate that would catch
what #969 risks breaking. If you edit `HarnessActivity.java`, compile it by
hand with `crates/dashscene-android/harness/build.sh` and say in the PR body
that CI did not.

## What is honestly blocked, and what is not

Say this plainly in the PR rather than implying more was verified than was.

- **#885 is OPEN** — the Vulkan measurement on target hardware has not been
  taken, and there is no device. So **#969's "the glyphs are visible in the
  device run" cannot be closed by you.** You can write the harness path, load
  the committed sheet, and run it on an emulator. Say which of those you did.
- **#969 needs committed assets** — a font file and a committed MSDF sheet (a
  PNG and its metrics blob), read out of the APK. Nothing bakes a sheet at run
  time. That is why the story that added the entry point did not do this. Decide
  where those bytes live before writing Java.
- **#960 is emulator-only and not bisected.** Its own body lists what was not
  established: whether it reproduces on hardware, whether the UI thread is the
  one stuck (an ANR in logcat would say so — logcat was not checked), and which
  part of the handshake does not complete. **Bisect before fixing.** Its
  reproducer needs a cold launch — `--windowingMode 6` against an already-running
  activity is swallowed as `onActivityRestartAttempt`, so force-stop first.
- **#872** (every surface cycle rebuilds the whole runtime and blocks the UI
  thread for about a second) is the same seam as #960, on `v1`, and a far worse
  symptom. Whichever you touch, check it against the other.
- **#884 is explicitly "worth doing when a second host needs it."** It is a
  design proposal — a `DsStatus::SurfaceLost` tail variant, additive under the
  ABI's versioning rule so `DS_ABI_VERSION` does not move. If you take it, the
  header and the Rust half must agree: `just check` runs a `c-abi` gate that
  compiles the committed header from C and checks exactly that. **You need a C
  toolchain for it.**
- **#981 is not blocking** and says so. It also rules out the obvious fix: not a
  sixth parallel array. Weigh a descriptor class or a byte-packed block, and if
  you cannot decide, say so in the PR and stop — do not pick silently.

## Definition of done

1. `just test` between edits. `just build` green before pushing — quote its
   Summary line, do not paraphrase it.
2. **`just android`** — the compile gate for all four Android members. Nothing
   else sees this code.
3. **`just check`** if you touched `crates/dashscene-ffi/` at all, for the
   `c-abi` gate.
4. Push. **`just verify` may fail on the secrets gate for reasons that are not
   yours** — worktrees share one object store. Issue #987 is about that gate.
5. Open the PR **as an ordinary PR, never a draft**.
6. Run `/code-review` on the PR **while CI runs, not after**. Capture every
   finding as a checklist in the PR description. Never drop one silently.
7. Fix all critical findings. File each minor one as its own `debt`-labeled
   issue linked to this work, **on the v0.20 milestone**.
8. **When a critical finding changes the implementation, review the fix too.**
9. In prose and commit messages write **`Refs #<n>`**. A closing keyword fires
   from commit messages that land on `main`, matches mid-sentence, takes only
   the first number, and a negated sentence matches just as well as a positive
   one.
10. Before merging: `gh issue list --milestone "v0.20 — hardening: the critical
    findings and the Android recovery path" --state open` and read it.
11. Rebase onto the latest `main`, squash to one conventional commit,
    force-push, wait for `ci` green **on the commit you are merging**, then
    `gh pr merge --merge`. Merging is strictly serial.
12. After merging, `gh issue view <n> --json state` for every issue your commits
    named, not only those in the PR body.
13. **Before merging, read your own PR's file list** — `gh pr view <n> --json files`.
    Any path outside `crates/dashscene-android/` and `crates/dashscene-ffi/` is a
    stray, and a stray is how a merge reverts another lane.
14. **After merging, check the previous lane's work is still on `main`:**

        git diff --stat <previous-merge-sha> origin/main -- <that PR's files>

    An empty diff is the pass. **This has failed twice.** PR #1037 reverted
    PR #1038 across seven files it never edited, and `main` was missing work four
    issues read as `CLOSED` for 90 minutes (restored by PR #1063). Earlier,
    PR #978 dropped PR #961's `justfile` recipe the same way (#991). CI is green
    through this: the older content still compiles and still passes its own
    tests, because the reverted lane's tests went with it.

## Do not

- Do not touch `crates/dashscene-android/harness/assert-drew.py` or the
  `android-splitscreen` recipe — another session owns both.
- Do not touch `crates/dashscene-gpu/src/render.rs` — lane D owns it.
- Do not claim a device measurement. #885 is open and there is no hardware.
- Do not close #969 on an emulator run. Say what ran and on what.
- Do not merge on a green `just verify` alone. It runs no test tier, and for
  this crate **no tier runs the loop at all**.
