# dashscene-staging — task runner
#
# Recipe set mirrors driftsys/git-std's own justfile (house style, see
# SCOPE_DECISIONS.md §7), plus two dashscene-specific additions: `wasm`
# (dashc -> wasm32-unknown-unknown, needed by the Deno importer) and the
# `deno-*` recipes scoped to importers/figma/.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Default: list recipes.
default:
    @just --list

# Build every crate in the workspace.
assemble:
    cargo build --workspace

# Run the Rust test suite.
test:
    cargo test --workspace

# Rust + markdown lint gate: clippy, cargo fmt check, dprint check, markdownlint.
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all -- --check
    dprint check
    markdownlint '**/*.md' --ignore target --ignore node_modules

# Dependency vulnerability audit.
audit:
    cargo audit

# Full non-build verification: test + lint + audit.
check: test lint audit

# Everything short of a PR: assemble + check.
build: assemble check

# Run before opening a PR (and as the pre-push hook): commit-message
# lint over commits not yet on origin/main, then build. The range is taken
# against origin/main rather than local main, because local main goes stale
# relative to the remote and would lint commits that are already upstream.
# The range can legitimately be empty (see the recipe) — that is issue
# #110, and it is handled rather than avoided by the choice of ref.
verify:
    #!/usr/bin/env bash
    set -euo pipefail
    # An unresolvable range and an empty range are different, and only the
    # second is benign — so check for origin/main before asking about the
    # range. `git rev-list missing/ref..HEAD` exits non-zero but prints
    # nothing, and `set -e` does not fire inside an `if` condition, so
    # testing the output alone would read a missing ref as "no commits" and
    # skip the lint silently. This gate must fail closed.
    if ! git rev-parse --verify --quiet origin/main >/dev/null; then
        echo "verify: origin/main is missing — run 'git fetch origin'." >&2
        echo "verify: refusing to skip the commit-message lint." >&2
        exit 1
    fi
    # An empty range IS benign: it is what a branch-deletion push, a re-push
    # of an already-pushed branch, and a push from an up-to-date main all
    # look like. `git std lint` exits non-zero on one, which blocked those
    # pushes outright (issue #110). `-n 1` because the question is only
    # whether any commit exists, not what they are.
    if [ -z "$(git rev-list -n 1 origin/main..HEAD)" ]; then
        echo "verify: no commits in origin/main..HEAD — nothing to lint"
    else
        git std lint --range origin/main..HEAD
    fi
    just build

# Reformat everything in place (Rust + markdown).
fmt:
    cargo fmt --all
    dprint fmt

# Open the rustdoc build in a browser.
doc:
    cargo doc --workspace --no-deps --open

# Serve the mdBook docs locally.
book:
    mdbook serve

# Cut a release: git-std bumps versions, writes the changelog, tags.
release:
    git std bump

# Publish every crate to crates.io in dependency order.
publish:
    cargo publish -p dashbuf
    cargo publish -p dashpaint
    cargo publish -p dashscene-core
    cargo publish -p dashscene-typeset
    cargo publish -p dashscene-engine
    cargo publish -p dashscene-validator
    cargo publish -p dashscene-skia
    cargo publish -p dashcue
    cargo publish -p dashlang
    cargo publish -p dashc
    cargo publish -p dashscene-unity
    cargo publish -p dashscene-web
    cargo publish -p dashscene

# Install local toolchain bits (git hooks, git-std, dprint, markdownlint-cli).
install:
    ./bootstrap

# Remove build artifacts.
clean:
    cargo clean

# Build dashc for wasm32-unknown-unknown, the target the Deno importer loads.
wasm:
    cargo build -p dashc --release --target wasm32-unknown-unknown

# Type-check the Deno importer's entry points.
deno-check:
    cd importers/figma && deno task check

# Run the Deno importer's test suite.
deno-test:
    cd importers/figma && deno task test

# Format the Deno importer sources.
deno-fmt:
    cd importers/figma && deno task fmt
