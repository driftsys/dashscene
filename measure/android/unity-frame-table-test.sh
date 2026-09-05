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
# from the run recorded in `docs/design/android-toolchain.md`, cut down to three
# lines of each kind, two CPU records and **one hand-corrupted line** — so this
# holds the parser to a capture, not to a format string. The two expected tables
# beside it are diffed byte for byte.
#
# **A diff alone is not enough**, and that is why the assertions below exist. A
# golden regenerated from a broken parser is a golden that pins the break, so
# this also asserts the three things the fixture was built to exercise: the
# corrupted line is reported and not tabulated, the CPU join produces a figure
# on the row it can be computed for, and both tables carry every readable line.
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

frame_rows="$(grep -c "${ROW}" "${frames}" || true)"
if [ "${frame_rows}" -ne 3 ]; then
    fail "unity-frames.md holds ${frame_rows} row(s), not 3. The fixture carries"
    fail "  three readable frame-cost lines and one corrupted one, so a fourth"
    fail "  row is the corrupted line half-read and a third missing one is a"
    fail "  readable line rejected."
fi

thread_rows="$(grep -c "${ROW}" "${threads}" || true)"
if [ "${thread_rows}" -ne 3 ]; then
    fail "unity-threads.md holds ${thread_rows} row(s), not 3."
fi

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

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-table-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-table-test: both tables match, and the corrupted line is reported"
