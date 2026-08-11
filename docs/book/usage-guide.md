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
- [`cargo-nextest`](https://nexte.st) — runs the test tiers (`just test`,
  `just test-regression`, `just calibrate`, `just test-all`); installed
  automatically by `./bootstrap`.
- [Deno](https://deno.com) 2.x if working on `importers/figma/`.

## Getting started

```sh
git clone https://github.com/driftsys/dashscene.git
cd dashscene
./bootstrap   # installs git-std, cargo-nextest, and repo git hooks
```

## Common commands

| Command                | What it does                                                                                                                             |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `just build`           | assemble + full check (this is what CI runs)                                                                                             |
| `just test`            | sanity tier — ~7 s. Between edits, and before every commit.                                                                              |
| `just test-regression` | regression tier — every test but the two calibration re-derivations. What `build` and the CI `test` job run; the pre-push hook does not. |
| `just calibrate`       | calibration tier — 10 tests, ~54 s. Re-derives the committed asset tables.                                                               |
| `just test-all`        | every tier in one run.                                                                                                                   |
| `just lint`            | clippy -D warnings, `cargo fmt --check`, `dprint check`, markdownlint                                                                    |
| `just fmt`             | reformat everything in place                                                                                                             |
| `just check`           | regression tier + lint + audit                                                                                                           |
| `just verify`          | the pre-push hook: commit-message lint, then lint + audit + a scoped secret scan. Seconds, and runs no test tier                         |
| `just wasm`            | build `dashc` for `wasm32-unknown-unknown`                                                                                               |
| `just deno-check`      | type-check the Deno Figma importer                                                                                                       |
| `just book`            | serve this book locally                                                                                                                  |

The full recipe set is in the repository's `justfile`. Which test tier to run
when is `docs/decisions/test-tiers.md`.
