set dotenv-load := true

# List available commands
help:
    just --list

# Build the CLI
build:
    cargo build

# Run the CLI, e.g. `just run list` or `just run -- create foo`
run *args:
    cargo run -- {{args}}

# Run tests
test:
    cargo test

# Format code
fmt:
    cargo fmt

# Run the full local verification suite
check:
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Check without producing a release binary
cargo-check:
    cargo check

# Install wt into Cargo's bin directory
install:
    cargo install --path . --force

# Build optimized binary
release:
    cargo build --release
