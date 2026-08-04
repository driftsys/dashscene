#!/usr/bin/env python3
"""A static file server that implements HTTP `Range`.

`python3 -m http.server` does not. Without ranges the browser host still draws —
it notices the whole file arrived and reads the envelope out of what it holds —
but the thing story #587 exists to demonstrate, fetching a prefix and then only
the payloads a document names, never happens. So the demonstration ships with a
server that can show it.

Deliberately small and deliberately local: it serves one directory over
loopback, and it is a development tool rather than anything the project ships.

    python3 demo-web/serve.py <directory> [port]
"""

from __future__ import annotations

import functools
import os
import re
import sys
from http.server import HTTPServer, SimpleHTTPRequestHandler

# `bytes=<first>-<last>`, where either end may be absent:
#   bytes=0-63    the first 64 bytes
#   bytes=64-     everything from 64 on
#   bytes=-64     the last 64 bytes (a suffix range)
RANGE = re.compile(r"^bytes=(\d*)-(\d*)$")


class RangeHandler(SimpleHTTPRequestHandler):
    """`SimpleHTTPRequestHandler` that answers a single byte range."""

    protocol_version = "HTTP/1.1"

    def send_head(self):
        header = self.headers.get("Range")
        if header is None:
            return super().send_head()

        parsed = RANGE.match(header.strip())
        if parsed is None:
            # A range this server does not implement — a multi-range request,
            # or a unit that is not bytes. Answering the whole file is allowed
            # and is what a server without range support would do anyway.
            return super().send_head()

        path = self.translate_path(self.path)
        if os.path.isdir(path):
            return super().send_head()
        try:
            handle = open(path, "rb")
        except OSError:
            self.send_error(404, "File not found")
            return None

        total = os.fstat(handle.fileno()).st_size
        first, last = parsed.group(1), parsed.group(2)
        if first == "":
            # A suffix range: the last N bytes.
            length = int(last or 0)
            start = max(0, total - length)
            end = total - 1
        else:
            start = int(first)
            end = int(last) if last else total - 1
            end = min(end, total - 1)

        if start > end or start >= total:
            handle.close()
            self.send_response(416, "Requested Range Not Satisfiable")
            # The total is still authoritative here, and it is exactly what
            # tells a client the file is shorter than the range it asked for.
            self.send_header("Content-Range", f"bytes */{total}")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return None

        handle.seek(start)
        self.send_response(206, "Partial Content")
        self.send_header("Content-Type", self.guess_type(path))
        self.send_header("Content-Range", f"bytes {start}-{end}/{total}")
        self.send_header("Content-Length", str(end - start + 1))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        # `copyfile` sends to the end of the file, so hand it only the slice.
        return _Slice(handle, end - start + 1)

    def end_headers(self):
        # Never cache. This serves a build directory that is rewritten every
        # time `just web-build` runs, and a browser holding the previous
        # `demo_web_bg.wasm` shows the previous host with no sign that it is
        # doing so — the most confusing failure this server could produce, and
        # the one a person would waste the most time on. A development server
        # has nothing to gain from caching.
        self.send_header("Cache-Control", "no-store, must-revalidate")
        # WebGPU is a secure context, which loopback already satisfies. These
        # two are what a browser additionally wants before it will hand a page
        # `SharedArrayBuffer`; harmless here and one less thing to discover
        # later if this host ever moves to a worker.
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()


class _Slice:
    """A read-only view of `handle` that ends after `remaining` bytes."""

    def __init__(self, handle, remaining: int):
        self._handle = handle
        self._remaining = remaining

    def read(self, size: int = -1) -> bytes:
        if self._remaining <= 0:
            return b""
        if size is None or size < 0:
            size = self._remaining
        chunk = self._handle.read(min(size, self._remaining))
        self._remaining -= len(chunk)
        return chunk

    def close(self) -> None:
        self._handle.close()


def main(argv: list[str]) -> int:
    directory = argv[1] if len(argv) > 1 else "."
    port = int(argv[2]) if len(argv) > 2 else 8787
    if not os.path.isdir(directory):
        print(f"serve: {directory} is not a directory", file=sys.stderr)
        return 1

    handler = functools.partial(RangeHandler, directory=directory)
    # Loopback only. This serves a directory with no authentication, and
    # binding it to every interface would put that on the network.
    server = HTTPServer(("127.0.0.1", port), handler)
    print(f"serve: http://127.0.0.1:{port}/ from {os.path.abspath(directory)}")
    print("serve: ranges are honoured; ^C to stop")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
