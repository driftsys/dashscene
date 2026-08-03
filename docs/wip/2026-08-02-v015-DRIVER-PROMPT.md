# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-03, after #577-#580 closed
    empties  when epic #569 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.15 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## Where things stand

`main` is at `a44cf65`. No open PRs. One worktree (the primary, on `main`).
Epic #569 tracks the slice; `docs/roadmap.md` has the slice map.

**Closed: #577, #578, #579, #580** (plus #600 and #671 earlier).

The painter can pack the whole of boundary B into one ordered instance buffer,
evaluate its SDF math by compute shader, and draw opaque rounded rects with a
solid fill — clipped, composited in slice order at free-path opacity,
offscreen. Gradients, images, group opacity, shadows and blur are all
**packed** and **not drawn**; each has a story below.

Four decision records carry the contracts. Read them before touching the
painter:

- `docs/decisions/instance-buffer-contract.md` — the row, the spans, the order
- `docs/decisions/shader-library-and-layer-2.md` — the one WGSL file, the
  compute conformance, the shadow's measured quadrature
- `docs/decisions/pipelines-and-layer-3.md` — the pipeline, the target format,
  and what layer 3 may and may not claim
- `docs/decisions/sub-word-members-widen-rather-than-pad.md`

## Order from the epic

**#585 next.** It depends only on #580, the epic wants it early because the
rest of the slice develops against it, and it is where R-T4's real work
belongs: `Renderer::render` currently allocates four buffers, a texture, a view
and a bind group _per call_, because its only caller renders one frame. Reusing
them across frames, and uploading only the dirty rects' spans, needs a caller
that renders more than once.

Then **#581** — read **#640** first; it is a prerequisite, not a finding to
surface during the story. Then #582, and #583 → #584 in parallel with it.
**#586** needs a GPU and a recorded adapter and cannot run in CI. Then #587,
which needs #585, and #588 last.

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

- the `mesa-vulkan-drivers` install and `VK_ICD_FILENAMES` in the `test` job
  (layer 2 and layer 3 both need a device). An earlier revision put the install
  in `clippy`, which compiles the suite and never runs it — caught by review,
  not by CI.
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
   other, and one ran `git checkout --` over another's in-flight edit.
7. Capture **every** finding as a checklist in the PR description. Fix
   criticals inline; file one `debt` issue per minor finding.
8. Merge, delete the branch with `gh api -X DELETE`, remove the worktree,
   comment the outcome on the story, update memory.

## What has actually cost time here

- **`Instance::kind` carries the sub-kind — there is no separate tag.** It used
  to be two fields whose values collided, and #580's shader painted a shadow
  from the solid-fill table. #582, #583 and #584 each add a kind; map by an
  exhaustive `match`, never `enum as u32`, so a reorder in `dashpaint` is
  harmless and an addition is a compile error.
- **The uniform-fixture trap has four levels**, and v0.15 hit all four: uniform
  data, uniform _arguments_ (an argument every probe passes the same value
  for), uniform _symmetry_ (every fixture centred, so a y-flip is invisible),
  and uniform _environment_ (one canvas width, so the readback padding never
  ran; everything opaque, so unpremultiply was the identity). Before writing a
  fixture, list what the code reads and vary each axis.
- **A mutation that does not apply looks exactly like a survivor.** Five false
  readings in one session. Verify by the **absence of the original**, not the
  presence of the replacement, before reading the result.
- **A green summary is not a green build.** `just build` is four gates and only
  the first prints a summary. Capture `cmd > file 2>&1; echo "REAL EXIT: $?"`.
- **`cd` does not persist between commands.** Use `git -C <abs-path>`.
- **Check `git config --get remote.origin.url` before any fetch, reset or
  push** (debt #677).
- **Another session may be working in this repo at the same time.**
- **`corpus/showcase/tests/migration.rs` compares two independent arenas.**
  Anything flattened into a table needs resolving there in the same change.

## Stop and ask, rather than deciding alone

- A story's scope turns out to be wrong, or already done.
- A golden moves. That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass.
- A decision that binds other stories (a band threshold, a format, an ABI
  shape). Write a `docs/decisions/` record and flag it.
- Layer 4 (#586) needs a GPU and a recorded adapter; it cannot run in CI.

## When the slice is done

Close epic #569 with a summary of what landed, revise the remaining epics and
stories against what v0.15 taught before v0.16 starts, and record scope-level
changes as `docs/decisions/` records — the phase-end ritual in `AGENTS.md`.
