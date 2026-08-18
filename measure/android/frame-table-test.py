#!/usr/bin/env python3
"""Exercises frame-table.py beside it. Needs no device, no SDK and no NDK.

The script under test is the only thing that reads the frame samples a device
run produces, and the run that produces them costs an emulator boot, a
cross-compile, an APK and minutes of drawing — so a defect in the parser is
discovered at the device, which is the one place this story exists to keep clear.
`verdict.sh` and `assert-drew.py` are both here for the same reason, and the
comment above `harness-tests` in the justfile records what reading them instead
missed.

Every logcat line below is the real shape, taken from a capture on the API 35
emulator on 2026-08-17 rather than written from the format string — including the
em dash, the two-space column gaps and the `/proc/<pid>/stat` line with its
parenthesised process name.

    ./measure/android/frame-table-test.py
"""

import os
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "frame-table.py")

# The statuses frame-table.py documents, named so a failure reads as prose.
TABLE, NO_SAMPLE, UNREADABLE = 0, 1, 2

# One real sample line, captured. Everything else is built from it.
def sample(epoch, pid, scene="surfaces", frames=240, tick="0.26", mean="18.80",
           p50="14.33", p95="34.73", top="369.01", fps="52.5",
           paint="1.20", paint50="1.10", runs=8, quads=97):
    return (
        f"         {epoch}  {pid}  {pid + 43} I dashscene: "
        f"{scene} over {frames} frames — tick {tick} ms, "
        f"paint mean {paint} p50 {paint50}, "
        f"submit mean {mean} p50 {p50} p95 {p95} max {top} ms "
        f"({fps} fps if unpaced) — {runs} run(s), {quads} glyph(s)"
    )


def cpu(epoch, pid, utime, stime, comm=".dashscene.demo"):
    """A CPU sampler line: the tag, then a verbatim `/proc/<pid>/stat`.

    The 13 fields before `utime` are real values from the emulator; only the two
    times vary per case, which is what the join is computed from.
    """
    stat = (
        f"{pid} ({comm}) S 365 365 0 0 -1 4194624 16457 0 1404 0 "
        f"{utime} {stime} 0 0 10 -10 23 0 26405 16825294848 34984"
    )
    # The logcat pid here is the SAMPLER's, deliberately different from the pid
    # inside the stat line: the script must key on the latter.
    return f"         {epoch}  9001  9001 I dashscene-cpu: {stat}"


def cpu_cell(out, scene="surfaces", index=1):
    """The CPU cell of one row, by scene and sample number.

    **Asserting `"| — |" in out` is not this**, and the difference was found by
    mutation: the `wall s` column prints an em dash under the same conditions the
    CPU column does, so a substring check passed with the CPU column reporting a
    hard-coded `0` — the exact reading this table must never produce, because a
    zero there says the process was idle where an em dash says nothing was
    measured.
    """
    for line in out.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        # A data row is `| scene | # | pid | ... |`, so the split has a leading
        # and a trailing empty cell.
        if len(cells) > 3 and cells[1] == scene and cells[2] == str(index):
            return cells[-2]
    return None


def span_cell(out, scene="surfaces", index=1):
    """The `wall s` cell of one row — the column before the CPU one.

    Its own helper for the same reason `cpu_cell` is: `(open)` and `—` both
    appear in the table's footnote prose, so a substring check over the whole
    output passes whatever the row says. Mutation found both.
    """
    for line in out.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) > 3 and cells[1] == scene and cells[2] == str(index):
            return cells[-3]
    return None


def paint_cell(out, scene="surfaces", index=1):
    """The `paint mean` cell of one row."""
    for line in out.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) > 3 and cells[1] == scene and cells[2] == str(index):
            return cells[6]
    return None


def glyph_cell(out, scene="surfaces", index=1):
    """The `glyphs` cell of one row.

    A helper for the same reason `cpu_cell` and `span_cell` are, and it was
    needed for the same reason: the first version of the no-suffix case asserted
    `"| — |" in out`, which the `wall s` and `cpu` columns already satisfy in a
    fixture with no CPU lines. Mutating the glyph cell to a literal left all 45
    cases green. That is the third time this file has been caught by a
    whole-output substring check.
    """
    for line in out.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) > 3 and cells[1] == scene and cells[2] == str(index):
            # -1 is the empty tail, then cpu, wall, fps, glyphs.
            return cells[-5]
    return None


def combined(epoch, pid, scene="surfaces", tick="0.26", mean="18.80",
             p50="14.33", p95="34.73", top="369.01", fps="52.5"):
    """The pre-split line shape, which every capture before 2026-08-17 carries.

    `run.sh` promises in every bundle README that its tables can be re-derived
    from its captures, so the parser has to keep reading these — a promise a
    regex replacement rather than a widening would have broken for every bundle
    already taken.
    """
    return (
        f"         {epoch}  {pid}  {pid + 43} I dashscene: "
        f"{scene} over 240 frames — tick {tick} ms, "
        f"draw mean {mean} p50 {p50} p95 {p95} max {top} ms "
        f"({fps} fps if unpaced)"
    )


def run(lines, args, tmp, key):
    """Write the capture, run the script, return (status, stdout)."""
    return run_many([lines], args, tmp, key)


def run_many(captures, args, tmp, key):
    """As `run`, with one file per capture — the shape production always uses.

    `frame-capture.sh` passes every `frames-<scene>.log` at once, so a single-file
    case does not exercise the path that runs on a device. Review found that gap
    and it is the reason this helper exists.
    """
    paths = []
    for index, lines in enumerate(captures):
        path = os.path.join(tmp, f"logcat-{key}-{index}.txt")
        with open(path, "w", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
        paths.append(path)
    done = subprocess.run(
        [sys.executable, SCRIPT, *args, *paths],
        capture_output=True,
        text=True,
        check=False,
    )
    return done.returncode, done.stdout


def main():
    cases = []
    with tempfile.TemporaryDirectory() as tmp:
        def case(name, want, got):
            cases.append((name, want, got))

        def table(lines, key, args=("--source", "emulator")):
            return run(lines, list(args), tmp, key)

        # --- the line parses at all ----------------------------------------
        #
        # Anchored on the whole line, so this is also the case that says the
        # anchoring did not over-tighten against real logcat spacing.
        status, out = table([sample("1786963759.840", 4014)], "one")
        case("one real sample parses", TABLE, status)
        case("its scene reaches the table", True, "| surfaces |" in out)
        case("its p50 is carried across", True, "14.33" in out)
        case(
            "fps is copied, not recomputed from the mean",
            True,
            # 1000/(18.80+0.26) is 52.5, and the instrument said 52.5. A
            # recomputation would round the same way here, so the case that
            # separates them is the one below with a doctored fps.
            "52.5" in out,
        )
        status, out = table([sample("1786963759.840", 4014, fps="7.5")], "verbatim")
        case(
            "a doctored fps is still copied rather than recomputed",
            True,
            "| 7.5 |" in out,
        )

        # --- the paint/present split, and the glyph counts -------------------
        #
        # The split exists because a single figure could not say whether a
        # frame's cost was this project's packing or the submit path. A parser
        # that dropped either half, or swapped them, would hide exactly that.
        status, out = table(
            [sample("1786963759.840", 4014, paint="4.44", paint50="4.11",
                    mean="9.99", quads=123)],
            "split",
        )
        case("paint mean reaches the table", True, "| 4.44 |" in out)
        case("paint p50 reaches the table", True, "| 4.11 |" in out)
        case("submit mean is still its own column", True, "| 9.99 |" in out)
        case("the glyph count reaches the table", True, "| 123 |" in out)
        case(
            "paint is left of present, not swapped",
            True,
            out.index("4.44") < out.index("9.99"),
        )
        case(
            "the header says what paint and submit are",
            True,
            "instance packing" in out and "swapchain" in out,
        )

        # **A new-shape line whose glyph suffix is missing is a truncation, and is
        # rejected.** It was accepted for one revision, on two justifications
        # that were both false: a pre-split capture is rejected by the
        # `paint mean` prefix whatever its tail does, and a frame with no text
        # still emits `— 0 run(s), 0 glyph(s)`. What optional actually bought was
        # accepting a line the ring had cut.
        #
        # The cut is derived from the helper rather than written out: hardcoding
        # `" — 8 run(s)"` coupled this to the helper's default, and changing that
        # default made the split match nothing, so the case tested a line that
        # still had its suffix and stayed green.
        full = sample("1786963759.840", 4014)
        truncated = full[: full.index(" fps if unpaced)") + len(" fps if unpaced)")]
        assert "run(s)" not in truncated, "the fixture must actually be truncated"
        status, _ = table([truncated], "cutsuffix")
        case("a new-shape line cut before its glyph suffix is rejected", NO_SAMPLE, status)

        # **A zero-glyph frame is a real row**, and it is what `layout` emits.
        status, out = table(
            [sample("1786963759.840", 4014, runs=0, quads=0)], "zeroglyphs"
        )
        case("a zero-glyph frame is a row, not a rejection", TABLE, status)
        case("and its glyph cell is 0, not an em dash", "0", glyph_cell(out))

        # **The one-day `present mean` shape**, which the archived device captures
        # carry: the split instrument used that word before it was renamed to
        # `submit`, and the archive's README promises those captures can still be
        # re-derived. A rename must not orphan evidence.
        renamed = sample("1786963759.840", 4014).replace("submit mean", "present mean")
        assert "present mean" in renamed, "the fixture must actually use the old label"
        status, out = table([renamed], "oldlabel")
        case("a capture using the old `present` label still parses", TABLE, status)
        case("and its paint column is intact", "1.20", paint_cell(out))

        # --- the pre-split shape, which every earlier bundle holds -----------
        status, out = table([combined("1786963759.840", 4014)], "legacy")
        case("a pre-split capture still parses", TABLE, status)
        case("its draw figure lands in the present column", True, "| 18.80 |" in out)
        case("its paint cell is an em dash, not a zero", "—", paint_cell(out))
        case("and its glyph cell is an em dash", "—", glyph_cell(out))

        # **The header names as many columns as the rows carry.** It did not:
        # the split added three columns to every row and to the separator, and
        # the header line was edited by a replacement that silently matched
        # nothing — so a real device table shipped with twelve names over fifteen
        # columns, and every column right of `paint` was mislabelled.
        status, out = table([sample("1786963759.840", 4014)], "widths")
        rows = [line for line in out.splitlines() if line.startswith("| ")]
        widths = {line.count("|") for line in rows}
        case("header, separator and data rows have equal column counts", 1, len(widths))

        # --- the provenance label, which is not optional --------------------
        #
        # #885's rule is that nothing describes Android as working until a
        # device measures it. A table with no provenance is how that rule is
        # broken by accident, so the flag has no default.
        status, _ = run([sample("1786963759.840", 4014)], [], tmp, "nosource")
        case("--source is required", UNREADABLE, status)
        status, out = table([sample("1786963759.840", 4014)], "emu")
        case("an emulator table says so", True, "EMULATOR RESULT" in out)
        status, out = table(
            [sample("1786963759.840", 4014)], "dev", ("--source", "device")
        )
        case("a device table does not say emulator", False, "EMULATOR RESULT" in out)

        # --- the header carries what the numbers are not --------------------
        status, out = table([sample("1786963759.840", 4014)], "hdr")
        case(
            "the table states fps-if-unpaced is not the frame rate",
            True,
            "not the frame rate" in out,
        )

        # --- no sample is a result about the run, not an error ---------------
        status, out = table(["         1786963747.579  4014  4057 I dashscene: first frame"], "none")
        case("a capture with no sample exits 1", NO_SAMPLE, status)
        status, _ = run([], ["--source", "emulator", "/nonexistent/logcat"], tmp, "gone")
        case("an unreadable capture exits 2, never 1", UNREADABLE, status)

        # --- CPU attribution ------------------------------------------------
        #
        # Two samples 10 s apart. The sampler runs across both, and the second
        # sample's interval is the 10 s between the two lines — NOT the whole
        # capture. 500 jiffies over 10 s at 100 ticks/s is 5.0 s of CPU, which
        # is 50% of one core.
        lines = [
            cpu("1786963750.000", 4014, 100, 0),
            sample("1786963760.000", 4014),
            cpu("1786963760.000", 4014, 400, 100),
            cpu("1786963770.000", 4014, 900, 100),
            sample("1786963770.000", 4014),
        ]
        status, out = table(lines, "cpu")
        case("two samples give two rows", 2, out.count("| surfaces |"))
        case("the second sample's CPU is over its own interval", "50", cpu_cell(out, index=2))
        case("the sample's wall time is its own interval", "10.0", span_cell(out, index=2))
        # The first sample's interval opens at the sampler's first reading
        # rather than at a boundary, and the table says so rather than hiding
        # it: 400 jiffies over 10 s is 40%. Read out of the cell — `(open)`
        # appears in the footnote prose as well.
        case("the first interval is marked open", "10.0 (open)", span_cell(out))
        case("the open interval is still attributed", "40", cpu_cell(out))

        # **The percentage is over the two readings used, not over the sample
        # interval requested**, so the number and the window it describes always
        # agree. Here the last reading lands 2 s before the sample line: 400
        # jiffies over the 8 s between the readings is 50%, where dividing by the
        # requested 10 s would report 40%.
        lines = [
            cpu("1786963750.000", 4014, 0, 0),
            sample("1786963760.000", 4014),
            cpu("1786963760.000", 4014, 100, 0),
            cpu("1786963768.000", 4014, 400, 100),
            sample("1786963770.000", 4014),
        ]
        status, out = table(lines, "bracket")
        case(
            "CPU is divided by the readings' own span",
            "50",
            cpu_cell(out, index=2),
        )

        # A reading from another process must not be used. Same times, same
        # numbers, different pid inside the stat line.
        lines = [
            cpu("1786963750.000", 9999, 100, 0),
            cpu("1786963760.000", 9999, 900, 100),
            sample("1786963760.000", 4014),
        ]
        status, out = table(lines, "otherpid")
        case("another process's CPU is not attributed", "—", cpu_cell(out))

        # No sampler at all: the column is `—` and never 0, which would read as
        # an idle process. Read out of the CPU cell itself — see `cpu_cell`.
        status, out = table([sample("1786963760.000", 4014)], "nocpu")
        case("no CPU sampler gives an em dash, not a zero", "—", cpu_cell(out))

        # A process using more than one core reports above 100 rather than
        # being clamped: 2000 jiffies over 10 s is 20 s of CPU on 10 s of wall.
        lines = [
            cpu("1786963750.000", 4014, 0, 0),
            sample("1786963760.000", 4014),
            cpu("1786963760.000", 4014, 0, 0),
            cpu("1786963770.000", 4014, 1000, 1000),
            sample("1786963770.000", 4014),
        ]
        status, out = table(lines, "multicore")
        case("more than one core is reported as over 100", True, "| 200 |" in out)

        # --- the stat line's shape ------------------------------------------
        #
        # Field 2 can contain spaces and parentheses, and counting fields from
        # the left is wrong for exactly those processes. The emulator's own name
        # survives a left-count, which is what makes this the bug that ships.
        #
        # **`stime` is non-zero here on purpose, and mutation is why.** A
        # left-count shifts the field window by exactly two, which reads `utime`
        # where `stime` belongs — so with `stime` at zero the two readings sum to
        # the same number and the case passed against a deliberately broken
        # split. 500 + 250 of CPU over 10 s is 75%; the shifted read would give
        # 50%.
        spaced = "a name (with) spaces"
        lines = [
            cpu("1786963750.000", 4014, 0, 0, comm=spaced),
            sample("1786963760.000", 4014),
            cpu("1786963760.000", 4014, 300, 200, comm=spaced),
            cpu("1786963770.000", 4014, 800, 450, comm=spaced),
            sample("1786963770.000", 4014),
        ]
        status, out = table(lines, "spacedcomm")
        case(
            "a process name with spaces and parens still parses",
            "75",
            cpu_cell(out, index=2),
        )

        # A truncated stat line is dropped rather than raising: a rotated ring
        # cuts lines, and one bad line must not take the table down.
        lines = [
            "         1786963750.000  9001  9001 I dashscene-cpu: 4014 (.dashscene.demo) S 365",
            sample("1786963760.000", 4014),
        ]
        status, out = table(lines, "truncstat")
        case("a truncated stat line is ignored", TABLE, status)

        # A truncated SAMPLE line is ignored rather than half-read. The tail
        # carries p95, max and fps, and a partial match would report the fields
        # it did read as a row of real-looking numbers.
        truncated = (
            "         1786963759.840  4014  4057 I dashscene: surfaces over 240 "
            "frames — tick 0.26 ms, draw mean 18.80 p50 14.3"
        )
        status, _ = table([truncated], "truncsample")
        case("a truncated sample line is not half-read", NO_SAMPLE, status)

        # **Two records joined by a lost newline**, which is what the trailing
        # anchor is for and the only case that exercises it: every other case
        # here is rejected by a missing field further left. A capture piped
        # through a tool that drops a newline yields this, and without the anchor
        # the first record matches and a row is reported for a line that is two.
        joined = sample("1786963759.840", 4014) + sample("1786963770.000", 4014)
        status, _ = table([joined], "joined")
        case("two records on one line are not read as one", NO_SAMPLE, status)

        # --- ordering and identity ------------------------------------------
        #
        # Sample numbering is per (pid, scene): a relaunch starts a new process
        # and its first sample is #1 again, so a table read months later does
        # not present two runs as one series.
        lines = [sample("1786963760.000", 4014), sample("1786963790.000", 5555)]
        status, out = table(lines, "relaunch")
        case("a relaunch is not numbered as a continuation", 2, out.count("| 1 |"))

        # Two scenes in one capture are numbered independently.
        lines = [
            sample("1786963760.000", 4014, scene="surfaces"),
            sample("1786963770.000", 4014, scene="surfaces"),
            sample("1786963790.000", 4014, scene="typography"),
        ]
        status, out = table(lines, "scenes")
        case("a second scene starts at sample 1", True, "| typography | 1 |" in out)
        case("the first scene reached 2", True, "| surfaces | 2 |" in out)

        # CRLF, which is what a capture redirected on a host with CRLF endings
        # carries.
        status, _ = table([sample("1786963759.840", 4014) + "\r"], "crlf")
        case("a CRLF capture parses", TABLE, status)

        # --- several captures at once, which is the only shape production uses -
        #
        # **The captures overlap when `logcat -c` fails**, which `lib.sh`
        # documents as ordinary on Android 11 and later and tolerates with
        # `|| true`: each `frames-<scene>.log` is a full ring dump, so the later
        # ones still hold the earlier scenes' sample lines. Before the
        # de-duplication this reported three drawn samples as six, and gave the
        # duplicated row a **negative** wall time — its interval opened at the
        # other copy's later timestamp.
        first = [sample("1786963760.000", 4014, scene="surfaces")]
        second = [
            sample("1786963760.000", 4014, scene="surfaces"),  # the failed clear
            sample("1786963800.000", 4014, scene="typography"),
        ]
        status, out = run_many([first, second], ["--source", "emulator"], tmp, "overlap")
        case("an overlapping capture is not counted twice", 1, out.count("| surfaces |"))
        case("and the other scene is still there", 1, out.count("| typography |"))

        # **Numbered by time, not by the order the files were given.** The shell
        # expands `frames-*.log` alphabetically — layout, surfaces, typography —
        # and the captures are taken in the order the scenes were asked for, so
        # the two disagree in production. Here the alphabetically-first file holds
        # the *later* sample, and sample 1 must still be the earlier one.
        status, out = run_many(
            [
                [sample("1786963900.000", 4014, mean="99.00")],
                [sample("1786963700.000", 4014, mean="11.00")],
            ],
            ["--source", "emulator"],
            tmp,
            "chrono",
        )
        rows_in_order = [line for line in out.splitlines() if "| surfaces |" in line]
        case("the earliest sample is numbered 1", True, "11.00" in rows_in_order[0])
        case("and the later one is numbered 2", True, "| surfaces | 2 |" in rows_in_order[1])
        case(
            "no row reports a negative wall time",
            False,
            any(
                (span_cell(out, scene, index) or "").startswith("-")
                for scene, index in (("surfaces", 1), ("surfaces", 2))
            ),
        )

        # Two captures with genuinely distinct samples: nothing is dropped. Same
        # pid, same scene, different epochs — the case a key on scene alone would
        # collapse.
        status, out = run_many(
            [
                [sample("1786963760.000", 4014)],
                [sample("1786963775.000", 4014)],
            ],
            ["--source", "emulator"],
            tmp,
            "distinct",
        )
        case("two distinct samples across two files both appear", 2, out.count("| surfaces |"))

        # --- the clock ------------------------------------------------------
        #
        # CLK_TCK is a parameter, and a wrong one silently scales every CPU
        # figure. At 1000 ticks/s the same 500 jiffies is 0.5 s, so 5%.
        lines = [
            cpu("1786963750.000", 4014, 0, 0),
            sample("1786963760.000", 4014),
            cpu("1786963760.000", 4014, 0, 0),
            cpu("1786963770.000", 4014, 500, 0),
            sample("1786963770.000", 4014),
        ]
        status, out = table(lines, "clk", ("--source", "emulator", "--clk-tck", "1000"))
        case("--clk-tck scales the CPU figure", True, "| 5 |" in out)
        status, _ = run(
            lines, ["--source", "emulator", "--clk-tck", "0"], tmp, "clkzero"
        )
        case("a zero --clk-tck is refused rather than dividing", UNREADABLE, status)

    failed = 0
    for name, want, got in cases:
        if want != got:
            print(f"frame-table-test: FAIL {name}: wanted {want!r}, got {got!r}")
            failed += 1
    if failed:
        print(f"frame-table-test: {failed} case(s) failed")
        return 1
    print(f"frame-table-test: all {len(cases)} cases held")
    return 0


if __name__ == "__main__":
    sys.exit(main())
