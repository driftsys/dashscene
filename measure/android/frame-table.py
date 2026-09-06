#!/usr/bin/env python3
"""Turn a captured logcat into the frame-cost table story #1229's bundle holds.

`demo-android/src/timing.rs` prints one line per 240 **drawn** frames and
nothing reads it. This does: it extracts every sample, attributes CPU to the
same interval each sample covers, and writes a table. That is the difference
between the frame-rate half of #842 being a five-minute run and being an
improvisation at the device.

## The input

`adb logcat -v epoch`, which is the only format this reads. Epoch is required
rather than preferred: the CPU attribution below joins two line kinds by time,
and `-v time` gives no year, so two captures either side of midnight cannot be
ordered. `measure/android/frame-capture.sh` passes it.

Six line kinds are read and everything else is ignored:

    <epoch> <pid> <tid> I dashscene: <scene> over <n> frames — tick ...
    <epoch> <pid> <tid> I Unity   : [showcase] frame cost — <entry> at WxH ...
    <epoch> <pid> <tid> I Unity   : [showcase] thread cost — <entry> at WxH ...
    <epoch> <pid> <tid> I dashscene: attached a WxH surface
    <epoch> <pid> <tid> I dashscene-cpu: <the /proc/<pid>/stat line>
    <epoch> <pid> <tid> I dashscene-window: entry <n> start|end pid=<pid>

The first is `Sample::line()`, the lean host's. The next two are the Unity
showcase player's, from `DashsceneFrameCost.Line()` and
`ThreadCostSample.Line()` — one parser reads all three because the CPU join
below is the same join for each, and a second script would be a second place for
it to be wrong. The fourth is the lean host's attach line, which is where its
rows get the extent a Unity row carries in its own line. The fifth is written by
the device-side sampler
`frame-capture.sh` and `unity-frame-cost.sh` start, through the `log` command,
**so that every kind carries one clock and one ordering**. Reading `/proc/<pid>/stat` into a host file
instead would need the device epoch mapped onto the host's, and `date +%s` on
the device is whole seconds — a ±1 s error on an interval of a few seconds.
Routing the readings through logcat removes the mapping rather than estimating
it. The sixth brackets one compositor window per entry per sweep (issue
#1457) — `unity-frame-cost.sh` logs it beside a `dumpsys SurfaceFlinger
--timestats -clear`/`-dump` pair, states the same pid the CPU line's stat text
does rather than a sweep letter, and `--table unity-cpu` joins the dump's own
`totalFrames` for the package's layer, passed with `--timestats`, to the
sampler readings this window brackets.

## What it does not do

It does not recompute a single number the instrument reported. `fps_if_unpaced`
is copied across verbatim, and the header says what that field is: **not the
frame rate**, because the loop is paced by vsync. A table that recomputed it
from the mean would be a second derivation of one fact, and one of the two would
eventually be wrong.

It also does not average the samples of a scene. The first sample of a scene
carries pipeline warm-up — 369 ms in a measured `max` against a 14 ms p50 — so a
mean over all of them describes no frame. Every sample is a row, numbered, and
the reader drops what they judge to be warm-up.

## Which table

`--table` selects the kind, because the four carry different columns and a
table that reported a Unity line under `paint`/`submit` headings would be
labelling a quantity with another instrument's word. `lean` is the default and
is what `frame-capture.sh` asks for; `unity-frames`, `unity-threads` and
`unity-cpu` are the three `unity-frame-cost.sh` writes from one set of
captures — the last needs `--timestats` as well, since its rows join the
captures to files `frame-table.py` does not read as logcat.

## Unreadable lines

A line that carries an instrument's marker and does not parse is **reported**,
under its own heading below the table, rather than silently dropped. The whole
reason each pattern is anchored on the end of the line is to reject a record the
logcat ring cut in half, and a rejection nothing states reads exactly like a run
that reported fewer samples.

It cannot catch every truncation: a line cut before its marker is complete is
not recognised as that kind at all, and nothing here can distinguish it from an
unrelated log record.

## Exit status

0 with a table, 1 when the capture holds no sample of the requested kind, 2 on
usage or a file that cannot be read. 1 and 2 are separate for the reason
`assert-drew.py` separates its own: 1 is a result about the run, and 2 is "ask
me again".
"""

import argparse
import os
import re
import sys
from collections import Counter

# `Sample::line()`, and the only place its shape is written down outside the
# Rust that produces it.
#
# **Anchored on the whole line rather than searched for.** A partial match would
# accept a line whose tail was truncated by the logcat ring and report the
# fields it did read, which is a row of real-looking numbers describing nothing.
#
# The separator is an em dash, which is what `format!` emits. It is matched
# literally: normalising dashes here would let a line this project does not
# produce parse as one it does.
#
# `-v epoch` right-pads the timestamp column, so a real line begins with
# whitespace. Leading `\s*` rather than a `strip()` at the call site, because the
# trailing anchor has to keep meaning "the line ends here".
#
# **Two shapes, both anchored, and neither optional in its own tail.** The
# instrument gained a paint/present split and glyph counts on 2026-08-17, so
# every capture taken before that reads `draw mean` and every one after reads
# `paint mean … present mean … — N run(s), M glyph(s)`. Both are accepted,
# because `run.sh` writes into every bundle's README that its tables can be
# re-derived from its captures — a promise that a regex replacement rather than a
# widening would have broken for every bundle already taken.
#
# The glyph suffix is **required** in the new shape rather than optional. It was
# optional for one revision, on two justifications that were both false: a
# pre-split capture is rejected by the `paint mean` prefix whatever the tail
# does, and a frame with no text still emits `— 0 run(s), 0 glyph(s)`. What
# optional actually bought was accepting a line the logcat ring had cut at
# `(52.5 fps if unpaced)`, which is the truncation the whole-line anchor exists
# to reject.
SAMPLE_SPLIT = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+I\s+dashscene:\s+"
    r"(?P<scene>\S+) over (?P<frames>\d+) frames — "
    r"tick (?P<tick>[\d.]+) ms, "
    r"paint mean (?P<paint>[\d.]+) p50 (?P<paint50>[\d.]+), "
    # **`submit` or `present`, because the label was renamed after captures
    # existed.** The split instrument printed `present mean` for one day before
    # that word was found to name a different quantity on the desktop host; the
    # captures archived under
    # `docs/archive/2026-08-17-v021-android-device-measurements/` carry the old
    # label, and the README there promises they can be re-derived. Accepting both
    # is what keeps that true — a rename is not a reason to orphan evidence.
    r"(?:submit|present) mean (?P<mean>[\d.]+) p50 (?P<p50>[\d.]+) p95 (?P<p95>[\d.]+) "
    r"max (?P<max>[\d.]+) ms "
    r"\((?P<fps>[\d.]+) fps if unpaced\)"
    r" — (?P<runs>\d+) run\(s\), (?P<quads>\d+) glyph\(s\)\s*$"
)

# The shape every capture before 2026-08-17 carries. `draw` spanned paint and
# present together, so it is read into the present column and the paint columns
# report `—`: reporting a zero there would claim a measurement that instrument
# never took.
SAMPLE_COMBINED = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+I\s+dashscene:\s+"
    r"(?P<scene>\S+) over (?P<frames>\d+) frames — "
    r"tick (?P<tick>[\d.]+) ms, "
    r"draw mean (?P<mean>[\d.]+) p50 (?P<p50>[\d.]+) p95 (?P<p95>[\d.]+) "
    r"max (?P<max>[\d.]+) ms "
    r"\((?P<fps>[\d.]+) fps if unpaced\)\s*$"
)

# The Unity showcase player's two lines, from `DashsceneFrameCost.Line()` and
# `ThreadCostSample.Line()`.
#
# **The tag is `Unity`, padded.** `Debug.Log` reaches logcat under Unity's own
# tag, and logcat pads a tag shorter than eight characters — `I Unity   :`. The
# lean host writes through `__android_log_print` with its own tag, which is why
# the two prefixes differ here rather than being shared.
#
# **The entry is not one token.** `scene surfaces` is what the player writes, so
# the lean pattern's `(?P<scene>\S+)` would match `scene` and then fail on
# ` surfaces at`. It is non-greedy up to ` at WxH over `, which is the first
# place the shape becomes unambiguous.
#
# **The extent is in the line itself**, unlike the lean host's, whose extent
# comes from a separate `attached a WxH surface` record. So a Unity row's extent
# is a property of the sample rather than of the interval around it, and the row
# says so exactly.
#
# Anchored at both ends for the reason every pattern here is: a line the ring cut
# must be rejected and reported, not half-read.
UNITY_PREFIX = r"^\s*(?P<epoch>\d+\.\d+)\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+I\s+Unity\s*:\s+"
UNITY_HEAD = (
    r"(?P<scene>.+?) at (?P<width>\d+)x(?P<height>\d+) over (?P<frames>\d+) frames — "
)

SAMPLE_UNITY_FRAME = re.compile(
    UNITY_PREFIX
    + r"\[showcase\] frame cost — "
    + UNITY_HEAD
    + r"tick (?P<tick>[\d.]+) ms, "
    r"draw mean (?P<mean>[\d.]+) p50 (?P<p50>[\d.]+) p95 (?P<p95>[\d.]+) "
    r"max (?P<max>[\d.]+) ms "
    r"\((?P<fps>[\d.]+) fps if unpaced\)\s*$"
)

SAMPLE_UNITY_THREAD = re.compile(
    UNITY_PREFIX
    + r"\[showcase\] thread cost — "
    + UNITY_HEAD
    + r"main mean (?P<main_mean>[\d.]+) p50 (?P<main_p50>[\d.]+) "
    r"p95 (?P<main_p95>[\d.]+) max (?P<main_max>[\d.]+) ms, "
    # **An em dash is a value here, and a legitimate one.** A counter this
    # player does not carry reports `—` rather than a zero, because a zero
    # Canvas-rebuild term reads as a Canvas that rebuilds nothing —
    # `ThreadCostSample.Line` states the rule. Rejecting the line would file a
    # correct reading under `Unreadable`; only the main-thread terms are
    # required to be numbers, because the instrument refuses to arm without
    # them.
    r"render mean (?P<render_mean>[\d.]+|—) p50 (?P<render_p50>[\d.]+|—) "
    r"p95 (?P<render_p95>[\d.]+|—) max (?P<render_max>[\d.]+|—) ms, "
    r"canvas (?P<canvas>[\d.]+|—) ms, gc (?P<gc>\d+|—) B/frame\s*$"
)

# What makes a line one of the three MARKED kinds at all, for the unreadable
# report. The attach and CPU records carry no marker here: neither is an
# instrument line, and a truncated one costs a reading rather than a row.
#
# **Deliberately looser than the pattern it guards**, and only just: it stops
# before the first field a truncation can eat, so a line that carries the marker
# and fails the full pattern is reported rather than dropped. A marker that went
# as far as the numbers would be satisfied by the same truncations the anchor
# exists to reject.
MARKERS = {
    "lean": re.compile(r"^\s*\d+\.\d+\s+\d+\s+\d+\s+I\s+dashscene:\s+\S+ over \d+ frames"),
    "unity-frames": re.compile(UNITY_PREFIX + r"\[showcase\] frame cost — "),
    "unity-threads": re.compile(UNITY_PREFIX + r"\[showcase\] thread cost — "),
}

# The extent the painter drew at, logged by `machine.rs` on every successful
# attach. Its pid column is the app's own, unlike the CPU sampler's line, so it
# joins onto a sample directly.
#
# **`attached`, never `attaching`.** The pair brackets the acquisition and the
# second one is written only when the surface was obtained; taking the first
# would report an extent for an attach that failed.
ATTACH = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+(?P<pid>\d+)\s+\d+\s+I\s+dashscene:\s+"
    r"attached a (?P<width>\d+)x(?P<height>\d+) surface\s*$"
)

# The device-side CPU sampler's line: the tag, then a verbatim
# `/proc/<pid>/stat`. The pid inside the stat line is captured rather than the
# logcat pid, because the logcat pid belongs to the *sampler* and the stat line
# belongs to the process being sampled.
CPU = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+\d+\s+\d+\s+I\s+dashscene-cpu:\s+(?P<stat>\d+ \(.*)$"
)

# One compositor window's start or end, logged by `unity-frame-cost.sh` beside
# a `dumpsys SurfaceFlinger --timestats -clear`/`-dump` pair (issue #1457). No
# epoch is carried in the message itself: the boundary is the epoch `-v epoch`
# already stamps on the line, the same source `CPU` above reads its own from,
# rather than a device `date` — which is whole seconds and the reason this
# project already routes the CPU reading through logcat instead of estimating
# a host mapping for it.
#
# **The pid IS carried in the message**, unlike the epoch — `unity-frame-cost.sh`
# resolves it once per sweep (the same value `ds_cpu_sampler_start` samples) and
# writes it into every marker of that sweep. A window keyed on the sweep's
# LETTER instead, inferred from whichever `dashscene-cpu` line came first in
# that sweep's file, silently named an earlier sweep's process whenever
# `ds_logcat_clear` failed to clear the ring ahead of it — which `lib.sh`
# documents as ordinary on Android 11 and later. Keying on the pid this line
# states directly is the same fix `stat_jiffies` already made for the CPU
# line: identity from content, not from position in the file.
WINDOW = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+\d+\s+\d+\s+I\s+dashscene-window:\s+"
    r"entry (?P<entry>\d+) (?P<edge>start|end) pid=(?P<pid>\d+)\s*$"
)

# `<sweep>-entry-<n>-sf-timestats.txt`, `unity-frame-cost.sh`'s per-entry
# compositor dump — `<sweep>` a letter at a device (`A-entry-1-...`), and
# whatever `read`'s own stripped basename gives a capture with no `sweep-`
# prefix, which is what the committed fixture is named for. The sweep and the
# entry number are read off the file name rather than its content, matching
# how a sample row's own sweep is read off its capture's file name.
TIMESTATS_NAME = re.compile(r"^(?P<sweep>.+)-entry-(?P<entry>\d+)-sf-timestats\.txt$")

# The app id `read_timestats` searches a dump's layer names for, when the
# caller passes none of its own.
DEFAULT_PACKAGE = "com.driftsys.dashscene.showcase"

# How many drawn frames one thread-time window covers.
#
# **Held to `ThreadCostAccumulator.Sample` by a gate, not by a comment.** This
# number is printed into every published thread-time table, and the constant it
# describes is C# in a file no Python here can read — so
# `unity/package-gate/tests/thread_cost_instrument.rs` reads the C# constant and
# asserts this line carries the same value. A count written in two languages
# drifts in one of them; that test is what makes it drift in neither.
THREAD_SAMPLE = 240

# `getconf CLK_TCK`, measured at 100 on the API 35 emulator and fixed at 100 on
# every Linux ABI Android ships. Passed in rather than assumed, and this is only
# the default.
CLK_TCK_DEFAULT = 100


def stat_jiffies(stat):
    """utime + stime out of one `/proc/<pid>/stat` line, with its pid.

    Split after the **last** `)`, never on whitespace: field 2 is the executable
    name in parentheses and it can contain spaces and parentheses both, so
    counting fields from the left is wrong for exactly the processes whose names
    are interesting. The emulator's own line is `4014 (.dashscene.demo) S ...`,
    which a left-count happens to survive — which is what makes this the kind of
    bug that ships.

    Returns None when the line does not hold the fields, rather than raising: a
    truncated line in a rotated ring is an ordinary event and it must not take
    the table down with it.
    """
    cut = stat.rfind(")")
    if cut < 0:
        return None
    try:
        pid = int(stat[: stat.index(" (")])
    except ValueError:
        return None
    fields = stat[cut + 1 :].split()
    # After the `)`: state, ppid, pgrp, session, tty_nr, tpgid, flags, minflt,
    # cminflt, majflt, cmajflt, utime, stime — so utime is index 11.
    if len(fields) < 13:
        return None
    try:
        return pid, int(fields[11]) + int(fields[12])
    except ValueError:
        return None


class Capture:
    """Everything read out of one set of logcat files.

    One object rather than a widening tuple: the three sample kinds are read by
    one pass, joined to one CPU series and reported by three emitters, and a
    six-element return would have to be unpacked correctly at every call site.
    """

    def __init__(self):
        # Per kind, in time order: "lean", "unity-frames", "unity-threads".
        self.samples = {kind: [] for kind in MARKERS}
        # Per kind, the lines that carried the marker and did not parse.
        self.unreadable = {kind: [] for kind in MARKERS}
        # pid -> time-ordered [(epoch, jiffies)].
        self.cpu = {}
        # pid -> time-ordered [(epoch, (width, height))].
        self.attaches = {}
        # (pid, entry) -> {"start": epoch, "end": epoch}, from the
        # `dashscene-window` lines `unity-frame-cost.sh` logs per entry. Keyed
        # on the pid the line states rather than on a sweep letter, for the
        # reason `WINDOW`'s own comment gives.
        self.windows = {}


def read(paths):
    """Return a `Capture` over the given logcat captures.

    **Six line kinds are read and everything else is ignored.** The three
    instrument kinds are de-duplicated by (kind, pid, epoch) and the attach and
    CPU records by (pid, epoch), for the reason `keep` gives; the sixth, a
    `dashscene-window` boundary, is de-duplicated by keeping the first
    occurrence per (pid, entry, edge) rather than through a separate set —
    see the `WINDOW` branch below.

**Every kind is de-duplicated**, and `frame-capture.sh` passes
    every `frames-<scene>.log` at once, so a line present in two captures would
    otherwise be counted twice. Without this, three drawn samples were
    reported as six, and the duplicated first row got a **negative** `wall s`:
    its interval opened at the previous copy's later timestamp. A negative wall
    time presented as a measurement is worse than a missing one.

    **The mechanism that produced the overlap is narrowed to one record, and
    the de-duplication stays.** Each capture was a full `logcat -d` dump until
    issue #1304, so a
    failed `logcat -c` between scenes — which `lib.sh` documents as ordinary on
    Android 11 and later and tolerates with `|| true` — left the later dumps
    holding the earlier scenes' sample lines. Since #1304 each capture is a
    `-T 1` follower opened after that clear. **That narrows the route to one
    line rather than closing it**: `-T 1` prints the most recent record already
    in the buffer before it follows, so on a failed clear the previous scene's
    last line — which can be a sample — still enters the next capture. A whole
    ring became one record, and this is what covers that record. It also costs
    one dictionary, and a duplicate reaching the table is a wrong number rather
    than a missing one. `frame-table-test.py` feeds the overlap synthetically
    rather than relying on a capture shape to produce it.

    A pid and an epoch to the millisecond identify one logcat record, so this
    drops re-reads of the same record and keeps two genuinely distinct samples
    even when their other fields are identical.
    """
    capture = Capture()
    cpu, attaches = capture.cpu, capture.attaches
    seen_samples, seen_cpu, seen_attach = set(), set(), set()
    for path in paths:
        # **The capture file's stem, kept per row.** `unity-frame-cost.sh` takes
        # several independent sweeps and its header promises each row names the
        # sweep it came from. The pid distinguishes them too — each sweep is its
        # own launch — but a letter is what the record's reader compares.
        sweep = os.path.basename(path)
        if sweep.endswith(".log"):
            sweep = sweep[: -len(".log")]
        if sweep.startswith("sweep-"):
            sweep = sweep[len("sweep-") :]
        try:
            # `errors="replace"`: a logcat ring can cut a UTF-8 sequence in
            # half, and one bad byte must not lose the whole capture.
            with open(path, encoding="utf-8", errors="replace") as handle:
                lines = handle.read().splitlines()
        except OSError as error:
            raise Unreadable(f"cannot read {path}: {error}") from error
        for line in lines:
            # `adb` on a host with CRLF line endings, and `﻿` from a
            # capture that went through a tool that added one.
            line = line.replace("\r", "").lstrip("﻿")
            found = SAMPLE_SPLIT.match(line) or SAMPLE_COMBINED.match(line)
            if found:
                row = found.groupdict()
                # The combined shape has no paint and no glyph counts; `—` says
                # not measured, where a zero would say measured as nothing.
                row.setdefault("paint", None)
                row.setdefault("paint50", None)
                row.setdefault("quads", None)
                keep(capture, "lean", row, sweep, seen_samples)
                continue
            found = SAMPLE_UNITY_FRAME.match(line)
            if found:
                keep(capture, "unity-frames", found.groupdict(), sweep, seen_samples)
                continue
            found = SAMPLE_UNITY_THREAD.match(line)
            if found:
                keep(capture, "unity-threads", found.groupdict(), sweep, seen_samples)
                continue
            # **Asked only after every pattern has been tried.** A line that
            # matched one of them is readable by definition; this is the branch
            # for one that carries a marker and nothing else matched.
            for kind, marker in MARKERS.items():
                if marker.match(line):
                    capture.unreadable[kind].append((path, line.strip()))
                    break
            found = ATTACH.match(line)
            if found:
                pid = int(found.group("pid"))
                epoch = float(found.group("epoch"))
                # De-duplicated on (pid, epoch) for the reason the samples are:
                # `-T 1` re-prints the newest record already in the buffer, so
                # one attach can enter two captures.
                if (pid, epoch) in seen_attach:
                    continue
                seen_attach.add((pid, epoch))
                extent = (int(found.group("width")), int(found.group("height")))
                attaches.setdefault(pid, []).append((epoch, extent))
                continue
            found = CPU.match(line)
            if found:
                parsed = stat_jiffies(found.group("stat"))
                if parsed is None:
                    continue
                pid, jiffies = parsed
                epoch = float(found.group("epoch"))
                if (pid, epoch) in seen_cpu:
                    continue
                seen_cpu.add((pid, epoch))
                cpu.setdefault(pid, []).append((epoch, jiffies))
                continue
            found = WINDOW.match(line)
            if found:
                entry = int(found.group("entry"))
                edge = found.group("edge")
                epoch = float(found.group("epoch"))
                window_pid = int(found.group("pid"))
                # A re-read (`-T 1` re-printing the newest buffered record)
                # repeats the same edge with the same epoch, so the first
                # occurrence is kept rather than de-duplicated by a separate
                # set — a second, different epoch for the same edge cannot
                # happen without two entries sharing a number.
                capture.windows.setdefault((window_pid, entry), {}).setdefault(edge, epoch)
    for readings in cpu.values():
        readings.sort()
    for events in attaches.values():
        events.sort()
    # **Sorted by time, not left in file order**, because file order is not
    # chronological: `frame-capture.sh` passes `frames-*.log`, which the shell
    # expands **alphabetically** — layout, surfaces, typography — while the
    # captures were taken in whatever order the scenes were asked for. Without
    # this, the `#` column numbers samples in an order that has nothing to do with
    # when they were reported, and the table's own claim that "the first sample of
    # a scene carries pipeline warm-up" is then about an arbitrary row. Sorting
    # also makes a negative `wall s` structurally impossible rather than merely
    # unobserved.
    for rows_of_kind in capture.samples.values():
        rows_of_kind.sort(key=lambda row: (row["epoch"], row["pid"]))
    return capture


def keep(capture, kind, row, sweep, seen):
    """Record one parsed sample, unless it is a re-read of the same record.

    Returns False when it was dropped as a duplicate. A pid and an epoch to the
    millisecond identify one logcat record, which is what `-T 1`'s re-print of
    the newest record in the buffer repeats.

    **The kind is part of the key, so two kinds cannot collide.** A shared key
    reads as safe — two instruments rarely write in the same millisecond,
    because the frame-cost line closes every 240 drawn frames and the
    thread-time line every 300 — but that safety is a property of two constants
    rather than of this function. Both lines close a window of the same 240
    frames — the warm-up is discarded at an entry change and not after a report
    — so what separates them is a constant 60-frame offset, measured at about
    1.97 s on the Pixel 5. Set `WarmUp` to 0 and that offset is 0: the two would
    be written in the same `Update`, and one of each pair would be dropped here
    as a duplicate of the other, silently. Keying on the kind costs nothing and
    removes the coupling.
    """
    row["epoch"] = float(row["epoch"])
    row["pid"] = int(row["pid"])
    row["frames"] = int(row["frames"])
    row["sweep"] = sweep
    if "width" in row:
        # A Unity line carries its own extent, so the row states it exactly
        # rather than inferring one from the attach events around it.
        row["extent"] = (int(row["width"]), int(row["height"]))
    key = (kind, row["pid"], row["epoch"])
    if key in seen:
        return False
    seen.add(key)
    capture.samples[kind].append(row)
    return True


class Unreadable(Exception):
    """A capture this script cannot read. Exit 2, never 1."""


def cpu_over(readings, start, end, clk_tck):
    """Percent of one core between `start` and `end`, or None.

    **Bracketed rather than interpolated.** The reading taken at or before
    `start` and the last one at or before `end` are used, and the percentage is
    over *those two* timestamps rather than over the requested interval — so the
    number and the window it describes always agree. Interpolating would produce
    a figure attributable to no pair of readings.

    None when the pair does not exist, which is the honest answer for a sample
    the sampler was not running across. A zero there would read as an idle
    process.
    """
    before = [r for r in readings if r[0] <= start]
    within = [r for r in readings if start < r[0] <= end]
    if not before or not within:
        return None
    first, last = before[-1], within[-1]
    span = last[0] - first[0]
    if span <= 0:
        return None
    return (last[1] - first[1]) / clk_tck / span * 100.0


def extent_over(events, start, end):
    """Every distinct extent in force over `(start, end]`, in order.

    The extent a sample was drawn at is the one attached most recently before
    its interval opened, plus any attached inside it. Returning the list rather
    than the last one is what lets the caller say "this sample spans two
    extents" instead of averaging across them — which is the defect issue #1236
    records, where `layout`'s three samples described two geometries.

    `start` of `None` means the interval has no lower bound — the first sample
    of a process with no CPU reading behind it — so every attach up to `end`
    counts. That is conservative in the right direction: it reports a change
    that may have happened before any frame in the sample rather than hiding
    one that happened during it.
    """
    in_force = []
    for epoch, extent in events:
        if epoch > end:
            break
        if start is None or epoch > start:
            in_force.append(extent)
        else:
            # Still the one in force when the interval opened; it replaces any
            # earlier one rather than adding to the list.
            in_force = [extent]
    # A re-attach at the same extent is not a change: rotation there and back,
    # and a surface recreated on resume, both produce one.
    distinct = []
    for extent in in_force:
        if not distinct or distinct[-1] != extent:
            distinct.append(extent)
    return distinct


def read_timestats(path, package):
    """The package's layer out of one `dumpsys SurfaceFlinger --timestats -dump`.

    Returns `{"total_frames", "dropped_frames", "average_fps"}` or `None` when
    no layer block names the package, or the block found does not parse as
    whole numbers — a dump taken before the app's first frame, or one the ring
    cut mid-write. Two or more candidate blocks are not themselves an
    Unreadable case: the `(BLAST)` preference below picks between them, with
    no further check that exactly one remains. `None` is reported under the
    `unity-cpu` table's own Unreadable heading and never folded into
    a row as a zero: a zero here would read as a compositor that presented
    nothing, which is a claim about the run rather than about a dump this
    parser could not read.

    Blocks are separated by a blank line, the shape `gpu-capture.sh`'s own
    dump already has; a block is a candidate when its `layerName` line
    contains `package`, the same substring test that script's own layer
    discovery uses (`grep -F`) rather than composing a name from parts that
    have changed across SurfaceFlinger releases.

    **A `(BLAST)` match is preferred over any other candidate**, for the
    reason `gpu-capture.sh` gives at its own layer discovery: a SurfaceView
    produces a container layer and a `(BLAST)` child beneath it that is the
    one actually receiving buffers, and `--maxlayers 8` can carry both (or a
    stale layer from a previous launch the compositor has not reaped) in one
    dump — a name-only match would take whichever sorts first rather than the
    layer this app is actually presenting through.
    """
    with open(path, encoding="utf-8", errors="replace") as handle:
        lines = [line.replace("\r", "") for line in handle.read().splitlines()]
    block = []
    blocks = []
    for line in lines:
        if line.strip() == "":
            if block:
                blocks.append(block)
            block = []
        else:
            block.append(line)
    if block:
        blocks.append(block)
    candidates = []
    for block in blocks:
        layer_line = next((line for line in block if line.startswith("layerName = ")), None)
        if layer_line is None or package not in layer_line:
            continue
        candidates.append((layer_line, block))
    if not candidates:
        return None
    blast = [pair for pair in candidates if "(BLAST)" in pair[0]]
    _, block = (blast or candidates)[0]
    values = {}
    for line in block:
        found = re.match(r"^(totalFrames|droppedFrames|averageFPS) = (.+)$", line)
        if found and found.group(1) not in values:
            values[found.group(1)] = found.group(2)
    if "totalFrames" not in values or "droppedFrames" not in values:
        return None
    try:
        total_frames = int(values["totalFrames"])
        dropped_frames = int(values["droppedFrames"])
    except ValueError:
        return None
    return {
        "total_frames": total_frames,
        "dropped_frames": dropped_frames,
        "average_fps": values.get("averageFPS", "—"),
    }


def unity_cpu_rows(capture, timestats_paths, clk_tck, package):
    """One row per compositor window: (sweep, entry), joined to the sampler.

    **The window is the entry's dwell, per sweep** (issue #1457) — a
    sweep-wide timestats window would give one figure per sweep rather than
    per row, so `unity-frame-cost.sh` clears and dumps once per entry instead.
    A frame-cost row cannot be aligned to a compositor window on its own,
    which is why this is a fourth table with its own rows rather than a
    column joined onto `unity-frames.md`.

    Returns `(rows, unreadable)`; `unreadable` holds `(path, reason)` for a
    dump `read_timestats` could not find the package's layer in, or whose name
    does not fit the pattern `unity-frame-cost.sh` writes at all.

    **Every path given is a row or a listed reason, never silently dropped.**
    Unlike a logcat line, which shares its stream with content this parser has
    no opinion about, every `--timestats` argument is a file the caller chose
    to name specifically — so one this pattern rejects is a caller mismatch
    worth surfacing, not an ordinary line to pass over.
    """
    frame_samples_by_sweep = {}
    for sample in capture.samples["unity-frames"]:
        frame_samples_by_sweep.setdefault(sample["sweep"], []).append(sample)
    thread_samples_by_sweep = {}
    for sample in capture.samples["unity-threads"]:
        thread_samples_by_sweep.setdefault(sample["sweep"], []).append(sample)

    # **Which app pid a sweep's own instrument lines carry**, the same
    # authoritative source `rows()` already trusts for the CPU join —
    # `dashscene-window` states its own pid directly (see `WINDOW`), but a
    # dump's file name states only the sweep letter, so resolving "this
    # sweep's pid" still has to go through the sweep's own samples. The mode
    # over both instrument kinds is what survives a stray line from another
    # launch outnumbered by the sweep's real ones; it does not survive a
    # sweep whose own samples are themselves the minority.
    sweep_pid = {}
    for sweep in set(frame_samples_by_sweep) | set(thread_samples_by_sweep):
        pids = Counter(
            sample["pid"]
            for sample in frame_samples_by_sweep.get(sweep, [])
            + thread_samples_by_sweep.get(sweep, [])
        )
        if pids:
            sweep_pid[sweep] = pids.most_common(1)[0][0]

    out = []
    unreadable = []
    for path in timestats_paths:
        name = os.path.basename(path)
        match = TIMESTATS_NAME.match(name)
        if match is None:
            unreadable.append((path, "name does not fit <sweep>-entry-<n>-sf-timestats.txt"))
            continue
        sweep = match.group("sweep")
        entry = int(match.group("entry"))

        result = read_timestats(path, package)
        if result is None:
            unreadable.append(
                (path, "no layer named the package, or its totalFrames/droppedFrames did not parse")
            )
            continue

        pid = sweep_pid.get(sweep)
        window = capture.windows.get((pid, entry), {}) if pid is not None else {}
        start, end = window.get("start"), window.get("end")
        span = None if start is None or end is None else end - start
        # **An interval that opens after it closes is not an interval**, the
        # same guard `cpu_over` already applies to its own pair of readings —
        # a re-read keeping a stale epoch, or a device clock step between the
        # two `adb shell log` round trips, must not print a negative or zero
        # `window s` as though it were a real duration.
        if span is not None and span <= 0:
            span = None

        readings = capture.cpu.get(pid, []) if pid is not None else []
        cpu = None if span is None else cpu_over(readings, start, end, clk_tck)

        drawn = 0
        extents = []
        for sample in frame_samples_by_sweep.get(sweep, []):
            # **Scoped to the resolved pid, like the window and the CPU join
            # above.** A stray sample from another launch sharing this
            # sweep's capture (the same `ds_logcat_clear` failure the pid
            # resolution above exists for) must not add its frames or its
            # extent to a row it does not belong to.
            if sample["pid"] != pid:
                continue
            if span is None or not start < sample["epoch"] <= end:
                continue
            drawn += sample["frames"]
            if sample["extent"] not in extents:
                extents.append(sample["extent"])

        out.append(
            {
                "sweep": sweep,
                "entry": entry,
                "extents": extents,
                "start": start,
                "end": end,
                "span": span,
                "presented": result["total_frames"],
                "dropped": result["dropped_frames"],
                "cpu": cpu,
                "drawn": drawn,
            }
        )
    out.sort(key=lambda row: (row["sweep"], row["entry"]))
    return out, unreadable


def rows(samples, cpu, attaches, clk_tck):
    """One row per sample, numbered per (pid, scene), with CPU attributed.

    The interval a sample covers is from the **previous sample of the same pid**
    to this one. That is the boundary `Timing` itself uses: it clears its buffers
    on every report, so consecutive lines from one process partition its drawn
    frames exactly. Keying on pid and not only on scene is what keeps a relaunch
    from being attributed to the run before it.

    The first sample of a process has no previous line, so its interval opens at
    the earliest CPU reading for that pid — which is when the sampler started,
    and is stated in the table as an open interval rather than silently treated
    as if a boundary existed.
    """
    out = []
    previous, index = {}, {}
    for sample in samples:
        pid, scene = sample["pid"], sample["scene"]
        key = (pid, scene)
        index[key] = index.get(key, 0) + 1
        readings = cpu.get(pid, [])
        start = previous.get(pid)
        opened = start is None
        if start is None:
            start = readings[0][0] if readings else None
            # **An interval that opens after it closes is not an interval.**
            # The first sample of a process has no previous line, so its
            # interval opens when the SAMPLER started — and a sampler started
            # after that sample was reported was not running across it at all.
            # Taking the difference anyway prints a negative `wall s`, which is
            # a measurement no reader can act on and worse than a missing one.
            # The same defect reached the table once before by another route,
            # from a duplicated first row; `read`'s de-duplication closed that
            # one and could not close this one.
            if start is not None and start > sample["epoch"]:
                start = None
        span = None if start is None else sample["epoch"] - start
        out.append(
            {
                **sample,
                "index": index[key],
                "span": span,
                "opened": opened,
                # **A Unity sample states its own extent**, because the player
                # writes it into every line and discards a part-sample when it
                # changes. Deriving one from the attach events instead would
                # mark a row `(changed)` on the strength of what was in force
                # when the interval opened, which for these rows is a fact about
                # the PREVIOUS sample. Everything else takes the join below.
                #
                # **Per pid, like the CPU join.** A relaunch at a new extent
                # must not relabel the run before it.
                "extents": [sample["extent"]]
                if "extent" in sample
                else extent_over(attaches.get(pid, []), start, sample["epoch"]),
                "cpu": (
                    None
                    if start is None
                    else cpu_over(readings, start, sample["epoch"], clk_tck)
                ),
            }
        )
        previous[pid] = sample["epoch"]
    return out


# **Printed on every table, and the reason it is not optional.** #885's rule is
# that nothing describes Android as working until the measurement is taken on
# target hardware. A table with no provenance is how that rule gets broken by
# accident six weeks later, when the file is the only thing left of the run.
SOURCES = {
    "emulator": (
        "**EMULATOR RESULT — NOT A DEVICE MEASUREMENT.** An emulator's adapter "
        "is the host machine's GPU behind a translation layer, so every number "
        "below describes that machine. It closes none of #885, #960, #969, "
        "#842 or #1128."
    ),
    "device": (
        "Device result. Name the device beside this table when it is recorded, "
        "and read `docs/design/android-toolchain.md` for what the adapter probe "
        "adds to it."
    ),
    "unity-showcase": (
        "Device result, from the Unity showcase player — "
        "`measure/android/unity-frame-cost.sh`, one row per reported sample and "
        "nothing averaged across sweeps. The engine floor is in every figure "
        "here: both instruments are inside a Unity frame, so the renderer's "
        "share is the difference from the empty entry's row and not the row "
        "itself. Name the device beside this table when it is recorded."
    ),
}


def extent_cell(extents):
    """One row's extent, or what it spanned when the surface changed under it.

    An em dash where nothing attached: a capture that began after the attach,
    and every bundle taken before the painter logged one. A guess would be
    worse than a gap here, because the gap is what makes a reader go and look.
    """
    if not extents:
        return "—"
    if len(extents) == 1:
        return f"{extents[0][0]}x{extents[0][1]}"
    return (
        " → ".join(f"{width}x{height}" for width, height in extents)
        + " (changed)"
    )


def cpu_footnote(clk_tck, out):
    """The CPU column's meaning, written once for all four tables.

    One function rather than four copies: the `—` rule below is the difference
    between "the sampler was not running" and "the process was idle", and a rule
    stated in three places drifts in one of them.
    """
    print(
        f"CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each "
        f"sample covers, at {clk_tck} jiffies per second, as a percentage of one "
        f"core — so a value above 100 is a process using more than one. `—` "
        f"means the sampler was not running across that interval, which is not "
        f"the same as an idle process. A `(open)` interval begins when the "
        f"sampler started rather than at a sample boundary.",
        file=out,
    )


def unreadable_report(lines, kind, out):
    """The lines that carried this table's marker and did not parse.

    **Reported rather than dropped.** Every pattern in this file is anchored on
    the end of the line so a record the logcat ring cut in half is rejected
    instead of half-read — and a rejection nothing states looks exactly like a
    run that reported fewer samples. A bullet list rather than a table, so a
    reader parsing the table's rows by their pipes does not pick these up as
    rows.
    """
    print(file=out)
    print("## Unreadable", file=out)
    print(file=out)
    if not lines:
        print(
            f"None. Every `{kind}` line in these captures parsed whole.",
            file=out,
        )
        return
    print(
        f"{len(lines)} line(s) carried the `{kind}` marker and did not parse — a "
        "record the logcat ring cut, or an instrument whose line shape changed "
        "without this parser. Each is quoted verbatim; none of them is in the "
        "table above.",
        file=out,
    )
    print(file=out)
    for path, line in lines:
        print(f"- `{os.path.basename(path)}`: `{line}`", file=out)


def extent_summary(table, out):
    """One sentence about the extents in a table whose rows each state their own.

    The Unity player writes the extent into every line and discards a sample
    that spans a change, so no row here can span two — which is why this is a
    sentence and not the four-way statement `emit` needs for a host whose extent
    comes from a separate record.
    """
    seen = []
    for row in table:
        for extent in row["extents"]:
            if extent not in seen:
                seen.append(extent)
    if len(seen) == 1:
        print(f"Every row below was drawn at {seen[0][0]}x{seen[0][1]}.", file=out)
        return
    print(
        "**The rows below were not all drawn at one extent.** Every extent "
        "these captures reported: "
        + ", ".join(f"`{width}x{height}`" for width, height in seen)
        + ". Each row states its own and no row spans two — the player discards "
        "a sample the extent changed inside — but rows at different extents are "
        "not comparable with each other (issue #1236).",
        file=out,
    )


def emit(table, source, describe, clk_tck, unreadable, out):
    print(f"# Frame costs — {describe}" if describe else "# Frame costs", file=out)
    print(file=out)
    print(SOURCES[source], file=out)
    print(file=out)
    print(
        "`paint` is this project's own instance packing, pure CPU. `submit` is "
        "the upload, the encode, the submit and the swapchain — named `submit` "
        "rather than `present` because `demo`'s desktop host prints `present` "
        "for paint plus present, and one word must not name two quantities. "
        "They are reported apart "
        "because they are different optimisation targets. `glyphs` is the "
        "glyph-quad count of the frame that **closed** the sample — a snapshot "
        "rather than a per-sample constant, since a scene whose text changes "
        "moves it: consecutive samples of `typography` reported 444 and 446. "
        "Read it as the order of magnitude the sample was drawing, never as a "
        "denominator exact for every frame in it.",
        file=out,
    )
    print(file=out)
    # **The extent, stated before the table rather than only inside it.**
    # Orientation changes the workload and not only the pixel count: re-measured
    # in landscape the same three scenes gave `typography` 14.6-15.1 ms against
    # 3.8-4.3 ms in portrait, on FEWER pixels, because a wider box lays out more
    # text. So a figure taken from a table that does not name its extent is not
    # comparable to any other figure, which is issue #1236.
    seen = []
    for row in table:
        for extent in row["extents"]:
            if extent not in seen:
                seen.append(extent)
    changed = [row for row in table if len(row["extents"]) > 1]
    # **Rows with NO extent are counted**, because the statements below are
    # about every row. A capture holding one attach line and a second process
    # that logged none produced "Every row below was drawn at 2204x805" over a
    # row whose own cell read an em dash — the claim covering a row it had no
    # reading for, which is issue #1236's defect reintroduced by its own fix.
    unknown = [row for row in table if not row["extents"]]
    if changed:
        print(
            "**The rows below were not all drawn at one extent.** "
            + "Every extent this run reported: "
            + ", ".join(f"`{width}x{height}`" for width, height in seen)
            + f". {len(changed)} row(s) span more than one and are marked "
            "`(changed)`; those are not one series and must not be compared "
            "with each other or with anything else. `settings put system "
            "user_rotation` applies only while an app that permits rotation is "
            "in front, so a capture that force-stops between scenes drifts back "
            "to the launcher's orientation (issue #1236).",
            file=out,
        )
    elif len(seen) == 1 and not unknown:
        print(
            f"Every row below was drawn at {seen[0][0]}x{seen[0][1]}.",
            file=out,
        )
    elif len(seen) == 1:
        print(
            f"{len(table) - len(unknown)} row(s) below were drawn at "
            f"{seen[0][0]}x{seen[0][1]}, and {len(unknown)} name no extent at "
            "all: no `attached a WxH surface` line precedes them. A figure "
            "with no extent beside it is not comparable to any other "
            "(issue #1236).",
            file=out,
        )
    elif seen:
        print(
            "**The rows below were not all drawn at one extent.** "
            + "Every extent this run reported: "
            + ", ".join(f"`{width}x{height}`" for width, height in seen)
            + ". No single row spans two, so each row is its own series — but "
            "rows at different extents are not comparable with each other."
            + (
                f" {len(unknown)} row(s) name no extent at all."
                if unknown
                else ""
            ),
            file=out,
        )
    else:
        print(
            "No `attached a WxH surface` line is in these captures, so no row "
            "below names the extent it was drawn at. A per-frame figure with "
            "no extent beside it is not comparable to any other (issue #1236).",
            file=out,
        )
    print(file=out)
    print(
        "One row per reported sample of 240 **drawn** frames "
        "(`demo-android/src/timing.rs`). Rows are not averaged: the first "
        "sample of a scene carries pipeline warm-up, which reaches `max` and "
        "not `p50`.",
        file=out,
    )
    print(file=out)
    print(
        "`fps if unpaced` is **not the frame rate** — the loop is paced by "
        "vsync, and this is the rate the measured work alone would allow, which "
        "is what says how much headroom there is. `wall` is how long the 240 "
        "drawn frames took, and it exceeds 240 vsyncs whenever the scene idles "
        "between pulse phases: the loop skips a frame that would draw nothing.",
        file=out,
    )
    print(file=out)
    print(
        "| scene | extent | # | pid | frames | tick ms | paint mean | paint p50 "
        "| submit mean | p50 | p95 | max | glyphs | fps if unpaced | wall s "
        "| cpu % of one core |",
        file=out,
    )
    print(
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- "
        "| --- | --- | --- | --- | --- |",
        file=out,
    )
    for row in table:
        print(
            f"| {row['scene']} | {extent_cell(row['extents'])} | {row['index']} "
            f"| {row['pid']} | {row['frames']} "
            f"| {row['tick']} | {row['paint'] or '—'} | {row['paint50'] or '—'} "
            f"| {row['mean']} "
            f"| {row['p50']} | {row['p95']} | {row['max']} "
            f"| {row.get('quads') or '—'} | {row['fps']} | {span_cell(row)} "
            f"| {cpu_cell(row)} |",
            file=out,
        )
    print(file=out)
    cpu_footnote(clk_tck, out)
    unreadable_report(unreadable, "lean", out)


def emit_unity_frames(table, source, describe, clk_tck, unreadable, out):
    """The Unity showcase player's frame-cost table.

    **Its own columns, and not the lean host's.** `draw` is one term where the
    lean host reports `paint` and `submit` apart, and printing it under either
    of those headings would label this quantity with a word that already names a
    different one — the rule `demo-android` renamed its own term for.
    """
    print(f"# Unity frame cost — {describe}" if describe else "# Unity frame cost", file=out)
    print(file=out)
    print(SOURCES[source], file=out)
    print(file=out)
    print(
        "`tick` is `ds_runtime_tick`, the one term directly comparable with the "
        "lean host's. `draw` is the frame lease, `BrgPainter.Draw`, the mark and "
        "the release — every part of the frame this project executes — and "
        "EXCLUDES the GPU's execution of the batches, URP's own passes, culling "
        "and the swapchain present, because Unity runs those after `Update` "
        "returns. `unity-threads.md` beside this file reports what that "
        "excludes. "
        "`unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs` "
        "states the definition term by term.",
        file=out,
    )
    print(file=out)
    extent_summary(table, out)
    print(file=out)
    print(
        "One row per reported sample of 240 **drawn** frames. Rows are not "
        "averaged: the first sample of an entry carries pipeline warm-up, which "
        "reaches `max` and not `p50`.",
        file=out,
    )
    print(file=out)
    print(
        "`fps if unpaced` is **not the frame rate** — Unity paces the loop, and "
        "this is the rate the two measured terms alone would allow. `wall` is "
        "how long the sample's frames took.",
        file=out,
    )
    print(file=out)
    print(
        "| sweep | entry | extent | # | pid | frames | tick ms | draw mean "
        "| p50 | p95 | max | fps if unpaced | wall s | cpu % of one core |",
        file=out,
    )
    print(
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- "
        "| --- | --- | --- |",
        file=out,
    )
    for row in table:
        print(
            f"| {row['sweep']} | {row['scene']} | {extent_cell(row['extents'])} "
            f"| {row['index']} | {row['pid']} | {row['frames']} | {row['tick']} "
            f"| {row['mean']} | {row['p50']} | {row['p95']} | {row['max']} "
            f"| {row['fps']} | {span_cell(row)} | {cpu_cell(row)} |",
            file=out,
        )
    print(file=out)
    cpu_footnote(clk_tck, out)
    unreadable_report(unreadable, "unity-frames", out)


def emit_unity_threads(table, source, describe, clk_tck, unreadable, out):
    """The Unity showcase player's thread-time table.

    **Every term here includes the engine floor**, and the table says so rather
    than subtracting a guess: the counters are Unity's own and are closed over
    the whole frame. The renderer's share is the difference from the empty
    entry's row taken in the same run.
    """
    print(f"# Unity thread cost — {describe}" if describe else "# Unity thread cost", file=out)
    print(file=out)
    print(SOURCES[source], file=out)
    print(file=out)
    print(
        "These are Unity's own `ProfilerRecorder` counters, not a bracket around "
        "code this project executes — so they include what `unity-frames.md` "
        "excludes by construction: the culling callback, the render thread's "
        "encode, URP's passes and a Canvas rebuild. `canvas` is "
        "`Canvas.SendWillRenderCanvases` plus `Canvas.BuildBatch`, which is zero "
        "for the painter and is the term a Canvas renderer is judged on. `gc` is "
        "`GC Allocated In Frame` divided by the sample's frames. "
        "`unity/com.driftsys.dashscene/Runtime/Engine/DashsceneThreadCost.cs` "
        "states the definition term by term.",
        file=out,
    )
    print(file=out)
    print(
        "**Every column carries the engine floor.** Subtract the empty entry's "
        "row, taken in the same run, for a renderer's own share; a figure read "
        "off one row alone describes Unity as much as it describes the "
        "renderer.",
        file=out,
    )
    print(file=out)
    extent_summary(table, out)
    print(file=out)
    print(
        f"One row per reported sample of {THREAD_SAMPLE} **drawn** frames, "
        "after a warm-up discarded at every entry change — so no row carries an "
        "entry's load or its first Canvas bakes. The count is "
        "`ThreadCostAccumulator.Sample`, and `unity/package-gate`'s "
        "`thread_cost_instrument` re-derives this number from that constant "
        "rather than letting the two drift.",
        file=out,
    )
    print(file=out)
    print(
        "| sweep | entry | extent | # | pid | frames "
        "| main mean | main p50 | main p95 | main max "
        "| render mean | render p50 | render p95 | render max "
        "| canvas ms | gc B/frame | wall s | cpu % of one core |",
        file=out,
    )
    print(
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- "
        "| --- | --- | --- | --- | --- | --- | --- |",
        file=out,
    )
    for row in table:
        print(
            f"| {row['sweep']} | {row['scene']} | {extent_cell(row['extents'])} "
            f"| {row['index']} | {row['pid']} | {row['frames']} "
            f"| {row['main_mean']} | {row['main_p50']} "
            f"| {row['main_p95']} | {row['main_max']} "
            f"| {row['render_mean']} | {row['render_p50']} "
            f"| {row['render_p95']} | {row['render_max']} "
            f"| {row['canvas']} | {row['gc']} "
            f"| {span_cell(row)} | {cpu_cell(row)} |",
            file=out,
        )
    print(file=out)
    cpu_footnote(clk_tck, out)
    unreadable_report(unreadable, "unity-threads", out)


def emit_unity_cpu(table, source, describe, clk_tck, unreadable, out):
    """The Unity showcase player's CPU-per-presented-frame table (issue #1457).

    D1 of
    `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
    divides the sampler's `utime + stime` by the **compositor's** frame count
    over the same window, not the player's own drawn-frame count — a layer
    that presents nothing for part of a window makes drawn and presented
    differ, which `docs/design/android-toolchain.md` records happening on
    `typography`. `presented frames` and `dropped` are the compositor's own
    count for the package's layer; `drawn frames (player)` is
    `unity-frames.md`'s frame-cost samples whose epoch falls inside this same
    window, so that gap is visible on the row rather than assumed away.
    """
    print(
        f"# Unity CPU per presented frame — {describe}"
        if describe
        else "# Unity CPU per presented frame",
        file=out,
    )
    print(file=out)
    print(SOURCES[source], file=out)
    print(file=out)
    print(
        "One row per compositor window: `dumpsys SurfaceFlinger --timestats` "
        "cleared and dumped once per entry, over the same seconds as that "
        "entry's dwell — a window over the whole sweep would give one figure "
        "per sweep rather than per row. `presented frames` and `dropped` are "
        "read from the package's own layer in that dump; a dump whose layer "
        "cannot be found is reported under Unreadable below rather than as a "
        "row of zeroes.",
        file=out,
    )
    print(file=out)
    print(
        "| sweep | entry | extent | window s | presented frames | dropped "
        "| cpu % of one core | cpu ms per presented frame "
        "| drawn frames (player) |",
        file=out,
    )
    print(
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
        file=out,
    )
    for row in table:
        print(
            f"| {row['sweep']} | {row['entry']} | {extent_cell(row['extents'])} "
            f"| {window_span_cell(row)} | {row['presented']} | {row['dropped']} "
            f"| {cpu_cell(row)} | {cpu_ms_cell(row)} | {row['drawn']} |",
            file=out,
        )
    print(file=out)
    cpu_footnote(clk_tck, out)
    print(
        "`cpu ms per presented frame` is `window s × cpu % / 100 × 1000 / "
        "presented frames` — from the three columns beside it on the same "
        "row, never recomputed from a different pair. `(open)` in `window s` "
        "here means one of this entry's two `dashscene-window` markers is "
        "missing — every `log` call `unity-frame-cost.sh` makes to write one "
        "is `|| true`, so a dropped write reads as an incomplete window rather "
        "than aborting the sweep — which is a different cause from the one "
        "the CPU footnote above states for its own `(open)`.",
        file=out,
    )
    unity_cpu_unreadable_report(unreadable, out)


def window_span_cell(row):
    """The `window s` cell.

    `(open)` where the window has only one boundary — a `dashscene-window`
    marker `unity-frame-cost.sh` never wrote, since every `log` call it makes
    to write one is `|| true`. `—` where both boundaries exist but the
    interval they describe is not positive — a re-read that kept a stale
    epoch, or a device clock step between the two `adb shell log` round
    trips — which `unity_cpu_rows` already turned into a `None` span rather
    than print a negative or zero duration as though it were real.
    """
    if row["start"] is None or row["end"] is None:
        return "(open)"
    if row["span"] is None:
        return "—"
    return f"{row['span']:.1f}"


def cpu_ms_cell(row):
    """The `cpu ms per presented frame` cell.

    `—` wherever one of its three inputs is: no CPU figure, no window span, or
    a compositor count of zero presented frames.
    """
    if row["cpu"] is None or row["span"] is None or row["presented"] <= 0:
        return "—"
    ms_per_frame = row["span"] * row["cpu"] / 100.0 * 1000.0 / row["presented"]
    return f"{ms_per_frame:.2f}"


def unity_cpu_unreadable_report(entries, out):
    """The per-entry dumps this table could not place, each with its own reason.

    Its own report rather than `unreadable_report`'s: that one describes a
    logcat line that carried an instrument's marker and failed to parse, where
    this describes a dump file — no matching layer block, a matched block
    whose totalFrames or droppedFrames did not parse, or a name
    `TIMESTATS_NAME` does not fit at all — a different failure, so it needs
    its own wording rather than reusing text that would claim a marker this
    table has none of.
    """
    print(file=out)
    print("## Unreadable", file=out)
    print(file=out)
    if not entries:
        print("None. Every dump named the package's layer.", file=out)
        return
    print(
        f"{len(entries)} dump(s) could not be placed in the table above — no "
        "layer naming the package (a capture taken before the app's first "
        "frame, one the ring cut mid-write, or a layer name changed by a "
        "SurfaceFlinger release), a matched layer whose totalFrames or "
        "droppedFrames did not parse (the ring cut it mid-write), or a name "
        "`TIMESTATS_NAME` does not fit at all. Each is named verbatim, with "
        "its own reason.",
        file=out,
    )
    print(file=out)
    for path, reason in entries:
        print(f"- `{os.path.basename(path)}`: {reason}", file=out)


def span_cell(row):
    """The `wall s` cell — the same rule the lean table prints inline."""
    if row["span"] is None:
        return "—"
    span = f"{row['span']:.1f}"
    if row["opened"]:
        # Marked rather than dropped: it is a real interval, it just does not
        # begin at a sample boundary.
        span += " (open)"
    return span


def cpu_cell(row):
    """The CPU cell. `—` where no pair of readings brackets the interval."""
    return "—" if row["cpu"] is None else f"{row['cpu']:.0f}"


# One emitter per table kind, keyed by the same names `MARKERS` uses so a kind
# cannot exist without both a pattern and a table to print it in. `unity-cpu`
# is not here: its rows come from `unity_cpu_rows`, not `rows`, and `main`
# dispatches it separately for that reason.
EMITTERS = {
    "lean": emit,
    "unity-frames": emit_unity_frames,
    "unity-threads": emit_unity_threads,
}


def main(argv):
    parser = argparse.ArgumentParser(
        description="Extract the frame-cost table from a captured logcat.",
    )
    # **Required, with no default.** Every script in this apparatus states in
    # its own output whether it is describing an emulator, and a default would
    # be the one path that produces an unlabelled table.
    parser.add_argument("--source", required=True, choices=sorted(SOURCES))
    # **Which instrument's table, with no widening of one to hold all three.**
    # A Unity `draw` printed under a `submit` heading would label a quantity
    # with a word that already names a different one, which is the rename
    # `demo-android/src/timing.rs` records. `unity-cpu` is a fourth choice not
    # in `MARKERS`: its rows are a join over dump files and window lines, not
    # one more instrument marker.
    parser.add_argument(
        "--table", default="lean", choices=sorted(set(MARKERS) | {"unity-cpu"})
    )
    parser.add_argument(
        "--describe",
        default="",
        help="what was measured, put in the heading verbatim",
    )
    parser.add_argument("--clk-tck", type=int, default=CLK_TCK_DEFAULT)
    # `--table unity-cpu` only: the per-entry `dumpsys SurfaceFlinger
    # --timestats -dump` files, and the app id to find inside them.
    parser.add_argument("--timestats", nargs="*", default=[])
    parser.add_argument("--package", default=DEFAULT_PACKAGE)
    parser.add_argument("logcat", nargs="+")
    args = parser.parse_args(argv[1:])
    if args.clk_tck <= 0:
        print("frame-table: --clk-tck must be positive", file=sys.stderr)
        return 2

    try:
        capture = read(args.logcat)
    except Unreadable as error:
        print(f"frame-table: {error}", file=sys.stderr)
        return 2

    if args.table == "unity-cpu":
        if not args.timestats:
            print(
                "frame-table: --table unity-cpu needs --timestats FILE...",
                file=sys.stderr,
            )
            return 2
        cpu_table, cpu_unreadable = unity_cpu_rows(
            capture, args.timestats, args.clk_tck, args.package
        )
        if not cpu_table and not cpu_unreadable:
            print(
                f"frame-table: no unity-cpu row over {', '.join(args.timestats)}.",
                file=sys.stderr,
            )
            return 1
        emit_unity_cpu(
            cpu_table, args.source, args.describe, args.clk_tck, cpu_unreadable, sys.stdout
        )
        return 0

    samples = capture.samples[args.table]
    unreadable = capture.unreadable[args.table]
    if not samples:
        print(
            f"frame-table: no {args.table} sample in "
            f"{', '.join(args.logcat)}. One line per 240 drawn frames is "
            "expected, so a run that idled, drew nothing, or was captured for "
            "less than a sample has none.",
            file=sys.stderr,
        )
        # **The lines that carried the marker, on the way out.** With no sample
        # there is no table to report them under, and "no sample" over a capture
        # full of truncated ones sends a reader to the player rather than to the
        # ring.
        if unreadable:
            print(
                f"frame-table: {len(unreadable)} line(s) carried the "
                f"{args.table} marker and did not parse — the logcat ring cut "
                "them, or the instrument's line shape changed. First: "
                f"{unreadable[0][1]}",
                file=sys.stderr,
            )
        if args.table == "lean":
            print(
                "frame-table: check the capture for 'first frame' — if that is "
                "absent the painter never drew, and on an emulator that is the "
                "launch mode: restart it with `-gpu host` (issue #1158).",
                file=sys.stderr,
            )
        else:
            print(
                "frame-table: check the capture for '[showcase] drew' — if that "
                "is absent the painter never drew; and for '[showcase] thread "
                "cost disarmed', which says the player cannot record a counter.",
                file=sys.stderr,
            )
        return 1

    EMITTERS[args.table](
        rows(samples, capture.cpu, capture.attaches, args.clk_tck),
        args.source,
        args.describe,
        args.clk_tck,
        unreadable,
        sys.stdout,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
