# The envelope is read from a prefix by a host-side reader, not by relaxing `Container::parse`

    status   accepted — built by story #587 as `dashbuf::prefix`. Raised at the
             v0.14 plan revision (2026-08-02) because two stories in two slices
             had each planned to change the same line without knowing about the
             other.
    scope    `crates/dashbuf/src/container.rs`, `crates/dashbuf/src/prefix.rs`;
             stories #587 (v0.15, the web target) and #595 (v0.16, map the
             file); binds any future producer that reads a `.dsb` without
             holding all of it.
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
- A third consumer wanting a prefix — a network loader, a future streaming
  transport — uses the same reader rather than adding a second mode to `parse`.

## As built

`dashbuf::prefix::Envelope::read(prefix, file_len)`, landed by story #587.
Three things came out differently from the sketch above, and the differences
matter to #595.

**The two readers share every rule rather than duplicating them.** This record
expected duplication and asked the second story to land a test guarding it.
Instead the header checks, the table-extent arithmetic and the table walk are
one implementation each — `Header::check`, `Header::table_end` and
`check_table` in `container.rs` — that both entry points call. The two remain
separate functions answering separate questions, which is what the choice above
argued for; what they do not have is two copies of the rules. The guard test
exists anyway, in `crates/dashbuf/tests/prefix.rs`, and it now catches a rule
added to one entry point rather than to the shared walk.

**The reader is told the file's length, and applies the bounds check.** The
sketch above framed the strict bounds check as the thing a prefix reader cannot
do. That was wrong in a way only a test showed: a host reading a prefix always
knows the file's total length without holding it — an HTTP range response
states it in `Content-Range`, a local file states it outright — so the check
becomes possible, and skipping it costs real safety. A section count of
`u32::MAX` describes a 256 GiB section table; with no length to check it
against, the reader answers "fetch 256 GiB" on a 64-bit host and "overflow" on
a 32-bit one, so the behaviour would have depended on the target, and wasm is
the 32-bit one.

So the difference between the two readers is not a rule. It is one number: the
strict reader takes the file's length from the slice it holds, and the prefix
reader is told. A truncated or malformed file is still named at the gate, which
is what "the strictness is the check's purpose" asked for.

**`NeedMore` is not an error.** Being short is the ordinary state of a prefix,
so `PrefixError` separates `NeedMore { need }` — fetch this much and ask again,
at most twice — from `Malformed(ContainerError)`, which carries the strict
reader's own diagnostic unchanged. A host that treats the two alike turns a
first request into a failure.

One thing the prefix reader cannot do, and callers must: `Container::blob_by_hash`
hashes a payload before handing it over, and `Envelope::blob_by_hash` returns a
byte range and holds nothing. Checking a fetched payload against the table is
the host's own step.
