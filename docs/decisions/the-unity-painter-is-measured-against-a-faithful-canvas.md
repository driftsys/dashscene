# The Unity painter is measured against a faithful Canvas: at or below on GPU, lower on CPU

    status   **accepted (2026-09-05, owner ruling in session)**. What is
             accepted is the CRITERION, the rules for building the baseline it
             is read against, and the instruments that read it — D1 to D3 below
             bind the ten stories of epic #1441. How the criterion is met is
             those stories' own. D4 carries the reading of R-T2 that story
             #1450's mechanism rests on, until that story moves it beside the
             rule.
    date     2026-09-05
    source   the owner's ruling of 2026-09-05, gardened from §2, §3, §4 and §9
             of docs/wip/2026-09-05-unity-painter-beside-a-faithful-canvas.md,
             which moves to docs/archive/ when story #1451 archives it; docs/design/android-toolchain.md, "The Unity host's
             presented rate, and what bounds it"; issues #1347, #1406, #1413
    scope    the Unity painter — unity/com.driftsys.dashscene/Runtime/Engine/
             BrgPainter.cs, Runtime/FramePacker.cs, the samples under Samples~/
             and the demo host under unity/demo/ — read on the three showcase
             scenes against a uGUI Canvas built from the same document in the
             same player, on the target device the budget record names. The
             mechanisms that meet the criterion reach the shared side —
             crates/dashpaint, crates/dashscene-core, crates/dashscene-gpu —
             and the lean painter's own obligation stays the budget record's
    related  docs/decisions/the-gpu-frame-on-the-target-device-is-budgeted.md,
             whose D1 is a separate obligation that still binds and whose D2 is
             the GPU instrument here;
             docs/specification/03-target-hardware-rules.md — R-T2 through
             D4, which is the only rule this record reasons about; R-T4 and
             R-T5 bind the epic's later stories and are not restated here;
             docs/decisions/unity-painter-uses-brg.md and
             docs/decisions/brg-draw-command-order-is-not-guaranteed.md, which
             story #1448 amends where two blended classes leave
             BatchRendererGroup; issue #549, which this record does not close:
             it names one device, one extent and one display mode, not a
             display class

## Context

The owner asked on 2026-09-05, in one sentence, whether a scene built directly
on a uGUI Canvas would consume less CPU or GPU than the same scene drawn through
the Unity painter — and then ruled that the renderer must be better on CPU and
at least on par on GPU with a Unity Canvas, and that the slice must plan for
that. Until that ruling the painter had a budget and no baseline: the budget
record's D1 says the `surfaces` scene shall present at the panel's rate on the
target device, and nothing said how the painter compares with the alternative a
Unity UI team would otherwise ship for the same design.

The assessment the ruling rests on was given in the same session, and is
recorded here so a reader does not re-derive it. Per shaded pixel, a Canvas
`Image` costs one texture sample times a vertex colour, while the overlay path
evaluates an analytic SDF — the box edge, the stroke, the clip loop and the
gradient stops — for every kind, so a plain fill and a gradient cost more on the
painter today, and a rounded rect, a stroke or a blur has no cheap Canvas
primitive at all. Overdraw is the same on both sides today: both draw every
element blended, back to front, with no depth test. Draw submission favours the
Canvas, which batches by material where the painter emits one draw command per
visible instance. CPU under animation favours the painter and CPU at rest
favours the Canvas, because the painter's host does the same work whether or not
anything changed and a Canvas at rest rebuilds nothing. Text is SDF on both
sides, and the engine floor is shared.

Four rulings followed, each an answer to a question put in session, the fourth
later the same day. **The baseline is a faithful Canvas** — the same scenes as a
Unity UI team would ship them, not the cheapest possible Canvas — which D2
defines. **The work is a new gating epic on v0.21**, #1441, beside the two MVP
epics, so the slice does not close before the criterion is measured and met.
**The GPU criterion carries no tolerance above the Canvas**: "within 5 %" was
proposed and refused, and the target is below the Canvas by the fraction story
#1450's occlusion pass removes. **Both painters take what is shared**: the
occlusion pass runs on the shared side so both painters read the same pieces,
and the document's validated kind set specialises the shader. What P1 forbids
baking — resolved layout, glyph positions, and rasterised pixels of the composed
picture — stays out.

## Decision

- **D1 — the criterion is parity on GPU and a win on CPU, per scene and per
  state.** On the target device the budget record names — the Pixel 5 at
  1080x2340 in the 60 Hz mode, at the drawable the player reports — for **each**
  of the three showcase scenes, **at rest** (no transition in flight, the tick
  reporting no advance) and **during a transition** (a spring from the scripted
  pulse in flight — no showcase scene carries a variant transition, so no FLIP
  track runs, which `corpus/showcase/src/layout.rs` records of the set), with
  the same URP asset, the same camera and the same player:

  - **GPU, required.** The Unity painter's `frameReady` cadence mean shall be
    **at or below** the faithful Canvas's for the same scene in the same state.
    No tolerance above. **Met when** both readings are taken by the budget
    record's D2 procedure —
    `measure/android/gpu-capture.sh <out>
    com.driftsys.dashscene.showcase`
    with `DS_GPU_WINDOW=10`, which bounds the `--timestats` window only; the
    cadence is the `frameReady` deltas of the one `--latency` dump the script
    takes after that window, which is the compositor's own ring rather than the
    window. The entry is named by the player's own `[showcase] drew` line, both
    readings come from one run of one player, and the painter's mean does not
    exceed the Canvas's. **A dump of fewer than 100 frame rows is not a
    reading**: `--latency` returns the refresh period and no rows on Android 15,
    which `measure/android/gpu-capture.sh` reports and does not fail on, and an
    empty set would satisfy every comparison here. The state is held for the
    whole of each dump — the entry left untouched for the at-rest reading, the
    pulse driven for the transition one — and the story that takes a reading
    records the procedure it held it with beside the numbers, since a dump
    spanning both states describes neither. Each scene and state is read on both
    renderers in the same run, and the band that separates a difference from
    run-to-run noise is story #1444's, calibrated on its first run and pinned; a
    difference inside that band is not a result either way.
  - **GPU, target.** Below the Canvas's, by an amount whose upper bound is the
    scene's occluded fraction, which story #1450's occlusion pass removes. **Met
    when** the shaded-area instrument of issue #1296 counts the reduction with
    no device — shaded area is the unit the fractions are stated in — and the
    same D2 cadence is read and recorded beside it. **No conversion from area to
    milliseconds is claimed here**, and this clause states no millisecond
    threshold: the one projection that made that conversion was refuted on the
    device, as the Alternatives below record, so the cadence is evidence about
    the target rather than its arithmetic. Being a target, it binds no story to
    a number; the required clause above is what a story is measured by. The
    fractions derived before the pass exists are in
    `driftsys/dashscene-v021-lanes/probe-1412/RESULTS.md`, outside this
    repository, and that file states why they are upper bounds: the derivation
    applied neither corner radii nor clip boxes to a core's interior.
  - **CPU, required.** The Unity painter's **process CPU per presented frame**
    shall be lower than the Canvas's, and its **main-thread cost above the
    empty-scene floor** shall be lower than the Canvas's. **No instrument
    reports the first quantity today**: the sampler is normalised by
    `measure/android/frame-table.py` over the interval each sample covers, as a
    percentage of one core, and the compositor's frame count comes from a
    separate `measure/android/gpu-capture.sh` run with its own window. Story
    #1443 is what joins them — one window, the sampler and the compositor read
    across it, the frame count taken from the compositor rather than from the
    player's own drawn-frame reports, because a layer that presents nothing for
    part of a window makes drawn and presented differ, which
    `docs/design/android-toolchain.md` records happening on `typography`. **Met
    when** `utime + stime` from `/proc/<pid>/stat`, taken by
    `ds_cpu_sampler_start` in `measure/android/lib.sh` — which
    `measure/android/frame-capture.sh` starts for the lean host and nothing
    starts for the Unity one, so story #1443 wires it into
    `measure/android/unity-frame-cost.sh` — divided by the compositor's frame
    count over the same window is lower for the painter, and D3's thread-time
    instrument reports the lower main-thread cost above D3's floor over that
    window. At rest the painter's main-thread cost shall be indistinguishable
    from the floor, where "indistinguishable" means **at or below the floor's
    own p95** over the same window — the instrument reports mean, p50, p95 and
    max, so p95 is a statistic it already carries — and the reading states all
    four for both.

  A reading on another device, another extent, another scene or another display
  mode is a new row, not a substitute — the budget record's rule, unchanged.

  **What the criterion is not.** It is not a frame budget: the budget record's
  D1, the `surfaces` scene at the panel's rate, still binds the painter on its
  own. The two can disagree — a Canvas that itself misses the panel rate on
  `surfaces` would leave parity met and the budget unmet — and both stand. It is
  not a display-class requirement either: issue #549 is open and this record
  pins no geometry.

- **D2 — the fairness rules, eight of them.** The Canvas is built to be what a
  careful Unity UI team would ship for the same design, and every advantage such
  a team would have is given to it. The rules are a decision record rather than
  prose in a story because they will be argued.

  1. **Geometry is identical by construction.** The Canvas hierarchy is
     generated at load from the runtime's resolved rect table — the same
     `DsFrame` the painter packs, never a second solve — so every
     `RectTransform` sits where the solver put the node. No uGUI `LayoutGroup`
     runs: uGUI's layout is a lossy box model, and a layout that differed would
     make the comparison two scenes rather than two renderers.
  2. **Solid fills, rounded corners and static strokes** are one 9-sliced sprite
     per distinct shape — corner radius, stroke width, stroke alignment —
     rasterised once at load at the shape's pixel size and tinted through
     `Image.color`. A rect whose fill and stroke are both static solid colours
     is one `Image`; otherwise the fill and the stroke are two.
  3. **Linear gradients** are a 256-texel strip stretched across the `Image`;
     **radial, angular and diamond gradients** — the three kinds
     `gradient_colour` in `crates/dashscene-gpu/src/shaders/paint.wgsl` branches
     on, linear being its fall-through — are a texture baked at the node's pixel
     size at load. Both are what a team ships as PNGs, so baking them at load is
     that build step moved to run time and is not counted in any per-frame
     figure.
  4. **Text** is TextMeshPro on the same font files, one text object per glyph
     run, placed at the run's origin, with TextMeshPro's own typesetting. **The
     showcase is not Latin only**: `typography` carries three Arabic runs from a
     second family with its own committed atlas
     (`corpus/showcase/src/resources.rs`, `corpus/showcase/src/typography.rs`),
     so the Canvas side takes both families and TextMeshPro's own bidi, joining
     and mark placement for the Arabic ones. That makes `typography`'s reading a
     comparison of two typesetters as well as two renderers, which is the honest
     shape of it — the painter's runs are shaped by `dashscene-typeset` and the
     Canvas's by TextMeshPro, and neither side is given the other's. A run
     TextMeshPro places unfaithfully is recorded with the reading and is not
     culled, because rule 7 forbids culling.
  5. **Animation drives the Canvas from the same tick**: each frame the host
     reads the committed tables and writes `anchoredPosition`, `sizeDelta` and
     `color` for the rows in the lease's dirty set. The Canvas side receives the
     dirty set; a naive Canvas that rewrote every element would be the painter's
     advantage, and that advantage is not taken.
  6. **Constructs the Unity painter refuses today** are omitted on both sides,
     and are named in each reading with the `drew` line's refusal count. The set
     is `PackDiagnostic` in `unity/com.driftsys.dashscene/Runtime/`, read there
     rather than listed here, because a list here would be a second copy of an
     enumeration that already grows in one place. A refused construct landing
     later reopens the reading, as the budget record already rules for its own
     D1.
  7. **Nothing is culled by hand** on either side. Both renderers draw every
     instance the document holds in the shown root.
  8. **The animated subtree is isolated**, as a careful team isolates it: a rect
     the scene's pulses move sits on its own child Canvas from the first pulse
     that dirties it, so a rebuild covers the moving elements and not the whole
     scene — the structural mirror of the dirty set rule 5 hands over. Pinned at
     run time: the batch-build marker reads zero at rest, and during a pulse the
     rebuilt Canvas holds only isolated elements.

- **D3 — the instruments, and their definition stated against the two that
  exist.** One player carries both renderers and a floor: the demo player gains
  a Canvas entry per showcase scene beside the painter's, and an **empty entry**
  — the camera, the clear and nothing else — whose readings are the floor. The
  switch is the desktop shell's painter-swap pattern, on a key bound in a new
  sample file so it does not collide with
  `Samples~/Showcase/DashsceneShowcase.cs`.

  - **GPU** stays on the budget record's D2 and gains nothing:
    `dumpsys SurfaceFlinger --timestats` over its window for the rate, and
    `--latency`'s `frameReady` cadence for the frame, both taken by
    `measure/android/gpu-capture.sh` with the showcase package named.
  - **CPU** gains one instrument beside the existing frame-cost line, in the
    package — `Runtime/Engine/DashsceneThreadCost.cs`, with its arithmetic
    factored into a Unity-free class placed under `Runtime/` rather than under
    `Runtime/Engine/`, because `unity/ffi-check` compiles the first and excludes
    the second. Placement makes the class reachable; story #1443 also writes the
    `Check` that executes it, as `Program.cs` already does for `CommitPacer`,
    and `unity/package-compat` then compiles it against netstandard2.1 under
    R-E10 — and a reporter in a new sample file. It reads `ProfilerRecorder` on
    the `Main Thread` and `Render Thread` counters, whose names story #1443
    confirms in a device-free test rather than assumes, and on the Canvas
    rebuild markers `Canvas.SendWillRenderCanvases` and `Canvas.BuildBatch`; it
    reports per 240 drawn frames in the same shape as `FrameCostSample.Line`,
    disarmed by the same kind of argument. Beside it, the process sampler
    `ds_cpu_sampler_start` in `measure/android/lib.sh`, read against the
    compositor's frame count over the same window — it exists and the lean
    host's capture already starts it, so what the Unity capture gains is the
    call; and the empty entry's readings, taken in the same run, are the floor.
  - **Allocation is part of CPU.** A steady frame — no transition, no
    reallocation — shall allocate zero managed bytes on the main thread,
    measured by `GC.GetAllocatedBytesForCurrentThread` around the host's
    `Update`, on the painter and on the Canvas alike. The two are measured in
    different hosts, because `unity/render-gate` draws two committed documents,
    holds no showcase entry and constructs no `Canvas`: the painter's half is
    story #1445's, in the render gate, and the Canvas's is taken in the demo
    player where story #1444's entries live. The Canvas is expected to fail this
    at rest only if it rebuilds, which rule 5 prevents.

  **The definition is stated against the two instruments that already exist**,
  because a Unity figure taken from `Time.deltaTime` or from the profiler
  measures the engine's frame rather than the painter's work, and a comparison
  built on one is between two harnesses rather than between two renderers —
  which is the trap issue #1347's body names. `demo/src/shell.rs`'s `Timing`
  brackets `tick` and `present` over a fixed sample of presents, and its
  `present` is the whole of the drawing: `paint` plus whatever putting the frame
  on the window costs. `DashsceneFrameCost.cs` brackets the lease acquire,
  `BrgPainter.Draw`, the mark and the release, and excludes everything Unity
  runs after `Update` returns — the culling callback, the render thread's
  encode, the pipeline's own passes and the swapchain present — so its `draw` is
  a strict subset of `shell.rs`'s `present`, and its own header says so. The
  thread-time instrument includes what both exclude, the Canvas rebuild among
  them, and states that in its header. The row that settles issue #1347 is story
  #1451's, with the lean painter's beside it.

- **D4 — R-T2's intent, and the second mechanism that meets it.** R-T2 names one
  mechanism, the depth-tested opaque core. Story #1450's occlusion pass meets
  the rule's intent by another, and the specification is as-built while that
  pass does not exist, so the paragraph below is **not** written into
  `docs/specification/03-target-hardware-rules.md` by this record. This record
  carries it, word for word:

  ```markdown
  R-T2's intent is that a pixel covered by a later opaque rect is never shaded.
  Two mechanisms satisfy it, and a painter may take either or both: the
  depth-tested opaque core the rule names, and an occlusion pass over the
  resolved rect table before packing, which emits only the visible pieces of
  each rect and needs no depth buffer
  (`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`).
  The depth-tested form was measured on the Pixel 5 on 2026-09-05 and did not
  shorten the frame (story #1412); the pass form is story #1450's.
  ```

  Story #1450 moves that paragraph beside R-T2 in the pull request that lands
  the pass, in the past tense and with the pass's own reading beside it.

## Consequences

- **The ten stories of epic #1441 cite D1, D2 and D3 by name**, and no story
  re-derives the criterion, the rules or the instruments. Story #1443 builds
  D3's thread-time instrument and the URP floor its own title names; story #1444
  builds the Canvas entries to D2 and the empty entry that is D3's floor, which
  its `-renderer none` argument selects; and story #1451 reads the comparison
  table against D1 and closes the epic on it.
- **This record binds those ten stories and does not itself decide when the
  slice closes.** Epic #1441 is gating on v0.21 by the owner's ruling of
  2026-09-05, which `docs/roadmap.md` records; that gating status, not this
  record, is what holds the slice open until the criterion is met.
- **The budget record's D1 remains a separate obligation**, and meeting D1 here
  does not meet it. Every reading a story takes under D3 serves both records, so
  a story reports its scene against both; issues #1412 and #1413 keep the
  obligations the budget record gave them.
- **Issue #549 stands.** This record names one device, one extent and one
  display mode, exactly as the budget record does. A reading on another device
  or another display class is a new row, and no display geometry is pinned by
  the specification.
- **Every reading this record is met by lives elsewhere.** Where the painter
  stands today — the per-scene `frameReady` cadence means and the shaded areas
  derived with no device — is `docs/design/android-toolchain.md`, "The Unity
  host's presented rate, and what bounds it": PR #1409 landed that section and
  PR #1431 added the per-scene cadence table to it. The Canvas has no reading at
  all until story #1444's entries exist, so no comparison against D1 can be
  taken before that story lands, and D1 is unmet rather than failed until then.

## Alternatives considered

- **Shader first, baseline later.** Rejected: it optimises against a number that
  does not exist, and the saving the budget record's D3 projects for the opaque
  core — about 7 ms of the 31 — was refuted on the device, where the build that
  took it lengthened every scene instead
  (`driftsys/dashscene-v021-lanes/probe-1412/RESULTS.md`, outside this
  repository; story #1412).
- **Replace `BatchRendererGroup` for every class.** Cleanest parity with the
  lean painter, and the argument is real: no showcase scene draws a lit class
  today. Not taken here, because D1 of `unity-painter-uses-brg.md` put every
  material class on that one path, the lit ones included, and **withdrawing that
  path wholesale is the owner's call rather than this epic's**. What story #1448
  does is narrower and is within the epic: it moves the two blended classes off
  BatchRendererGroup and leaves D1's lit half standing, on the shape
  `brg-draw-command-order-is-not-guaranteed.md` D5 measured — a draw command
  carrying `HasSortingPosition` names exactly one visible instance. Story #1447
  reads what that shape costs on the device before story #1448 is written.
- **A tolerance above the Canvas, 5 %.** Proposed and refused by the owner: the
  criterion is at or below, and story #1450's pass is what makes "below"
  reachable.
- **Waiting for the depth experiment before choosing the overdraw mechanism.**
  Offered and refused: the occlusion pass needs no driver behaviour and is
  derived without a device, so it is committed now, and R-T2's depth-tested form
  stays a possible further step under story #1412's own experiment.
- **Reducing resolution or render scale.** The budget record already rejects it
  as a route; it stays a diagnostic.
- **The cheapest possible Canvas as the baseline.** Refused as the criterion:
  flat `Image`s and legacy `Text` are not the same picture. It may still be read
  once as a floor, and no story here does so.
- **Placing the work on the non-gating epic #1120.** Refused: the owner wants
  the slice held open on this criterion, which is why #1441 is gating.
- **Occlusion in the C# packer alone.** The first draft of the mechanism,
  refused later the same day when the owner asked whether the work benefits the
  lean painter. One pass on the shared side costs a C ABI change and provides
  both painters, with one implementation and one set of tests.
- **Baking more at import.** Weighed and left out, kind by kind: gradient strips
  as a `dashpack` derivation (microseconds at load, and an animated paint needs
  a run-time re-bake anyway), a static-or-dynamic flag per node (the dirty set
  already carries it), pre-shaped static strings as a per-target derivation
  (startup rather than the frame, so the loading-performance class and not this
  epic), a pre-solved layout (refused outright: P1 forbids resolved x/y/w/h in
  the document, whatever it would cost), and pre-composited static subtrees as
  bitmaps (resolution-dependent, bandwidth on a tiler, and P1). Only the kind
  set survived, as story #1449's per-document specialisation.
