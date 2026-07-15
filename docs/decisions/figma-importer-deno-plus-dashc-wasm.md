# Figma importer: Deno/TypeScript, calling `dashc.wasm`, same repo as the Rust core

    status   accepted
    date     2026-07-11
    scope    importers/figma/, crates/dashc

## Context

`docs/design/architecture.md` (Stage 1) and `docs/design/dashc.md` call
for a Figma importer but do not fix its implementation language. The
importer needs native HTTP/auth/JSON
strengths on one side, and the same Figma≠CSS lowering, validation, and
`.dsb` emission code `dashc` already owns on the other.

## Options

1. A pure Rust binary implementing the whole importer, including the
   Figma REST client.
2. Deno/TypeScript owning the REST/auth/JSON side, calling into
   `dashc` compiled to `wasm32-unknown-unknown` for lowering,
   validation, and emission.
3. As option 2, but in a separate repo from the Rust workspace.

## Choice

Option 2, in the same repo as the Rust core.

**Deno owns** (native TS strengths — HTTP, auth, JSON shaping): REST
fetch against `@figma/rest-api-spec`'s official TS types, personal-
access-token rotation, root-frame declarations, reachability closure
across files, variant-set closure, trim layers (root scoping,
slot-child replacement, `_`-prefix sugar, sharedPluginData role reads),
token phase 1/2 join. It also hosts the small Figma annotator plugin
that writes sharedPluginData roles
(`docs/decisions/annotator-plugin-contract-frozen.md`) — that plugin has
to run inside Figma's own plugin sandbox regardless of any other
language choice, so it lives alongside the Deno importer as natural
JS-side kin.

**`dashc.wasm` owns** (same Rust code path as the native `dashc` binary,
compiled to `wasm32-unknown-unknown`, not reimplemented): Figma≠CSS
lowering (negative gap, stroke-align, canvas stacking, scale-to-inset),
profile/vocabulary validation via `dashscene-validator`, `.dsb`
emission. Deno hands it canonical post-closure JSON and gets back
`.dsb` bytes or a diagnostics report — the same rule applies whether
`dashc` is invoked natively in CI or from Deno: an error blocks
emission, never a silent drop.

Layout:

    dashscene-staging/
      crates/            13 dashscene-* / dashbuf / dashpaint / dashcue /
                          dashlang / dashc crates
      importers/figma/   Deno importer + sharedPluginData annotator plugin
        deno.json
        src/
        plugin/          Figma plugin manifest + sandboxed plugin code
      corpus/
      goldens/

## Why

- Keeps exactly one implementation of lowering and validation — no
  drift between a Rust path and a hypothetical TypeScript
  reimplementation. Byte-reproducibility (R7) holds trivially since
  it's the same Rust code either way, the same argument that already
  applies to wasm-Skia goldens matching CPU goldens by construction.
- **Same repo, not option 3.** Unlike Unity
  (`docs/decisions/unity-separate-repo-deferred.md`), where the only
  coupling to the core is a narrow versioned FFI wire protocol and a
  repo split costs nothing, the Deno importer directly imports
  `dashc.wasm` — the compiled output of the `dashc` crate sitting in
  the same workspace. Splitting repos would mean publishing
  `dashc.wasm` as a versioned artifact and consuming it with a version
  pin from a second repo, coordinating two-PR landings every time the
  wasm interface changes — real overhead for a boundary that isn't
  architecturally distinct, since it's the same compiler, just called
  from a different host process. A monorepo doesn't require one
  toolchain: the Deno code lives in its own subdirectory with its own
  `deno.json` and its own CI job (path-filtered so Rust-only changes
  don't trigger it and vice versa); JSR publishing works fine from a
  subdirectory, same as crates.io publishing works fine for individual
  crates inside a Cargo workspace.

## Consequences

- The importer publishes as `@driftsys/dashscene-figma` on JSR
  (`importers/figma/deno.json`). The name is confirmed available — the
  `@driftsys` scope already exists on JSR and the package name is
  unclaimed. Publishing has not happened yet; it waits on real code to
  ship.
- The wasm ABI `dashc` exposes to the Deno side is its own decision:
  `docs/decisions/dashc-wasm-abi.md`.
