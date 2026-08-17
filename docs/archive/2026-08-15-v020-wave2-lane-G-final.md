# Lane G — the final pass

Checked against `origin/main` at `b768dc8d` on 2026-08-15. This supersedes both
`lane-G-android-ffi.md` and `lane-G-completion.md`; read only this one.

## Where you are — 4 of the original 6 are closed

    #888  CLOSED  the frame loop's state machine has no tests      PR #1051
    #940  CLOSED  the rebuild bound never fires                    PR #1051
    #884  CLOSED  the C ABI cannot say what is recoverable         PR #1077
    #981  CLOSED  the JNI text entry point and face_index          PR #1087
    #960  OPEN    the wedged attach — diagnosis fixed, cause not
    #969  OPEN    the harness text path — emulator done, hardware not

**Your own reviews then filed eight more.** That is what is left, and it is the
whole of this pass:

    from PR #1077's review   #1080  #1081  #1082  #1083  #1085
    from PR #1087's review   #1088  #1089
    found while working #981 #1086

Read each with `gh issue view <n>`.

## Setup

Your worktree is on `debt/v020-android-recovery`, already merged. **Cut a fresh
branch from current `main`.**

    cd <worktrees>/wt-lane-g-android
    git fetch origin && git checkout -b debt/v020-android-review-inflow origin/main

You need the NDK. `just android` is the Rust gate; **`just android-apk` is the
Java gate**, added by PR #1053 — run it for any harness Java change.

## #960 and #969 cannot be closed by you, and that is settled

PR #1077 states it in its own "What this does not close", and it is right:

- **#969** — the harness exercises the text path and the glyphs are visible, but
  that is an **emulator** run. **#885 is OPEN** — the Vulkan measurement on
  target hardware has not been taken and there is no device. The first half of
  its "done when" is met; the second is not.
- **#960** — what PR #1077 fixed is the **diagnosis**, not the wedged
  acquisition. The render thread is inside `Frames::attach` and never comes out,
  so it never reaches the poll loop where `handshake.teardown_requested()` is
  read. The handshake was never deadlocked. Whether a debug attach ever
  completes, and whether it wedges on target hardware, is unmeasured.

**Do not attempt to close either.** Instead, as the last step of this pass,
**propose where they should live** — they are hardware-gated, they are sitting on
a slice that is trying to close, and #885 is on **v0.19**, an older milestone
still open. Put the proposal in the PR body; do not move milestones unilaterally.

## Territory — two of the eight are not yours

**#1080** (the split-screen recipe still names `nativeIsRunning`) and **#1081**
(`assert-drew.py`'s premise narrowed when the harness scene became a text frame)
are in the `justfile`'s `android-splitscreen` recipe and in
`crates/dashscene-android/harness/assert-drew.py`.

**Another session owns both files** — it has #1006, #1007, #1008 and #1029.
Before touching either, check whether it has an open PR; if it does, hand #1080
and #1081 to it and say so in your PR body rather than editing behind it.

**#1086** (no gate runs clippy for `aarch64-linux-android`) is a `justfile` plus
`.github/workflows/ci.yml` change — the same file that session is in. Same check
applies.

That leaves **#1082, #1083, #1085, #1088, #1089** as unambiguously yours, all in
`crates/dashscene-android/src/`.

## What the five core issues are about

Verified provenance, not recalled:

- **#1082** — `Handshake::request_teardown` accepts a report interval. From
  PR #1077's review.
- **#1083** — the first attach's teardown check is the one decision that is not
  testable where the others are. PR #1077 moved the rebuild path's guard into
  `machine.rs` "where the decision **is** testable"; this is the half that did
  not move.
- **#1085** — `loop_::render_thread` leaks its `LoopState` deliberately (a posted
  vsync callback cannot be cancelled, so the state must stay readable after the
  loop ends), and that leak now retains a whole cascade.
- **#1088** — `read_face` looks up the class and every field id once per call.
- **#1089** — nothing checks `DsFace`'s field names against the six the type
  declares.

**#1088 and #1089 are the same function and should be one PR.** #1082 and #1083
are both `Handshake`/`machine.rs` and probably another.

## Two things PR #1077 established that you must not undo

Both were found by its review and both are pinned by mutation tests:

- **`waiting()` must not be called holding the state `MutexGuard`.** `Handshake`
  is public, so a reporter that asks the same handshake anything deadlocks on a
  non-reentrant mutex. Putting the call back under the guard **hangs the suite** —
  that is the confirmation, and it is the exact silent freeze the reporting
  exists to end.
- **`Step::Rebuild` calls the same `Frames::attach`** — the second place this
  crate acquires a device. It had no `attaching` line and no teardown check, so
  the wedge had a second entrance. The guard lives in `machine.rs` and
  `a_teardown_requested_while_the_surface_is_lost_stops_instead_of_re_attaching`
  fails when it is mutated away.

## The trap this crate keeps producing

`crates/dashscene-android/src/loop_.rs` is behind `cfg(target_os = "android")`,
so **no test tier runs it** — `just test`, `just test-regression` and the
`android-build` CI job (a compile only) all pass with the loop stalled. #940 was
the third consecutive change to break that path invisibly.

**Anything decidable without an NDK symbol goes in `machine.rs` or
`handshake.rs`, on the host side, with a test.** That is what #888 built and what
#1083 says is not finished.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line.
2. **`just android`** (Rust) and **`just android-apk`** (Java) — both.
3. **`just check`** if you touch `crates/dashscene-ffi/`, for the `c-abi` gate.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` on it
   **while CI runs**. Capture every finding as a checklist; never drop one.
   Expect volume: PR #1077's review returned 14, PR #1039's needed seven rounds.
5. Fix all critical findings. **Review the fix round too** — on PR #1077 the
   review of the fix is where the rebuild-path gap and the mutex deadlock were
   found, and both were worse than what the original pass caught.
6. File each remaining minor finding as `debt` on **v0.20**.
7. **Before merging** — `gh pr view <n> --json files`. Any path outside
   `crates/dashscene-android/` is a stray, and a stray is how a merge reverts
   another lane (PR #1037 reverted PR #1038 across seven files this afternoon;
   PR #1063 exists only to restore them).
8. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
9. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one. **PR #1077 got this right — its
   body opens "Closes nothing."** Copy that habit.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green
    **on the commit you are merging**, then `gh pr merge --merge`.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## The last step of the pass

Post one comment, on epic **#951**, stating:

- which of the eight you closed, and which you filed forward;
- that **#960 and #969 are hardware-gated behind #885** and your proposal for
  where they should sit;
- **what your own review filed** — that is the number the slice close needs, and
  every previous lane's inflow was discovered late.

Do not declare v0.20 complete. The milestone had 50 open issues at
`b768dc8d`, and that is the owner's call, not this lane's.
