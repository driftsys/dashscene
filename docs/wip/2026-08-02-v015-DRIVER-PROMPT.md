# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-05, after story #584 merged
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

**That is the whole list.** The window confirmation that stood here until story
584 closed is done, and it is the first settled item below.

Everything else is closed, so do not spend time re-establishing it:

- **Both shadow kinds and render-target group opacity, on a window, and the
  shadow measured against the reference painter.** `surfaces` drawn by the lean
  painter across 12766 generations, 25795 ticks and 25465 presents, with painter
  swaps both ways at the end — no panic and no validation error. Until then both
  effects were verified only by offscreen tests asserting arithmetic.

  The shadow was then **measured rather than judged**, because on that scene it
  reads as absent. Over a fixture carrying `tile-drop-shadow`'s own parameters
  the two painters agree to **within one code point at every probe down the
  falloff**; the worst whole-frame delta, 27, is at the rounded corners and is
  the antialiasing divergence story #586 already expects. **The scene is why it
  looks absent, not the painter**: a black shadow at 0.8 alpha over a near-black
  background has **19 code points of 255** to work in. Filed as debt #738 rather
  than fixed, because both remedies move committed goldens and change what the
  showcase looks like.

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
- **Six more painter swaps on the typography scene**, after issue #715, across
  2211 generations with 2438 ticks and 2412 presents — no panic, no validation
  error, nothing refused. The heap that story introduced is bound by every one
  of those frames, and a swap rebuilds it from nothing each time.
- **The `surfaces` scene rendered through both painters and compared by eye**,
  offscreen at 960x600 — the scene's own design size. That one scene holds all
  **nine** of the showcase's gradient fills, and `typography` and `layout` hold
  none, so it is the whole of the gradient evidence. No gradient tile differed
  visibly, including the masked one. What did differ is exactly the undrawn set:
  shadows, group opacity and the frosted panel's backdrop blur.

  **By eye, and one scene at one size on one adapter.** It is not a band and does
  not pre-empt story #586. What it said at the time is that the remaining visible
  difference between the two painters lived in #583 and #584; both have landed,
  so what is left of that difference is the frosted panel's backdrop blur
  alone.

## Where things stand

`main` is at `e669256`. No open pull requests. Epic #569 tracks the slice;
`docs/roadmap.md` has the slice map. Seventeen of the milestone's twenty-two
issues are closed. **Take that pair from `gh issue list --milestone`, never from
here** — it has been wrong three times, most recently within an hour of being
corrected, because filing one issue moves it.

**Closed**, the milestone's own seventeen, in issue order:

    133  577  578  579  580  581  582  583  584  585  600  640  671  710  714
    715  716

Story #584 landed as PR #735, merged on local evidence. Story #583 landed as
PR #730 and closed issue #133 with it.

**Issue #714 was the drawable-extent fix and carried no milestone until now**,
so it was real v0.15 work filed against nothing. An earlier revision of this
prompt listed it among the milestone's closed issues while omitting #133, which
kept the count at fifteen and named the wrong fifteen. Both are fixed: #714 now
carries the milestone, and the count above is the milestone's own. **Take these
two numbers from `gh issue list --milestone`, not from this file** — the list
and the count have disagreed twice.

**Another session is working this repo in parallel.** It holds worktrees for
story #587 (`story/gpu-web-target`) and a demo backend badge. Run
`git worktree list` before assuming a story is unstarted, and read
`git config --get remote.origin.url` before any fetch, reset or push.

The painter packs the whole of boundary B into one ordered instance buffer,
evaluates its SDF math by compute shader, and draws **solid, gradient and image
fills, outline strokes, positioned glyph runs, a fill masked by a baked vector
field, render-target group opacity, and both shadow kinds** — clipped,
composited in slice order at free-path opacity, offscreen or to a window's
swapchain. **The backdrop blur is the whole of the gap**: one `InstanceKind`
variant, packed since story #578, reaching `fs_main`'s final `discard`.

Ten decision records carry the contracts. Read the ones your story touches:

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
- `docs/decisions/tables-the-vertex-stage-reads.md` — which stage may read a
  table, the test that decides it, and the second sampler
- `docs/decisions/the-paint-parameter-heap.md` — **read this before adding any
  fragment-side parameter table**: the heap, its regions, the fixed gradient
  stride, and why strokes and images stayed out of it
- `docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`
  — **read this before story #733**: the layer targets, the pass planner, and
  why a second pipeline rather than another binding
- `docs/decisions/instance-buffer-contract.md` D9 — **read this before adding a
  kind whose ink leaves its bounds**: `Instance::outset`, what the packer
  resolves into it, and why the vertex stage cannot compute it
- `docs/decisions/sub-word-members-widen-rather-than-pad.md`

## Order from the epic

**Story #733 next** — the backdrop blur, the half split out of #584 and the one
that carries the open design question. Story **#586** needs it, because it
measures the vocabulary against the reference painter and the blur is the last
thing that vocabulary is missing — and #586 needs a GPU and a recorded adapter,
so it cannot run in CI. Then **#587** and **#588**, and then the epic itself
closes.

**What #584 settled, and what it leaves #733.** The shadow closed form was
already built and already conformance-tested, so that story was wiring: the
parameters extend the paint heap as a third region at a two-word stride, and
`Globals` now carries two bases and is **thirty-two bytes**. Two records and this
prompt's own binding section claimed sixteen; all three are corrected.

**The blur inherits one thing #584 did not need and one it invented.** It
inherits the second-pipeline route (below). It also inherits the answer to a
question #584 hit first: **the vertex stage cannot read the paint heap**, which
is bound to the fragment stage alone, so anything the _quad_ needs cannot live
in a heap row. #584's drop shadow needed its spread, its blur's support and its
offset to size a quad, and that reach moved onto `Instance::outset` — the word
that used to be declared padding. The stroke's outset moved with it, so **the
vertex stage now reads three storage buffers of four** and binding 4 is
fragment-only. A backdrop blur's quad will have the same question; ask it early.

**Measured for #584, and still true for #733:** `surfaces` packs 2 drop shadows,
1 inner and 1 backdrop across 95 instances; `typography` (380 instances) and
`layout` (28) have **none of any kind**. So one showcase scene exercises the
whole of this, and one backdrop instance cannot falsify a stride or a row.
Build the fixtures with two rows differing in every field rather than reaching
for the corpus.

**What remains undrawn is exactly one thing**: the backdrop blur. It is the
`Backdrop` instance kind, packed since story #578, reaching `fs_main`'s final
`discard`. Nothing else in the v0 paint vocabulary is missing.

Render-target group opacity was never an instance kind at all — it rides on
`Instance::layer` — and story 583 drew it. Story 584 drew the two shadow
kinds.

**The scope check has now paid off twice, so do it again for #733.** Story 583's
body described clips as work to be done and `git log -S clip_coverage` put that
work in story 580's commit; the story was retitled before any code was
written. Story #584's body was checked the same way and held — the closed form
really was built, and the gap really was that no binding carried the table. One
command settles it either way. Spend it.

**Two things checked for #733 already, so you do not have to.** Its body cites
issue #422 as though it were pending — "the `blur-falloff` oracle band splits
into a residual and a gate under issue #422 … read it before tuning anything".
**#422 is CLOSED**; read its resolution rather than waiting on it. And its sigma
claim holds, but the constant has moved: it is
**`dashpaint::BLUR_SIGMA_PER_RADIUS`** since #584, cited by both painters, and
`dashscene-gpu` applies it in `pack::blur_sigma` when it writes a row. Do not
restate `0.4375` a third time.

**And one thing #583 did _not_ give #733, despite the story body saying it
would.** The body says backdrop blur reuses "S15.7's compositing machinery
rather than a parallel path". The second-_pipeline_ route transfers exactly —
see the section below. The _layers_ do not: a backdrop blur has to **read what
is already in the destination**, and a texture cannot be a render attachment and
a sampled binding in the same pass. `composite::plan` has no step that resolves
or copies the current target, and `Step` has two variants with no third. That is
a real gap between what the prerequisite delivered and what this story needs —
which is the fourth item under **Stop and ask** below, and worth raising before
building rather than after.

**#587 depends only on #585 and could start at any time**, if there is a reason
to parallelise. #588 is last by design.

**What remains undrawn is exactly three things**, all of them #584's: the two
shadow kinds and the backdrop blur. All three are `InstanceKind` variants —
`ShadowDrop`, `ShadowInner`, `Backdrop` — that reach `fs_main`'s final
`discard`, and the packer has emitted all three since story #578. Nothing else
in the v0 paint vocabulary is missing.

The fourth thing on this list until story #583 was render-target group opacity,
which was never an instance kind at all: it rides on `Instance::layer`, and
nothing read that field. It draws now.

**The scope check paid off, so do it again for #584.** Story #583's body was
titled "clips and group opacity" and described clips as work to be done. That
work was already done — `clip_coverage` in `paint.wgsl`, and
`git log -S clip_coverage` put it in story #580's commit. The story was
retitled before any code was written, which is the first time in this slice the
miss was caught before the close rather than three closes later. One command
settled it. Spend that command on #733.

**Two things checked for #584 already, so you do not have to.** Its body cites
issue #422 as though it were pending — "the `blur-falloff` oracle band splits
into a residual and a gate under issue #422 … read it before tuning anything".
**#422 is CLOSED**; read its resolution rather than waiting on it. And its sigma
claim holds: `FIGMA_BLUR_SIGMA_PER_RADIUS = 0.4375` is real, but it lives in
`crates/dashscene-skia/src/lib.rs` and nothing shares it. A second painter
restating that number is a constant stated in two places with nothing holding
them together — the exact shape the scale-mode and gradient-kind tests exist to
catch. Pin it across the two, or share it.

**And one thing #583 did _not_ give #584, despite the story body saying it
would.** #584's body says backdrop blur reuses "S15.7's compositing machinery
rather than a parallel path". The second-_pipeline_ route transfers exactly —
see the section below. The _layers_ do not: a backdrop blur has to **read what
is already in the destination**, and a texture cannot be a render attachment and
a sampled binding in the same pass. `composite::plan` has no step that resolves
or copies the current target, and `Step` has two variants with no third. That is
a real gap between what the prerequisite delivered and what this story needs —
which is the fourth item under **Stop and ask** below, and worth raising before
building rather than after.

Debt #133 is closed, on a measurement rather than an argument: the deepest clip
chain anywhere in the corpus is **3**, and the ancestry duplication it named
costs **19 boxes — 608 bytes** in total. `ClipTable`'s doc comment carries the
numbers now.

**That accident is worth remembering, because it happened twice.** Nobody drew
strokes (#710) and nobody drew gradients (#715), both because a sentence in
`render.rs` and in `pipelines-and-layer-3.md` named the owning story and the
sentence was wrong. Both are corrected. **When prose tells you which story owns
something, check it against that story's own body before believing it.**

Open debt worth knowing, none of it blocking: **#708** (`pack::pack` still walks
every rect, so R-T4's CPU half is unmet), **#703** (`cargo doc` runs nowhere),
**#718** (the lean painter declares it cannot sample JPEG or GIF), **#720** (a
payload larger than the atlas panics rather than getting its own texture —
widened by story #582 to cover glyph atlases and baked-vector atlases, and a CJK
sheet is the likeliest of the three to exceed 2048 square), **#724** (a glyph
atlas with `px_per_em` of zero divides unguarded, where every sibling degenerate
case is named), **#729** (a clipping node interns its outgoing clip region
before any child is known to paint, so a frame with no painting descendants
leaves an orphan — 26 of the corpus's 55 stored boxes, uploaded every frame).

**Issue #727 is filed and deliberately unscheduled** — a backend implementation
guide with a worked example painter, for this epic's phase-end revision. Do not
start it inside a story; it is scope for the revision to place.

## The fragment stage is full, and the heap is where a table goes now

`wgpu::Limits::downlevel_defaults` allows **four storage buffers per shader
stage**. The pipeline binds seven, and story #584 moved one of them:

    vertex    instances(0), glyph runs(8), shapes(9)               3 of 4
    fragment  paints(1), clips(2), strokes(4), images(5)           4 of 4

**The vertex stage has one slot free**, which it did not before. The stroke rows
were bound to both stages so that stage could size a stroke's quad; story #584
needed the same growth for a shadow, whose parameters are in the fragment-only
heap, so the growth moved onto `Instance::outset` and the stroke table left the
vertex stage with it. A free slot is not an invitation —
`docs/decisions/tables-the-vertex-stage-reads.md` D4 says why a value that fits
on the instance is better than a table that needs a binding.

**Binding 1 is no longer the solid table.** Since issue #715 it is the
**paint-parameter heap**: one `array<vec4f>` holding the solid colours at base
zero, then the gradient rows at a fixed twelve-word stride, and since story 584
the shadow rows at a two-word stride, each region's base travelling in the
per-frame uniform. `Viewport` is renamed `Globals`; it carries two bases
now, which took it to **thirty-two bytes**, since five scalars is twenty and a
uniform rounds up to a multiple of sixteen.
`docs/decisions/the-paint-parameter-heap.md` is the record.

**So the answer for the next fragment-side table is: extend the heap.** Do not
go looking for a free binding on that stage — there is not one, and there will
not be one. Strokes and images were deliberately left out of the heap because
folding them in frees nothing; a new region costs one more base, and a base can
travel in `Globals` beside the two already there.

**And the answer for anything that has to sample a rendered target is: a second
pipeline.** Story #583 needed to read a layer texture and had no binding to read
it through, so it did not try — a pipeline owns its own bind group layout, and a
separate one costs the paint pipeline nothing at all. `shaders/composite.wgsl`
is the whole of it: its own `@group(0)`, a texture and a uniform, no sampler,
`textureLoad` at the fragment's own pixel. **Story #733's backdrop blur takes
this route**, and so does anything after it that reads pixels rather than
parameters. The two answers do not compete: the heap is for per-instance
_parameters_ the fragment stage indexes, and a second pipeline is for _pixels_
that have already been drawn.

**Story #582's route is still available for a table that qualifies**, and the
test is unchanged: bind to the vertex stage and pass the values across in
`VertexOut`, but **only when every value a fragment needs of that table is
constant across the instance**. A glyph run's colour and range are; a coverage
mask's plane, rectangle and range are. A gradient's stop array was not — it is
indexed by a value the fragment computes from its own coordinate — and that is
why the heap exists. `docs/decisions/tables-the-vertex-stage-reads.md` D2 states
that test. **The vertex stage has a slot free again since story #584** — the
stroke table left it — so that route costs a binding there rather than only a
varying. It is still the second choice: a value that fits on `Instance` costs
neither, which is what #584 did with the outset.

**Story #583's group opacity was constrained by something else, and the guess
here was right.** It needed a second render target and an offscreen pass rather
than another parameter table, so the storage-buffer count was not its limit. The
second-pipeline paragraph above is what that story settled, and it is now a fact
rather than an expectation.

**This paragraph has been wrong twice, in opposite directions.** An early draft
of the residency record claimed one free slot when there were none. Then the
record written by story 582 claimed the varyings were counted against "the sixty
`downlevel_defaults` allows" — wgpu 30 has no such field at all, the real limit
is `max_inter_stage_shader_variables` at **15**, it counts `@location` slots
rather than float components, and `VertexOut` uses **9 of 15**. Both were caught
by review rather than by the compiler, because a wrong number in prose compiles.
Issue #715's own review re-read both figures out of the pinned crate and they
held, which is the only reason this section is trustworthy today. **Read the
limit out of the pinned crate before trusting any figure here:**

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
  Read-only held across all five this time, and again across all five on #715.

**Issue #715 ran the same fan-out and the result had a different shape, which is
worth knowing before the next one.** Two findings, **both prose, zero code
defects across five agents** — a decision record misquoting the record it cited,
and a shader comment naming a test that does not exist. The mutation pass had
already taken the code defects. So the two instruments are not redundant and
they do not overlap: **mutation finds what the code does wrong, the fan-out finds
what the prose claims wrongly.** Run both; do not treat a clean mutation pass as
a reason to skip the review, and do not expect the review to find arithmetic.

**Story #583 ran it a third time and the shape held: six findings, and not one
of them was a defect in the rendering path.** Five were prose or unfalsifiable
tests; the sixth was a miscount in a record written that same day.

**Story #584 ran it a fourth time and the shape held exactly: seven findings,
all prose, zero code defects.** Every one was a sentence the story's own change
had falsified and left standing — a binding table in the file whose binding
moved, a crate status line, a WGSL comment calling a field "the trailing pad"
after it stopped being one, a size assertion citing a renamed symbol. **Two of
the seven were recurrences of PR #730's findings about the same two files**
(`lib.rs`'s per-story paragraph, `demo/src/present.rs`'s "packed and not
drawn"), which says the sweep after a change should start from the previous
story's finding list.

So across four stories the fan-out has found zero arithmetic and a great deal of
wrong prose, which is worth knowing when deciding what to spend the review on.
**The cheap defence is a grep, before the review**: after changing a field name,
a binding, a byte count or what the painter draws, grep the tree for the old
_token_ — not the concept — and read every hit.

**Two of #583's six were repeats of issue #719's own findings**, and that is the
part to carry forward. This crate has two standing test obligations that are
easy to miss twice:

- **A new term in `Renderer::allocations` is unfalsifiable unless some fixture
  makes it non-zero.** `frame_path.rs` passes `&[]` for groups at every call
  site, so deleting the layer term passed the entire suite. Difference the same
  scene with and without the feature; do not assert on one absolute number.
- **Every Rust struct read by WGSL carries a
  `const _: () = assert!(size_of::<T>() == N)`.** `GpuComposite` had none, and a
  `vec3f` pad had already made it 32 bytes once during the branch — wgpu says
  "bound with size 16 where the shader expects 32". A `vec3f` aligns to 16; use
  scalars.

The prose half is not cosmetic in this repository. A record is what the next
story reads to decide its approach — the binding section above misled twice for
exactly that reason — so a wrong citation is a wrong input to a decision, not a
typo.

## What has actually cost time here

- **`Instance::kind` carries the sub-kind — there is no separate tag.** Story
  #733 adds no kind — `Backdrop` has existed since #578 — but map every
  discriminant by an exhaustive `match`, never
  `enum as u32`, so a reorder in `dashpaint` is harmless and an addition is a
  compile error. `stroke_align`, `scale_mode` and `gradient_kind` in `render.rs`
  are the pattern. `scale_mode` and `gradient_kind` are also pinned against the
  shader's own source text by a test, which is what a constant stated in both
  languages needs, because nothing in either language holds the two together.
  `stroke_align` has no such test — a real gap, and the cheapest one in this
  crate to close.
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
- **Measure before pinning a layout, and be willing for the answer to be
  "neither".** Story #583 measured twice and both measurements cancelled work.
  Issue #133's quadratic clip ancestry: deepest chain in the whole corpus is
  **3**, total cost **608 bytes**, and the parent-pointer fix would have made
  the shader _slower_ — closed won't-fix. The group nesting: **one group, depth
  one, one scene**, which collapsed a per-layer-texture versus pooled-texture
  design fork that was about to cost real effort. Both measurements took one
  throwaway test each.
- **Probing for one number finds neighbouring ones.** The #133 probe turned up
  something the issue never named — 26 of the corpus's 55 stored clip boxes sit
  in regions no rect resolves to, because a clipping node interns its outgoing
  region before any child is known to paint. That is now #729, and it is the
  larger of the two effects.
- **A mutation can be caught for the wrong reason, which leaves the thing you
  meant to test unproven.** Widening `GpuComposite` failed to compile — on the
  _initialiser's_ type, not on the new size assertion. Mutating both is what
  made the assertion itself fire (`E0080`). Check _which_ check caught it, not
  merely that something did.
- **The uniform-fixture trap has four levels**: uniform data, uniform
  _arguments_, uniform _symmetry_, uniform _environment_. Before writing a
  fixture, list what the code reads and vary each axis.
- **One row cannot falsify a stride.** Any layout addressed as
  `base + row * stride` is read correctly for row 0 at _every_ stride, because
  row 0 sits at the base whatever the multiplier is. Issue #715's whole gradient
  suite passed with the stride multiplied by anything until a second row was
  added. **Two rows are the minimum for any indexed layout**, and the second one
  must differ from the first in every field, or a wrong row still reads
  plausibly. The same argument applies to a table with one entry, an atlas with
  one payload, and a group with one member — which #583 will have to think about.
- **A branch can be unobservable because a later iteration overwrites it.**
  `gradient_segment_t`'s hard-stop answer only matters when the zero-width
  segment is the _last_ one the ramp walks, since the walk keeps overwriting. A
  fixture with the repeated offset in the middle of four stops could not tell
  `1.0` from `0.0`, and only mutation said so. Before trusting a fixture over a
  loop, ask which iteration's result actually survives.
- **A mutation that does not apply looks exactly like a survivor.** Verify by
  the **absence of the original**, and compare the **whole block** — `grep -F`
  matches a multi-line pattern line by line, so a common line such as
  `continue;` inside it reports "still present" for a mutation that applied.
- **Commit before mutation testing, and never revert a mutation with
  `git checkout --` against uncommitted work.** Story #584's mutation script
  reverted each mutation that way with the story still uncommitted, and the
  third revert **destroyed three source files** — the packer, the renderer and
  the shader — leaving a tree that still compiled from the files that survived.
  The evidence it had already produced was worthless too, because every run
  after the first ran against a half-reverted tree. Commit first; then a revert
  is free and a mutation is a diff you can see.
- **A probe that is outside the shape proves nothing about which shape.** #584's
  corner probe sat outside the drop shadow at the spread radius _and_ at the
  unspread one, so removing the spread from the corners survived. Choose the
  probe by computing where the two candidate answers **differ**, not by finding
  somewhere the correct answer is zero.
- **The quad clips the probe before the coverage does.** An inner shadow's quad
  is its own bounds plus the antialiasing width, so #584's "nothing outside the
  node" probe two units out was discarded by the geometry whatever the shader
  did — and a shadow that never clipped itself to the node's shape passed. The
  only place that clip is observable is _inside_ the quad and _outside_ the
  shape: half a unit out, not two.
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
- **A test that compares two independent arenas must resolve every index
  first** — a row index means nothing outside the table that assigned it. The
  rule lives in `docs/decisions/cross-arena-comparison-resolves-indices.md`.
  Its former worked example, `corpus/showcase/tests/migration.rs`, was deleted
  in commit `535b547`; the comparisons that still exist are `assert_same_output`
  in `crates/dashlang/tests/builder.rs` and in `crates/dashlang/tests/paint.rs`,
  `assert_dsl_matches_hand_built` in `goldens/tooling/tests/v02_flex.rs`, and
  the `via_taffy`/`via_fixed` rect equalities in
  `crates/dashscene-engine/tests/solve.rs`.
- **markdownlint reads a line-initial `#123` as a heading.** dprint reflows the
  paragraph, so the safe fix is to reword rather than to move the number. It
  caught this prompt twice more while issue #715 was being written up.
- **An indented block in a Rust doc comment is a doctest, and it will fail the
  build.** A layout diagram written as four-space-indented lines under `///`
  compiles as Rust. Fence it as `` ```text ``. `just build` catches it, but
  only at the doc-test gate, which is the last of the four.

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
  separate commit. **Story #733 is very likely the next instance**: its body
  says it reuses #583's compositing machinery, and #583 built no way to read the
  destination — see the paragraph under "Order from the epic". Story #584 was
  one too, in a smaller way: it needed a word on `Instance` that no prerequisite
  had made available, and that was agreed with the owner before any code was
  written rather than decided inside the story.
- Layer 4 (#586) needs a GPU and a recorded adapter; it cannot run in CI.

## When the slice is done

Close epic #569 with a summary of what landed, revise the remaining epics and
stories against what v0.15 taught before v0.16 starts, and record scope-level
changes as `docs/decisions/` records — the phase-end ritual in `AGENTS.md`.
