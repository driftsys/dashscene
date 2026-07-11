# Usage guide

There is no published library or binary yet — this guide covers building
the project from source.

## Prerequisites

- Rust (stable), plus the `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
- [`flatc`](https://github.com/google/flatbuffers) — required at build
  time by the `dashbuf` crate. Install a version matching the
  `flatbuffers` crate pinned in the workspace `Cargo.toml` (e.g.
  `brew install flatbuffers` or
  `apt-get install flatbuffers-compiler`).
- [`just`](https://github.com/casey/just), [`dprint`](https://dprint.dev),
  and [`markdownlint-cli`](https://github.com/igorshubovych/markdownlint-cli)
  for linting.
- [Deno](https://deno.com) 2.x if working on `importers/figma/`.

## Getting started

```sh
git clone https://github.com/driftsys/dashscene-staging.git
cd dashscene-staging
./bootstrap   # installs git-std and repo git hooks
```

## Common commands

| Command           | What it does                                                 |
| ----------------- | ------------------------------------------------------------ |
| `just build`      | assemble the workspace, then run the full check suite        |
| `just test`       | `cargo test --workspace`                                     |
| `just lint`       | clippy, `cargo fmt --check`, `dprint check`, markdownlint    |
| `just fmt`        | reformat everything in place                                 |
| `just wasm`       | build `dashc` for `wasm32-unknown-unknown`                   |
| `just deno-check` | type-check the Deno Figma importer                           |
| `just book`       | serve this book locally                                      |
| `just verify`     | commit-message lint + `just build` — run before opening a PR |

The full recipe set is in the repository's `justfile`.
