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

# Run before opening a PR: commit-message lint over the branch range, then build.
verify:
    git std lint --range main..HEAD
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
    mdbook serve docs

# Cut a release: git-std bumps versions, writes the changelog, tags.
release:
    git std bump

# Publish every crate to crates.io in dependency order.
publish:
    cargo publish -p dashbuf
    cargo publish -p dashscene-core
    cargo publish -p dashscene-typeset
    cargo publish -p dashscene-engine
    cargo publish -p dashscene-validator
    cargo publish -p dashpaint
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
