# Contributing to dashscene

Thanks for your interest in contributing. This document covers the mechanics
of getting a change in; for the architecture and rationale behind the crate
layout, start with `docs/design/architecture.md` and `docs/decisions/`.

## Getting set up

```sh
git clone https://github.com/driftsys/dashscene
cd dashscene
./bootstrap
```

`bootstrap` installs [git-std](https://github.com/driftsys/git-std) and hands
off to `git std bootstrap`, which wires up repo-local git hooks.

## Workflow

1. Branch from `main`.
2. Make your change. Every crate under `crates/` inherits
   `[workspace.package]` from the root `Cargo.toml` (`edition = "2024"`,
   `license = "Apache-2.0"`) — do not override those per-crate.
3. Write commits as [Conventional Commits](https://www.conventionalcommits.org)
   (`feat:`, `fix:`, `chore:`, etc., with a scope when it aids clarity, e.g.
   `feat(dashscene-engine): ...`). `git-std` lints commit messages against
   this format and drives changelog generation from them.
4. Run `just verify` before opening a PR. This runs `git std lint
   --range main..HEAD` followed by the full `just build` (assemble, test,
   lint, audit, secrets).

   When every changed file is documentation — Markdown under `docs/` or at
   the repository root — `verify` runs `lint`, `audit` and `secrets` instead
   of `build`, and no test tier runs. `scripts/is-code-change` decides, and
   the CI `changes` job gates on that same script, so your local result and
   CI agree on what documentation means.

   The `secrets` step needs [gitleaks](https://github.com/gitleaks/gitleaks),
   which `./bootstrap` reports on but does not install — `brew install
   gitleaks`, or a release binary. `just check` has two other external
   prerequisites `./bootstrap` does not install either — `cargo-audit` for
   `just audit`, and a C toolchain for `just c-abi`. Each stops the gate
   rather than being skipped.
5. Open a PR. CI runs the same gates (`fmt`, `dprint`, `clippy`, `test`,
   `convco`, `secrets`) plus the path-filtered `deno` job, the `wasm-build`
   job, and the `atlas-repro` job (byte-reproducibility of the glyph-atlas
   pipeline, with a cached `msdf-atlas-gen` build), aggregated into a
   single required `ci` check.

## Recipes

Run `just --list` for the full recipe set. The common ones:

| recipe        | what it does                                                                                                                                                    |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `just build`  | assemble + check (test, lint, audit, secrets) — the local gate; two tests that re-derive committed asset tables sit outside it (`docs/decisions/test-tiers.md`) |
| `just verify` | commit-message lint + `just build` — run before a PR. A documentation-only change takes lint + audit + secrets instead, and runs no test tier                   |
| `just fmt`    | reformat Rust and markdown in place                                                                                                                             |
| `just wasm`   | build `dashc` for `wasm32-unknown-unknown`                                                                                                                      |
| `just book`   | serve the mdBook docs locally                                                                                                                                   |

## Crate ownership and scope

See `docs/decisions/crate-name-map.md` for the full crate map and the reasoning
behind each name. If your change spans a boundary described there (e.g.
boundary A — the `.dsb` load gate, or boundary B — the painter contract),
call that out explicitly in the PR description.

## Deno importer

Changes under `importers/figma/` follow `deno.json`'s own `fmt`/`lint`/`test`
tasks (`just deno-fmt`, `just deno-test`, `just deno-check`) rather than the
Rust toolchain. CI runs the `deno` job when the importer, the fixture corpus,
or the Rust side it calls across the wasm ABI changes — the suite loads
`dashc_wasm.wasm` and pins its output against a golden `.dsb`, so a Rust-only
change can break it with no edit under `importers/`
(`docs/decisions/dashc-wasm-abi.md`).

## Reporting issues

Use GitHub issues on this repository. For security-sensitive reports, see
`SECURITY.md` instead of filing a public issue.
