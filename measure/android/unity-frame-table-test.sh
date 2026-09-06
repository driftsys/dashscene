#!/usr/bin/env bash
# Exercises `frame-table.py`'s three Unity tables against a committed capture.
# Needs no device, no editor, no SDK and no NDK.
#
# **The Unity showcase player's lines were read by a `sed` inside
# `unity-frame-cost.sh` until story #1443**, and nothing exercised it: the
# script needs an attached device with a Unity player installed on it, so a
# pattern that stopped matching was discovered at the device — the one place
# this apparatus exists to keep clear. `frame-table-test.py` beside this file
# makes the same argument for the lean host's lines and gives the reasoning in
# full.
#
# **A committed capture, and a committed table.** The fixture is real logcat
# from the run recorded in `docs/design/android-toolchain.md`, cut down to a few
# lines of each kind, two CPU records, one corrupted line per kind, one
# duplicated record, one constructed line and three `dashscene-window` entry
# boundaries (issue #1457) — so this holds the parser to a capture, not to a
# format string. `measure/android/fixtures/README.md` says what each of those
# is for. The three expected tables beside it are diffed byte for byte.
#
# **A diff alone is not enough**, and that is why the assertions below exist. A
# golden regenerated from a broken parser is a golden that pins the break, so
# this also asserts what the fixture was built to exercise: each table reports
# its own unreadable line and tabulates neither, the row counts account for the
# de-duplicated record, every thread column carries a distinct value on some
# row, the CPU join produces a figure where it can be computed, a capture named
# like a sweep gets the sweep letter, a compositor dump with no matching layer
# is reported rather than tabulated as a zero, and a window that opens after
# the sampler's last reading reports `—` rather than a number extrapolated past
# it.
#
#     ./measure/android/unity-frame-table-test.sh

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
fixtures="${here}/fixtures"
capture="${fixtures}/unity-frame-cost.log"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

failures=0
fail() {
    echo "unity-frame-table-test: FAIL: $*" >&2
    failures=$((failures + 1))
}

# The describe and the tick rate are fixed, so the output is a function of the
# capture alone: a run that took either from the environment could not be
# diffed against a committed file.
DESCRIBE="the fixture capture"
CLK_TCK=100

for kind in unity-frames unity-threads; do
    got="${work}/${kind}.md"
    expected="${fixtures}/${kind}.expected.md"
    if ! python3 "${here}/frame-table.py" \
        --source unity-showcase \
        --table "${kind}" \
        --describe "${DESCRIBE}" \
        --clk-tck "${CLK_TCK}" \
        "${capture}" > "${got}"; then
        fail "frame-table.py reported no ${kind} sample over the fixture, which"
        fail "  holds three. Its own message is above."
        continue
    fi
    if ! diff -u "${expected}" "${got}"; then
        fail "${kind}.md does not match ${expected}."
        fail "  If the change is intended, regenerate it with the command in"
        fail "  fixtures/README.md and read the diff before committing it."
    fi
done

# `unity-cpu` takes its own invocation rather than the loop above: its rows
# join the capture to three per-entry compositor dumps `--timestats` names,
# which the other two tables never read.
TIMESTATS=(
    "${fixtures}/unity-frame-cost-entry-1-sf-timestats.txt"
    "${fixtures}/unity-frame-cost-entry-2-sf-timestats.txt"
    "${fixtures}/unity-frame-cost-entry-3-sf-timestats.txt"
    "${fixtures}/unity-frame-cost-stray-sf-timestats.txt"
)
cpu_got="${work}/unity-cpu.md"
cpu_expected="${fixtures}/unity-cpu.expected.md"
if ! python3 "${here}/frame-table.py" \
    --source unity-showcase \
    --table unity-cpu \
    --describe "${DESCRIBE}" \
    --clk-tck "${CLK_TCK}" \
    --package com.driftsys.dashscene.showcase \
    "${capture}" \
    --timestats "${TIMESTATS[@]}" > "${cpu_got}"; then
    fail "frame-table.py reported no unity-cpu row over the fixture, which"
    fail "  holds two. Its own message is above."
fi
if ! diff -u "${cpu_expected}" "${cpu_got}"; then
    fail "unity-cpu.md does not match ${cpu_expected}."
    fail "  If the change is intended, regenerate it with the command in"
    fail "  fixtures/README.md and read the diff before committing it."
fi

frames="${work}/unity-frames.md"
threads="${work}/unity-threads.md"
cpu="${cpu_got}"

# The fixture is not named `sweep-*.log`, so its sweep cell is its own stem —
# which makes a data row unambiguous to match. The header row begins `| sweep`,
# so a looser pattern would count it.
# **A basic regex, not `grep -F`.** `-F` takes the whole pattern literally,
# anchor included, so `^| unity-frame-cost | ` matched nothing and the row count
# read 0 — a test reporting a broken parser over a table that was correct. In a
# basic regex `|` is an ordinary character and `^` still anchors.
ROW='^| unity-frame-cost | '

# 1. The corrupted line is reported, and is not a row.
#
# **`grep` reads the file directly.** `producer | grep -q` closes the pipe on
# its first match and the writer dies with SIGPIPE, which `pipefail` turns into
# 141 — a failure on a MATCH. This repository's memory records that class; a
# file argument has no writer to kill.
if ! grep -qF 'carried the `unity-frames` marker and did not parse' "${frames}"; then
    fail "the corrupted line is not reported under Unreadable in unity-frames.md."
    fail "  A line the ring cut must be rejected AND said out loud; a rejection"
    fail "  nothing states reads as a run that reported fewer samples."
fi

# **Three, over four captured frame-cost lines**: three readable, one the ring
# cut, and one that is a re-read of a record already counted — de-duplicated by
# (pid, epoch), which nothing else here exercises.
frame_rows="$(grep -c "${ROW}" "${frames}" || true)"
if [ "${frame_rows}" -ne 3 ]; then
    fail "unity-frames.md holds ${frame_rows} row(s), not 3. The fixture carries"
    fail "  three readable frame-cost lines, one corrupted and one duplicated"
    fail "  record, so a fourth row is either the corrupted line half-read or"
    fail "  the duplicate counted twice."
fi

# Four: the three real ones and the constructed all-numeric line.
thread_rows="$(grep -c "${ROW}" "${threads}" || true)"
if [ "${thread_rows}" -ne 4 ]; then
    fail "unity-threads.md holds ${thread_rows} row(s), not 4."
fi

# 1b. The threads table reports its own corrupted line.
#
# **Its own, not the frames table's.** The two kinds have separate markers and
# separate unreadable lists, and a marker map that stopped matching thread lines
# would drop a ring-cut one in silence while the frames half stayed green.
if ! grep -qF 'carried the `unity-threads` marker and did not parse' "${threads}"; then
    fail "the corrupted thread-cost line is not reported under Unreadable in"
    fail "  unity-threads.md."
fi

# 1c. Every thread column carries a distinct value on at least one row.
#
# **Six columns would otherwise be an em dash in every row** — the four render
# terms, `canvas` and `gc`. No player on this device records those counters, so
# a capture alone cannot tell the columns apart: exchanging two of them, or
# narrowing the pattern so a NUMERIC reading is rejected, passes over a fixture
# of dashes. The constructed line in the fixture carries all six as numbers.
numeric_row="$(grep "${ROW}" "${threads}" | grep -F "| 3.10 |" || true)"
if [ -z "${numeric_row}" ]; then
    fail "no row of unity-threads.md carries the constructed line's render mean"
    fail "  of 3.10, so the six em-dash columns are exercised only as dashes"
    fail "  and a swap between them would pass."
fi
for cell in "2.90" "4.20" "6.30" "5.30" "640"; do
    if ! grep -qF "| ${cell} |" <<<"${numeric_row}"; then
        fail "the constructed thread row does not carry ${cell} in its own"
        fail "  column, so two of the thread columns are exchanged."
    fi
done

# 2. The CPU join produced a figure, on the row it can be computed for.
#
# **Read out of the row rather than searched for in the file.** The `wall s`
# column prints an em dash under the same conditions the CPU column does, and
# the footnote prose carries both — so a substring check over the whole table
# passes with the CPU column reporting nothing, which is the mutation
# `frame-table-test.py` records finding.
cpu_cell="$(grep "${ROW}" "${frames}" | tail -1 | awk -F'|' '{ print $(NF-1) }' \
    | tr -d ' ')"
if [ -z "${cpu_cell}" ] || [ "${cpu_cell}" = "—" ]; then
    fail "the last row of unity-frames.md reports no CPU figure (\"${cpu_cell}\")."
    fail "  The fixture carries two dashscene-cpu records placed to bracket it,"
    fail "  so an em dash here means the join stopped reading them — which is"
    fail "  what story #1443 wired ds_cpu_sampler_start into unity-frame-cost.sh"
    fail "  to make possible."
fi

# 3. `unity-cpu.md` holds exactly two rows: entry 1 and entry 3. Entry 2's
# dump carries no layer naming the package, so it must not become a third row
# of zeroes — issue #1457's own rule, restated in the mutation below.
cpu_rows="$(grep -c "${ROW}" "${cpu}" || true)"
if [ "${cpu_rows}" -ne 2 ]; then
    fail "unity-cpu.md holds ${cpu_rows} row(s), not 2. The fixture carries three"
    fail "  per-entry dumps and one names no layer for the package, so a third"
    fail "  row here is that dump read as a zero rather than as unreadable."
fi

# 3b. Entry 2 is reported under Unreadable, not folded into a row.
if ! grep -qF 'unity-frame-cost-entry-2-sf-timestats.txt' "${cpu}"; then
    fail "unity-cpu.md does not name entry 2's dump under Unreadable — a"
    fail "  compositor dump with no layer for the package must be reported,"
    fail "  never silently dropped or printed as a row of zeroes."
fi
if grep -q "^| unity-frame-cost | 2 | " "${cpu}"; then
    fail "unity-cpu.md holds a row for entry 2, whose dump names no layer for"
    fail "  the package — that dump must be Unreadable, never a row."
fi

# 3c. A `--timestats` path whose name does not fit the pattern at all is
# reported too, not silently dropped from both the table and Unreadable.
# `unity-frame-cost-stray-sf-timestats.txt` carries no `-entry-<n>-` infix, the
# shape a leftover file from a differently-shaped run or a caller mismatch
# would have.
if ! grep -qF 'unity-frame-cost-stray-sf-timestats.txt' "${cpu}"; then
    fail "unity-cpu.md does not name the stray, wrongly-named dump under"
    fail "  Unreadable — a --timestats path this parser cannot place must be"
    fail "  reported, never dropped with nothing said."
fi

# 4. Entry 1's window is bracketed by the fixture's two CPU readings and must
# carry a real figure; entry 3's window opens after the last one and must read
# `—`, never a number extrapolated past the sampler's own last reading.
entry1_cpu="$(grep "^| unity-frame-cost | 1 | " "${cpu}" | awk -F'|' '{ print $8 }' | tr -d ' ')"
entry1_ms="$(grep "^| unity-frame-cost | 1 | " "${cpu}" | awk -F'|' '{ print $9 }' | tr -d ' ')"
if [ -z "${entry1_cpu}" ] || [ "${entry1_cpu}" = "—" ]; then
    fail "entry 1's row in unity-cpu.md reports no CPU figure (\"${entry1_cpu}\")."
    fail "  Its window is bracketed by the fixture's two dashscene-cpu records."
fi
if [ -z "${entry1_ms}" ] || [ "${entry1_ms}" = "—" ]; then
    fail "entry 1's row in unity-cpu.md reports no cpu-ms-per-frame figure"
    fail "  (\"${entry1_ms}\"), though its CPU and window columns are both real."
fi
entry3_cpu="$(grep "^| unity-frame-cost | 3 | " "${cpu}" | awk -F'|' '{ print $8 }' | tr -d ' ')"
entry3_ms="$(grep "^| unity-frame-cost | 3 | " "${cpu}" | awk -F'|' '{ print $9 }' | tr -d ' ')"
if [ "${entry3_cpu}" != "—" ]; then
    fail "entry 3's row in unity-cpu.md reads \"${entry3_cpu}\" for CPU, not —."
    fail "  Its window opens after the fixture's last dashscene-cpu reading, so"
    fail "  any number here is extrapolated past the sampler rather than read"
    fail "  from a bracketing pair."
fi
if [ "${entry3_ms}" != "—" ]; then
    fail "entry 3's row in unity-cpu.md reads \"${entry3_ms}\" for cpu ms per"
    fail "  presented frame, not —. This is derived FROM the CPU figure, which"
    fail "  is itself —, so a number here means the derivation ignored it rather"
    fail "  than propagating the missing reading."
fi

# 5. The sweep letter is read off the capture's own file name.
#
# **The product depends on the stripping and the fixture does not exercise it.**
# `frame-table.py` strips a `sweep-` prefix so the sweep cell reads `A`, and
# `measure/android/unity-frame-cost.sh` then counts data rows with
# `grep -c '^| [A-Z] | '` and exits 1 on zero — so deleting the stripping would
# make the device recipe report "holds no row" over a correct table, with every
# device-free gate green. The fixture is deliberately named for what it is, so
# this runs one more pass over a copy named the way a sweep is.
cp "${capture}" "${work}/sweep-A.log"
if ! python3 "${here}/frame-table.py" \
    --source unity-showcase \
    --table unity-frames \
    --describe "${DESCRIBE}" \
    --clk-tck "${CLK_TCK}" \
    "${work}/sweep-A.log" > "${work}/sweep.md"; then
    fail "frame-table.py reported no sample over a copy named sweep-A.log."
elif ! grep -q '^| A | ' "${work}/sweep.md"; then
    fail "a capture named sweep-A.log does not produce rows beginning '| A | '."
    fail "  unity-frame-cost.sh counts its rows with that pattern and exits 1"
    fail "  when it finds none."
fi

# 6. The pid-keyed join, on a fixture the main one cannot exercise: two pids
# (a stale one and the real one) share the same sweep tag, the shape a failed
# `logcat -c` (ordinary on Android 11+, per lib.sh's own ds_logcat_clear) would
# produce. A mutation reverting the join to the sweep-keyed scheme this issue
# replaced turns entry 1's real CPU figure into `—` — checked by hand while
# writing this fixture, not re-run here — so these values being real rather
# than dashes IS the pin.
join_log="${fixtures}/unity-cpu-join.log"
tie_log="${fixtures}/unity-cpu-tie.log"
join_got="${work}/unity-cpu-join.md"
if ! python3 "${here}/frame-table.py" \
    --source unity-showcase \
    --table unity-cpu \
    --describe "the join fixture" \
    --clk-tck "${CLK_TCK}" \
    --package com.driftsys.dashscene.showcase \
    "${join_log}" "${tie_log}" \
    --timestats "${fixtures}/unity-cpu-join-entry-1-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-2-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-3-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-4-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-5-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-6-sf-timestats.txt" \
                "${fixtures}/unity-cpu-join-entry-7-sf-timestats.txt" \
                "${fixtures}/unity-cpu-tie-entry-1-sf-timestats.txt" \
    > "${join_got}"; then
    fail "frame-table.py reported no unity-cpu row over the join fixture."
fi

# 6a/6b. Entry 1's row must match the value derived by hand from pid 9002's
# own data, cell for cell — not merely "not a dash" or "not 999":
#
#   extent      1080x2340   pid 9002's own in-window frame-cost samples
#   presented   150         the dump's (BLAST) block, not the container's 999
#   dropped     0           the (BLAST) block's own count
#   cpu %       7           (2100-2000)/100/(155.0-140.0)*100 = 6.667, pid
#               9002 only, rounded to 7 for display
#   cpu ms      4.00        9.0 * 6.667 / 100 * 1000 / 150 — from the
#               UNROUNDED percentage, not the "7" the table prints
#   drawn       480         pid 9002's two in-window samples (240 each), not
#               pid 9001's "stale" one (999 frames) landing in the same window
#
# Pid 9002 outnumbers pid 9001 three samples to two across this sweep's own
# frame-cost lines, so `sweep_pid`'s mode has an actual contest to resolve —
# not a single uncontested candidate. It is also not a contest either a
# "first" or a "last sample in the sweep" resolution would happen to win the
# same way: pid 9001 owns both the chronologically first sample (epoch 145.0,
# "stale-early") AND the chronologically last one (epoch 158.0, "stale"), so
# only counting occurrences — not position — reaches the right answer. Every
# one of these cells would read differently (cpu 160, cpu ms 96.00, drawn 0)
# if resolution picked pid 9001 by any of those routes, or drawn 1479 if the
# drawn/extent join were not itself scoped to the resolved pid.
if ! grep -qF '| unity-cpu-join | 1 | 1080x2340 | 9.0 | 150 | 0 | 7 | 4.00 | 480 |' "${join_got}"; then
    fail "entry 1's row in unity-cpu-join.md does not read"
    fail "  \"| 1080x2340 | 9.0 | 150 | 0 | 7 | 4.00 | 480 |\" — the row hand-derived"
    fail "  from pid 9002's own data alone. Got: $(grep '^| unity-cpu-join | 1 | ' "${join_got}" || echo '(no row)')"
fi

# 6c. Entry 2's window is degenerate (start == end); the row must still exist,
# with the dump's own presented/dropped counts untouched, but window s, CPU
# and cpu ms per presented frame all read `—` rather than 0.0 or a fabricated
# number.
join_entry2="$(grep '^| unity-cpu-join | 2 | ' "${join_got}" || true)"
if [ -z "${join_entry2}" ]; then
    fail "unity-cpu-join.md holds no row for entry 2, whose dump is valid —"
    fail "  a degenerate window must still produce a row, with dashes for the"
    fail "  window-derived columns only."
else
    if ! grep -qF '| — | 300 | 1 | — | — | 0 |' <<<"${join_entry2}"; then
        fail "entry 2's row in unity-cpu-join.md does not read"
        fail "  \"| — | 300 | 1 | — | — | 0 |\" for window s/presented/dropped/"
        fail "  cpu/cpu-ms/drawn (\"${join_entry2}\"). A degenerate window"
        fail "  (start == end) must not print a zero or negative duration, or"
        fail "  a CPU figure derived from one."
    fi
fi

# 6d. Entry 3's dump is missing droppedFrames entirely; it must be Unreadable,
# never a row reporting dropped as zero.
if grep -q '^| unity-cpu-join | 3 | ' "${join_got}"; then
    fail "unity-cpu-join.md holds a row for entry 3, whose dump has no"
    fail "  droppedFrames field — that must be Unreadable, never a row"
    fail "  defaulting the missing field to zero."
fi
if ! grep -qF 'unity-cpu-join-entry-3-sf-timestats.txt' "${join_got}"; then
    fail "unity-cpu-join.md does not name entry 3's dump under Unreadable."
fi

# 6e. Entry 4's totalFrames is present but not a whole number (the ring cut
# it mid-digit); it must be Unreadable, never a row that coerced the field.
if grep -q '^| unity-cpu-join | 4 | ' "${join_got}"; then
    fail "unity-cpu-join.md holds a row for entry 4, whose totalFrames does"
    fail "  not parse as a whole number — that must be Unreadable, never a"
    fail "  row with the field silently coerced."
fi
if ! grep -qF 'unity-cpu-join-entry-4-sf-timestats.txt' "${join_got}"; then
    fail "unity-cpu-join.md does not name entry 4's dump under Unreadable."
fi

# 6f. Entry 5 is the other half of entry 3's check: totalFrames missing,
# droppedFrames present. Either field missing alone must be Unreadable.
if grep -q '^| unity-cpu-join | 5 | ' "${join_got}"; then
    fail "unity-cpu-join.md holds a row for entry 5, whose dump has no"
    fail "  totalFrames field — that must be Unreadable, never a row."
fi
if ! grep -qF 'unity-cpu-join-entry-5-sf-timestats.txt' "${join_got}"; then
    fail "unity-cpu-join.md does not name entry 5's dump under Unreadable."
fi

# 6g. Entry 6's window carries only a start marker (a dropped `log` call);
# the row must exist with window s reading "(open)" and cpu/cpu-ms dashes —
# not a duration computed from one boundary, and not folded into Unreadable
# either, since the dump itself is valid.
join_entry6="$(grep '^| unity-cpu-join | 6 | ' "${join_got}" || true)"
if [ -z "${join_entry6}" ]; then
    fail "unity-cpu-join.md holds no row for entry 6, whose dump is valid —"
    fail "  a one-sided window must still produce a row."
elif ! grep -qF '| — | (open) | 400 | 0 | — | — | 0 |' <<<"${join_entry6}"; then
    fail "entry 6's row in unity-cpu-join.md does not read"
    fail "  \"| — | (open) | 400 | 0 | — | — | 0 |\" (\"${join_entry6}\"). A window"
    fail "  with only a start marker must read (open), never a duration."
fi

# 6h. Entry 7's window is genuinely inverted (end precedes start), not merely
# equal (entry 2's case). The row must still exist, with window s, cpu and
# cpu ms all reading dashes rather than a negative duration.
join_entry7="$(grep '^| unity-cpu-join | 7 | ' "${join_got}" || true)"
if [ -z "${join_entry7}" ]; then
    fail "unity-cpu-join.md holds no row for entry 7, whose dump is valid —"
    fail "  an inverted window must still produce a row."
elif ! grep -qF '| — | 500 | 0 | — | — | 0 |' <<<"${join_entry7}"; then
    fail "entry 7's row in unity-cpu-join.md does not read"
    fail "  \"| — | 500 | 0 | — | — | 0 |\" (\"${join_entry7}\"). An inverted window"
    fail "  (end before start) must not print a negative duration or a CPU"
    fail "  figure derived from one."
fi

# 6i. unity-cpu-tie.log gives its one sweep an exact 1-to-1 pid tie (7001
# against 7002 across its own frame-cost samples), so `sweep_pid`'s mode has
# no majority to find. The row must still exist, with the dump's own
# presented/dropped untouched but extent, window s, cpu and cpu ms all
# reading (open)/dashes — never a pid picked by insertion order.
join_tie="$(grep '^| unity-cpu-tie | 1 | ' "${join_got}" || true)"
if [ -z "${join_tie}" ]; then
    fail "unity-cpu-join.md holds no row for unity-cpu-tie's entry 1, whose"
    fail "  dump is valid — an unresolved pid tie must still produce a row."
elif ! grep -qF '| — | (open) | 600 | 0 | — | — | 0 |' <<<"${join_tie}"; then
    fail "unity-cpu-tie's row does not read"
    fail "  \"| — | (open) | 600 | 0 | — | — | 0 |\" (\"${join_tie}\"). A 1-to-1 pid"
    fail "  tie must resolve to no pid at all, never to whichever pid the"
    fail "  Counter happened to count first."
fi

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-table-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-table-test: all three tables match, and the corrupted line is reported"
