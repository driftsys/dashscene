#!/usr/bin/env bash
# Drives `unity-frame-cost.sh` end to end against a stub `adb`, to check its
# refusal of a `unity-cpu.md` with no numeric `cpu ms per presented frame`, the
# player arguments it launches with and records, its check of the player's
# readback of them, a run held at rest, and its report of a player that wrote
# no census. Needs no device, no SDK and no NDK.
#
# `unity-frame-table-test.sh` checks the parser's output, but the refusal is in
# the sweep script, which only a device reaches. The stub replays the committed
# fixture capture as the sweep's logcat, and answers every compositor dump with
# one fixture dump, so the script runs one sweep of three entries with no
# waiting (`sleep` is stubbed too).
#
#   control  the fixture dump, 163 presented frames: entry 1 has a numeric
#            `cpu ms per presented frame`, and the script must exit 0
#   zero     the same dump with `totalFrames = 0`: entry 1 keeps a numeric
#            `cpu %` but every `cpu ms per presented frame` is `—`, and the
#            script must exit 1 with the refusal. A numeric `cpu %` beside a
#            dash is what tells the cpu ms column apart from the cpu % column
#            The control run also launches once with no `-e`, and its
#            provenance says `(no arguments)`
#   args     `DS_UNITY_ARGS='-renderer canvas -pulse 0.5'` and the player's
#            matching readback: one launch carrying them as one quoted extra,
#            exit 0, and the arguments in every table's provenance
#   typed    `-pulse 0.50` read back as `every 0.5 s`: exit 0, because the
#            period is compared as a number
#   rest     `-pulse 0`, `pulse: off`, and no frame-cost or thread-cost line
#            at all: exit 0 with a numeric `unity-cpu.md`, `extents none` in
#            its provenance, and neither of the other two tables, an earlier
#            run's copies included
#   unread, wrongrenderer, wrongpulse, nearpulse, restasperiod, periodasoff,
#   pulsealone
#            the readback absent, naming another renderer, another period
#            (2.5 s or 0.6 s for 0.5), a period for `-pulse 0`, `off` for
#            `-pulse 0.5`, or `-pulse` passed without `-renderer`, which
#            nothing reads back: each exits 1 and names the readback
#   nolines  `-pulse 0.5` and no frame-cost line: exit 1, since only a run
#            held at rest is exempt
#   resttwo, restsweeps
#            held at rest with frame-cost lines at two extents, in one sweep
#            or across two: exit 1, since rest exempts no extent at all, not
#            two of them
#   quote    a single quote in `DS_UNITY_ARGS`: exit 1 and no launch at all
#   failed   no census line and a `[dashscene]` error in the log: exit 1
#            quoting the error after ONE census read, not asking whether the
#            player is installed
#   missing  no census line and only a `W` `[dashscene]` line: exit 1 asking
#            whether the player is installed, quoting nothing
#
#     ./measure/android/unity-frame-cost-test.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
fixtures="${here}/fixtures"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
failures=0
fail() {
    echo "unity-frame-cost-test: FAIL: $*" >&2
    failures=$((failures + 1))
}

mkdir -p "${work}/bin"
# The capture `logcat -v epoch -d` returns: the fixture, plus the graphics line
# the script requires once per run, which the fixture does not carry.
{
    cat "${fixtures}/unity-frame-cost.log"
    printf '         1705482048.000 23903 23922 I Unity   : [showcase] graphics: Vulkan, rung RawBuffer\n'
} > "${work}/capture.log"

cat > "${work}/bin/adb" <<'STUB'
#!/usr/bin/env bash
case "$*" in
    devices) printf 'List of devices attached\nstub-1\tdevice\n' ;;
    "shell getprop ro.product.model") echo "Stub" ;;
    "shell date -u +%Y%m%dT%H%M%SZ") echo "20260101T000000Z" ;;
    "shell cmd package resolve-activity --brief "*) echo "com.driftsys.dashscene.showcase/.Main" ;;
    "shell am start "*) echo "$*" >> "${STUB_STARTS}" ;;
    # Sweep B's capture, where a case gives one: the second launch is sweep B.
    "logcat -v epoch -d") if [ -n "${STUB_CAPTURE_B}" ] && [ "$(wc -l < "${STUB_STARTS}")" -ge 2 ]; then
        cat "${STUB_CAPTURE_B}"; else cat "${STUB_CAPTURE}"; fi ;;
    "shell pidof "*) echo "23903" ;;
    "shell getconf CLK_TCK") echo "100" ;;
    "logcat -d") echo read >> "${STUB_READS}"
        if [ -n "${STUB_NO_CENSUS}" ]; then cat "${STUB_CAPTURE}"; else echo "I Unity   : [showcase] entries: 3"; fi ;;
    "shell dumpsys SurfaceFlinger --timestats -dump --maxlayers 8") cat "${STUB_DUMP}" ;;
    *) ;;
esac
exit 0
STUB
printf '#!/usr/bin/env bash\nexit 0\n' > "${work}/bin/sleep"
chmod +x "${work}/bin/adb" "${work}/bin/sleep"

cp "${fixtures}/unity-frame-cost-entry-1-sf-timestats.txt" "${work}/control-dump.txt"
sed 's/^totalFrames = 163$/totalFrames = 0/' "${work}/control-dump.txt" > "${work}/zero-dump.txt"

# run <case> [player arguments] -> exit status; output in ${work}/<case>.out
# and .err, every `am start` the script sent in ${work}/<case>.starts, and one
# line per census read in ${work}/<case>.reads. The logcat capture is
# ${work}/<case>-capture.log (and sweep B's <case>-capture-b.log) and the
# compositor dump ${work}/<case>-dump.txt where they exist, and the shared ones
# where not. SWEEPS sets DS_SWEEPS (1 by default) and CENSUS_WAIT
# DS_CENSUS_WAIT.
# `DS_UNITY_ARGS` is always set from the second argument, so a value in the
# caller's own environment never reaches a case.
run() {
    local capture="${work}/capture.log" dump="${work}/control-dump.txt" capture_b=""
    [ -e "${work}/$1-capture.log" ] && capture="${work}/$1-capture.log"
    [ -e "${work}/$1-capture-b.log" ] && capture_b="${work}/$1-capture-b.log"
    [ -e "${work}/$1-dump.txt" ] && dump="${work}/$1-dump.txt"
    (
        cd "${work}" &&
            PATH="${work}/bin:${PATH}" ADB="${work}/bin/adb" \
            STUB_CAPTURE="${capture}" STUB_CAPTURE_B="${capture_b}" STUB_DUMP="${dump}" \
            STUB_STARTS="${work}/$1.starts" STUB_READS="${work}/$1.reads" \
            STUB_NO_CENSUS="${STUB_NO_CENSUS:-}" DS_CENSUS_WAIT="${CENSUS_WAIT:-60}" \
            DS_SWEEPS="${SWEEPS:-1}" DS_DWELL=0 DS_UNITY_ARGS="${2:-}" \
            "${here}/unity-frame-cost.sh" com.driftsys.dashscene.showcase "${work}/out-$1" \
            > "${work}/$1.out" 2> "${work}/$1.err"
    )
}

run control
status=$?
if [ "${status}" -ne 0 ]; then
    fail "the control run exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/control.err" >&2
elif ! grep -q '^| A | 1 | .* | [0-9][0-9.]* | [0-9]* |$' "${work}/out-control/unity-cpu.md"; then
    fail "the control run's unity-cpu.md has no entry 1 row with a numeric"
    fail "  cpu ms per presented frame, so the zero case below proves nothing."
fi

run zero
status=$?
if [ "${status}" -ne 1 ]; then
    fail "the zero-presented run exited ${status}, not 1: a unity-cpu.md whose"
    fail "  every cpu ms per presented frame is — was accepted."
elif ! grep -qF "none has a numeric cpu ms per presented frame" "${work}/zero.err"; then
    fail "the zero-presented run exited 1, but not with the refusal. Its stderr:"
    sed 's/^/    /' "${work}/zero.err" >&2
fi
if ! grep -q '^| A | 1 | .* | [0-9][0-9]* | — | [0-9]* |$' "${work}/out-zero/unity-cpu.md" 2>/dev/null; then
    fail "the zero-presented run's entry 1 row does not have a numeric cpu % beside"
    fail "  a — cpu ms, so a guard reading the cpu % column would also pass."
fi

# **The player's own arguments reach the launch as ONE extra.** `adb shell`
# joins its argv with spaces and the device's shell splits the line again, so
# a value with a space in it is delivered only if it carries its own quotes —
# `-e unity -renderer canvas` would hand `am` a bare `canvas` as a package
# filter, and the player would see `-renderer` alone and draw the painter.
launch="shell am start -n com.driftsys.dashscene.showcase/.Main"
if [ ! -e "${work}/control.starts" ] || [ "$(wc -l < "${work}/control.starts")" -ne 1 ] \
    || ! grep -qxF -- "${launch}" "${work}/control.starts"; then
    fail "the control run did not launch exactly once with no player arguments:"
    sed 's/^/    /' "${work}/control.starts" >&2 2>/dev/null
fi
for table in unity-frames unity-threads unity-cpu; do
    if ! grep -qxF -- "    player    (no arguments)" "${work}/out-control/${table}.md"; then
        fail "the control run's ${table}.md does not say it had no player arguments."
    fi
done

# refused <case> <player arguments> <text>: the case must exit 1 with <text>
# in its stderr.
refused() {
    run "$1" "$2"
    local status=$?
    if [ "${status}" -ne 1 ]; then
        fail "the $1 run exited ${status}, not 1."
    elif ! grep -qF -- "$3" "${work}/$1.err"; then
        fail "the $1 run exited 1, but not saying \"$3\". Its stderr:"
        sed 's/^/    /' "${work}/$1.err" >&2
    fi
}

# The player's readback, as `DashsceneCanvasBaseline.Awake` writes it at launch.
readback() {
    printf '         1705482048.100 23903 23922 I Unity   : [showcase] pulse: %s\n' "$1"
    printf '         1705482048.200 23903 23922 I Unity   : [showcase] renderer: %s\n' "$2"
}
{ cat "${work}/capture.log"; readback "every 0.5 s" canvas; } > "${work}/args-capture.log"
run args '-renderer canvas -pulse 0.5'
status=$?
if [ "${status}" -ne 0 ]; then
    fail "the run with DS_UNITY_ARGS exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/args.err" >&2
else
    if ! grep -qxF -- "${launch} -e unity '-renderer canvas -pulse 0.5'" "${work}/args.starts"; then
        fail "DS_UNITY_ARGS did not reach the launch as one quoted extra. It sent:"
        sed 's/^/    /' "${work}/args.starts" >&2
    fi
    for table in unity-frames unity-threads unity-cpu; do
        if ! grep -qxF -- "    player    -renderer canvas -pulse 0.5" "${work}/out-args/${table}.md"; then
            fail "${table}.md does not record the player arguments in its provenance."
        fi
    done
fi

{ cat "${work}/capture.log"; readback "every 0.5 s" canvas; } > "${work}/typed-capture.log"
run typed '-pulse 0.50 -renderer canvas'
status=$?
if [ "${status}" -ne 0 ]; then
    fail "-pulse 0.50 read back as every 0.5 s exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/typed.err" >&2
fi

# **At rest, no frame-cost or thread-cost line closes**, because the player
# skips drawing a settled scene, and `unity-cpu.md` is the one table left. An
# earlier run's other two tables are planted first, to be removed.
no_samples() {
    grep -vF -e '[showcase] frame cost' -e '[showcase] thread cost' "${work}/capture.log"
}
{ no_samples; readback off canvas; } > "${work}/rest-capture.log"
mkdir -p "${work}/out-rest"
touch "${work}/out-rest/unity-frames.md" "${work}/out-rest/unity-threads.md"
run rest '-renderer canvas -pulse 0'
status=$?
if [ "${status}" -ne 0 ]; then
    fail "the at-rest run exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/rest.err" >&2
elif ! grep -q '^| A | 1 | .* | [0-9][0-9.]* | [0-9]* |$' "${work}/out-rest/unity-cpu.md"; then
    fail "the at-rest run's unity-cpu.md has no entry 1 row with a numeric"
    fail "  cpu ms per presented frame."
elif [ -e "${work}/out-rest/unity-frames.md" ] || [ -e "${work}/out-rest/unity-threads.md" ]; then
    fail "the at-rest run left a frame-cost or thread-cost table in its directory."
elif ! grep -qxF -- "    extents   none (as reported)" "${work}/out-rest/unity-cpu.md"; then
    fail "the at-rest run's unity-cpu.md does not say no extent was reported."
fi

# **Rest exempts a run from having samples, and from nothing else.**
{ no_samples; readback "every 0.5 s" canvas; } > "${work}/nolines-capture.log"
refused nolines '-renderer canvas -pulse 0.5' "reported no frame cost at all"
{
    cat "${work}/capture.log"
    sed 's/ at 1080x2340 over / at 2340x1080 over /' "${work}/capture.log" | grep -F '[showcase] frame cost'
    readback off canvas
} > "${work}/resttwo-capture.log"
refused resttwo '-renderer canvas -pulse 0' "drifted across extents"
{ cat "${work}/capture.log"; readback off canvas; } > "${work}/restsweeps-capture.log"
sed 's/ at 1080x2340 over / at 2340x1080 over /' "${work}/restsweeps-capture.log" \
    > "${work}/restsweeps-capture-b.log"
SWEEPS=2 refused restsweeps '-renderer canvas -pulse 0' "did not agree on an extent"

# **The readback is checked, not assumed.** Absent, it is a player that never
# read the arguments; naming another value, it is one that read them wrongly.
cp "${work}/capture.log" "${work}/unread-capture.log"
{ cat "${work}/capture.log"; readback "every 0.5 s" painter; } > "${work}/wrongrenderer-capture.log"
{ cat "${work}/capture.log"; readback "every 2.5 s" canvas; } > "${work}/wrongpulse-capture.log"
{ cat "${work}/capture.log"; readback "every 0.6 s" canvas; } > "${work}/nearpulse-capture.log"
{ cat "${work}/capture.log"; readback off canvas; } > "${work}/periodasoff-capture.log"
for case in unread wrongrenderer wrongpulse nearpulse periodasoff; do
    refused "${case}" '-renderer canvas -pulse 0.5' "readback"
done
{ no_samples; readback "every 0.5 s" canvas; } > "${work}/restasperiod-capture.log"
refused restasperiod '-renderer canvas -pulse 0' "readback"
cp "${work}/capture.log" "${work}/pulsealone-capture.log"
refused pulsealone '-pulse 0.5' "readback"

run quote "-renderer 'canvas"
status=$?
if [ "${status}" -ne 1 ] || [ -e "${work}/quote.starts" ]; then
    fail "a single quote in DS_UNITY_ARGS exited ${status} and launched the player."
fi

{
    cat "${work}/capture.log"
    printf '         1705482048.100 23903 23922 E Unity   : [dashscene] -pulse takes a period in seconds\n'
} > "${work}/failed-capture.log"
# A short census wait, so a script that ignored the error line would spin
# through several reads rather than stopping at the first.
STUB_NO_CENSUS=1 CENSUS_WAIT=3 run failed '-renderer canvas -pulse x'
status=$?
if [ "${status}" -ne 1 ] || ! grep -qF -- "[dashscene] -pulse takes a period" "${work}/failed.err" \
    || grep -qF "is it installed" "${work}/failed.err"; then
    fail "a player that refused its arguments exited ${status} without quoting"
    fail "  its error line. Its stderr:"
    sed 's/^/    /' "${work}/failed.err" >&2
elif [ "$(wc -l < "${work}/failed.reads")" -ne 1 ]; then
    fail "the census wait read the log $(wc -l < "${work}/failed.reads") time(s) after an"
    fail "  error line, not once."
fi

# **Only an error ends the wait.** A `W` line carrying the same prefix is the
# player warning and carrying on, so it neither stops the wait nor is quoted.
{
    cat "${work}/capture.log"
    printf '         1705482048.100 23903 23922 W Unity   : [dashscene] a warning\n'
} > "${work}/missing-capture.log"
STUB_NO_CENSUS=1 CENSUS_WAIT=1 refused missing '' "is it installed?"
if grep -qF -- "a warning" "${work}/missing.err"; then
    fail "the missing run quoted a warning line as the player's error."
fi

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-cost-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-cost-test: the script accepts a table with a cpu ms figure and refuses one without, checks the player's readback of its arguments, and holds a run at rest to unity-cpu.md alone"
