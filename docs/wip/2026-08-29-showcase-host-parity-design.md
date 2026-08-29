# The showcase, the same on both Android hosts

Design session of 2026-08-29. Working memory: this file is gardened into the
durable records by the branch that implements it, and is removed from
`docs/wip/` in the same commit.

## The goal

`demo-android` draws the showcase through the lean painter. A Unity player draws
it through the BatchRendererGroup painter. Both ran on one Pixel 5 on
2026-08-29. The goal of this work is that the two are **the same showcase**: the
same thing to use, measured under the same conditions, and drawing the same
pixels.

## Where the two hosts stand

Both draw the same three scenes — `surfaces`, `typography` and `layout` — and
the content is already single-sourced. `corpus/showcase::SCENES` is the
registry; `demo-android` links it directly, and the Unity sample reaches it
through the `ds_demo_*` entry points that `unity/demo-producer` exports. Nothing
in this design changes that.

What diverges is everything around the scenes. Read from the tree at `ec833ea`,
after PR #1377:

|                        | `demo-android`                                                   | Unity `Samples~/Showcase`                                                |
| ---------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------ |
| choosing a scene       | one scene per launch, from the `scene` intent extra              | left and right walk the scenes, then the committed `.dsb` documents      |
| variant switch         | not reachable; `layout::switch_variant` is never called          | the space bar                                                            |
| input on a device      | none — `setContentView(new SurfaceView(this))`, no touch handler | keyboard only, so `adb shell input keyevent` drives it and a hand cannot |
| frame cost readout     | logcat, from `demo-android/src/timing.rs`                        | an on-screen `OnGUI` label, always drawn                                 |
| terms reported         | `tick`, `paint`, `submit`                                        | `tick`, `draw`                                                           |
| build profile measured | `release`                                                        | `demo-release`                                                           |
| extent measured        | 1080x1984                                                        | 1080x2340                                                                |
| samples per scene      | 3, the first discounted as pipeline warm-up                      | 1 per sweep, so `max` carries warm-up                                    |
| pixels compared        | nothing compares the two hosts                                   | nothing compares the two hosts                                           |

The extent difference is the system bars. The consequence is recorded in
`docs/design/android-toolchain.md`: only `tick` may be subtracted between the
two tables, the Unity figure is lower on every scene by 0.51, 0.40 and 0.27, and
that record states plainly that it does not explain why. The two variables that
would explain it — the extent and the build profile — both differ.

## What "the same showcase" means here

Three things, chosen by the owner on 2026-08-29:

1. **The same thing to use.** The same scene set, the same navigation, the same
   variant switch, on both hosts, drivable by hand on a device.
2. **Comparable numbers.** One controlled run of both hosts under identical
   conditions, so the two frame-cost tables can honestly be set side by side.
3. **The same pixels, checked.** Each host's frame captured on the device for
   the same scene at the same extent, and the two compared.

A fourth reading was offered and not taken: the Unity showcase running on the
package as a customer installs it, rather than on a staged library that exports
`ds_demo_*`. That stays issue #1352.

## The contract

One normative record, `docs/decisions/the-showcase-hosts-share-one-surface.md`,
linked from `docs/design/host-integration.md`. It is written by the first story
that implements it rather than by a story of its own, because a
documentation-only story in this repository has twice produced dozens of review
findings and no executable defect.

It fixes four things.

**Five commands.** `next`, `previous`, `action`, `orientation`, `readout`. Each
is bound to a gesture and to a key event. The gesture makes the build usable by
hand; the key event makes it drivable by `adb shell input` without a second
driving path existing only for the harness.

**Entry order.** The first three entries are the scenes, in
`corpus/showcase::SCENES` order, on both hosts. Unity's `.dsb` documents are an
extension appended after them. So `next` applied twice from the start reaches
the same scene on both builds.

**Extent policy.** Both hosts run edge to edge, at the full display extent. This
removes the 1080x1984 against 1080x2340 difference, and it is what makes the
frame-cost tables comparable and a pixel comparison possible at all.

**The readout.** The same fields, in the same order, in the same units, and
suppressible on both hosts. Suppression is required because `adb screencap`
composites the readout into the captured frame.

**Capture mode**, entered by a launch parameter rather than by a gesture:
`capture <scene> <phase>` builds that scene, pins that phase, suppresses the
readout, draws a fixed number of frames and then holds. It is delivered as an
intent extra on both hosts, because both are Android activities and
`demo-android` already reads its scene selection that way. The scenes advance
their scripted phase on a timer in normal use, which makes an unpinned capture
non-deterministic. One launch parameter covers the whole need; a separate "hold
the phase" command would be a second command for the same purpose.

## The six pieces

**W1 — `demo-android` gains the shared surface.** Edge to edge; a `FrameLayout`
holding the `SurfaceView` under a suppressible `TextView` readout; a touch
handler binding swipe left and right to `previous` and `next`, a tap to
`action`, a two-finger tap to `orientation` and a long press to `readout`; the
same five commands from key events; in-app scene switching that rebuilds the
arena for the new scene at the current extent; and capture mode. Carries the
contract record. Depends on nothing.

**W2 — the Unity showcase matches it.** The same five commands bound to touch,
keeping the existing key events; readout suppression as a flag read inside
`OnGUI`; capture mode. The `.dsb` document list stays, appended after the
scenes. Depends on nothing.

**W3 — one run condition.** Both hosts built at `demo-release`, run at the same
extent and orientation, 240 frames per sample, at least three samples per scene
with the first discarded as warm-up on both sides. The Unity dwell is raised so
that three samples actually land: today the dwell yields one sample per sweep,
which is why that table's `max` carries warm-up and the lean painter's does not.
Depends on W1 and W2.

**W4 — a two-image comparison in `goldens/tooling`.** Extract the comparison
arithmetic out of `assert_matches_golden_within` into a public function over two
PNG buffers, reporting the differing fraction, the bounding box of the differing
pixels, and the maximum per-channel delta. `assert_matches_golden`,
`assert_matches_golden_within` and `assert_matches_golden_max_pixels` are
re-expressed on top of it, so the committed golden tests pin the extraction.
Device-free, depends on nothing, and can run in parallel with W1 and W2.

**W5 — the parity harness.** `measure/android/host-parity.sh` with a device-free
`measure/android/host-parity-test.sh` beside it, following the shape of
`unity-frame-cost.sh` and `unity-lifecycle.sh`. Per scene, per host: launch in
capture mode, assert the logged entry name and extent are the ones asked for,
`adb exec-out screencap -p`, then compare the pair with W4 and write the table.
`measure/android/record-check.py` re-derives every published cell. Depends on
W1, W2 and W4.

**W6 — the device pass.** W3 and W5 run on the Pixel 5. The readings go into
`docs/design/android-toolchain.md` beside the tables already there; the raw
captures are archived under `docs/archive/`. Depends on all of the above.

## The capture protocol

Per scene, per host:

1. Launch the host in capture mode, naming the scene and the phase.
2. Read back the host's logged entry name and drawable extent, and refuse the
   capture unless both are what was asked for. A host that failed to reach the
   scene must not be reported as a host that drew something different.
3. Capture with `adb exec-out screencap -p`.
4. Compare the two hosts' captures for that scene.

Notifications and system overlays are suppressed for the duration of a run. The
captured extent is asserted before any comparison, in the way
`unity-frame-cost.sh` already refuses a drifted extent rather than noting it.

## The tolerance, and what the comparison cannot see

The two painters will differ at anti-aliased edges, in gradient dithering, and
in shadow falloff. **No threshold is chosen in advance.** W5's first run
establishes the baseline and W6 pins it, so the published tolerance is a
measurement rather than a guess.

A differing fraction alone pins very little: a systematic shift confined to one
region of the frame passes a whole-frame fraction. So each scene pins three
numbers — the differing fraction, the bounding box of the differing pixels, and
the maximum per-channel delta.

One prediction, stated so that it can fail: **`typography` should be the closest
match of the three.** The Unity HLSL is generated from the same `sdf.wgsl` the
lean painter uses, and 2393 probes over 13 functions already check the generated
`Sdf.hlsl` against the committed table. If `typography` diverges more than
`surfaces` does, that is a defect in the text seam and not a tolerance to widen.

What this comparison cannot see: the two hosts agreeing and both being wrong. It
compares them to each other, not to the specification. Correctness against the
reference painter stays with `goldens/` and `conformance/`, which this work does
not touch.

## Testing

- **W1 and W2** — command-table tests in the style of `demo-android`'s existing
  `every_scene_is_reachable_by_name`: every command reachable, every scene
  reachable by `next` from the start entry. Each is mutated by flipping one
  binding, which must turn it red.
- **W3** — the sample-count and warm-up-discard arithmetic, tested away from a
  device on both sides.
- **W4** — the committed golden tests pin the extraction. Three synthetic cases
  are added directly: identical images, a single differing pixel, and a
  region-shifted image. The third is what demonstrates that a fraction alone is
  not enough.
- **W5** — `host-parity-test.sh`, device-free, beside the nine harness scripts
  already there.
- **W6** — `record-check.py` re-derives every cell published in
  `docs/design/android-toolchain.md`.

Two hazards this repository has already paid for, and which the harness work
must avoid by construction: `grep -q` reading a pipe under `pipefail` inverts
above the pipe buffer, so comparisons read from a herestring; and the Unity
readout must be suppressed by a flag read inside `OnGUI`, never by disabling the
behaviour, because that file records that disabling it took the frame loop down
with the readout.

## What this deliberately does not do

- `demo-android` does not gain a `.dsb` document path. Its own header records
  that `dashscene-android` owns that path and this host owns its arena in code.
- The showcase scenes do not gain golden images. `corpus/showcase/src/lib.rs`
  records why they have none, and that reasoning is unchanged.
- `draw` and `submit` are not unified into one term. They measure genuinely
  different spans of a frame — Unity runs its passes, culling and present after
  `Update` returns — so the record states the mapping and only `tick` is
  subtracted.
- Issue #1352, the staged library, stays debt.
- Issue #1345, the read on the target board, stays carried to v0.22.

## Alternatives considered

**One driver in Rust behind the C ABI.** Move the navigation state into
`unity/demo-producer`, re-sited as a workspace crate, and have both hosts call
it, so the state machine has one implementation and cannot drift. Rejected: it
re-sites a `publish = false` crate out of `unity/`, it adds an FFI hop to a Rust
host that has no product reason for one, and the state machine it would
single-source is about thirty lines. The part that would genuinely be worth
single-sourcing — the scene registry — already is.

**A harness-only parity.** Both hosts accept the same key events and nothing
else changes. Rejected because it leaves both builds drivable only by `adb`, and
neither is then a demonstration anyone can pick up.

**Both hosts compared against a Skia golden** rather than against each other. A
stronger claim, because each host would be pinned to the specification rather
than to the other. Rejected because it reverses the recorded decision to leave
the showcase scenes un-goldened, after which every re-authoring of a scene
churns committed images.

**Comparing the committed paint tables instead of the pixels.** Device-free and
cheap, and it would catch a document or ABI divergence. Rejected as the primary
check because it cannot see a shading difference, which is precisely where two
painters diverge.

## Placement, and its consequence

Filed as stories on **v0.21**, under epic #1107, whose Track B is Unity on
Android hardware, integration and performance. This is the owner's ruling of
2026-08-29, taken against the alternative of opening a track on v0.22.

**The consequence is stated rather than discovered later:** v0.21 was one
phase-end revision from closing. PR #1377 delivered #1329's third limb, #1346
and #1347, and #1345 is already carried to v0.22. Filing this work here holds
the slice open until it lands, and defers the phase-end revision by that much.

## Trace links

- Epic #1107, Track B — Unity on Android hardware, integration and performance.
- `docs/design/android-toolchain.md` — the two device tables this work makes
  comparable, and where W6's readings go.
- `docs/design/host-integration.md` — links the new contract record.
- `docs/decisions/unity-painter-uses-brg.md` — D4, the rung the Unity painter
  selects on this device.
- Issues #1352, #1379, #1380, #1381 and #1384 — open debt beside this work, none
  of it in scope here.
