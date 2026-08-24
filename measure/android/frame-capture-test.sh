#!/usr/bin/env bash
# Drives `frame-capture.sh` end to end against a stub `adb`. Needs no device, no
# SDK and no NDK.
#
# **The same argument `attach-timing-test.sh` makes, one script along.** That
# file exists because `attach-outcome-test.sh` calls `ds_attach_outcome` and
# `ds_capture_state` with synthetic arguments and so never executes the `case`
# that maps a capture state onto a verdict — and a reviewer of PR #1300 showed
# that dropping an arm from it left every one of those cases green. Issue #1304
# put the same decision into this script, over the same three states, none of
# which a working device produces.
#
# **What the stub is built to discriminate.** Each line is a mutation that was
# green before the case beside it existed; every one of them was run.
#
#   the follower opened BEFORE each launch  the stub counts launches and
#                                           followers, so the Nth follower must
#                                           open with N-1 launches behind it.
#                                           Counting is what makes it work per
#                                           SCENE: a single "has anything
#                                           launched" marker is set by scene 1
#                                           and makes every later scene look
#                                           late
#   `-T 1` on the follower                  without it the stub replays a
#                                           PREVIOUS launch's sample line, so a
#                                           scene that drew nothing reads as one
#                                           that drew
#   the capture leaving `frames-*.log`      the degraded scene's capture HOLDS
#                                           samples, so a table built over the
#                                           wrong glob gains rows
#   the early break on a dead follower      measured by the clock: the run must
#                                           finish well inside its own timeout
#   `unreadable` as a count, not a flag     a two-scene run with both captures
#                                           unreadable names both
#   the sample COUNT, not a boolean         the control asks for two samples
#                                           against two lines
#   the per-scene loop, twice               one scene readable and one not
#
# **Some behaviour here is reached by no case**, and the honest form of that is
# a rule rather than a list: anything whose effect is invisible in the output —
# the post-stop re-read of the sample count, the parent-side truncation, the
# `am force-stop` on the degrade path — and the `*)` unrecognised-state arm,
# which no stub can reach because only `ds_capture_state` decides that value.
# A closed count is not kept here: two review passes each found the previous
# count already stale, which is what a census in a comment does.
#
#     ./measure/android/frame-capture-test.sh

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
failed=0
total=0
work=$(mktemp -d)
trap 'rm -rf "${work}"' EXIT

# `frame-capture.sh` installs `target/android-demo/showcase.apk` by a relative
# path and refuses without it, so the run happens from a directory that has one.
mkdir -p "${work}/proj/target/android-demo" "${work}/bin"
: > "${work}/proj/target/android-demo/showcase.apk"

# The sample and CPU lines are the shapes `frame-table.py` parses;
# `frame-table-test.py` carries the same two, taken from a real capture. They
# are here so the control reaches a written table rather than
# `frame-table.py`'s no-sample exit.
cat > "${work}/bin/adb" <<'STUB'
#!/usr/bin/env bash
# **Every epoch is derived from the follower index**, so two scenes in one run
# never share a `(pid, epoch)`. `frame-table.py` de-duplicates on exactly that
# pair, so a fixture that reused the epochs made the second scene's samples
# vanish into the first's — and the case asserting that a parked capture
# contributes no rows then passed with the table's glob widened to take it.
scene_line() { printf '         %s.0  4321  4364 I dashscene: scene %s — 3 rects\n' "$1" "$2"; }
sample_line() {
    printf '         %s.0  4321  4364 I dashscene: %s over 240 frames — tick 0.26 ms, paint mean 1.20 p50 1.10, submit mean 18.80 p50 14.33 p95 34.73 max 369.01 ms (52.5 fps if unpaced) — 8 run(s), 97 glyph(s)\n' "$1" "$2"
}
cpu_line() {
    printf '         %s.0  9001  9001 I dashscene-cpu: 4321 (.dashscene.demo) S 365 365 0 0 -1 4194624 16457 0 1404 0 %s %s 0 0 10 -10 23 0 26405 16825294848 34984\n' "$1" "$2" "$3"
}
# **Stays alive without `exec`, and cleans up its own child on TERM.** `exec
# sleep` was tried and it blinded both suites' orphan predicates: after `exec`
# the process command line is `sleep N`, so `pgrep -f "<work>/bin/adb logcat"`
# can never match the process it is written to find. Keeping this shell in place
# keeps the command line — which carries the run's own mktemp directory and so
# cannot match another run — and the trap is what stops the `sleep` being
# reparented and outliving the suite.
stay_alive() {
    trap 'kill "${sleeper:-}" 2>/dev/null; exit 0' TERM
    sleep 30 &
    sleeper=$!
    wait "${sleeper}"
    exit 0
}
case "$1 ${2:-}" in
  "devices "*)
      # `disappears` answers "present" until the follower has started and
      # "absent" after, because `frame-capture.sh` refuses to begin with no
      # device — so a device that was never there cannot reach the state under
      # test.
      if [ "${DS_STUB_DEVICE:-yes}" = "disappears" ] && [ -e "${DS_STUB_MARK}" ]; then
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
      # Which follower this is, and which scene it belongs to.
      n=1
      [ -f "${DS_STUB_COUNT}" ] && n=$(( $(cat "${DS_STUB_COUNT}") + 1 ))
      printf '%s\n' "${n}" > "${DS_STUB_COUNT}"
      this_scene=$(printf '%s\n' ${DS_STUB_SCENES:-surfaces} | sed -n "${n}p")
      [ -n "${this_scene}" ] || this_scene=surfaces

      # **A follower opened AFTER its own scene's launch captures nothing.**
      # That is the defect issue #1304 is about, seen from the stub. Counted
      # rather than flagged: the Nth follower is in order when exactly N-1
      # launches have happened, and a single "something launched" marker would
      # make every scene after the first look late.
      launches=0
      [ -f "${DS_STUB_LAUNCHES}" ] && launches=$(cat "${DS_STUB_LAUNCHES}")
      if [ "${launches}" -ge "${n}" ]; then
          printf -- '--------- beginning of main\n'
          [ "${DS_STUB_DIE_AT:-}" = "${n}" ] && exit 0
          stay_alive
      fi

      # **Without `-T 1`, a bare `adb logcat` replays the ring.** The stale
      # sample below is a PREVIOUS launch's, which is what makes a scene that
      # drew nothing read as one that drew.
      has_t=no
      for a in "$@"; do [ "$a" = "-T" ] && has_t=yes; done
      printf -- '--------- beginning of main\n'
      base=$(( n * 1000 ))
      if [ "${has_t}" = "no" ]; then
          sample_line $(( base - 500 )) "${this_scene}"
      fi

      case "${DS_STUB_CASE}" in
        drew | two-scene | vanished)
            # **Three samples against a threshold of two.** Equal counts made
            # the threshold and the arrival count the same number in two roles,
            # and reporting `${SAMPLES}` where `${seen}` belongs then passed.
            scene_line $(( base + 0 )) "${this_scene}"
            cpu_line $(( base + 1 )) 10 5
            sample_line $(( base + 2 )) "${this_scene}"
            cpu_line $(( base + 3 )) 40 15
            sample_line $(( base + 4 )) "${this_scene}"
            cpu_line $(( base + 5 )) 70 25
            sample_line $(( base + 6 )) "${this_scene}"
            cpu_line $(( base + 7 )) 100 35 ;;
        no-sample)
            scene_line $(( base + 0 )) "${this_scene}" ;;
        preamble-only) : ;;
      esac
      # `DS_STUB_DIE_AT` is the follower exiting while the device stays, at a
      # chosen scene — which is what makes a degraded SECOND scene reachable.
      [ "${DS_STUB_DIE_AT:-}" = "${n}" ] && exit 0
      stay_alive ;;
  shell)
      shift
      case "$*" in
        "am start"*)
            if [ "${DS_STUB_AM:-ok}" = "refuse" ]; then
                # One of the three shapes `ds_am_start` returns 1 on, which is
                # what ends the script under `set -e` with the follower already
                # running. It counts the launch first: a refused launch is still
                # a launch that happened after the follower should have opened.
                l=0; [ -f "${DS_STUB_LAUNCHES}" ] && l=$(cat "${DS_STUB_LAUNCHES}")
                printf '%s\n' "$((l + 1))" > "${DS_STUB_LAUNCHES}"
                printf 'Error: Activity not started, unable to resolve Intent\n'
                exit 0
            fi
            l=0; [ -f "${DS_STUB_LAUNCHES}" ] && l=$(cat "${DS_STUB_LAUNCHES}")
            printf '%s\n' "$((l + 1))" > "${DS_STUB_LAUNCHES}"
            printf 'Status: ok\nTotalTime: 1234\n'; exit 0 ;;
        "pidof"*) printf '4321\n'; exit 0 ;;
        "getconf CLK_TCK"*) printf '100\n'; exit 0 ;;
        *) exit 0 ;;
      esac ;;
  install | uninstall) exit 0 ;;
esac
exit 0
STUB
chmod +x "${work}/bin/adb"
export PATH="${work}/bin:${PATH}"
export DS_STUB_MARK="${work}/mark"
export DS_STUB_LAUNCHES="${work}/launches"
export DS_STUB_COUNT="${work}/count"

# run <name> <expected exit> <expected stderr substrings...> — then RUN_OUT
# holds the output directory and RUN_ELAPSED the wall seconds it took.
#
# **Sets globals rather than echoing them, and is never called in a command
# substitution or a pipeline.** Both put the call in a subshell, where `total`
# and `failed` are incremented in a copy and thrown away — the first draft of
# this file lost the control case's counters exactly that way.
#
# `DS_SCENES` chooses the scene list and `DS_STUB_SCENES` tells the stub the
# same list, so each follower can name its own scene.
run() {
    local name expect_status out status text started
    name="$1"; expect_status="$2"; shift 2
    total=$((total + 1))
    rm -f "${work}/mark" "${work}/launches" "${work}/count"
    out="${work}/out-${total}"
    mkdir -p "${out}"
    started=${SECONDS}
    (
        cd "${work}/proj" || exit 99
        # shellcheck disable=SC2086
        ADB="${work}/bin/adb" DS_SAMPLES="${DS_SAMPLES:-2}" \
            DS_FRAME_TIMEOUT="${DS_FRAME_TIMEOUT:-3}" DS_POLL="${DS_POLL:-1}" \
            DS_CPU_INTERVAL=1 DS_STUB_SCENES="${DS_SCENES:-surfaces}" \
            "${here}/frame-capture.sh" "${out}" release ${DS_SCENES:-surfaces}
    ) > "${out}/stdout.txt" 2> "${out}/stderr.txt"
    status=$?
    RUN_OUT="${out}"
    RUN_ELAPSED=$((SECONDS - started))
    if [ "${status}" -ne "${expect_status}" ]; then
        printf '  FAIL %-46s wanted exit %s, got %s\n' \
            "${name}" "${expect_status}" "${status}"
        sed 's/^/         /' "${out}/stderr.txt" | tail -4
        failed=$((failed + 1))
        return 1
    fi
    for text in "$@"; do
        if ! grep -qF -- "${text}" "${out}/stderr.txt"; then
            printf '  FAIL %-46s wanted %s on stderr\n' "${name}" "${text}"
            sed 's/^/         /' "${out}/stderr.txt" | tail -4
            failed=$((failed + 1))
            return 1
        fi
    done
    printf '  ok   %-46s exit %s\n' "${name}" "${status}"
    return 0
}

# check <yes|no> <name> <detail> — a bare assertion the caller has evaluated.
check() {
    total=$((total + 1))
    if [ "$1" = "yes" ]; then
        printf '  ok   %-46s %s\n' "$2" "$3"
    else
        printf '  FAIL %-46s %s\n' "$2" "$3"
        failed=$((failed + 1))
    fi
}

# rows <table> <scene> — data rows for one scene.
#
# **Anchored on the scene's own cell, not on "a row that starts with a word".**
# The first version counted `^| [a-z]+ |`, which also matches the header row
# `| scene | # | pid | …` — so a two-sample control reported three. That is the
# whole-output substring trap `frame-table-test.py`'s helpers were written to
# avoid, hit again one file along.
rows() {
    awk -F'|' -v s="$2" '$2 ~ "^ *" s " *$" { n++ } END { print n + 0 }' \
        "$1" 2>/dev/null || true
}

# **The control**, and the only path a working device produces. Two samples
# against a threshold of two, so the count is a count.
RUN_OUT=""; RUN_ELAPSED=0
DS_STUB_CASE=drew run "a run that sampled" 0
ok="no"
[ -s "${RUN_OUT}/frames.md" ] \
    && [ "$(rows "${RUN_OUT}/frames.md" surfaces)" = "3" ] && ok="yes"
check "${ok}" "the control wrote three sample rows" \
    "rows=$(rows "${RUN_OUT}/frames.md" surfaces)"
# **The count the run REPORTS, as well as the rows the table holds.** They come
# from the same file and are computed twice, which is exactly how they can
# disagree; asserting only the table would leave `captured` unpinned.
#
# **Matched on the success line's own shape, not on "2 sample(s)".** The
# shortfall line reads `2 of 3 sample(s) in 240 s` and contains that substring,
# so the first version of this assertion passed under a mutation that turned the
# count into a boolean — `grep -c -m1`, which reports 1 and then reports
# `1 of 2 sample(s)`. That is the whole-output substring trap again.
ok="no"; grep -qF "surfaces: 3 sample(s) ->" "${RUN_OUT}/stdout.txt" && ok="yes"
check "${ok}" "the run reported the count it tabulated" "3 sample(s) ->"

# **The one verdict this script may give about the painter**, and the only
# failure a working device reproduces (issue #1158): the capture is readable,
# the scene line is there, and no sample arrived.
DS_STUB_CASE=no-sample run "readable, and the painter drew nothing" 1 \
    "painter never drew"
ok="no"; [ ! -f "${RUN_OUT}/frames.md" ] && ok="yes"
check "${ok}" "no table over a scene that drew nothing" "frames.md absent"

# **The three unreadable states.** Each asserts its own message AND the closing
# line, which is where the exit comes from: after issue #1304's degrade change
# no arm exits, so a case asserting only the arm's warning would pass with the
# arm deleted.
DS_STUB_CASE=preamble-only run "a capture holding only the preamble" 1 \
    "nothing but logcat's own preamble" "not a result about the painter"
ok="no"; [ ! -f "${RUN_OUT}/frames.md" ] && ok="yes"
check "${ok}" "no table over an empty capture" "frames.md absent"

# The device goes away with a full sample count already captured, so the poll
# breaks on success and the run would otherwise report a measurement. The
# capture state is asked before the sample count is believed.
DS_STUB_CASE=vanished DS_STUB_DEVICE=disappears run "the device went away" 1 \
    "adb no longer lists the device" "not a result about the painter"
ok="no"; [ ! -f "${RUN_OUT}/frames.md" ] && ok="yes"
check "${ok}" "no table over a departed device" "frames.md absent"

# The truncated shape: the follower exits with the scene line captured and no
# sample after it, which is what a dropped transport mid-wait leaves.
DS_STUB_CASE=no-sample DS_STUB_DIE_AT=1 run "the follower exited early" 1 \
    "exited before the wait ended" "not a result about the painter"
ok="no"; [ ! -f "${RUN_OUT}/frames.md" ] && ok="yes"
check "${ok}" "no table over a truncated capture" "frames.md absent"

# **The early break, measured by the clock**, because "it stops waiting" is not
# visible in any output. The follower dies at once and the timeout is ten times
# the poll, so without the break this run takes the whole timeout.
DS_FRAME_TIMEOUT=30 DS_POLL=3 DS_STUB_CASE=no-sample DS_STUB_DIE_AT=1 \
    run "a dead follower ends the wait" 1 "exited before the wait ended"
check "$([ "${RUN_ELAPSED}" -lt 15 ] && echo yes || echo no)" \
    "it did not wait out the timeout" "${RUN_ELAPSED} s of a 30 s bound"

# **One scene readable and the next not.** The degraded scene is dropped from
# the table and the run still writes one — the shape `attach-timing.sh` uses,
# and the reason this guard does not end the run: ending it would throw away
# every scene already captured. It is also the only case that arms and stops the
# follower twice.
#
# **The degraded scene's capture HOLDS samples.** With an empty one, "excluded
# from the table" and "contributes no rows" are the same thing, and widening the
# table's glob to `*.log` passed.
DS_SCENES="surfaces typography" DS_STUB_CASE=two-scene DS_STUB_DIE_AT=2 \
    run "a second scene whose capture died" 0 "1 of 2 scene(s) are absent"
ok="no"
[ -s "${RUN_OUT}/frames.md" ] \
    && [ "$(rows "${RUN_OUT}/frames.md" surfaces)" = "3" ] \
    && [ "$(rows "${RUN_OUT}/frames.md" typography)" = "0" ] \
    && [ -f "${RUN_OUT}/unreadable-typography.log" ] \
    && [ ! -f "${RUN_OUT}/frames-typography.log" ] && ok="yes"
check "${ok}" "the readable scene is tabulated alone" \
    "surfaces=$(rows "${RUN_OUT}/frames.md" surfaces) typography=$(rows "${RUN_OUT}/frames.md" typography)"
# **The summary line names the scenes it tabulated**, and this is the only run
# where that differs from the scenes asked for.
ok="no"; grep -qF "across 1 of 2 scene(s) ->" "${RUN_OUT}/stdout.txt" && ok="yes"
check "${ok}" "the summary counts tabulated scenes" "across 1 of 2 scene(s)"
# **Asserted positively, on the content.** The first version of this line
# grepped the capture for its own FILE NAME and inverted the result with `||`,
# so it reported success for a file with no samples and for no file at all. It
# is the sentinel that keeps the case above honest — with an empty parked
# capture, "excluded from the table" and "contributes no rows" are the same
# thing, and the widened-glob mutation survives.
parked=$(grep -cF ' over 240 frames' "${RUN_OUT}/unreadable-typography.log" 2>/dev/null || echo 0)
check "$([ "${parked:-0}" -ge 1 ] && echo yes || echo no)" \
    "the parked capture kept its samples" "${parked} sample line(s) in it"

# **One readable scene that drew nothing, beside one whose capture died.** This
# is the case the closing guard's condition is about: with `unreadable -gt 0`
# the run reported "a failed measurement, not a result about the painter" and
# suppressed the `-gpu host` remedy — which the readable scene did support.
DS_SCENES="surfaces typography" DS_STUB_CASE=no-sample DS_STUB_DIE_AT=2 \
    run "one readable scene drew nothing, one died" 1 \
    "1 of 2 scene(s) could not be read" "painter never drew"
ok="no"; [ ! -f "${RUN_OUT}/frames.md" ] && ok="yes"
check "${ok}" "no table over the mixed failure" "frames.md absent"

# **Every scene unreadable**, which is a different result from a painter that
# drew nothing — and it is what makes `unreadable` a count rather than a flag.
DS_SCENES="surfaces typography" DS_STUB_CASE=preamble-only \
    run "every scene unreadable" 1 "all 2 scene(s) produced a capture"
ok="no"; grep -qF "painter never drew" "${RUN_OUT}/stderr.txt" || ok="yes"
check "${ok}" "the painter is not named for it" "no painter diagnosis"

# **A degrade path whose `mv` cannot succeed.** Unguarded, `set -e` ended the
# script at the one point whose whole purpose is not ending it; guarded, it
# refuses the table rather than building one over a capture that stopped
# watching.
#
# **The target is an unwritable directory, not merely a directory.** A plain
# directory was tried first and `mv file dir` moves the file INTO it and
# succeeds, so the case passed with exit 0 — a failure mode that is not one.
rm -f "${work}/mark" "${work}/launches" "${work}/count"
mkdir -p "${work}/out-blocked" "${work}/out-blocked/unreadable-typography.log"
chmod a-w "${work}/out-blocked/unreadable-typography.log"
(
    cd "${work}/proj" || exit 99
    DS_STUB_CASE=two-scene DS_STUB_DIE_AT=2 ADB="${work}/bin/adb" \
        DS_SAMPLES=2 DS_FRAME_TIMEOUT=3 DS_POLL=1 DS_CPU_INTERVAL=1 \
        DS_STUB_SCENES="surfaces typography" \
        "${here}/frame-capture.sh" "${work}/out-blocked" release surfaces typography
) > "${work}/out-blocked/stdout.txt" 2> "${work}/out-blocked/stderr.txt"
blocked_status=$?
check "$([ "${blocked_status}" -ne 0 ] && echo yes || echo no)" \
    "a move it cannot make refuses the table" "exit ${blocked_status}"
# **The message is what distinguishes the guard from `set -e`.** An unguarded
# `mv` failure also exits non-zero and also writes no table, so the status alone
# cannot tell the two apart — verified by mutation, which passed on status.
check "$(grep -qF "Refusing to build a table" "${work}/out-blocked/stderr.txt" \
    && echo yes || echo no)" \
    "and says why rather than dying under set -e" "the guard's own message"
check "$([ ! -f "${work}/out-blocked/frames.md" ] && echo yes || echo no)" \
    "and writes no table" "frames.md absent"
chmod u+w "${work}/out-blocked/unreadable-typography.log"

# **The early exit, which is the only path the shared trap is for.**
# `ds_am_start` returns 1 on a refused launch and `set -euo pipefail` ends the
# script there — after `ds_logcat_follow` has spawned the follower and before
# `ds_logcat_stop` runs. A run that completes normally proves nothing about the
# trap; without it an `adb logcat` outlives the run and appends to a capture
# file the next run truncates underneath it.
#
# **The refusal itself is asserted too.** With only the orphan predicate, a
# change that let `ds_am_start` swallow the `Error:` line — the regression its
# own header exists to prevent — would leave this case green, and so would a
# follower that was never spawned at all: hence the marker check.
rm -f "${work}/mark" "${work}/launches" "${work}/count"
(
    cd "${work}/proj" || exit 99
    DS_STUB_CASE=no-sample DS_STUB_AM=refuse ADB="${work}/bin/adb" \
        DS_SAMPLES=1 DS_FRAME_TIMEOUT=3 DS_POLL=1 DS_STUB_SCENES=surfaces \
        "${here}/frame-capture.sh" "${work}/out-refused" release surfaces
) >/dev/null 2>&1
refused_status=$?
check "$([ "${refused_status}" -ne 0 ] && echo yes || echo no)" \
    "a refused launch ends the run" "exit ${refused_status}"
check "$([ -e "${work}/mark" ] && echo yes || echo no)" \
    "a follower had attached before it" "the stub logged its start"
# **Scoped to this run's own mktemp directory**, which is what makes the
# predicate a statement about this suite. It matched `sleep 30` for one round,
# and that failed against any unrelated `sleep 30` anywhere on the machine —
# including a second lane running this same file.
sleep 1
total=$((total + 1))
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
    echo "frame-capture-test: ${failed} of ${total} case(s) failed"
    exit 1
fi
echo "frame-capture-test: all ${total} cases held"
