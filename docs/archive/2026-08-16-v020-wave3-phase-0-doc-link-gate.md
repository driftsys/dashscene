# Phase 0 — the doc-link gate, alone, before Wave 3 starts

Run this with **Opus**. Everything marked "Measured" was run against
`origin/main` at `4faeeda2` on 2026-08-16.

**This runs on its own.** No other lane may be in flight. The fix edits doc
comments in three crates, so it collides with every lane simultaneously — and it
is the gate that decides whether Wave 3's own documentation is checked at all.
Start it, land it, then start Wave 3.

## Setup

    git worktree add <worktrees>/wt-doc-link-gate -b debt/v020-doc-link-gate origin/main
    cd <worktrees>/wt-doc-link-gate
    ./bootstrap

## The issue

**#1046 — the intra-doc-link gate omits `--document-private-items`, so a broken
link on a private item lands green.** Read it with `gh issue view 1046`.

`just lint` runs the gate at **`justfile:116`**:

    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --quiet

Without the flag, rustdoc strips private items before the link pass, so a doc
comment **on** a private item, or a link **to** one, is never resolved. The
recipe's own comment two lines above says the gate exists because "clippy does
not resolve doc links, so a link to an item that does not exist could reach
`main`". That reasoning covers private items exactly as it covers public ones.

The issue was filed after PR #1038's review caught a broken `` [`push_entry`] ``
link that `just lint` had passed.

## Measured — this is the whole of what turning it on breaks

Do not re-derive this. Run:

    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --document-private-items

**Exit 101, five errors in three crates:**

    crates/dashscene-web/src/shown.rs:65:7    unresolved link to `crate::document`
    crates/dashscene-web/src/shown.rs:79:7    unresolved link to `crate::document`
    crates/dashscene-web/src/shown.rs:98:34   unresolved link to `crate::document`
    crates/dashscene-desktop/src/host.rs:459:39  unresolved link to `Drawn::No`
    crates/dashc/src/lib.rs:19:12            `emit` is both a function and a module

The last one is an **ambiguity**, not an absence — rustdoc's own help says "to
link to the function, add parentheses". Read the surrounding sentence before
choosing: `//! what [`emit`] writes *out of*` reads as the function, but check
which one the paragraph means rather than taking rustdoc's first suggestion.

That is a small, bounded fix. The issue says "turning it on is not a one-line
change" and is right, but the change is five sites, not a sweep.

## What to be careful about

- **Fix the links, do not silence them.** `#[allow(rustdoc::broken_intra_doc_links)]`
  is what rustdoc's help suggests and it is the wrong answer here — the whole
  point of the issue is that the gate was not seeing these.
- **The three `crate::document` links are the same link three times** in one
  file. Work out what it should name once; `crates/dashscene-web/src/` has the
  answer, and `dashscene-desktop` has a sibling module worth comparing against.
- **`Drawn::No`** — check whether the variant exists and is merely unimported
  into scope for rustdoc, or whether the name is wrong. An earlier session
  invented a type from a word in a comment; grep the identifier before assuming.
- **Wave 3 will be gated by whatever you land here.** If you narrow the flag —
  say, applying it per-crate rather than workspace-wide — say so explicitly in
  the PR, because six lanes are about to write documentation against it.

## Definition of done

1. `just lint` green **with the flag in the recipe**, and the five sites fixed
   rather than suppressed.
2. `just build` green — quote its Summary line, do not paraphrase.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` on it
   **while CI runs**. Capture every finding as a checklist; never drop one.
4. Fix critical findings, and **review the fix round too**.
5. File each independent minor finding as `debt` on **v0.20**.
6. Write **`Refs #1046`** in prose, and put the one closing line on its own line
   at the end. A closing keyword fires from commit messages that land on `main`,
   matches mid-sentence, takes only the first number, and a negated sentence
   matches as well as a positive one.
7. **Before merging** — `gh pr view <n> --json files`; confirm the diff is the
   `justfile` plus the three crates named above and nothing else.
8. **After merging** — tell the owner, so Wave 3 can start. Six lanes are waiting
   on this.
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
   the commit being merged, then `gh pr merge --merge`.

## Do not

- Do not start any Wave 3 issue. This slot is exclusive by design.
- Do not merge on a green `just verify` alone — it runs no test tier.
