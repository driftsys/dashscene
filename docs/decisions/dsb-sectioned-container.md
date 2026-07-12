# .dsb becomes a thin sectioned container; each section is one flatbuffer

    status   accepted (spike #56, 2026-07-12) — envelope lands with the
             v1 loading-performance work; binds schema design now
    scope    crates/dashbuf, the .dsb file format

## Context

`DESIGN_1.md` §5 (R5) requires hot sections (tree, tables, strings)
packed at the file head, cold sections (heavy decor) page-aligned at
the tail, and per-section hashes so the load gate verifies hot
sections without touching cold pages. Spike #56 measured how much of
that a single flatbuffer can express, against the exact toolchain this
repo pins: `flatc` 25.12.19 and the `flatbuffers` Rust crate 25.12.19.
Full evidence is on the issue
(<https://github.com/driftsys/dashscene-staging/issues/56>).

## Options

1. Keep `.dsb` a single flatbuffer; approximate the section discipline
   with write ordering and padding hacks.
2. A thin container: fixed envelope (magic, version, section table
   with per-section id, flags, offset, length, hash), one complete
   flatbuffer per section.
3. In-band composition inside one flatbuffer (`nested_flatbuffer` or
   size-prefixed concatenation).

## Choice

Option 2: `.dsb` is a thin sectioned container. Each section is an
independent flatbuffer with its own `root_type` and `file_identifier`.
The envelope is deferred to the v1 loading-performance work; until
then `.dsb` files stay single-flatbuffer. Two rules bind the schema
stories (#8, #13, #20, #26) immediately:

1. Cross-table references are integer indices, never flatbuffer
   offsets that reach into another future section.
2. Every section-destined table (node tree, layout, paint, variant,
   text/strings, heavy decor) stays one offset away from `Document`,
   so lifting it to a section root is mechanical.

## Why

- Content alignment inside a flatbuffer is capped at 32 bytes across
  the whole toolchain (`flatc` rejects `force_align` above 32;
  `FLATBUFFERS_MAX_ALIGNMENT` is 32 in the C++ runtime). Page
  alignment for cold sections is not expressible.
- The Rust builder exposes no alignment API at all, and vector-level
  `force_align` is silently ignored by Rust codegen (upstream
  documents it as C++-only). The producer side is Rust (`dashc`), so
  the C++ escape hatches do not apply.
- Vtable deduplication is automatic, global, and cannot be disabled: a
  hot table's vtable can physically live among cold bytes when shapes
  collide, which no schema discipline can prevent. A clean hot/cold
  page partition inside one buffer is therefore not guaranteeable even
  with perfect write ordering.
- The stock verifier walks everything reachable from the root and has
  no subtree scoping, so a hot-only load gate on a single buffer would
  need a custom partial verifier.
- FlatBuffers has no integrity layer; in a single buffer, sections
  have no stable byte ranges to hash. In the container, each hash
  covers a contiguous range any external signing tool can compute.
- Option 3 fails on the same grounds: `nested_flatbuffer` is a
  `[ubyte]` field inside the outer buffer (same alignment cap, Rust
  ignores its `force_align`), and size-prefixed concatenation gives no
  random access, alignment, or integrity.
- The container preserves `SCOPE_DECISIONS.md` §3 ("one schema for
  file and wire"): both roles frame the same per-section flatbuffers —
  the wire keeps length-prefixed framing, the file role uses the
  envelope. Cross-section references as indices cost nothing: the
  schema already uses indices (flattened DFS tree, doc index =
  rect-table index, paint indices per the boundary-B contract).
