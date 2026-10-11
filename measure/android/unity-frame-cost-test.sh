#!/usr/bin/env bash
# Drives `unity-frame-cost.sh` end to end against a stub `adb`, to check its
# refusal of a `unity-cpu.md` with no numeric `cpu ms per presented frame`.
# Needs no device, no SDK and no NDK.
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
#   args     `DS_UNITY_ARGS='-renderer canvas -pulse 0.5'` and the player's
#            matching readback: one launch carrying them as one quoted extra,
#            exit 0, and the arguments in every table's provenance
#   rest     `-pulse 0`, `pulse: off`, and no frame-cost or thread-cost line
#            at all: exit 0 with a numeric `unity-cpu.md` and neither of the
#            other two tables
#   unread, wrongrenderer, wrongpulse
#            the readback absent, naming another renderer, or another
#            period: each exits 1 and names the readback
#   quote    a single quote in `DS_UNITY_ARGS`: exit 1 and no launch at all
#   failed   no census line and a `[dashscene]` error in the log: exit 1
#            quoting the error, not asking whether the player is installed
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
    "shell pidof "*) echo "23903" ;;
    "shell getconf CLK_TCK") echo "100" ;;
    "logcat -d") if [ -n "${STUB_NO_CENSUS}" ]; then cat "${STUB_CAPTURE}"; else echo "I Unity   : [showcase] entries: 3"; fi ;;
    "logcat -v epoch -d") cat "${STUB_CAPTURE}" ;;
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
# and .err, and every `am start` the script sent in ${work}/<case>.starts. The
# logcat capture is ${work}/<case>-capture.log and the compositor dump
# ${work}/<case>-dump.txt where they exist, and the shared ones where not.
# `DS_UNITY_ARGS` is always set from the second argument, so a value in the
# caller's own environment never reaches a case.
run() {
    local capture="${work}/capture.log" dump="${work}/control-dump.txt"
    [ -e "${work}/$1-capture.log" ] && capture="${work}/$1-capture.log"
    [ -e "${work}/$1-dump.txt" ] && dump="${work}/$1-dump.txt"
    (
        cd "${work}" &&
            PATH="${work}/bin:${PATH}" ADB="${work}/bin/adb" \
            STUB_CAPTURE="${capture}" STUB_DUMP="${dump}" \
            STUB_STARTS="${work}/$1.starts" STUB_NO_CENSUS="${STUB_NO_CENSUS:-}" \
            DS_SWEEPS=1 DS_DWELL=0 DS_UNITY_ARGS="${2:-}" \
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

# **At rest, no frame-cost or thread-cost line closes**, because the player
# skips drawing a settled scene, and `unity-cpu.md` is the one table left.
{
    grep -vF -e '[showcase] frame cost' -e '[showcase] thread cost' "${work}/capture.log"
    readback off canvas
} > "${work}/rest-capture.log"
run rest '-renderer canvas -pulse 0'
status=$?
if [ "${status}" -ne 0 ]; then
    fail "the at-rest run exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/rest.err" >&2
elif ! grep -q '^| A | 1 | .* | [0-9][0-9.]* | [0-9]* |$' "${work}/out-rest/unity-cpu.md"; then
    fail "the at-rest run's unity-cpu.md has no entry 1 row with a numeric"
    fail "  cpu ms per presented frame."
elif [ -e "${work}/out-rest/unity-frames.md" ] || [ -e "${work}/out-rest/unity-threads.md" ]; then
    fail "the at-rest run wrote a frame-cost or thread-cost table from no sample."
fi

# **The readback is checked, not assumed.** Absent, it is a player that never
# read the arguments; naming another value, it is one that read them wrongly.
cp "${work}/capture.log" "${work}/unread-capture.log"
{ cat "${work}/capture.log"; readback "every 0.5 s" painter; } > "${work}/wrongrenderer-capture.log"
{ cat "${work}/capture.log"; readback "every 2.5 s" canvas; } > "${work}/wrongpulse-capture.log"
for case in unread wrongrenderer wrongpulse; do
    run "${case}" '-renderer canvas -pulse 0.5'
    status=$?
    if [ "${status}" -ne 1 ]; then
        fail "the ${case} run exited ${status}, not 1: a readback that does not"
        fail "  match DS_UNITY_ARGS was accepted."
    elif ! grep -qF "readback" "${work}/${case}.err"; then
        fail "the ${case} run exited 1, but not naming the readback. Its stderr:"
        sed 's/^/    /' "${work}/${case}.err" >&2
    fi
done

run quote "-renderer 'canvas"
status=$?
if [ "${status}" -ne 1 ] || [ -e "${work}/quote.starts" ]; then
    fail "a single quote in DS_UNITY_ARGS exited ${status} and launched the player."
fi

{
    cat "${work}/capture.log"
    printf '         1705482048.100 23903 23922 E Unity   : [dashscene] -pulse takes a period in seconds\n'
} > "${work}/failed-capture.log"
STUB_NO_CENSUS=1 run failed '-renderer canvas -pulse x'
status=$?
if [ "${status}" -ne 1 ] || ! grep -qF -- "[dashscene] -pulse takes a period" "${work}/failed.err" \
    || grep -qF "is it installed" "${work}/failed.err"; then
    fail "a player that refused its arguments exited ${status} without quoting"
    fail "  its error line. Its stderr:"
    sed 's/^/    /' "${work}/failed.err" >&2
fi

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-cost-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-cost-test: the script accepts a table with a cpu ms figure and refuses one without, and checks the player's readback of its arguments"
