# Finishing story #771 — one open finding, then merge

    status   live; hand this to a session as its first message. It is narrower
             than `2026-08-09-v018-DRIVER-PROMPT.md`, which is still the guide
             for the rest of the slice — that prompt says "story #771 is what
             to build next", and this is what remains of it.
    written  2026-08-09, after pull request #865 was opened, reviewed by the
             fan-out, and ten of its eleven findings fixed. Everything below
             was checked against the branch at c86386c.
    empties  when #865 merges. Delete it in the same commit that removes the
             branch, and edit `docs/wip/README.md` with it.
    archived 2026-08-09, in #865 itself — the pull request it carries — so the
             move and the ledger edit are the one commit it asks for. The body
             below is left unedited by design. Two things it states turned out
             to be narrower than what was found, and the pull request records
             both: the finding is not that the switch snaps to its destination
             but that the switch is dropped from that commit altogether, and
             `LayoutSolver` has two further methods whose defaults a wrapper
             silently inherits.

Read `AGENTS.md` first. It holds the story workflow, the test tiers, the merge
method and the five principles, and it is authoritative over anything here.

## Where to work

The branch is `story/771-motion-rows`, pushed, with its worktree already at
`<worktrees>/wt-771-motion-rows`. Run
`./bootstrap` there if the hooks are not installed. Do not start a new
worktree — this one is clean and matches what is pushed.

## What is already done

Pull request #865 builds story #771 in the three parts issue #617's ruling
set out, and closes #617 by name. The epic's definition-of-done line is met:
a `.dsb` carries a transition and a document loaded from a file animates
through the ordinary frame loop, proved by
`a_document_loaded_from_a_file_animates_through_the_frame_loop` in
`goldens/tooling/tests/loaded_variant_flip.rs`.

`just build` is green at **1699 tests**. Ten of the eleven review findings are
fixed, each mutation-tested. The checklist in the pull request description is
the record — read it, do not re-derive it.

**`cargo audit` fails**, on a corrupted upstream advisory database
(`duplicate advisory ID: RUSTSEC-2026-0244`). It is not this branch: untouched
`main` at 404d71d fails identically and this diff adds no dependency. The
pre-push hook runs `just verify`, which runs it, so pushing needs
`--no-verify` after the regression tier and lint are green. Say so on the
pull request rather than pushing silently.

## The one open finding, and the fix

**A variant switch plus a layout-dirty write in the same tick snaps to the
final position, then rewinds and re-animates.** `layout_dirty` takes priority
over the switch branch in `LiveScene::tick`, so this frame's sample is
discarded while the track stays live.

Two attempts failed, each on a different invariant nothing had written down.
Both are recorded on the pull request as a comment; the short form:

- **Do not move the solve before the drain.** A _smoothed_ binding on a
  solve-class channel — the showcase's spring-driven gap — only sets
  `layout_dirty` when its sample is applied, in the scheduler drain. A solve
  placed earlier never sees it and the gap stops re-solving.
- **Do not route a layout-dirty tick through the cached replay.** Glyph runs
  are produced by the commit and only when the real solver runs inside it;
  `corpus/showcase/tests/badge.rs` states this in its own comment, and its
  `announcing_a_painter_adds_exactly_one_glyph_run_to_every_scene` is what
  fails. The scene loses every run it had.

**So do not move the solve at all — wrap it.** A `LayoutSolver` that defers
to the injected one and overlays the frame's samples on its output:

    struct FlipOverlay<'a> {
        inner: &'a mut dyn LayoutSolver,
        samples: &'a [(NodeId, Patch)],
    }

    impl LayoutSolver for FlipOverlay<'_> {
        fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
            let mut rects = self.inner.solve(arena);
            // overlay each animating channel onto its node's solved rect
            rects
        }
    }

The commit then stays exactly where it is: a layout-dirty tick still calls
`commit_with` with a real solver inside it, so glyph runs stage and the layout
is authoritative, and the samples ride on its output so the transition eases
instead of snapping. Nothing reorders, no second solve, and the smoothed-gap
case is untouched because `layout_dirty` is still read after the drain.

Two details that are already in place and should stay: the switch's tracks
are bound in step 0 from the pre-switch cache, and each live track carries its
spec so a target moved mid-transition can be retargeted through `dashcue`
(which resumes from the current sample rather than snapping).

## What the fix must pass

Run all four. The first two are what the failed attempts broke, and neither
lives in the crate being changed — which is why the full tier is the gate and
a crate-scoped run is not.

- `cargo test -p demo --bin demo` — two `input::tests` cases.
- `cargo test -p showcase announcing_a_painter`.
- `cargo test -p goldens --test loaded_variant_flip` — the animation still
  eases, 100 to 97.6 to 88 to 76.
- `just build`.

Then add the test the finding has no shape for: a scene carrying a variant
transition **and** a `Visible` binding flipped in the same tick, asserting the
animated node sits strictly between its endpoints on the switch frame and
never moves backward on a later one. Mutate the fix to prove it fails without
it.

## Then merge

- Re-read the milestone's open issues first, not only this story's:
  `gh issue list --milestone "v0.18 — animation vocabulary" --state open`.
  Debt filed against a slice in progress is usually a warning about the story
  open right now — issue #783 was filed twelve minutes after its pull request
  opened.
- Tick finding 3 on the pull request description. An unticked checklist is
  what says the branch is not ready.
- Squash to one commit per meaningful change, rebase onto `main`, force-push.
- `gh pr merge --merge` — name the method, never rely on the button's
  preselection.
- Delete the branch, remove the worktree, comment the outcome on story #771,
  and check that #617 closed with it.
- Archive this prompt to `docs/archive/` and empty `docs/wip/` of it.

## CI is down for billing, and will not be fixed

`changes`, `dprint` and `fmt` fail with **zero steps** and every other job
skips behind them. The reason lives on one endpoint and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

Every `failure` in this state says nothing about the code. Merge on local
evidence and record the exception on the pull request.

## What remains in the slice after this

Stories #772 (loop tracks) and #773 (Figma prototype reactions), plus
debt #845. **#773 needs a Figma file with prototype reactions that only the
owner can author**, so it is the likeliest thing to hold the slice open. Epic #769
closes after both, and the phase-end revision follows.
