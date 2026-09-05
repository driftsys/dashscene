# What the two Android hosts drew, 2026-08-29

Six captures, one per host per scene, taken on a Pixel 5 (`redfin`, Android 14,
Adreno 620, Vulkan) with both hosts in capture mode at `phase 0`, `signal 0.5`,
at **2340x1080** — the same extent, which is what makes them comparable at all.

    just android-apk release          # the lean host, at commit 962ec74
    just unity-demo-android           # the Unity host, at commit 6c083ee
    adb shell am start -n <host> --es capture_scene <scene> \
        --ei capture_phase 0 --ef capture_signal 0.5
    adb exec-out screencap -p > <host>-<scene>.png

Compared with `cargo run -p goldens --bin compare-images`.

## The noise floor is one to three levels, and it is systematic

At threshold 0 the `layout` pair differs in **99.98%** of its pixels while
looking identical. Sampled at three coordinates spread across the frame, the two
hosts differ by one to three levels per channel at every one of them:

    (10,10)     left [14, 18, 30]    right [14, 18, 29]
    (400,120)   left [33, 41, 59]    right [32, 40, 59]
    (900,640)   left [84, 170, 158]  right [84, 170, 155]

**So a differing-pixel count taken at threshold 0 says nothing about two
painters.** It is the right measure for a golden, where the reviewed image comes
from the same painter, and the wrong one here. The threshold sweep below is what
separates this floor from a real difference:

    threshold   differing   bounds
    0           99.98%      whole frame
    2            7.37%      shrinking
    3            1.68%      shrinking
    8            1.67%      [586,916 - 1753,989]   <- stable
    32           1.67%      [586,916 - 1753,989]

Above threshold 3 the count stops moving and the bounding box collapses onto one
band. **A fraction alone could not have found that band**, which is why
`Comparison` carries bounds and a maximum channel delta.

## The three scenes, at threshold 8

    scene        differing   bounds                  max delta
    surfaces      31.10%     [50,50 - 2289,808]        240
    typography     2.36%     [62,78 - 2106,794]        222
    layout         1.67%     [586,916 - 1753,989]      210

**`surfaces` is explained and is not a defect.** The Unity painter reports five
refusals on it in terms — shadows, backdrop blurs, image fills, baked vector
nodes and render-target groups, over 9 rects — so the two hosts are drawing
different pictures on purpose.

**The design session predicted that `typography` would be the closest match of
the three, and that is wrong.** `layout` is closer. The prediction's reasoning —
that Unity's HLSL is generated from the same `sdf.wgsl` and 2393 probes check it
— does not survive the finding below. What the prediction got right is its
actionable half: it said that `typography` diverging *more than `surfaces`*
would mean a defect in the text seam rather than a tolerance to widen, and
`typography` is nowhere near `surfaces`.

## The finding: the Unity host draws no text on this device

`unity-typography.png` carries the scene's panels, its bar and no glyphs at all.
`lean-typography.png` carries the heading, the Arabic run, the signal-driven
speed string, the wrapped paragraph and the clipped line.

**It is not a capture-mode artifact.** The player was then run normally and
walked to the entry by hand with keyevent 93; `unity-typography-normal.png` is
that frame and it carries no glyphs either, while the player's own IMGUI readout
renders. So the failure is specific to the dashscene MSDF glyph path.

**The host reports success throughout.** `[showcase] drew scene typography: 381
instance(s), rung RawBuffer` — no refusal, no diagnostic, no exception. The
instance count is consistent with the glyphs having been submitted.

**Why no gate saw it.** `just unity-demo-android` asserts that every entry drew
and reads the per-entry frame cost; it never compares a frame against anything,
and 381 submitted instances satisfy every check it makes. The text seam was
verified on macOS/Metal, where it draws. Nothing had compared an Android frame
against a reference until this pair.

This is a device-and-API-specific gap between what the painter reports and what
reaches the display, and it is not fixed here.
