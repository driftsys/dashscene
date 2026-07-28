# Driver prompt — glyph runs become a commit output

    status  live. Story #542, then issues #274 and #275. v0.13. Written
            2026-07-29 against a measured golden-movement bound. Archive
            verbatim to docs/archive/ when the chain lands, and update
            docs/wip/README.md's count.

You are building **story #542** in `driftsys/dashscene-staging`, and the two
issues it unblocks. Read `AGENTS.md` first — its conventions override your
defaults. Then `./bootstrap`.

This is the largest deliberate pixel change in the slice. It is also fully
designed and fully measured before you start. Read both before writing code:

- `docs/decisions/glyph-runs-cross-boundary-b.md` — the decision, including
  the section "The producer story, decided".
- `docs/wip/2026-07-27-glyph-runs-from-commit-SPIKE.md` — the design, and the
  feasibility work already prototyped and verified.

## Do this first, before anything else

**Re-record `goldens/images/v011-backdrop-blur.png` as its own commit.**

It is stale on `main` (issue #538) and re-records to
`58054832d224da932fddcb9fcd8176fbbd5e1e39` on an unmodified tree — confirmed
three times from three different working trees. It moves whatever you do.

If you leave it, its 23 px land in the same commit as your six and nobody can
attribute either. Land it alone, with the hash before and after, and say it is
issue #538 rather than this story.

## The work, in order

1. **#542** — `dashscene-core`'s commit emits the glyph-run table.
2. **#275** — the painter honours a run's clip region.
3. **#274** — the painter composites a run into its group's layer.

2 and 3 are small once 1 lands. That is the whole point of the ordering: they
are painter-side symptoms of a producer that did not exist.

## What is already settled, so you do not re-derive it

- **Core stamps runs; it does not build them.** It depends only on `dashbuf`,
  `dashpaint` and `rustc-hash`, and `dashbuf`'s schema carries no glyph atlas.
  That stays true. A stager is handed to commit the way a solver already is —
  `docs/decisions/layout-solver-seam.md` established that seam shape.
- **One field, not two.** A reference to the run's rect carries clip, group
  membership and z-order together. A separate clip index mirroring
  `RectEntry::clip` was considered and rejected: it is derivable, and two
  fields can disagree.
- **The two-trait shape does not compile.** The spike prototyped it and hit
  E0499 — `TaffySolver` would implement both, and one object cannot go to two
  `&mut dyn` parameters. The corrected shape is two defaulted methods on the
  existing `LayoutSolver`. That is in the spike document and it compiles.
- **A stager must receive _this_ commit's geometry.** The spike's fake stager
  ignored its `&Arena` and so never exercised this; today's `stage_text` reads
  `arena.committed()`, which is the _previous_ front buffer.
- **11 `GlyphRun` struct-literal sites** need updating: 10 in test and golden
  code, 1 in non-test source (the goldens harness), 0 in a shipped crate's
  `src/`. Mechanical, except that a hand-built table with runs but no rects
  stops drawing — the spike hit 2 of 41 painter tests failing that way, fixed
  with one draws-nothing anchor rect each.

## The golden movement, measured

Six goldens move, and **these are ceilings, not forecasts**. The measurement
drew every run _before_ every rect — the maximum possible z-order disagreement.
The real interleave is a strict subset.

| golden                        | ceiling |
| ----------------------------- | ------: |
| `v05-text-latin.png`          | 8.400 % |
| `v07-variant-topology.png`    | 4.528 % |
| `v07-text-fallback.png`       | 4.326 % |
| `v06-text-arabic.png`         | 3.391 % |
| `v013-baseline-hug-cross.png` | 1.839 % |
| `v07-text-lowering.png`       | 1.159 % |

**27 of 33 committed PNG goldens do not move. No `.dsb` golden moves. No
oracle frame moves.**

Expect several of the six to move by **zero**: the interleave differs from
today only where a rect is drawn after a run _and overlaps it_, and text
sitting on its own background with nothing painted over it renders identically
either way.

**A frame that moves more than its ceiling is a defect, not a re-baseline.**
That is the single most useful number in this prompt — it converts every
re-baseline from a judgement into a check.

Anything outside those six moving at all is also a defect. Assert it per file
with `git hash-object` against `origin/main`, never inferred from a green
suite. Sweep with `--no-fail-fast`; without it an early failure stops the sweep
and later goldens never re-record, which reads as "nothing moved".

## What is not settled, and is yours to decide

- **The E7 oracle cannot keep its own text staging.** `render_oracle.rs` stages
  without a wrap width while the measure seam wraps at the solved width (issue
  #306). The owner has ruled: **the oracle adopts the single stager**. Read the
  ruling comment on #306 — including that fixing it by teaching `text_runs` to
  wrap on its own terms is the wrong shape, because it entrenches a second
  stager. Expect the seven E7 frames not to move, since every committed text
  fixture is HUG where the two agree; if one does move, that is #306's latent
  bug surfacing, and it is investigated rather than absorbed.
- **Per-frame cost is unmeasured.** Shaping is cached; line breaking and
  positioning are not. Land full re-staging, measure against the hero, and make
  it incremental only if the measurement says so.

## Standing rules, all earned in this slice

- **An issue can be wrong, and so can a prescribed fix.** Repeatedly here an
  issue's own text contradicted the code and the code was right; one issue
  prescribed a fix that would have left dangling indices reaching a painter.
  Evaluate the fix, not just the problem.
- **Mutation-test every check.** Break what it should catch, confirm a _named_
  test goes red. **A mutation that stays green is the finding** — work out why
  before patching, because the answer is often that the fixture cannot express
  the difference. Record every mutation, green ones included.
- **Review inline. Do not spawn subagents to review.** Never wait on a
  notification from a command you started — run it in the foreground and read
  its exit code. Both stalls happened.
- `just verify` must exit 0. CI cannot run (billing, issue #263).

## Workflow

Branch per step, so #542, #275 and #274 stay attributable. Rebase onto
`origin/main` before each PR — it moves constantly. **Never**
`git reset --soft origin/main`; it silently reverts anything that landed in
between and `just verify` still passes, because a revert is self-consistent.
Check `git diff --name-only origin/main HEAD` before pushing.

Squash each branch to one commit. Conventional commit, scope mandatory and
validated. Amend with `--no-verify`. Draft PR, `/code-review`, findings as a
checklist, critical fixed and minor filed as one `debt` issue each, ready only
after review, merge with a merge commit.

End each PR body with `Closes #N` for the issue it actually completes. **Never
write "closes", "fixes" or "resolves" followed by a number anywhere else,
including mid-sentence** — GitHub acts on a closing keyword wherever it
appears, and a story was closed by accident exactly that way. Equally, never
write only `Refs #N` for an issue you did resolve; two were left open that way.
And do not put a closing keyword on an issue whose second half is outstanding —
that happened too, and the residual lost its home.

Prose in plain, literal English. No idioms.
