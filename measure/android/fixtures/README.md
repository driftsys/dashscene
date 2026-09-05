# Capture fixtures

Real logcat, cut down, committed so a parser that only runs at a device has a
test that does not need one. `../unity-frame-table-test.sh` reads them and
`just harness-tests` runs it, so `just build` and CI do.

## `unity-frame-cost.log`

The Unity showcase player's two instrument lines and the device-side CPU
sampler's, taken from the run recorded under "The thread-time line, and the URP
floor" in `docs/design/android-toolchain.md`. It holds:

- three `[showcase] frame cost` lines and three `[showcase] thread cost` lines,
  across two entries, so the row numbering per entry is exercised;
- two `dashscene-cpu` records, **placed to bracket the last frame-cost sample**
  — one at or before the interval's start and one inside it, which is what
  `cpu_over` needs. A pair placed anywhere else would leave every CPU cell an em
  dash and the join would be exercised by nothing;
- **two hand-corrupted lines**, one of each kind, cut where the logcat ring cuts
  a record: after the last field the anchor requires. Each must appear under its
  own table's `Unreadable` heading and must not become a row. One of each kind,
  because the two have separate markers and separate unreadable lists;
- **one duplicated record** — a frame-cost line repeated verbatim, the same pid
  and the same epoch, which is what `-T 1` re-printing the newest record of the
  previous capture produces. It must be de-duplicated and must not become a
  second row, so the frames table holds three rows over five captured frame-cost
  lines: three readable, one cut, one repeated;
- **one constructed thread-cost line**, the only line here carrying a number
  that was never captured. Its four unrecorded terms carry distinct numbers
  instead —
  `render
  mean 3.10 p50 2.90 p95 4.20 max 6.30 ms, canvas 5.30 ms, gc 640 B/frame`
  — and its epoch is moved one millisecond so it is its own record.

  **Why it has to be constructed.** No player on this device records the
  render-thread, Canvas or allocation counters — the demo player is
  `BuildOptions.None` and draws no Canvas — so a fixture cut only from real
  capture holds an em dash in six of the thread table's columns on every row.
  Three mutations pass over such a fixture, and all three were run: exchanging
  the render mean with its p50, exchanging `canvas` with `gc`, and narrowing the
  pattern so a numeric reading is rejected outright. Story #1444's Canvas player
  is the first thing that produces numbers in those columns, and it would
  otherwise meet an untested path.

The capture is `adb logcat -v epoch -d`, which is the only format
`frame-table.py` reads. Captures taken before story #1443 are in logcat's
default `threadtime` format and are not re-readable by it; `record-check.py`
reads those directly.

## Regenerating the expected tables

Only after reading the diff — a golden regenerated from a broken parser pins the
break, which is why the test asserts properties of its own beside the diff: that
each table reports its own unreadable line, that every thread column carries a
distinct value on some row, that the CPU join produces a figure, and that a
capture named like a sweep gets the sweep letter.

```sh
cd "$(git rev-parse --show-toplevel)"
for kind in unity-frames unity-threads; do
  ./measure/android/frame-table.py \
    --source unity-showcase \
    --table "${kind}" \
    --describe "the fixture capture" \
    --clk-tck 100 \
    measure/android/fixtures/unity-frame-cost.log \
    > "measure/android/fixtures/${kind}.expected.md"
done
```

`--describe` and `--clk-tck` are fixed here and in the test, so the output is a
function of the capture alone.
