# The envelope is read from a prefix by a host-side reader, not by relaxing `Container::parse`

    status   open — the choice is stated and argued, and is not yet
             implemented. Raised at the v0.14 plan revision (2026-08-02)
             because two stories in two slices had each planned to change the
             same line without knowing about the other.
    scope    `crates/dashbuf/src/container.rs`; stories #587 (v0.15, the web
             target) and #595 (v0.16, map the file); binds any future producer
             that reads a `.dsb` without holding all of it.
    builds on `docs/decisions/dsb-sectioned-container.md` (the envelope's
             shape and why it is not a flatbuffer)

## Context

`Container::parse` bounds-checks every section's declared extent against the
length of the slice it is given, and raises `SectionOutOfRange` when an extent
runs past the end. It never reads those bytes; it only checks their extents.

That one check costs nothing on one target and everything on another, which is
what makes it a decision rather than an implementation detail:

- **Under `mmap`** the mapping is full-length and the pages are never touched,
  so the check is free. This is what story #595 wants, and
  `dsb-sectioned-container.md` already specifies: "One `mmap` of the whole
  file, once. The envelope is read through the mapping."
- **In wasm** there is no mapping. The same check forces the entire file into
  linear memory before the envelope can be read at all — which is the opposite
  of what story #587 needs, since the point there is to fetch a prefix, read
  the envelope, and fetch the rest lazily.

Story #587 put it exactly: "One bounds check, opposite cost on the two
targets." Story #595 calls the same line "the one obstacle, and it is one
line". Neither referenced the other; both were written to change it.

## Choice

**Leave `Container::parse` strict, and read the envelope from a prefix with a
separate host-side reader.**

The envelope is deliberately not a flatbuffer. `dsb-sectioned-container.md`
gives the reason: checking it "must be plain bounds/magic/version comparisons
on fixed offsets". It is a fixed 64-byte header, a fixed 64-byte section
stride, no implicit padding, little-endian throughout. A reader for that is
small, and BLAKE3 for the table hash is its only dependency.

The section layout cooperates. `parse` enforces ascending, non-overlapping
sections with no structured section after a blob, so a file is strictly
`header | table | structured… | blobs…` and the hot prefix is one contiguous
run from offset zero.

## Why not the alternative

The alternative is a prefix-tolerant parse mode: treat a declared extent beyond
the slice as "not resident yet" rather than an error.

It is rejected because the strictness is the check's purpose. `parse` exists so
that a malformed or truncated file is named at the gate rather than discovered
by a painter, and a mode that accepts a short slice is one flag away from
becoming the default path — at which point a genuinely truncated file loads
clean and fails somewhere else. Story #587 identified this risk itself and
preferred the reader for the same reason.

Keeping the two readers separate also keeps their failure modes separate: the
strict parse answers "is this whole file consistent", and the prefix reader
answers "can I see enough to know what to fetch next". Those are different
questions, and one function answering both with a flag is how the answer to
each stops being trustworthy.

## Consequences

- Story #587 builds the prefix reader. Story #595's mmap work needs no change
  to `parse` at all, which is a smaller change than that story currently
  assumes.
- The two readers must agree about the envelope's layout, and nothing yet
  forces them to. Whichever story lands second should add a test that parses
  one fixture both ways and asserts the same section table — otherwise the
  duplication is real and unguarded.
- A third consumer wanting a prefix — a network loader, a future streaming
  transport — uses the same reader rather than adding a second mode to `parse`.

## Status of this record

Nothing is implemented. This record exists so the choice is made once, in one
place, rather than settled by whichever of #587 and #595 happens to run first.
It should move to `accepted` when the first of them lands the reader, and gain
its as-built shape then.
