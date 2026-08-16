#!/usr/bin/env python3
"""Exercises assert-drew.py beside it. Needs no device, no SDK and no NDK.

Every case here is a defect that reached `main`, not a hypothetical. The script
under test is the **only** witness that the Android painter drew anything, it is
reachable solely through `just android-splitscreen`, and that recipe runs on no
runner — so until this file existed the only check on it was reading it, and
reading it missed a black frame passing for months (issue #1029).

The images are synthesised here rather than committed: a PNG this file writes is
one whose every pixel is stated in the case that writes it, which is what makes a
failure name a cause. Committed screenshots would be opaque and would rot.

Frames are small. The checks that matter are about *fractions* of the height, so
a 480x300 frame exercises the same code paths as the emulator's 2560x1600 in a
fraction of the time — this file is run by CI and by the recipe itself.

    ./crates/dashscene-android/harness/assert-drew-test.py
"""

import importlib.util
import os
import struct
import subprocess
import sys
import tempfile
import zlib

# **Never byte-compile into the working tree.** `_load_module` below imports the
# script under test, and importing writes `__pycache__/`. `.gitignore` records
# PR #1098 committing a `.pyc` through exactly this path, and this is the first
# tracked script here that imports another — which would turn a one-off accident
# into routine dirt.
sys.dont_write_bytecode = True

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "assert-drew.py")

# The verdicts assert-drew.py documents, named so a failure reads as prose.
DREW, DID_NOT_DRAW, UNREADABLE, NO_TEXT = 0, 1, 2, 3

W, H = 480, 300
BLACK = (0, 0, 0)


def _load_module():
    """assert-drew.py, imported — for the decode checks that need its internals."""
    spec = importlib.util.spec_from_file_location("assert_drew", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)


def png(width, height, rows, interlace=0, filt=0, compression=0, filter_method=0,
        depth=8, colour=6, omit_ihdr=False):
    """Build an 8-bit RGBA PNG, applying scanline filter `filt` properly.

    `filt` may be an int, or a callable taking the row index — which is how the
    mixed-filter fixture is built. That case matters because the filter-0 fast
    path in the script keeps `prev` up to date for the *next* row, and only an
    image whose rows use different filters can tell whether it does.
    """
    stride = width * 4
    raw = bytearray()
    prev = bytearray(stride)
    for y in range(height):
        filt_y = filt(y) if callable(filt) else filt
        line = bytearray()
        for x in range(width):
            red, green, blue = rows(x, y)
            line += bytes((red, green, blue, 255))
        raw.append(filt_y)
        if filt_y == 0:
            raw += line
        else:
            encoded = bytearray(stride)
            for i in range(stride):
                a = line[i - 4] if i >= 4 else 0
                b = prev[i]
                c = prev[i - 4] if i >= 4 else 0
                if filt_y == 1:
                    encoded[i] = (line[i] - a) & 0xFF
                elif filt_y == 2:
                    encoded[i] = (line[i] - b) & 0xFF
                elif filt_y == 3:
                    encoded[i] = (line[i] - (a + b) // 2) & 0xFF
                else:
                    encoded[i] = (line[i] - _paeth(a, b, c)) & 0xFF
            raw += encoded
        prev = line

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(
        ">IIBBBBB", width, height, depth, colour, compression, filter_method, interlace
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + (b"" if omit_ihdr else chunk(b"IHDR", ihdr))
        + chunk(b"IDAT", zlib.compress(bytes(raw)))
        + chunk(b"IEND", b"")
    )


def write(data, name, tmp):
    path = os.path.join(tmp, name + ".png")
    with open(path, "wb") as handle:
        handle.write(data)
    return path


def run(data, name, tmp):
    done = subprocess.run(
        [sys.executable, SCRIPT, write(data, name, tmp)],
        capture_output=True,
        text=True,
    )
    return done.returncode, (done.stdout + done.stderr).strip()


def chrome_stripe(y0, y1):
    """Black everywhere except a colourful band — what system chrome looks like."""

    def rows(x, y):
        if y0 <= y < y1:
            return ((x * 7) % 256, (x * 13) % 256, (y * 11) % 256)
        return BLACK

    return rows


def drawn(height):
    """A light ground with antialiased dark ink in the middle band.

    Shaped like the fixture the harness ships — near-white ground, and the only
    dark content is glyph-like strokes with intermediate values at their edges.

    **It takes its height**, because closing over the module-level one made
    every short fixture a single flat colour: the ink band is a fraction of the
    height, and a 64-row image never reached a band computed from 300. Flat
    images also degenerate the Average and Paeth predictors, since a == b == c
    for every interior byte, which hides a class of reconstruction bug.
    """
    lo, hi = int(height * 0.45), int(height * 0.55)

    def rows(x, y):
        if lo <= y < hi:
            phase = x % 9
            if phase == 0:
                return (18 + (y % 6), 18, 24)
            if phase in (1, 8):
                # The antialiased edge, varying in x and y so the frame carries
                # the dozens of colours a real render has rather than a handful.
                shade = 70 + ((x * 3 + y) % 55)
                return (shade, shade, shade + 4)
        # A faint vertical wash, as a real background has — and what keeps the
        # positive case clear of MIN_DISTINCT by more than a rounding error.
        return (246 + (x % 10), 247 + (y % 8), 250)

    return rows


def light_over_black(fraction):
    """A colourful light wash over the top `fraction`, undrawn black below.

    The shape that passed review round two: the light majority and the ink were
    measured over disjoint regions, so up to half the client area could be
    unpainted and still report that the text drew.
    """

    def rows(x, y):
        if y < int(H * fraction):
            return (200 + (x % 56), 200 + (y % 56), 210 + ((x + y) % 45))
        return BLACK

    return rows


def flat_light(x, y):
    """Colour, but no ink: the background drew and the text did not."""
    return (232 + (x % 24), 236 + (y % 12), 244)


def main():
    cases = []
    with tempfile.TemporaryDirectory() as tmp:
        def case(name, want, data, key):
            cases.append((name, want, run(data, key, tmp)))

        big, small = drawn(H), drawn(64)

        # --- issue #1029 §1: a black frame must not pass -------------------
        #
        # The first two are real: they are the shapes of the two screenshots
        # `just android-splitscreen` took on 2026-08-16 against an emulator
        # whose painter never obtained a device. Both PASSED on `main`.
        case("black frame, gesture bar along the bottom", DID_NOT_DRAW,
             png(W, H, chrome_stripe(int(H * 0.94), H)), "gesturebar")
        case("black frame, multi-window caption at 12%-16%", DID_NOT_DRAW,
             png(W, H, chrome_stripe(int(H * 0.12), int(H * 0.16))), "caption")
        top_band, bottom_band = chrome_stripe(0, int(H * 0.10)), chrome_stripe(int(H * 0.94), H)
        case("black frame, chrome top AND bottom", DID_NOT_DRAW,
             png(W, H, lambda x, y: top_band(x, y) if y < int(H * 0.10)
                 else bottom_band(x, y)), "both")
        case("wholly black frame", DID_NOT_DRAW, png(W, H, lambda _x, _y: BLACK), "black")

        # **A chrome band INSIDE the surveyed region**, which no exclusion can
        # remove. The first revision of the ink check passed this — 829 colours
        # and 14883 "ink" pixels, all of them the black background — because ink
        # was counted without asking whether there was a ground to count it
        # against. It is the same false PASS as the two real frames above,
        # reintroduced by the fix for issue #1100 and caught in review.
        case("black frame, chrome band at 20%-24%", DID_NOT_DRAW,
             png(W, H, chrome_stripe(int(H * 0.20), int(H * 0.24))), "midband")

        # --- the frame that should pass ------------------------------------
        case("a drawn frame with ink on a light ground", DREW, png(W, H, big), "drawn")

        # --- issue #1100: colour without glyphs ----------------------------
        #
        # The background drew and the text did not. A colour count alone cannot
        # tell this from the case above, which is the whole of that issue. It is
        # exit 3, not 1: the remedy is the text path, not the GPU device.
        case("background drew, no glyphs", NO_TEXT, png(W, H, flat_light), "noglyphs")

        # **The ink ceiling, at and around its boundary.** Ink is "darker than
        # luma 128", so undrawn area counts as ink — a floor alone credits a
        # black region as text. 62% light over black passed review round two
        # with "59.5% light and 6210 ink pixels", every one of them the black.
        # Nothing in the suite then sat between "almost all black" and "almost
        # all light", which is why that gap survived.
        case("light wash over 38% undrawn black", DID_NOT_DRAW,
             png(W, H, light_over_black(0.62)), "lightoverblack")
        case("light wash over 25% undrawn black", DID_NOT_DRAW,
             png(W, H, light_over_black(0.75)), "lob75")
        # Just past the ceiling: the client area is rows 18%..92%, so a black
        # region starting at 84% of the height is 10.8% of it.
        case("light wash, black just past the ceiling", DID_NOT_DRAW,
             png(W, H, light_over_black(0.84)), "lob84")
        # And just inside it. **This is a declared tolerance, not an oversight**:
        # a ceiling admits everything below it, so a small undrawn strip passes.
        # Tightening it towards the fixture's own 0.94% would start failing a
        # device render, whose glyphs are larger. The undrawn region that
        # actually matters is the other pane in multi-window, which no fraction
        # of the display can bound — that is issue #1191.
        case("light wash, a 4% undrawn strip is tolerated", DREW,
             png(W, H, light_over_black(0.89)), "lob89")

        # --- issue #1029 §2 and §3: unreadable is 2, never 1 ----------------
        good = png(W, 64, small)
        case("truncated IDAT", UNREADABLE, good[: len(good) // 2], "truncated")
        case("truncated inside a chunk header", UNREADABLE, good[:10], "cuthead")
        case("not a PNG at all", UNREADABLE, b"not a png", "notpng")
        case("IHDR claims more rows than IDAT carries", UNREADABLE,
             good.replace(struct.pack(">II", W, 64),
                          struct.pack(">II", W, 4096), 1), "shortidat")
        case("Adam7 interlaced", UNREADABLE, png(64, 64, small, interlace=1), "interlaced")
        case("unsupported filter method", UNREADABLE,
             png(64, 64, small, filter_method=3), "badmethod")
        case("unsupported compression method", UNREADABLE,
             png(64, 64, small, compression=1), "badcompression")
        case("16-bit depth", UNREADABLE, png(64, 64, small, depth=16), "depth16")
        case("greyscale colour type", UNREADABLE, png(64, 64, small, colour=0), "grey")
        case("no IHDR chunk", UNREADABLE, png(64, 64, small, omit_ihdr=True), "noihdr")
        # Zero sampled pixels is "cannot judge", not "drew nothing" — it used
        # to report the no-GPU-device diagnosis for a degenerate image.
        case("a frame with no client area at all", UNREADABLE,
             png(0, 0, small), "empty")

        # A filter byte outside 0..4 used to be applied as filter 0, which
        # desynchronises the decode and reads as "many colours" — a pass.
        case("scanline filter byte out of range", UNREADABLE,
             _with_filter_byte(png(64, 64, small), 7), "badfilter")

        # --- the filter-0 fast path decodes identically (issue #1029 §4) ----
        #
        # Hoisting filter 0 out of the per-byte loop is the performance fix, and
        # its whole claim is that it "changes no decoded byte". These encode the
        # same image under each filter and compare the decoded buffers.
        #
        # **The mixed case is the one that tests the fast path's `prev` update**,
        # and without it that line can be deleted with every case still green:
        # a uniformly-filtered image never has a filter-0 row followed by a
        # filtered row, which is the only place `prev` is read back.
        # Guarded, so a decode that raises — which is the regression these
        # exist to catch — names its case instead of aborting the run before
        # anything is reported.
        module = _load_module()
        tall = drawn(96)

        def decodes_like(name, reference, image, key):
            try:
                cases.append((name, True,
                              (module.read_png_rgba(write(image, key, tmp)) == reference,
                               "")))
            except Exception as error:  # noqa: BLE001 - any raise is the finding
                cases.append((name, True, (f"raised {type(error).__name__}: {error}", "")))

        reference = module.read_png_rgba(write(png(W, 96, tall), "f0", tmp))
        for filt in (1, 2, 3, 4):
            decodes_like(f"filter {filt} decodes byte-identically to filter 0",
                         reference, png(W, 96, tall, filt=filt), f"f{filt}")
        decodes_like("filters mixed per scanline decode identically", reference,
                     png(W, 96, tall, filt=lambda y: (0, 2, 0, 4, 1, 0, 3)[y % 7]),
                     "fmixed")

    failed = 0
    for name, want, (got, output) in cases:
        if got == want:
            print(f"  ok   {name:52s} {got}")
        else:
            failed += 1
            print(f"  FAIL {name:52s} expected {want}, got {got}")
            if output:
                print(f"       {output.splitlines()[0]}")

    print()
    if failed:
        print(f"assert-drew-test: {failed} case(s) failed")
        return 1
    print(f"assert-drew-test: all {len(cases)} cases held")
    return 0


def _with_filter_byte(data, value):
    """Re-emit `data` with every scanline's filter byte replaced by `value`."""
    start = data.index(b"IDAT") + 4
    length = struct.unpack(">I", data[start - 8 : start - 4])[0]
    raw = bytearray(zlib.decompress(data[start : start + length]))
    width = struct.unpack(">I", data[16:20])[0]
    stride = width * 4
    for offset in range(0, len(raw), 1 + stride):
        raw[offset] = value
    body = zlib.compress(bytes(raw))
    head = data[: start - 8]
    tail = data[start + length + 4 :]
    return (
        head
        + struct.pack(">I", len(body))
        + b"IDAT"
        + body
        + struct.pack(">I", zlib.crc32(b"IDAT" + body) & 0xFFFFFFFF)
        + tail
    )


if __name__ == "__main__":
    sys.exit(main())
