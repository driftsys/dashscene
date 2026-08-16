#!/usr/bin/env python3
"""Assert that a device screenshot shows a drawn scene rather than a black frame.

`adb exec-out screencap -p` is the only view of what the painter actually put on
screen. The harness logs `surfaceCreated`, `surfaceChanged` and a runtime handle
whether or not a device was obtained, and logs nothing when one was not — issue
#960 — so a log-only check passes while the screen is black. This is the check
that does not.

It counts distinct colours below the title bar. The harness ships
`goldens/dsb/v07-text-hug-in-fill.dsb` — see `build.sh` — so a drawn frame has
dozens, most of them from glyph antialiasing.

**A frame the painter never reached does not reliably have one**, which is the
limit of this check rather than its premise: system chrome below the title bar
is counted too, and issue #1029 verified that a gesture-navigation pill alone
clears the threshold on an otherwise black frame. See `MIN_DISTINCT` below.

Exits 0 when the client area looks drawn, 1 when it does not, 2 on a file it
cannot read. Reads PNG directly: the SDK's Python has no PIL, and adding a
dependency for one assertion would be the wrong trade.
"""

import struct
import sys
import zlib

# The activity's title bar and the status bar above it are drawn by Android, not
# by the painter, so they carry colour whatever the painter did. Skipping the top
# of the frame is what makes "one distinct colour" mean "the painter drew
# nothing" rather than "the painter drew nothing but Android drew a title".
SKIP_TOP_FRACTION = 0.12

# A solid-colour scene would be a false negative, which is why the harness ships
# a fixture that is not solid — if that ever changes, this threshold has to
# change with it.
#
# **The margin narrowed and the threshold did not move** (issue #1081). This was
# written for `v03-paint.dsb`, a gradient fixture yielding thousands. The harness
# moved to `v07-text-hug-in-fill.dsb` at issue #969, so the text entry point is
# exercised, and most of the colour now comes from glyph antialiasing rather than
# from a gradient: **68 distinct colours measured on the emulator, against this
# threshold of 16.** Four times over rather than three orders of magnitude.
# That figure was taken with the early return below removed — as committed the
# script stops counting at the threshold and reports `16+`, so re-deriving it
# means lifting that return, not just running this.
#
# Sixteen is kept, and the reasoning is worth recording because the number now
# looks arbitrary where it used to look obvious. What this check separates is
# "the painter drew" from "the painter drew nothing", and the failing side of
# that is one colour, not fifteen. Raising it towards 68 would trade a real
# margin against antialiasing that differs by device, driver and scale factor.
#
# **It is not the instrument for the black-frame hole, and raising it is not the
# fix for that either.** Issue #1029 verified that an all-black frame carrying
# only an API 35 gesture-navigation pill in the bottom 4% passes this check: the
# pill's antialiasing alone supplies more than sixteen colours. `SKIP_TOP_FRACTION`
# excludes the top 12% and nothing at the bottom, so system chrome is counted as
# though the painter had drawn it. A larger threshold would raise the bar that
# chrome has to clear rather than stop counting it, which is a different and
# worse fix than excluding the bottom bar — and that is #1029's to make.
#
# Nor does a colour count know anything about *text*, which is what the fixture
# is staged for since issue #969. It cannot say the glyphs drew: it only says
# some part of the frame is not uniform, and the fill and the system chrome can
# supply that between them. Issue #1100 carries it — the second half of #1081,
# split out rather than closed with it.
MIN_DISTINCT = 16


def read_png_rgba(path):
    """Return (width, height, pixels) where pixels is a bytes-like RGBA buffer."""
    with open(path, "rb") as handle:
        data = handle.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, width, height, depth, colour = 8, bytearray(), 0, 0, 0, 0
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
        pos += 12 + length
    if depth != 8 or colour != 6:
        raise ValueError(f"expected 8-bit RGBA, got depth={depth} colour={colour}")
    raw = zlib.decompress(bytes(idat))

    # Undo the per-scanline filters. screencap emits filter 0 in practice, but a
    # decoder that assumes so silently produces garbage on any other value, and
    # garbage here reads as "many colours" — a pass. So all five are handled.
    stride, out, prev = width * 4, bytearray(), bytearray(width * 4)
    at = 0
    for _ in range(height):
        filt = raw[at]
        line = bytearray(raw[at + 1 : at + 1 + stride])
        at += 1 + stride
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if filt == 1:
                line[i] = (line[i] + a) & 0xFF
            elif filt == 2:
                line[i] = (line[i] + b) & 0xFF
            elif filt == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif filt == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        out += line
        prev = line
    # bytes, not bytearray: slices of a bytearray are unhashable, so a set of
    # them raises rather than counting — and the raise exits 1, which is the
    # same code as a genuine "drew nothing". A crash that looks like a finding.
    return width, height, bytes(out)


def main(argv):
    if len(argv) != 2:
        print("usage: assert-drew.py <screenshot.png>", file=sys.stderr)
        return 2
    try:
        width, height, pixels = read_png_rgba(argv[1])
    except (OSError, ValueError) as error:
        print(f"assert-drew: cannot read {argv[1]}: {error}", file=sys.stderr)
        return 2

    start = int(height * SKIP_TOP_FRACTION)
    seen = set()
    for y in range(start, height):
        row = y * width * 4
        # Every 7th pixel: enough to find a gradient, cheap on a 2560-wide frame,
        # and coprime with any plausible repeating pattern.
        for x in range(0, width, 7):
            i = row + x * 4
            seen.add(pixels[i : i + 3])
            if len(seen) >= MIN_DISTINCT:
                print(
                    f"assert-drew: PASS — {len(seen)}+ distinct colours below the "
                    f"title bar in {width}x{height}"
                )
                return 0
    print(
        f"assert-drew: FAIL — only {len(seen)} distinct colour(s) below the title "
        f"bar in {width}x{height}. The painter drew nothing.",
        file=sys.stderr,
    )
    print(
        "assert-drew: check logcat for 'Failed to open rendernode' — the painter "
        "reports nothing when it cannot obtain a device (issue #960).",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
