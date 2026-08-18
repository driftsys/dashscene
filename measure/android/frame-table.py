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

Two line kinds are read and everything else is ignored:

    <epoch> <pid> <tid> I dashscene: <scene> over <n> frames — tick ...
    <epoch> <pid> <tid> I dashscene-cpu: <the /proc/<pid>/stat line>

The first is `Sample::line()`. The second is written by the device-side sampler
`frame-capture.sh` starts, through the `log` command, **so that both kinds carry
one clock and one ordering**. Reading `/proc/<pid>/stat` into a host file
instead would need the device epoch mapped onto the host's, and `date +%s` on
the device is whole seconds — a ±1 s error on an interval of a few seconds.
Routing the readings through logcat removes the mapping rather than estimating
it.

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

## Exit status

0 with a table, 1 when the capture holds no sample at all, 2 on usage or a file
that cannot be read. 1 and 2 are separate for the reason `assert-drew.py`
separates its own: 1 is a result about the run, and 2 is "ask me again".
"""

import argparse
import re
import sys

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
    r"submit mean (?P<mean>[\d.]+) p50 (?P<p50>[\d.]+) p95 (?P<p95>[\d.]+) "
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

# The device-side CPU sampler's line: the tag, then a verbatim
# `/proc/<pid>/stat`. The pid inside the stat line is captured rather than the
# logcat pid, because the logcat pid belongs to the *sampler* and the stat line
# belongs to the process being sampled.
CPU = re.compile(
    r"^\s*(?P<epoch>\d+\.\d+)\s+\d+\s+\d+\s+I\s+dashscene-cpu:\s+(?P<stat>\d+ \(.*)$"
)

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


def read(paths):
    """Return (samples, cpu) from the given logcat captures.

    `samples` is a list of dicts in file order; `cpu` maps pid to a
    time-ordered list of (epoch, jiffies).

    **Both kinds are de-duplicated by (pid, epoch), and the reason is that the
    captures overlap.** `frame-capture.sh` passes every `frames-<scene>.log` at
    once, and each is a full `logcat -d` dump — so when `logcat -c` fails between
    scenes, which `lib.sh` documents as ordinary on Android 11 and later and
    tolerates with `|| true`, the later dumps still hold the earlier scenes'
    sample lines. Without this, three drawn samples were reported as six, and the
    duplicated first row got a **negative** `wall s`: its interval opened at the
    previous copy's later timestamp. A negative wall time presented as a
    measurement is worse than a missing one.

    A pid and an epoch to the millisecond identify one logcat record, so this
    drops re-reads of the same record and keeps two genuinely distinct samples
    even when their other fields are identical.
    """
    samples, cpu = [], {}
    seen_samples, seen_cpu = set(), set()
    for path in paths:
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
                row["epoch"] = float(row["epoch"])
                row["pid"] = int(row["pid"])
                row["frames"] = int(row["frames"])
                key = (row["pid"], row["epoch"])
                if key in seen_samples:
                    continue
                seen_samples.add(key)
                samples.append(row)
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
    for readings in cpu.values():
        readings.sort()
    # **Sorted by time, not left in file order**, because file order is not
    # chronological: `frame-capture.sh` passes `frames-*.log`, which the shell
    # expands **alphabetically** — layout, surfaces, typography — while the
    # captures were taken in whatever order the scenes were asked for. Without
    # this, the `#` column numbers samples in an order that has nothing to do with
    # when they were reported, and the table's own claim that "the first sample of
    # a scene carries pipeline warm-up" is then about an arbitrary row. Sorting
    # also makes a negative `wall s` structurally impossible rather than merely
    # unobserved.
    samples.sort(key=lambda row: (row["epoch"], row["pid"]))
    return samples, cpu


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


def rows(samples, cpu, clk_tck):
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
        span = None if start is None else sample["epoch"] - start
        out.append(
            {
                **sample,
                "index": index[key],
                "span": span,
                "opened": opened,
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
}


def emit(table, source, describe, clk_tck, out):
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
        "| scene | # | pid | frames | tick ms | paint mean | paint p50 "
        "| submit mean | p50 | p95 | max | glyphs | fps if unpaced | wall s "
        "| cpu % of one core |",
        file=out,
    )
    print(
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
        file=out,
    )
    for row in table:
        span = "—" if row["span"] is None else f"{row['span']:.1f}"
        if row["opened"] and row["span"] is not None:
            # Marked rather than dropped: it is a real interval, it just does
            # not begin at a sample boundary.
            span += " (open)"
        cpu = "—" if row["cpu"] is None else f"{row['cpu']:.0f}"
        print(
            f"| {row['scene']} | {row['index']} | {row['pid']} | {row['frames']} "
            f"| {row['tick']} | {row['paint'] or '—'} | {row['paint50'] or '—'} "
            f"| {row['mean']} "
            f"| {row['p50']} | {row['p95']} | {row['max']} "
            f"| {row.get('quads') or '—'} | {row['fps']} | {span} | {cpu} |",
            file=out,
        )
    print(file=out)
    print(
        f"CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each "
        f"sample covers, at {clk_tck} jiffies per second, as a percentage of one "
        f"core — so a value above 100 is a process using more than one. `—` "
        f"means the sampler was not running across that interval, which is not "
        f"the same as an idle process. A `(open)` interval begins when the "
        f"sampler started rather than at a sample boundary.",
        file=out,
    )


def main(argv):
    parser = argparse.ArgumentParser(
        description="Extract the frame-cost table from a captured logcat.",
    )
    # **Required, with no default.** Every script in this apparatus states in
    # its own output whether it is describing an emulator, and a default would
    # be the one path that produces an unlabelled table.
    parser.add_argument("--source", required=True, choices=sorted(SOURCES))
    parser.add_argument(
        "--describe",
        default="",
        help="what was measured, put in the heading verbatim",
    )
    parser.add_argument("--clk-tck", type=int, default=CLK_TCK_DEFAULT)
    parser.add_argument("logcat", nargs="+")
    args = parser.parse_args(argv[1:])
    if args.clk_tck <= 0:
        print("frame-table: --clk-tck must be positive", file=sys.stderr)
        return 2

    try:
        samples, cpu = read(args.logcat)
    except Unreadable as error:
        print(f"frame-table: {error}", file=sys.stderr)
        return 2

    if not samples:
        print(
            "frame-table: no frame sample in "
            f"{', '.join(args.logcat)}. One line per 240 drawn frames is "
            "expected, so a run that idled, drew nothing, or was captured for "
            "less than a sample has none.",
            file=sys.stderr,
        )
        print(
            "frame-table: check the capture for 'first frame' — if that is "
            "absent the painter never drew, and on an emulator that is the "
            "launch mode: restart it with `-gpu host` (issue #1158).",
            file=sys.stderr,
        )
        return 1

    emit(rows(samples, cpu, args.clk_tck), args.source, args.describe, args.clk_tck, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
