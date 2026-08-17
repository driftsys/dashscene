# Wave 3, lane J — dashscene-android

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16. Everything marked "the issue claims"
was not — check it yourself.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.**

You need an **Android NDK**, which `bootstrap` does not install. `just android`
is the Rust gate and `just android-apk` the Java gate. Confirm both run before
writing anything.

## Setup

    git worktree add <worktrees>/wt-lane-j-android -b debt/v020-android-wave3 origin/main
    cd <worktrees>/wt-lane-j-android
    ./bootstrap

## What you own

    #1093  machine.rs's tests carry three fixture arities where one would do
    #1094  the first attach takes up a 0x0 extent, and check_extent does not refuse one
    #1096  the DsFace descriptors in face.rs are not tied to host.rs's jni_sig! literals
    #1097  the DsFace gate is an unanchored substring match over the whole file
    #1035  the Android JNI has no mapped load, so the C ABI's bounded path has no caller

Read each with `gh issue view <n>`.

**#1096 and #1097 are the same gate and should be one PR** — both came from
PR #1095's review, both are about `face.rs`'s check against `DsFace.java`.

## Verified file map

`crates/dashscene-android/src/` holds: `face.rs`, `frames.rs`, `handshake.rs`,
`host.rs`, `lib.rs`, `logging.rs`, `loop_.rs`, **`machine.rs`** (new, from
PR #1092).

The harness Java is `DashsceneNative.java`, `DsFace.java` and
`HarnessActivity.java` under
`crates/dashscene-android/harness/java/dev/driftsys/dashscene/`.

Verified symbols:

    LoopState                     machine.rs:139
    LoopState::step               machine.rs:377
    a_zero_extent_is_never_offered_to_the_implementation
                                  machine.rs:1175
    FACE_FIELDS                   face.rs:108   ([(&CStr, &str); 6])
    read_face                     host.rs:450
    Handshake::request_teardown   handshake.rs:164
    Handshake::request_teardown_every
                                  handshake.rs:190

## #1094 crosses into another lane — read this before starting it

**`check_extent` is not in this crate.** Verified: it is
`Renderer::check_extent`, `pub(crate)`, at
**`crates/dashscene-gpu/src/render.rs:1411`**. Only *comments* in
`frames.rs:129` and `lib.rs:39` name it, which is how it reads as local.

`crates/dashscene-gpu/src/render.rs` is **lane H's file**, and lane H has seven
issues in it. So #1094 has two halves and they belong to different lanes:

- **Yours** — the first attach takes up a 0x0 extent. `machine.rs`'s `step`
  already refuses a zero extent, and `a_zero_extent_is_never_offered_to_the_implementation`
  records that, so **the first attach is bypassing a rule the machine already
  has**. That is the real defect and it is entirely in this crate.
- **Lane H's** — `check_extent` not refusing a zero extent is defence in depth
  behind your fix.

**Do the android half. Do not edit `render.rs`.** State in your PR body that the
`check_extent` half is left to lane H, and file it as its own `debt` issue on
v0.20 if lane H has already merged. Do not leave it implied.

Note also: #1094 is **pre-existing, not introduced by PR #1092** — that PR moved
the atomic read into `LoopState::start` without changing it. Do not write a PR
body blaming #1092.

## #1096 and #1097 — what PR #1095 did and did not do

PR #1095 gave the six `DsFace` field **names** one spelling: `face.rs` holds
them, `host.rs` binds each to a `const` derived from that list, so a rename on
the Rust side is a compile error.

**The types did not get the same treatment.** `FACE_FIELDS` carries a JNI
descriptor per name for the Java comparison; `host.rs` passes its own `jni_sig!`
literal at each call site; nothing relates the two.

`face.rs` **already documents both gaps in its own doc comments** — at
`face.rs:95` ("are not mechanically tied to the `jni_sig!` literals in
`host.rs`") and `face.rs:244` (which cites issue #1096 by number). Read those
before designing: the previous lane wrote down exactly what it left undone.

#1097 is the gate's matching being **unanchored** — it asks whether the
stripped, whitespace-collapsed `DsFace.java` *contains*
`public final <type> <name>;`, so the text can appear anywhere rather than as a
declaration of the `DsFace` class. The issue names two measured cases that pass
while the field is not where JNI will look. Reproduce both before fixing.

## #1035 is a feature, not a repair

`ds_runtime_load_document_mapped` maps a `.dsb` from a path and reads only the
assets the named root's subtree draws. **No host calls it.** Both
`nativeSurfaceCreated` and its `WithText` sibling take a `JByteArray` and call
`env.convert_byte_array`, so the file is read whole into the JVM heap and copied
again into a `Vec` before the ABI sees it.

This one adds a JNI entry point and touches `crates/dashscene-ffi` at its
boundary. **Run `just check` for the `c-abi` gate if you touch the ffi crate** —
it compiles the committed header from C and checks the two halves agree, and it
needs a C toolchain. Consider doing #1035 as its own PR, last: it is the only
one of the five that adds surface rather than closing a gap.

## The trap this crate keeps producing

`crates/dashscene-android/src/loop_.rs` is behind `cfg(target_os = "android")`,
so **no test tier runs it** — `just test`, `just test-regression` and the
`android-build` CI job (a compile only) all pass with the loop stalled. Issue
#940 was the third consecutive change to break that path invisibly.

**`machine.rs` exists because of that**, and it is where a decision belongs if it
can be decided without an NDK symbol. #1094's android half is exactly such a
decision — put it there, with a test, not in `loop_.rs`.

Also do not undo either of these, both pinned by mutation tests from PR #1077:

- **`waiting()` must not be called holding the state `MutexGuard`** — `Handshake`
  is public, so a reporter asking the same handshake anything deadlocks on a
  non-reentrant mutex. Putting the call back under the guard **hangs the suite**.
- **`Step::Rebuild` calls the same `Frames::attach`** — the second place this
  crate acquires a device, and it now carries the marker and the teardown guard.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just android`** (Rust) and **`just android-apk`** (Java) — both. Run
   `android-apk` for any change to the harness Java.
3. **`just check`** if you touch `crates/dashscene-ffi/`, for `c-abi`.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one. This crate's
   recent PRs returned 14 findings (#1077) and needed a fix-round review that
   found the two worst defects.
5. Fix all critical findings. **Review the fix round too.**
6. File each independent minor finding as `debt` on **v0.20**.
7. Write **`Refs #<n>`**. PR #1077's body opens "Closes nothing." — copy that
   habit when a PR advances an issue without finishing it.
8. **Before merging** — `gh pr view <n> --json files`. Any path outside
   `crates/dashscene-android/` (plus `crates/dashscene-ffi/` for #1035) is a
   stray, and a stray is how a merge reverts another lane.
9. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashscene-gpu/src/render.rs` — lane H owns it, and #1094's
  `check_extent` half lives there.
- Do not edit the `justfile`, `harness/build.sh` or `harness/assert-drew.py` —
  **lane K** owns all three and has ten issues in them.
- Do not merge on a green `just verify` alone. It runs no test tier, and for
  `loop_.rs` **no tier runs the code at all**.
