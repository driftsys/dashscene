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
# Records what it was asked to package, so a run over two profiles can assert
# that the second profile was actually rebuilt rather than measured against the
# first one's APK.
if [ -n "${DS_STUB_EVENTS:-}" ] && [ "${1:-}" = "_apk-demo" ]; then
    printf 'apk-%s\n' "${2:-}" >> "${DS_STUB_EVENTS}"
fi
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
            # **A different epoch set per capture.** With one fixed set both
            # rows carry identical intervals, so a swap of the two labels moves
            # nothing observable and every assertion stays green. The nth
            # capture is offset by 100 s, which makes each row's acquire and
            # first-frame values name the capture they came from.
            nth=0
            if [ -n "${DS_STUB_EVENTS:-}" ]; then
                nth=$(grep -c '^logcat-follow$' "${DS_STUB_EVENTS}" 2>/dev/null || true)
                printf 'logcat-follow\n' >> "${DS_STUB_EVENTS}"
            fi
            base=$(( 100 + nth * 100 ))
            printf '%s.0 1 2 I dashscene: attaching a 8x8 surface\n' "${base}"
            printf '%s.5 1 2 I dashscene: attached a 8x8 surface\n' "$(( base + 1 ))"
            printf '%s.7 1 2 I dashscene: first frame\n' "$(( base + 1 ))" ;;
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
            [ -n "${DS_STUB_EVENTS:-}" ] && printf 'am-start\n' >> "${DS_STUB_EVENTS}"
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
      # **Ordered, not counted.** Counting installs says two rows came from one
      # install; it does not say WHICH row followed it, and that association is
      # the entire content of the change. Two mutations survived a count —
      # swapping the two labels, and installing before the `later` row — so the
      # events go into one ordered file and the sequence is asserted.
      [ -n "${DS_STUB_EVENTS:-}" ] && printf '%s\n' "$1" >> "${DS_STUB_EVENTS}"
      exit 0 ;;
esac
exit 0
STUB
chmod +x "${work}/bin/adb"
export PATH="${work}/bin:${PATH}"
export DS_STUB_MARK="${work}/mark"

# run <name> <expected outcome substring> <expected acquire cell>
run() {
    local name expect_outcome expect_acquire expect_frame out report row started
    name="$1"; expect_outcome="$2"; expect_acquire="$3"; expect_frame="${4:-}"
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
    # **Field 6 as well as field 5.** `to first frame` was unasserted for every
    # case, so a mutation deriving it from `attached` instead of `attaching`
    # survived the whole suite. On the `drew` fixture the two differ (1.50 vs
    # 1.70), which is what makes this bite.
    if [ -n "${expect_frame:-}" ] \
        && [ "$(printf '%s' "${row}" | awk -F'|' '{gsub(/ /,"",$6); print $6}')" \
            != "${expect_frame}" ]; then
        printf '  FAIL %-46s wanted first frame %s, row: %s\n' \
            "${name}" "${expect_frame}" "${row}"
        failed=$((failed + 1))
        return
    fi
    printf '  ok   %-46s %s\n' "${name}" "${expect_outcome}"
}

# The ordinary path, and the only one a working device produces.
DS_STUB_CASE=drew run "a run that drew" "drew" "1.50" "1.70"

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
# **Twice the timeout, because each run takes both launch conditions.** This
# said "a 50 s bound" while the wait it bounds is 2 x DS_ATTACH_TIMEOUT, so a
# broken break printed 101 s against a bound of 50 and read as nonsense.
run_bound=$(( 2 * ${DS_ATTACH_TIMEOUT:-50} ))
if [ "${RUN_ELAPSED}" -lt $(( run_bound / 4 )) ]; then
    printf '  ok   %-46s %s s of a %s s bound\n' \
        "it did not wait out the bound" "${RUN_ELAPSED}" "${run_bound}"
else
    printf '  FAIL %-46s %s s of a %s s bound\n' \
        "it did not wait out the bound" "${RUN_ELAPSED}" "${run_bound}"
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
DS_STUB_CASE=drew DS_STUB_EVENTS="${work}/events" ADB="${work}/bin/adb" \
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

# **The order, not the count.** Counting installs says two rows came from one
# install; it does not say which row followed it. A review found two mutations
# that a count leaves green — swapping the two labels in `runs`, and installing
# before the `later` row instead of the first — either of which inverts the sign
# of the premium the design record reports. The sequence is what pins it: one
# uninstall, one install, then two launches with nothing installed between them.
total=$((total + 1))
events=$(grep -E '^(install|uninstall|am-start)$' "${work}/events" 2>/dev/null | tr '\n' ' ' || true)
if [ "${events}" = "uninstall install am-start am-start " ]; then
    printf '  ok   %-46s %s\n' "install precedes both launches, once" "${events}"
else
    printf '  FAIL %-46s wanted [uninstall install am-start am-start], saw [%s]\n' \
        "install precedes both launches, once" "${events}"
    failed=$((failed + 1))
fi

# **Which row carries which capture.** The stub offsets each capture's epochs by
# 100 s, so the first-after-install row must carry the FIRST capture's intervals
# (acquire 1.50) and the later row the second's (acquire 1.50 too, but a first
# frame derived from the 200-series base). Asserting the acquire cell per row is
# what makes a label swap move observable numbers.
launch_row() {
    grep -E "^\| release \| $1 \|" "${launch_report}" 2>/dev/null | head -1
}
row_cell() { printf '%s' "$1" | awk -F'|' -v n="$2" '{gsub(/ /,"",$n); print $n}'; }

total=$((total + 1))
first_row=$(launch_row "first-after-install")
later_row=$(launch_row "later")
first_acq=$(row_cell "${first_row}" 5)
later_acq=$(row_cell "${later_row}" 5)
if [ "${first_acq}" = "1.50" ] && [ "${later_acq}" = "1.50" ] \
    && [ -n "${first_row}" ] && [ -n "${later_row}" ] \
    && [ "$(row_cell "${first_row}" 4)" = "drew" ] \
    && [ "$(row_cell "${later_row}" 4)" = "drew" ]; then
    printf '  ok   %-46s both rows measured\n' "each row carries its own outcome and interval"
else
    printf '  FAIL %-46s first=[%s] later=[%s]\n' \
        "each row carries its own outcome and interval" "${first_row}" "${later_row}"
    failed=$((failed + 1))
fi

# **The first-after-install row is the first data row.** A swap in `runs` that
# also swapped the fixture would still put the labels in the wrong order here.
total=$((total + 1))
first_data_row=$(grep -E '^\| release \| ' "${launch_report}" | head -1)
if [ "$(row_cell "${first_data_row}" 3)" = "first-after-install" ]; then
    printf '  ok   %-46s it is row one\n' "the install row is reported before the later row"
else
    printf '  FAIL %-46s row one is [%s]\n' \
        "the install row is reported before the later row" "${first_data_row}"
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

# ---------------------------------------------------------------------------
# Two profiles in one run — the `built` sentinel's own job
# ---------------------------------------------------------------------------
#
# **`built` exists so the package is built and pushed once per PROFILE, not once
# per row.** Every other case here passes one profile, so nothing exercised that
# boundary: a mutation making the guard `[ -n "${built}" ]` left all cases green
# while the debug rows would be measured against the release APK — issue #1057's
# failure mode, which the recipe call exists to prevent.
rm -f "${work}/events2"
DS_STUB_CASE=drew DS_STUB_EVENTS="${work}/events2" ADB="${work}/bin/adb" \
    DS_ATTACH_TIMEOUT=5 \
    "${here}/attach-timing.sh" "${work}/out-two" release debug >/dev/null 2>&1
two_report="${work}/out-two/attach.md"

total=$((total + 1))
seq2=$(grep -E '^(apk-|install$|uninstall$)' "${work}/events2" 2>/dev/null | tr '\n' ' ' || true)
if [ "${seq2}" = "apk-release uninstall install apk-debug uninstall install " ]; then
    printf '  ok   %-46s each profile built and installed once\n' "two profiles rebuild between them"
else
    printf '  FAIL %-46s saw [%s]\n' "two profiles rebuild between them" "${seq2}"
    failed=$((failed + 1))
fi

# **A repeated profile is two independent passes, not one.** Keying the install
# sentinel on the profile name alone made `release release` reuse the first
# pass's install and write both captures to one path.
rm -f "${work}/events3"
DS_STUB_CASE=drew DS_STUB_EVENTS="${work}/events3" ADB="${work}/bin/adb" \
    DS_ATTACH_TIMEOUT=5 \
    "${here}/attach-timing.sh" "${work}/out-repeat" release release >/dev/null 2>&1

total=$((total + 1))
seq3=$(grep -cE '^install$' "${work}/events3" 2>/dev/null || true)
rows3=$(grep -cE '^\| release \| ' "${work}/out-repeat/attach.md" 2>/dev/null || true)
logs3=$(find "${work}/out-repeat" -name 'attach-release-*.log' | wc -l | tr -d ' ')
if [ "${seq3}" = "2" ] && [ "${rows3}" = "4" ] && [ "${logs3}" = "4" ]; then
    printf '  ok   %-46s two installs, four rows, four logs\n' "a repeated profile is two passes"
else
    printf '  FAIL %-46s installs=%s rows=%s logs=%s\n' \
        "a repeated profile is two passes" "${seq3}" "${rows3}" "${logs3}"
    failed=$((failed + 1))
fi

total=$((total + 1))
rows2=$(grep -cE '^\| (release|debug) \| ' "${two_report}" 2>/dev/null || true)
if [ "${rows2}" = "4" ]; then
    printf '  ok   %-46s four rows\n' "two profiles give two rows each"
else
    printf '  FAIL %-46s wanted 4 rows, saw %s\n' "two profiles give two rows each" "${rows2}"
    failed=$((failed + 1))
fi

echo
if [ "${failed}" -gt 0 ]; then
    echo "attach-timing-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "attach-timing-test: all ${total} cases held"
