# v0.19 driver prompt — story #838, and issue #863 behind it

    status  written 2026-08-11, rewritten 2026-08-12 when two of the three
            stories it was written for landed overnight. Archived to
            `docs/archive/` at v0.19's close, with its row removed from
            `docs/wip/README.md` in the same commit.
    scope   story #838 on `main`, and issue #863, which is labelled `story` and
            has no branch yet. The rest of the slice: #834 to #837 closed,
            #839 to #842 the Android half on `integration/v0.19-android`, and
            #843, the records, which depends on all of them.
    epic    #833

## What landed while this prompt sat here, and what it changes

**#836 and #837 are closed** (PRs #928 and #935, 2026-08-12). The earlier
revision of this file described all three as ahead of you; two are behind you.

What #837 built, because #838 stands on it:

- **`dashbuf::prefetch::ShownRoot(u32)`**, with `prefetch::resolve`, is the
  vocabulary. `docs/decisions/the-shown-root-is-named-by-ordinal.md` records
  why it is an ordinal.
- **`first_root` is deleted**, not deprecated — the module says leaving it
  would be "an invitation". Both integration crates now take a `ShownRoot`
  (`crates/dashscene-web/src/document.rs`,
  `crates/dashscene-desktop/src/document.rs`).
- **`dashscene-ffi`'s own module documentation has already been updated** and
  is worth reading before you touch anything: it explains why a `ShownRoot`
  parameter there would be "a bound that is not one", and names #838 and #925
  as the two things that would make one mean something.

**The v0.18 hold is discharged.** #838 opens with "Hold until v0.18's
`dashscene-core` and `dashscene-engine` stories have landed." They have — v0.18
closed 2026-08-11 at epic #769. Read it as satisfied.

## Story #838

**Read the issue rather than a summary of it.** It is precise where a summary
would not be: it names the three sites with line numbers, it records that D1
made painting every root the architecture as designed — so this is a change of
intent and not a bug fix — and it states the consequence that matters, that
`Arena::dfs_order` is the shared index space and root-scoping it makes a change
of shown root **a renumbering event** the dirty-set contract must treat the way
it treats `document_replaced`.

Two things to carry in beside it:

- The contract it names lives at `Present::document_replaced`
  (`crates/dashscene-desktop/src/present.rs`) and
  `SurfaceRenderer::document_replaced` (`crates/dashscene-gpu/src/surface.rs`),
  described in `docs/design/host-integration.md`.
  `docs/decisions/dirty-set-advisory-across-boundary-b.md` is about the dirty
  set across boundary B and does **not** cover this.
- **Issue #822 is what this story becomes.** Epic #833's table names it in
  S19.5's own row. Close it with the story, or the milestone keeps an issue
  describing shipped work.

## Issue #863 — read this section before you read the issue

**The issue's report is sound. The triage comments on it are not, and one of
them is mine.** On 2026-08-12 I claimed the document carries a glyph atlas,
citing a `GlyphAtlas` table. `grep -rn GlyphAtlas` over the tree returns
nothing. The table is `VectorAtlas`, for baked vector-shape fields, referenced
only by `VectorShape.atlas`; the phrase "glyph atlas" appears near it twice, as
an analogy both times. PR #933 was built on that and closed unmerged. A later
comment on the issue corrects it. Read the corrections, not the first comment.

What is actually true, each of it derived:

- **Neither the font nor the glyph atlas is in the document.** The atlas set
  reaches the runtime through one seam,
  `GlyphRunTable::with_atlases(solver.atlases())`
  (`crates/dashscene-core/src/arena.rs`) — that is, from the solver the host
  builds. `AssetKind` has two variants, `Image` and `DistanceField`.
- **The atlas is the first blocker, not the font.** `TaffySolver::stage_text`
  checks `self.atlases.is_empty()` **before** the typesetter.
- **A decision record already covers this**:
  `docs/decisions/font-resolution-order.md`, accepted 2026-07-25. It rules that
  an embedded font should use the content-addressed asset table, and says "Step
  1 is not implementable yet, and **the blocker is the atlas, not the format**"
  — the render path consumes an `AtlasBundle` and the MSDF baker is an external
  pinned binary, so nothing turns embedded font bytes into glyphs at load time.
  It names two exits: bake at pack time (#345, dashpack), or bake in process.

**So #863 does not need a new decision record. It needs that one extended in
place**, which is what AGENTS.md and the sdd lifecycle rule both require. The
issue adds two things to it: the gap is now reachable through three shipped
integration crates rather than being a v1 concern, and the layout half —
text nodes measuring as empty leaves, so siblings reflow around a box the design
did not specify — is named nowhere.

Four things a review surfaced that are still open, all checked:

- **`dashscene-android` is affected and documented nowhere.** Its "Where the
  document comes from" bullet points straight at `ds_runtime_load_document`,
  and it has a "What is not established yet" section where the note belongs.
- **An `AssetKind` append is not free.** `dashpack`'s `AssetClass::of`
  (`crates/dashpack/src/profile.rs`) matches the two kinds and returns
  `PackError::UnknownKind` for anything else, so a `Font` variant would compile
  and then fail to pack. Making it work needs an `AssetClass`, a colour space
  and a lossy-rung ladder, none of which mean anything for a binary face.
- **The bank was never answered.** #863 asked whether these come from the
  document, the bank, or the host. `dashpack` exists for cold-bank assembly and
  `crates/dashbuf/tests/bank.rs` assembles one document under two banks — the
  natural home for bytes shared across many documents.
- **Both demonstrations with a document path can reproduce it** — `demo --dsb`
  and `demo-web`'s `Source::Document` arm — and hide it only because their
  defaults are text-free. `docs/features.md` names the no-glyphs effect and not
  the layout one.

## Environment

Verified on 2026-08-11 and 2026-08-12. Each has cost someone an hour:

- **`just verify` runs no test tier** since PR #908. It still type-checks —
  `clippy --all-targets` compiles what it lints — so a compile error fails
  there. No test does. Run `just build` and quote its `Summary` line.
- **CI compiles for wasm32** since #903: the `wasm-gates` job runs
  `just wasm-painter`, `just wasm-host`, `just wasm-lint`.
- **`flatc` installs from `.github/actions/install-flatc`**, deriving its
  version from the workspace manifest. Do not copy the version anywhere (#909).
- **A closing keyword next to an issue number closes it**, from a commit
  message as well as PR prose, and a negation does not help. AGENTS.md carries
  the rule; two issues were shut by accident on 2026-08-11.
- **`git push` hangs** behind `git-credential-manager`. Use
  `git -c credential.helper='!gh auth git-credential' push`, and expect a retry.
- **Another session works this repository.** `main` moved five times during the
  session that wrote this, and twice under a branch that was mid-review. Fetch
  before you branch and rebase before you merge.

## Definition of done

AGENTS.md's story workflow is the authority; this is what it means here.

- `just build` green, and the tier named in the pull request.
- The pull request opened **ready, never a draft**.
- `/code-review <pr> high` run, findings as a checklist in the description,
  criticals fixed, minors filed one `debt`-labelled issue each. On this slice it
  has returned nine to fifteen findings on every pull request, including two
  that were wrong at their foundation and had to be closed rather than patched.
  Assume it will find something and leave time for that.
- **CI green on the commit being merged**
  (`docs/decisions/ci-green-before-story-merge.md`). `just build` locally is not
  a substitute.
- Re-read the milestone's open issues before merging, not only at the start.
