# Design: text nodes — strings + style refs in dashbuf and dashscene-core (#26)

    status   working memory (Superpowers spec) — gardened on story finish
    story    #26 (epic #24, v0.5 — text I: Latin)
    date     2026-07-12
    traces   DESIGN_1.md §5 (text table: strings + style refs, interned
             strings, dedup style pool; never glyph positions), P1
             (intent, never results), R7 (append-only schema evolution),
             docs/design/dashbuf.md, docs/design/dashscene-core-arena.md
    blocks   #28 (typeset Latin), then #29/#30

## Purpose

Give the document format and the semantic model a text vocabulary:
what a text node says (a string) and how it is styled (font family,
size, weight, color) — and nothing the runtime computes (no glyph
positions, no line breaks, no measured sizes; P1).

## Approach A — schema shape (dashbuf)

**Chosen: two new document-level pools plus two sentinel-indexed node
fields, all append-only (R7):**

    Document.strings:      [string]      interned string pool
    Document.text_styles:  [TextStyle]   dedup style pool
    Node.text:       uint32 = MAX        index into strings; MAX = the
                                         node has no text
    Node.text_style: uint32 = MAX        index into text_styles; MAX =
                                         unstyled (validator territory
                                         later if text is present)

    TextStyle (table):
      family:  string (required)         font family name — the
                                         verifier rejects a family-less
                                         style at the load gate (P4,
                                         same mechanism as Gradient's
                                         required fields)
      size_px: float32                   em size in document units
      weight:  ushort = 400              CSS-scale 100..900
      color:   Color                     same struct paint uses

Dedup policy is the producer's job (the pools make dedup possible;
nothing in the schema forces it) — same posture as `Document.paints`.

Alternatives considered:

- _Inline `Node.text: string`_ — simpler today, but retrofitting the
  DESIGN §5 interning later means a dead field and a second text field
  (append-only ids); the pool costs one indirection now and no schema
  churn later. Rejected.
- _Text as a node-kind union_ — restructures `Node` for no v0.5 gain;
  DESIGN §5 models text as node content (strings + style refs), not a
  parallel node array. Rejected.
- _`family` as a strings-pool index_ — loses the verifier-enforced
  presence check; families repeat little once styles are pooled.
  Rejected.

## Approach B — core storage and mutation (dashscene-core)

**Chosen: mirror the schema shapes as plain Rust types (no dashbuf
dependency, same as #2), extend `Prop`, add intent-side accessors,
leave the committed output untouched.**

    TextStyle { family: String, size_px: f32, weight: u16, color: Color }

    Prop::Text(String)          set/replace the node's text content
    Prop::TextStyle(TextStyle)  set/replace the node's style

    Arena::text_of(NodeId) -> Option<&str>
    Arena::text_style_of(NodeId) -> Option<&TextStyle>

The accessors read the intent model, so staged (uncommitted) values
are visible immediately — unlike `committed()`. That is the seam #28's
standalone typeset pipeline and #29's measure callback will read from;
they are documented as intent-side on purpose.

The commit pipeline does not change: text does not influence the v0.5
rect table (text-driven hug sizing arrives with #29), the committed
output carries no glyph data (P1 — boundary B gains positioned glyph
runs at #28/#30), and a text-only change therefore produces no dirty
entry yet.

Alternatives considered:

- _Intern styles into a committed style table at commit (paint-pool
  precedent)_ — no consumer exists until #28/#29 define what crosses
  the boundary; building the table now is speculative. Deferred with
  the seam documented. Rejected for this story.
- _Store text in the committed output now_ — same reason. Rejected.

## Components

    crates/dashbuf/schema/dashbuf.fbs   append TextStyle table, the two
                                        Document pools, the two Node
                                        fields
    crates/dashbuf/tests/text_roundtrip.rs
                                        per-construct round-trip tests
    crates/dashscene-core/src/arena.rs  NodeData text fields, Prop
                                        variants, accessors
    crates/dashscene-core/src/lib.rs    re-export TextStyle
    crates/dashscene-core/tests/arena.rs
                                        text prop/accessor cases appended

## Testing

- dashbuf round-trips (one focused test per construct, matching
  `paint_roundtrip.rs`'s style): a text node referencing pool entries;
  two nodes sharing one string index (interning is representable); the
  no-text and no-style sentinels as defaults; `weight` default 400;
  a `TextStyle` without `family` is rejected by the flatbuffer
  verifier.
- core: `Prop::Text`/`Prop::TextStyle` set and read back via the
  accessors; staged values visible before commit (documented intent
  semantics); replacing text/style overwrites; accessors return `None`
  for nodes without text; out-of-range `NodeId` panics (existing
  contract).

## Out of scope (this story)

Shaping, line breaking, run caching (#28); measure callback and hug
sizing (#29); glyph-run committed output and painting (#30); charset
coverage (#34); text validation diagnostics (validator slice); string
pool deduplication enforcement (producer concern).
