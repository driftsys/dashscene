# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-04, after story #581 merged
    empties  when epic #569 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.15 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## What story #581 left unverified

**One thing, and it needs a Linux runner.** Whether lavapipe advertises
`TEXTURE_COMPRESSION_ASTC` is unknown: the baked-block arm of
`goldens/tooling/tests/lean_painter_baked_assets.rs` skips loudly without it,
and no Linux runner has ever executed this suite. Everything ASTC in this slice
was verified on an Apple M3 via Metal. The uncompressed rung exercises the same
upload path with the block arithmetic removed, and runs everywhere. Nothing can
be done about this until CI can schedule a job.

Everything else is closed, so do not spend time re-establishing it:

- **Twenty-six painter swaps** on one running window across 13500 generations,
  no panic and no validation error, and the owner confirmed the picture. That is
  epic #569's "walked against v0.14's checklist with the wgpu painter selected",
  for the subset drawn so far.
- **Ten scene rebuilds by window resize**, with images resident, no assertion
  and the picture intact. That is the residency invalidation — PR #719's most
  serious review finding — exercised interactively: a resize replaces the arena,
  so the new image table starts again at index 0 with the same format, offset
  and length, and the `PayloadKey` is byte-identical to the previous arena's
  behind a different allocation. The debug digest assertion that catches a stale
  slot is live in that build and did not fire across ten arenas.
- **A drawable of 3024x1832**, past 2048, reached during those resizes. Useful
  beyond its own story: it is a live demonstration that `Renderer::max_extent`
  and `ATLAS_EXTENT` must differ. The drawable followed the adapter, as issue
  #714 requires; the atlas stayed a 16 MiB budget rather than becoming a
  gigabyte, as `atlas-residency-and-image-fills.md` requires. Conflating the two
  is the mistake the review caught in this story.

## Where things stand

`main` is at `f85710b`. No open pull requests. Epic #569 tracks the slice;
`docs/roadmap.md` has the slice map.

**Closed: #577, #578, #579, #580, #581, #585, #710, #714, #716**, plus issues
600, 671 and 640 earlier. Story #581 and issue #716 landed together as PR #719,
merged on local evidence with the owner's confirmation of the picture.

The painter packs the whole of boundary B into one ordered instance buffer,
evaluates its SDF math by compute shader, and draws **solid fills, outline
strokes and image fills** — clipped, composited in slice order at free-path
opacity, offscreen or to a window's swapchain. Gradients, text, group opacity,
shadows and blur are all **packed** and **not drawn**; each has a story below.

Seven decision records carry the contracts. Read the ones your story touches:

- `docs/decisions/instance-buffer-contract.md` — the row, the spans, the order
- `docs/decisions/shader-library-and-layer-2.md` — the one WGSL file, the
  compute conformance, the shadow's measured quadrature
- `docs/decisions/pipelines-and-layer-3.md` — the pipeline, the target format,
  what layer 3 may and may not claim, the stroke, the `wgsl_to_wgpu` revisit
- `docs/decisions/the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md`
  — the swapchain, the painter swap, R-T4's upload half
- `docs/decisions/baked-texel-payloads-cross-boundary-b.md` — baked formats, the
  flattened image table, `BoundPayload`, `Painter::samples`, and since #716 the
  extent on the row
- `docs/decisions/atlas-residency-and-image-fills.md` — the atlas per texel
  format, the draw runs, the binding budget, the sampler
- `docs/decisions/sub-word-members-widen-rather-than-pad.md`

## Order from the epic

**Story #582 next** (text and baked vector fields), and it is unblocked:
residency is the mechanism it was waiting for, and it arrives with one consumer
already using it. A glyph reaches it through the same
`Residency::resident` call an image does. Glyph instances carry their texel
rectangle in `Instance::corners`, which is meaningless for a glyph and is the
slot the instance-buffer contract reserved for exactly this — an image fill
could not take that route, because an image still needs its own rounded box.

Then **#715**, **#583 → #584**, **#586** (needs a GPU and a recorded adapter;
cannot run in CI), **#587**, **#588**, then close #569.

**Issue #715 is a gap this slice found twice over.** Nobody drew strokes (#710)
and nobody drew gradients (#715), both because a sentence in `render.rs` and in
`pipelines-and-layer-3.md` named the owning story and the sentence was wrong.
Both are corrected. **When prose tells you which story owns something, check it
against that story's own body before believing it.**

Open debt worth knowing, none of it blocking: **#708** (`pack::pack` still walks
every rect, so R-T4's CPU half is unmet), **#703** (`cargo doc` runs nowhere),
**#718** (the lean painter declares it cannot sample JPEG or GIF), **#720** (an
image larger than the atlas panics rather than getting its own texture).

## The binding budget is spent — read this before starting #715

`wgpu::Limits::downlevel_defaults` allows **four storage buffers per shader
stage**, and `pipelines-and-layer-3.md` D2 holds this painter to those limits.
The fragment stage now reads exactly four: solids, clips, strokes, images.

Story #581 made room for the image table by taking the instance array out of the
fragment stage entirely — `VertexOut` carries the values a fragment needs. That
bought the fifth binding and **no headroom at all**. Gradients need the gradient
rows _and_ their flat stop array: two more bindings against zero free slots.

So #715 cannot be a binding away. It has to change the structure — a
paint-parameter heap, one storage buffer of `vec4f` with a per-kind base offset,
is the obvious candidate — and `pipelines-and-layer-3.md` D6's `wgsl_to_wgpu`
question rides along with it. An earlier draft of the residency record said "one
free slot"; it was wrong, the review caught it, and the corrected arithmetic is
in `atlas-residency-and-image-fills.md` D4.

## CI IS DOWN — READ THIS BEFORE ANYTHING ELSE

The account's GitHub Actions billing is unsettled and **no job can be
scheduled**. A job that never got a runner reports **zero steps**, ~2 seconds,
no runner name, and its log 404s. The reason lives on one endpoint:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns, verbatim: _"The job was not started because recent account payments
have failed or your spending limit needs to be increased."_ The UI's "this
check has no steps" describes the symptom, not a config fault — the workflow
file is valid. **Query annotations before diagnosing anything else.**

**The owner authorised merging on local evidence while this lasts** — `just
build`, plus `just calibrate` when the diff touches the `packer` filter. Record
the exception on each PR rather than merging silently. When billing is settled,
re-run a workflow on `main` and confirm `exit-gate` and `ci` go green; the
standing rule then returns to force.

**Two things have never executed and are unverified:**

- the `mesa-vulkan-drivers` install and `VK_ICD_FILENAMES` in the `test` job.
  Four suites now need a device — `layer2_conformance`, `layer3_render_smoke`,
  `layer3_image_fills` and `frame_path` — and none has ever run on lavapipe.
- **whether lavapipe advertises `TEXTURE_COMPRESSION_ASTC`.** The baked-block
  arm of `goldens/tooling/tests/lean_painter_baked_assets.rs` skips loudly
  without it. Everything ASTC in this slice was verified on an Apple M3 via
  Metal, which has it. The uncompressed rung exercises the same upload path with
  the block arithmetic removed, and runs everywhere.

## The loop, per story

1. Read the story issue and every comment on it.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. Implement.
4. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it**. `Cargo.toml` and `Cargo.lock` are in it; a crate manifest is
   not, because that entry is root-anchored.
5. Open the PR **ready, never a draft**. Name the tiers you actually ran.
6. **Run `/code-review` and mean it — see below.**
7. Capture **every** finding as a checklist in the PR description. Fix
   criticals inline; file one `debt` issue per minor finding.
8. Merge, delete the branch with `gh api -X DELETE`, remove the worktree,
   comment the outcome on the story, update memory.

## Reviewing your own diff does not work

Story #581 is the evidence. An inline author-only pass found three real defects
and reported the work as reviewed-with-a-caveat. The five-agent `/code-review`
fan-out then found **nine more**, three at 100 confidence, including one that
defeated the entire purpose of the story: a resident PNG was being fully
re-decoded on every frame, which is the exact cost — 20.4 % of every frame —
that #581 was opened to remove.

**If the session forbids subagents, ask.** The owner enabled them on request.
Do not settle for an inline pass and a caveat.

Four things that fan-out taught, worth more than the individual bugs:

- **The picture is identical whether a payload is decoded once or every frame.**
  A cost with no visible symptom needs a counter, not a golden.
  `Residency::decodes` exists for this, the way `dashscene-skia` counts its own
  (issue #101).
- **Fixing a finding and mutation-testing the fix are two separate steps.**
  After `Renderer::allocations` was corrected to count residency, the mutation
  that removed the term _still passed everything_, because the atlas is created
  on the first frame and a steady-state delta is zero either way. Mutate every
  fix, not only the original code.
- **Derive a fixture's bound from the constant, never restate it.** The
  oversized-payload fixture said `2049`; the atlas fix changed that number, and
  a restated literal would have left the fixture too small to prove anything
  while it kept passing.
- **Give each review agent its own worktree or make it read-only**, emphatically
  and by name — five agents once destroyed each other's edits in one worktree.
  Read-only held across all five this time.

## What has actually cost time here

- **`Instance::kind` carries the sub-kind — there is no separate tag.** Stories
  #582, #583 and #584 each add a kind; map by an exhaustive `match`, never
  `enum as u32`, so a reorder in `dashpaint` is harmless and an addition is a
  compile error. `stroke_align` and `scale_mode` in `render.rs` are the pattern.
- **An instance can draw outside the bounds its quad is built from.** A stroke
  does. Ask it of every kind you add — a shadow already grows its bounds in the
  packer, a blur will need to.
- **A dirty set is stated against the commit before it**, so `Changes` carries
  the generation and `Present::document_replaced` is how a host says the arena
  restarted. **Anything else cached across frames must clear on that same
  signal.** The residency cache did not, and a scene swap could have drawn one
  image as another in a release build — the same defect one table over, found
  only by review.
- **The uniform-fixture trap has four levels**: uniform data, uniform
  _arguments_, uniform _symmetry_, uniform _environment_. Before writing a
  fixture, list what the code reads and vary each axis.
- **A mutation that does not apply looks exactly like a survivor.** Verify by
  the **absence of the original**, and compare the **whole block** — `grep -F`
  matches a multi-line pattern line by line, so a common line such as
  `continue;` inside it reports "still present" for a mutation that applied.
- **A test name is a claim.** Four in story #581 could not fail on what they
  claimed, and only mutation found it.
- **An atlas is a budget, not the device maximum.** It is allocated whole on
  first use, so `ATLAS_EXTENT` is 2048 clamped by the device; sizing it from a
  16384-capable adapter would commit a gigabyte for one image fill. That is the
  opposite of `Renderer::max_extent`, which issue #714 deliberately took _from_
  the adapter. Confusing the two is what the review caught.
- **A block-compressed texture's dimensions must be a multiple of its
  footprint**, and copies into it must be block-aligned unless they reach the
  texture's edge. Four of the six ASTC rungs do not divide 2048.
- **A device feature must be requested, not merely advertised.** Intersect
  rather than require, so a machine without it still builds.
- **After a rebase, re-read the prose near every conflict, not just the code.**
  A comment that was true when written became false when issue #714 changed the
  device request under it, and resolving the conflict never touched that line.
- **Estimate refactor churn from the read sites, not the construction sites.**
- **A green summary is not a green build.** `just build` is four gates and only
  the first prints a summary. Capture `cmd > file 2>&1; echo "REAL EXIT: $?"`.
- **`cd` does not persist between commands.** Use `git -C <abs-path>`, and do
  not remove a worktree while the shell's cwd is inside it — **or while a
  process is still running from it.** Removing the story worktree killed the
  showcase host mid-run, which then reported a failure exit with an empty log
  and read exactly like a crash.
- **Check `git config --get remote.origin.url` before any fetch, reset or
  push** (debt #677).
- **Another session may be working in this repo at the same time.** `main` moved
  under PR #719 mid-review and it had to be rebased.
- **Shape the branch before merging.** PR #719 went in as two commits: issue
  #716's boundary-B change, which is a different issue with its own reason to
  exist, and the story itself with its review fixes folded in, because a fix
  that reintroduces nine defects when reverted is not separately revertable.
  Check the tree hash across a squash — it must not change.
- **`corpus/showcase/tests/migration.rs` compares two independent arenas.**
- **markdownlint reads a line-initial `#123` as a heading.** dprint reflows the
  paragraph, so the safe fix is to reword rather than to move the number.

## Stop and ask, rather than deciding alone

- A story's scope turns out to be wrong, or already done.
- A golden moves. That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass.
- A decision that binds other stories (a band threshold, a format, an ABI
  shape). Write a `docs/decisions/` record and flag it.
- **A story needs something its prerequisite did not deliver.** Issue #640 made
  baked formats representable and left them unusable, because boundary B carried
  no extent; issue #716 closed that inside story #581's own pull request as a
  separate first commit. Ask before choosing between a separate PR and a
  separate commit.
- Layer 4 (#586) needs a GPU and a recorded adapter; it cannot run in CI.

## When the slice is done

Close epic #569 with a summary of what landed, revise the remaining epics and
stories against what v0.15 taught before v0.16 starts, and record scope-level
changes as `docs/decisions/` records — the phase-end ritual in `AGENTS.md`.
