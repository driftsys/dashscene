#!/usr/bin/env python3
"""Re-derive the design record's device tables from the archived captures.

Needs no device, no editor and no SDK.

**This exists because the summary was wrong.** `docs/design/android-toolchain.md`
carries two hand-written summary tables over the archived Unity and lean-painter
runs, and the review of the pull request that added them re-derived every cell:
six of the Unity table's fifteen disagreed with the captures, `typography`'s
`max` published as `0.47-0.50` against an observed `0.76`. The branch that
published them had, in the same diff, put PR #1299 on hold for "a single
hand-transcribed sweep with no raw artifact" — so the defect was not that the
raw artifact was missing, it was that nothing re-derived the summary from it.

**A summary is still worth having.** The generated tables are one row per
sample; the record's job is to put three scenes beside three other scenes so a
reader can see which is which. What this script removes is the possibility that
the two disagree.

**Ranges, because that is what the record states.** Each cell is either one
value, when every sample agrees to the printed precision, or `lo-hi`. The
comparison is textual against exactly that form, so a record that rounds
differently fails rather than being quietly accepted.

    ./measure/android/record-check.py
"""

import os
import re
import sys

sys.dont_write_bytecode = True

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

RECORD = os.path.join(ROOT, "docs", "design", "android-toolchain.md")
BUNDLE = os.path.join(
    ROOT, "docs", "archive", "2026-08-29-v021-unity-device-measurements"
)

# The Unity player's line, as `DashsceneFrameCost.Line()` writes it. Anchored on
# the whole tail so a truncated logcat record cannot half-match.
UNITY = re.compile(
    r"frame cost — (?P<entry>.+?) at (?P<extent>\d+x\d+) over \d+ frames — "
    r"tick (?P<tick>[\d.]+) ms, "
    r"draw mean (?P<mean>[\d.]+) p50 (?P<p50>[\d.]+) p95 (?P<p95>[\d.]+) "
    r"max (?P<max>[\d.]+) ms"
)


def unity_samples():
    """Every Unity sample in the archived Vulkan sweeps, by scene."""
    out = {}
    directory = os.path.join(BUNDLE, "unity-frame-cost")
    names = sorted(n for n in os.listdir(directory) if n.startswith("sweep-"))
    if not names:
        raise SystemExit(f"record-check: no sweep-*.log under {directory}")
    for name in names:
        with open(os.path.join(directory, name), encoding="utf-8", errors="replace") as handle:
            for line in handle:
                found = UNITY.search(line)
                if not found or not found.group("entry").startswith("scene "):
                    continue
                scene = found.group("entry")[len("scene "):]
                out.setdefault(scene, []).append(found.groupdict())
    return out


def lean_rows():
    """Every lean-painter row out of the archived `frames.md`, by scene."""
    out = {}
    path = os.path.join(BUNDLE, "lean-painter", "frames.md")
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith("| ") or "---" in line or "| scene |" in line:
                continue
            cells = [cell.strip() for cell in line.split("|")]
            # | scene | extent | # | pid | frames | tick | paint mean |
            # paint p50 | submit mean | p50 | p95 | max | ...
            if len(cells) < 13:
                continue
            out.setdefault(cells[1], []).append(
                {
                    "tick": cells[6],
                    "paint": cells[7],
                    "mean": cells[9],
                    "p50": cells[10],
                    "p95": cells[11],
                }
            )
    if not out:
        raise SystemExit(f"record-check: no rows parsed out of {path}")
    return out


def span(values):
    """`lo-hi`, or one value when they agree at the printed precision."""
    ordered = sorted(values, key=float)
    return ordered[0] if ordered[0] == ordered[-1] else f"{ordered[0]}-{ordered[-1]}"


def table_after(text, heading, scenes):
    """The indented table following `heading`, as {scene: [cells]}."""
    at = text.find(heading)
    if at < 0:
        raise SystemExit(f"record-check: {RECORD} no longer carries:\n  {heading}")
    rows = {}
    for line in text[at:].splitlines():
        parts = line.split()
        if parts and parts[0] in scenes and line.startswith("    "):
            rows[parts[0]] = parts[1:]
        elif rows and not line.strip():
            break
    return rows


def main():
    with open(RECORD, encoding="utf-8") as handle:
        record = str(handle.read())

    scenes = ("surfaces", "typography", "layout")
    failed = []

    unity = unity_samples()
    published = table_after(
        record, "Unity, Vulkan, `RawBuffer`, 1080x2340,", scenes
    )
    for scene in scenes:
        samples = unity.get(scene)
        if not samples:
            failed.append(f"unity/{scene}: no sample in the archived sweeps")
            continue
        want = [span([s[k] for s in samples]) for k in ("tick", "mean", "p50", "p95", "max")]
        got = published.get(scene)
        if got is None:
            failed.append(f"unity/{scene}: the record's table has no such row")
        elif got != want:
            failed.append(
                f"unity/{scene}: record says {got}, the captures say {want}"
            )

    lean = lean_rows()
    published = table_after(
        record, "release, 1080x1984, three samples per scene:", scenes
    )
    for scene in scenes:
        rows = lean.get(scene)
        if not rows:
            failed.append(f"lean/{scene}: no row in the archived frames.md")
            continue
        want = [span([r[k] for r in rows]) for k in ("tick", "paint", "mean", "p50", "p95")]
        got = published.get(scene)
        if got is None:
            failed.append(f"lean/{scene}: the record's table has no such row")
        elif got != want:
            failed.append(
                f"lean/{scene}: record says {got}, the captures say {want}"
            )

    if failed:
        print("record-check: the design record does not match its own captures:")
        for line in failed:
            print(f"  {line}")
        print("record-check: docs/design/android-toolchain.md, against")
        print(f"record-check: {os.path.relpath(BUNDLE, ROOT)}")
        return 1

    print(
        f"record-check: both device tables re-derive from the archive "
        f"({sum(len(v) for v in unity.values())} Unity sample(s), "
        f"{sum(len(v) for v in lean.values())} lean row(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
