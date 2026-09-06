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

Issue #1457 added six `dashscene-window` lines, each stating the app's own pid
(`pid=23903`, the same pid every other line in this capture carries) rather than
a sweep letter — the reason `WINDOW`'s own comment gives: `entry 1
start`,
`entry 1 end`/`entry 2 start` (the same instant — one entry's clear is the next
one's window opening) and `entry 3 start`/`entry 2 end` likewise, then
`entry 3 end`. `entry 1`'s window is placed to bracket the two `dashscene-cpu`
records above; `entry 3`'s opens **after** the second one, so its window has a
`before` reading and no `within` one — `cpu_over` returns `None` for it, and the
row must print `—` rather than a number extrapolated past the sampler's last
reading.

## `unity-frame-cost-entry-{1,2,3}-sf-timestats.txt`

One `dumpsys SurfaceFlinger --timestats -dump` per entry, issue #1457's join
target for the `unity-cpu` table. Entries 1 and 3 are cut down from the same run
`unity-frame-cost.log` came from
(`driftsys/dashscene-v021-lanes/probe-1443/gpu-thread-cost/sf-timestats.txt`
carries the field names, `layerName` and `totalFrames` among them).

**Entry 2 is hand-constructed with the package's own layer block removed** —
only the `none` layer other apps' traffic produces is left. No player on this
device fails to draw, so a real dump never lacks the layer; this is the case
`read_timestats` must still cover: a missing layer is reported under
`unity-cpu.md`'s own `Unreadable` heading and must not become a row of zeroes.

## `unity-frame-cost-stray-sf-timestats.txt`

**Named wrong on purpose** — no `-entry-<n>-` infix, so `TIMESTATS_NAME` does
not match it at all, the shape a leftover file from a differently-shaped run or
a caller mismatch would have. Every `--timestats` path is one the caller named
specifically, unlike a logcat line sharing its stream with content this parser
has no opinion about, so one this pattern rejects must still be reported —
`unity_cpu_rows` folding it in with nothing said was a real defect this fixture
exists to hold shut.

## `unity-cpu-join.log` and `unity-cpu-join-entry-{1..7}-sf-timestats.txt`

Not diffed against a golden — these are checked with targeted assertions in
`unity-frame-table-test.sh` (its own "6." section) instead, because what they
pin is a join between two pids, not one table's whole shape.

`unity-frame-cost.log` above carries one pid end to end, so it cannot exercise
the pid-keyed compositor-window join itself: a mutation reverting that join back
to inferring identity from a sweep letter produced a byte-identical golden
against it. `unity-cpu-join.log` gives one sweep tag two pids instead — `9002`
("real") and `9001` ("stale"), the shape a failed `logcat -c` (ordinary on
Android 11+, per `lib.sh`'s `ds_logcat_clear`) would leave in one capture file.
Pid `9002` carries three frame-cost samples against pid `9001`'s two, so
`sweep_pid`'s mode vote has an actual contest to resolve — and pid `9001` owns
both the chronologically first sample and the chronologically last one, so
neither a "first" nor a "last" resolution would happen to agree with the mode by
coincidence; pid `9001`'s in-window sample lands inside entry 1's real window
with an implausible frame count (999) so that it is excluded is observable in
`drawn frames (player)` rather than assumed.

Its seven timestats dumps each isolate one further gap a review round found
unpinned: entry 1's carries two candidate layer blocks — a container and a
`(BLAST)` child with different `totalFrames` — so the `(BLAST)` preference has
something to choose between; entry 2's is a plain valid dump paired with a
degenerate window (`start == end` in the log, a re-read or a device clock step)
so the `span <= 0` guard has a real row to blank out rather than a window that
was never bracketed at all; entry 3's omits `droppedFrames` entirely and entry
5's omits `totalFrames` entirely, so either required field missing alone is
covered; entry 4's carries a `totalFrames` value that is present but does not
parse as a whole number, the "ring cut it mid-digit" case distinct from a
field's outright absence; entry 6's window carries only a `start` marker, so
`window_span_cell`'s `(open)` branch has a real one-sided window to print rather
than never firing; entry 7's window is genuinely inverted (`end` before
`start`), not merely equal, so the `span <= 0` guard is exercised on a negative
span and not only a zero one.

## Regenerating the expected tables

Only after reading the diff — a golden regenerated from a broken parser pins the
break, which is why the test asserts properties of its own beside the diff: that
each table reports its own unreadable line, that every thread column carries a
distinct value on some row, that the CPU join produces a figure, that a capture
named like a sweep gets the sweep letter, that a compositor dump with no
matching layer is reported rather than tabulated as zero, that a window past the
sampler's last reading reads `—` rather than an extrapolated number, and that a
`--timestats` path this parser cannot place is reported rather than silently
dropped.

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
./measure/android/frame-table.py \
  --source unity-showcase \
  --table unity-cpu \
  --describe "the fixture capture" \
  --clk-tck 100 \
  --package com.driftsys.dashscene.showcase \
  measure/android/fixtures/unity-frame-cost.log \
  --timestats measure/android/fixtures/unity-frame-cost-entry-1-sf-timestats.txt \
              measure/android/fixtures/unity-frame-cost-entry-2-sf-timestats.txt \
              measure/android/fixtures/unity-frame-cost-entry-3-sf-timestats.txt \
              measure/android/fixtures/unity-frame-cost-stray-sf-timestats.txt \
  > measure/android/fixtures/unity-cpu.expected.md
```

`--describe` and `--clk-tck` are fixed here and in the test, so the output is a
function of the capture alone.

## What this fixture cannot catch

`unity-frame-cost.sh` clears the compositor's timestats between entries so each
window starts empty; nothing here exercises that `-clear` itself happened,
because these fixtures are dumps already taken, not a live compositor. A build
with `-clear` removed would still pass every check above — no automated gate in
this repository refuses that build. The device run in
`docs/design/android-toolchain.md`, "CPU per presented frame", is what catches
it, and only by manual inspection: each entry's `presented frames` is read
against the dwell's own frame budget at 60 Hz (20 s ≈ 1200), and a count near
double that would say `-clear` did not bound the window. Reading that comparison
is not automatic, and nothing in this repository fails a run for skipping it.
