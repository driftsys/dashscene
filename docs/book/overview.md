# Overview

dashscene turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation, one shared
layout+text runtime, and interchangeable paint backends.

Primary targets are embedded/automotive-class devices rendered by a game
engine (Unity) or a lean native renderer, with a Skia backend serving as
the reference implementation, test oracle, and 2D path.

## See it run

`cargo run -p demo --release` opens a window and animates one of three
showcase scenes, drawn by the Skia reference painter:

```sh
cargo run -p demo --release                 # surfaces, the default
cargo run -p demo --release -- typography   # Latin and Arabic text
cargo run -p demo --release -- layout       # flex, grid, reflow, a variant switch
cargo run -p demo --release -- --list       # what there is
```

`corpus/showcase/README.md` lists every construct the three scenes cover,
and what each one costs.

## Status

This project is in early development. Nothing is published, and there is
no end-user functionality yet: the demonstration above is a host around
the runtime, not a product.

v0 is built one slice at a time. Slices `v0.1` to `v0.13` have closed —
the walking skeleton, the flex core, the paint vocabulary, variants and
staged mutation, Latin and then Arabic text, the Figma importer, layout
and paint fidelity, real-file fidelity, document sections and the asset
model, the packer and its quality profiles, and a pre-v1 hardening pass.
The exception is `v0.9`, which stays open on its one remaining item, the
CI job asserting the seven v0 exit criteria together. `v0.14` — the
showcase runtime above — is the current slice.

Two components named in this book are not built: the Unity painter
(`dashscene-unity` carries Rust FFI bindings only, and the Unity project
is a separate repository that does not exist yet) and the lean painter
(`dashscene-gpu`, whose crate exists and draws nothing — epic #569).
The `dashscene-web` name once described a wasm/tiny-skia painter, which is
retired and superseded by `dashscene-gpu`; the crate is the web
integration surface since story #741, and `dashscene-desktop` is its
desktop counterpart since story #794.
`docs/roadmap.md` is the authority on what has landed.

## Where things live

- The repository's entry point: `README.md` at the root.
- Goals and requirements: `docs/specification/`.
- Architecture — stack, document format, producers, painters: the
  per-component records in `docs/design/`, starting from
  `docs/design/architecture.md`.
- Decisions made since, each traced to what it affects: `docs/decisions/`.
- The release plan: `docs/roadmap.md`.
- Contributor-facing entry point (crate map, commands, current start
  order): `AGENTS.md` at the repository root.

See the [usage guide](./usage-guide.md) for building the project from
source.
