# Wave 3, lane L — the validator registry and the goldens harness

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.**

## Setup

    git worktree add <worktrees>/wt-lane-l-validator -b debt/v020-validator-goldens origin/main
    cd <worktrees>/wt-lane-l-validator
    ./bootstrap

## What you own

    #1042  the rule registry test walks Construct::ALL, so a document-gate rule is invisible
    #1048  nothing refuses a non-finite RectEntry origin
    #1021  a degenerate coverage field is dropped with no diagnostic (added 2026-08-16)
    #997   two more baked-payload copies of the shape #967 corrected
    #1015  the per-frame band carries no text

## #1021 was handed to you by lane H (added 2026-08-16)

It is on nobody's original list. Lane H filed the ruling on it and could not act:
the diagnostic belongs in `dashscene-validator`, which is **your** crate.

**The painter half is already done** — `a_degenerate_coverage_field_draws_nothing`
pins that such a field draws nothing on both consumers, and both painters now
take one predicate, `dashpaint::VectorField::draws`. What is missing is that the
drop is **named**: a refused payload is recorded on `Renderer::refusals`, and a
degenerate field reaches no diagnostic at any seam, which is what P4 forbids.

Two things lane H established that save you the investigation:

- **`Renderer::refusals` is the wrong home**, even inside the painter. A
  `Refusal` carries a `ResidencyError`, and every arm of that type is about what
  residency could not do — but a degenerate field never reaches residency at all,
  because `resolve_frame` short-circuits on the predicate before
  `resident_image`. Recording one there needs an arm that is not a residency
  error.
- **The predicate to mirror is `VectorField::draws`**, not a restatement of it.
  It rejects a quad whose width or height is not finite and positive, and an
  atlas rectangle with no texels. Since issue #1144 it is one method both
  painters call, so a validator rule that disagrees with it is a third copy —
  which is the mechanism #1000, #1034 and #1144 were each filed for.

**It pairs naturally with #1048**, which is also about a rule that names a
non-finite value crossing boundary B, and which raises the same "which gate does
it go in" question — `validate_scene` has no production caller. Answer that once
for both.

**#1042 must land before #1048.** #1042 is the reason a missing validator rule
goes unnoticed; #1048 adds a rule. Fix the pin first and the new rule is actually
held by it. The other order ships a rule nothing checks — which is the exact
history #1042 records: three rules had been absent since story B1 landed.

## Verified symbol map

    rule::ALL                     crates/dashscene-validator/src/lib.rs:415
    rule::is_known                crates/dashscene-validator/src/lib.rs:509
    check_rect_extent             crates/dashscene-validator/src/scene.rs:222
    validate_scene                crates/dashscene-validator/src/scene.rs:45
    validate_document             crates/dashscene-validator/src/document.rs:90

The test #1042 names is
`the_rule_registry_is_unique_and_covers_every_construct` in
`crates/dashscene-validator/tests/triage.rs`.

**`src/lib.rs:480` already carries a comment saying that test "cannot catch"
something** — read it before designing. The gap is partly documented in the code
that has it.

## #1048 — decide where the rule goes before writing it

The issue is accurate: `check_rect_extent` refuses a `RectEntry` whose `w` or `h`
is out of domain and **does not look at `x` or `y`**, so a `RectEntry` with
`x: f32::NAN` crosses boundary B unnamed.

**But it is a `validate_scene` rule, and `validate_scene` has no production
caller.** Verified: its only callers are in
`crates/dashscene-validator/tests/scene.rs`. `validate_document` is the one with
real callers — `crates/dashc/src/lib.rs`, `crates/dashc/src/main.rs`,
`crates/dashscene-desktop/src/document.rs`, `crates/dashscene-desktop/src/lib.rs`
and `crates/dashscene-ffi/src/lib.rs`.

So adding the check beside `check_rect_extent` puts it in a gate nothing runs.
`docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md` records that
fact and is the governing record here. **Decide explicitly** whether the rule
belongs in the scene gate anyway (consistent, but inert), in the document gate
(fires, but a different entry point), or at the `dashpaint` table seam that
record established — and say which, and why, in the PR body. If you cannot
decide, say so and stop; do not pick silently.

## #997 and #1015 are the goldens half, and are independent of the validator half

`goldens/tooling/tests/` has a shared `common/` module already — `mod.rs`,
`manifest.rs`, `many_root.rs`, `stress.rs`. Use it rather than adding another
copy of anything.

- **#997** — two more sites of the shape issue #967 corrected: a payload copied
  into an `ImageAsset` while the source is dropped at the end of the same
  expression. #967 was scoped to `load_atlas` rather than to the pattern, which
  is why these two were missed. **Grep for the pattern, not for `load_atlas`**,
  and say in the PR how many sites your grep reached.
- **#1015** — `goldens/tooling/tests/per_frame_scaling.rs` is the repository's
  per-frame criterion, and its terms are **counts, not time**
  (`TaffySolver::solves()` and the committed rect-row count), deliberately, per
  `docs/decisions/startup-scaling-is-measured-by-a-counter.md`. The band carries
  no text, so nothing weighs the glyph-run path per frame — which matters because
  PR #1005 added a per-quad pass to `GlyphRunTable::push_run` on the commit path
  with no instrument to measure it.

  **Read that decision record before adding a term.** Changing what the band
  measures changes what the record says it measures.

## What to measure rather than argue

- **#1042** — write the failing test first. A registry pin that passes before
  your change is not a pin. Remove a rule from `ALL` and confirm the new test
  fails; that is the falsification.
- **#1048** — build the `RectEntry` with `x: f32::NAN` and show what each painter
  then computes. "Both painters divide by geometry derived from it" is the
  issue's claim, not a measurement.
- **#1015** — a band term you add must move when the thing it measures moves.
  Add the term, then mutate the glyph-run path and confirm the number changes.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just calibrate` if your diff touches any path in the `packer` filter.** The
   filter is defined in the `changes` job of `.github/workflows/ci.yml` and
   enumerated with a reason per entry in `docs/decisions/test-tiers.md`. Read it
   there — that list has drifted three times as a partial copy. Goldens work
   often lands in it.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
4. Fix all critical findings. **Review the fix round too** — #1048 itself was
   found by the review of PR #1038's fix round.
5. File each independent minor finding as `debt` on **v0.20**.
6. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
7. **Before merging** — `gh pr view <n> --json files`. Your paths are
   `crates/dashscene-validator/` and `goldens/tooling/`. Anything else is a
   stray, and a stray is how a merge reverts another lane.
8. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
   the commit being merged, then `gh pr merge --merge`.
10. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashc/` — **lane I** owns it. A new validator rule can
  change what `dashc` emits under `EmitPolicy::Strict`; if one of their tests
  moves because of your rule, say so in the PR rather than editing their file.
- Do not edit `crates/dashpaint/src/lib.rs` — **lane N** owns it in Phase 2, even
  if #1048's answer points at the table seam. File that instead and say so.
- Do not merge on a green `just verify` alone. It runs no test tier.
