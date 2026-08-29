# Unity on a device — the raw bundles behind #1346 and #1347

Taken 2026-08-29 on a Google Pixel 5 (`redfin`), Android 14 / API 34, Adreno
620, over USB. **These are device measurements**, not emulator results.

The branch is `story/v021-unity-device-runs`, based on `origin/main` at
`0e818315`. It predates PR #1372's change to the Unity painter's buffer binding
(`26c3955f`), so every figure here sits on one side of that change. The Unity
player was built at commit `0cf9024b`.

`docs/design/android-toolchain.md` carries the reading. This directory carries
what it was read from, so the tables can be re-derived rather than trusted.

    unity-frame-cost/          issue #1347 — the Unity painter, Vulkan, on the
                               `RawBuffer` rung. `unity-frames.md` plus the
                               three lettered sweeps it was written from
    unity-frame-cost-default-api/
                               the same sweeps from a player built with Unity's
                               DEFAULT graphics API selection, which chose
                               OpenGL ES and the `ConstantBuffer` rung, with
                               their own captures beside them. Kept because it
                               is the other real answer about this device, and
                               because its own extent column caught the rotation
                               drifting between sweeps — sweep A landed at
                               2340x1080 and sweeps B and C at 1080x2340
    unity-lifecycle/           issue #1346 — rotation, backgrounding and
                               split-screen, with the capture each verdict was
                               taken from
    unity-lifecycle-default-api/
                               the same three cases against the OpenGL ES
                               player, run 28 minutes earlier. **Its verdicts
                               predate the check that detects a case which
                               never happened** — read the paragraph below
                               before any row in it
    lean-painter/              `demo-android` over the same three scenes on the
                               same device the same day, for the comparison.
                               Also bears on issue #1304, whose remaining debt
                               is a device re-run of the converted capture path

**The device's clock is unset**, so every `taken` line in these files reads
2024-01-09 while the host's reads 2026-08-28/29. Every interval inside them is
device-clock to device-clock and is correct; the provenance is what the two
clocks disambiguate, which is the second half of issue #1236.

## Three of the four default-API lifecycle rows are not results

`unity-lifecycle-default-api/unity-lifecycle.md` answers `survived` in all four
rows. Only `backgrounded and resumed` is a verdict its own capture supports.

That run took its verdict from whether the process was still alive and reporting
frames. It did not check whether the event under test had changed anything, so a
case that never happened reads the same as a case that was survived and passed.
`unity-lifecycle/` ran after the extent check was added and answers
`NOT EXERCISED` where this run answers `survived`.

**Both rotation rows report `1080x2340`, which is the portrait extent the player
started at.** The landscape row in `unity-lifecycle/` reports `2340x1080`. So
the player did not rotate in this run, and the two rotation rows describe an
event that did not occur. `docs/design/android-toolchain.md` states the same
failure in prose — "the first run of this apparatus reported all three rotation
cases as `survived` against a player that never rotated" — and this directory is
the capture that sentence is about.

**The split-screen row divides into a part this capture settles and a part it
cannot.** Every frame-cost line in `lifecycle.log`, before and after the
cold launch, reports `1080x2340`, so the drawable never changed and that half is
derivable here rather than inferred. Whether the activity entered multi-window
at all is not: this run logs no windowing-mode diagnostic of any kind, because
that instrumentation did not exist yet. `unity-lifecycle/` has it and reads
`windowing mode fullscreen`. Issue **#1381** carries what a route from adb alone
on a handset would need.

**It is kept because the failure is the evidence.** The archive holds what a
reading was taken from so it can be re-derived rather than trusted, and the
design record's account of why the extent check exists had no artifact behind it
until this directory. `target/` is not tracked, so a `cargo clean` would have
destroyed it.

## Both lifecycle runs are stamped `commit 0cf9024b`, and one of them cannot be

The `commit` line in each `unity-lifecycle.md` is `git rev-parse --short HEAD`
taken when the script ran, and no measure script records whether the tree was
clean. `unity-lifecycle/lifecycle.log` contains a `[showcase] graphics:` line,
which the showcase source does not emit at `0cf9024b` — it was added at
`bb28f80` — so that capture ran against code newer than the commit it names,
from a tree with uncommitted changes. The stamp is accurate about `HEAD` and
misleading about the code. `unity-lifecycle-default-api/` carries no such line
and its stamp is consistent with its contents. Issue **#1382** carries the fix.

**The two painters' extents differ** — 1080x2340 for the Unity player, which is
full-screen, against 1080x1984 for `demo-android`, which is not. Every row in
both tables names its own extent, which is the first half of #1236.
