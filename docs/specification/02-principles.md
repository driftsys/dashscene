# Principles

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §3

`P1`-`P5` are binding on all downstream work (see `AGENTS.md`, "Principles").

P1 — The document carries intent, never results. No resolved
x/y/w/h, no rasterized pixels, no glyph positions. Anything
resolved would pin the document to one backend or font build.

P2 — One solver, one typesetter; painters only color. Layout
(Taffy) and text placement (rustybuzz) run exactly once, in shared
Rust. Every painter consumes finished rects and positioned glyph
runs. Cross-backend identity is structural, not tested-for.

P3 — Producers mutate, the runtime owns time. Producers commit
structure, props, and variant switches whenever they like; nothing
producer-side executes inside the frame loop. All animation is
descriptive data.

P4 — Vocabulary is validated, never discovered. Paint profiles are
checked at import/commit; every out-of-profile construct is a named
diagnostic, never a runtime surprise.

P5 — Figma compatibility is a property of one producer. The dashscene
document is a schema-first IR with its own spec; no producer's
limitations define the format.

P5's wording differs from the seed document, which named the IR "DSB". See
[dashscene-document-is-the-ir.md](../decisions/dashscene-document-is-the-ir.md),
which retires that name: the dashscene document is the IR, and `.dsb` is only
the extension of the flatbuffer that serializes it.
