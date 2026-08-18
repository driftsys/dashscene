# Cold launch to first frame, by build profile

Pixel 5 (redfin), Android 14 / API 34, device

Issue #960's standing measurement is 0.74 s in release against no
observed completion in debug, abandoned after 218 s. `just android`
builds debug, so debug is the path a developer meets first.

`acquire` is `attaching` to `attached` — the adapter, the device and the
pipelines. `to first frame` adds the first tick and draw. `TotalTime` is
`am start -W`'s own number for the activity being displayed, which is not
the same quantity: a window can be displayed with nothing drawn in it.

| profile | outcome | acquire s | to first frame s | TotalTime ms |
| --- | --- | --- | --- | --- |
| release | drew | 0.31 | 0.34 | 222 |
| debug | drew | 0.69 | 0.93 | 183 |

`NO COMPLETION OBSERVED` is a bound and not a duration: the acquisition
had not returned within the timeout, and nothing here says it ever would.
`attach failed` is the opposite outcome — it returned, and said no — and
reading a missing `attached` as a wedge would report both as the same
thing (issue #1080).
