#!/usr/bin/env python3
"""Assert that a device screenshot shows a drawn scene rather than a black frame.

`adb exec-out screencap -p` is the only view of what the painter actually put on
screen. The harness logs `surfaceCreated`, `surfaceChanged` and a runtime handle
whether or not the painter obtained a device, so a log-only check passes while
the screen is black. This is the check that does not.

It asks three things of the client area, and all of them have to hold:

    distinct colours      is anything drawn at all
    ink at or above a     did the *text* draw — the fixture's glyphs are the
    floor                 only dark-on-light content in it
    ink below a ceiling   is the frame still mostly light. Undrawn area is
                          black, and black is ink, so without this a large
                          unpainted region reads as text

Exits 0 when the client area looks drawn, 1 when it does not, 2 on a file it
cannot read, and 3 when something drew but the text did not. Three is separate
from one because the remedies have nothing in common: 1 points at the painter
and the GPU device, 3 points at the text path (issue #1100).

Reads PNG directly: the SDK's Python has no PIL, and adding a dependency for one
assertion would be the wrong trade.

## The rect, and why it is not optional in multi-window (issue #1191)

`screencap` captures the whole display. With no `--rect` this surveys all of it
minus a fraction of the top and bottom, which is roughly the painter's area **in
the fullscreen phase only**. In multi-window the painter owns about half the
screen and another window owns the rest, so a colourful second pane can supply
the distinct colours, the light ground and the ink while the painter's own pane
is black — a false PASS by a route no exclusion fraction can close, because the
region to exclude is wherever the window manager put the other window.

`--rect X,Y,W,H` is the answer, and `HarnessActivity` logs exactly that. Measured
on the API 35 emulator on 2026-08-17, where the display is 2560x1600 and the
status and title bars take the first 176 rows:

    I dashscene: harness: window bounds 0,176 2560x1360

The exclusion fractions do **not** apply inside that rect — see `survey`, which
carries the reason and the measurement. The whole-display path is kept and is
unchanged, because a run that cannot read the bounds must still be able to check
the fullscreen phase.
"""

import argparse
import struct
import sys
import zlib

# **The client area is what the painter owns; everything else is Android's.**
#
# The activity's title bar and the status bar above it are drawn by the system,
# and so is the gesture-navigation bar along the bottom, so all of them carry
# colour whatever the painter did. Excluding them is what makes "one distinct
# colour" mean "the painter drew nothing" rather than "the painter drew nothing
# but Android drew its chrome".
#
# **Only the top was excluded until issue #1029, and that made this check pass
# on the black frame it exists to catch.** Measured on 2026-08-16 against two
# real screenshots from `just android-splitscreen`, on an emulator whose painter
# never obtained a device (`Failed to open rendernode`), so both frames are
# black in the client area — 1 distinct colour from 12% to 94%:
#
#     skip top 0.12, bottom 0.00   fullscreen  88 colours -> PASS   <- the bug
#                                  multiwindow 67 colours -> PASS   <- the bug
#     skip top 0.12, bottom 0.08   fullscreen   1 colour  -> FAIL
#                                  multiwindow 65 colours -> PASS   <- still wrong
#     skip top 0.18, bottom 0.08   fullscreen   1 colour  -> FAIL
#                                  multiwindow  1 colour  -> FAIL
#
# The bottom fraction is the gesture-navigation bar, which occupied the bottom
# 6% of a 2560x1600 frame. The top had to grow as well, which issue #1029 did
# not anticipate: **in multi-window the app gets a caption bar**, and it sat at
# 12%-16% — below the old exclusion — supplying 65 colours on its own.
#
# Both are fractions rather than pixel counts because the two hosts this runs
# against have different densities, and both are generous rather than tight: the
# cost of excluding too much is a smaller client area to find colour in, and the
# cost of excluding too little is the false PASS above.
SKIP_TOP_FRACTION = 0.18

SKIP_BOTTOM_FRACTION = 0.08

# A solid-colour scene would be a false negative, which is why the harness ships
# a fixture that is not solid — if that ever changes, this threshold has to
# change with it.
#
# **The margin narrowed and the threshold did not move** (issue #1081). This was
# written for `v03-paint.dsb`, a gradient fixture yielding thousands. The harness
# moved to `v07-text-hug-in-fill.dsb` at issue #969, so the text entry point is
# exercised, and most of the colour now comes from glyph antialiasing rather than
# from a gradient. Rendered through the Skia reference painter on the host, that
# fixture yields **55 distinct colours on this script's sampling grid** — which is
# the quantity compared against, and not the 214 the whole image carries. Quoting
# the image figure would overstate the headroom roughly fourfold.
#
# Sixteen is kept. What this check separates is "the painter drew" from "the
# painter drew nothing", and the failing side of that is one colour, not fifteen.
# Raising it towards 55 would trade a real margin against antialiasing that
# differs by device, driver and scale factor.
#
# **It is no longer the only thing standing between a black frame and a PASS**,
# which it was when issue #1029 was filed. The exclusions above remove the system
# chrome that used to supply this count on an undrawn frame, and the ink band
# below asks the question this count cannot answer.
MIN_DISTINCT = 16

# **Colour count cannot say the glyphs drew** (issue #1100), and the glyphs are
# why the harness stages this fixture at all: the JNI text entry point, the face
# cascade and the committed MSDF sheet are all on that path, and nothing else in
# this repository exercises them.
#
# The same colours would be reported by a gradient, or by the fixture with every
# glyph missing and its background drawn. So one further quantity is measured —
# the share of sampled pixels that are ink — and it is bounded on **both** sides.
#
# **Both bounds are load-bearing, and two review rounds were needed to see why.**
# Ink is "darker than luma 128", so undrawn black is ink:
#
#   * asking only for a floor passed a frame that was black except a chrome band
#     at 20%-24% of the height — `829 distinct colours and 14883 ink pixels`,
#     every one of them the black background;
#   * adding "at least half the area is light" was not enough either. A frame
#     that is a colourful light wash over its top 62% and pure black below
#     passed with `59.5% light and 6210 ink pixels`. The light majority and the
#     ink were measured over disjoint regions, so up to half the client area
#     could be undrawn and still report that the text drew.
#
# A ceiling closes both, because it is the same measurement read the other way:
# glyphs are a *small* part of a light frame, and a large dark region is not this
# fixture however light the rest is. It also removes the separate light fraction,
# which was only ever `1 - ink`.
#
# **A fraction, not a count.** The floor was 12 sampled pixels, which is 0.94% of
# the host render's grid but 0.0028% of a 2560x1600 device frame — so on the
# device it runs against, any dark icon or cursor satisfied "the glyphs drew".
# Fractions hold across both.
#
# **The fixture is what makes the numbers meaningful.** Rendered on the host,
# `v07-text-hug-in-fill.dsb` is a near-white ground with a lavender panel and an
# orange chip, and the string "hug inside fill" is the only dark thing in it:
# **0.94% of sampled pixels are ink**, every one between 40% and 60% of the
# height. The band below is an order of magnitude either side of that.
#
# Like `MIN_DISTINCT`, both bounds are coupled to that fixture: a dark-themed
# scene would breach the ceiling and be reported as an undrawn frame.
# `harness/build.sh` chooses the scene and records the coupling.
#
# **Not calibrated on a device, which is a real limitation.** The emulator
# available on 2026-08-16 could not obtain a GPU device at all (issue #1158), so
# no drawn device frame exists to measure. The first device that draws should
# re-derive both — and the failure messages say so, because a false FAIL here
# would otherwise read as a text-path regression.
MIN_INK_FRACTION = 0.001

MAX_INK_FRACTION = 0.10

# Luma below which a sampled pixel counts as ink. Rec. 601 weights, integer
# arithmetic: this file has no numpy and no PIL either.
INK_LUMA = 128


class Unreadable(Exception):
    """A screenshot this script cannot decode. Reported as exit 2, never as 1.

    The distinction is the whole point: exit 1 is the painter's verdict and
    exit 2 is "ask me again with a file I can read". Issue #1029 §2 records the
    two decode paths that used to escape as an uncaught traceback and exit 1 —
    which `just android-splitscreen` then reported as "the painter drew
    nothing", a diagnosis for a bug that had not happened.
    """


def read_png_rgba(path):
    """Return (width, height, pixels) where pixels is a bytes-like RGBA buffer."""
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError as error:
        raise Unreadable(f"cannot open: {error}") from error

    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise Unreadable("not a PNG")

    pos, idat = 8, bytearray()
    width = height = depth = colour = 0
    compression = filter_method = interlace = 0
    seen_ihdr = False
    # Every field read out of the file is a length or an offset into it, so a
    # truncated file walks off the end. `struct.error` and `IndexError` are as
    # much "cannot read this" as `OSError` is, and both used to escape.
    try:
        while pos < len(data):
            (length,) = struct.unpack(">I", data[pos : pos + 4])
            kind = data[pos + 4 : pos + 8]
            body = data[pos + 8 : pos + 8 + length]
            if kind == b"IHDR":
                # **All seven fields, not the first four.** Reading width,
                # height, depth and colour type and stopping left the
                # compression, filter and interlace methods unexamined, so an
                # Adam7-interlaced PNG decoded as though it were progressive —
                # producing a scrambled but colour-rich buffer, which reads as
                # "many colours" and passes (issue #1029 §3).
                (
                    width,
                    height,
                    depth,
                    colour,
                    compression,
                    filter_method,
                    interlace,
                ) = struct.unpack(">IIBBBBB", body[:13])
                seen_ihdr = True
            elif kind == b"IDAT":
                idat += body
            elif kind == b"IEND":
                break
            pos += 12 + length
    except (struct.error, IndexError) as error:
        raise Unreadable(f"truncated or malformed chunk stream: {error}") from error

    if not seen_ihdr:
        raise Unreadable("no IHDR")
    if depth != 8 or colour != 6:
        raise Unreadable(f"expected 8-bit RGBA, got depth={depth} colour={colour}")
    if compression != 0 or filter_method != 0:
        raise Unreadable(
            f"unsupported compression={compression} filter method={filter_method}"
        )
    if interlace != 0:
        # Refused rather than decoded wrongly, for the reason above: garbage
        # here reads as "many colours", which is a pass.
        raise Unreadable("interlaced PNG (Adam7); screencap does not emit one")

    try:
        raw = zlib.decompress(bytes(idat))
    except zlib.error as error:
        # The likeliest real corruption path: an `adb exec-out screencap`
        # interrupted mid-transfer. It used to escape as a traceback and exit 1.
        raise Unreadable(f"corrupt image data: {error}") from error

    stride = width * 4
    if len(raw) < height * (1 + stride):
        raise Unreadable(
            f"IDAT carries {len(raw)} bytes, {height * (1 + stride)} needed for "
            f"{width}x{height}"
        )

    out, prev = bytearray(), bytearray(stride)
    at = 0
    for _ in range(height):
        filt = raw[at]
        line = bytearray(raw[at + 1 : at + 1 + stride])
        at += 1 + stride
        # **Filter 0 is lifted out of the per-byte loop, and it is the path
        # every real run takes** — screencap emits filter 0. Measured at 2.72 s
        # for a 2560x1600 frame before this: 16.4 M iterations, each doing three
        # indexed reads and up to four comparisons that cannot change the byte
        # (issue #1029 §4). It decodes identically.
        if filt == 0:
            out += line
            prev = line
            continue
        if filt > 4:
            # An out-of-range filter used to match none of the branches below
            # and be applied as filter 0, desynchronising the decode from that
            # scanline on — and garbage reads as "many colours", a pass
            # (issue #1029 §3).
            raise Unreadable(f"scanline filter {filt} is not one of 0..4")
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
            else:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        out += line
        prev = line
    # bytes, not bytearray: slices of a bytearray are unhashable, so a set of
    # them raises rather than counting — and that raise would exit 1, the same
    # code as a genuine "drew nothing". A crash that looks like a finding.
    return width, height, bytes(out)


def clamp_rect(rect, width, height):
    """The intersection of `rect` with the image, or None when it is empty.

    **Clamped rather than trusted** (issue #1191). The rect comes from a logcat
    line, and a window can be reported partly offscreen, or a screenshot can be
    taken at a different rotation from the one the bounds were logged at. An
    unclamped rect walks off the pixel buffer, and the row arithmetic below would
    then read the *next* row's pixels rather than raising — sampling a region
    that is not the painter's and reporting a verdict about it.
    """
    x, y, w, h = rect
    left, top = max(0, x), max(0, y)
    right, bottom = min(width, x + w), min(height, y + h)
    if right <= left or bottom <= top:
        return None
    return left, top, right - left, bottom - top


def survey(width, height, pixels, rect=None):
    """Return (colours, ink, sampled) over the client area.

    `light` is not returned because it is not a second measurement: it is
    `sampled - ink` by definition, and returning both invited the two to be
    compared over different regions, which is exactly how a 38%-black frame
    passed review round two.

    **`rect` is the painter's own window when it is known** (issue #1191), so in
    multi-window the survey covers the painter's pane and not the neighbour that
    happens to be colourful. With `rect` absent this is the whole display,
    unchanged.

    **The exclusions do not apply inside a rect, and that is the point rather
    than an omission.** They exist to remove *system chrome* — the status bar, the
    activity's title bar, the multi-window caption and the gesture-navigation bar
    — every one of which is **outside** the painter's window. The rect has already
    removed them, so applying the fractions again would throw away 26% of the
    painter's own pane, and it would throw away the wrong 26%: measured on the API
    35 emulator on 2026-08-17, the window is `0,176 2560x1360` and an 18% top
    fraction of *that* starts the survey at row 420, where the harness's document
    is drawn in rows 176 to 300. The whole of what the painter drew fell inside
    the exclusion, and the script reported one distinct colour — "the painter drew
    nothing" — about a frame that had drawn.
    """
    if rect is None:
        left, top = 0, 0
        region_width, region_height = width, height
        start = int(height * SKIP_TOP_FRACTION)
        stop = height - int(height * SKIP_BOTTOM_FRACTION)
    else:
        left, top, region_width, region_height = rect
        start, stop = top, top + region_height
    seen, ink, sampled = set(), 0, 0
    for y in range(start, stop):
        row = y * width * 4
        # Every 7th pixel: enough to find a gradient, cheap on a 2560-wide
        # frame, and coprime with any plausible repeating pattern. The ink
        # bounds are fractions of this same grid, so they move with it.
        #
        # **Stepped from the region's own left edge**, so the phase of the grid
        # is the same whether or not a rect was given — a grid anchored at the
        # display's origin would sample different columns of the same window
        # depending on where the window manager put it.
        for x in range(left, left + region_width, 7):
            i = row + x * 4
            red, green, blue = pixels[i], pixels[i + 1], pixels[i + 2]
            seen.add(pixels[i : i + 3])
            sampled += 1
            if (red * 299 + green * 587 + blue * 114) // 1000 < INK_LUMA:
                ink += 1
    return seen, ink, sampled


def parse_rect(text):
    """`X,Y,W,H` into four ints, for argparse.

    Refused rather than partly parsed: a rect that arrived as three numbers, or
    with a stray character from a logcat line, would otherwise become a survey
    over the wrong region — and this script's whole job is to report what is in a
    region.
    """
    parts = text.split(",")
    if len(parts) != 4:
        raise argparse.ArgumentTypeError(f"expected X,Y,W,H, got {text!r}")
    try:
        x, y, w, h = (int(part) for part in parts)
    except ValueError:
        raise argparse.ArgumentTypeError(f"expected four integers, got {text!r}") from None
    if w <= 0 or h <= 0:
        raise argparse.ArgumentTypeError(f"a rect must have a positive extent, got {text!r}")
    return x, y, w, h


def main(argv):
    parser = argparse.ArgumentParser(
        description="Assert a screenshot shows a drawn scene, not a black frame.",
    )
    parser.add_argument("screenshot")
    parser.add_argument(
        "--rect",
        type=parse_rect,
        default=None,
        metavar="X,Y,W,H",
        help=(
            "the painter's own window in the screenshot, from HarnessActivity's "
            "'window bounds' line. Required to judge the multi-window phase "
            "(issue #1191); without it the whole display is surveyed."
        ),
    )
    # argparse exits 2 on a usage error, which is this script's own "ask me again"
    # status — so the two already agree and nothing is remapped here.
    args = parser.parse_args(argv[1:])
    path = args.screenshot
    try:
        width, height, pixels = read_png_rgba(path)
    except Unreadable as error:
        print(f"assert-drew: cannot read {path}: {error}", file=sys.stderr)
        return 2

    rect = None
    if args.rect is not None:
        rect = clamp_rect(args.rect, width, height)
        if rect is None:
            print(
                f"assert-drew: cannot judge {path}: the rect "
                f"{','.join(str(v) for v in args.rect)} does not intersect the "
                f"{width}x{height} screenshot. A rotation between the logged "
                f"bounds and the capture does this.",
                file=sys.stderr,
            )
            return 2
        if rect != args.rect:
            print(
                f"assert-drew: the rect was clamped to the screenshot: "
                f"{','.join(str(v) for v in rect)}",
                file=sys.stderr,
            )

    seen, ink, sampled = survey(width, height, pixels, rect)

    # **No sampled pixels is "cannot judge", not "drew nothing".** A 0x0 frame,
    # or any height whose exclusions leave no rows, otherwise reported zero
    # colours and the no-GPU-device diagnosis — the exit-1-for-exit-2 confusion
    # issue #1029 §2 exists to remove, reached by a different route.
    if sampled == 0:
        # **The two messages name different causes, because the causes are
        # different.** Without a rect this is the exclusions leaving no rows. With
        # one it cannot be: `clamp_rect` returns None for an empty intersection
        # and otherwise guarantees at least one row and column, and no exclusion
        # applies inside a rect — so naming the fractions here would point a
        # reader at arithmetic that did not run. The branch is kept for the rect
        # path rather than assumed unreachable: it is one line, and a future edit
        # relaxing `clamp_rect` would otherwise reach a message that is wrong.
        if rect is None:
            print(
                f"assert-drew: cannot judge {path}: {width}x{height} leaves no "
                f"client area after excluding the top {SKIP_TOP_FRACTION:.0%} and "
                f"bottom {SKIP_BOTTOM_FRACTION:.0%}.",
                file=sys.stderr,
            )
        else:
            print(
                f"assert-drew: cannot judge {path}: the rect "
                f"{rect[0]},{rect[1]} {rect[2]}x{rect[3]} sampled no pixels of the "
                f"{width}x{height} screenshot.",
                file=sys.stderr,
            )
        return 2

    # **The region is named in every verdict**, passing or failing. A PASS over
    # the whole display and a PASS over the painter's own window are different
    # claims — issue #1191 is the record of the first being read as the second —
    # and a transcript is usually all that is left of a run.
    where = (
        f"in the client area of {width}x{height} "
        f"(top {SKIP_TOP_FRACTION:.0%} and bottom {SKIP_BOTTOM_FRACTION:.0%} excluded)"
        if rect is None
        else (
            f"in the whole of the painter's window at {rect[0]},{rect[1]} "
            f"{rect[2]}x{rect[3]} of a {width}x{height} screenshot "
            f"(no chrome exclusion: the window has none in it)"
        )
    )
    inked = ink / sampled

    def no_device():
        # **Issue #1158, and deliberately not issue #960.** This script and
        # `android-splitscreen` both used to cite #960 here, which matches that
        # issue's *title* — but its body is entirely the deadlock reproducer,
        # `surfaceDestroyed` entered and never returned, and never mentions a
        # GPU device or a black frame. A reader chasing the citation found no
        # evidence for the claim it supported (issue #1029 §5). #1158 is the
        # issue whose body measures this symptom and names the remedy.
        print(
            "assert-drew: check logcat for 'Failed to open rendernode'. The painter "
            "draws nothing when it cannot obtain a device, and on an emulator that "
            "is the launch mode: restart it with `-gpu host` (issue #1158).",
            file=sys.stderr,
        )

    if len(seen) < MIN_DISTINCT:
        print(
            f"assert-drew: FAIL — only {len(seen)} distinct colour(s) {where}. "
            f"The painter drew nothing.",
            file=sys.stderr,
        )
        no_device()
        return 1

    # **The ceiling is checked before the floor, and the order is the point.**
    # Undrawn area is black and black is ink, so a frame with a large unpainted
    # region clears any floor. Asking "is this mostly light" first means the
    # floor below is only ever read on a frame that plausibly is this fixture.
    if inked > MAX_INK_FRACTION:
        print(
            f"assert-drew: FAIL — {len(seen)} distinct colour(s) {where}, but "
            f"{inked:.1%} of it is dark, above the {MAX_INK_FRACTION:.0%} ceiling. "
            f"The fixture is dark text on a near-white ground, so a client area "
            f"this dark is not it drawn — the colour above is chrome or another "
            f"window, not the painter.",
            file=sys.stderr,
        )
        no_device()
        return 1

    if inked < MIN_INK_FRACTION:
        print(
            f"assert-drew: FAIL — {len(seen)} distinct colour(s) {where} and only "
            f"{inked:.3%} of it dark, below the {MIN_INK_FRACTION:.1%} floor. "
            f"The background drew and the text did not.",
            file=sys.stderr,
        )
        print(
            "assert-drew: the fixture's glyphs are the only dark-on-light content in "
            "it, so this is the JNI text entry point, the face cascade or the "
            "committed MSDF sheet (issue #1100). This is NOT the no-GPU-device case, "
            "which fails above. NOTE: this floor is derived from a host render, not "
            "from a drawing device — no emulator that draws has been available. If "
            "the frame plainly shows text, re-derive it rather than chasing a "
            "regression.",
            file=sys.stderr,
        )
        return 3

    print(
        f"assert-drew: PASS — {len(seen)} distinct colour(s) and {inked:.2%} ink "
        f"{where}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
