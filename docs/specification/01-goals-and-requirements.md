# Goals and requirements

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §1

Requirement identifiers (`G1`-`G3`, `R1`-`R7`) are cited across the codebase and
are preserved verbatim. Several of `R1`-`R7` are not independently verifiable as
written; making them measurable is tracked separately and deliberately not done
in this pass.

Each requirement's proof lives in [05-qualification.md](05-qualification.md).

Goals:

    G1  Designers author in Figma; engineers author in code (Rust
        now, C# in-engine later); both produce the SAME document
        format and render identically.
    G2  Multiple render backends show the same pixels: Unity
        (product, lit/3D-capable), native (lean: far less memory
        and CPU than a game engine), Skia (reference/testing/2D),
        wasm (review).
    G3  Everything is testable: bit-exact goldens where possible,
        structural rect-table diffs everywhere, import-time
        diagnostics instead of runtime surprises.

Hard requirements:

    R1  Text: Middle-East scripts required (Arabic — shaping,
        ligatures, bidi/RTL). Perfect text quality. Identical text
        size and quality on every backend. High performance.
    R2  Layout: full Figma auto-layout vocabulary — all four modes
        (horizontal, vertical, wrap, GRID incl. spans), hug/fill/
        fixed sizing, min/max, gap, padding, alignment.
    R3  Backends: GPU is present on targets; GPU performance and
        memory bandwidth are the bottleneck. The native variant
        must consume far less memory and CPU than the engine
        backend. Backend selection is whole-scene, not per-node.
    R4  Animation must be reproducible in tests and have statically
        provable frame cost (no producer code in the frame loop).
    R5  Documents load fast: cold-start cost proportional to what
        is shown, not to file size (mmap + section discipline).
    R6  Unsupported design vocabulary is a named import diagnostic
        (warning/error), never a silent drop or runtime fallback.
    R7  Reproducible builds: same input → byte-identical document
        (hashing, signing, and CI depend on it).
