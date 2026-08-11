# v0.19 driver prompt — the shown-root chain

    status  written 2026-08-11, after #835's merge and the debt it surfaced.
            Archived to `docs/archive/` at v0.19's close, with its row removed
            from `docs/wip/README.md` in the same commit.
    scope   stories #836, #837 and #838, on `main`. Not the Android half —
            #841 and #842 are `integration/v0.19-android`'s.
    epic    #833

## Read first

- Epic **#833** — the slice's shape and why these three go to `main` rather
  than the integration branch: they improve crates that already exist, and they
  have to interleave with v0.18's `dashscene-core` and `dashscene-engine` work
  story by story rather than as one deferred merge.
- [`../decisions/the-shown-root-bounds-the-load-not-the-paint.md`](../decisions/the-shown-root-bounds-the-load-not-the-paint.md)
  — the ruling #838 builds on. Read it before designing anything.
- [`../design/host-integration.md`](../design/host-integration.md) — what the
  two integration crates are as built, including the "Known gaps, named"
  section where #822 still sits.
- `crates/dashscene-ffi/src/lib.rs` — the module documentation, specifically
  the paragraph beginning "**Root selection is absent on purpose.**"

## The three stories, and why the order is not negotiable

- **#836 measures and changes nothing.** The engine solves every root and
  `Arena::dfs_order` walks all of them into one index space; nothing prices
  that. It is first because the two after it change what it measures.
- **#837 is a vocabulary.** No host can say which root it shows. It settles
  what "the shown root" _is_ as a concept.
- **#838 spends it.** The solve, the committed table and the paint follow the
  shown root. This is the story that edits `Arena::dfs_order`.

## What #837 costs beyond `main`

**The C ABI is waiting on it, in writing.** `dashscene-ffi`'s module
documentation says root selection is absent on purpose, that the ABI would
carry it, and that "It joins when #837 lands." So #837 is not only a `main`
story — it unblocks a parameter on a published, version-negotiated C ABI, and
that crate's own versioning rule says what adding one costs.

Whoever takes #837 should read that paragraph and decide whether the ABI change
rides with the story or follows it. Do not leave the sentence saying the
selection concept is still to come once it has.

## What #838 has to treat as a renumbering event

`Arena::dfs_order` is the **shared index space**. Root-scoping it makes a change
of shown root a renumbering event, and the dirty-set contract has to treat that
the way it treats `document_replaced` — a new arena's generations restart and
nothing in the frames themselves says so. Getting this wrong does not fail
loudly; it patches an instance buffer against indices that mean something else
now.

`docs/decisions/` already carries the dirty-set contract. Re-read it before
touching the order, not after.

## What is true of the three integration crates today

Checked on 2026-08-11, because "both integration crates" appears in prose
written when there were two and there are now three:

- `dashscene-web` and `dashscene-desktop` call `dashbuf::prefetch::first_root`
  in their document loaders, so "the shown root" means "root 0" in both.
- **`dashscene-ffi` calls neither.** `ds_runtime_load_document` takes the whole
  file as bytes and calls `dashscene_core::load_document` over every payload,
  with no root selection anywhere. That is structural — the host hands over
  bytes it has already read, so there is no byte range left to bound — but it
  means R5 has no expression at all on the Android path, where the other two at
  least bound the load to root 0.

So when #838 confines the solve, the table and the paint, that is the only part
of R5 that can hold on Android. Say so in the records rather than letting a
reader infer that the Android path gained what the other two have.

## The cost this chain exists to remove

The roadmap prices it: **sixty-five artboards of solve and committed table per
frame while one is shown.** On a desktop GPU that is waste. On the tiling GPU
with a fixed frame budget that this project targets, it is the difference
between meeting the budget and not — which is what story #842 will measure on
device, so the two are related even though only one is on this branch.

## Environment, as of 2026-08-11

Four things changed under this repository during the day these stories were
queued. All are verified, and each has cost someone an hour:

- **`just verify` no longer runs any test tier.** PR #908 bounded the pre-push
  gate at seconds: commit-message lint, `lint`, `audit`, a scoped secret scan.
  A green `verify` is **not** a statement that anything ran. Run `just build`
  by hand for the regression tier, and quote its `Summary` line rather than
  `verify`'s exit code.
- **CI now compiles for wasm32.** The `wasm-gates` job runs `just wasm-painter`,
  `just wasm-host` and `just wasm-lint` (issue #903). Before it, five commands
  that every developer ran locally ran on no runner at all.
- **`flatc` installs from `.github/actions/install-flatc`**, which derives its
  version from the workspace manifest. Do not add a copy of the version
  anywhere (issue #909).
- **A closing keyword next to an issue number closes that issue, from a commit
  message as well as from PR prose, and a negation does not save you.** Two
  issues were shut by accident this way on 2026-08-11. AGENTS.md carries the
  rule; the safe form is the keyword-free reference.

Also still true: **`git push` hangs** behind `git-credential-manager`. Use
`git -c credential.helper='!gh auth git-credential' push`, and expect even that
to need a retry. `gh` itself works throughout.

## The definition of done these three share

- `just build` green, and the tier named in the pull request.
- The pull request opened **ready, never a draft** — a draft is not reviewed.
- `/code-review <pr> high` run, every finding captured as a checklist in the
  description, criticals fixed and minors filed one issue each. On the four
  pull requests that ran it on 2026-08-11 the fan-out found 10, 10, 13 and 14
  findings; the author pass found none of them. It is not optional.
- Re-read the milestone's open issues before pressing merge, not only at the
  start: debt filed against a slice in progress is often a warning about the
  story that is open right now.
