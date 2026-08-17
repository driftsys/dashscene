# Lane I2 — dashc, the inflow after lane I closed

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `291fbcbc` on 2026-08-16.

**Why this lane exists:** lane I finished and `crates/dashc` is free. Both issues
below arrived from PR #1135's review, after that lane closed. No other running
lane touches this crate.

## Setup

    git worktree add <worktrees>/wt-lane-i2-dashc -b debt/v020-dashc-inflow origin/main
    cd <worktrees>/wt-lane-i2-dashc
    ./bootstrap

## What you own

    #1137  the echoed-finding collapse keys on the rendered message
    #1142  the variant pass walks each switch's host chain two or three times

Both from the `/code-review` fan-out on PR #1135. **#1137 reviewed that PR's own
fix for debt #1056** — so it is a defect in a fix, which is the category this
repository's workflow now says is fixed rather than filed. It was filed because
that PR had already merged.

## Verified symbol map — `crates/dashc/src/figma/variants.rs`

    report                    variants.rs:709
    interaction_diagnostics   variants.rs:917
    hosts_of                  variants.rs:1249
    belongs_to                variants.rs:1301

## Two symbols in #1142 that will waste your time — verified

**`table_host_of` does not exist on `main`.** I grepped the whole of `crates/`;
there is no such symbol. The issue names it as a place the host chain is
re-walked. Either it was renamed before PR #1135 merged, or the issue describes
an intermediate state of that branch. **Find the actual second walk before
accepting the issue's count of "two or three times."**

**`landing` is not a function either.** It is a local binding — the destructuring
at `variants.rs:935`, `for (switch, (landing, reach)) in read.switches.iter().zip(resolved)`.
The issue says the chain is walked "inside `landing`", which reads as a call. It
is not one.

This is the standard failure here: an issue names a mechanism, and the mechanism
is the part that is wrong. Trace the consequence — how many times a host chain is
actually walked for one switch — rather than trusting the two names.

## #1137 — what the collapse actually keys on

`figma::variants::report` (`variants.rs:709`) collapses copies of one authored
reaction, keyed on `(authored_source, rule, message)` where `message` is the
**fully rendered string**, and the survivor's message is then mutated by
`push_str` to carry the copy count.

Every message `interaction_diagnostics` (`variants.rs:917`) writes today happens
to contain only the destination id, which is identical across copies — so the
collapse works **by coincidence**. The moment any message carries a node-specific
token, the key stops matching and the collapse silently stops collapsing.

**The defect is that nothing holds the property in place**, not that anything is
broken now. So:

- A fix that changes the key must keep today's output byte-identical. Prove that,
  do not assert it.
- A test that only checks today's messages cannot fail. The falsification is a
  message with a node-specific token — write one and confirm the collapse still
  works, or that it fails loudly rather than silently.

## #1142 is redundant work, not a wrong answer

The issue says so itself, and it is the same shape as #1066 which lane I closed.
**Do not let a performance fix change a diagnostic's output.** If it does, that
is a behaviour change and needs its own justification, its own test, and a look
at whether any decision record describes the old behaviour.

Show the walk count before and after, and confirm the diagnostics are
byte-identical either side.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just wasm`** — `dashc` builds to `wasm32-unknown-unknown` for the Deno
   importer, and that is the only gate that sees the wasm half.
3. If your diff reaches `importers/figma/`, run `just deno-check` and
   `just deno-test`.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one. This crate's
   PR #1039 needed **seven passes and produced 75 findings**, eleven of them
   defects its own fix rounds introduced. If your rounds start finding fresh
   cases of a rule you just wrote, **the defect is the generality** — narrow the
   rule rather than adding another case.
5. **The finding-triage rule changed on 2026-08-16 — do not use the old one.**
   Findings are **fixed in the pull request that found them**. File one as `debt`
   only when (a) the fix cannot be made here — blocked on hardware, on a missing
   dependency, on a v1 consumer, or on an owner ruling — or (b) it is not
   critical, is over half a day, and names no correctness defect. **This PR
   closes `debt` issues, so under (b) you may file only a nice-to-have — a
   finding that names no defect at all.** A finding you judge incorrect is
   rejected on the checklist with the reasoning. Record fixed / rejected / filed
   against each item.
   (`docs/decisions/review-before-ready-not-before-open.md`.)
6. **Review every change made after the review pass.**
7. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
8. **Before merging** — `gh pr view <n> --json files`. Anything outside
   `crates/dashc/` is a stray.
9. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`. **Enqueue only once `ci`
    is green** — with checks still running, `gh pr merge` silently enables
    auto-merge instead.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashscene-validator/` — **lane L** owns it, and a validator
  rule change can move your tests. If one moves because of theirs, say so rather
  than fixing it silently.
- Do not merge on a green `just verify` alone. It runs no test tier.
