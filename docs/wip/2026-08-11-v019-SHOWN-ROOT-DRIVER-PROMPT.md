# v0.19 driver prompt — story #838, and issue #863 behind it

    status  written 2026-08-11, rewritten twice on 2026-08-12 — once when two of
            the three stories it was written for landed overnight, and again
            when a review found the rewrite had reproduced the staleness it was
            written to fix. Archived to `docs/archive/` at v0.19's close, with
            its row removed from `docs/wip/README.md` in the same commit.
    scope   story #838 on `main`, branch `story/v019-shown-root-paint`, and
            issue #863, labelled `story` with no branch yet.
    epic    #833
    slice   #834 to #837 closed. #839 to #841 closed, and
            `integration/v0.19-android` merged into `main` on 2026-08-09 — that
            branch is history, not somewhere to work. #842 is open and owes a
            frame-rate number from hardware. #843, the records, depends on all
            of them.

## Read first

- **Issue #838 itself**, and
  `docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md`, the ruling
  it builds on. D3 and D4 are what make the traversal change a renumbering event
  and what keep `Bound::EveryRoot` in the API.
- `docs/design/host-integration.md` — "Which root each host shows", "Known gaps,
  named" (which carries the R5 document-shape condition #838 must remove), and
  the `document_replaced` contract.
- **Line numbers in #838 and in that decision record have drifted.**
  `arena.rs:975` was `dfs_order` when the issue was filed and is now the
  variant-overlay accessor; `dfs_order` is nearer 1235, and `render.rs:2152` has
  moved similarly. Find the symbols, not the lines.

## What landed while this prompt sat here

**#836 at PR #928 and #837 at PR #935**, both 2026-08-12.

**#837 built what #838 stands on.** `dashbuf::prefetch::ShownRoot(u32)` with
`prefetch::resolve`; `first_root` **deleted** rather than deprecated, because
the module says leaving it would be "an invitation"; both integration crates now
take a `ShownRoot`; and `docs/decisions/the-shown-root-is-named-by-ordinal.md`
records why it is an ordinal. `dashscene-ffi`'s module documentation was
rewritten with it and explains why a `ShownRoot` parameter there would be "a
bound that is not one" — read that before touching the ABI.

**#836 built the band that will fail when you succeed.**
`goldens/tooling/tests/per_frame_scaling.rs` asserts `MANY_LAYOUT_SOLVES = 65`
and `MANY_RECT_ROWS = 65`, and its own module documentation says the band "will
notice when #838 lands" and that stating the move "is what keeps #838's
before-and-after from claiming" more than it measured. **This is not a
regression when it goes red.** #838's definition of done requires that band
re-measured and moved with the before and after numbers stated, so take the
before number from a run on `main` before you change anything.

## Story #838

Read the issue rather than a summary. It names the three sites, records that D1
made painting every root the architecture as designed — so this is a change of
intent, not a bug fix — and states the consequence that matters:
`Arena::dfs_order` is the shared index space, so root-scoping it makes a change
of shown root **a renumbering event** the dirty-set contract must treat the way
it treats `document_replaced`.

Its definition of done is five items, and three of them are not code:

- the solve, the committed table and the paint confined to the shown root;
- a change of shown root handled as a renumbering event, **with a test that
  fails if it is treated as an ordinary commit**;
- #836's band re-measured and moved, before and after stated;
- `docs/specification/05-qualification.md`'s R5 note updated — D5 makes the
  claim per target **and per document shape**, and this is what removes the
  document-shape condition;
- debt #779 closed, or its remaining part restated.

Two things to carry in beside it:

- The renumbering contract lives at `Present::document_replaced`
  (`crates/dashscene-desktop/src/present.rs`) and
  `SurfaceRenderer::document_replaced` (`crates/dashscene-gpu/src/surface.rs`),
  described in `docs/design/host-integration.md`. **Not**
  `docs/decisions/dirty-set-advisory-across-boundary-b.md`, which is about the
  dirty set across boundary B and will send you looking for something absent.
- **Issue #822 is what this story becomes**, per epic #833's own table. Close it
  with the story.

## Issue #863 — read this before the issue

**The issue's report is sound. Its first triage comment is mine and is wrong.**
It claimed the document carries a glyph atlas, citing a type that does not exist
in the tree — search for the identifier `Glyph` followed by `Atlas` and the only
hit will be this file. PR #933 was built on that claim and closed unmerged. A
later comment on the issue corrects it; read the corrections.

What is true, each item derived:

- **Neither the font nor a baked glyph atlas is in any document today.** The
  atlas set reaches the runtime through one seam,
  `GlyphRunTable::with_atlases(solver.atlases())` in
  `crates/dashscene-core/src/arena.rs` — from the solver the host builds.
- **The atlas is the first blocker, not the font.** `TaffySolver::stage_text`
  checks `self.atlases.is_empty()` **before** the typesetter.
- **`AssetKind::DistanceField` already contemplates one.** Its schema comment
  reads "a baked vector's MSDF, a glyph atlas". So the question is not whether
  the enum needs a new variant — it is whether an atlas may be embedded at all,
  and `docs/decisions/font-resolution-order.md` answers that in its Choice: **"A
  rasterised atlas is the opposite and must never be embedded: it is a
  result."** That is P1. Do not design the embedding.
- **That record already rules on the rest**, accepted 2026-07-25: an embedded
  font should use the content-addressed asset table, and "Step 1 is not
  implementable yet, and **the blocker is the atlas, not the format**" — the
  render path consumes an `AtlasBundle` and the MSDF baker is an external pinned
  binary, so nothing turns embedded font bytes into glyphs at load time. Two
  exits: bake at pack time (#345, dashpack), or bake in process.

**So #863 extends that record in place.** It does not get a new one — AGENTS.md
and the sdd lifecycle rule both require the edit rather than a second record,
and a competing record is what PR #933 would have landed.

What #863 adds to it: the gap is reachable through three shipped integration
crates rather than being a v1 concern, and the layout half — text nodes
measuring as empty leaves, so siblings reflow around a box the design did not
specify — is named in fewer places than the glyph half.

Three findings still open, checked:

- **An `AssetKind` append would not pack.** `dashpack`'s `AssetClass::of`
  (`crates/dashpack/src/profile.rs`) matches the two kinds and returns
  `PackError::UnknownKind` otherwise. Relevant only if the ruling above is ever
  revisited for a font, which is not an atlas.
- **The bank branch is unanswered.** #863 asked whether these come from the
  document, the bank, or the host. `dashpack` exists for cold-bank assembly and
  `crates/dashbuf/tests/bank.rs` assembles one document under two banks — the
  natural home for bytes shared across many documents.
- **`docs/features.md` names the glyph effect and not the layout one**, at the
  line citing the issue.

`dashscene-android` is **already** documented —
`crates/dashscene-android/src/frames.rs` and `demo-android/src/lib.rs` both name
the issue, and `docs/features.md` names it in the Android section. An earlier
draft of this prompt said otherwise.

**Issue #925 is not a structural limit.** The C ABI has no mapped entry point
because the story its documentation deferred that to closed without giving it an
owner. Settle it near #838 — after which the paint half of R5 holds on that path
and the load half still does not — and do not write it up as impossible.

## Environment

Verified 2026-08-12. Each has cost someone an hour:

- **The repository is public and `main` carries an active ruleset.** It takes no
  direct push: a pull request and a green `ci` are required, force-push and
  deletion refused, and the bypass list is empty. `ci.yml` fires on
  `pull_request` and on pushes to `main`, so **a branch pushed with no pull
  request open runs nothing at all** — open the PR to get CI.
- **`just verify` runs no test tier** since PR #908. It still type-checks, so a
  compile error fails there; no test does. Run `just build` and quote its
  `Summary` line.
- **CI compiles for wasm32** since #903, in the `wasm-gates` job.
- **`flatc` installs from `.github/actions/install-flatc`**, deriving its
  version from the workspace manifest. Do not copy the version anywhere (#909).
- **A closing keyword next to an issue number closes it**, from a commit message
  as well as pull-request prose, and a negation does not help. Two issues were
  shut by accident on 2026-08-11.
- **`git push` hangs** behind `git-credential-manager`. Use
  `git -c credential.helper='!gh auth git-credential' push`, and expect a retry.
- **Another session works this repository.** `main` moved five times during the
  session that wrote this, twice under a branch mid-review, and two stories were
  finished overnight between this file's first and second revisions.

## Definition of done

AGENTS.md's story workflow is the authority, and pull request #934 changed it on
2026-08-12 — read it rather than this list, which is a pointer:

- **Garden what the branch added to `docs/wip/` first**, before the build and
  before the pull request.
- `just build` green, and the tier named in the pull request.
- The pull request opened **ready, never a draft**.
- `/code-review <pr> high` run, findings as a checklist, criticals fixed, minors
  filed one `debt`-labelled issue each **on a milestone**. **When a critical
  finding changes the implementation, review the fix too.**
- **CI green on the commit being merged**
  (`docs/decisions/ci-green-before-story-merge.md`).
- Re-read the milestone's open issues before merging.

On the reviews: expect them to find real defects, including in prose. Three
successive handoff documents written for this slice came back with ten to
fifteen findings each, and two pull requests were closed rather than patched
because their premises were wrong. Leave time for a second round.
