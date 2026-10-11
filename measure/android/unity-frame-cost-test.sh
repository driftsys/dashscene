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
    "logcat -d") echo "I Unity   : [showcase] entries: 3" ;;
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

# run <case> -> exit status; output in ${work}/<case>.out and .err, and every
# `am start` the script sent in ${work}/<case>.starts
run() {
    (
        cd "${work}" &&
            PATH="${work}/bin:${PATH}" ADB="${work}/bin/adb" \
            STUB_CAPTURE="${work}/capture.log" STUB_DUMP="${work}/$1-dump.txt" \
            STUB_STARTS="${work}/$1.starts" \
            DS_SWEEPS=1 DS_DWELL=0 \
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
if grep -q -- '-e unity' "${work}/control.starts" 2>/dev/null; then
    fail "the control run passed the player arguments it was never given:"
    sed 's/^/    /' "${work}/control.starts" >&2
fi
cp "${work}/control-dump.txt" "${work}/args-dump.txt"
DS_UNITY_ARGS='-renderer canvas -pulse 0' run args
status=$?
if [ "${status}" -ne 0 ]; then
    fail "the run with DS_UNITY_ARGS exited ${status}, not 0. Its stderr:"
    sed 's/^/    /' "${work}/args.err" >&2
elif ! grep -qF -- "-e unity '-renderer canvas -pulse 0'" "${work}/args.starts"; then
    fail "DS_UNITY_ARGS did not reach the launch as one quoted extra. It sent:"
    sed 's/^/    /' "${work}/args.starts" >&2
fi

if [ "${failures}" -ne 0 ]; then
    echo "unity-frame-cost-test: ${failures} check(s) failed" >&2
    exit 1
fi
echo "unity-frame-cost-test: the script accepts a table with a cpu ms figure and refuses one without"
