#!/usr/bin/env bash
# The verdict `just android-splitscreen` reaches from HarnessActivity's markers.
#
# **Extracted so it can be run without a device.** That recipe needs an attached
# emulator, so until this was a file the only check on the logic deciding PASS
# or FAIL was reading it — and reading it missed five distinct false-verdict
# paths across two review rounds (issue #1006 and the reviews of PR #1177).
# `verdict-test.sh` beside this file exercises it against synthetic logcat text
# and needs neither a device nor an SDK.
#
# Nothing here touches adb. It takes a log string and the counts observed before
# the split transition, and answers. That split is the whole reason it is
# testable.
#
# ## What the markers mean
#
# `HarnessActivity.surfaceDestroyed` logs `entering the handshake`
# unconditionally, then exactly one of three exits:
#
#     handshake complete, returning         blocked for the frame loop to stop
#                                           and the surface to be dropped —
#                                           D4's case, and the only good exit
#     no drawable extent was ever reported  nothing was started, so there was
#                                           nothing to hand back (issue #1094)
#     no runtime handle, nothing to hand    nativeSurfaceCreated could not get
#     back                                  the window or spawn the thread
#
# So `entering == complete + no-drawable + no-handle` holds once every callback
# has returned, and a shortfall is precisely the use-after-free window D4 names.
#
# ## Why the baselines
#
# The cold `--windowingMode 6` launch resizes the harness window itself, so a
# complete create/destroy cycle can already be in the log before the split
# transition happens. Counting absolutely would let the launch's own cycle
# satisfy the verdict, and the recipe would pass having never observed the
# transition it exists to measure. Every check below is therefore about the
# cycles that appeared *after* the baseline was taken.

ds_count() {
    printf '%s\n' "$1" | grep -c "$2" || true
}

# ds_tally <log> — sets ds_entering, ds_complete, ds_nohandle, ds_nodrawable.
ds_tally() {
    ds_entering=$(ds_count "$1" "entering the handshake")
    ds_complete=$(ds_count "$1" "handshake complete, returning")
    ds_nohandle=$(ds_count "$1" "no runtime handle, nothing to hand back")
    ds_nodrawable=$(ds_count "$1" "no drawable extent was ever reported")
}

# ds_balanced — true when every entry so far has reached one of its three exits.
ds_balanced() {
    [ "${ds_entering}" -eq $((ds_complete + ds_nohandle + ds_nodrawable)) ]
}

# ds_settled <base_entering> — true when the split has produced an entry and
# every entry has returned. This is the wait loop's break condition.
ds_settled() {
    [ "${ds_entering}" -gt "$1" ] && ds_balanced
}

# ds_verdict <base_entering> <base_complete> <base_nohandle>
#
# Echoes PASS or FAIL:<reason>. Call ds_tally first.
ds_verdict() {
    _be="$1"; _bc="$2"; _bn="$3"

    # Nothing was destroyed after the baseline, so whatever is in the log
    # predates putting a second app in the other half.
    if [ "${ds_entering}" -le "${_be}" ]; then
        echo "FAIL:split-destroyed-nothing"; return
    fi
    # An entry with no exit. The callback is still inside the handshake, which
    # is the case D4 exists to prevent.
    if ! ds_balanced; then
        echo "FAIL:entered-never-returned"; return
    fi
    # A new no-handle exit means nativeSurfaceCreated failed during the split.
    # Checked before the completion count, because a run with both should be
    # reported as the JNI failure rather than as a pass. It is baselined, so a
    # no-handle from before the split — which is not this recipe's business —
    # does not fail the run, which is what the unbaselined substring check in
    # `main` got wrong in the other direction (issue #1006 comment c).
    if [ "${ds_nohandle}" -gt "${_bn}" ]; then
        echo "FAIL:split-had-no-handle"; return
    fi
    # The split's own cycles all returned, but none ran a handshake — so D4's
    # case did not execute, whatever an earlier cycle did.
    if [ "${ds_complete}" -le "${_bc}" ]; then
        echo "FAIL:split-ran-no-handshake"; return
    fi
    echo "PASS"
}
