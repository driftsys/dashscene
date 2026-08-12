# v0.19 driver prompt — the shown-root chain

    status  written 2026-08-11, after story #835's merge. Archived to
            `docs/archive/` at v0.19's close, with its row removed from
            `docs/wip/README.md` in the same commit.
    scope   stories #836, #837 and #838, on `main`. The slice's other stories
            are #834 and #835 (`main`, both closed), #839 to #842 (the Android
            half, on `integration/v0.19-android`), and #843, the records, which
            depends on all of them and is nobody's until they land.
    epic    #833

## The hold on these three is discharged

Epic #833 and all three issues open with **"Hold until v0.18's `dashscene-core`
and `dashscene-engine` stories have landed."** They have: v0.18 closed on
2026-08-11 when epic #769 closed. Read that instruction as satisfied rather than
as a stop — the interleaving it was protecting is why these three go to `main`
story by story instead of onto an integration branch, not a reason to wait.

## Read first

- Epic **#833** — the slice's shape, and the story table where these are S19.3
  to S19.5.
- [`../decisions/the-shown-root-bounds-the-load-not-the-paint.md`](../decisions/the-shown-root-bounds-the-load-not-the-paint.md)
  — the ruling #838 builds on. Read it before designing anything.
- [`../design/host-integration.md`](../design/host-integration.md) — the two
  integration crates as built, the "Known gaps, named" section, and the
  `document_replaced` contract named below.
- `crates/dashscene-web/src/shown.rs` — the module documentation. It states what
  R5 does and does not do on the web today, in its own words.
- `crates/dashscene-ffi/src/lib.rs` — the paragraph beginning "**Root selection
  is absent on purpose.**"

## The three stories, and why the order is not negotiable

- **#836 measures and changes nothing.** The engine solves every root and
  `Arena::dfs_order` walks all of them into one index space; nothing prices
  that. It is first because the two after it change what it measures, and
  because #836's own body says that without the band, half of #822's
  justification "would ship as an assertion, which is the shape v0.13's t2 tier
  spent a slice removing". Do not pre-empt its number — including in prose.
- **#837 is a vocabulary.** No host can say which root it shows. It settles what
  "the shown root" _is_ as a concept.
- **#838 spends it.** The solve, the committed table and the paint follow the
  shown root. This is the story that edits `Arena::dfs_order`.

## #838 is what issue #822 becomes

Not a neighbouring gap: epic #833's story table names #822 in S19.5's own row,
and the shown-root decision record is what that issue becomes. **#822 is still
open**, and closing it belongs with #838 rather than to a later tidy-up —
otherwise the milestone keeps an open issue describing work that has shipped, or
someone files a duplicate against it.

## What #837 costs beyond `main`

**The C ABI is waiting on it, in writing.** `dashscene-ffi`'s module
documentation says root selection is absent on purpose, that the ABI would carry
it, and that "It joins when #837 lands." So #837 unblocks a parameter on a
published, version-negotiated boundary, and that crate's versioning rule says
what adding one costs. Decide whether the ABI change rides with the story or
follows it, and do not leave the sentence saying the concept is still to come
once it is not.

## What #838 has to treat as a renumbering event

`Arena::dfs_order` is the **shared index space**. Root-scoping it makes a change
of shown root a renumbering event, and it has to be treated the way a replaced
document is: a new arena's generations restart and nothing in the frames
themselves says so. The contract is `Present::document_replaced`
(`crates/dashscene-desktop/src/present.rs`) and
`SurfaceRenderer::document_replaced` (`crates/dashscene-gpu/src/surface.rs`),
described in [`../design/host-integration.md`](../design/host-integration.md).
`docs/decisions/dirty-set-advisory-across-boundary-b.md` is about the dirty set
across boundary B and does **not** cover this; do not go looking there for it.

Getting this wrong does not fail loudly. It patches an instance buffer against
indices that mean something else now.

## Where the three integration crates actually stand

Checked in the source on 2026-08-11, because prose written when there were two
crates says "both integration crates" and there are three:

- **`dashscene-web` bounds the load only when no other root draws a payload.**
  `shown.rs` says it plainly: it reads the shown root's assets "only when no
  other root draws one, and otherwise reads the union over every root", and "the
  many-frame document R5's criterion is really about — many roots, one payload
  each — takes the widened path, so **R5 does not hold** for that shape on this
  target." That is the shape this chain is about, so do not write that the web
  already bounds it.
- **`dashscene-desktop`** maps the file and binds a byte range per asset entry,
  hashing only the shown root's, so an unread row still decodes.
- **`dashscene-ffi` selects no root at all.** `ds_runtime_load_document` takes
  the whole file as bytes and hands every payload to
  `dashscene_core::load_document`. Its own documentation says a mapped path
  "belongs with the platform host that has the file (story #841)" — and #841
  closed without it, while `dashscene-android` says where the document comes
  from is the embedder's. **So that path is unowned rather than impossible** —
  issue #925, filed off this prompt's own review. Do not record it as a
  structural limit; settle it near #838, after which the paint half of R5 holds
  on that path and the load half still does not.

## The cost this chain exists to remove

The roadmap prices the shape: **sixty-five artboards of solve and committed
table per frame while one is shown.** What that costs in milliseconds is exactly
what #836 measures and what nothing has measured yet — on a tiling GPU with a
fixed frame budget it is the obvious suspect, and "obvious suspect" is not a
number.

Story #842 will not supply it either: it measures the showcase's frame rate on
device, and `corpus/showcase`'s scenes are single-root. The sixty-five-root
document is `goldens/tooling/tests/startup_scaling.rs`'s.

## Environment, as of 2026-08-11

Four things changed under this repository the day these stories were queued. All
are verified, and each has already cost someone an hour:

- **`just verify` no longer runs a test tier.** PR #908 bounded the pre-push
  gate at seconds: commit-message lint, `lint`, `audit`, a scoped secret scan.
  It still type-checks — `clippy --all-targets` compiles what it lints, over the
  workspace and every package `wasm-lint` names — so a compile error still fails
  there. What no longer runs is any test. Run `just build` by hand for the
  regression tier and quote its `Summary` line.
- **CI compiles for wasm32.** The `wasm-gates` job runs `just wasm-painter`,
  `just wasm-host` and `just wasm-lint` (issue #903). Before it, four of those
  commands ran on no runner at all; the fifth, a `dashscene-web` clippy line,
  had been in the `clippy` job since PR #901.
- **`flatc` installs from `.github/actions/install-flatc`**, which derives its
  version from the workspace manifest and asserts what it installed. Do not put
  a copy of the version anywhere (issue #909).
- **A closing keyword next to an issue number closes that issue** — from a
  commit message as well as from PR prose, and a negation does not save you. Two
  issues were shut by accident this way on 2026-08-11. AGENTS.md carries the
  rule.

Also still true: **`git push` hangs** behind `git-credential-manager`. Use
`git -c credential.helper='!gh auth git-credential' push`, and expect even that
to need a retry. `gh` itself works throughout.

## The definition of done these three share

AGENTS.md's story workflow is the authority; this is what it means here.

- `just build` green, and the tier named in the pull request.
- The pull request opened **ready, never a draft** — a draft is not reviewed.
- `/code-review <pr> high` run, every finding captured as a checklist in the
  description, criticals fixed and minors filed as one **`debt`-labelled** issue
  each. Every pull request that ran it on 2026-08-11 came back with nine to
  fifteen findings, and on several the author's own pass had found none of them.
  It is not optional, and this prompt is an example: its first draft carried
  fifteen.
- **CI green on the commit being merged**, not on an earlier one
  (`docs/decisions/ci-green-before-story-merge.md`). A local `just build` does
  not substitute for it, and `just verify` no longer runs a test tier at all.
- Re-read the milestone's open issues before pressing merge, not only at the
  start: debt filed against a slice in progress is often a warning about the
  story that is open right now.
