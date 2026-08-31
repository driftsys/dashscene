# Technote — BatchRendererGroup, and the ordering pitfall

Informative. What `BatchRendererGroup` is, how the Unity painter uses it, and
the one property of it that breaks a painter's-algorithm renderer with no
diagnostic. The pitfall is not hypothetical: it is why the Unity painter drew no
text in any player build on any platform (issue #1389), and it had gone
unnoticed through ten green configurations.

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
those rows in order and emits one draw command per contiguous **run** of
instances that share a material, splitting a run that grows past 256 instances
because the SRP core shader library declares exactly that many visible-instance
slots.

Text is what makes more than one material necessary. A glyph is sampled from a
font atlas, an atlas is a texture, and a texture is bound to a material — so a
document naming three typefaces needs three text materials over one shader,
alongside the class material that draws every non-text node. The command list
for a real document therefore alternates: class, text, class, text, and so on.

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

One further observation, from about forty player runs: one run of the
shrinking-offset configuration came back with a frame carrying no bright pixels
at all, where every other run of that same configuration gave 3157. It was not
reproduced. It is recorded here because a renderer that usually agrees with
itself is not the same as one that always does.

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

## 7. Three smaller traps in the same API

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
  shader rather than of the device. A run longer than that must be split.
- **The first sixteen bytes of every window must be zero.** A metadata value of
  0 addresses byte 0, and that is what any property Unity asks for and the
  painter does not supply resolves to.

## 8. How to check a painter for this pitfall

Ask one question: _if the renderer reordered my draw commands by material, would
the picture change?_ If the answer is yes, the order has to be stated rather
than assumed.

The cheapest test is to remove things rather than rearrange them. Rearranging
the emission order proves nothing here — the shipped order, glyph runs first and
surfaces first all produce the same frame, because emission order is not what
reaches the GPU. Deleting the one command that overlaps everything else is what
isolates it.
