# v0.14 + v0.15 — the showcase runtime and the wgpu painter

    status   WIP — design + work breakdown, NOT yet ratified. Produced
             2026-07-29 in a design session (user + Opus). No issue has
             been filed and no crate has been created. The painter-strategy
             decision this assumes is recorded nowhere yet; see
             "Decisions this needs" below.
    scope    what comes after v0.13: a windowed showcase runtime (v0.14),
             then a wgpu painter covering native and web (v0.15); which
             open debt must land before v0.14; which debt v0.14 makes
             measurable for the first time
    builds on docs/wip/2026-07-19-wgpu-painter-direction.md (ecosystem
             research, ruled-out crates, helper stack — not repeated here),
             docs/decisions/backend-tiering-unity-skia-lean.md,
             docs/specification/03-target-hardware-rules.md (R-T4, R-T5),
             docs/decisions/pre-v1-hardening-slice.md (the precedent for
             inserting a slice rather than growing v1)

## The premise

Nothing in this repo has ever drawn a frame into a window. There is no
`winit` dependency, no event loop, no surface, and no examples; the only
two binary targets are `dashc` and `dashpack`, both command-line tools.
Every pixel the project has produced is an offscreen raster compared
against a PNG.

That is the gap v0.14 closes. It is also why the painter question
("Skia or wgpu first?") turns out not to be a choice — see "Why this is
not a painter choice".

## What already exists, and what that does to the estimate

`dashlang::reactive::LiveScene` (`crates/dashlang/src/reactive.rs`, 1385
lines) is already the per-frame driver:

    LiveScene::tick(&mut self, dt: f32, arena: &mut Arena) -> u64

with `Signal<T>` and `set`, `map` / `scale` / `map_range` / `clamp` /
`format`, `Spring::critically_damped`, and per-node `bind(channel, expr)`,
`smooth(channel, spring)`, `bind_text`, `visible_when`. `attach_live`
wires it to an arena and a `LayoutSolver`.

`dashcue` (494 lines) holds the animation vocabulary and its scheduler,
and **all thirteen of its debt issues are closed** — issues #68 to #77,
plus issues #208, #214 and #488.

So the animation runtime is built and its debt is paid. v0.14 adds a
window, a clock, a present path, and an input pump around a function that
already works. This is a small slice, not a large one.

## Why this is not a painter choice

A demonstration is mostly presentation — window, swapchain, present,
clock, event pump. Skia provides none of that; it is a rasteriser with no
windowing, so a Skia-presented demo means bringing `winit` plus a GL
context plus Ganesh, against a dependency the project already resolved to
"pin deliberately" while Ganesh moves to Graphite
(`docs/technotes/rendering-and-painters.md` §6).

Against that, `dashscene-skia` already draws the entire v0 vocabulary and
a wgpu painter draws none of it.

Boundary B resolves the tension rather than forcing a decision. The shell
— window, clock, input, `tick`, present — sits above the painter and is
painter-agnostic by construction, which is the property boundary B exists
to provide ("painter swap = re-golden, not redesign",
`docs/design/architecture.md`). So:

- v0.14 ships the shell and presents through Skia by blitting its CPU
  raster to the window. No GL context, no Ganesh, no trim profile.
- v0.15 builds the wgpu painter behind the same shell, which becomes its
  development harness: each primitive is watched as it lands rather than
  diffed as a PNG, and both painters can run against one document and one
  animation clock.

**One honest limit.** A Skia CPU raster of the whole window every frame
is unlikely to hold 60 frames per second for a complex scene. v0.14
therefore produces a real frame budget, but a _CPU-raster_ frame budget:
enough to validate the loop and to expose per-frame allocation defects,
not enough to characterise GPU frame cost. The representative budget
arrives with v0.15. This matters to the debt argument below and is not
glossed over there.

## The debt check — the question that was asked

Three groups, and the middle one is the answer.

### Already paid, no action needed

- **#197** — retained paint and clip interners grew without bound, one
  entry per animated frame for the arena's life. This was the single worst
  defect for a long-running demonstration. **Closed in v0.13.**
- **#98** (iterative build/readback for deep trees), **#287** (mask bounds)
  — closed in v0.13.
- **All dashcue debt** — closed, listed above.

### Must land before v0.14, or the demonstration is not credible

Each of these sits on the exact per-frame path the host will call. All are
open, and all are currently parked in the v1 milestone behind epic #476's
measurement gate.

- **#101 — `SkiaPainter::paint` decodes every image asset once per
  image-filled rect, on every call.** The issue states its own trigger:
  "This is the reference painter and paint is effectively one-shot for
  goldens, so the cost is not on any hot path today. **When incremental
  painting or a real frame loop lands**, decode each `ImageTable` index at
  most once per `paint()`." v0.14 is that frame loop. Any showcase scene
  containing an image re-decodes its PNGs sixty times a second. This is
  the hardest blocker in the list and the fix is scoped in the issue.
- **#191 — `LiveScene::tick`'s no-solve path clones the full rect vector
  every frame.** `CachedSolver { rects: self.cached_solve.clone() }`, an
  O(nodes) allocation inside the exact function the shell calls once per
  frame, on the path taken by _most_ frames (a no-solve tick is the common
  case). It is the per-frame allocation most directly created by v0.14.
- **#278 — group composites are rebuilt from scratch every commit** and
  are outside the dirty-set instance-buffer model. A showcase exercising
  group opacity or backdrop blur rebuilds its render targets every frame.
  Tolerable at low scene complexity, and the fix is the same retention the
  issue already prescribes for "the v1 incremental painters".
- **#205 — `VariantFlip::advance` prunes targets in
  O(animating_nodes² · channels) per frame.** Bounded, so R4 holds, but a
  showcase whose point is animating many nodes at once is precisely the
  input that makes the bound bite.
- **#225 — `BidiInfo::new` reruns the full UAX #9 resolution on every
  `layout()` call**, ahead of the shaped-run cache lookup, and "the engine
  measure callback calls `layout()` several times per text node per Taffy
  solve". S14.5 requires Arabic text in the showcase and signal-driven text
  that changes content, so every solve repays full bidi resolution several
  times per text node. The issue also records a second defect worth fixing
  in the same pass: the doc comment above the cache lookup claims one entry
  serves every layout of the paragraph, which the code does not do for the
  bidi step. **#226** (`position_line` clones the paragraph level vector
  once per line) sits in the same file on the same path and should be
  bundled into this story rather than moved separately.

### Moved to v0.15, not v0.14

- **#133 — `intern_region` copies each clip region's whole ancestor
  chain**, so a chain 8 deep stores 36 `ClipBox` values for 8 distinct
  boxes. This is `dashpaint` data, which means boundary B, and the wgpu
  painter's S15.7 is its second consumer. The issue's own proposed fix names
  the reason to wait: "an `Rc<[ClipBox]>` shared prefix, **or a
  parent-pointer chain the painter resolves once**". Which of those is right
  depends on how painters consume it, and choosing with only Skia in the
  room risks a representation that suits one painter. Fold it into S15.7,
  where there are two consumers to design against.

### v0.14 changes the justification for a larger class

This is the structural finding, and it is worth recording independently of
the slice.

Epic #476 holds twenty perf and allocation items behind an explicit entry
condition: "Do not start these before v1's performance pass has produced a
profile against target hardware." Its argument is sound and stated as
**"resolvable is not the same as measurable"** — fixing one today yields a
change whose only success criterion is that the tests still pass.

**v0.14 creates the project's first frame budget.** That does not move
epic #476 wholesale, and it must not: the budget v0.14 produces is a
desktop CPU-raster budget, not the target-SoC budget that epic is waiting
for. But it does two things:

1. The items above stop being unmeasured optimisations and become
   prerequisites, because the demonstration is their measurement.
2. A further set becomes _measurable_ for the first time, on the loop
   rather than on target hardware: issues #60, #138, #184 and #199
   (commit-path allocation, paid every frame), #273 (grid-template vectors
   rebuilt on every incremental restyle), #323 (the baseline-correction walk
   when no Baseline row exists), and #206 (per variant switch, so a lower
   priority than #205). None is a blocker; each gets a number it never had.

### Checked and deliberately left in v1

Recorded so the reasoning is not re-derived.

- **#374** — the weight-substitution log is unbounded and dedups in O(n).
  This looked like #197's shape, an unbounded structure growing inside a
  frame loop, and it is not: the measurement in the issue (801 distinct
  weight requests producing 799 entries) shows growth is bounded by the
  count of _distinct weight values the document requests_, not by elapsed
  frames. A showcase requests a handful. It stays in v1.
- **#418** (dashc `push_asset` dedup) and **#80** (dashlang double name
  allocation) are compile-time and scene-construction-time respectively, so
  a frame loop does not reach them.
- **#231** and **#230** need pathological bidi input that no showcase scene
  will contain.
- **#285** (zero-alpha shadow early-out) is Skia-specific and stays — but
  S15.8 should implement the early-out natively rather than reproduce the
  defect in a second painter.
- **#278**'s retained group composition is being designed in v0.14 for
  Skia, and the issue frames it for "the v1 incremental painters", of which
  `dashscene-wgpu` is one. S15.7 and S15.8 reuse that design rather than
  inventing a second one.

The recommended handling is to pull the four prerequisites into v0.14 as
ordinary stories, and to leave the second set in #476 with its entry
condition amended: it now reads "profiled against a frame budget", of
which v0.14's is the first and v1's target-hardware pass the
authoritative one. That keeps #476's discipline while acknowledging its
instrument arrived earlier than expected.

## Decisions this needs recorded first

Neither slice should start before these exist in `docs/decisions/`. All
three are strategy, not implementation.

1. **The painter strategy.** `docs/wip/2026-07-19-wgpu-painter-direction.md`
   deliberately declined to make it: "Adopting wgpu is a painter-strategy
   decision … It should be made on those grounds and recorded in
   `docs/decisions/`, not inherited as a side effect." The decision to
   record: wgpu becomes the named product painter for web and for the
   entry tier _candidate_ slot, Skia keeps the entry-tier bridge until
   wgpu is measured on a real entry SoC, and Skia is permanently the
   bit-exact CPU oracle. This **amends** rather than supersedes
   `backend-tiering-unity-skia-lean.md`, whose sequencing ("ship entry on
   trimmed Skia, measure on the real entry SoC, build the lean painter
   only if trimmed Skia busts the budget") this respects.
2. **The wgpu painter's crate identity — resolved: `dashscene-wgpu`.**
   `dashscene-web` was the reserved name for a wasm/tiny-skia painter, and
   one wgpu painter covers native and web, so that name no longer describes
   the component. The record amends
   `docs/decisions/crate-name-map.md` to add `dashscene-wgpu` and to retire
   `dashscene-web`. Unlike `demo/`, this is a **published** crate, so the
   name also needs reserving on crates.io — the same precaution that
   produced the twelve originally-squatted names
   (`docs/decisions/repo-staging-and-public-facade.md`). Reserving it is a
   task in S15.1, not an afterthought.
3. **How the runtime is driven — resolved: the runtime never reads a
   clock; hosts pass a clamped `dt`.** Two halves, and the first is an
   invariant that already holds by accident and should be pinned before it
   is broken. See "The `dt` ruling" below.

One correction to carry into the strategy record: **the Skia C++
dependency does not leave the workspace.** Skia is permanently the
bit-exact oracle, so `skia-safe` stays. What wgpu retires is the _trim
profile_ — the from-source GLES build, `skia_use_gl`, and the
Ganesh-to-Graphite churn watch. That is a real simplification and a
smaller one than "drop Skia".

### The `dt` ruling

**Variable `dt`, clamped at the host. No accumulator.**

The question looked like "fixed or variable timestep" and is not, because
`dashcue` already solves the part that usually forces a fixed step.
`dashcue-spring-uses-semi-implicit-euler.md` states that `advance(dt)`
steps "in equal substeps below the stability bound
`h < 1 / ((2ζ + 1)·ω)` — a frame-scale `dt` within the bound is a single
step; a frame hitch splits into several, **so the integration cannot
diverge**." A host-side accumulator would reimplement, one layer up, the
substepping the scheduler already performs.

Reproducibility needs nothing either. `LiveScene::tick(&mut self, dt: f32,
arena: &mut Arena)` takes `dt` as a parameter and reads no clock, so a test
passes an explicit sequence and never involves a host; the same record
pins the per-step arithmetic as IEEE basic operations, so an identical
sequence is bit-identical across machines. **R4's reproducibility clause is
already satisfied**, and the property that satisfies it is currently an
implementation accident rather than a stated invariant. Pin it: no crate at
or below `LiveScene` may read a clock.

What is left is one guard. R4 also requires _statically provable frame
cost_, and substep count scales with `dt`, so an unbounded `dt` — a
debugger pause, an alt-tab, an operating-system deschedule — is an
unbounded substep burst. The host clamps the `dt` it passes (100 ms is a
reasonable ceiling, roughly six frames at 60 Hz). The failure mode becomes
"animation falls behind wall-clock time", which is the correct one for a
cockpit: late is better than wrong.

Deliberately not solved: the residual stutter a fixed step would have
introduced is absent here, so the usual remedy — rendering an interpolated
state between two simulation states — is not needed. It would also not be
cheap, because the committed output is a solved rect table and
interpolating two rect tables is neither a boundary-B operation nor
compatible with P1.

## v0.14 — work breakdown

Goal: the system is seen. A window, an animated scene exercising the full
v0 paint vocabulary, driven by `LiveScene::tick`, presented through a
painter-agnostic seam.

Not in scope: any GPU painter, any web target, any performance claim
beyond "the loop runs".

### The demonstration is run, not asserted

**Resolved: CI proves the demonstration builds. Nothing more.**

It is not a golden producer, not a coverage gate, and not a fidelity
check. Its value is that a person watches it, and no CI assertion
substitutes for that. This is deliberate rather than a gap: a demonstration
wired into CI would become a suite whose green state reads as evidence of
correctness it never established — the `t2-check-has-no-teeth` failure the
v0.13 tiering exists to remove. Better to claim nothing than to claim
something unearned.

Consequences: `cargo build -p demo` is the CI job, there is no
`tests/coverage.rs`, and vocabulary coverage is a written checklist in the
slice's own record that a person confirms by running the thing. The frame
budget is a recorded measurement taken by hand, not a threshold.

The wgpu painter's own tests (v0.15, layers 1 to 3) are unaffected — those
test the painter, not the demonstration, and they carry the whole
verification burden.

### Structure

    demo/                     new workspace member, not a published crate
      src/shell.rs            window, clock, event pump, frame loop
      src/present.rs          the Present seam + the Skia blit implementation
      src/main.rs             scene selection, .dsb loading

    corpus/showcase/          the showcase scenes themselves

A workspace member rather than a sixteenth published crate, following
`goldens/tooling`. Nothing in `demo/` is published, so only
`dashscene-wgpu` (v0.15) touches
`docs/decisions/crate-name-map.md`.

**The scenes live in `corpus/`, not in `demo/`** — resolved. They exercise
the full vocabulary, which is exactly what the stress corpus is for, so two
parallel scene sets would be duplication of the kind that drifts. `demo/`
holds the host; `corpus/showcase/` holds the content, and the existing
corpus tooling keeps working against it.

### Stories

**S14.1 — the per-frame debt prerequisites.** Five pull requests, each with
its own before-and-after measurement:

    101   image assets re-decoded per rect, per paint
    191   LiveScene::tick clones the full rect vec every frame
    278   group composites rebuilt from scratch every commit
    205   VariantFlip::advance prunes in O(animating² · channels)
    225   full UAX #9 bidi resolution repeated per layout() call
          (bundles 226, the per-line paragraph-level clone)

Sequenced first because every later story runs on this path. Each is a move
out of the v1 milestone into this one, so each needs its entry in epic #476
re-attributed rather than silently dropped.
_Depends on: nothing. Blocks: S14.3._

**S14.2 — the `Present` seam, and the Skia blit behind it.** A `Present`
trait local to `demo/` (never in `dashpaint` — boundary B stays free of
presentation concerns), plus the implementation that takes
`dashscene-skia`'s CPU raster and blits it to a `winit` window through
`softbuffer`. No GL context and no Ganesh.
_Depends on: nothing. Blocks: S14.3._

**S14.3 — the frame loop.** `winit` event loop, a clock producing `dt`
clamped to 100 ms, `LiveScene::tick(dt, &mut arena)`, `arena.committed()`,
present. P3 holds by construction: the host owns time and nothing
producer-side runs inside the loop. Includes the resize path. Also lands
the clock invariant as a test — no crate at or below `LiveScene` reads a
clock — so the property R4's reproducibility rests on stops being an
implementation accident.

**An idle frame neither paints nor presents**, and this needs no new API.
`LiveScene::tick` already returns the generation and already holds it
steady on an idle frame: `crates/dashlang/src/reactive.rs:819` skips the
commit when the scheduler is settled and no signal is dirty, and its
comment names the reason — keeping the generation "a meaningful
'something changed' signal for a downstream consumer". The shell is that
consumer. The loop records the generation it last painted and skips both
`paint` and present when `tick` returns the same value. No new flag, no
new return value, and no change to `dashpaint` or boundary B.

The generation reports document and animation change only, so the shell
forces a redraw independently of it for the first frame, a resize or
surface reconfigure, a scale-factor change, a lost surface or recreated
swapchain, and re-exposure after occlusion on platforms that do not
preserve surface contents.

The reason this is a frame-loop requirement and not an optimisation:
no painter has a partial-redraw path. `DirtyMode::Retained`
(`crates/dashscene-skia/src/lib.rs:32`) patches the retained instance
buffer from the dirty set and then redraws every quad, which is the
behaviour a product painter is meant to have (R-T1 prefers a full pass
over reloading the previous framebuffer into tile memory). A static
screen therefore costs a full frame of fill, every frame, and skipping
the frame is the only thing that removes that cost. It also decides the
event loop's wait mode: the loop polls at the frame rate while the
generation advances and waits for input while it is steady, rather than
waking sixty times a second to redraw an unchanged screen. A signal
producer outside the loop consequently needs a way to wake it; for the
showcase's internally scripted signals that is a `winit`
`EventLoopProxy`, and naming it is part of this story.
_Depends on: S14.1, S14.2. Blocks: S14.4, S14.5, S14.6._

**S14.4 — input to signals.** `winit` events mapped onto
`LiveScene::set` and variant switches. Deliberately small: pointer
position and a few keys onto named signals, one key cycling a variant set.
Enough to make the demonstration interactive, not a general input system.
_Depends on: S14.3._

**S14.5 — the showcase scenes, in `corpus/showcase/`.** Authored in
`dashlang` against the reactive API. Must cover the full v0 paint
vocabulary: solid and gradient fills, strokes and stroke alignment, corner
radii, images, MSDF text in Latin **and** Arabic (bidi), baked vector MSDF
fields, clips, group opacity, masks, shadows, backdrop blur — plus flex
layout, variants, FLIP, springs, keyframes, and signal-driven text.
Coverage is a written checklist confirmed by running the demonstration, not
a test. Includes a pass over the existing corpus for scenes that can be
reused rather than re-authored.
_Depends on: S14.3._

**S14.6 — load a `.dsb` in the host.** The same host, pointed at a
compiled document rather than a `dashlang` scene, so the hero Figma import
can be seen animating. Uses the existing loader; no format work.
_Depends on: S14.3._

**S14.7 — the README, seeded by the demonstration.** The repository has no
`README.md` at all. Write it now, pointing at `cargo run -p demo`, with a
still captured from S14.5. Also the book's `overview.md`, which is one of
only three files in `docs/book/`. Scoped to the entry path — what this is,
why it exists, how to run it — and explicitly not "make all documentation
consistent", which has no definition of done.
_Depends on: S14.5._

### Definition of done for the slice

- The demonstration runs on macOS and Linux, animates, and accepts input —
  confirmed by a person running it, which is the whole point.
- `cargo build -p demo` green in CI. That is the only CI claim this slice
  makes.
- The vocabulary checklist walked and signed off against a running
  demonstration.
- A frame budget recorded by hand — frame time for each showcase scene,
  with the machine and the painter named beside the number, in the same
  discipline the project already applies to golden measurements. A
  measurement, not a threshold.
- The static and animated cases measured separately. Because no painter
  redraws partially, the two differ by whether a frame runs at all rather
  than by how much of it runs, and a single averaged number hides which
  case produced it. A settled scene must show that the loop stopped
  painting, not that it painted something cheap.
- Zero goldens moved, asserted per file with `git hash-object`, not
  assumed. Nothing in this slice should change rendered output; the fixes
  for issues #101 and #278 are the two that could, and both must be checked
  explicitly.

## v0.15 — work breakdown

Goal: a wgpu painter behind boundary B, covering native and web, verified
by instruments that do not require a GPU in CI.

The ecosystem research is not repeated here. Vello is ruled out, no
drop-in crate exists, GPUI is the reference implementation, and the helper
stack is pinned — all in
`docs/wip/2026-07-19-wgpu-painter-direction.md`, which also records two
techniques to avoid (do not copy iced's smoothstep shadow; do not copy
GPUI's alpha-multiplied group opacity).

### Verification — four layers, and the line between gate and measurement

CI runs entirely on `ubuntu-latest` with no GPU. Skia goldens work because
that painter CPU-rasters. A wgpu painter has no such path, so fidelity is
decomposed so that only the smallest part needs real hardware.

R-T4 is what makes this possible: "CPU frame cost = dirty-range
instance-buffer upload from the rect table + submission. Nothing else." If
that is the painter's whole job, the instance buffer is its output and the
GPU is a pure function of it.

| Layer                                           | What it catches                                                                                                                               | GPU |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | --- |
| 1. Instance-buffer goldens, bit-exact           | wrong table-to-draw translation: dropped clip, wrong paint index, wrong z, group applied to the wrong set                                     | no  |
| 2. Shader-math conformance, compute on lavapipe | wrong SDF distance, AA ramp, MSDF median-of-3 resolve, blurred-rounded-rect closed form                                                       | no  |
| 3. Render smoke on lavapipe                     | pipeline, bind groups, formats, naga validation; coverage inside versus outside a shape; clip rejection; group opacity where contents overlap | no  |
| 4. Perceptual band against the Skia CPU oracle  | how it actually looks on a real driver                                                                                                        | yes |

Layers 1 to 3 are the gate. **Layer 4 is a measurement, not a gate**, and
layer 3 must never be described as a fidelity check — a green suite that
reads as evidence it is not is exactly the `t2-check-has-no-teeth` failure
v0.13 exists to remove.

Two supporting points:

- Compute-shader evaluation of the SDF math is far more stable across Mesa
  versions than rasterising, because it removes the rasteriser, the
  antialiasing resolve, the blend stage and the texture sampler from the
  loop. What is left is float arithmetic. So lavapipe is trustworthy for
  layer 2 and untrustworthy for layer 4.
- Layer 1 and layer 2 are **shared with the Unity painter**, which is also
  instanced SDF quads. That makes R-T5 ("SDF shader math single-sourced
  into both painters' shading languages") an executable conformance suite
  both painters run, rather than a review promise.

The residual risk is stated plainly: nothing in CI will catch "it looks
wrong on a real automotive GLES driver". Layer 4 is the only instrument
and it is not automated. It is a named gate in the slice's definition of
done, run once on recorded hardware.

### Stories

**S15.1 — the `dashscene-wgpu` crate and the strategy record.** Write the
painter-strategy decision, amend `crate-name-map.md` to add
`dashscene-wgpu` and retire `dashscene-web`, **reserve the name on
crates.io**, create the crate, and stand up an empty painter that satisfies
the `Painter` trait and draws nothing.
_Depends on: the painter-strategy decision. Blocks: everything below._

**S15.2 — the instance-buffer contract.** The `#[repr(C)]` per-instance
struct, the packer that turns boundary-B tables into it, and layer-1
goldens. Deliberately first: it is the shape Unity will share, and layer 1
is the widest part of the verification net.
_Depends on: S15.1. Blocks: S15.3, S15.4._

**S15.3 — the shader library and layer-2 conformance.** WGSL for the
rounded-box SDF and its antialiasing, gradients, strokes, the MSDF resolve
(`px_range` 4, screen-pixel range from a uniform rather than `fwidth`
derivatives, per the direction note), and the blurred-rounded-rect
shadow — Levien's closed form, validated against a real multi-pass blur
before it is trusted, since he describes his constants as empirically
tuned. Plus the compute-shader conformance harness.
_Depends on: S15.2._

**S15.4 — pipelines, bind groups, and the first pixels.** `wgsl_to_wgpu`
for typed bind groups, `naga_oil` for WGSL includes, and layer-3 render
smoke. First primitive: opaque rounded rects.
_Depends on: S15.2._

**S15.5 — atlas residency.** `etagere` plus an LRU, following glyphon's
design. Serves MSDF glyphs, baked vector fields, and image assets.
_Depends on: S15.4._

**S15.6 — text and baked vector fields.** MSDF glyph runs and
`VectorField` sampling through the shared resolve from S15.3.
_Depends on: S15.5._

**S15.7 — clips and group opacity.** Per-instance clip parameters
evaluated in the shader, following GPUI's `content_mask` rather than
iced's scissor, so batching survives; and true offscreen group
compositing, **not** GPUI's independent alpha multiply, which is only
correct when group contents do not overlap. Reuses the retained group
composition designed in v0.14 for issue #278 rather than inventing a second
scheme. Also carries **issue #133** — the quadratic clip-region storage —
because this story is the second consumer that decides which of the two
proposed representations is right.
_Depends on: S15.4._

**S15.8 — shadows and backdrop blur.** The closed-form shadow from S15.3,
and render-to-texture ping-pong blur reusing S15.7's compositing
machinery.
_Depends on: S15.7._

**S15.9 — swap into the host.** The wgpu painter behind v0.14's `Present`
seam, presenting to a surface instead of blitting. Both painters
selectable at run time against one document and one clock — which is the
development loop this slice actually runs on, and the reason the seam was
built painter-agnostic in v0.14.
_Depends on: S14.3, S15.4._

**S15.10 — layer 4, measured and recorded.** The perceptual band against
the Skia CPU oracle on a real GPU, with adapter, driver and version
recorded beside every number. Also decides whether the render oracle gains
per-painter bands or a separate band set.
_Depends on: S15.6, S15.7, S15.8._

**S15.11 — the web target.** wasm build, WebGPU with a WebGL2 fallback,
and the shell as a browser application. This is where lazy `.dsb` section
loading stops being hypothetical: `Container::parse` currently requires a
full-length slice (`crates/dashbuf/src/container.rs`, the
`SectionOutOfRange` check), which is free under `mmap` and forces the
whole file into linear memory in wasm. Either a prefix-tolerant parse mode
or a host-side envelope reader; the envelope is deliberately not a
flatbuffer and is fixed-layout little-endian, so a small host-side reader
is the cheaper option.
_Depends on: S15.9._

**S15.12 — retire `dashscene-web`.** Delete or repurpose the parked
tiny-skia painter, and amend the affected records: `crate-name-map.md`,
`backend-tiering-unity-skia-lean.md`, and the per-painter capability table
in `docs/wip/2026-07-19-backdrop-blur-v011.md`, whose "future wgpu
painter" row stops being hypothetical.
_Depends on: S15.11._

### Definition of done for the slice

- Layers 1 to 3 green in CI on the existing runners, each able to fail for
  a real reason. These are the slice's only CI claims.
- Layer 4 measured once on recorded hardware, with the number and the
  adapter written down.
- The full v0 paint vocabulary drawn, walked against v0.14's checklist with
  the wgpu painter selected.
- The web build loads and animates a document in a browser.
- Any golden movement declared before the story starts, landed alone, with
  both measurements recorded — the v0.13 rule, which applies because a new
  painter cannot be expected to hold Skia's bytes.

## Plan tracking

**Resolved: each slice gets its own milestone and its own `epic`-labeled
issue**, following the one-epic-one-milestone-per-slice rule in `AGENTS.md`.
v0.14 is not folded into v0.13's tail, even though it is small and its
first stories are v0.13-flavoured debt: those items are being pulled
forward _for_ v0.14, and that reasoning has to be visible in one place
rather than buried in a hardening epic. It also keeps v0.13's scope honest
— a hardening slice that acquires a windowed runtime is no longer a
hardening slice.

    milestone  v0.14 — the showcase runtime          epic, 7 stories
    milestone  v0.15 — the wgpu painter               epic, 12 stories

The roadmap is revised at each phase-end epic close (`AGENTS.md`), so
`docs/roadmap.md` gains both slices when v0.13's epic closes, not before.
Two records change at the same point: the v1 section stops carrying the web
painter, and `backend-tiering-unity-skia-lean.md` gains wgpu as the named
lean-painter candidate.

## Open questions

- **Does v0.15 need the entry-tier GLES measurement to close?** The
  strategy record says Skia keeps the entry tier until wgpu is measured on
  a real entry SoC, and no such hardware is in the loop (epic #476: "no
  frame budget, no target-hardware measurement"). So v0.15 can close
  without it, and the entry-tier switch is a later, separate decision.
  Worth stating in the record so the slice is not read as making that
  switch.
- **Where the `dt` invariant is tested.** The ruling is settled; its
  enforcement is not. "No crate at or below `LiveScene` reads a clock" is
  the kind of property a grep-based test can assert cheaply and a
  human-reviewed convention cannot. S14.3 should decide the mechanism.
