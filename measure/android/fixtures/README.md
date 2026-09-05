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
- **one hand-corrupted line**: a frame-cost line cut inside its tail, the way
  the logcat ring cuts a record. It must appear under the table's `Unreadable`
  heading and must not become a row.

The capture is `adb logcat -v epoch -d`, which is the only format
`frame-table.py` reads. Captures taken before story #1443 are in logcat's
default `threadtime` format and are not re-readable by it; `record-check.py`
reads those directly.

## Regenerating the expected tables

Only after reading the diff — a golden regenerated from a broken parser pins the
break, which is why the test asserts three properties of its own beside the
diff.

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
