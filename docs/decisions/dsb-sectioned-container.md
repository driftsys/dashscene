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

- Page alignment for cold sections is not expressible through any
  supported route for a Rust producer. `flatc` caps struct
  `force_align` at 32; vector `force_align` above 32 parses, but only
  C++ codegen honors it (upstream documents the attribute as
  C++-only, and the C++ runtime asserts `alignment <=
  FLATBUFFERS_MAX_ALIGNMENT` (32) in debug builds). Rust codegen
  silently ignores vector `force_align`, and `FlatBufferBuilder` in
  the Rust crate has no direct alignment method (no
  `ForceVectorAlignment`/`PreAlign` equivalent). The public
  `Push`/`PushAlignment` traits are uncapped, but the vector
  constructors do not take a custom alignment, so page alignment is
  reachable only through unsupported custom-`Push` workarounds. The
  producer side is Rust (`dashc`), so the C++-only routes do not
  apply.
- In the Rust builder, vtable deduplication is unconditional: a hot
  table's vtable can physically live among cold bytes when shapes
  collide, which no schema discipline can prevent. C++ exposes
  `DedupVtables(bool)`; the Rust crate has no equivalent. A clean
  hot/cold page partition inside one Rust-built buffer is therefore
  not guaranteeable even with perfect write ordering.
- The verifier's supported entry points walk everything reachable
  from the root, and `VerifierOptions` has no scoping option. A
  hot-only load gate on a single buffer would have to be hand-composed
  from the crate's lower-level verifier primitives
  (`Verifiable::run_verifier`), which upstream documents as not meant
  to be called directly.
- FlatBuffers has no integrity layer; in a single buffer, sections
  have no stable byte ranges to hash. In the container, each hash
  covers a contiguous range any external signing tool can compute.
- Option 3 fails for the same reasons: `nested_flatbuffer` is a
  `[ubyte]` field inside the outer buffer (same alignment situation,
  Rust ignores its `force_align`), and size-prefixed concatenation
  gives no random access, alignment, or integrity.
- The container preserves `SCOPE_DECISIONS.md` §3 ("one schema for
  file and wire"): both roles frame the same per-section flatbuffers —
  the wire keeps length-prefixed framing, the file role uses the
  envelope. Cross-section references as indices cost little: the
  design already prescribes indices (flattened DFS tree, doc index =
  rect-table index, a u32 paint index in the boundary-B rect entry).
  The one v0.1 deviation is `Node.paint`, an inline table today; story
  #13 lifts it into a `Document`-level paint table plus index, per
  rule 1 above.
