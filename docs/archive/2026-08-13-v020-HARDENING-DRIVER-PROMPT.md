# v0.20 driver prompt — the nine items that can start now

    status  written 2026-08-13, the day epic #951 was filed. **The slice was
            planned on 2026-08-12**, which is the date the roadmap and the epic
            carry and which is load-bearing in the roadmap. Archived to
            `docs/archive/` at v0.20's close; a driver prompt has **no row** in
            `docs/wip/README.md` — captures have that table, prompts are
            described in its prose — so the same commit updates the paragraphs
            describing this one.
    scope   the nine v0.20 items that carry no hold. The three held items are
            named below with the condition that releases them, and are **not**
            this prompt's work.
    epic    #951
    slice   v0.20 — hardening. The milestone holds thirteen **open** issues,
            twelve plus the epic; `--state all` returns fifteen.

## What this file is, and what it deliberately is not

**It carries only what you cannot cheaply derive: the sequencing, and four
things that are true of the code but stated in no issue.** It does not restate
the issues, enumerate the crates, or describe what other documents say.

That is not brevity for its own sake. Four review rounds found **fifty-five**
defects in a longer version of this file, and the pattern was consistent: every
restatement was a chance to be wrong, and three rounds running, the fix for one
round's finding introduced the next round's. A handoff's reader cannot check it,
so the only safe shape is a short one pointing at sources that can be.

**Read the nine issues themselves.** They are the specification. Where this file
and an issue disagree, check the tree and trust that.

## Read first

- The nine issues: #621, #718, #720, #724, #802, #875, #890, #916, #634.
- **Epic #951** — the slice's shape and its definition of done.
- `docs/decisions/pre-v1-hardening-slice.md` — why each of these is here rather
  than on v1, and what its third term means for the `owner-input` label.
- `AGENTS.md` — the principles P1 to P5, the test tiers, and the story workflow.

## No hold applies to the nine — and three other items are held

**The nine carry no blocking condition.** Checked issue by issue on 2026-08-13:
none contains a "blocked on", "waits on" or "hold until" line.

Two stale blockers you will meet if you read carelessly. **#752** says "Blocked
on #596"; #596 is closed and landed. **#634**'s suggested fix is conditional —
replace the `#101` reference "with whichever issue tracks the per-frame
residual, **if one exists**" — and it names none and has no comments, so
establish whether one exists rather than substituting a number.

**Three items are held and are not yours: #884, #940, #888.** #884 edits
`DsStatus` in `crates/dashscene-ffi/src/lib.rs`, which three open v0.19 issues
(#947, #925, #945) also edit. #940 and #888 collide with nothing but are held
with #884 because they are one decision. Do not start them; if you believe the
v0.19 C ABI stories have landed, re-derive that from the issues.

## Four things the issues do not tell you

**1. #621 is a design decision, not a delegation.** `CachedSolver` is
`struct CachedSolver { rects: Vec<(NodeId, SolvedRect)> }` — no inner solver, so
nothing can be "forwarded", and it defines **neither** `stage_text` nor
`atlases`. Its doc states the property a fix must argue with: `commit_with` on
it "never invokes the real solver, which is how a contained write performs no
layout solve (A1)". The alternative — carrying runs forward inside `commit_with`
— lands in `crates/dashscene-core/src/arena.rs`, where v0.19's #943, #944 and
#946 are working. **That is a merge-conflict reason to prefer the `dashlang`
side, not a design reason**, and it is the one place any of these nine touches
open v0.19 work. Say which you chose and why.

Do not assume one half of the fix fails loudly and the other silently. That
asymmetry was measured on the `FlipOverlay` decorator, on the layout-dirty path,
and does not transfer. **Mutate both halves separately and confirm each fails
something.**

**2. #718 and #720 are not independent, and #718's ruling has an obstacle.**
Both edit `crates/dashscene-gpu/src/residency.rs`, and `resolve_frame`'s Panics
doc bundles them: every `ResidencyError` arm is a broken promise with no channel
to report on, because `Painter::paint` returns nothing by decision. So a "named
diagnostic" needs somewhere to go — the refusal moves earlier, to load, or the
painter's return type widens. **Whichever item lands first decides that shape,
so take them together or agree the channel before either starts.** The ruling on
#718 did not scope this.

The ruling's exact words are "do not link a decoder, and do not wire
`Painter::samples` into the bind path **for now**" — and the same comment
records that wiring it into the load path **stays open as the larger
follow-up**, the only option that stops the document reaching the painter at
all. File it if you meet it.

**3. Two of the nine are wider than their issue bodies.** #720's issue is about
an oversized image, but `crates/dashscene-gpu/src/render.rs` has **two**
`ResidencyError` panic sites — one for image assets inside `resident_image`, and
a separate one for glyph atlases inline in `resolve_frame` — and a CJK glyph
atlas is likelier to exceed 2048 than a photograph. #875 scopes itself to the
`if !blockers.is_empty()` site, but `crates/dashc/src/figma/mod.rs` has a second
early `return Ok(())` for an unsupported node kind that drops its subtree the
same way, reached by any unlowered kind. Fixing only the named site leaves the
commoner case.

**4. `corpus/figma-fixtures/jpeg-fill.json` is not a reproducer.** It is a Figma
REST capture, so it is the *source* of a JPEG-carrying document and has to go
through `dashc` first.

## Environment

Run `./bootstrap` after `git clone` or `git worktree add`. It runs
`install_git_std`, `install_nextest`, `install_jq`, `install_prim` and
`check_gitleaks`, which is what `AGENTS.md` says too. **If your context and the
file disagree, the file wins** — an earlier draft of this prompt called
`AGENTS.md` stale on the strength of a stale context snapshot.

Work in a worktree, created **before the first edit**, on a branch named in the
issue or `fix/<short-slug>`.

- `just test` — sanity tier, between edits and before every commit.
- `just build` — regression tier. Run before opening a pull request.
- `just verify` — what the pre-push hook runs, and it **runs no test tier**. A
  green push is not a statement that any test ran.
- `just prim` — `prim fmt --check .` and `prim lint .`, over **Markdown, JSON,
  YAML and TOML**, not Markdown alone. Several of these items touch JSON
  fixtures and manifests. CI runs exactly this recipe.
- `just calibrate` — only if your diff touches a path in the `packer` filter,
  defined in the `changes` job of `.github/workflows/ci.yml` and enumerated with
  a reason per entry in `docs/decisions/test-tiers.md`. Read it there; that list
  has drifted three times as a partial copy.

There is one pre-existing MD080 warning in
`docs/decisions/glyph-runs-cross-boundary-b.md`; `prim lint` still exits 0.

## Definition of done, per item

- `just build` green, and name the tier you ran in the pull request body. Never
  report a tier as run that was not run.
- Open the pull request as an ordinary one — **never a draft**; `/code-review`
  declines drafts.
- Run `/code-review` and capture **every** finding as a checklist in the
  description. Fix all critical findings; file one `debt`-labeled issue per
  minor finding rather than fixing it inline.
- **Never write "closes #N", "fixes #N" or "resolves #N" in prose.** GitHub
  closes on a keyword anywhere in the body, including inside a negated sentence.
  Write `Refs #N`.
- Re-read the milestone's open issues before merging, not only your own.
- Rebase onto the latest `main`, squash to one conventional commit, force-push,
  then merge with `gh pr merge --merge` — name the method explicitly.
- **CI must be green on the commit you are merging.** `main` carries an active
  ruleset requiring a pull request and passing checks with an empty bypass list,
  so a force-push invalidates the previous run and the merge is refused until
  the new one lands.

The slice's own definition of done is epic #951's, not this file's.
