# Driver prompt — finish story #393 (backdrop blur), stages B-3 and B-4

You are finishing **stages B-3 and B-4 of story #393** in `driftsys/dashscene-staging`.
Read `AGENTS.md` first — its conventions override defaults. `main` is at `f65241b`.

## Read these before touching code

- `gh issue view 393` — the story, staged B-1 … B-4, with a progress comment.
- `docs/decisions/backdrop-blur-is-core-vocabulary.md` — accepted; it fixes the
  shape you are implementing and is the authority for every design question below.
- `docs/design/dashpaint.md` — the boundary-B contract, including the ordering
  guarantee B-2 added.

## What already landed (PR #394, merged)

- **B-1** — `Blur { kind, radius }` and `blurs: [Blur]` on `Paint` in `dashbuf`.
  Backdrop blur is now NOW-band vocabulary: no `Construct`, no diagnostic, and it
  lowers through `blurs_of` in `crates/dashc/src/figma/mod.rs`. The blur reaches
  the arena as `Prop::Blurs` and lands on the committed `PaintEntry.blurs`.
- **B-2** — `PaintEntry::samples_backdrop()` (derived from `blurs`, never stored)
  and one ordering guarantee on `Painter::paint`: a painter may reorder freely
  **except** that every rect at a lower index than a backdrop-sampling rect must
  be composited before it is drawn.

So the data is already in the committed scene. Nothing paints it yet.

## B-3 — the Skia reference painter honours it

Branch `feat/blur-skia-painter`, own worktree, `./bootstrap` after creating it.

Skia has this natively: `save_layer` with a `SaveLayerRec` carrying a backdrop
`ImageFilter`. `crates/dashscene-skia/src/lib.rs` currently imports only
`MaskFilter`/`BlurStyle`, so the capability is unwired rather than missing.

Figma's `radius` maps to the painter's sigma as `sigma = radius/2` — the same
mapping shadows use, pinned by the `blur-falloff` band. Do not invent a new one.

**The open question B-2 deliberately left to you.** What does a backdrop-sampling
rect read when it falls inside a `GroupComposite` range? Figma samples through the
group; an isolated layer does not. B-2 stated the ordering guarantee over rect
indices only and marked the hole in both the trait doc and the design record.
Decide it, record the decision, and say why — this is a real fidelity choice, not
a detail. If it turns out to need the repository owner's call, ask rather than
guessing.

Done when: a golden renders a frosted panel over blurred content, and **every
existing golden is unchanged**. A moved golden means you changed something you
did not intend.

## B-4 — the oracle frame, and the band its residual earns

Branch `feat/blur-oracle-frame`.

The fixture is already committed: `corpus/figma-fixtures/backdrop-blur.json`
(Figma file `dFD9yAtbAwPPEGaaXLTouM`, a 320x180 frame — three hard-edged bands, a
circle crossing both seams, and a 200x90 frosted panel at corner radius 16, white
at 0.2 alpha, `BACKGROUND_BLUR` radius 16). It is image-free.

1. Wire it into `goldens/oracle/import-manifest.json`.
2. `deno task import-oracle-capture` from `importers/figma/` for the design
   source. `FIGMA_TOKEN` comes from the macOS keychain
   (`security find-generic-password -a "$USER" -s figma-pat -w`); never echo it.
   The token expires silently, so an auth failure is probably that.
3. Measure, then **classify the band from the measured residual — never from
   expectation.** That rule exists because `v08-baseline` was predicted into one
   band and measured into another. The three bands are reused read-only and are
   never retuned.
4. Record the number and what the residual is made of in the manifest note.

Also flip `emits` for the fixture in `corpus/figma-fixtures/manifest.json` if
B-1 did not already — check, do not assume.

## Two things that will bite you

**Triage-clean does not imply lowered.** `constructs_of` and
`blurs_of`/`shadows_of` are independent walks over `node.effects` with nothing
tying them together. B-1's review found the blur was being silently dropped on
three paths that never called `blurs_of` — VECTOR, TEXT, and a fill-less ELLIPSE
— including the hero's own frosted panel. All three are fixed, but the structural
gap remains and #396 files the same latent problem for shadows. If you add or
move effect vocabulary, check every path that builds a `PaintEntry`.

**A green oracle is not evidence of absence.** Debt #395 was a silent
paint-entry collapse that survived because the fixture exercising stacked fills
has only one stacked node, so the collision never formed. `stacked-fills`
measured 0.000 % throughout. When a frame passes, ask what it would have to
contain to fail.

## Colour space is a live variable in your residual

A blur is a weighted average of neighbouring pixels, and averaging in
sRGB-encoded space differs visibly from averaging in linear light. That question
is genuinely open — `docs/wip/2026-07-19-color-space-blur-and-msdf.md`. **If B-4
measures high, that is the first suspect, not the blur radius.** The fix may
belong to that open question rather than to this story.

## Model guidance

- **Opus** — the `GroupComposite` sampling decision, the Skia compositing design,
  and any band classification argument. These are judgement calls where a wrong
  answer is expensive and hard to see.
- **Sonnet** — wiring the manifest frame, running the capture, mechanical test
  scaffolding, and doc updates once the shape is settled.
- Review passes: **Opus** for anything asserting behavioural equivalence or
  hunting silent drops; Sonnet is fine for prose and convention checks.

## Workflow (from AGENTS.md, non-negotiable)

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
  `feat(dashscene-typeset)` passes, `feat(typeset)` and bare `feat:` are rejected
  by the pre-push hook. `git commit --amend` on a clean tree trips a stash trap —
  amend with `--no-verify`.
- Prose everywhere (comments, commit messages, docs, PR bodies) is plain literal
  English, no idioms.

## The number this is all for

The live Landify hero measurement, `just render S30AJmYfnDKGeSQmzuXEUk 1973:6580`
then `magick compare -metric AE -fuzz 5%` against Figma's `GET /images` export:

    v0.10           6.2514 %
    after #368      6.1721 %   (weights)
    after #385      4.1618 %   (families — Inter)
    after #395      3.5691 %   (a lost stacked fill layer)

Backdrop blur is the largest remaining **identified** contributor. Re-measure
after B-3 lands and record it; it is a third-party file, so the number lives in
prose and neither the file nor its render is ever committed.

Note that a second session may be working epic #344 concurrently. It touches
`crates/dashscene-skia/src/lib.rs` too — image decode and bind, a different
region from your blur path — and it will regenerate the committed `.dsb` byte
fixtures, which you will not. Whoever lands second rebases.
