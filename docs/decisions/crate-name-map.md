# Crate naming: reuse the 12 already-reserved crates.io names

    status   accepted
    date     2026-07-11
    scope    the 13-crate Cargo workspace

## Context

12 crate names were reserved on crates.io earlier (published 2026-03-18,
one placeholder version each, not real releases): `dashscene`,
`dashscene-core`, `dashscene-engine`, `dashscene-compose`,
`dashscene-unity`, `dashscene-web`, `dashscore`, `dashlang`, `dashc`,
`dashcue`, `dashpaint`, `dashbuf`. The question was whether to build the
workspace against these names, and how to map each onto the
architecture in `docs/design/architecture.md`.

## Choice

Reuse all 12, mapped onto the roles in `docs/design/architecture.md`:

    reserved name       role
    -------------------  ------------------------------------------------
    dashscene            umbrella crate — facade / public API surface
    dashscene-core        arena, node tree, layout tables, paint tables —
                          the semantic model — AND the staged
                          producer-mutation API (open/set_prop/
                          set_variant/commit)
    dashscene-engine      Taffy solve, variants, FLIP, measure callback —
                          runtime that resolves the model
    dashc                 compiler: Figma importer orchestration target,
                          lowering, diagnostics, .dsb emission — also
                          built to wasm32 for the Deno importer to call
                          into directly
    dashbuf               the flatbuffer schema itself — document format,
                          sections, hashes; also names the .dsb file
                          extension
    dashpaint             paint table (fill/stroke/effect params, token
                          refs, material class) + the painter trait,
                          boundary B
    dashcue               descriptive animation vocabulary + its runtime
                          scheduling — variant transitions, FLIP
                          triggers, springs, keyframes, loop tracks,
                          enter/exit; NOT the staged-mutation API, which
                          is dashscene-core's
    dashlang              Rust DSL skin (v0) and future typed skins over
                          the one producer surface
    dashscene-unity        Rust-side FFI bindings for the Unity painter;
                          the Unity/C# work itself lives in a separate
                          repo
    dashscene-web          wasm/tiny-skia painter, parked
    dashscore              parked — an authoring IDE, not in scope
    dashscene-compose      parked — Android Jetpack Compose backend, not
                          a target

Three crates the architecture needs had no reserved name at the time:
typesetting (bidi/shaping/atlas), the Skia reference painter (the entire
v0 painter), and the shared validator (profiles/diagnostics/waivers).
Names chosen and confirmed available on crates.io: `dashscene-typeset`,
`dashscene-skia`, `dashscene-validator`.

## Why

- `dashscene-typeset` was chosen over `dashscene-text` (too generic — the
  role is "one typesetter", not just text) and over `dashscene-type`
  (collides with the Rust ecosystem's `*-type`/`*-types` convention for
  shared type-definition crates).
- `dashscore`, `dashlang`, and `dashscene-compose` carried the most
  interpretive risk in this mapping: `dashscore`/`dashscene-compose` are
  treated as unused/parked (no equivalent in the architecture), and
  `dashlang` is treated as "the DSL family" rather than a literal new
  declarative language.

## Consequences

- The three new names (`dashscene-typeset`, `dashscene-skia`,
  `dashscene-validator`) needed reserving on crates.io before they could
  be squatted out from under the project.
- The staged-mutation API's assignment to `dashscene-core` (not
  `dashcue`) is elaborated in `docs/decisions/staged-mutation-v01-scope.md`.
