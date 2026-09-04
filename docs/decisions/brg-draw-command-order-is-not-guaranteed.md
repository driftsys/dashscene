# BatchRendererGroup draw-command order is not a guarantee this painter may rest on

    status   **accepted (2026-08-31, issue #1389); amended 2026-09-03 (issue
             #1401) with D5**. What is accepted is the CONSTRAINT — D1–D5
             below bind downstream work now. What is not settled is the
             ordering mechanism itself, which is larger than the lane that found
             it; this record exists so the repository stops saying something
             untrue about how the picture is ordered while that remains so, and
             "What is still owed" names what would close it.
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
material per glyph atlas — declare `ZWrite Off` and `ZTest Always`, so on that
path there is no depth buffer and no depth test: sequence is the only thing that
decides what covers what.

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
the repository does not claim that those keys order the picture.**

Setting the flag and writing the keys is what takes the commands out of material
grouping, and it is what makes glyphs reach the screen at all. That is measured
and repeatable. What is **not** established is that the resulting order is the
painter's order, and three measurements say it is not — two of them cleanly, the
third only weakly. The table and the reasoning are in
`docs/technotes/batch-renderer-group.md` §5b:

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
ORDER needs a fixture where every permutation gives a different composite.

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

The same arms on the Pixel 5 over Vulkan, the same day (§5e): the multi-instance
shape 3 dropped-band frames in 25,693, the one-instance shape 0 in 40,000. On
three events the exact test puts that at 6 %, so the device round is consistent
with D5 and does not carry it alone; the macOS rows do.

**What D5 does not say.** It does not say why Unity drops those commands: the
sort is not readable from C# and no measurement here reached it. It does not say
the keys now order the picture — D1's caution stands unchanged, because §5b's
order measurements were taken under the multi-instance shape and have not been
re-run under this one.

**What it costs.** The command count becomes the instance count, and so does the
count of sorting keys that must stay distinct floats.
`docs/design/unity-csharp-host.md` carries the before/after frame-cost pair
measured across that rise, and
`unity/package-gate/tests/sorting_key_arithmetic.rs`'s
`the_floor_keeps_instance_scale_command_counts_distinct` re-checks D4's
precision floor at a command count of that order.

## What this costs, and what is still owed

The painter draws its text today and the ordering it draws it in is not
specified. Any document this painter draws with more than one material therefore
still has an order that rests on unspecified behaviour — text is only where it
was total, because a backdrop hides glyphs completely.

What is owed, and what this record does not decide:

- **§5b's order measurements, re-run under D5's shape.** Every row in that table
  was taken with the multi-instance shape D5 now forbids, so none of them
  describes the commands this painter emits. Whether one key per instance
  reproduces the emission order is therefore reopened as a question rather than
  answered: it is a follow-up story (issue #1402), and D1 and D2 stand until it
  reports.
- **The fixture.** A document with a full-bleed backdrop, text in at least two
  atlases, and a node packed after a glyph run. `goldens/dsb/` has none — the
  `v07-text-*` files are byte records pinned by `crates/dashc/tests/`, not
  renderable fixtures with atlases. Whether the gate grows an atlas-install path
  or the showcase's typography scene becomes the gate is undecided.
- **What the native sort actually does with these keys.** It is not readable
  from C#, and the measurements above rule out a plain distance sort without
  replacing it with anything.
- **Whether the keys should be kept at all** if the order they impose cannot be
  established. They are kept for now because without them no glyph draws.

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
