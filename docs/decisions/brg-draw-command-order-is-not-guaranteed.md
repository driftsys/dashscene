# BatchRendererGroup draw-command order is not a guarantee this painter may rest on

    status   **accepted (2026-08-31, issue #1389); amended 2026-09-03 (issue
             #1401) with D5; amended 2026-09-05 (issue #1402): D1 and D2
                          revised on the single-instance measurement, the fixture and the
             gate exist; amended 2026-09-05 (story #1412) with D6, the opaque
             cores' shape**. What is accepted is the CONSTRAINT — D1–D6 below
             bind downstream work now. What is not settled is the mechanism
             inside Unity, which nothing reads; its observable behaviour is
             measured and pinned, and "What is still owed" names what is not.
    date     2026-08-31
    source   issue #1389; docs/technotes/batch-renderer-group.md §4 and §5b.
             D5 is issue #1401, §5d and §5e
    scope    unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs, and any
             painter that draws a dashscene document with more than one material
    related  docs/decisions/unity-painter-uses-brg.md (D1 chooses BRG and buys
             the lean painter's data model, which is not the same as buying its
             submission order)

## Context

A dashscene document is a flat list drawn back to front. The two SHADERS a text
document draws through — `UnlitOverlay`, the class material, and `Text`, one
material per glyph atlas — declared `ZWrite Off` and `ZTest Always` when this
record was written, so on that path there was no depth test: sequence was the
only thing that decided what covers what. Since story #1412 both test depth
against R-T2's opaque cores and still write none (D6), and sequence still
decides what covers what among blended fragments.

**`Text` is not a `MaterialClass`.** That enum has three values —
`UnlitOverlay`, `LitOpaque` and `LitCutout` (`PaintHeap.cs`) — and the latter
two declare `ZWrite On` and `ZTest LEqual`. This record is about the overlay
path, which is the class the bulk of a UI takes and the one every measurement
here was made on.

`BatchRendererGroup` does not promise to draw commands in the order they were
emitted, and it groups by material. That is ordinary renderer behaviour and it
is not logged. Under it the Unity painter drew every surface and no glyph, on
every platform, through ten green configurations.

`BatchDrawCommand.sortingPosition` with
`BatchCullingOutputDrawCommands.instanceSortingPositions` is the public means of
stating an order, and Unity's own GPU Resident Drawer uses exactly those fields
for transparent content on this URP version. The obvious repair is to state the
painter's order through them.

## Decision

**D1 — the painter sets `HasSortingPosition` and writes one key per command, and
under D5's shape the keys are measured to order the picture.**

Setting the flag and writing the keys takes the commands out of material
grouping — measured and repeatable since 2026-08-31 — and, **once each flagged
command names one visible instance (D5), the keys' rank is the draw order**:
Unity's sorted-transparent path draws the commands farthest-first, the painter
lays command 0 farthest, and the emission order is what reaches the screen.
Measured 2026-09-05 on the fixture this record asked for, in a macOS/Metal
player build: seven composite probes read the painter's order, negating the step
drew the backdrop last, over everything, and
`docs/technotes/batch-renderer-group.md` §5b carries the four arms. **The
composite is pinned; the keys' part in it is measured, not pinned**:
`just unity-render`'s order phase runs those probes on every pass and fails the
run on any other composite — which on this fixture the flag-off arm also
produces — and the negated-step and tied-key arms that discriminate the keys
were hand-run from the evidence shelf, not by the gate. One graphics API, as
every claim that gate makes.

The three multi-instance measurements of 2026-08-31 once said the keys did not
order the picture — two of them cleanly, the third only weakly. They were taken
under commands D5 has since forbidden, and §5b keeps them as history:

- reversing the keys draws more of the document rather than a reversed picture;
- two key sets that tie the same ten commands produce different pictures, one
  with both panels emptied — weaker than the other two, because those two sets
  also move the backdrop's own rank, so they do not isolate the tied group;
- splitting the same document into eight batches instead of one changes the
  picture, though splitting a contiguous run into two commands drawn in sequence
  paints identical pixels under any mechanism that orders them.

Two explanations were tested and ruled out rather than left open: declaring the
range `allDepthSorted` — which is the honest declaration now that every command
in it carries the flag — and rebuilding the keys along the camera's own forward
axis, which is what an orthographic sort actually projects onto. Neither moves
the frame by a pixel, and neither does enlarging the step by five orders of
magnitude. Only the RANK of the keys has any effect. §5b carries both.

**D2 — a legible frame is not evidence of a correct order, and no gate may be
written as though it were.** The four passing rows carry three distinct
near-white counts — 3034, 3157 and 2836, a spread of 10 % — while being visibly
different pictures: under the keys this painter ships, two of the three Arabic
runs and all but the first two characters of the clipped line are occluded, and
under one of the others both panels are emptied. A global "is there near-white
on screen" count separates the defect from a fix, which is what R-E22 asks of
it, and does not separate a correct order from a wrong one. Any test that pins
ORDER needs a fixture where every order of its nodes gives a different composite
— `unity/render-gate/order.json` is that fixture since issue #1402, and the
order phase of `just unity-render` is the test: it asserts seven composite
pixels and counts nothing.

**D3 — depth writes remain rejected as a way to order the OVERLAY path.** This
does not say no shipped class writes depth: `LitOpaque` and `LitCutout` both do,
and they exist for content whose silhouette is opaque. It says the overlay and
text classes may not be given depth to solve this problem. With translucent A
over B drawn top-first, B's fragments fail the test under A's quad and A
composites against the backdrop instead of against B; and every MSDF edge
fragment has fractional coverage but would write full depth, cutting a hard halo
out of anything beneath each glyph. The subset where depth is correct is "alpha
identically 1 with no anti-aliased coverage anywhere", and this content model
has no such subset, because every edge is an SDF ramp. This is not reopened by
the above.

**D4 — the keys are laid out behind the sheet, so no span can reach the
camera.** Command `c` sits `(commandCount - 1 - c)` steps on the far side of the
document from the viewer, which puts it at
`distance + (commandCount - 1 - c) *
step` from the camera: falling in `c`, at
any span, with command 0 farthest.

This is a repair of an earlier form that walked the keys TOWARD the camera,
where distance is `|distance - c * step|` and the rank folds back once the span
passes the viewing distance. Capping the span to prevent that put the cap in
direct conflict with the precision floor the keys also need — float32 resolution
is relative to the coordinate stored, so a document far from the world origin
needs a LARGER step, while a near camera admits only a smaller one — and taking
the smaller of the two rounded every key onto one float. That is the tie this
record's whole subject returns from: every command carrying an identical key is
the same thing as no command carrying one.

**The rank is unchanged by the repair**, which is what lets §5b's measurements
stand: `unity/package-gate/tests/sorting_key_arithmetic.rs` models both layouts
in `f32` and asserts they order the commands identically wherever the older one
did not fold, that the floor keeps every key distinct across the placements a
host can reach, and that distance from the camera falls with the command index
at any span. It is a model of two lines rather than a run of the painter, for
the reason every gate over this file is: nothing in CI compiles it.

**D5 — a draw command that carries `HasSortingPosition` names exactly one
visible instance.**

Unity's sorted-transparent path was measured dropping a contiguous subset of
draw commands for a single frame when a flagged command carried more than one
visible instance. The dropped region renders as bare backdrop; nothing is
logged, no exception is raised, and the painter's own culling emission is
byte-identical on the dropped frame. Unity documents no restriction on the
shape. `visibleCount = 1` per flagged command is the shape
`docs/technotes/batch-renderer-group.md` §3 attributes to Unity's own GPU
Resident Drawer, and it is the only shape measured free of the defect.

The measured basis on macOS/Metal, Apple M3, Unity 6000.3.23f1, URP 17.3.0, the
showcase typography scene, 20,000 frames per run, 2026-09-03 — dropped-band
frames per run:

- the multi-instance shape carrying the flag: 292, 311 and 317 over three runs,
  and 410 on this lane's own base commit `dd20a18`;
- the same shape with every per-frame host call stopped from frame 60: 115;
- the flag removed: 0 and 0;
- the flag kept, one visible instance per command: 0, 0 and 0.

`docs/technotes/batch-renderer-group.md` §5d carries the tables, each count's
`grep`/`awk` derivation, and the instrument's own liveness proof.

The same arms on the Pixel 5 over Vulkan, the same day, are §5e: consistent with
D5 on three events, and not carrying it alone — the macOS rows do. The counts
live there and are not restated here.

**What D5 does not say.** It does not say why Unity drops those commands: the
sort is not readable from C# and no measurement here reached it. What the keys
order under this shape is D1's, re-measured on 2026-09-05 (issue #1402).

**D6 — the opaque cores carry no sorting key and travel as multi-instance
commands in a draw range of their own, nearest first (R-T2, story #1412).**

The depth test orders a core, not the sort: a core's fragment under a
later-painted core fails `ZTest LEqual` whatever order the two commands are
drawn in, so the flag D5 is stated for has nothing to do on a core. The cores of
a batch are one command per 256 instances (R-E20's bound), walked from the
last-painted instance back so the nearer core is drawn first and rejects the
most. The blended commands keep D5's shape and D1's keys, and now test depth
(`ZTest Less`, writing none) against the cores; D3 stands, because nothing
blended writes depth.

**What is measured.** `just unity-render` draws `v03-paint.dsb`'s thirteen cores
and the order fixture's two through that shape on every pass, and the picture is
the one the flagged single-instance shape drew: the ink numbers and the seven
order probes are unchanged (2026-09-05, Metal). **What is not**: a 20,000-frame
soak on §5d's instrument, which is what would say whether the drop D5 measured
is specific to the flagged path. Issue #1404 asked that of `LitOpaque` and
`LitCutout`; the cores are the measured case, and a dropped core frame is
invisible — the fringe's interior then passes `ZTest Less` against the far plane
and draws the node whole — so the soak would decide a cost for one frame, not a
picture. Issue #1404 closes on this record with that caveat, and the soak is the
arm to run if a core is ever seen to flicker.

**What it costs.** On the target device, as written, more than it rejects: the
Pixel 5's `frameReady` cadence on `surfaces` went from 31.2 to 56.9 ms with the
cores (2026-09-05; `docs/design/android-toolchain.md`'s presented-rate section
carries the readings and the candidate cause). The command count becomes the
instance count, and so does the count of sorting keys that must stay distinct
floats. `docs/design/unity-csharp-host.md` carries the before/after frame-cost
pair measured across that rise, and
`unity/package-gate/tests/sorting_key_arithmetic.rs`'s
`the_floor_keeps_instance_scale_command_counts_distinct` re-checks D4's
precision floor at a command count of that order.

## What this costs, and what is still owed

The painter draws its text, and the order it draws it in is the emission order,
through the keys, measured on one graphics API; the composite is pinned by a
gate that runs on that API, and the keys' part in it by arms hand-run from the
evidence shelf. What is still owed, and what this record does not decide:

- **A mutation arm the gate runs.** Issue #1402 asked for the negated-step
  mutation in the gate; it was run by hand from
  `dashscene-v021-lanes/probe-1402/`, outside this repository, and nothing in
  the tree re-runs it — so a change to the key step is caught by no gate, while
  the flag-off arm shows the composite survives without the keys on this
  fixture.

- **The same gate on GLES and Vulkan.** `just unity-render` runs on the
  developer's Metal; the fleet's two APIs have no player-build reading of the
  order fixture. §5e's device arms read the band defect there, not the order.
- **What the native sort actually does with these keys.** Still not readable
  from C#. What is measured is its observable: farthest-first by rank under one
  instance per command, and the fall-back order — text materials first, the
  class material last — when every key ties.
- **Why the flag-off arm composites in order on the fixture.** #1389's flag-off
  reading, every surface and no glyph, was the typography scene under
  multi-instance commands; the fixture under single-instance commands draws in
  the painter's order with the flag off. Whether the shape or the document
  carries that difference is unmeasured, and the keys are kept because theirs is
  the path whose order is both measured and mutable.

## Alternatives considered

- **Rest on emission order, as before.** This is the defect. Rejected by
  measurement, not by argument.
- **One material for the whole document.** Removes the grouping and so restores
  emission order, and it draws — but glyph runs belonging to other atlases then
  sample the wrong sheet, which is a different wrong picture. A symptom
  treatment.
- **Write and test depth.** Rejected under D3.
- **Claim the keys work, on the strength of a green pixel count.** Rejected: the
  count that would have carried the claim is 3034 against a failing 0, and two
  later measurements show the order behind that number is not the painter's — a
  third points the same way without isolating its variable.
