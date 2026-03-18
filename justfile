# DashScene task runner

# Build all crates
assemble:
    cargo build

# Run all tests
test:
    cargo test

# Run linters
lint:
    cargo clippy -- -D warnings
    cargo fmt -- --check

# Run tests and lints
check: test lint

# Full build pipeline
build: assemble check

# Format code
fmt:
    cargo fmt

# Generate and open documentation
doc:
    cargo doc --open

# Publish crates to crates.io in dependency order
publish:
    cargo publish -p dashscene-core
    cargo publish -p dashcue
    cargo publish -p dashpaint
    cargo publish -p dashbuf
    cargo publish -p dashlang
    cargo publish -p dashc
    cargo publish -p dashscene-engine
    cargo publish -p dashscene-compose
    cargo publish -p dashscene-unity
    cargo publish -p dashscene-web
    cargo publish -p dashscore
    cargo publish -p dashscene

# Clean build artifacts
clean:
    cargo clean
