#!/usr/bin/env bash
# Exercises `frame-table.py`'s two Unity tables against a committed capture.
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
# duplicated record and one constructed line — so this holds the parser to a
# capture, not to a format string. `measure/android/fixtures/README.md` says
# what each of those is for. The two expected tables beside it are diffed byte
# for byte.
#
# **A diff alone is not enough**, and that is why the assertions below exist. A
# golden regenerated from a broken parser is a golden that pins the break, so
# this also asserts what the fixture was built to exercise: each table reports
# its own unreadable line and tabulates neither, the row counts account for the
# de-duplicated record, every thread column carries a distinct value on some
# row, the CPU join produces a figure where it can be computed, and a capture
# named like a sweep gets the sweep letter.
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

frames="${work}/unity-frames.md"
threads="${work}/unity-threads.md"

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

# 3. The sweep letter is read off the capture's own file name.
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

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-table-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-table-test: both tables match, and the corrupted line is reported"
