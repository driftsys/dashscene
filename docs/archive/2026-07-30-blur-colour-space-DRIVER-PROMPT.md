# Driver prompt — settle the blur colour space by measuring it

    status  live. The blur-colour-space open question
            (docs/technotes/open-questions.md), issue #412, epic #474.
            Written 2026-07-30. Archive verbatim to docs/archive/ when the
            question is settled, and update docs/wip/README.md's count.

You are settling the **blur colour space** question in
`driftsys/dashscene-staging`. Read `AGENTS.md` first — its conventions override
your defaults. Then `./bootstrap`.

This is the last non-billing blocker in v0.13. It is described as an open
question, but it is not one: it is a missing measurement, and the measurement
needs one authored Figma file. Treat it that way.

## The question

The reference painter's surface carries **no colour space**
(`surfaces::raster_n32_premul`). That is deliberate — it is why MSDF sampling
reads the distance channels correctly, recorded in
`docs/wip/2026-07-19-color-space-blur-and-msdf.md`. It also means blur blends
in **sRGB-encoded space rather than linear light**.

That did not matter for shadow blur, which averages one colour with itself. It
matters for **backdrop** blur, which averages multi-coloured content: averaging
in the wrong space shifts the result, and usually darkens it.

Read, before touching anything:

- `docs/technotes/open-questions.md`, the "Blur colour space" entry
- `docs/wip/2026-07-19-color-space-blur-and-msdf.md` — the full analysis
- `gh issue view 412` — which is blocked on this
- `docs/decisions/backdrop-blur-is-core-vocabulary.md`

## Why it blocks issue #412

That issue measured Figma's `BACKGROUND_BLUR` as mapping nearer
`0.42-0.45 * radius` than the `radius / 2` this project ships, with sigma 7
fitting better than sigma 8 by 56 % on mean and 58 % on RMS.

**That measurement was taken through this confound.** If the painter blends in
the wrong space, the best-fitting sigma is partly compensating for that. So
retuning the constant now would bake a correction for a different defect into a
value **shared with shadows**, where `blur-falloff` was tuned against the shadow
fixtures.

Settle the space first, re-measure, then change the constant once. That
ordering is the reason #412 is on hold rather than merely unscheduled.

## What is missing, and it is one file

> Settling it needs a backdrop-blur oracle fixture over multi-coloured content,
> which does not exist yet.

The committed `backdrop-blur` and `vector-backdrop-blur` fixtures put a frosted
panel over a **flat** backdrop. Averaging a colour with itself gives the same
answer in either space, so those frames are blind to this by construction —
which is why the question survived two slices of blur work.

### Your first deliverable: a fixture-author command

Add a `backdrop-blur-multicolour` command to
`importers/figma/plugins/fixture-author/`. Read the existing `backdrop-blur`
command for the shape, and `grid-fr-overflow` for a command written
specifically to answer one question — that is the model to follow.

Design it so the measurement reads without interpretation:

- **A backdrop of well-separated colours.** Adjacent saturated blocks, or a
  strong two-colour gradient. The bigger the chroma distance across the blur
  kernel, the larger the divergence between the two spaces. A red-to-green or
  blue-to-yellow edge separates them far more than any two nearby hues.
- **A frosted panel straddling the boundary**, so the blur kernel genuinely
  mixes both colours rather than sampling one of them.
- **Nothing else.** No text, no shadows, no stroke. One construct, so a
  disagreement bisects to this and not to something adjacent
  (`corpus/figma-fixtures/README.md` §8).
- Register it in the plugin manifest, the fixture-author README, and
  `corpus/figma-fixtures/manifest.json` with a placeholder file key.

**You cannot author or capture the file yourself.** Running the plugin needs
Figma Desktop, which is the repository owner's step. Say plainly in your PR what
they need to do: run the command, send the file key, then `just deno-capture`.

### Then: the measurement, once the capture exists

Add the frame to the import oracle and measure our render against Figma's own
export. The disagreement in the blurred band is the answer:

- if our sRGB-space blend diverges materially from Figma's render, Figma is
  blending in linear light and the painter needs to as well;
- if it agrees, sRGB-space blending is what Figma does too, and the current
  surface stays as it is.

**Either outcome settles the question.** Agreement is a result, not a
non-result — record it as one.

Pick the band deliberately rather than by analogy. `blur-falloff` was measured
in issue #422 as unable to fail on a bounded-area defect of the frames it
governs, which is why it now carries a separate gate. Whatever band this frame
takes must be able to fail on the thing it is measuring; state the mutation that
fails it.

## What this does not authorise

**Do not change the painter's colour space on this branch.** The measurement is
the deliverable. If it says the surface must change, that is a second, larger
change with its own re-baseline — `docs/wip/2026-07-19-color-space-blur-and-msdf.md`
records that the no-colour-space surface is what makes MSDF sampling correct,
so moving it is not a one-line switch and must not ride along with the fixture.

**Do not touch the sigma constant.** That is issue #412, and it comes after
this, not with it.

## Standing rules, all earned in this slice

- **An issue can be wrong, and so can a prescribed fix.** Repeatedly here an
  issue's own text contradicted the code and the code was right; one prescribed
  a fix that would have been actively harmful. Read the implementation before
  treating any of the documents above as a specification.
- **A stated blocker may never have been checked.** Two items sat for four
  slices on beliefs that measurement dissolved in minutes. This question is
  itself a candidate: check that the flat-backdrop fixtures really are blind to
  it before assuming they are.
- **Mutation-test every check.** Break what it should catch, confirm a _named_
  test goes red. **A mutation that stays green is the finding** — work out why
  before patching, because the answer is often that the fixture cannot express
  the difference. That is exactly the defect this whole prompt exists to fix, so
  do not reproduce it. Record every mutation, green ones included.
- **Zero committed-artifact movement** until a capture legitimately adds one.
  Assert per file with `git hash-object` against `origin/main`, never inferred
  from a green suite. Sweep with `--no-fail-fast`.
- **Review inline. Do NOT spawn subagents to review.**
- **Nothing will notify you when a command you started finishes.** Run it in the
  foreground and read its exit code. Two agents deadlocked this way; do not be
  the third.
- Run `just verify` against the **actual commit**, not before it exists —
  otherwise the commit-message lint has not linted the real message.

## Workflow

`just verify` must exit 0; CI cannot run (billing, issue #263), so it is the
only gate. Branch `feat/backdrop-blur-multicolour-fixture`. Rebase onto
`origin/main` before the PR — it moves. **Never**
`git reset --soft origin/main`; it silently reverts anything that landed in
between and `just verify` still passes, because a revert is self-consistent.
Check `git diff --name-only origin/main HEAD` before pushing.

Squash to one commit, conventional, scope mandatory and validated. Amend with
`--no-verify`. Draft PR, `/code-review`, findings as a checklist, critical fixed
and minor filed as one `debt` issue each, ready only after review, merge with a
merge commit.

Use `Refs #412` and `Refs #474` — this prompt's first half **does not close**
either, because the question is not settled until the capture is measured.
**Never write "closes", "fixes" or "resolves" followed by a number anywhere in a
PR body unless that PR genuinely completes the issue** — GitHub acts on a
closing keyword wherever it appears, and a story was closed by accident exactly
that way in this repo.

Prose in plain, literal English. No idioms.
