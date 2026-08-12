# Contributing to dashscene

Thanks for your interest in contributing. This document covers the mechanics of
getting a change in; for the architecture and rationale behind the crate layout,
start with `docs/design/architecture.md` and `docs/decisions/`.

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
2. Make your change. Every crate under `crates/` inherits `[workspace.package]`
   from the root `Cargo.toml` (`edition = "2024"`, `license = "Apache-2.0"`) —
   do not override those per-crate.
3. Write commits as [Conventional Commits](https://www.conventionalcommits.org)
   (`feat:`, `fix:`, `chore:`, etc., with a scope when it aids clarity, e.g.
   `feat(dashscene-engine): ...`). `git-std` lints commit messages against this
   format and drives changelog generation from them.
4. If your change added anything under `docs/wip/`, fold it into the durable
   records **before** you open the PR — and before the build in step 5, so that
   `just build` covers the prose you just wrote. `docs/wip/` holds working
   notes, transient by design. Folding one in is two steps, not one: write the
   durable record under `docs/specification/`, `docs/design/`, `docs/decisions/`
   or `docs/technotes/`, **and** move the original note to `docs/archive/`, in
   the same commit. A record written while the original is still sitting in
   `docs/wip/` has been copied, not folded in.

   Doing this before opening the PR rather than before merging is deliberate: it
   puts the records inside the diff that gets reviewed.

   Two other states are fine, and
   `docs/decisions/review-before-ready-not-before-open.md` is the full statement
   of all three. A note not ready to fold in stays put and gets its entry in
   `docs/wip/README.md` saying what would empty it. Half of one can be folded in
   and half held, which is common for a note spanning several slices — say which
   half is which in the note itself. If you remove a note from `docs/wip/`,
   update that README in the same commit; the ledger going stale is a failure it
   records against itself more than once.

   Most changes add nothing under `docs/wip/` and this step is then empty for
   you. **`docs/wip/` is not meant to be empty in general** — it is also a
   standing shelf of notes for work that has not started, each listed in
   `docs/wip/README.md` with the condition that removes it. Leave those alone;
   the step is about what your own branch adds.

5. `just verify` runs automatically on every push, and takes seconds:
   `git std lint --range main..HEAD`, then `lint`, `audit`, and a secret scan of
   the objects your push adds. It runs **no test tier** — `lint` still
   type-checks the workspace and runs both markdown gates, so a compile error or
   a formatting error fails locally, but a failing test does not.

   Run `just build` by hand when you want the tests before pushing. Until a PR
   exists nothing runs them for you: `ci.yml` fires on `pull_request` and on
   pushes to `main`, so a push to a branch with no PR open triggers no workflow.
   Once the PR is open, each further push re-runs `ci`. Even then a green `ci`
   is not by itself a statement that a test tier ran — the compile and test jobs
   are gated on whether the diff touches code (`docs/decisions/test-tiers.md`).

   The `secrets` step needs [gitleaks](https://github.com/gitleaks/gitleaks),
   which `./bootstrap` reports on but does not install —
   `brew install
   gitleaks`, or a release binary. `just check` has two other
   external prerequisites `./bootstrap` does not install either — `cargo-audit`
   for `just audit`, and a C toolchain for `just c-abi`. Each stops the gate
   rather than being skipped.

6. Open a PR — an ordinary one, not a draft. CI runs the same gates (`fmt`,
   `prim`, `clippy`, `test`, `convco`, `audit`, `secrets`) plus `demo-build`,
   `wasm-build`, `wasm-gates` (everything compiled for
   `wasm32-unknown-unknown`), `android-build`, `render-oracle`, the two
   `exit-gate` jobs, the `deno` and `calibration` jobs, and the `atlas-repro`
   job (byte-reproducibility of the glyph-atlas pipeline, with a cached
   `msdf-atlas-gen` build), aggregated into a single required `ci` check.

   The compile and test jobs are gated on whether the diff touches code, and
   `deno` and `calibration` on path filters, so a documentation-only change
   skips most of them. It does **not** skip everything: `fmt`, `prim`, `audit`,
   `secrets` and `convco` are ungated and still have to pass.

   `main` carries a ruleset, so a change reaches it through a pull request with
   `ci` green and the branch up to date. Nobody can step around that at the
   merge button — a maintainer could disable the ruleset, which is a deliberate
   act with an audit-log entry, not a quiet exception.

7. A maintainer reviews the PR while CI runs, and records the findings as a
   checklist on it. Anything critical is fixed before merge; the rest is filed
   as follow-up issues. Applying the `debt` label and a milestone to those needs
   triage permission, so it is the maintainer's step rather than yours — you do
   not need to file anything.

## Recipes

Run `just --list` for the full recipe set. The common ones:

| recipe        | what it does                                                                                                                                                                             |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `just build`  | assemble + check (test, lint, audit, secrets) — the local gate; the tests that re-derive the committed asset tables sit outside it, in `just calibrate` (`docs/decisions/test-tiers.md`) |
| `just verify` | the pre-push hook: commit-message lint + lint + audit + a scoped secret scan. Seconds, and runs no test tier                                                                             |
| `just fmt`    | reformat Rust and markdown in place                                                                                                                                                      |
| `just wasm`   | build `dashc` for `wasm32-unknown-unknown`                                                                                                                                               |
| `just book`   | serve the mdBook docs locally                                                                                                                                                            |

## Crate ownership and scope

See `docs/decisions/crate-name-map.md` for the full crate map and the reasoning
behind each name. If your change spans a boundary described there (e.g. boundary
A — the `.dsb` load gate, or boundary B — the painter contract), call that out
explicitly in the PR description.

## Deno importer

Changes under `importers/figma/` follow `deno.json`'s own `fmt`/`lint`/`test`
tasks (`just deno-fmt`, `just deno-test`, `just deno-check`) rather than the
Rust toolchain. CI runs the `deno` job when the importer, the fixture corpus, or
the Rust side it calls across the wasm ABI changes — the suite loads
`dashc_wasm.wasm` and pins its output against a golden `.dsb`, so a Rust-only
change can break it with no edit under `importers/`
(`docs/decisions/dashc-wasm-abi.md`).

## Reporting issues

Use GitHub issues on this repository. For security-sensitive reports, see
`SECURITY.md` instead of filing a public issue.
