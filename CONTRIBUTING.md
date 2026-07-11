# Contributing to dashscene

Thanks for your interest in contributing. This document covers the mechanics
of getting a change in; for the architecture and rationale behind the crate
layout, start with `specs/DESIGN_1.md` and `specs/SCOPE_DECISIONS.md`.

## Getting set up

```sh
git clone https://github.com/driftsys/dashscene-staging
cd dashscene-staging
./bootstrap
```

`bootstrap` installs [git-std](https://github.com/driftsys/git-std) and hands
off to `git std bootstrap`, which wires up repo-local git hooks.

## Workflow

1. Branch from `main`.
2. Make your change. Every crate under `crates/` inherits
   `[workspace.package]` from the root `Cargo.toml` (`edition = "2024"`,
   `license = "MIT"`) — do not override those per-crate.
3. Write commits as [Conventional Commits](https://www.conventionalcommits.org)
   (`feat:`, `fix:`, `chore:`, etc., with a scope when it aids clarity, e.g.
   `feat(dashscene-engine): ...`). `git-std` lints commit messages against
   this format and drives changelog generation from them.
4. Run `just verify` before opening a PR. This runs `git std lint
   --range main..HEAD` followed by the full `just build` (assemble, test,
   lint, audit).
5. Open a PR. CI runs the same gates (`fmt`, `dprint`, `clippy`, `test`,
   `convco`) plus the path-filtered `deno` and `wasm-build` jobs where
   relevant, aggregated into a single required `ci` check.

## Recipes

Run `just --list` for the full recipe set. The common ones:

| recipe        | what it does                                               |
| ------------- | ---------------------------------------------------------- |
| `just build`  | assemble + check (test, lint, audit) — the full local gate |
| `just verify` | commit-message lint + `just build` — run before a PR       |
| `just fmt`    | reformat Rust and markdown in place                        |
| `just wasm`   | build `dashc` for `wasm32-unknown-unknown`                 |
| `just book`   | serve the mdBook docs locally                              |

## Crate ownership and scope

See `specs/SCOPE_DECISIONS.md` §2 for the full crate map and the reasoning
behind each name. If your change spans a boundary described there (e.g.
boundary A — the `.dsb` load gate, or boundary B — the painter contract),
call that out explicitly in the PR description.

## Deno importer

Changes under `importers/figma/` follow `deno.json`'s own `fmt`/`lint`/`test`
tasks (`just deno-fmt`, `just deno-test`, `just deno-check`) rather than the
Rust toolchain. CI only runs the `deno` job when files under that path
change.

## Reporting issues

Use GitHub issues on this repository. For security-sensitive reports, see
`SECURITY.md` instead of filing a public issue.
