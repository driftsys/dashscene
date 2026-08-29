# Cold launch to first frame, by build profile

Pixel 5 (redfin), Android 14 / API 34, device

Release against debug, because `just android` builds debug and that
is the path a developer meets first. Measured on a Pixel 5 over three
runs: release 0.27-0.33 s to a first frame, debug 0.93-14.57 s. Two of
the three debug runs agree at about 0.95 s and one is fifteen times
larger, cause unknown. Every run is a first launch after install —
this loop uninstalls and installs before each profile — so that is
not what the spread is, and `docs/design/android-toolchain.md` says
what it is not. An emulator run was once abandoned at 218 s, which is
what the timeout below exists for.

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

`launch` is `first-after-install` or `later`. **Every figure this
script produced before 2026-08-29 was the first kind**, because it
uninstalled and installed before every profile unconditionally — so a
spread between two of its rows could never be a first-launch effect,
which is what the design record read one as until then (issue #960).
A `later` row is the same build launched again with no reinstall
between.

| profile | launch | outcome | acquire s | to first frame s | TotalTime ms |
| --- | --- | --- | --- | --- | --- |
| release | first-after-install | drew | 0.31 | 0.34 | 205 |
| release | later | drew | 0.26 | 0.28 | 179 |
| debug | first-after-install | drew | 0.73 | 0.97 | 203 |
| debug | later | drew | 0.68 | 0.91 | 182 |
