# Driver prompt — a baked-vector backdrop blur must replace its region

    status  live. Issue #503, Stream A (epic #475), v0.13. Written
            2026-07-27 at the close of the session that fixed the
            parametric half. Archive verbatim to docs/archive/ when the
            work lands, and update docs/wip/README.md's count.

You are fixing **issue #503** in `driftsys/dashscene-staging`. Read `AGENTS.md`
first — its conventions override your defaults. Then `./bootstrap`.

This is one bug, and it is the one the live hero actually depends on.

## What is already true

Do not re-derive any of this.

**Issue #405 was half-fixed.** A backdrop blur opened a `save_layer` carrying
the blurred backdrop and restored it, so it composited `SrcOver` over the
unmodified backdrop instead of replacing the region. Against an opaque backdrop
the two are indistinguishable; against a partially transparent one the sharp
original shows through beneath the blurred copy, the alpha falloff is lost, and
the alpha edge stays hard. RGB is correct throughout; only alpha is wrong.

PR #504 fixed `draw_backdrop_blur_box` — the parametric, rounded-box path — by
setting `BlendMode::Src` on the layer paint at opacity 1.0. Reading the row that
crosses a band edge inside the panel, before and after:

    x       18    24    28    31    34
    before  255   255   255   255    85
    after   254   232   186   136    85

The after row is issue #405's reference table exactly.

**`draw_backdrop_blur_field` still composites.** That is your job.

## The trap, already sprung once

The obvious move is to apply `BlendMode::Src` to the field path too. **That was
tried and it is wrong.** It took the import oracle's `vector-backdrop-blur`
frame to **22.599 % differing against a 2 % band**.

The two paths confine the blur differently:

- the box path clips to the node's rounded rect, so the layer's extent **is**
  the region to replace;
- the field path clips to the field's **padded bounding box** and then applies
  the coverage mask with `DstIn` _inside_ the layer, so the layer is transparent
  everywhere the shape does not cover.

`Src` replaces the whole clip. On the field path that writes transparent
wherever coverage is zero, erasing the real backdrop in the box around the
shape. **`Src` is sound only where the layer extent equals the region being
replaced.**

That failure is the most useful thing in this prompt: the change passed every
unit test and was caught only by the oracle. Expect the same of whatever you try.

## Shapes that might work

None is verified. They are starting points, not a design.

1. **Replace weighted by coverage.** Compute a coverage-weighted blend of
   backdrop and blurred copy, so the covered region is replaced and the
   uncovered region keeps the original exactly. One pass, and it degenerates to
   the box case at full coverage.
2. **Erase then composite.** Clear the destination within the coverage first,
   then composite the masked layer over the hole. Two passes over the same
   region; the erase needs the same coverage mask the layer uses.
3. **Nest the layers** so the outer one is already confined to the coverage,
   making its extent equal the replaced region and letting `Src` apply
   unchanged. Whether Skia lets a backdrop filter read past a non-rectangular
   clip the way it does past a rectangular one needs checking — the box path
   depends on exactly that behaviour, pinned by
   `the_backdrop_blur_reads_past_the_node_box`.

Below opacity 1.0 the copy should still composite over the sharp original, as
the box path does. That is the CSS model this project follows and
`docs/decisions/backdrop-blur-is-core-vocabulary.md` settles the surrounding
question: an opacity below 1 makes a group a backdrop root, so the isolating
case is handled by isolation rather than by a blend mode.

## How you know it worked

Two instruments, and you need both.

**A unit test** in `crates/dashscene-skia/tests/painter.rs`, alongside
`a_backdrop_blur_over_a_transparent_backdrop_softens_its_alpha_edge`, which is
the parametric twin and shows the shape to follow. Yours needs a fill-less
**vector** panel over a partially transparent backdrop, asserting the alpha
falloff across the edge rather than a flat 255. Write it failing first and
record the before values.

**The import oracle**, which is what actually decides this:

    cargo test -p goldens --test import_oracle -- --nocapture

`vector-backdrop-blur` must stay inside its 2 % band. Record its number before
you start and after you finish. If it moves at all, say by how much and why —
it should not, because the fixture's backdrop is opaque, and a change that
moves it is telling you something.

Also re-run the seven E7 frames and record them:

    cargo test -p goldens --test render_oracle \
      the_reference_renders_match_their_design_source -- --nocapture

## The artifact permit

Stream A owns every committed artifact and is the only stream that may
regenerate one. **You still should not need to.** The default is zero movement,
asserted per file with `git hash-object` against `origin/main` — not inferred
from a green suite. If your fix moves a golden, that is a finding to report with
both measurements and the reason, not something to absorb.

## Standing rules, all earned the same day

- **An issue can be wrong.** Five times in one session an issue's own text was
  wrong or its stated blocker had never been checked, and each time the code was
  right. Verify claims against the code — including the claims in issue #503,
  which the same session wrote.
- **Mutation-test every check you add.** Break what it should catch and confirm
  a named test goes red. **A mutation that stays green is a finding, not a
  nuisance** — report it and explain why, rather than patching the test until it
  fails.
- **Review inline. Do not spawn subagents to review.** And never wait on a
  notification from a command you started: run it in the foreground and read its
  exit code.
- `just verify` must exit 0. CI cannot run (billing, issue #263), so it is the
  only gate. Verify the 1-4 second no-steps failure signature rather than
  assuming it.

## Workflow

Branch `fix/vector-backdrop-blur-replaces`. Rebase onto `origin/main` before
opening the PR. **Never** `git reset --soft origin/main` — it silently reverts
anything that landed in between, and `just verify` still passes because a revert
is self-consistent. Check `git diff --name-only origin/main HEAD` before pushing.

Squash to one commit. Conventional commit, scope mandatory and validated:
`fix(dashscene-skia)`. Amend with `--no-verify`.

Draft PR, `/code-review`, findings captured as a checklist, critical fixed and
minor filed as one `debt` issue each, ready only after review, merge with a
merge commit.

End the PR body with `Closes #503`. **Never write "closes", "fixes" or
"resolves" followed by a number anywhere else in the body, including
mid-sentence** — GitHub acts on a closing keyword wherever it appears, and a
story was closed by accident exactly that way. Equally, do not write only
`Refs #N` for an issue you did resolve: two issues were left open the same day
for that reason.

Prose everywhere in plain, literal English. No idioms.
