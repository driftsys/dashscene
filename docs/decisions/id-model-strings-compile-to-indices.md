# Id model: source strings compile to dense indices; hashes and handles by role

    status   accepted (design session, 2026-07-12) — binds schema
             stories (#8, #20) and the future wire work; the exports
             table and codegen bindings land with the stories that
             need them
    scope    crates/dashbuf schema, dashscene-core producer API, dashc

## Context

Sources name things with strings: Figma node ids and layer names,
dashlang bindings, variant names, design-token names, asset paths. The
file and the frame loop want dense integers; remoting wants stable
addressing across structural change; assets want identity that
survives across documents and machines. The question was which id kind
each concept gets, who allocates it, and what happens to the source
strings.

## The model

One rule with three allocation modes: **strings are an authoring and
late-binding concept; the file and the frame loop speak dense
integers; identity that must hold across documents or machines is a
content hash.**

    id kind              in source          in file            forged by
    -------------------  -----------------  -----------------  --------------------
    node address         layer/DSL name     u32 doc index      flattener (DFS pos)
    producer handle      DSL binding        not in file        arena counter/session
    exported node name   annotated string   interned + u32     compiler, opt-in
    asset id             path / image ref   content hash       the payload bytes
    string/style/token   literal string     u32 pool index     interner (visit order)
    variant              name ("dark")      u32 variant index  compiler
    property (set_prop)  field name         u16 schema enum    the schema

- **Positional ids are derived, not allocated**: the flattener assigns
  doc indices by canonical DFS order. They are valid within one
  generation of one document — snapshots speak indices, deltas speak
  handles (see `remoting-two-transports.md`).
- **Handles are session-scoped counters, never persisted**: on load,
  handle = initial doc index; created nodes get the next counter
  value. Handles exist so a producer can address its node across
  structural commits that shuffle indices. **The producer API must
  keep handles distinct from doc indices** — this is the rule that
  binds current core work.
- **Content hashes are self-forging** (see the asset-model record):
  no allocator, no registry, automatic dedup.

Source strings survive in exactly two places:

1. **Debug names** — `Node.name`, interned, tooling and diagnostics
   only; a release profile can strip them because nothing addresses
   through them.
2. **An opt-in exports table** — `{name → doc index}` for late
   binding: an app process driving Figma-compiled UI addresses "the
   SpeedLabel node" without having built the tree. Export is annotated
   (via the sharedPluginData plugin), not scraped from layer names —
   Figma layer names are not unique; export names get
   validator-enforced uniqueness at compile time (P4: validated
   vocabulary, never discovered). The same table pattern serves
   variants and design tokens.

String resolution happens once, at bind time; every subsequent
mutation is pure integers. No string crosses the wire in a delta or is
compared inside the frame loop (P3).

## Alternatives considered

- **String ids at runtime** — rejected: per-frame string comparison
  and hashing violate the frame-loop discipline, and interning already
  provides the compact form.
- **Positional ids everywhere (no handles)** — rejected: one
  structural insert shifts every later index, so positional deltas
  degenerate into resends; remoting becomes impractical.
- **Per-node UUIDs in the file** — rejected: 16 bytes per node buys
  nothing over index + opt-in exports; global uniqueness is only
  needed for assets, where the content hash provides it with meaning.

## Consequences

- **Determinism (R7) dictates the allocators**: index and intern order
  must be a pure function of the canonical input traversal —
  first-visit order in a canonical DFS, no hash-map iteration order
  anywhere in `dashc` — so the same input produces a byte-identical
  `.dsb` and stable goldens.
- **Codegen bindings are the intended endgame**: `dashc` can emit a
  companion constants module (Rust/Kotlin/TS) for exports, variants,
  and tokens next to the `.dsb`, making a misspelled name a compile
  error instead of a wire diagnostic.
