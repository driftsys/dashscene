#!/usr/bin/env bash
# Drives `attach-timing.sh` end to end against a stub `adb` and a stub `just`.
# Needs no device, no SDK and no NDK.
#
# **`attach-outcome-test.sh` covers the two decisions and not the wiring between
# them.** It calls `ds_attach_outcome` and `ds_capture_state` directly with
# synthetic arguments, so the `case` in `attach-timing.sh` that maps a capture
# state onto `readable`, onto one of three warnings, and onto the `*)` refusal
# had never executed — nor had the follower, its trap, the liveness read or the
# suppression of the interval columns. Every one of those is reachable only with
# a device, which is the one place this apparatus is supposed to be already
# proven. A reviewer of PR #1300 pointed out that dropping `readable="no"` from
# any arm left all 19 cases green, which is what this file exists to stop.
#
# The stubs are the whole trick: `adb` and `just` are resolved from `PATH` and
# by the `ADB` variable, so a directory prepended to `PATH` supplies both.
#
#     ./measure/android/attach-timing-test.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
failed=0
total=0
work=$(mktemp -d)
trap 'rm -rf "${work}"' EXIT

# A `just` that answers `_apk-demo` without building anything.
mkdir -p "${work}/bin"
cat > "${work}/bin/just" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB
chmod +x "${work}/bin/just"

# An `adb` whose behaviour is chosen by DS_STUB_CASE. `logcat -T 1 -v epoch` is
# the follower: it writes the preamble, then whatever the case wants, then
# either keeps running or exits — which is what `capture-died` needs.
cat > "${work}/bin/adb" <<'STUB'
#!/usr/bin/env bash
case "$1 ${2:-}" in
  "devices "*)
      # `disappears` is present until the follower has started and absent after,
      # because `attach-timing.sh` refuses to begin with no device attached — so
      # a device that was never there cannot reach the state being tested.
      if [ "${DS_STUB_DEVICE:-yes}" = "disappears" ] \
          && [ -e "${DS_STUB_MARK}" ]; then
          printf 'List of devices attached\n'
      else
          printf 'List of devices attached\nemulator-5554\tdevice\n'
      fi
      exit 0 ;;
esac
case "$1" in
  logcat)
      for a in "$@"; do [ "$a" = "-c" ] && exit 0; done
      : > "${DS_STUB_MARK}"
      printf -- '--------- beginning of main\n'
      case "${DS_STUB_CASE}" in
        drew)
            printf '100.0 1 2 I dashscene: attaching a 8x8 surface\n'
            printf '101.5 1 2 I dashscene: attached a 8x8 surface\n'
            printf '101.7 1 2 I dashscene: first frame\n' ;;
        wedge)
            printf '100.0 1 2 I dashscene: attaching a 8x8 surface\n' ;;
        preamble-only) : ;;
      esac
      # `capture-died` is the follower exiting while the device stays; every
      # other case keeps it alive for the whole wait.
      [ "${DS_STUB_FOLLOWER:-alive}" = "alive" ] && sleep 120
      exit 0 ;;
  shell)
      shift
      case "$*" in
        "am start"*)
            if [ "${DS_STUB_AM:-ok}" = "refuse" ]; then
                # One of the three shapes `ds_am_start` returns 1 on, which is
                # what ends `attach-timing.sh` under `set -e` with the follower
                # already running.
                printf 'Error: Activity not started, unable to resolve Intent\n'
                exit 0
            fi
            printf 'Status: ok\nTotalTime: 1234\n'; exit 0 ;;
        *) exit 0 ;;
      esac ;;
  install | uninstall) exit 0 ;;
esac
exit 0
STUB
chmod +x "${work}/bin/adb"
export PATH="${work}/bin:${PATH}"
export DS_STUB_MARK="${work}/mark"

# run <name> <expected outcome substring> <expected acquire cell>
run() {
    local name expect_outcome expect_acquire out report row
    name="$1"; expect_outcome="$2"; expect_acquire="$3"
    total=$((total + 1))
    out="${work}/out-${total}"
    rm -f "${work}/mark"
    ADB="${work}/bin/adb" DS_STUB_MARK="${work}/mark" DS_ATTACH_TIMEOUT=5 \
        "${here}/attach-timing.sh" "${out}" release >/dev/null 2>&1
    report="${out}/attach.md"
    row=$(grep -E '^\| release \|' "${report}" 2>/dev/null || true)
    if [ -z "${row}" ]; then
        printf '  FAIL %-46s no row written\n' "${name}"
        failed=$((failed + 1))
        return
    fi
    if ! printf '%s' "${row}" | grep -qF "${expect_outcome}"; then
        printf '  FAIL %-46s wanted outcome %s, row: %s\n' \
            "${name}" "${expect_outcome}" "${row}"
        failed=$((failed + 1))
        return
    fi
    if [ "$(printf '%s' "${row}" | awk -F'|' '{gsub(/ /,"",$4); print $4}')" \
        != "${expect_acquire}" ]; then
        printf '  FAIL %-46s wanted acquire %s, row: %s\n' \
            "${name}" "${expect_acquire}" "${row}"
        failed=$((failed + 1))
        return
    fi
    printf '  ok   %-46s %s\n' "${name}" "${expect_outcome}"
}

# The ordinary path, and the only one a working device produces.
DS_STUB_CASE=drew run "a run that drew" "drew" "1.50"

# The wedge: `attaching` and nothing after it, device present, follower alive.
DS_STUB_CASE=wedge run "the wedge" "NO COMPLETION OBSERVED" "—"

# **The three unreadable states, which are the wiring this file exists for.**
# Each must suppress both interval columns as well as flipping the outcome.
DS_STUB_CASE=preamble-only run "a capture holding only the preamble" \
    "CAPTURE UNREADABLE" "—"
DS_STUB_CASE=wedge DS_STUB_DEVICE=disappears run "the device went away" \
    "CAPTURE UNREADABLE" "—"
DS_STUB_CASE=wedge DS_STUB_FOLLOWER=dies run "the follower exited early" \
    "CAPTURE UNREADABLE" "—"

# **The early exit, which is the only path the trap is for.** `ds_am_start`
# returns 1 on a refused launch and `set -euo pipefail` ends the script there —
# after the follower is spawned and before the `kill` at the end of the loop. So
# a run that completes normally proves nothing about the trap: this case is the
# one that does, and without the trap it leaves an `adb logcat` running.
rm -f "${work}/mark"
DS_STUB_CASE=wedge DS_STUB_AM=refuse ADB="${work}/bin/adb" DS_ATTACH_TIMEOUT=5 \
    "${here}/attach-timing.sh" "${work}/out-refused" release >/dev/null 2>&1
total=$((total + 1))
sleep 1
if pgrep -f "${work}/bin/adb logcat" >/dev/null 2>&1; then
    printf '  FAIL %-46s a follower outlived the aborted run\n' \
        "no orphan after an aborted launch"
    pkill -f "${work}/bin/adb logcat" 2>/dev/null || true
    failed=$((failed + 1))
else
    printf '  ok   %-46s none left running\n' "no orphan after an aborted launch"
fi

echo
if [ "${failed}" -gt 0 ]; then
    echo "attach-timing-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "attach-timing-test: all ${total} cases held"
