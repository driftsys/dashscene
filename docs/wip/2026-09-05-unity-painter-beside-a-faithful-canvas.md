# The Unity painter beside a faithful Canvas: lower on CPU, at or below on GPU

    status    WIP — specification (2026-09-05, owner + Fable). The design was approved in session on
              2026-09-05. The epic and its ten stories are filed — #1441, and
              #1442 to #1451 — and nothing is implemented. The implementation
              plan follows in this directory. **PARTLY GARDENED by story
              #1442**: the record
              docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md
              owns §2, §3, §4 and §9 — the criterion, the fairness rules, the
              instruments and the alternatives considered — and is what every
              later story cites. Those four sections are superseded here rather
              than deleted, because the nine later stories still read this file
              for the story shape; they leave with the plan when story #1451
              archives both. Where this file and that record differ, the record
              is what binds. Two differences so far, both corrections: §4 says
              demo/src/shell.rs brackets "tick, paint and present", where Timing
              brackets tick and present with paint inside present; and §4 and
              §5.1 say the process sampler in measure/android/lib.sh "already
              runs", where measure/android/frame-capture.sh starts it for the
              lean host and nothing starts it for the Unity one. A third: §3's
              rule 4 says the Canvas takes "the same font file" and is "Latin
              only, as the showcase is", where corpus/showcase/src/typography.rs
              draws three Arabic runs from a second family with its own atlas.
    scope     the Unity painter — `unity/com.driftsys.dashscene/Runtime/Engine/
              BrgPainter.cs`, `Runtime/FramePacker.cs`, the two samples under
              `Samples~/`, the demo host under `unity/demo/` — the shared
              shading in `crates/dashscene-gpu/src/shaders/paint.wgsl` and its
              generated HLSL twin, the occlusion pass on the shared side —
              `crates/dashpaint` beside `RectEntry`, the commit in
              `crates/dashscene-core`, and one new slice on `DsFrame` in
              `crates/dashscene-ffi` — the Android measurement kit under
              `measure/android/`, and the records that hold the readings
    builds on `docs/decisions/the-gpu-frame-on-the-target-device-is-budgeted.md`
              `docs/decisions/unity-painter-uses-brg.md`
              `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
              `docs/specification/03-target-hardware-rules.md` — R-T1, R-T2,
              R-T4, R-T5
              `docs/design/android-toolchain.md`, "The Unity host's presented
              rate, and what bounds it"
              `docs/design/unity-csharp-host.md`, "The painter, and where each
              rule it obeys comes from"
    satisfies R-T4 (the CPU frame cost rule) and R-T5 (single-sourced shading)
              directly; R-T2's intent — a covered pixel is never shaded — by
              a mechanism that rule does not name, which §6.4 states and §9
              routes to the specification
    opens on  issue #1347 (the Unity painter beside the lean painter on one
              device; story 10 reads the lean painter's row beside the table, which is what settles it), #1406 (the per-command term), #1306 (the dirty-range
              upload), #1413 (the per-kind sweep and fast paths). Filed 2026-09-05 by the plan's
              Task 0: epic #1441; stories #1442 (criterion), #1444 (Canvas),
              #1443 (instrument), #1445 (settle), #1446 (dirty range), #1447
              (per-command term), #1448 (in-order draws), #1449 (fast paths),
              #1450 (occlusion), #1451 (the table).

## 0. The question, and the ruling

The owner asked on 2026-09-05, in one sentence: given how the painter works,
would a scene built directly on a uGUI Canvas consume less CPU or GPU — and
then: the renderer must be better on CPU and at least on par on GPU with a Unity
Canvas; plan for that.

The assessment that the spec rests on, given in the same session and recorded
here so the reader does not re-derive it:

- **Per shaded pixel**, a Canvas Image costs one texture sample times a vertex
  colour. The overlay path costs an analytic SDF evaluation — box, stroke, the
  clip loop and the gradient stops — for every kind. So a plain fill and a
  gradient cost more on the painter today, and a rounded rect, a stroke or a
  blur has no cheap Canvas primitive at all.
- **Overdraw** is the same on both sides today: both draw every element blended,
  back to front, with no depth test.
- **Draw submission** favours the Canvas today: a Canvas batches by material,
  and the painter emits one draw command per visible instance.
- **CPU under animation** favours the painter, and **CPU at rest** favours the
  Canvas, because the painter's host does the same work whether or not anything
  changed and a Canvas at rest rebuilds nothing.
- **Text** is SDF on both sides. **The engine floor** is shared.

Four rulings followed, each an answer to a question put in session, the fourth
later the same day:

1. **The baseline is a faithful Canvas** — the same scenes as a Unity UI team
   would ship them, not the cheapest possible Canvas. §3 defines it.
2. **A new gating epic on v0.21**, beside the two MVP epics, so the slice does
   not close before the criterion is measured and met.
3. **The GPU criterion carries no tolerance above the Canvas.** "Within 5 %" was
   proposed and refused; the criterion is at or below, and the target is below
   by the occluded fraction §6.4 derives.
4. **Both painters take what is shared.** Asked whether the work benefits the
   lean painter and what can be baked at import, the owner ruled, later on
   2026-09-05, that the occlusion pass runs on the shared side so both painters
   read the same pieces (§6.4), and that the document's validated kind set
   specialises the shader (§6.3). What P1 forbids baking — resolved layout,
   glyph positions, rasterised pixels of the composed picture — stays out, and
   §9 lists what was weighed.

## 1. Where the frame goes today

Everything in this section was read on 2026-09-05. The per-frame path was
verified against `origin/main` at `789103c` — the commit the plan's three maps
were read at — not the primary checkout, which was behind it by the whole
showcase-parity story. The device figures are the records' own and are cited,
not restated beyond what the criterion needs.

### 1.1 The host's per-frame path

- `DashsceneShowcase.Update` and `DashsceneFrameLoop.Update` tick, acquire a
  lease, draw and mark on every Unity frame. `DashsceneRuntime.Tick` returns
  whether the generation moved — `ds_runtime_tick`'s `out_advanced` — and both
  samples discard it. `CommitPacer.ShouldCommit` is a fixed-rate divider, not a
  settle detector, and both samples default it to every frame. Nothing in the
  package idles.
- `BrgPainter.Draw` repacks every instance and uploads the whole staging array,
  including the capacity past the live instances, on every frame, and re-uploads
  all four heap tables. `DsFrame.Dirty`'s rows are read by nothing;
  `FramePacker.cs`'s own header says so and names issue #1306.
- `BrgPainter.BindHeap` rebinds every heap buffer on every material on every
  frame — its own heading is "Every material, on every frame".
- `BrgPainter.OnPerformCulling` rebuilds the draw-command arrays on every call
  and emits **one draw command per visible instance** with `HasSortingPosition`,
  which is D5 of the draw-order record — the shape issue #1401's fix required.
  The typography scene emits 381 of them.
- `unity/demo/DemoBuild.cs` creates the URP asset from defaults and sets one
  field, `useSRPBatcher`; HDR, MSAA, the depth and opaque textures and
  post-processing are whatever the defaults give.

### 1.2 The device figures

Pixel 5, Adreno 620, 1080x2340 in the 60 Hz mode, explicit 60 asked, Vulkan. The
frame figures are the 2026-09-05 reading of the committed pacing build, in
`docs/design/android-toolchain.md`'s "The Unity host's presented rate" section
since PR #1431 merged, which is this branch's base; the dumps are under
`driftsys/dashscene-v021-lanes/probe-1412/`. The shaded areas are the same
section's, derived by the lean painter's packer over each scene's committed
tables. The compositor's `frameReady` cadence is the GPU frame:

| scene      | instances | frameReady mean | rate      | shaded area (derived)                              |
| ---------- | --------: | --------------: | --------- | -------------------------------------------------- |
| surfaces   |        56 |         31.2 ms | 32.4 fps  | 6.06 Mpx, of which the Unity host draws nearer 5.2 |
| typography |       381 |         20.7 ms | 49.9 fps  | 3.17 Mpx                                           |
| layout     |        29 |         16.7 ms | the panel | 5.43 Mpx                                           |

Per-pixel cost differs by kind: solid fills at or under 3.1 ms per megapixel,
gradients and strokes at 5 to 6, glyphs about 5.4 or a per-command term the
records have not separated (issue #1406). The lean painter's 1.9 ms per
megapixel on solid rects is the floor. `UnityMain` was at 14 % of a core and the
render thread at 7 % in one `top` sample on `surfaces`; nothing attributes
either to the painter.

**R-T2's GPU form does not shorten the frame today.** The opaque-core build of
story #1412 lengthened every scene — `surfaces` 31.2 → 56.9 ms — and the
falsifying build of the same morning, the core inset in the vertex stage with no
discard, still adds 17 ms on `surfaces`. The next experiment separates "the
depth test does not reject in this pass on this device" from "the core pass
costs what it rejects". `driftsys/dashscene-v021-lanes/probe-1412/RESULTS.md`
carries the readings; the story branch's records carry the reading in prose.
This specification does not depend on how that experiment lands.

## 2. The criterion — D1

On the target device the budget record names — the Pixel 5 at 1080x2340 in the
60 Hz mode, the drawable the player reports — for **each** of the three showcase
scenes, **at rest** (no transition in flight, the tick reporting no advance) and
**during a transition** (the scripted pulse's spring or FLIP in flight), with
the same URP asset, the same camera and the same player:

- **GPU — required:** the Unity painter's `frameReady` cadence mean, read by the
  budget record's D2 procedure over its window, shall be **at or below** the
  faithful Canvas's for the same scene in the same state. No tolerance above.
- **GPU — target:** below the Canvas's by up to the scene's occluded fraction
  (§6.4), read on the same instrument and on the shaded-area instrument of issue
  #1296.
- **CPU — required:** the Unity painter's **process CPU per presented frame**
  (`utime + stime` from `/proc/<pid>/stat`, the sampler `measure/android/lib.sh`
  already runs, divided by the compositor's frame count over the same window)
  shall be lower than the Canvas's; and its **main-thread cost above the
  empty-scene floor** (§4) shall be lower than the Canvas's. At rest the
  painter's main-thread cost shall be indistinguishable from the floor, where
  "indistinguishable" is within the floor's own spread over the same window,
  stated with the reading.

A reading on another device, another extent, another scene or another display
mode is a new row, not a substitute — the budget record's rule, unchanged.

**What the criterion is not.** It is not a frame budget: the budget record's D1
(the `surfaces` scene at the panel's rate) still binds the painter on its own.
The two can disagree — a Canvas that itself misses the panel rate on `surfaces`
would leave parity met and the budget unmet — and both stand. It is not a
display-class requirement either; issue #549 is still open and this file does
not pin geometry.

## 3. The fairness rules — D2

The Canvas is built to be what a careful Unity UI team would ship for the same
design, and every advantage such a team would have is given to it. The rules are
a decision record, not prose in a story, because they will be argued.

1. **Geometry is identical by construction.** The Canvas hierarchy is generated
   at load from the painter's own resolved rect table — the same `DsFrame` the
   painter packs — so every `RectTransform` sits where the solver put the node.
   No uGUI `LayoutGroup` runs; the design seed already records that uGUI's
   layout is a lossy box model, and a layout that differed would make the
   comparison two scenes rather than two renderers.
2. **Solid fills, rounded corners and static strokes** are one 9-sliced sprite
   per distinct shape (corner radius, stroke width, stroke alignment),
   rasterised once at load at the shape's pixel size, tinted through
   `Image.color`. A rect whose fill and stroke are both static solid colours is
   one `Image`; otherwise the fill and the stroke are two.
3. **Linear gradients** are a 256-texel strip stretched across the `Image`.
   **Radial, angular and diamond gradients** — `gradient_colour` in `paint.wgsl`
   branches on those three and linear, four kinds — are a texture baked at the
   node's pixel size at load. Both are what a team ships as PNGs; baking them at
   load is the build step moved to run time and is not counted in any per-frame
   figure.
4. **Text** is TextMeshPro on the same font file, one text object per glyph run,
   placed at the run's origin, TextMeshPro's own typesetting. Latin only, as the
   showcase is.
5. **Animation** drives the Canvas from the same tick: each frame the host reads
   the committed tables and writes `anchoredPosition`, `sizeDelta` and `color`
   for the rows in the lease's dirty set. The Canvas side receives the dirty
   set; a naive Canvas that rewrote every element would be the painter's
   advantage, and it is not taken.
6. **Constructs the Unity painter refuses today** — the shadow, the backdrop
   blur, the image fill, the baked vector nodes and the render-target groups —
   are omitted on both sides, named in the record with the `drew` line's refusal
   count. A refused construct landing later reopens the reading, as the budget
   record already rules for its own D1.
7. **Nothing is culled by hand** on either side. Both renderers draw every
   instance the document holds in the shown root.
8. **The animated subtree is isolated**, as a careful team isolates it: a rect
   the scene's pulses move sits on its own child Canvas from the first pulse
   that dirties it, so a rebuild covers the moving elements and not the whole
   scene — the structural mirror of the dirty set rule 5 hands over. Pinned at
   run time: the batch-build marker reads zero at rest, and during a pulse the
   rebuilt Canvas holds only isolated elements.

## 4. The harness and the instruments — D3

**One player, two renderers, one key.** The demo player gains a Canvas entry per
showcase scene beside the painter's, and an **empty entry** — the camera, the
clear and nothing else — whose readings are the floor. The switch is the desktop
shell's painter-swap pattern, on a key bound in a **new** sample file so it does
not collide with `DashsceneShowcase.cs`, which another lane owns.

**GPU** stays on the budget record's D2: `dumpsys SurfaceFlinger --timestats`
over its window for the rate, `--latency`'s `frameReady` cadence for the frame,
taken by `measure/android/gpu-capture.sh` with the showcase package named.

**CPU** gets one new instrument beside the existing frame-cost line, in the
package — `Runtime/Engine/DashsceneThreadCost.cs`, its arithmetic in a
Unity-free class the ffi gate executes — and a reporter in a new sample file:

- `ProfilerRecorder` on the `Main Thread` and `Render Thread` counters — names
  story 3 confirms in its device-free test rather than assumes —, and on the
  Canvas rebuild markers (`Canvas.SendWillRenderCanvases`, `Canvas.BuildBatch`),
  reported per 240 drawn frames in the same shape as `FrameCostSample.Line`,
  disarmed by the same kind of argument;
- the process sampler `measure/android/lib.sh` already runs, read against the
  compositor's frame count over the same window;
- the empty entry's readings, taken in the same run, as the floor.

**The definition is stated against the two that exist.** `demo/src/shell.rs`
brackets tick, paint and present; `DashsceneFrameCost.cs` brackets the lease,
`BrgPainter.Draw` and the release and excludes everything Unity runs after
`Update` returns. The thread-time instrument includes what both exclude — the
culling callback, the render thread's encode, the Canvas rebuild — and says so
in its header. That statement answers the two-harnesses trap issue #1347's body
names; the row that settles that issue is story 10's, with the lean painter
beside.

**Allocation** is part of CPU: a steady frame — no transition, no reallocation —
shall allocate zero managed bytes on the main thread, measured by
`GC.GetAllocatedBytesForCurrentThread` around the host's `Update` in the render
gate, on both the painter and the Canvas entries. The Canvas is expected to fail
that at rest only if it rebuilds, which rule 5 prevents.

## 5. CPU — what changes

### 5.1 The settle path

The host loop skips acquire, pack, upload and bind when `Tick` reports no
advance. `ds_runtime_tick`'s contract already supports it: a commit is marked
shown through the `drawn` argument of `ds_runtime_release_frame` —
`FrameLease.MarkDrawn` on the C# side — which the C ABI documents as what "marks
the commit shown so a settled scene stops reporting `out_advanced` and the host
can idle". The trap `DashsceneFrameLoop` records — a host that acquires and
skips the mark reports advanced forever — is avoided by skipping the acquire
itself, never the mark.

**Forced redraws**, each a condition under which the tick reports no advance and
the screen is nonetheless wrong: a replaced document (`DocumentReplaced` on the
lease), a new atlas set (`SetAtlases`), a recreated graphics device or surface
(the C ABI names this case: after re-attaching, "a host that only draws when the
tick advanced would show an empty window until something else moved the scene"),
and a changed drawable extent. Each sets a flag the next frame consumes.

Inside the painter: `BindHeap` runs only when a heap buffer was reallocated, the
atlas set changed, or the scalars it binds — the anti-aliasing width and the two
heap bases — changed, which a resize and a newly interned solid both do; the
culling callback re-emits from the command description `Draw` last computed
rather than recomputing it. Unity's `Allocator.TempJob` arrays in the callback
are Unity's contract and stay.

### 5.2 The dirty-range pack and upload — issue #1306

`DsFrame.Dirty` carries the dirty set as `u32` rect indices. A rect is not an
instance row: `FramePacker.cs` records that a rect packs to one instance or
several — a backdrop, a fill, stacked fills, a stroke — and that a refused rect
packs to none, so `v03-paint.dsb`'s fourteen rects pack to sixteen instances.
The packer therefore keeps, from its last full pack, each rect's range of
instance rows and the heap rows those instances reference. When the rect count
and order are unchanged from the previous commit and every dirty rect packs to
the same number of instances as before, the packer rewrites those rows in place
and uploads the coalesced ranges through `GraphicsBuffer.SetData`'s offset form
— per batch and per property stream, since the staging buffer is stream-major
inside each batch. The heap is repacked and uploaded whole on every commit, as
the lean painter does: a changed paint earns a new interned row and moves the
gradient base, so no heap slot is stable, and the heap is small beside the
instance buffer R-T4 bounds. A changed rect count, order or per-rect instance
count falls back to the full pack, which is today's path. The dirty set is
relative to consecutive commits, so the settle path's skip — which happens only
when no commit occurred — loses none of it.

The lean painter has the same gap under issue #708 and keeps it: this epic is
the Unity painter's, and R-T4 binds both.

## 6. GPU — what changes

### 6.1 The per-command term — issue #1406

Two players from one tree on the typography scene: one command per visible
instance, as shipped, against one command per contiguous same-material run, the
pre-#1401 shape, which drops frames and is built only to be read. Both are read
on the render thread (§4) and on the compositor. The reading decides whether
§6.2 is on the parity path or only on R-T4's, and is recorded either way.

### 6.2 In-order instanced draws for the overlay and text classes

A `ScriptableRendererFeature` whose render-graph raster pass issues one
procedural instanced draw per contiguous same-material run, in document order,
through
`RasterCommandBuffer.DrawProcedural(matrix, material, pass,
MeshTopology.Triangles, 6, instanceCount)`
— present in the SRP core package the demo project stages, read on 2026-09-05
from
`target/unity-demo/Library/PackageCache/com.unity.render-pipelines.core@*/Runtime/CommandBuffers/RasterCommandBuffer.cs`.
The shader reads a `StructuredBuffer` of instances by `SV_InstanceID`, the lean
painter's binding shape, so R-T5's parity gets simpler, not harder. There is no
culling callback and no sorting key for these classes: within one draw the
instances rasterise in instance order, and across draws the pass issues them in
sequence.

`BatchRendererGroup` stays for `LitOpaque` and `LitCutout`. D1 of the BRG record
chose BRG for the bulk of the SDF-quad UI on one path, lit included; this
**reverses D1 for the two blended classes** — rung 3 of D3's ladder taken by
design rather than by unavailability, for a measured reason: the
sorted-transparent path costs one command per instance — and keeps its lit half.
That record's D1 and D3 are amended in place to say so, and the draw-order
record's scope, D1, D4 and D5 narrow to the classes that stay on BRG, since the
ordered path has no keys. The order gate issue #1402 landed with PR #1433 is the
falsifying test: it must pass unchanged on the new path.

**What it keeps.** The known gap that blended text sits after every opaque node
whatever the document order (`unity-csharp-host.md`, the text section) stays,
because the pass sits after the lit classes in the frame. It is the same gap on
both paths.

### 6.3 Fast paths in the shared shading — issue #1413

After the per-kind sweep that issue specifies, and where it says a kind costs
more than twice the solid rate:

- **a plain-fill path** — an instance with no corner radius, no stroke and no
  clip evaluates only the box edge's anti-aliasing and the fill;
- **gradients from a baked strip** — a 256-texel-wide texture with one row per
  gradient row of the paint heap, baked once on the shared side at commit like
  §6.4's pieces (the CPU ramp the conformance test already carries is the
  baker), held on the committed scene and handed to the Unity painter through
  one C ABI call when its generation moves; the fragment computes the gradient
  parameter (linear, radial, angular or diamond) as it does today and samples
  the strip instead of walking the stops. The parameter arithmetic stays
  single-sourced in `paint.wgsl`; the sample is per-painter composition, exactly
  as `msdf_sample` is today.

With both, a gradient pixel costs what a Canvas gradient pixel costs — one
sample — and a solid pixel costs less, because it fetches nothing. The layer-2
conformance table under `conformance/` is extended for the new branches, and the
goldens decide whether a 256-entry strip's quantisation of stop positions is
within tolerance.

**Per-document specialisation.** One function over the committed tables in
`crates/dashpaint` reports which paint kinds and constructs a document uses — a
census of validated vocabulary, not a discovery of new vocabulary, so P4 holds —
reported per commit — the tables it is a census of grow when a paint or a stroke
is interned mid-run — and read by each painter on every drawn frame, the lean
painter from the committed scene and the Unity painter through one C ABI call,
re-selecting when the bits change and compiling the variant with the dead
branches removed: no clip loop for a document with no clip boxes, no stroke
arithmetic. A uniform branch costs little on the Adreno; a compiled-out branch
costs nothing. The wgpu painter selects it through pipeline-overridable
constants and a pipeline cache keyed on the set, the Unity painter through
shader keywords — only the branches that exist to be removed carry a keyword, so
the variant count grows by what is compiled out and no more — and the per-kind
sweep measures what it is worth before it is written. The generated arithmetic
is unchanged; what is specialised is the composition around it.

### 6.4 Occlusion at commit — the target's mechanism

At commit, before either painter packs, subtract every later **opaque core**'s
interior from the instances beneath it and emit only the visible pieces. Each
piece carries the original box, so the SDF shape and the anti-aliased edge are
evaluated against the whole rect and only the quad's extent shrinks — the same
split the inset-core experiment made in the vertex stage, driven by occlusion
instead of by alpha. An opaque core takes the derivation's alpha rule — a solid
fill with alpha 1, or a gradient whose stops all have alpha 1, on a node at
opacity 1 — and a stricter interior than the derivation's one pixel per side:
the box shrunk by the largest corner radius plus the anti-aliasing band,
intersected with the occluder's clip boxes. Not a core: a node whose silhouette
is a baked vector field rather than its box, and a node inside a group
composited below alpha 1. Never occluded, kept whole: a rect whose stroke outset
or shadow reaches past its box. The anti-aliasing band the interior is inset by
is the host's — one device pixel at its scale — set through one C ABI call where
the host sets its extent, not a constant the commit assumes.

What it is worth is derived, not measured — `probe-1412/rejected.rs`, at
2340x1080, counting the area under a later core:

| scene      | shaded before | under a later core | fraction |
| ---------- | ------------: | -----------------: | -------: |
| surfaces   |      4.85 Mpx |           1.36 Mpx |     28 % |
| typography |      3.17 Mpx |           0.45 Mpx |     14 % |
| layout     |      5.42 Mpx |           2.84 Mpx |     52 % |

Those are upper bounds: the derivation did not apply corner radii or clip boxes
to the interior, so a rounded or clipped core counted a little large. The shaded
area here is the derivation's own model of what the Unity painter draws — fills,
strokes and glyphs, the refused kinds left out, the cores inset by one pixel —
which is why `surfaces` reads 4.85 Mpx here against §1.2's 6.06 through the lean
painter's packer (the record's "nearer 5.2" for the Unity host is a third count,
by refused rects); `layout`'s 5.42 here is the derivation's 5.423, which §1.2's
record rounds to 5.43.

Cost is rectangle subtraction over the opaque cores — 19, 5 and 27 on the three
scenes by the same derivation — once per commit. The commit also holds the dirty
set, so an incremental form is possible there; this epic does not promise it.
The piece count grows: a backdrop under seventeen occluders splits into tens of
quads, each an instance, and the pass bounds it — a rect that would split past a
stated cap keeps its whole quad and is not occluded. The count per scene is
derived with no device before the ABI changes, and the story records it. The
shaded-area instrument of issue #1296 counts the result with no device, which is
the story's first gate; the compositor is the second.

**Where it runs: on the shared side, once, for both painters.** The frame's rect
table is `dashpaint::RectEntry` rows held on `dashscene_core::CommittedScene`
and handed over the C ABI as `DsFrame.rects`. The pass is commit-time geometry
and lives with the commit, in `crates/dashscene-core` beside the clip resolution
that already crosses the boundary resolved; the `Piece` row type lives in
`crates/dashpaint` with the other boundary-B rows. The commit calls the pass
after the solve has written the rects, holds the result — one piece per visible
sub-rectangle, carrying the rect index it belongs to and the quad extent — on
the committed scene beside the rects, hands it out as one new slice on
`DsFrame`, **and dirties every rect whose pieces changed**: a moving core
changes the pieces of the rects beneath it, which the entry-bit compare alone
does not see, and a partial pack keyed on the old dirty set would leave stale
pieces on the device. P2 permits it: it is geometry on the resolved rect table,
and both painters still only colour. P3 holds: it runs inside the tick, never in
a painter. The lean painter reads the pieces from the committed scene; the C#
packer reads the slice and packs pieces where it packed rects. The cost is a C
ABI change — a new member on `DsFrame`, the row declared on `dashpaint-abi`'s
surface and mirrored in `Runtime/BoundaryB.cs` so the abi gate round-trips it (a
stride check sees size, not field order), the stride check R-E17 already runs,
`unity/ffi-check`'s geometry check extended to a piece, and an ABI version bump
— and it provides one implementation and one set of tests for two painters.

### 6.5 The URP asset

`DemoBuild.CreatePipeline` sets HDR, MSAA, the camera depth texture, the camera
opaque texture and post-processing explicitly off, and the record says which
default each replaced. This is shared floor and helps both renderers equally; it
is measured before and after on both so the parity reading is not confounded by
it.

## 7. What stays out

- **R-T2's GPU form** — the opaque core with depth — stays in story #1412's lane
  with its open experiment. It is the painter's further gain after parity, a
  Canvas cannot take it, and it regressed on the device; it is not on this path,
  and this file does not decide it.
- **The lean painter's** dirty-range pack (#708), and the host wiring that
  passes it a dirty set at all (#615). Both are named above as the same gap;
  neither is promised here. Its dirty-range upload already exists and is the
  shape story 5 copies.
- **The SA8255P row.** One target-class device is attached; the criterion's rows
  are that device's. The second tier is a new row when a board is attached.
- **Anything the Unity painter refuses today** (§3, rule 6).

## 8. The stories, and what verifies each

The plan owns the steps; this is the shape and the dependency, and the epic's
planned count, which the slice-planning rule asks an epic to state: **ten
stories, plus the epic.**

| #  | story                                      | depends on        | verified by                                                                                                                                                                                                                    |
| -- | ------------------------------------------ | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1  | the criterion and fairness record (§2, §3) | —                 | review; the record is what every later reading cites                                                                                                                                                                           |
| 2  | the faithful Canvas entries (§3)           | 1                 | a comparison per scene against the painter's frame, within a band calibrated on the first run and pinned                                                                                                                       |
| 3  | the thread-time instrument and floor (§4)  | —                 | the render gate reads a line from it; a device-free test on its arithmetic                                                                                                                                                     |
| 4  | the settle path (§5.1)                     | 3                 | render gate: draws fewer than frames on a static entry; zero managed bytes per steady frame                                                                                                                                    |
| 5  | the dirty-range pack and upload (§5.2)     | 4                 | the render gate pins the uploaded row count against the dirty spans; `unity/ffi-check` pins the packer's ranges and the stream layout; the package gate pins the whole-array upload to the full-pack branch; goldens unchanged |
| 6  | the per-command term (§6.1)                | 3, device         | the two readings, recorded                                                                                                                                                                                                     |
| 7  | in-order instanced draws (§6.2)            | 6                 | the order gate passes unchanged; goldens; the two records amended                                                                                                                                                              |
| 8  | the fast paths and specialisation (§6.3)   | 3, #1413's sweep  | conformance table extended; goldens; the sweep before and after; a test that a document with no clip boxes selects the clip-free pipeline on the wgpu painter and enables no clip keyword on the Unity painter                 |
| 9  | occlusion on the shared side (§6.4)        | 5, the C ABI gate | the shaded-area instrument's count per scene; both painters' goldens; `unity/ffi-check` on the new slice; the compositor                                                                                                       |
| 10 | the comparison table and the close (§2)    | all               | the table in `android-toolchain.md`, with the lean painter's row from `demo-android` read on the same device the same day under the stated definition; #1347 closes on that row                                                |

§6.5's asset change is folded into story 3, since the asset is read there first.

Stories 1 and 3 start now with no device; story 2 starts against §3 and cites
story 1's record once it lands. Readings for 3, 5, 6, 7, 8, 9 and 10 batch onto
the device between code stories: one device is one lane, and no recipe passes
`adb -s`; story 3's reading covers the Canvas only if story 2 has landed by
then. Stories 4, 5, 7, 8 and 9 touch files story #1412's open opaque-core branch
also touches; each reads that branch's diff when it opens and coordinates per
file with that lane. Story 9 also changes the C ABI, so it lands through
`unity/ffi-check` and the ABI version, the way every `DsFrame` change has.

## 9. Alternatives considered

- **Shader first, baseline later.** Rejected: it optimises against a number that
  does not exist, and the opaque-core projection of 7 ms was wrong by a factor
  of two in the direction that lengthened the frame.
- **Replace `BatchRendererGroup` for every class.** Cleanest parity with the
  lean painter, and the argument is real: no showcase scene draws a lit class
  today. Not taken here because D1's lit half is the product's path to
  three-dimensional content, and reversing it is the owner's call, not this
  epic's; §6.2 reverses D1 only where the cost was measured.
- **A tolerance above the Canvas, 5 %.** Proposed and refused by the owner; the
  criterion is at or below, and §6.4 is what makes "below" reachable.
- **Waiting for the depth experiment before choosing the overdraw mechanism.**
  Offered and refused: the CPU form needs no driver behaviour and is measured
  without a device, so it is committed now and the GPU form stays a possible
  further step.
- **Reducing resolution or render scale.** The budget record already rejects it
  as a route; it stays a diagnostic.
- **The cheapest possible Canvas as the baseline.** Refused as the criterion:
  flat Images and legacy Text are not the same picture. It may still be read
  once as a floor; that is not a story here.
- **Placing the work on the non-gating epic #1120.** Refused: the owner wants
  the slice held open on this criterion.
- **Occlusion in the C# packer alone.** The first draft of §6.4; refused later
  the same day when the owner asked whether the work benefits the lean painter.
  One pass on the shared side costs an ABI change and provides both painters and
  one set of tests.
- **Baking more at import.** Weighed: gradient strips as a dashpack derivation
  (microseconds at load, and an animated paint needs a runtime re-bake anyway),
  a static-or-dynamic flag per node (the dirty set already carries it),
  pre-shaped static strings and a pre-solved layout as a per-target derivation
  (startup, not the frame — the loading-performance class, not this epic), and
  pre-composited static subtrees as bitmaps (resolution-dependent, bandwidth on
  a tiler, and P1). Only the kind set survived, as §6.3's specialisation.

## 10. What this does not settle, and the risks it names

- **R-T2's wording.** The rule names the depth-tested core; §6.4 meets its
  intent by another mechanism. Story 1's record carries the reinterpretation as
  D4; the paragraph beside R-T2 in the as-built specification is written by
  story 9, in the PR that lands the pass — a rule met by an unnamed mechanism is
  the drift this repository's `check-the-specification` lesson is about, and a
  shipped record describing an unbuilt pass is the other drift.
- **The fairness rules will be argued.** That is why they are a record with a
  review, and every reading cites it.
- **The occlusion piece count** could grow the instance count on a dense scene,
  and with it the batch count `BrgPainter.InstancesPerBatch` splits it into; the
  pass caps pieces per rect, the count is derived with no device before the ABI
  changes, and the extent a piece needs rides in a side table indexed from the
  instance row's spare word rather than lengthening every row.
- **A 256-texel strip quantises stop positions**; the goldens decide, and the
  story may widen the strip.
- **The dirty set is relative to consecutive commits**; a host that skips a
  commit — not a frame — would lose it. The settle path skips only when no
  commit occurred, and `CommitPacer` at a non-zero rate skips ticks before the
  commit, not after. A test pins that.
- **The render-graph pass sits after the lit classes**, so the text-over-opaque
  gap stays. It is the same gap on both paths and is recorded, not fixed.
- **A `DsFrame` change reaches every host.** The pieces slice is a new member,
  so the desktop, web and Android hosts and the C# binding all see a new stride;
  R-E17's check and `unity/ffi-check` are what catch a host built against the
  wrong layout, and the ABI version moves with it.
- **The Canvas's own floor.** If the faithful Canvas misses the panel rate on
  `surfaces`, parity is met while the budget record's D1 is not; both bind.
