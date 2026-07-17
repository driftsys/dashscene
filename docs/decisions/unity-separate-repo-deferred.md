# Unity: separate repo, C#, deferred until v0 exits

    status   accepted
    date     2026-07-11
    scope    the Unity painter and its producer front end; crates/dashscene-unity

## Context

Unity work can't live in the Cargo workspace — different language and
toolchain entirely (C#, Unity project format or a UPM package). Two
distinct pieces belong together, both C#, both in one Unity repo/package:

- **Producer front end** (`docs/design/architecture.md`, "C# decl.
  (v1)") — a C# declarative DSL running in-engine, builds a describe
  buffer, one commit across the FFI seam (no per-prop FFI; struct/Span,
  pooled, GC-free; typed keys via codegen).
- **Painter back end** (`docs/technotes/rendering-and-painters.md`) —
  the renderer: rect table + glyph runs consumed over FFI, projected
  onto pre-instantiated GameObjects, paint entries resolved to
  SDF-shader-library materials (lit-opaque / lit-cutout / unlit-overlay).

## Choice

Do not create the Unity repo yet. Per `docs/roadmap.md`'s plan, Unity
work doesn't start until v1, after the v0 exit criteria (E1-E7, which
are Rust+Skia only). The Rust-side `dashscene-unity` crate (already
reserved, `docs/decisions/crate-name-map.md`) becomes the thin
FFI-bindings crate the C# side links against, not the Unity project
itself.

## Why

- The default is to defer repo creation until v0 actually exits, rather
  than stand up empty scaffolding for work that can't start yet.
- The only coupling between the Unity repo and this one is a narrow,
  versioned FFI wire protocol — a repo split costs nothing
  architecturally, unlike the Figma importer's direct dependency on
  `dashc.wasm`
  (`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`).

## Consequences

- Revisit only if the `dashscene-unity` name needs reserving earlier to
  prevent squatting — it is already reserved, so this has not
  triggered.
