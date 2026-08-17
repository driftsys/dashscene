# Wave 3, lane I — dashc

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16. Everything marked "the issue claims"
was not — check it yourself.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.**

## Setup

    git worktree add <worktrees>/wt-lane-i-dashc -b debt/v020-dashc-wave3 origin/main
    cd <worktrees>/wt-lane-i-dashc
    ./bootstrap

## What you own

Five issues, all filed by the `/code-review` fan-out on PR #1039:

    #1064  a CHANGE_TO on any layer but an instance root contributes no transition
    #1065  a nested INSTANCE switching its parent's variant resolves against its own set
    #1047  a mirrored relativeTransform lowers upright with no diagnostic
    #1056  one authored reaction inside a master is reported once per instance
    #1066  variants::apply rescans per fixed-point round and reads interactions twice

Read each with `gh issue view <n>`.

**#1064 and #1065 are one seam.** Both are about *which set a `CHANGE_TO`
resolves against*: #1064 says only the instance root's own switches reach the
transition table, #1065 says a nested instance resolves against the wrong set.
Decide the resolution rule once and both follow; fixing either alone will
probably move the defect. Say in the PR body which of the two you consider
load-bearing.

## Verified symbol map — `crates/dashc/src/figma/`

    variants::apply               variants.rs:137
    Walked                        variants.rs:726   (a struct, new since PR #1039)
    paths(file) -> Vec<Walked>    variants.rs:806
    Walked::of                    variants.rs:910
    Plan                          variants.rs:1068
    Plan::of                      variants.rs:1091  ← two `fn of` in this file;
    Plan::emit                    variants.rs:1316     do not confuse them
    is_definition                 variants.rs:493
    interaction_diagnostics       variants.rs:534
    differs_beyond_overrides      variants.rs:1513
    prototype::read               prototype.rs:94
    read_action                   prototype.rs:141
    rest::Node::turn              rest.rs:374
    matrix_turn                   rest.rs:423
    rest::Node::component_id      rest.rs:341

**`shown` is a local, not a function.** It is built inside `apply` at
`variants.rs:250` from `walked.node.component_id`, beside `switchable`
(`variants.rs:245`). Do not go looking for `fn shown`.

**PR #1039 restructured this file heavily** — `paths` now returns
`Vec<Walked<'_>>` where it returned `Vec<(&Node, String)>` a day ago. Any line
number in an issue body older than that PR is stale. Cite symbols.

## What PR #1039 cost, and what that means for you

That PR needed **seven `/code-review` passes and produced 75 findings**, eleven
of which were defects its own fix rounds introduced. The recorded lesson from it:

> When each round finds another **case** of your new general rule, the defect is
> the **generality**. Two rounds contradicting each other is the stop signal.

Your five issues are the residue of that work. Three of them (#1064, #1065,
#1056) are about the same resolution/reporting pass. **If your rounds start
finding fresh cases of a rule you just wrote, stop and narrow the rule rather
than adding another case.**

## Per-issue notes, verified where marked

- **#1064** — `Plan::emit` applies an instance's own reactions from `own`, which
  `apply` passes as `prototype::read(instance).switches` — **the instance root
  only**. A `CHANGE_TO` on any other layer contributes no transition. Pre-existing;
  PR #1039 made the loss visible by reporting it as having lost nothing.
- **#1065** — a `CHANGE_TO` resolves against the set of the nearest node that
  shows one. Right for a layer switching its own instance's variant; wrong for a
  **nested instance switching its parent's**. Pre-existing — the pre-#1039 code
  resolved from `node.component_id` alone and had the same blind spot.
- **#1047** — `matrix_turn` (`rest.rs:423`) reads a negative determinant as
  `0.0` **deliberately**: the document has no vocabulary for a mirror, and
  reporting the half-turn `atan2` "would draw a new wrong picture rather than
  repair one". That is
  `docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md` and issue
  #878. **The defect is the missing diagnostic, not the zero.** If your fix
  changes what that record says, edit the record in the same PR.
- **#1056** — Figma echoes a component's interaction onto every instance, so
  one authored reaction becomes one finding per instance. Measured on PR #1039's
  head. Read what "measured" meant there before re-measuring.
- **#1066** — two redundancies, not wrong answers: the reachability fixed point
  rescans the whole file each round, and each member's interactions are read
  twice. Both in the pass PR #1039 restructured. **Do not let a performance fix
  change a diagnostic's output** — if it does, that is a behaviour change and
  needs its own justification.

## What to measure rather than argue

- **#1065's population is a nested INSTANCE inside another instance** — two
  levels, not one. A fixture with a single instance proves nothing.
- **#1056 needs a master with one authored reaction and two instances.** Count
  the findings; do not infer the count from the code path.
- **#1066 is a claim about redundant work.** Show the rescan count before and
  after, and confirm the diagnostics are byte-identical either side.

`crates/dashc/tests/` holds `prototype_lowering.rs`, `figma_lowering.rs`,
`component_lowering.rs`, `bindings_lowering.rs`, `text_lowering.rs`,
`flex_lowering.rs`, `round_trip.rs`, `abi.rs`, `asset_table.rs`,
`image_id_gate.rs`, `vector_field_weld.rs`, and a shared `common/mod.rs`. Add to
one of those rather than inventing another. **`prototype_lowering.rs` is yours** —
an earlier prompt carried a stale ban on it, copied from an expired block.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just wasm`** — `dashc` builds to `wasm32-unknown-unknown` for the Deno
   importer, and that is the only gate that sees the wasm half.
3. If your diff reaches `importers/figma/`, run `just deno-check` and
   `just deno-test`.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
5. Fix all critical findings. **Review the fix round too**, and heed the stop
   signal above.
6. File each independent minor finding as `debt` on **v0.20**.
7. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
8. **Before merging** — `gh pr view <n> --json files`. Any path outside
   `crates/dashc/` is a stray, and a stray is how a merge reverts another lane.
9. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not change a diagnostic's severity without reading what `docs/specification/`
  says the policy is. P4 is "every out-of-profile construct is a named
  diagnostic, never a silent drop" — but "named" and "withholds the document
  under `Strict`" are different answers.
- Do not edit `crates/dashscene-validator/` — **lane L** owns it, and a new
  validator rule can change what `dashc` emits. If one of your tests moves
  because of their change, say so in the PR rather than fixing it silently.
- Do not merge on a green `just verify` alone. It runs no test tier.
