# Lane S — the sweeper: address every remaining v0.20 issue, to exhaustion

Run this with **Opus**. Checked against `origin/main` at `27beddbb` on
2026-08-16.

**This lane is different from every other one. It has no territory.** Every other
lane owns a crate; you own a *question* — "is this issue addressed?" — over
whatever is left. That makes you the one lane that can collide with all of them,
so the claim protocol below is not optional politeness. It is the whole design.

Your job ends when **v0.20 has no open issue but its epic (#951)**.

## What "addressed" means — three outcomes, never a fourth

For each issue you take, exactly one of:

1. **Fixed** — in a PR, closing the issue.
2. **Re-milestoned**, with the reason recorded as a comment on the issue.
3. **Rejected** — the issue is wrong or no longer true. Comment with the
   evidence, then close it.

**Never leave an issue silently untouched.** If you cannot decide, comment saying
so and what you would need. That is a fourth *state*, not a fourth outcome, and
it must be visible.

## Routing — where a re-milestoned issue goes

AGENTS.md's standing rule, quoted rather than paraphrased:

> route it by the standing rule — a quick item blocking nothing to the
> rolling-debt milestone, anything unlocking only with a v1 consumer to `v1`.

The exact open milestone titles, so you do not invent one:

    v0.21 — Unity and Android on target hardware
    v0.22 — SVG as a second producer
    v0.23 — rolling quick debt
    v1 — full feature set, performance, production toolchain

- **Blocked on a device** → `v0.21`. That milestone already holds #885, #960 and
  #969 for exactly this reason.
- **Blocked on a v1 consumer, or a dependency this workspace does not have** →
  `v1`.
- **Quick, blocking nothing, and just not this slice's subject** → `v0.23`, the
  holding milestone. It has no epic and never will.
- **A better slice exists** → that slice. SVG work to `v0.22`, and so on.

**Say which of these applies in the comment.** "Moved to v0.23" with no reason is
the thing that makes a milestone unreadable at the next close.

## The claim protocol — do this before touching any issue

Nine other lanes are running. **Three of the six issues below already have a
claimant**, and taking one from under them is worse than leaving it.

Before you start an issue:

1. `gh pr list --state open --json number,headRefName,body` and grep for its
   number. If an open PR names it, **it is not yours**.
2. `git worktree list` and look for a lane in the same crate. If one exists,
   comment on the issue asking whether that lane is taking it, and move on to
   the next. Come back later.
3. If neither, **comment on the issue claiming it** — one line, so the next
   session sees it — then work it.

## Your starting list, with what I know about each

**Two of these are probably not yours. Check first.**

- **#1185** (`dashscene-gpu`) — **lane H2's stated second issue.** Its PR #1199
  claims only #1149, so #1185 may be coming in a second PR from that lane.
  `wt-lane-h2-gpu` exists. **Ask before taking.** Note #1195 looks like its
  follow-up and is also gpu.
- **#1153** (`dashscene-engine`) — **lane P is working it now**: its worktree is
  on `debt/v020-engine-baseline-probe`, which is this issue. **Not yours.**
  Verify and skip.
- **#1146** (`goldens/tooling/tests/per_frame_scaling.rs`) — recommended to lane
  L, but **not in their PR #1203** and possibly never passed on. Ask lane L; if
  they are not taking it, it is yours. It is in their file, so coordinate rather
  than assume.
- **#1168** — *the docs half of PR #1038 is still reverted on `main`.* **Take
  this one first if it is free.** #1038 touched 11 files, PR #1037 reverted 10,
  PR #1063 restored 7 — the code only. Three docs files still carry pre-#1038
  content and **one contradicts the code #1063 put back**. This is a live
  incorrect record, not a tidy-up.

  Two things the incident taught, both worth reusing here: **count the
  restoration against the revert, not against the crates you were thinking
  about**, and **compare blobs, not greps** — this repo's prose wraps at 80
  columns, so a grep for a sentence misses it.
- **#1187** — D4 and D5 have no as-built amendment, and the split-screen evidence
  has been got wrong three times in three review rounds. **Read the issue's own
  history before writing anything**; it was split out precisely because the
  underlying fact kept being restated wrongly.
- **#1190** — `docs/design/c-abi.md` duplicates the crate's module docs and
  states something. **Lane P's #1183 also edits that file** (PR #1198). Check
  #1198 before touching it.

## Then: sweep to exhaustion

After the six, work the rest of v0.20. Re-derive the list each round rather than
trusting one you took earlier — issues arrive continuously here:

    gh issue list --milestone "v0.20 — hardening: the critical findings and the Android recovery path" \
      --state open --limit 60 --json number,title

**Currently unassigned and with no lane in their territory** (checked 2026-08-16):

    #1154 #1194   demo-android — no lane owns this member at all
    #965          the capture's version check
    #996          the per-host document-ordinal conversion
    #1004         a Typesetter outliving its document
    #1067         no corpus capture pins the REST echo question
    #1070         an unreported renumbering is lost
    #979          CodeQL's rust/access-invalid-pointer, dismissed not fixed

**#979 first among those** — PR #1198's CodeQL job is failing right now and
#1198 is the C ABI PR. Plausibly the same alert. Read it before treating either
as new.

**The five oldest — #965, #996, #1004, #1067, #1070 — have survived three waves
untouched.** That is usually a sign the milestone is wrong rather than the work.
Consider `v0.23` for whichever are quick and block nothing, with the reason
recorded. Do not fix something into v0.20 just because it is listed there.

**Issues in a running lane's territory but never assigned to it** — offer them to
that lane before taking them:

    #1195           dashscene-gpu     → H2
    #1205           dashpaint         → N
    #1206           dashscene-engine  → P
    #1167 #1175 #1178 #1204   repo/justfile → K

## Watching for arrivals

New issues land continuously — every lane's `/code-review` files more. Between
rounds:

    gh issue list --milestone "v0.20 — hardening: the critical findings and the Android recovery path" \
      --state open --limit 60 --json number,createdAt,title

Anything created since your last pass is new. Apply the same three outcomes.
**Do not treat inflow as failure** — it is the review rule working. Your job is
that none of it sits unaddressed, not that none of it arrives.

**Stop when the only open issue is #951**, and say so plainly, with the count of
what you fixed, re-milestoned and rejected.

## Definition of done, per PR

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. Run the gate the crate needs: `just wasm`/`wasm-painter`/`wasm-lint` for
   wasm halves, `just android` and `just android-apk` for Android, `just check`
   for `crates/dashscene-ffi` (the `c-abi` gate), `just calibrate` if the diff
   touches the `packer` filter defined in `ci.yml`'s `changes` job.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
4. **Findings are fixed in the pull request that found them.** File one as `debt`
   only when (a) the fix cannot be made here — blocked on hardware, a missing
   dependency, a v1 consumer, or an owner ruling — or (b) it is not critical, is
   over half a day, and names no correctness defect. **Your PRs close `debt`
   issues, so under (b) you may file only a nice-to-have — a finding that names
   no defect at all.** A finding you judge incorrect is rejected on the checklist
   with the reasoning. Record fixed / rejected / filed against each item.
   (`docs/decisions/review-before-ready-not-before-open.md`.)
5. **Review every change made after the review pass.**
6. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one. **This lane writes more prose
   about issues than any other** — you will be writing sentences saying an issue
   was moved rather than fixed, and that is exactly the shape that has closed
   three issues by accident on this repo.
7. **Before merging** — `gh pr view <n> --json files`. You have no fixed
   territory, so instead check that every path is one your claimed issues
   actually need. A stray is how a merge reverts another lane.
8. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass. This has failed twice here, and the
   second repair was itself only half done — which is #1168, your first issue.
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
   the commit being merged, then `gh pr merge --merge`. **Enqueue only once `ci`
   is green** — with checks still running, `gh pr merge` silently enables
   auto-merge instead, which merges later with nobody reading the checklist.
10. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not take an issue another lane is in. The protocol above exists because you
  are the only lane that can collide with all nine.
- Do not re-milestone something merely because it is hard. The three outcomes are
  fixed, moved **with a stated blocker or a better home**, and rejected **with
  evidence**.
- Do not close the epic #951. That is the owner's call.
- Do not merge on a green `just verify` alone. It runs no test tier.
