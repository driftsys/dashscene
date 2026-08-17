# Wave 3, lane M — four documents that assert what the code no longer does

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.** It changes
what `just lint` resolves in doc comments, and two of your four issues are doc
comments.

## Setup

    git worktree add <worktrees>/wt-lane-m-docs -b debt/v020-docs-wave3 origin/main
    cd <worktrees>/wt-lane-m-docs
    ./bootstrap

## What you own

    #1059  docs/roadmap.md — the v0.20 rationale says the Android recovery path is untested
    #1036  docs/design/dashscene-skia.md — describes a DstIn mask a commit replaced
    #992   the desktop and web facades describe building text resources without from_faces
    #956   demo/src/shell.rs — spawn_pulses' doc states a 28 ms cost that predates two fixes

All four are the same defect in four places: **prose asserting what the code does
not do.** That is this repository's most common defect and the reason
`docs/features.md` gets re-checked at every phase end.

**Two are Markdown, two are Rust doc comments.** A `.rs` file whose diff is only
`///` lines still runs the full CI — the docs-only gate needs *every* changed
file to be Markdown under `docs/` or at the repository root. Do not expect a
cheap run for #992 or #956.

## The rule for this lane

**Check every claim against the code, not against another document.** The
`docs/features.md` re-check found 35 factual errors, and the majority came from
claims written out of this repository's own design and specification records —
four of which had themselves drifted. Reading a sibling document is how the
error propagates.

## Per issue

**#1059 — `docs/roadmap.md`.** Four claims justify why v0.20 runs before Unity
and SVG; two are settled and the paragraph still asserts them in the present
tense: the recovery path is called untested, and the give-up bound unreachable.
Both were closed — **#888** gave the loop's state machine tests and **#940** made
the bound reachable. Verify both are closed and read what they actually did
before rewriting; then correct the paragraph without inflating what now exists.

**#1036 — `docs/design/dashscene-skia.md`.** It says a baked-vector frosted node
is confined by opening a layer over the field's padded quad and using
`BlendMode::DstIn` against the coverage shader. `draw_backdrop_blur_field` has
not worked that way since commit `3aa5eb4`, which replaced it with a clip shader.
**Read the current function**, then write what it does. Do not paraphrase the
commit message.

**#992 — `crates/dashscene-desktop/src/lib.rs` and
`crates/dashscene-web/src/lib.rs`.** Both re-export `TextResources` and both
explain in prose what building one means, describing the pre-#947 world.
`dashscene_engine::TextResources::from_faces` has since been the second route,
and `corpus/showcase/src/resources.rs` — the worked example both sentences cite —
takes that route as of PR #988. So the two records point at an example that does
not do what they describe. **Check what the facades can actually call**: the
issue's own title says they "cannot call it", which is a claim about the
re-export surface, not only about the prose.

**#956 — `demo/src/shell.rs`.** `spawn_pulses`' doc says the rearm handshake is
"routine in a debug build, where the `surfaces` scene costs about 28 ms per frame
at 1920x1200".

**This one is not a prose fix, and do not treat it as one.** That figure came
from a replay harness measurement that predates issues #639 and #644, both of
which removed per-frame decoding. `corpus/showcase/README.md` already marks it as
stale. `docs/technotes/frame-budget.md` **cannot supply a replacement** — it times
`paint` offscreen as a median of per-frame medians, which is not comparable to
that mean.

So there are two honest endings, and you choose one explicitly:

- re-run the eight-phase replay that produced the original number, and state the
  new one with the harness named; or
- **drop the number** and keep the qualitative claim (a debug frame can overrun
  the interval), which is what the measurement was there to support.

Say which you did and why. Do not invent a substitute figure from a different
harness — that error is already recorded in this repository.

## Definition of done

1. `just build` green before pushing — quote its Summary line, do not paraphrase.
   Two of your four files are Rust, so this is not a formality.
2. **`just prim`** — both verbs. `prim lint` reports no format drift for Markdown,
   so `prim fmt --check` is the half that catches it.
3. **`just lint`** — with Phase 0 landed, the doc-link gate now resolves links on
   private items too. Your `///` edits are subject to it.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one. A documentation
   PR is not a small review: one in this repository returned **15 findings**.
5. Fix all critical findings. **Review the fix round too** — prose corrections
   introduce new false claims at about the rate they remove them.
6. File each independent minor finding as `debt` on **v0.20**.
7. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and **a negated
   sentence matches as well as a positive one** — which matters more in this lane
   than any other, because you will be writing sentences about what issues did
   and did not close. Story #49 was closed by a docs PR discussing whoever would
   close it, and two shipped documents then described its deliverable as shipped.
8. **Before merging** — `gh pr view <n> --json files`. Your paths are
   `docs/roadmap.md`, `docs/design/dashscene-skia.md`,
   `crates/dashscene-desktop/src/lib.rs`, `crates/dashscene-web/src/lib.rs` and
   `demo/src/shell.rs`. Anything else is a stray.
9. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`.
11. After merging, `gh issue view <n> --json state` for **every** issue your
    commits named, not only those in the PR body.

## Do not

- Do not correct a document by reading another document. The code is the source
  of truth; a design record that disagrees with it is drift to be surfaced, not a
  reference to copy.
- Do not rewrite a claim into a stronger one. #1059's paragraph is wrong because
  it understates what exists; the fix is accuracy, not advocacy.
- Do not edit `crates/dashscene-skia/src/lib.rs` while correcting #1036 — you are
  fixing the description, and that file belongs to **lane N** in Phase 2. If the
  code is also wrong, file it.
- Do not merge on a green `just verify` alone. It runs no test tier.
