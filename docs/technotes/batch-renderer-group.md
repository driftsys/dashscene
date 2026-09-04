# Technote — BatchRendererGroup, and the ordering pitfall

Informative. What `BatchRendererGroup` is, how the Unity painter uses it, and
the one property of it that breaks a painter's-algorithm renderer with no
diagnostic. The pitfall is not hypothetical: it is why the Unity painter drew no
text in any player build on any platform (issue #1389), and it had gone
unnoticed through ten green configurations.

The same API has a **second** property that reports nothing when it is broken,
and §5d carries it: a draw command that states a sorting position may name only
one visible instance, or Unity drops commands from single frames (issue #1401).

Companion reading: `rendering-and-painters.md` §10 records **why** BRG was
chosen over a GameObject per node. This note is about how it behaves once
chosen.

## 1. What BRG is

Unity's normal path draws a `GameObject` with a `MeshRenderer` per thing on
screen. For a UI document that is thousands of objects, each with a transform
and a component, and the cost is in the bookkeeping rather than in the pixels.

`BatchRendererGroup` removes the bookkeeping. You hand Unity:

- one **mesh** — for dashscene, a unit quad, registered once;
- one or more **materials**, registered once each;
- one **`GraphicsBuffer`** holding every instance's data, laid out as you
  choose;
- a **batch**, which is a description of how to read that buffer — the metadata
  that says "the property `_DsQuad` for instance `i` lives at this offset";
- a **culling callback**, which Unity calls each time it wants to draw, and in
  which you emit **draw commands**.

A draw command says: draw this many instances, from this batch, with this
material, using this mesh. There are no GameObjects anywhere. The painter fills
a buffer and describes it; Unity draws it.

## 2. How the painter uses it

`BrgPainter` packs a committed dashscene frame into flat arrays — one row per
node — and uploads them to its instance buffer. In `OnPerformCulling` it walks
those rows in order and emits **one draw command per instance**, each carrying
`BatchDrawCommandFlags.HasSortingPosition` and one sorting key. The command
count is therefore the instance count.

**That shape is a fix, and §5d is the measurement behind it.** Until issue #1401
the callback emitted one command per contiguous run of instances sharing a
material, splitting a run past 256 instances because the SRP core shader library
declares exactly that many visible-instance slots. Unity's sorted-transparent
path drops commands from single frames under that shape; one visible instance
per command is the shape its own GPU Resident Drawer feeds the path (§3), and
the only one measured free of the defect. R-E20's 256 is satisfied without a
split at all, because one is never more than 256.

Text is what makes more than one material necessary. A glyph is sampled from a
font atlas, an atlas is a texture, and a texture is bound to a material — so a
document naming three typefaces needs three text materials over one shader,
alongside the class material that draws every non-text node. The material
therefore changes along the command list for a real document: a stretch of class
commands, then a stretch of text commands, and so on.

## 3. The contract — what BRG does and does not promise

BRG promises to draw the instances you name, with the material you name, from
the batch you name. **It does not promise to draw your commands in the order you
emitted them.**

That is ordinary renderer behaviour and it is not a defect in Unity. A renderer
is free to group work by material to avoid changing GPU state thousands of times
a frame, and grouping means reordering. Unity's own BRG sample puts several
materials in one unsorted range as normal usage. Nothing is logged when it
happens, because nothing is wrong.

BRG does give you the means to state an order, and they are per command and per
instance:

    BatchDrawCommand.sortingPosition
    BatchCullingOutputDrawCommands.instanceSortingPositions
    BatchDrawRange.filterSettings.allDepthSorted

These are not a workaround. Unity's own GPU Resident Drawer orders transparent
content through the same fields and the same public API — SRP core 17.3.0 flags
every transparent material's command with
`BatchDrawCommandFlags.HasSortingPosition`
(`Runtime/GPUDriven/InstanceCullingBatcherBurst.cs`) and writes one `float3` per
flagged command into `instanceSortingPositions`, at the float offset
`3 * commandIndex` (`Runtime/GPUDriven/InstanceCuller.cs`). A renderer that
needs an order states it; there is no path on which submission order alone is
the contract.

**The per-command instance count is a different claim, sourced differently.**
SRP core's own files establish the flag and the float3 writes above; they do not
say how many visible instances a flagged command may name. That figure —
`visibleCount = 1` — is reported by third-party analysis of the closed sorter,
matched by this repository's own measurements (§5d). §2 and §5d cite this
paragraph rather than restate the sourcing.

## 4. The pitfall

**A painter's-algorithm renderer has no order except the order it submits in, so
it must state that order explicitly — or it has no order at all.**

A dashscene document is a flat list drawn back to front, the way a painter works
on canvas: backdrop first, then panels, then glyphs on top. The two classes a
text document draws with — `UnlitOverlay` and `Text` — declare `ZWrite Off` and
`ZTest Always`, so on that path there is no depth buffer and no depth test, and
nothing but sequence decides what covers what.

**That is a property of the overlay path, not of every class.** `MaterialClass`
has three values (`PaintHeap.cs`), and `LitOpaque` and `LitCutout` declare
`ZWrite On` and `ZTest LEqual`. A painter constructed with either of those does
have a depth test — so the pitfall below is stated for the class the bulk of a
UI takes, and the keys the painter now writes are set for all three while having
been measured on one.

The Unity painter emitted its commands in exactly the right sequence and then
passed `sortingPosition = 0` on every one of them,
`instanceSortingPositions =
null`, and `allDepthSorted = false`. It stated no
order at all. It assumed the emission order survives, and across materials it
does not.

**Stating the order did not settle it, and §5b is the reason.** The painter now
sets `HasSortingPosition` and writes a key per command, which is what makes
glyphs reach the screen at all. It does not follow that the keys order the
picture, and measurement says they do not. Treat the keys as the thing that
takes the commands out of material grouping, and the emission order as still
load-bearing.

The consequence, isolated in a player build on macOS/Metal — the symptom, no
text at all, reproduces on Android/Vulkan and has not been isolated there: the
backdrop is the first row the packer writes and it sits on the class material,
so it joins the class material's group — which is drawn after the glyph
commands. **The backdrop is painted last, over the text.** The glyphs were drawn
correctly and then covered.

## 5. What the measurements showed

The rig renders one frame offscreen and counts pixels; `white` counts near-white
pixels, which for this document is the paragraph text alone.

These were taken before the painter stated any order — the configuration §4
describes, with the flag unset.

| what was drawn                                      | white |
| --------------------------------------------------- | ----: |
| that configuration                                  |     0 |
| every command forced onto one material              |  2013 |
| the glyph commands only, surfaces dropped           |  3303 |
| that configuration minus the backdrop command alone |  2977 |

The last row is the one that names the cause. Removing a single draw command —
changing no material, no batch, no shader, no registration — makes the text
appear, with all four materials still in use.

The single-material row draws because with one material there is nothing to
group, so the emission order survives. That is also why it is a symptom
treatment rather than a fix, and why it draws only 2013: with one text material,
glyph runs belonging to the other two atlases sample the wrong sheet.

## 5b. What the sorting keys were measured to do

All of this is macOS/Metal, Apple M3, Unity 6000.3.23f1, a player build, the
showcase typography scene — four materials, 381 instances, eleven draw commands
— on 2026-08-31. Every row was taken three times and did not move. `white`
counts near-white pixels, which for this document is the paragraph text alone.
The keys are built from one base point on the sheet, one direction from that
point to the camera, and a per-command step along it.

| the keys each command was given                      | white |
| ---------------------------------------------------- | ----: |
| none — the flag unset, the pre-fix state             |     0 |
| offset growing with the command index                |  3034 |
| offset shrinking with the command index              |  3157 |
| the backdrop alone nearest the camera, the rest tied |  3157 |
| the backdrop alone at the base, the rest tied        |  2836 |

**The first row is the defect and the rest are all pictures with text in them.**
Setting the flag is what changes the frame from every-surface-and-no-glyph to
something legible. That much is settled.

**The order those keys produce is not.** Three results say so:

- **Reversing the keys draws MORE of the document, not a reversed picture.**
  Under the growing offsets two of the three Arabic runs and all but the first
  two characters of the clipped line are occluded; under the shrinking ones all
  of them draw. Whichever of the two is the painter's intended order, it is not
  the one the growing offsets produce.
- **Two key sets that tie the same ten commands give different pictures.**
  Placing the backdrop alone at one extreme and tying the rest gives 3157;
  placing it at the other extreme and tying the rest gives 2836, with both
  panels emptied. **This row is weaker than the other two and is kept as such**:
  the two key sets also move the backdrop's own rank, so the difference is
  explainable without the tied group having reordered at all. Holding the
  backdrop's rank fixed and permuting only the tied group would isolate it, and
  has not been run.
- **Batch size changes the picture, and it should not.** Capping the batch
  capacity so the same document is split into eight batches rather than one
  moves `white` from 3034 to 2565. A run cannot cross a batch — `RunEnd` is
  called with the batch's own limit — so this DOES add a boundary at each new
  batch edge and takes the command count from eleven to sixteen. That is the
  point rather than a confound: splitting a contiguous same-material run into
  two commands drawn in sequence paints identical pixels, so under a mechanism
  that ordered the commands the frame would be unchanged. It is not. (This row
  needs the `AddBatches` window-offset fix in the same change to be measurable
  at all; without it Unity refuses seven of the eight batches.)

So the mechanism is **recorded as unsettled**. Do not describe this painter as
ordering its picture through `BatchDrawCommand` sorting positions, and do not
read a legible frame as evidence that it does. A test that pins the order needs
a fixture where every permutation gives a different composite — §6 — and that
fixture does not exist yet.

## 5c. Two explanations that were tested and ruled out

Both are the obvious next thing to try, so they are recorded as closed rather
than left for someone to spend a day on.

- **`allDepthSorted` left false while every command carries the flag.** Unity
  documents that range field as an assertion that every draw command in the
  range has `HasSortingPosition` set, which is now true here — SRP core leaves
  it false only because its own ranges mix flagged transparent commands with
  unflagged opaque ones. Setting it true changes nothing: 3034 and 3157 for the
  growing and shrinking keys, the same to the pixel as with it false.
- **The sort axis under an orthographic camera.** The showcase camera is
  orthographic, and an orthographic sort projects onto the view axis rather than
  taking a range from the camera position — so the shipped direction, which
  points from the sheet to the camera POSITION, spent all but 2.5 % of each step
  on axes such a sort discards. Rebuilding the keys along the camera's own
  forward (`localToWorldMatrix` column 2), which puts the whole step on the axis
  that counts, changes nothing: the same 3034 and 3157. Nor does enlarging it —
  at a step of 400 world units, spanning 4400 across the eleven commands against
  a 400-unit camera distance, the frame is still identical to one stepped by
  0.004.

Magnitude and axis therefore both drop out, and only the RANK of the keys has
any effect at all. That is the signature of a comparison sort being fed these
keys and then not ordering the draw by them.

**One construction detail, because it is easy to get wrong twice.** The keys are
laid out behind the sheet and run back toward it, so command 0 is farthest and
distance from the camera falls with the command index at any span. Walking them
toward the camera instead — the obvious way round — makes the rank fold once the
span passes the viewing distance, and the cap that prevents the fold conflicts
with the precision floor the keys need at a document far from the world origin.
Taking the smaller of those two bounds ties every key, which is this note's
whole subject. `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` D4
carries it.

One further observation, from about forty player runs: one run of the
shrinking-offset configuration came back with a frame carrying no bright pixels
at all, where every other run of that same configuration gave 3157. It was not
reproduced. It is recorded here because a renderer that usually agrees with
itself is not the same as one that always does.

## 5d. The band defect — a flagged command may name only one instance

**Unity's sorted-transparent path drops a contiguous subset of draw commands for
a single frame when a command carrying `HasSortingPosition` names more than one
visible instance.** macOS/Metal, Apple M3, Unity 6000.3.23f1, URP 17.3.0,
windowed player, vSyncCount 1, measured 2026-09-03. This is issue #1401, and it
is why `OnPerformCulling` now emits one command per instance.

The dropped frame renders the affected region as **bare backdrop** — a large
contiguous set of panels and glyph runs missing for exactly one frame, with the
rest of the document still drawn. The instrument dumps that frame and its
predecessor as a pair, and the predecessor shows the scene intact. Nothing is
logged, no exception is raised, and the painter's own culling emission is
byte-identical on the dropped frame, so no evidence of it exists on this side of
the boundary. Unity documents no restriction on how many instances a flagged
command may name.

**The instrument, and what a band-frame is.** The camera renders into an
explicit 1024x768 `RenderTexture`; a `WaitForEndOfFrame` coroutine blits it to
192x120, reads it back synchronously, and compares each cell of a 6x4 grid
against a baseline taken over frames 61 to 120, at a threshold of 8 on 0-255. A
**band-frame** is one frame on which at least four cells fire at once. Every
count below is derived the same way from that run's own log:

```text
grep "DROP" <log> \
  | awk -F'frame=' '{split($2,a," "); print a[1]}' \
  | sort -n | uniq -c | awk '$1>=4' | wc -l
```

**The instrument is not blind, and each run proves it separately.** Its
`BASELINE` line carries 17 to 24 distinct cell means where a blind instrument
reports one value in every cell, and the typography document's own looping
progress bar fires a single cell on nearly every frame throughout a run — so a
band-frame count of zero is zero coincident events on an instrument that is
otherwise firing, not an instrument that saw nothing. The after-run below logged
29,385 detector events while counting no band-frame at all.

**The arms, 20,000 frames each**, on the showcase typography scene — four
materials, 381 instances:

| the shape the painter emitted                             | band-frames |
| --------------------------------------------------------- | ----------: |
| multi-instance commands, flag set                         |         317 |
| multi-instance commands, flag set                         |         292 |
| multi-instance commands, flag set                         |         311 |
| the same, every per-frame host call stopped from frame 60 |         115 |
| flag removed, no keys                                     |           0 |
| flag removed, no keys                                     |           0 |
| flag kept, one visible instance per command               |           0 |
| flag kept, one visible instance per command               |           0 |
| the same, every per-frame host call stopped               |           0 |

The third row and the two one-instance rows were taken from the **same build** —
an environment switch, `DASHSCENE_PROBE1401_SPLIT1`, in the shelf's probe patch
flips the emission between the two shapes at runtime — so the difference between
311 and 0 is the command shape and nothing else.

**And the fix lane's own before and after**, measured on this branch rather than
cited from the arms above, same instrument, same scene, 20,000 frames each:

| commit                              | band-frames |
| ----------------------------------- | ----------: |
| `dd20a18`, the branch's base        |         410 |
| `3a39728`, one instance per command |           0 |

410 on 20,000 is 2.05 %, above the 292-to-317 range the arms measured on a
different base commit; the two bases differ in nothing else the runs varied, so
it reads as the same defect at a higher rate rather than a second one.

**What the arms rule out.** The frozen arm makes no per-frame call at all — no
tick, no draw, no upload, no bind — and still bands at 115 per 20,000, which
takes this package's own per-frame path out of the chain by construction.
Cycling the instance and heap buffers through a ring changes nothing, measured
twice; and the write hazard that would motivate a ring is documented for
`GraphicsBuffer.LockBufferForWrite`, not for `SetData`, which is what Unity's
own GPU Resident Drawer calls on one persistent buffer.

**What implicates the shape rather than the flag.** Removing the flag also gives
zero, over 40,000 frames — but that is issue #1389 restored, with every glyph
hidden behind the backdrop again. Keeping the flag and naming one instance per
command gives zero over 60,000 frames while drawing the document correctly, and
that shape matches what §3 attributes to Unity's own GPU Resident Drawer.

**What this does not establish.** Nothing here reads Unity's sort, so the
mechanism is unknown; the shape is a measured boundary, not an explanation. The
same arms were run on the Pixel 5 over Vulkan on 2026-09-03 — §5e — and the fix
is platform-independent either way. And **§5b's rows were all taken under the
multi-instance shape**, including the `RunEnd` material-run walk it names, which
this change removed: those measurements describe commands this painter no longer
emits, so the order question they left open is reopened rather than answered.
§5c's single unreproduced dark frame is **consistent** with this defect and is
not identified as it: that rig captures one frame per player run, so one dark
result in about forty runs is one dark frame in about forty, which is the same
order as the 1.5 % to 2.1 % measured here on a live host — well above the 0.58 %
(115/20,000) measured with the host frozen. Nothing re-ran it under a counting
instrument.

**The paint entry, re-measured on the fix build.** The filed configuration — the
paint entry, 16 instances, one material, one flagged command, which is issue
#1401's own reported case — was re-measured on the fix build: 0 detector events
of any kind in 20,000 frames, on a live instrument (`fixlane-after-paint.log` on
the shelf). This also corrects an earlier reading of that entry: the paint
document is a v0.3 `.dsb` predating loop tracks, so its progress bar is static
on this host and cannot animate. The pre-fix runs' oscillation on that same tile
— background rows dropping to bare backdrop on roughly 15 % of frames — was the
defect on the filed entry itself, at approximately the filed rate of 12.2 %, not
the bar animating.

| configuration                                 | DROP events |
| --------------------------------------------- | ----------: |
| paint entry (16 inst., 1 material), fix build |           0 |

The tables, the per-run logs, the anomalous frame captures and the probe patch
are on the evidence shelf at
`driftsys/dashscene-v021-lanes/probe-1401/2026-09-03-arms/RESULTS.md`, outside
this repository.

## 5e. The same arms on the Pixel 5, over Vulkan

**The defect exists on Android/Vulkan, at roughly one hundredth of the Metal
rate, and the one-instance shape is clean there too.** Pixel 5 (Adreno 620,
Android 14), Unity 6000.3.23f1, IL2CPP arm64, Vulkan chosen by `DemoBuild`,
measured 2026-09-03 — issue #1403's device half. Same instrument as §5d, ported
by one change: an Android player has no environment, so each switch arrives as
an Intent string extra (`--es probe1401_measure 1`), the mechanism the
showcase's own capture request already uses. The player paced itself at 30 fps
throughout this round — Unity's Android default with `targetFrameRate` left at
-1, which nothing in the project sets (`docs/design/android-toolchain.md`, "The
Unity host's presented rate").

**The arms, on the story branch's base `dd20a18` and on `5b279f6` with PR #1407
merged**, typography scene, 381 instances, four materials:

| build     | the shape the painter emitted                             | frames | band-frames |
| --------- | --------------------------------------------------------- | -----: | ----------: |
| `dd20a18` | multi-instance commands, flag set                         |  5,693 |           1 |
| `dd20a18` | multi-instance commands, flag set                         | 20,000 |           2 |
| `dd20a18` | flag kept, one visible instance per command (same build)  | 20,000 |           0 |
| `dd20a18` | flag removed, no keys — issue #1389's picture again       | 20,000 |           0 |
| `dd20a18` | as built, every per-frame host call stopped from frame 60 | 20,000 |           0 |
| `5b279f6` | one visible instance per command, as shipped              | 20,000 |           0 |
| `5b279f6` | the paint entry — issue #1401's filed configuration       | 20,000 |  0 (0 DROP) |

The first arm's log stream was cut at 5,693 frames by a concurrent Unity build
restarting the adb server; the player itself ran to 20,000, and its one event
stands. **Every band-frame carries §5d's signature exactly**: the six cells of
one grid row at the backdrop value for one frame, `gap=0` between them, and the
dumped frame pair shows the title, readout and bar gone to bare backdrop while
the lower text still draws. The instrument's liveness holds per run — 15 to 16
distinct `BASELINE` cells with glyphs drawn, 8 with the flag off and the glyphs
hidden, and about 24,470 single-cell events per 20,000 frames from the scene's
own pulse.

**What the counts support.** Three band-frames in 25,693 as-built frames is 1.2
× 10⁻⁴ per frame, against 1.5 % to 2.1 % on the M3. At that rate one
20,000-frame arm expects 2.3 events, so any single zero has a 10 % chance of
being luck — the flag-off and frozen arms are consistent with §5d and settle
nothing on their own. The pooled one-instance reading, 0 in 40,000 across the
same-build switch and the shipped commit, expects 4.7 and has a 0.9 % chance if
the shape changed nothing. That is the verification #1403 asked for. Frame cost
held as on macOS: typography `draw mean` 0.41 ms before and 0.42 ms after,
`tick` 0.17 to 0.18 ms both.

**A second round at the display rate found the rate is also a matter of
timing.** With the player asked for 60 Hz and Unity's optimized frame pacing on,
the render-target arms run at about 50 fps, and the **as-built** shape then gave
0 band-frames in 40,000 — a reading with a 0.9 % chance under the 30 fps rate.
The shipped shape gave 0 in 40,000 as well. So on this device the dropout
depends on pacing as well as on the command shape, the same direction as the
frozen arm's 115 against ~300 in §5d, and a run at that pacing has no positive
control: the fix's verification rests on the 30 fps arms.

**What this does not establish.** The mechanism, still. Whether the per-frame
host path matters on Android — the frozen arm's zero is a 10 % reading. Anything
about a GLES player, which was not built. Evidence, logs, the ported patches and
the frame pairs: `driftsys/dashscene-v021-lanes/probe-1403/RESULTS.md`, outside
this repository.

## 6. Why no test caught it

Two reasons, both worth avoiding again.

The gates asserted that _something_ drew, and checked instance counts. A painter
that draws every surface and no glyph passes both. **A count measures the near
side of the boundary.** What is needed is per-atlas ink attribution — for each
font sheet, at least one pixel that demonstrably came from _that_ sheet — and a
node packed after a glyph shown to paint over it.

The standalone BRG harness written to reproduce the fault could not, because it
laid its instances out in a grid where nothing overlaps. Reordering
non-overlapping quads is invisible. **A reproduction whose geometry cannot
express the bug will report the code as correct every time.**

## 7. Four smaller pitfalls in the same API

- **The RawBuffer window must be zero.** `AddBatch` takes a window offset and
  size. On the `RawBuffer` rung Unity requires both to be zero and rejects the
  batch otherwise, with the offsets belonging in the metadata instead; on the
  `ConstantBuffer` rung they carry the window. Getting this wrong refuses every
  batch after the first, and the refusal is a log line rather than an exception.
  **This painter had it wrong until issue #1389** and nothing reported it,
  because the RawBuffer rung doubles its per-batch capacity until one batch
  covers the whole document — so the offset it passed was always zero, and the
  broken path was never taken.
- **256 visible instances per draw command.** The SRP core shader library
  declares `unity_DOTSVisibleInstances[256]`, so this is a property of the
  shader rather than of the device. A run longer than that must be split. This
  painter meets it without splitting anything, because the pitfall below holds
  it to one instance per command; the bound still binds any painter that batches
  more.
- **The first sixteen bytes of every window must be zero.** A metadata value of
  0 addresses byte 0, and that is what any property Unity asks for and the
  painter does not supply resolves to.
- **A command carrying `HasSortingPosition` may name only one visible
  instance.** Unity documents no such restriction, and this one is measured
  rather than specified: the multi-instance shape was seen dropping a contiguous
  subset of commands for single frames, with no log line, no exception, and a
  byte-identical culling emission on the dropped frame (§5d). One instance per
  command is the shape §3 attributes to Unity's own GPU Resident Drawer, and the
  only shape measured free of it. Nothing on this side of the boundary reports
  the defect, so the only way to count it is to difference tens of thousands of
  rendered frames — it is visible by eye as flicker.

## 8. How to check a painter for this pitfall

Ask one question: _if the renderer reordered my draw commands by material, would
the picture change?_ If the answer is yes, the order has to be stated rather
than assumed.

The cheapest test is to remove things rather than rearrange them. Rearranging
the emission order proves nothing here — the shipped order, glyph runs first and
surfaces first all produce the same frame, because emission order is not what
reaches the GPU. Deleting the one command that overlaps everything else is what
isolates it.
