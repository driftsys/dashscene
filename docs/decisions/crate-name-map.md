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

## `dashpack`, added at the v0.12 open (story #429, 2026-07-26)

    dashpack             asset packer — encodes canonical payloads into
                         per-profile derivations (RAW/HiFi/Lite), assembles
                         cold banks onto the sectioned container, and records
                         every choice in the derivation manifest

A fourteenth crate, and the fourth name that was not among the 12
reserved. It is added here rather than at the original mapping because
the packer was not a v0 crate: the asset pipeline that needs it was
designed in `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`
and scheduled to v0.12.

**Why it is a workspace crate and not a separate repo.** The recorded bar
for a separate repo is toolchain incompatibility — the reason the Unity
work got its own (`docs/decisions/unity-separate-repo-deferred.md`) — and
the packer is plain cargo. Its coupling runs the other way: it compiles
against `dashbuf`'s asset and manifest schemas, its band oracle reuses
`goldens`' oracle, and its weld and profile-preview tests span packer
output and the reference painter. The workspace already absorbs a heavier
build than a vendored astcenc `-sys` crate, in `skia-bindings`.

The requirement it has to satisfy is that the packer is a **standalone
tool** (user requirement, 2026-07-19: no external CLIs anywhere in the
pipeline). That is met by the binary artifact, `cargo build -p dashpack`,
not by repo ownership.

**Extraction bar, recorded now so it is not re-argued.** Revisit a
separate repo only if an external consumer needs the source tree, not
merely the binary. Publishing `dashpack` as its own crate happens at the
staging-to-public promotion regardless, like every other crate here.

**Availability.** Unlike the other three new names, `dashpack` is not
reserved on crates.io. Nothing here is published yet
(`docs/decisions/repo-staging-and-public-facade.md`), so the reservation
belongs with the promotion rather than with this story — but the name can
be squatted out from under the project in the meantime, which is the same
exposure the original three had before they were reserved.

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
