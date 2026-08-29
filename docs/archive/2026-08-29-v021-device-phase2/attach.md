# Cold launch to first frame, by build profile

Pixel 5 (redfin), Android 14 / API 34, device

Release against debug, because `just android` builds debug and that
is the path a developer meets first. Measured on a Pixel 5 on
2026-08-17: release 0.27-0.31 s to a first frame, debug 0.93-14.57 s
across two runs — the spread is a first-launch-after-install effect
rather than steady-state, and `docs/design/android-toolchain.md` says
so. An emulator run was once abandoned at 218 s, which is what the
timeout below exists for.

`acquire` is `attaching` to `attached` — the adapter, the device and the
pipelines. `to first frame` adds the first tick and draw. `TotalTime` is
`am start -W`'s own number for the activity being displayed, which is not
the same quantity: a window can be displayed with nothing drawn in it.

`CAPTURE UNREADABLE` is not an outcome about the acquisition at all —
the capture stopped watching, so **the two interval columns are absent**
**rather than measured**. `TotalTime` still holds on such a row: it comes
from `am start -W` and not from the capture. The three ways to reach it
are a log holding nothing but logcat's preamble, a device that went away,
and a follower that exited before the wait ended.

`NO COMPLETION OBSERVED` is a bound and not a duration: the acquisition
had not returned within the wait, and nothing here says it ever would.
`attach failed` is the opposite outcome — it returned, and said no — and
reading a missing `attached` as a wedge would report both as the same
thing (issue #1080).

| profile | outcome | acquire s | to first frame s | TotalTime ms |
| --- | --- | --- | --- | --- |
| release | drew | 0.33 | 0.35 | 202 |
| debug | drew | 0.71 | 0.95 | 148 |
