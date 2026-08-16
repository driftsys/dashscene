#!/usr/bin/env bash
# Exercises verdict.sh beside it. Needs no device, no SDK and no NDK.
#
# `just android-splitscreen` runs this before it does anything expensive, so a
# broken verdict fails in milliseconds rather than after a cross-compile, an APK
# build, an install and ten minutes on an emulator. It can also be run directly:
#
#     ./crates/dashscene-android/harness/verdict-test.sh
#
# **Every case below is a false verdict that reached review**, not a
# hypothetical. Three versions of this check have existed and each got a
# different pair wrong, which is why the two superseded ones are kept here and
# compared: a table of what each answers is the only durable record of why the
# shipped one is shaped as it is.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./verdict.sh
. "${here}/verdict.sh"

E="I dashscene: harness: surfaceDestroyed — entering the handshake"
C="I dashscene: harness: surfaceDestroyed — handshake complete, returning"
N="I dashscene: harness: surfaceDestroyed — no runtime handle, nothing to hand back"
D="I dashscene: harness: surfaceDestroyed — no drawable extent was ever reported, so nothing was started"

# What `main` carried: substring presence, no counting and no baseline.
presence() {
    case "$1" in *"no runtime handle, nothing to hand back"*) echo "FAIL:no-handle"; return;; esac
    case "$1" in *"entering the handshake"*) :;; *) echo "FAIL:never-destroyed"; return;; esac
    case "$1" in *"handshake complete, returning"*) :;; *) echo "FAIL:never-returned"; return;; esac
    echo "PASS"
}

# The first revision of PR #1177: counted and paired, but not baselined.
paired() {
    ds_tally "$1"
    if [ "${ds_entering}" -eq 0 ]; then echo "FAIL:never-destroyed"; return; fi
    if ! ds_balanced; then echo "FAIL:entered-never-returned"; return; fi
    if [ "${ds_complete}" -eq 0 ]; then echo "FAIL:no-handshake-ran"; return; fi
    echo "PASS"
}

# The shipped one. Baselines are what the recipe observes before the split.
shipped() { ds_tally "$1"; ds_verdict "$2" "$3" "$4"; }

fails=0
check() {
    if [ "$2" = "$3" ]; then
        printf '  ok   %-40s %s\n' "$1" "$3"
    else
        printf '  FAIL %-40s expected %s, got %s\n' "$1" "$2" "$3"
        fails=$((fails + 1))
    fi
}

echo "verdict.sh — the split transition produced:"
check "a cycle that completed"          PASS "$(shipped "$E
$C
$E
$C" 1 1 0)"
check "nothing at all"                  FAIL:split-destroyed-nothing "$(shipped "$E
$C" 1 1 0)"
check "a cycle that never returned"     FAIL:entered-never-returned "$(shipped "$E
$C
$E" 1 1 0)"
check "a cycle with no drawable extent" FAIL:split-ran-no-handshake "$(shipped "$E
$C
$E
$D" 1 1 0)"
check "a cycle with no runtime handle"  FAIL:split-had-no-handle "$(shipped "$E
$C
$E
$N" 1 1 0)"
check "a no-handle AND a completion"    FAIL:split-had-no-handle "$(shipped "$E
$C
$E
$N
$E
$C" 1 1 0)"
check "a completion, after a pre-split no-handle" PASS "$(shipped "$E
$N
$E
$C" 1 0 1)"

echo
echo "what the two superseded versions answer on the same logs:"
printf '  %-46s %s\n' "presence: split entered, never returned" "$(presence "$E
$C
$E")"
printf '  %-46s %s\n' "presence: completion after a no-handle" "$(presence "$E
$C
$E
$N
$E
$C")"
printf '  %-46s %s\n' "paired:   split destroyed nothing" "$(paired "$E
$C")"
printf '  %-46s %s\n' "paired:   split had no drawable extent" "$(paired "$E
$C
$E
$D")"
printf '  %-46s %s\n' "paired:   split had no runtime handle" "$(paired "$E
$C
$E
$N")"
echo "  (each of those five is a wrong answer the shipped verdict gets right)"

echo
if [ "${fails}" -eq 0 ]; then
    echo "verdict-test: all assertions held"
else
    echo "verdict-test: ${fails} assertion(s) failed"
    exit 1
fi
