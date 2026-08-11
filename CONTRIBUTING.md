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
4. `just verify` runs automatically on every push, and takes seconds:
   `git std lint --range main..HEAD`, then `lint`, `audit`, and a secret scan
   of the objects your push adds. It runs **no test tier** — `lint` still
   type-checks the workspace, so a compile error fails locally, but a failing
   test does not.

   Run `just build` by hand when you want the tests before pushing. CI runs
   the full tier on every push and pull request either way.

   The `secrets` step needs [gitleaks](https://github.com/gitleaks/gitleaks),
   which `./bootstrap` reports on but does not install — `brew install
   gitleaks`, or a release binary. `just check` has two other external
   prerequisites `./bootstrap` does not install either — `cargo-audit` for
   `just audit`, and a C toolchain for `just c-abi`. Each stops the gate
   rather than being skipped.
5. If your change produced working notes under `docs/wip/`, fold them into the
   durable records under `docs/specification/`, `docs/design/`, `docs/decisions/`
   or `docs/technotes/` **before** opening the PR, so they are part of what gets
   reviewed. A PR that leaves work sitting in `docs/wip/` is not finished.
6. Open a PR. CI runs the same gates (`fmt`, `dprint`, `clippy`, `test`,
   `convco`, `secrets`) plus `demo-build`, `wasm-build`, `wasm-gates`
   (everything compiled for `wasm32-unknown-unknown`), `android-build`,
   `render-oracle`, the two `exit-gate` jobs, the path-filtered `deno` and
   `calibration` jobs, and the `atlas-repro` job (byte-reproducibility of the
   glyph-atlas pipeline, with a cached `msdf-atlas-gen` build), aggregated into a
   single required `ci` check.
7. Expect a review alongside CI rather than after it, and expect its findings to
   appear as a checklist on the PR. Findings that matter are fixed before merge;
   the rest become `debt`-labelled issues against a milestone. If a fix changes
   the implementation, the fix gets looked at too — in this repository the
   serious defects have more often been in a correction than in the original.

## Recipes

Run `just --list` for the full recipe set. The common ones:

| recipe        | what it does                                                                                                                                                    |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `just build`  | assemble + check (test, lint, audit, secrets) — the local gate; two tests that re-derive committed asset tables sit outside it (`docs/decisions/test-tiers.md`) |
| `just verify` | the pre-push hook: commit-message lint + lint + audit + a scoped secret scan. Seconds, and runs no test tier                                                    |
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
