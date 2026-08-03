# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-03, after #585, #710 and #640 closed
    empties  when epic #569 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.15 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## Where things stand

`main` is at `3a3d583`. No open PRs. One worktree (the primary, on `main`).
Epic #569 tracks the slice; `docs/roadmap.md` has the slice map.

**Closed: #577, #578, #579, #580, #585, #710, #640** (plus #600 and #671
earlier).

The painter packs the whole of boundary B into one ordered instance buffer,
evaluates its SDF math by compute shader, and draws **solid fills and outline
strokes** — clipped, composited in slice order at free-path opacity — either
offscreen or to a window's swapchain. Gradients, images, text, group opacity,
shadows and blur are all **packed** and **not drawn**; each has a story below.

Six decision records carry the contracts. Read the ones your story touches:

- `docs/decisions/instance-buffer-contract.md` — the row, the spans, the order
- `docs/decisions/shader-library-and-layer-2.md` — the one WGSL file, the
  compute conformance, the shadow's measured quadrature
- `docs/decisions/pipelines-and-layer-3.md` — the pipeline, the target format,
  what layer 3 may and may not claim, and (since #710) the stroke and the
  `wgsl_to_wgpu` revisit
- `docs/decisions/the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md`
  — the swapchain, the painter swap, and R-T4's upload half
- `docs/decisions/baked-texel-payloads-cross-boundary-b.md` — baked formats,
  the flattened image table, `BoundPayload`, `Painter::samples`
- `docs/decisions/sub-word-members-widen-rather-than-pad.md`

## Order from the epic

**#581 next**, and it is now unblocked: issue #640 landed the representation it
should be written against, so nothing about it is a finding to surface
mid-story any more. Four things it inherits and must use rather than rebuild:

- **`Painter::samples`** — the lean painter takes the conservative default
  today (source-encoded formats only). This story is where it declares what it
  can actually upload. Overriding it is the one change that makes the baked
  path real.
- **`dashscene_core::BoundPayload`** — how a host binds a derived payload with
  its own format. Nothing selects a derivation yet; if this story wires one up,
  the profile question is already answered by `dashpack`.
- **`ImageFormat`'s baked variants** — exactly the rungs the packer emits: six
  ASTC block sizes in two colour spaces, plus `Rgba8`. `ImageFormat::block()`
  gives the footprint. A format the packer cannot produce is deliberately
  absent; do not add one speculatively.
- **The bind group is five entries**, not four. `pipelines-and-layer-3.md` D6
  names the binding surface growing as the trigger to revisit `wgsl_to_wgpu`;
  it was revisited at five and still not adopted. An atlas is the next
  candidate, and the argument to weigh is in that record.

Then **#582** (text and baked vector fields), and **#583 → #584** in parallel
with it. **#586** needs a GPU and a recorded adapter and cannot run in CI.
Then #587, which needs #585, and #588 last.

Open debt worth knowing, neither blocking: issue **#708** — `pack::pack` still
walks every rect, so R-T4's CPU half is unmet even though its upload half
landed. Issue **#703** — `cargo doc` runs nowhere.

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
  Three suites now need a device — `layer2_conformance`, `layer3_render_smoke`
  and `frame_path` — and none of them has ever run on lavapipe. If the ICD path
  is wrong they fail by name rather than passing vacuously, which is the
  failure mode that matters.
- layer 3 on lavapipe. It puts the rasteriser, the AA resolve and the blend
  stage back — three of the four things the job's own comment cites as the
  reason lavapipe is trustworthy for layer 2. The exact `assert_eq!(alpha, 0)`
  checks and the `(120..=136)` bands are the most exposed.

## The loop, per story

1. Read the story issue and every comment on it.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. Implement.
4. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it**. `Cargo.toml` and `Cargo.lock` are in it; a crate manifest is
   not, because that entry is root-anchored.
5. Open the PR **ready, never a draft**. Name the tiers you actually ran.
6. Run `/code-review`. **Give each review agent its own worktree or make it
   read-only** — five agents mutation-testing one worktree stepped on each
   other, and one ran `git checkout --` over another's in-flight edit. If the
   session forbids subagents, review inline and **say so in the comment**, so
   nobody reads it as the usual pass.
7. Capture **every** finding as a checklist in the PR description. Fix
   criticals inline; file one `debt` issue per minor finding.
8. Merge, delete the branch with `gh api -X DELETE`, remove the worktree,
   comment the outcome on the story, update memory.

## Use the painter swap — it is how the last gap was found

`demo --painter gpu` starts on the lean painter, and **`P` swaps it on the
running window** keeping the arena, the clock and the pulse phase, so the two
painters draw the same frame. Story #710 existed because that comparison showed
missing borders an hour after the key landed, and nothing else in the slice
would have shown them before slice close.

Run it after every primitive lands. What is missing should be missing by
_story_; anything missing that no story owns is a gap in the breakdown, and
that is a finding worth filing rather than absorbing.

## What has actually cost time here

- **`Instance::kind` carries the sub-kind — there is no separate tag.** It used
  to be two fields whose values collided, and story #580's shader painted a
  shadow from the solid-fill table. #582, #583 and #584 each add a kind; map by
  an exhaustive `match`, never `enum as u32`, so a reorder in `dashpaint` is
  harmless and an addition is a compile error. `stroke_align` in `render.rs` is
  the pattern to copy.
- **An instance can draw outside the bounds its quad is built from.** A stroke
  does: Outside by a full width, Center by half, and the vertex shader grows
  the quad by that outset. Miss it and the outer half is clipped by its own
  geometry, which looks like a thinner stroke rather than like a defect. Ask
  the same question of every kind you add — a shadow already grows its bounds
  in the packer, a blur will need to.
- **A dirty set is stated against the commit before it.** Patching anything by
  it is sound only if every commit reached the device, in order, from the same
  arena — so `Changes` carries the generation, and `Present::document_replaced`
  is how the host says the arena restarted. A presenter that declines a frame
  (timed-out acquire, occluded window, zero extent) breaks the chain silently,
  and a converging animation makes the loss permanent: the last step of a
  spring landed on a declined frame and a rect stayed 0.02 units narrow for the
  rest of the run. Do not weaken either guard without reading
  `the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md` D5-D6.
- **The uniform-fixture trap has four levels**, and v0.15 hit all four: uniform
  data, uniform _arguments_, uniform _symmetry_ (every fixture centred, so a
  y-flip is invisible), and uniform _environment_ (one canvas width, so the
  readback padding never ran). Before writing a fixture, list what the code
  reads and vary each axis.
- **A mutation that does not apply looks exactly like a survivor.** Verify by
  the **absence of the original** before reading the result — and make the
  string you check _unique to the line under test_. A `grep -qF` that matched a
  different line with the same substring reported "did not apply" for a
  mutation that had.
- **Estimate refactor churn from the read sites, not the construction sites.**
  Flattening `ImageTable` looked like 42 files and cost five, because the
  producer kept its shape and only the readers moved.
- **A green summary is not a green build.** `just build` is four gates and only
  the first prints a summary. Capture `cmd > file 2>&1; echo "REAL EXIT: $?"`.
- **`cd` does not persist between commands.** Use `git -C <abs-path>`, and do
  not remove a worktree while the shell's cwd is inside it — every later
  command then fails with `getcwd`.
- **Check `git config --get remote.origin.url` before any fetch, reset or
  push** (debt #677).
- **Another session may be working in this repo at the same time.**
- **`corpus/showcase/tests/migration.rs` compares two independent arenas.**
  Anything flattened into a table needs resolving there in the same change.
- **markdownlint reads a line-initial `#123` as a heading.** Reflow so an issue
  number never starts a line; it has failed `just lint` three times.

## Stop and ask, rather than deciding alone

- A story's scope turns out to be wrong, or already done.
- A golden moves. That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass.
- A decision that binds other stories (a band threshold, a format, an ABI
  shape). Write a `docs/decisions/` record and flag it. Issue #640 was exactly
  this, and its own first open question named the wrong enum — so read the code
  before trusting an issue's framing.
- Layer 4 (#586) needs a GPU and a recorded adapter; it cannot run in CI.

## When the slice is done

Close epic #569 with a summary of what landed, revise the remaining epics and
stories against what v0.15 taught before v0.16 starts, and record scope-level
changes as `docs/decisions/` records — the phase-end ritual in `AGENTS.md`.
