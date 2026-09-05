# The showcase hosts share one surface

Status: accepted (2026-08-29)

Every host that draws the `corpus/showcase` scenes presents the same surface:
the same entry order, the same input vocabulary, the same extent policy and the
same readout rule. A host may extend it — the Unity sample appends committed
`.dsb` documents after the scenes — but it may not disagree with it.

## Why

The scene content was already single-sourced. `corpus/showcase::SCENES` is the
registry, `demo-android` links it directly and the Unity sample reaches it
through `unity/demo-producer`'s `ds_demo_*` entry points. Everything around the
scenes was implemented once per host by accident, and the hosts had drifted
apart in four ways that made them incomparable:

- **One key meant two things.** `demo/src/input.rs` binds the left and right
  keys to the two ends of the showing scene's own signal range;
  `DashsceneShowcase.cs` bound them to the previous and next entry.
- **Neither Android host was drivable by hand.** The Unity sample read only
  keys, and a phone has no keyboard; `demo-android` took no input at all.
- **The extents differed**, so neither the frame costs nor the frames could be
  compared. Measured on a Pixel 5: `demo-android` 1080x1984 against the Unity
  player's 1080x2340.
- **The readouts differed in kind** — logcat against an on-screen label — and
  neither could be suppressed, so `adb screencap` composited one of them into
  any frame captured for comparison.

## The vocabulary

Two kinds of input. Each is bound to a gesture and to a key event: the gesture
makes a build usable by hand, and the key event makes it drivable by
`adb shell input` without a second driving path existing only for the harness.

**The signal.** Every scene declares one scalar signal — `surfaces::SWEEP`,
`typography::LEVEL`, `layout::SPREAD` — authored over `0.0..=1.0`. A horizontal
drag drives it from the pointer's normalised horizontal position; the left and
right keys set it to the bottom and the top of its range.

**Five commands.** `next` and `previous` walk the entries, `action` runs the
scene's own variant switch, `orientation` swaps portrait and landscape, and
`readout` shows or hides the frame-cost readout.

|                          | gesture              | key event                           |
| ------------------------ | -------------------- | ----------------------------------- |
| signal to bottom, to top | horizontal drag      | `DPAD_LEFT` (21), `DPAD_RIGHT` (22) |
| `next`, `previous`       | swipe up, swipe down | `PAGE_DOWN` (93), `PAGE_UP` (92)    |
| `action`                 | tap                  | `SPACE` (62)                        |
| `orientation`            | two-finger tap       | `DPAD_UP` (19)                      |
| `readout`                | long press           | `R` (46)                            |

**The left and right keys mean the signal, never navigation.** This is
`demo/src/input.rs`'s binding, which is the older of the two and the one written
to name no scene, and it is the one that wins. The Unity sample's navigation
moved to the page keys on 2026-08-29.

**One of the five key events is measured on a device and the other four are
not.** `PAGE_DOWN` is; `PAGE_UP` (92), `SPACE` (62), `DPAD_UP` (19) and `R` (46)
reaching `KeyCode.PageUp`, `Space`, `UpArrow` and `R` are assumed by the same
table, on the strength of `DPAD_RIGHT` and `PAGE_DOWN` both arriving. A run
driving each of the four and reporting a distinct effect is what would settle
them.

**`PAGE_DOWN` reaching `KeyCode.PageDown` under Unity on Android is measured,
not assumed.** Taken on a Pixel 5 (`redfin`, Android 14) on 2026-08-29: a
`just unity-demo-android` cycle driven entirely by keyevent 93 walked all six
entries in order and reported each one distinctly. Had the keycode not arrived,
the run would have reported the first entry six times.

## Entry order

The first entries are the scenes, in `corpus/showcase::SCENES` order, on every
host. A host that carries more — the Unity sample's committed `.dsb` documents —
appends them after the scenes. So `next` applied twice from the start reaches
the same scene on every host, which is what lets one harness command drive both.

The count is read from the registry and never written down. `demo-android`'s
`advance` takes the entry count as a parameter for exactly this reason: with
three scenes committed, an implementation wrapping at a hard-coded three and one
wrapping over the registry are the same function, and no test can separate them
until a fourth scene lands and the hosts silently disagree.

## Extent

Both Android hosts draw at the full display extent. `demo-android` reaches it
with four changes, each of which was needed and none of which any gate here can
see — measured on the Pixel 5 on 2026-08-29:

|                                             | extent                                      |
| ------------------------------------------- | ------------------------------------------- |
| before                                      | 1080x1984                                   |
| edge to edge, system bars hidden            | 2204x948 landscape                          |
| plus `LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS` | 2340x948 landscape                          |
| plus a fullscreen theme                     | **2340x1080 landscape, 1080x2340 portrait** |

The fourth is not an extent step and so has no row: the root view consumes the
window insets (`WindowInsets.CONSUMED`). Hiding the bars and going edge to edge
still left this host at 1080x2186, because the insets were being applied to the
content; consuming them is what makes the drawable the whole display.

`mMaxBounds` for that device is 1080x2340, and the Unity player reports
2340x1080 in the same orientation. The two now agree.

## The readout

**Suppressible on both hosts, by a flag, never by disabling whatever draws it**
— `DashsceneShowcase.cs` records that disabling the behaviour took the frame
loop down together with the readout. That is the half of the drift this contract
closes, and it is closed on both.

**The fields are not yet the same, and this records the difference rather than
asserting it away.** `demo-android` draws entry, extent, then `tick`, `paint`,
`submit`, `p50`, `p95`, `max` and a frame count, on screen. The Unity sample's
on-screen label carries the entry, the rung, the instance count and the
diagnostics, and its frame cost — entry, extent, frames, `tick`, `draw` mean,
`p50`, `p95`, `max` and an unpaced-fps figure — goes to logcat through
`DashsceneFrameCost.Line()`. So one host shows its frame cost and the other logs
it, and the two orders differ in where the frame count sits. Bringing the Unity
frame cost on screen is what closes this; until then a reader comparing the two
readouts is comparing two shapes.

The shape is shared and the term names are not. Each host names its own measured
spans, because `paint` and `submit` on the lean host and `draw` on the Unity
host measure genuinely different parts of a frame: Unity runs its passes, its
culling and its present after `Update` returns. Printing them under one word is
the error `demo/src/shell.rs` warns about.

## Capture mode

`capture <scene> <phase> <signal>`, delivered as three intent extras
(`--es capture_scene`, `--ei capture_phase`, `--ef capture_signal`). The host
builds that scene, pins that phase, sets that signal, suppresses the readout,
and holds that state for as long as the launch runs. **Neither host counts
frames**, and this record asked for a fixed count before either was written:
what the comparison needs is the state pinned, and a `screencap` takes whatever
frame is on the display when it runs.

**All three or none.** A capture with a defaulted phase or signal photographs a
different state than the other host is holding, and a comparison fed by it would
be meaningless rather than merely wrong. A partial set is not a capture, and the
host runs the demonstration instead.

**An unknown scene name is refused rather than defaulted**, which is the
opposite of the demonstration launch's rule and deliberately so: drawing
something anyway is right for a demonstration and wrong for a measurement, where
it photographs the wrong scene silently.

The phase and the signal are both pinned because a scene's appearance is a
function of both. Two hosts captured at one phase and two signal values differ
for a reason that has nothing to do with either painter.

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
neither is then a demonstration anyone can pick up by hand.

**Both hosts compared against a Skia golden** rather than against each other. A
stronger claim, because each host would be pinned to the specification rather
than to the other. Rejected because it reverses the recorded decision to leave
the showcase scenes un-goldened, after which every re-authoring of a scene
churns committed images. `corpus/showcase/src/lib.rs` carries that reasoning.

**Comparing the committed paint tables instead of the pixels.** Device-free and
cheap, and it would catch a document or an ABI divergence. Rejected as the
primary check because it cannot see a shading difference, which is precisely
where two painters diverge.

**The Unity sample running on the package as a customer installs it**, rather
than on a staged library exporting `ds_demo_*`. Offered on 2026-08-29 and not
taken; it stays issue #1352.

## What this obliges

- A new command is added here first, then to every host that draws these scenes.
- Anything driving a host over `adb` sends the key events in the table above.
  Four call sites sent `DPAD_RIGHT` to walk entries and now send `PAGE_DOWN`:
  `measure/android/unity-frame-cost.sh`, and three in the `unity-demo-android`
  recipe including the hint it prints and the comment explaining the choice.
- `measure/android/unity-lifecycle.sh` sends `DPAD_UP` for orientation, which
  collides with nothing and is unchanged.
