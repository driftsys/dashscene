# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-04, after story #582 merged
    empties  when epic #569 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.15 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## What is left unverified

**One thing, and it needs a Linux runner.** Whether lavapipe advertises
`TEXTURE_COMPRESSION_ASTC` is unknown: the baked-block arm of
`goldens/tooling/tests/lean_painter_baked_assets.rs` skips loudly without it,
and no Linux runner has ever executed this suite. Everything ASTC in this slice
was verified on an Apple M3 via Metal. The uncompressed rung exercises the same
upload path with the block arithmetic removed, and runs everywhere. Nothing can
be done about this until CI can schedule a job.

Everything else is closed, so do not spend time re-establishing it:

- **Forty-one painter swaps** on one running window across 18600 generations and
  18632 presents, no panic and no validation error, and the owner confirmed the
  picture with text drawn. That is epic #569's "walked against v0.14's checklist
  with the wgpu painter selected", for the subset drawn so far. A swap tears
  down and rebuilds what the device holds, so it is also the residency
  invalidation exercised forty-one times with glyph atlases resident.
- **The baked vector field measured against the reference painter, once.**
  The showcase's own star, solid-filled at 320x320: worst channel delta **9 of
  255**, and **4 pixels of 102400** differing by more than 8. One shape at one
  size on one adapter, so it is not a band and does not pre-empt story #586 —
  but it does say the baked-field arm is not where that band will be spent.
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

`main` is at `eb0b2b1`. No open pull requests. Epic #569 tracks the slice;
`docs/roadmap.md` has the slice map.

**Closed: #577, #578, #579, #580, #581, #582, #585, #710, #714, #716**, plus
issues 600, 671 and 640 earlier. Story #582 landed as PR #723, merged on local
evidence with the owner's confirmation of the picture.

The painter packs the whole of boundary B into one ordered instance buffer,
evaluates its SDF math by compute shader, and draws **solid fills, outline
strokes, image fills, positioned glyph runs, and a solid fill masked by a baked
vector field** — clipped, composited in slice order at free-path opacity,
offscreen or to a window's swapchain. Gradients, group opacity, shadows and blur
are all **packed** and **not drawn**; each has a story below.

**A masked _gradient_ fill therefore still draws nothing**, and that is the one
combination where two open stories meet: story #582 resolves the mask and its
coverage is correct, and the colour it would modulate is issue #715's. The
showcase's only baked vector field is exactly that node, so `surfaces` shows an
empty tile there under this painter and a star under Skia. Expected, not a
regression.

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
- `docs/decisions/tables-the-vertex-stage-reads.md` — **read this before #715**:
  which stage may read a table, the test that decides it, and the second sampler
- `docs/decisions/sub-word-members-widen-rather-than-pad.md`

## Order from the epic

**Issue #715 next** (gradient fills), and read the binding section below before
starting it — the arithmetic changed when story #582 landed.

Then **#583 → #584**, **#586** (needs a GPU and a recorded adapter; cannot run
in CI), **#587**, **#588**, then close #569.

**Issue #715 is a gap this slice found twice over.** Nobody drew strokes (#710)
and nobody drew gradients (#715), both because a sentence in `render.rs` and in
`pipelines-and-layer-3.md` named the owning story and the sentence was wrong.
Both are corrected. **When prose tells you which story owns something, check it
against that story's own body before believing it.**

Open debt worth knowing, none of it blocking: **#708** (`pack::pack` still walks
every rect, so R-T4's CPU half is unmet), **#703** (`cargo doc` runs nowhere),
**#718** (the lean painter declares it cannot sample JPEG or GIF), **#720** (a
payload larger than the atlas panics rather than getting its own texture —
widened by story #582 to cover glyph atlases and baked-vector atlases, and a CJK
sheet is the likeliest of the three to exceed 2048 square), **#724** (a glyph
atlas with `px_per_em` of zero divides unguarded, where every sibling degenerate
case is named).

## BOTH stages are now full — read this before starting #715

`wgpu::Limits::downlevel_defaults` allows **four storage buffers per shader
stage**. As of story #582 the pipeline binds seven, and the count is:

    vertex    instances(0), strokes(4), glyph runs(8), shapes(9)   4 of 4
    fragment  solids(1), clips(2), strokes(4), images(5)           4 of 4

**Do not go looking for the route story #582 used.** It bound its two tables to
the vertex stage and passed their values across in `VertexOut`, which cost the
fragment stage no binding at all. That works only when **every value a fragment
needs of a table is constant across the instance** — a glyph run's colour and
range are, a coverage mask's plane, rectangle and range are. A gradient's stop
array is **not**: it is indexed by a value the fragment computes from its own
coordinate, so it does not cross as a varying at any width.

So #715 has to change the structure, as it always did. A paint-parameter heap —
one storage buffer of `vec4f` with a per-kind base offset — is still the obvious
candidate, and `pipelines-and-layer-3.md` D6's `wgsl_to_wgpu` question rides
along with it. `docs/decisions/tables-the-vertex-stage-reads.md` D2 and D4 state
the test and the arithmetic; its "Alternatives considered" argues why the heap
was not built inside #582, which is the reasoning to overturn if you disagree
rather than to rediscover.

**This paragraph has been wrong twice, in opposite directions.** An early draft
of the residency record claimed one free slot when there were none. Then the
record written by story 582 claimed the varyings were counted against "the sixty
`downlevel_defaults` allows" — wgpu 30 has no such field at all, the real limit
is `max_inter_stage_shader_variables` at **15**, it counts `@location` slots
rather than float components, and `VertexOut` now uses **9 of 15**. Both were
caught by review rather than by the compiler, because a wrong number in prose
compiles. **Read the limit out of the pinned crate before trusting any figure
here:**

    grep -n "max_inter_stage" ~/.cargo/registry/src/*/wgpu-types-30.0.0/src/limits.rs

and check which constructor it belongs to — `defaults`, `downlevel_defaults` and
`downlevel_webgl2_defaults` sit together and carry different numbers.

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
  #583 and #584 each add a kind; map by an exhaustive `match`, never
  `enum as u32`, so a reorder in `dashpaint` is harmless and an addition is a
  compile error. `stroke_align` and `scale_mode` in `render.rs` are the pattern.
- **"Packed but not drawn" is only safe if the shader actually discards.** A
  masked node drew as a plain rounded rectangle over its whole box from story
  #578 until #582: the packer set `Instance::shape` and the fragment stage never
  read it, so the picture was **wrong** rather than absent — the one place in
  this pipeline where an unimplemented construct did not simply draw nothing.
  Check the shader's fall-through for every kind you leave undrawn.
- **An instance can draw outside the bounds its quad is built from**, or draw
  somewhere else entirely. A stroke does the first; a masked instance does the
  second — its quad is the coverage field's padded plane quad instead of the
  node's box, substituted in the vertex stage, while `VertexOut.bounds` stays
  the node box because a gradient's frame is stated over it. Ask it of every kind you add — a shadow already grows its bounds in the
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
- **When two inputs agree in every parameter, no fixture can tell them apart.**
  Story #582's two corpus atlases agree on extent, `px_per_em` and
  `distance_range_px`, so swapping one for the other moved the measured ink by
  **5 px of 736** — no tolerance separates that from noise. The test that closes
  it packs **once** and renders **twice**, varying only the one input under
  test and comparing the outputs. Reach for that shape when an assertion on a
  single output cannot discriminate.
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
