# Driver prompt — open epic #344's own scope (sectioned container + asset model)

You are opening **the v0.11 slice's own scope**: the sectioned `.dsb` envelope
and the content-addressed `AssetTable`. Read `AGENTS.md` first — its conventions
override defaults. `main` is at `f65241b`.

Everything v0.11 has shipped so far was a **rider** off #379 (fonts) and a
fidelity thread. The epic's actual body is untouched, and that is what you are
starting.

## Read these before touching code

- `gh issue view 344` — the epic, with progress comments.
- `docs/roadmap.md`, the v0.11 section — what the slice delivers and why.
- **`docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`** — the design
  capture this work feeds from (kernel §1, §4). It is tracked, and it is the only
  `docs/wip/` entry that belongs to you. Its open points are named there:
  `AssetEntry` hash semantics, placeholder colour, derivation-manifest signing.
- `docs/decisions/asset-model-content-addressed-blobs.md` (#107).
- `docs/decisions/dsb-sectioned-container.md` — it deferred the envelope to
  exactly this work.

## What the slice delivers

1. **The sectioned `.dsb` envelope** — magic, section table, per-section hashes;
   hot head, page-aligned cold tail.
2. **The content-addressed `AssetTable`** (#107), replacing v0's inline image
   bytes. v0 documents remain the RAW/null-binding special case.
3. **Shared image identification and header parsing in `dashc`** for all
   producers (#339 rides here) — the P4 gate the dashlang path currently lacks.
   Identification and header parse only; **never decode**.
4. It seeds the R5 loading-performance measurements (mmap, cold-page behaviour).

## The R7 conversation you must have deliberately

R7 is "same input, byte-identical document". Every schema change so far has been
_additive_, so committed `.dsb` fixtures stayed byte-identical and that was the
proof. **The envelope is structural — it will rewrite them.** That is expected,
but it must be a decision you state and defend, not a diff you discover.

The eight committed byte fixtures are under `goldens/dsb/` and
`crates/dashbuf/tests/fixtures/`. Their guardians:

- `crates/dashbuf/tests/schema_evolution.rs` decodes the frozen v0.5 fixture — it
  proves **decode-compatibility**, not byte-identity, and it is regenerated only
  under `UPDATE_DSB_FIXTURE=1`.
- The byte-identity half is pinned elsewhere, by
  `crates/dashc/tests/figma_lowering.rs::the_fixture_emits_the_golden_dsb` and its
  siblings in `flex_lowering.rs` and `text_lowering.rs`, which recompile a fixture
  and `assert_eq!` the bytes against a committed golden.

Know which of those you are invalidating and why, before you regenerate anything.

## Re-measure the hero after you land

The live Landify hero diff is the slice's fidelity instrument:

    just render S30AJmYfnDKGeSQmzuXEUk 1973:6580
    magick compare -metric AE -fuzz 5% /tmp/hero-figma.png /tmp/render.png null:

    v0.10           6.2514 %
    after #368      6.1721 %   (weights)
    after #385      4.1618 %   (families — Inter)
    after #395      3.5691 %   (a lost stacked fill layer)

Your work changes how image bytes reach the painter, so it can move that number.
**Re-measure and record it deliberately** rather than letting a later session
discover the drift. It is a third-party file: the number lives in prose, and
neither the file nor its render is ever committed
(`docs/decisions/figma-corpus-self-authored-only.md`). `FIGMA_TOKEN` is in the
macOS keychain (`security find-generic-password -a "$USER" -s figma-pat -w`);
never echo it, and note it expires silently.

## Two failure modes this repo has actually hit — worth carrying in

**A green oracle is not evidence of absence.** Debt #395 was a silent
paint-entry collapse that lost a fill layer on load; it survived because the
fixture exercising stacked fills has only one stacked node, so the collision
never formed. Fifteen oracle frames stayed green throughout, and fixing it moved
the hero by 0.59 pp. When a frame passes, ask what it would have to contain to
fail.

**Independent walks over the same data drift apart.** In `dashc`,
`constructs_of` and `shadows_of`/`blurs_of` walk `node.effects` separately with
nothing tying them together, so a construct can be triage-clean and still never
lowered — a silent drop. #396 files the live instance for shadows. You are adding
another walk over image data; give it one source of truth rather than a parallel
one.

## Model guidance

- **Opus** — the envelope layout and section table, the `AssetTable` hash
  semantics, the R7 argument, and anything touching the load path. Schema and
  format decisions are expensive to revisit and hard to see wrong.
- **Sonnet** — mechanical migration of call sites, test scaffolding, doc updates
  once the shape is settled, and the `dashc` identification/header-parse plumbing
  after its contract is fixed.
- Review passes: **Opus** for behavioural equivalence, byte-identity claims, and
  silent-drop hunting; Sonnet for prose and convention checks.

## Workflow (from AGENTS.md, non-negotiable)

- The epic is broken into `story`-labeled issues, one per independently workable
  piece. **File the stories before you build** — the plan lives as GitHub issues.
- One git worktree per story branch; `./bootstrap` after `git worktree add`.
- `just build` green before anything is called done.
- Open the PR as a **draft**, run `/code-review` on it, capture **every** finding
  as a checklist in the PR description, fix the critical ones, file one
  `debt`-labeled issue per minor one.
- Mark ready only when the review pass is complete. Merge with a merge commit
  (`gh pr merge --merge`), never "Rebase and merge".
- **CI cannot run** — every GitHub Actions job fails in 3-4 s with no steps, the
  billing block tracked as #263. Local `just build` is the gate.
- Commit messages: conventional, and the **scope is mandatory and validated**.
  `feat(dashbuf)` passes, bare `feat:` is rejected by the pre-push hook.
  `git commit --amend` on a clean tree trips a stash trap — amend with
  `--no-verify`.
- Prose everywhere is plain literal English, no idioms.
- When you finish the slice, garden `docs/wip/` — the asset-pipeline capture is
  gardened when this work lands, and `docs/wip/README.md` has a table saying so.

## Concurrency with the other session

A second session is finishing story #393 (backdrop blur) — stages B-3, the Skia
painter, and B-4, its oracle frame.

- **No schema collision.** #393's schema change (B-1) already landed, so you
  build on a `dashbuf.fbs` that already carries `Blur`. B-3 and B-4 change no
  schema and will not regenerate the `.dsb` fixtures — only you will.
- **One shared file**: `crates/dashscene-skia/src/lib.rs`. You touch image decode
  and bind; they add a blur draw path. Different regions, ordinary textual
  conflict at worst.
- Their oracle frame is **image-free**, so your `AssetTable` change cannot
  perturb its residual.
- Whoever lands second rebases.
