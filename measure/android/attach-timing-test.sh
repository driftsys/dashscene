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
# any arm left every one of those cases green, which is what this file exists to
# stop. No number is quoted: that file has since grown more cases, and a count
# repeated here is a second copy that drifts.
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
      #
      # **Stays alive without `exec`, and cleans up its own child on TERM.**
      # `exec sleep 120` was tried and it turned the orphan case below fail-open:
      # after `exec` the process command line is `sleep 120`, so
      # `pgrep -f "<work>/bin/adb logcat"` can never match the process it is
      # written to find — deleting the EXIT trap from `ds_logcat_follow` then
      # left all six cases green with a real orphan alive. Keeping this shell
      # keeps the command line, and the trap is what stops the `sleep`
      # outliving the suite.
      if [ "${DS_STUB_FOLLOWER:-alive}" = "alive" ]; then
          trap 'kill "${sleeper:-}" 2>/dev/null; exit 0' TERM
          sleep 120 &
          sleeper=$!
          wait "${sleeper}"
          exit 0
      fi
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
  install | uninstall)
      # Recorded, because "how many times was this installed" is the whole
      # question for the later-launch row: a later launch that reinstalls is a
      # first launch wearing the other label.
      [ -n "${DS_STUB_INSTALLS:-}" ] && printf '%s\n' "$1" >> "${DS_STUB_INSTALLS}"
      exit 0 ;;
esac
exit 0
STUB
chmod +x "${work}/bin/adb"
export PATH="${work}/bin:${PATH}"
export DS_STUB_MARK="${work}/mark"

# run <name> <expected outcome substring> <expected acquire cell>
run() {
    local name expect_outcome expect_acquire out report row started
    name="$1"; expect_outcome="$2"; expect_acquire="$3"
    total=$((total + 1))
    out="${work}/out-${total}"
    rm -f "${work}/mark"
    started=${SECONDS}
    ADB="${work}/bin/adb" DS_STUB_MARK="${work}/mark" \
        DS_ATTACH_TIMEOUT="${DS_ATTACH_TIMEOUT:-5}" \
        "${here}/attach-timing.sh" "${out}" release >/dev/null 2>&1
    RUN_ELAPSED=$((SECONDS - started))
    report="${out}/attach.md"
    # **The first-after-install row specifically.** Every run now writes two
    # rows, one per launch condition, and a bare `^| release |` matches both —
    # which makes `row` two lines and every check below read a concatenation.
    row=$(grep -E '^\| release \| first-after-install \|' "${report}" 2>/dev/null || true)
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
    # Field 5, not 4: the `launch` column sits between profile and outcome.
    if [ "$(printf '%s' "${row}" | awk -F'|' '{gsub(/ /,"",$5); print $5}')" \
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

# **The early break on a dead follower, measured by the clock**, because "it
# stops waiting" is invisible in the report. The follower exits at once and the
# bound is ten times the poll cadence, so without the break this run takes the
# whole bound. `frame-capture.sh`'s copy of this break has the same case; this
# sibling had none, and deleting the line here left all six cases green.
RUN_ELAPSED=0
DS_ATTACH_TIMEOUT=50 DS_STUB_CASE=wedge DS_STUB_FOLLOWER=dies \
    run "a dead follower ends the wait" "CAPTURE UNREADABLE" "—"
total=$((total + 1))
if [ "${RUN_ELAPSED}" -lt 25 ]; then
    printf '  ok   %-46s %s s of a 50 s bound\n' \
        "it did not wait out the bound" "${RUN_ELAPSED}"
else
    printf '  FAIL %-46s %s s of a 50 s bound\n' \
        "it did not wait out the bound" "${RUN_ELAPSED}"
    failed=$((failed + 1))
fi

# **The early exit, which is the only path the trap is for.** `ds_am_start`
# returns 1 on a refused launch and `set -euo pipefail` ends the script there —
# after the follower is spawned and before `ds_logcat_stop` runs, which is where
# the `kill` lives since issue #1304 moved the mechanism into `lib.sh`. So
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

# ---------------------------------------------------------------------------
# The two launch conditions (issue #960)
# ---------------------------------------------------------------------------
#
# **The defect these exist for.** This script uninstalled and installed before
# every profile, unconditionally, so every figure it had ever produced was a
# first launch after install — and `docs/design/android-toolchain.md` explained
# a fifteen-fold spread between two of them as a first-launch effect, a
# condition that did not vary across the rows it was explaining. Separating the
# two needs a row taken WITHOUT the reinstall, which the apparatus could not
# take at all.
DS_STUB_CASE=drew DS_STUB_INSTALLS="${work}/installs" ADB="${work}/bin/adb" \
    DS_ATTACH_TIMEOUT=5 \
    "${here}/attach-timing.sh" "${work}/out-launch" release >/dev/null 2>&1
launch_report="${work}/out-launch/attach.md"

launch_case() {
    local name expected
    name="$1"
    expected="$2"
    total=$((total + 1))
    if grep -qF -- "${expected}" "${launch_report}" 2>/dev/null; then
        printf '  ok   %-46s %s\n' "${name}" "found"
    else
        printf '  FAIL %-46s missing: %s\n' "${name}" "${expected}"
        failed=$((failed + 1))
    fi
}

launch_case "the first launch after install is a row" "| release | first-after-install |"
launch_case "a later launch is a row of its own" "| release | later |"

# **The one assertion that cannot be satisfied by relabelling.** Two rows are
# easy to print; two rows over ONE install is what makes the second row a later
# launch. A `later` row taken after a reinstall is a first launch under another
# name, which is exactly the confusion this issue is about.
total=$((total + 1))
installs=$(grep -c '^install$' "${work}/installs" 2>/dev/null || echo 0)
if [ "${installs}" = "1" ]; then
    printf '  ok   %-46s one install for two rows\n' "the later launch does not reinstall"
else
    printf '  FAIL %-46s wanted 1 install, saw %s\n' \
        "the later launch does not reinstall" "${installs}"
    failed=$((failed + 1))
fi

# Each row needs its own capture, or the second reads the first's markers back.
total=$((total + 1))
if [ -f "${work}/out-launch/attach-release-first-after-install.log" ] \
    && [ -f "${work}/out-launch/attach-release-later.log" ]; then
    printf '  ok   %-46s two captures\n' "each launch condition captures separately"
else
    printf '  FAIL %-46s one or both logs absent\n' \
        "each launch condition captures separately"
    failed=$((failed + 1))
fi

echo
if [ "${failed}" -gt 0 ]; then
    echo "attach-timing-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "attach-timing-test: all ${total} cases held"
