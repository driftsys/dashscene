# v0.19 driver prompt — the debt that is left, and what is not pickable

    status  written 2026-08-15, re-derived against this branch's base
            `c29c5232`, after #925, #945
            and #946 landed. **Every count below was re-derived against the
            worktree, and four of the issues state numbers that have since
            gone stale** — each is corrected in its own section, with the
            command that re-derives it. Supersedes
            `2026-08-15-v019-DEBT-DRIVER-PROMPT.md`, archived in the same
            commit. **A driver prompt has no row in `docs/wip/README.md`**
            — captures have the table, prompts have that file's prose — so
            this commit updates those paragraphs, and the tracked count is
            unchanged because one prompt replaced another.
    scope   **written for eight issues; one remains.** This prompt was
            written on 2026-08-15 and half its scope was gone inside a
            day, which is itself the most useful thing it records about
            this milestone. Live: **#944**. Landed: #922
            (PR #1084), then #929, #930 and #932 together (PR #1099),
            then #950 (2026-08-16) — which took a story to close and
            left `docs/decisions/one-solver-per-live-scene.md` behind
            it.
            Moved out on 2026-08-16 after the "decide rather than start"
            call each section argued for: **#828 to v0.21**, where Unity
            is the second painter that can validate a portable suite, and
            **#767 to v1**, beside #476 and #462, which wait on the same
            cold-cache measurement. Their sections are kept below, struck,
            because the reasoning is the record.
    epic    #833

## Re-derive before trusting any of this

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state open --json number,title,labels

At 2026-08-16 that returns **five rows**: one `epic` (#833), three `story`
(#842, #843, #950 — relabelled from `debt`), and one `debt` (#944). It returned
twelve on 2026-08-15, and six earlier on 2026-08-16, before **#885 moved to
v0.21**. **#950 landed later the same day**, which takes it to four. Two of this
file's figures moved on 2026-08-16 for two unrelated reasons, which is the
argument for the next line rather than for any number here: re-run the query.

**The milestone's own counts include pull requests, and reading them as issue
progress overstates it.** `gh api repos/{owner}/{repo}/milestones` reports
`closed=33` for v0.19; three of those are merged PRs (#1079, #935, #928), so the
issue figure is **30 closed of 36**. Count issues explicitly:

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state all --limit 100 --json number,state

**The list decays under v0.20's work, which is running in parallel and is
loud.** On 2026-08-15 v0.20 took **53 new issues**, of which **20 were closed
the same day**; it closed **34** in total that day, and **33 of its 44 open
issues were filed** that day. Anything in `goldens/tooling/` or
`crates/dashscene-gpu/` is being edited by someone else right now. Check each
premise against the code before starting, not after — that is not advice, it is
what turned up the four stale counts below.

## What is not pickable, so nobody starts it

- **#842** (`story`) — the showcase on device with the frame-timing instrument.
  Needs an Android device.
- **#843** (`story`) — the records. This is the slice's close write-up and is
  written when the rest is done, not before.

**#885 was on this list until 2026-08-16**, when it moved to v0.21 along with
#960 and #969 — the slice was re-scoped to carry the hardware-gated Android work
beside Unity. Epic #951 records the ruling. It is not pickable for the same
reason it never was; it is simply no longer this milestone's.

**The slice cannot close without hardware for #842.** Everything in the scope
above is ordinary debt that gates nothing.

## What landed since the previous prompt, so it is not redone

- **#925** — the C ABI's mapped load. `ds_runtime_load_document_mapped` takes a
  path and a required `ShownRoot` ordinal. `first_derived_payload` and
  `show_appended_root` moved to `dashscene-core` with it. Merged as PR #1054.
- **#945** — the renumbering report. `LiveScene::take_renumbering` stamps as it
  answers; all four loops that tick a `LiveScene` read it, including
  `demo-android`'s showcase loop. Merged as PR #1069.
- **#946** — closed on two repairs and one **rejected** finding. Its amendment
  carried a false supporting sentence, corrected in a comment on the issue:
  `attach_live` ends with its own `commit_with`, which clears `renumbered`, so
  the tick-side report never fires after a load on any host.

## The one that is a story wearing a `debt` label

- **#944 — the commit's per-node scratch vectors scale with the document.** Its
  own body says it: "That is a story, not a fix inside another story." Give it a
  spec and a plan rather than a sweep commit.

  **Verified against `Txn::commit_with` in `crates/dashscene-core/src/arena.rs`:
  all eight vectors are still sized by node count.** `solved` is
  `vec![None; arena.nodes.len()]`; the other seven — `region_out_index`,
  `region_out_changed`, `mask_region`, `mask_changed`, `eff_hidden`,
  `hidden_changed`, `rect_of_slot` — are sized by a local `n`, and `n` is
  `arena.nodes.len()`. The carry-forward loop that seeds `solved` still runs
  once per node in the whole document.

  **There is a ninth vector in that same block, and it is already bounded —
  start there.** `painted_extent` is `vec![None; order.len()]`, where `order` is
  `arena.dfs_order()` and therefore covers only the shown roots' subtrees. Its
  own comment says so and cites issue #980. So the pattern this story needs
  already exists a line below `rect_of_slot`, on a vector that was converted for
  a different reason: whoever plans #944 is extending an in-tree precedent
  rather than inventing one, and should read that line before designing
  anything.

  **The band cannot see this**, and that is the part to plan for. Its two terms
  are Taffy layout computations and committed rect rows, and neither moves.
  Whichever way the vectors are fixed, the change needs a third term — an
  allocation count or a bytes-touched count — or it is unfalsifiable.

## The four that are ordinary, with what is stale in each

**#929, #930 and #932 are settled by PR #1099 (2026-08-16); #950 is not, and is
now a story.** Their sections below describe the tree as it was before that
branch and are kept for the reasoning, not as a current reading — #930's says
the memoisation is absent and #932's says the measurement has not been taken,
and both are now false. What changed:

- **#932** was measured and closed **on the measurement**: 0.31 s across the
  eleven binaries that name none of the three generators, under the threshold
  the issue set itself, so nothing was split.
- **#930**'s memoisation buys CPU and not wall clock — the redundant builds
  already overlapped on parallel test threads.
- **#929** collapsed only the two copies in binaries that already declared
  `mod common;`. `derived_bank.rs` keeps its own, measured at 0.43 s to 0.77 s
  for that binary if it joined; issue #1090 carries the rest.
- **#950** turned out not to be debt: the conversion it prescribes regresses
  `demo`'s variant-switch test, because the showcase commits through solvers
  that are not the `LiveScene`'s and a retained tree never sees those dirty
  sets. See the comment on the issue; it was re-labelled `story` and **landed on
  2026-08-16**, by staging the variant switch for `LiveScene::tick` to publish
  rather than committing it. `docs/decisions/one-solver-per-live-scene.md` is
  the record; the detector for the general hazard is issue #1104.

This leaves **#944**, **#828** and **#767** live in this prompt.

This section held five until #922 landed; its bullet is kept below, struck,
because the reasoning in the ones around it refers to the count.

- **#950 — LANDED 2026-08-16, do not start it.** The section below is kept
  because its site count is what the work was done against, and because one
  thing in it is wrong in a way worth seeing: the six sites are not one
  population. Three are the scene's own `build_live` solver and carry the whole
  saving; three are one-shot commits, and exactly one of those — the variant
  switch — stages layout intent and so was the correctness problem.

  **Premise verified and its site list is correct**, which is worth saying
  because three of the four other issues here are not. `TaffySolver::owning`
  exists in `crates/dashscene-engine/src/lib.rs`, and `ShowcaseSolver::new` has
  **six** construction sites: `corpus/showcase/src/layout.rs` three,
  `surfaces.rs` two, `typography.rs` one. Re-derive with

      grep -rc "ShowcaseSolver::new" corpus/showcase/src/*.rs | grep -v ":0"

  `| wc -l` over the whole directory gives the total and not the split, and the
  split is the part that drifts.

  Three documents name this as the shape that was wrong —
  `TaffySolver::owning`'s doc comment, the `Text` enum's, and
  `docs/decisions/measure-callback-typesetter-seam.md` — so a reader following
  any of them finds it still in use. The issue asks for a measurement rather
  than an assumption: `demo/src/shell.rs`'s frame-timing instrument reports tick
  milliseconds, and the showcase scenes are small, so the saving may not clear
  its noise. One answer in the tree beats two either way.

- **#932 — LANDED in PR #1099, do not start it.** The section below is kept
  because its corrections to the issue's counts are what the work was done
  against.

- **#932 — `goldens/tooling/tests/common/` compiles three generators into every
  binary that declares it.** **The issue says nineteen such binaries. It is
  eighteen** — of 37 test binaries in that directory, so the compile-cost
  measurement the issue asks for is scoped to 18, not to all of them.

      grep -ln "mod common;" goldens/tooling/tests/*.rs | wc -l

  `common/mod.rs` still declares three public modules — `manifest`, `many_root`
  and `stress` — under a file-level `#![allow(dead_code)]`, which is what keeps
  the cost invisible: there are no warnings to notice. The issue asks for the
  compile cost to be measured before either shape is chosen, and says plainly
  that if it is under a second the honest answer is to write the reason in
  `mod.rs` and close this. That is a real outcome, not a failure.

- **#930 — LANDED in PR #1099, do not start it.** The section below is kept
  because its corrections to the issue's counts are what the work was done
  against.

- **#930 — the many-root document is regenerated on every call.** **Both call
  counts in the issue are now low.** `many_root::document` is still uncached —
  no `LazyLock`, no `OnceLock`. But:

  - `per_frame_scaling.rs` calls its `load` helper **four** times, one at
    `extra = 0` and **three** at `EXTRA_FRAMES`. The issue says the 65-root
    document is built twice; it is three times.
  - `startup_scaling.rs` calls `document` **five** times — three at `0`, two at
    `EXTRA_FRAMES`. The issue says twice and twice.

  Re-derive with

      grep -n "load(" goldens/tooling/tests/per_frame_scaling.rs | grep -v "fn load"
      grep -nE "[^_]document\(" goldens/tooling/tests/startup_scaling.rs | grep -v "//"

  The second filter is load-bearing: a bare `grep "document("` also matches
  `validate_document(` and a comment, and prints seven lines against a true
  count of five.

  **Read the saving before spending on it, which the issue is right about.**
  There is none under nextest, which runs one process per test, so a `LazyLock`
  is never shared — and the regression tier and `just build` both run under
  nextest. The saving is real for **the two steps that run these two binaries**
  under `cargo test`, both in `.github/workflows/ci.yml`: the startup-scaling
  criterion step and the per-frame scaling step. That file runs `cargo test` in
  at least six places — the doc tests, `atlas_pipeline`, `render_oracle` and
  others — so "the two `cargo test` steps" would be wrong; these are the two
  that build this document. Whoever takes this states which of those they are
  buying.

- **#929 — LANDED in PR #1099, do not start it.** The section below is kept
  because its corrections to the issue's counts are what the work was done
  against.

- **#929 — a third `decode_png`.** **The title says byte-identical and that is
  no longer true.** There are still three, in
  `goldens/tooling/tests/derived_bank.rs`,
  `goldens/tooling/tests/perceptual_calibration.rs` and
  `goldens/tooling/tests/common/many_root.rs`. The first two are identical to
  each other; `many_root`'s **differs in two diagnostic strings** — its
  `read_info` expect message and its non-RGB `panic!` are shorter. The
  RGB-to-RGBA widening logic is the same in all three, so the issue's substance
  holds: a change to it needs three applications **inside
  `goldens/tooling/tests/`**.

  **There is a fourth application site outside that directory**, which the issue
  does not name: `crates/dashscene-gpu/src/residency.rs` has its own
  `decode_png` carrying the same `chunks_exact(3)` widening. It is not a fourth
  copy — it normalises to color8 and handles `GrayscaleAlpha` — so it is out of
  scope for a test-helper consolidation. It matters because that crate is under
  concurrent edit, and because "three" is only true of the directory. Whoever
  consolidates picks which message set survives, and **should not simply keep
  the longer ones**: they say "the canonical payload", which is right for
  `derived_bank` and `perceptual_calibration` and wrong for `many_root`, whose
  helper decodes the corpus tile images the many-root document is built from. A
  shared helper needs a message naming neither — "a PNG payload" — or the caller
  has to pass its own.

  Re-derive with

      for f in goldens/tooling/tests/derived_bank.rs \
               goldens/tooling/tests/perceptual_calibration.rs \
               goldens/tooling/tests/common/many_root.rs; do
        awk '/^fn decode_png/,/^}/' "$f" | md5
      done

  `perceptual_calibration.rs` declares `mod common;` and `derived_bank.rs` does
  not, which is why the issue says collapsing two of the three is one step and
  the third is another. That is still true.

  **It is a calibration-tier file**, so editing it means running
  `just calibrate` for a change that cannot move a table. That is the whole
  reason it was not folded into PR #928.

- **#922 — the flatc install has no integrity check. LANDED, do not start it.**
  The committed-table option was taken.
  `.github/actions/install-flatc/flatc-sha256.txt` maps the derived version to
  the sha256 of the release asset it fetches — named once, in `action.yml`,
  deliberately not repeated here — the action verifies the download against it,
  and a version with no row fails the install by name rather than fetching bytes
  nothing checks. So **bumping `flatbuffers` in `Cargo.toml` now means adding a
  row in the same commit** — that is the cost the option was chosen with, not an
  oversight.

  Two things a later reader will otherwise rediscover:

  - **GitHub's releases API exposes a per-asset `digest` field**, which the
    issue's evidence comment does not mention and which looks like a fourth
    option. It is not one. That digest comes from the same server as the asset,
    and `.github/workflows/ci.yml` already records why that buys nothing: a
    checksum fetched from the server that serves the artifact proves the
    transfer was not corrupted and nothing more.
  - **The nine-fetches/no-retry/no-cache half of the issue was not fixed** and
    is now its own issue — see the note under "Suggested order".

## The two that are not really pickable either, and why they are listed anyway

- **#828 — MOVED to v0.21 on 2026-08-16.** Unity is where a second
  implementation of R-T5's maths appears; v0.19 delivered a second _platform_
  reaching the **same** painter, since `dashscene-ffi` links `dashscene-gpu`.
  Designing a portable expectation format with no second consumer to validate it
  is how the wrong format gets designed. The original text:

- **#828 — a portable conformance suite.** Filed against v0.19 because this is
  the first slice with a second implementation. The layer-2 suite is real —
  `crates/dashscene-gpu/tests/layer2_conformance.rs` and
  `crates/dashscene-gpu/tests/shaders/conformance.wgsl` both exist — and it is
  written against `dashscene-gpu`'s own shader library, which is exactly what
  makes it unportable. Closing this means restating the probes as **data** —
  inputs, expected values, tolerances — with the existing suite becoming the
  first consumer rather than the definition.

  **This is a story-sized piece of work with no second painter to validate it
  against.** It is honest to move it to v1 rather than to do it badly here; that
  is a decision for whoever closes the slice, and it should be made deliberately
  rather than by the issue sitting still.

- **#767 — MOVED to v1 on 2026-08-16.** It sits with #476 and #462, which wait
  on the same cold-cache measurement. The original text:

- **#767 — madvise the prefetch ranges.** Unbuilt: `crates/dashbuf/src/map.rs`
  calls no `advise`, and the workspace pins `memmap2 = "0.9"`, which has
  `advise_range` behind `#[cfg(unix)]`.

  **It needs a cold-cache measurement, which is a harness and hardware
  question.** `goldens/tooling/tests/startup_scaling.rs` writes its own
  documents, so they are in the page cache the moment they exist: every fault is
  minor and `WILLNEED` against a cached file is a no-op. So this is blocked in
  practice for the same reason #885 is, even though nothing in it needs an
  Android device. Do not start it expecting to finish.

## What is left, and in what order

1. **#944** — the story. Give it its own spec and plan, and start from the ninth
   vector: `painted_extent` is already bounded by the shown-root walk, one line
   below `rect_of_slot`.

**#950 landed on 2026-08-16** and was item 1 here. It was indeed independent and
bounded, and it was not the shape-removal it reads as: the conversion is correct
only once nothing but the scene's own solver commits into its arena.
`docs/decisions/one-solver-per-live-scene.md`.

Everything else in this milestone is either hardware-gated (#842) or the close
write-up (#843).

## Traps this session hit, which cost real time

- **A `head` on a `grep` made me write a false correction.** I concluded
  `typography.rs` had no `ShowcaseSolver` because the pipe truncated before it.
  Count with `wc -l` or `grep -c` and read the whole list before asserting an
  absence.
- **A background command's reported exit code is the last command's.**
  `just build > log; echo done` notifies success whatever the build did. Write
  the status to a file and read that file.
- **`just verify`, the pre-push hook, runs no test tier.** `just build` is the
  thorough local gate; quote its `Summary` line rather than a claim.
- **A clean rebase is not a correct one.** `main` moved four times during one PR
  on 2026-08-15. Re-run the gate after the rebase, not before it.
- **Grep the issue number, not the concept, when closing something.** A
  concept-grep missed D8 of `the-shown-root-is-named-by-ordinal.md`, which named
  the very issue being closed. `grep -rn "#N" docs/ crates/` found it and two
  driver prompts in one command.
- **CodeQL's `rust/access-invalid-pointer` fires on any FFI handle round-trip in
  a test.** Dismissed twice now, as alerts 4 and 5, and tracked as #979. Do not
  spend a CI cycle rediscovering it.

## Environment

- `git push` hangs behind `git-credential-manager`. Use
  `git -c credential.helper='!gh auth git-credential' push`.
- Commit scopes are pinned in `.git-std.toml`. It is `docs(docs)`, never
  `docs(decisions)`. `goldens` covers `goldens/tooling/`, and `corpus` covers
  `corpus/showcase/`.
- **Several sessions work this repository at once.** The stash stack is shared
  across worktrees, so never `git stash` — a hook's own stash was left orphaned
  on 2026-08-15 when another session pushed one on top of it.
- Disk filled to 415 MiB free on 2026-08-15 and broke a build with
  `rustc-LLVM ERROR: IO failure ... No space left on device`. Nineteen merged
  worktrees held about 250 GiB of `target/`. `cargo clean` in a worktree whose
  branch is merged is free space.
