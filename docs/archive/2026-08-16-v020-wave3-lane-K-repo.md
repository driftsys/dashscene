# Wave 3, lane K — the justfile and the Android harness scripts

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.**

This is the biggest lane and **the only one that is serial inside itself**: ten
issues through one `justfile` and two shell scripts. It is the wave's long pole,
so start it first among the Phase 1 lanes.

You need an **Android NDK** for `just android`/`android-lint`, and an emulator
for anything that actually exercises `android-splitscreen`. Say what you could
not run.

## Setup

    git worktree add <worktrees>/wt-lane-k-repo -b debt/v020-repo-wave3 origin/main
    cd <worktrees>/wt-lane-k-repo
    ./bootstrap

## #1101 is your first PR, and it is not optional

**The NDK toolchain wiring is written three times.** Verified — the same
`export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` line appears at
**`justfile:867`, `:923` and `:1013`**, inside three recipes:

    justfile:826   _android-ndk-bin      the shared lookup that already exists
    justfile:862   android               wiring copy 1  (line 867)
    justfile:915   android-lint          wiring copy 2  (line 923) — added by PR #1098
    justfile:982   android-apk
    justfile:1008  android-probe         wiring copy 3  (line 1013)
    justfile:1107  android-splitscreen

Each copy is five lines: `bin=$(just _android-ndk-bin)`, the `clang` path, and
three exports. Raising `ANDROID_API`, changing the linker name, or adding
`RANLIB_aarch64_linux_android` needs three edits today, and a partial edit fails
only on whichever recipe was missed.

`_android-ndk-bin` is the file's own precedent for a shared private recipe.
**Consolidate first**, then every other issue below rebases onto one wiring
instead of three.

## The rest, in the order that keeps the hunks apart

**Group 2 — the `android-splitscreen` recipe** (`justfile:1107`):

    #1007  duplicates android-probe's adb discovery instead of sharing a private recipe
    #1008  `just --list` renders a sentence fragment as its description
    #1006  the shell defects, including a false PASS the justfile cannot fix alone

Do #1007 and #1008 with #1101 in mind — #1007 is the same "share a private
recipe" move, so the two consolidations may want the same shape.

**#1006 is the substantial one.** Its own title says the false PASS *the justfile
cannot fix alone*: `am force-stop` is followed immediately by `adb logcat -c`, and
destroying the previous instance logs both handshake markers, so entries written
in the millisecond window are attributed to the new run. PR #1032 already fixed
the Java half of the marker problem — read what it changed before designing this.

**Group 3 — `assert-drew.py`** (`crates/dashscene-android/harness/assert-drew.py`):

    #1029  passes a fully black frame, and it is the only witness that the painter drew
    #1100  counts colours, not glyphs, so the text fixture's own contribution is unasserted

**One PR.** #1100 was split out of #1081, and PR #1098 already settled the other
half of #1081 — the docstring named a fixture the harness no longer ships, and
`MIN_DISTINCT` stays at 16 with the measurement recorded. So do not re-open that;
#1100 is only the glyph-contribution half.

**Group 4 — the APK gate** (`justfile:982` plus
`crates/dashscene-android/harness/build.sh` and `demo-android/android/build.sh`):

    #1057  packages a stale release library while rebuilding the debug one
    #1058  the gate's determinism and hygiene
    #1062  the dex step assumes one d8 batch and one classes.dex

All three came from PR #1053's review. **#1062 came from the review of that PR's
own fix round** — PR #1053 made the compiled file set unbounded and the dex step
still assumes one `d8` invocation. #1058 is explicitly a group of several
defects, all predating PR #1053 except the first, which it introduced.

**Group 5 — `just secrets`:**

    #987  the history gate adjudicates only what .gitleaksignore leaves

Measured, not reasoned: with `.gitleaksignore` present the history scan finds 63
findings / 31 distinct pairs; without it, 101 / 54. gitleaks matches a bare
`<file>:<rule>:<line>` in git mode, so a fingerprint silences that path and line
in **every commit that carries it**. This one is independent of the Android work
and could go first or last within the lane.

## Why this lane keeps producing issues

Every Android-toolchain change so far has filed more than it closed: PR #1003
produced #1006, #1007, #1008 and #1029; PR #1053 produced #1057, #1058 and
#1062; PR #1098 produced #1100 and #1101. The recurring cause is that these
recipes cannot be exercised without a device, so defects are found by reading
rather than running.

**Where you can make something runnable, do.** A recipe that only a reviewer's
eyes check will be back.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just android` and `just android-apk`** for anything touching those recipes
   or either `build.sh`. `just android-lint` too, since #1101 rewires it.
3. **`just --list`** after any comment change — that is #1008's actual symptom
   and the only way to see it.
4. `just secrets` for #987. **Note it may fail for reasons that are not yours** —
   worktrees share one object store, so the scan sees every unpushed commit on
   this machine. Check what it names before assuming your change is at fault.
5. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
6. Fix all critical findings. **Review the fix round too** — #1062 exists because
   that pass caught what PR #1053's first fix broke.
7. File each independent minor finding as `debt` on **v0.20**.
8. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
9. **Before merging** — `gh pr view <n> --json files`. Your paths are `justfile`,
   `crates/dashscene-android/harness/*.sh`, `harness/assert-drew.py`,
   `demo-android/android/build.sh` and `.github/workflows/ci.yml`. Anything else
   is a stray, and a stray is how a merge reverts another lane: PR #1037 reverted
   PR #1038 across seven files it never edited, and PR #978 dropped PR #961's
   `android-splitscreen` recipe the same way (#991).
10. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
    PR's files>`; an empty diff is the pass. **This lane has the repo's only two
    recorded instances of that failure**, both in the `justfile`. Do it every time.
11. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`.
12. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashscene-android/src/` — **lane J** owns it and has five
  issues there. Your half of that crate is `harness/` only.
- Do not edit `crates/dashscene-gpu/` — lane H.
- Do not merge on a green `just verify` alone. It runs no test tier.
