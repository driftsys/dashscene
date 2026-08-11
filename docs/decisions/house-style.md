# House style: follow driftsys/git-std, driftsys/upskill, driftsys/markspec conventions

    status   accepted
    date     2026-07-11 — git-std dogfooding and the docs/ taxonomy confirmed
             2026-07-12
    scope    repo-wide: Cargo workspace shape, justfile, CI, docs/ taxonomy,
             governance files

## Context

`dashscene` needed a set of repo-tooling conventions — workspace
shape, task runner, formatting, versioning, CI — rather than inventing
its own. `driftsys/git-std`, `driftsys/upskill`, and `driftsys/markspec`
were read directly as the house-style reference.

## Decision

Follow those three repos' conventions:

**Cargo workspace shape** (git-std): `resolver = "3"`;
`[workspace.package]` with `edition = "2024"` (not 2021), `license =
"MIT"`, shared `repository`; `[workspace.dependencies]` with `path +
version` for every internal crate; `[profile.release]` — `lto = true`,
`strip = true`, `codegen-units = 1`.

**This repository diverges on the licence.** git-std's convention is MIT,
and git-std is itself MIT-licensed. dashscene is Apache-2.0, for the
patent grant — see
`docs/decisions/apache-2-0-for-the-patent-grant.md`. The rest of the
workspace shape above is followed as written.

`[workspace.package]` also carries **`version`**, added by story #795 when
the workspace's together-versioning was made structural: every crate takes
`version.workspace = true` and holds no version of its own to drift
([publishable-and-the-first-version.md](publishable-and-the-first-version.md)).

**`justfile`** (git-std's is the template): `assemble` (cargo build),
`test`, `lint` (`cargo clippy -- -D warnings` + `cargo fmt -- --check` +
`dprint check` + `markdownlint-cli`), `audit` (`cargo audit`), `check`
(test + lint + audit), `build` (assemble + check), `verify` (`git std
lint --range main..HEAD` + `just build` — run before opening a PR),
`fmt`, `doc` (`cargo doc --open`), `book` (`mdbook serve`), `release`
(`git std bump`), `publish` (ordered `cargo publish` per crate,
dependency order), `install`, `clean`. Add two dashscene-specific
recipes: `wasm` (build `dashc` for `wasm32-unknown-unknown`, needed by
the Deno importer) and `deno-check`/`deno-test`/`deno-fmt` scoped to
`importers/figma/`.

The `test` and `check` recipes deviate from that template: `test` runs the
sanity tier and `check` the regression tier, and `test-regression`,
`calibrate` and `test-all` are additions. The reason, the measurements and
the tier definitions are in [test-tiers.md](test-tiers.md).

The dashscene-specific set has grown past those two, and the `justfile`
itself is the authority rather than this list. Two additions are worth
naming here because they carry decisions: **`package`**, which runs the
registry-consistency test and then `cargo package` for every publishable
member — the step that answers "what does a consumer actually get" — and
**`measure-runtime`**, which weighs `measure/web-minimal` beside `demo-web`
because a library crate has no measurable size of its own. Both came from
story #795, and both are explained in
[publishable-and-the-first-version.md](publishable-and-the-first-version.md).

**`dprint.json`**: markdown only (`includes: ["**/*.md"]`, the
`dprint/markdown` plugin) — it does not replace `cargo fmt` or `deno
fmt`, both of which run as their own separate lint/fmt steps for their
respective languages.

**`.git-std.toml`**: `scheme = "semver"`, `strict = true`, `scopes` as
an explicit list rather than `"auto"`, which only discovers `crates/*`
and leaves no valid scope for commits that aren't crate-specific. The
list is every crate name — 13 when this was written, **19 today** — plus a
scope for each non-crate component
that has its own artifacts and tooling — `goldens` (the golden images
and their diff tooling), `corpus` (the fixture corpus itself: captured
Figma JSON, fonts, generated stress scenes — data only, since the
capture tool is code and lives under `importers/`), `importers` (the
Deno/TypeScript Figma importer and its capture tool, which have their
own toolchain and their own CI job), `demo` (both showcase hosts) and
`measure` (artifacts built to be weighed rather than run) — plus the
repo-wide scopes `repo`,
`docs`, `ci`, `hooks`, `deps`, `release`. `specs/` and `docs/` share the
`docs` scope: `specs/` is documentation and earns no scope of its own.
Also `[versioning] tag_prefix = "v"`.

**That count is now load-bearing rather than descriptive.**
`demo/tests/registry_consistency.rs` derives the crate list from
`[workspace] members` and fails when `scopes` disagrees with it, so a crate
added without its scope no longer merges (story #795).

**`[[version_files]]` changed shape entirely at story #795, and the
description above no longer holds.** There is no longer one entry per crate
pointing at that crate's own version string: the crates inherit
`version.workspace = true` and hold no version to point at. The entries are
now one per **internal dependency requirement** in the root manifest, each
anchored on its own crate name — nineteen of them — because git-std's
`write_version` splices exactly one span per entry, so a single unanchored
entry would move one requirement and leave the rest at the old version
behind a registry that looked covered. The workspace version itself needs no
entry, since git-std's builtin Cargo handling moves `[workspace.package]
version` section-scoped. The full reasoning, including the eighteenth entry
that was written and removed on review, is in
[publishable-and-the-first-version.md](publishable-and-the-first-version.md).

**CI** (`.github/workflows/ci.yml`, git-std's shape): separate jobs for
`fmt` (`cargo fmt -- --check`), `dprint` (`dprint/check@v2.3` action),
`clippy` (`cargo clippy -- -D warnings`, `Swatinem/rust-cache`), `test`
(`cargo test`, `Swatinem/rust-cache`), `convco` (PR-only conventional-
commit-message validation), aggregated by a final `ci` job that fails if
any of the above failed. For dashscene, add a `deno` job (check/lint/
test/fmt, scoped to `importers/figma/` via a `dorny/paths-filter` gate so
Rust-only changes don't trigger it), a `wasm-build` job (`dashc` →
`wasm32-unknown-unknown`, verifies the Deno importer's dependency
actually builds) and a `wasm-gates` job (everything in `just check` and
`just lint` that names that triple and is not `dashc` — `just
wasm-painter`, `just wasm-host` and `just wasm-lint`, invoked as recipes
so CI holds no second copy of the list). The two are separate because
`deno` waits on the first for its artifact and need not wait on the
second. Neither covers the **release**-profile wasm builds `just
web-build` and `just measure-runtime` run, so a failure that appears only
under `lto = true` is caught by no job. No cross-platform
`build-release` matrix yet — that's
git-std's own binary-distribution concern, not relevant until dashscene
ships a distributable binary of its own.

The `test` job deviates from that template: it runs `cargo nextest run
--workspace` (the regression tier) plus `cargo test --workspace --doc`
rather than a plain `cargo test`, and a `calibration` job runs the
calibration tier on the `packer` path filter. The reason and the tier
definitions are in [test-tiers.md](test-tiers.md).

**`bootstrap` script**: ensures `git-std` itself is installed (detects
platform, downloads the matching release, verifies the sha256, installs
to `~/.local/bin`), then `exec git-std bootstrap` — git-std's own
subcommand handles the repo-specific setup (git hooks, etc.) from there.
Run after cloning or creating a worktree.

**Deno side** (markspec's `deno.json` is the template, applies to
`importers/figma/`): a `workspace` array pointing at the package
directory (Deno's native workspace feature, same idea as the Cargo
workspace); imports preferring JSR (`jsr:@std/...`) over npm where a JSR
package exists, `npm:` specifier otherwise (e.g. `@figma/rest-api-spec`
is npm-only); `tasks` for `check` (`deno check` on entry points), `test`
(`deno test` with the narrowest `--allow-*` set that works), `lint`
(`deno lint`), `fmt` (`deno fmt`); `fmt.include` scoped to
`ts/tsx/js/jsx/mts/cts/mjs/cjs`; `test.exclude`/`lint.exclude` covering
`editors/`, `.worktrees/`, `.claude/worktrees/`.

**Governance/docs files**, present in all three reference repos and
expected here too: `LICENSE` (MIT in the reference repos; Apache-2.0
here, per the divergence above), `CODE_OF_CONDUCT.md`,
`CONTRIBUTING.md`, `SECURITY.md`, `.editorconfig`, `.markdownlint.json`,
`book.toml` + `docs/book/` (mdBook source — an overview and a usage
guide, the online guide's actual content).

**`docs/` follows the `sdd-working-memory-lifecycle` rule's taxonomy**
(separate from `docs/book/`, the online guide): `docs/wip/` (Superpowers
spec+plan working memory, transient, tracked), `docs/archive/` (raw wip
content once gardened), `docs/specification/` (requirements),
`docs/design/` (architecture), `docs/decisions/` (decision records),
`docs/technotes/` (explanatory notes).

**Dogfooding**: `dashscene` dogfoods `git-std` from day one. The
`justfile` (`release`/`verify` recipes), `.git-std.toml`, `bootstrap`
script, and CI `convco` job all wire it in for real rather than as
stubs/placeholders.

## Why

Copying conventions already proven across three sibling repos avoids
re-deriving repo tooling from scratch and keeps `dashscene`
consistent with the rest of the driftsys house style.
