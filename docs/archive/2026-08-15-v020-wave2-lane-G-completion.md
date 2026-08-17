# Lane G — continuation to completion

Checked against `origin/main` at `94454e36` on 2026-08-15. This supersedes the
"What is honestly blocked" section of `lane-G-android-ffi.md`; everything else in
that file still stands.

## Where you are

**2 of 6 done.** PR #1051 closed #940 and #888. Four remain, and **all four are
startable now** — nothing is held any more:

    #960  surfaceDestroyed enters the handshake and never returns
    #969  the harness does not exercise the text-carrying entry point
    #884  the C ABI cannot say which frame failures are recoverable
    #981  the JNI text entry point cannot name a face inside a collection

Your worktree is still on `debt/v020-android-recovery`, which has already merged.
**Cut a fresh branch from current `main`** before doing anything.

## What changed under you since the original prompt

**PR #1032 merged at 10:50** (`fix/v020-harness-handshake-markers`). It edited
`crates/dashscene-android/harness/java/dev/driftsys/dashscene/HarnessActivity.java`
and the `justfile`, and it moved the handshake completion marker **inside** the
guard that does the work. Your hold on #960 and #969 is released.

**This matters for #960 specifically.** Its entire evidence is those markers —
"entering the handshake" logged, "handshake complete, returning" never logged,
150 s later. #1032 changed the code that observation was made against. The two
point in opposite directions (#1032 is a marker printed when no handshake ran;
#960 is a handshake that entered and never returned), so #1032 is **not**
obviously a fix. **Re-derive the symptom on current `main` before diagnosing it.
Do not carry the old logcat forward as evidence.**

**#1030 is CLOSED** (PR #1053), so the original prompt's statement that nothing
compiles the harness Java is **no longer true**. There is now a gate:

    just android-apk        # depends on `android`; packages both Android APKs
                            # and is the only thing that compiles their Java

Run it for any change to `HarnessActivity.java` or `DashsceneNative.java`. The
original prompt told you to compile by hand and note that CI did not — ignore
that; the gate exists and CI schedules it.

## #884 and #981 were never blocked

The original prompt put them under a heading called "What is honestly blocked,
and what is not", and quoted each issue's own hedge — #884 is *"worth doing when
a second host needs it"*, #981 says *"this is not blocking"*. That was meant as
context, not as permission to skip them. **Both are in scope for this lane.**

Both are design decisions rather than repairs, so the definition of done for each
includes a recorded answer:

- **#884** — the proposal is a `DsStatus::SurfaceLost` tail variant, additive
  under the ABI's versioning rule so `DS_ABI_VERSION` does not move, and a host
  built against an older header sees an unknown status and stops, which is the
  safe side. If you take it, the Rust half and the committed header
  `crates/dashscene-ffi/include/dashscene.h` must agree — `just check` runs a
  `c-abi` gate that compiles the header from C and checks exactly that. **Needs a
  C toolchain.**
- **#981** — the issue rules out the obvious fix: **not** a sixth parallel array.
  Five already have to agree in length and the mismatch check is a log line and a
  zero handle. The alternatives are a Kotlin/Java descriptor class read field by
  field, or a byte-packed descriptor block the native side parses. Weigh both.

**If you cannot decide either one, say so in the PR and stop — do not pick
silently.** That is a legitimate outcome and it is better than a guess.

## What stays honestly out of reach

- **#885 is still OPEN** — the Vulkan measurement on target hardware has not been
  taken and there is no device. So **#969 cannot be closed on a device run.** Do
  the work, run it on an emulator, and say exactly what ran and on what. Do not
  claim a hardware measurement.
- **#969 needs committed assets** — a font file and a committed MSDF sheet (a PNG
  and its metrics blob) read out of the APK. Nothing bakes a sheet at run time,
  which is why the story that added the entry point did not do this. Decide where
  those bytes live before writing Java.
- **#872** (every surface cycle rebuilds the whole runtime and blocks the UI
  thread for about a second) is the same seam as #960, sits on `v1`, and is a
  worse symptom. Whichever you touch, check it against the other.

## Suggested order

1. **#960** — re-derive first. It is the only live defect of the four, and what
   you find may change #969's harness work.
2. **#969** — the harness path, once #960's diagnosis is settled.
3. **#884** and **#981** — the two ABI/JNI design questions, in either order.

Group them into PRs however the diffs fall; say in each body which issues it
closes and which it does not.

## New: the merge-revert check

**A merge can revert a file it never edited, and CI stays green.** This has now
happened twice on this repo:

- PR #1037 reverted PR #1038 across seven `dashpaint`, `dashscene-skia` and
  `dashscene-validator` files. `main` was missing work that four issues read as
  `CLOSED` for 90 minutes (restored by PR #1063).
- PR #978 dropped PR #961's `android-splitscreen` recipe the same way (#991).

The older content still compiles and still passes its own tests, because the
reverted lane's tests went with it. So:

- **Before merging** — `gh pr view <n> --json files`. Any path outside
  `crates/dashscene-android/` and `crates/dashscene-ffi/` is a stray. In both
  incidents the stray paths were plainly visible in that list.
- **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
  PR's files>`. An empty diff is the pass.

## Definition of done

Unchanged from `lane-G-android-ffi.md`, with the gates restated because they have
moved:

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line.
2. **`just android`** — the Rust compile gate for the four Android members.
3. **`just android-apk`** — the Java gate, new since PR #1053. Run it for any
   harness Java change.
4. **`just check`** if you touch `crates/dashscene-ffi/` at all, for `c-abi`.
5. `just verify` may fail on the secrets gate for reasons that are not yours —
   worktrees share one object store. Issue #987 is about that gate.
6. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` on it
   **while CI runs, not after**. Capture every finding as a checklist in the PR
   description; never drop one silently.
7. Fix all critical findings; file each minor one as its own `debt` issue on the
   **v0.20** milestone. **When a critical finding changes the implementation,
   review the fix too.**
8. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green
   **on the commit you are merging**, then `gh pr merge --merge`. Merging is
   serial: `main` needs an up-to-date branch and auto-merge is off.
10. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not merge on a green `just verify` alone. It runs no test tier, and for
  `crates/dashscene-android/src/loop_.rs` **no tier runs the loop at all** — it is
  behind `cfg(target_os = "android")`. That is why #940 was the third consecutive
  change to break that path.
- Do not touch `crates/dashscene-gpu/src/render.rs`, `crates/dashpaint/src/lib.rs`
  or `crates/dashscene-skia/src/lib.rs` — PR #1073 is open across all three.
