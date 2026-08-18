# dashscene Android measurement bundle

    Pixel 5 (redfin), Android 14 / API 34, device
    taken 20231229T060616Z (the device's own clock)

## Device result

Name the device in whatever record cites this bundle.
`docs/design/android-toolchain.md` is where the adapter, the
storage-buffer limit and the device-request verdict belong (#885),
and the frame and CPU figures belong to #842.

## What is here

- `environment.md`
- `adapter-report.txt`
- `layer-cost.txt`
- `frames.md`
- `attach.md`
- `sf-timestats.txt`
- `sf-latency.txt`
- `gfxinfo.txt`
- `perfetto-README.md`

Per-scene logcat captures are `frames-<scene>.log`, and each script's
own transcript is `<name>.log`. The captures are the raw evidence: every
table here is derived from them and can be re-derived with
`measure/android/frame-table.py`.

## Which issue each artifact belongs to

| artifact | issue |
| --- | --- |
| `adapter-report.txt` | #885 — D3a, the Vulkan measurement |
| `frames.md`, `frames-*.log` | #842 — the showcase on device |
| `attach.md` | #960 — whether a debug attach ever completes |
| `sf-timestats.txt` | #842, and the GPU half of #1107 |
| `sf-latency.txt`, `gfxinfo.txt` | neither is the painter's frames — read their own headers |
| `layer-cost.txt` | #1128 — Q-6, the render-target budget |

The text path (#969) is the **harness** host and not this one, and it is
checked by `just android-splitscreen`, whose witness is
`assert-drew.py`. It is not in this bundle because it is a pass/fail
gate rather than a measurement.
