# ICN development tasks
_default:
    @just --list

# Run all CI checks
ci: lint test

# Run all linting checks
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo deny check

# Run all tests
test:
    cargo test --all

# Run benchmarks
bench:
    cargo bench

# Update documentation
docs:
    ./scripts/update_docs.sh

# Setup development environment
setup:
    cargo install cargo-deny cargo-readme pre-commit
    pre-commit install
    pre-commit install --hook-type commit-msg

# Clean build artifacts
clean:
    cargo clean

# Build all packages in release mode
build-release:
    cargo build --release --all

# Update dependencies
update-deps:
    cargo update 