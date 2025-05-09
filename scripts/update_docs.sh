#!/usr/bin/env bash
set -euo pipefail

# Change to repository root
cd "$(git rev-parse --show-toplevel)"

echo "Updating documentation..."

# Check if gen-crate-readmes is installed
if command -v cargo-readme &> /dev/null; then
    echo "Generating crate READMEs..."
    
    # Find all Cargo.toml files
    find ./crates -name "Cargo.toml" -type f | while read -r crate_path; do
        crate_dir=$(dirname "$crate_path")
        echo "Updating README for $crate_dir"
        (cd "$crate_dir" && cargo readme > README.md || echo "Failed to generate README for $crate_dir")
    done
else
    echo "cargo-readme not found, skipping crate README generation"
    echo "To install: cargo install cargo-readme"
fi

# Check if mkdocs is installed and if mkdocs.yml exists
if command -v mkdocs &> /dev/null && [ -f "mkdocs.yml" ]; then
    echo "Building MkDocs site..."
    mkdocs build
fi

# Check if there are any changes to READMEs
if [ -n "$(git status --porcelain | grep 'README.md')" ]; then
    echo "⚠️ There are uncommitted changes to README files."
    echo "Please review and commit these changes."
    git status --porcelain | grep 'README.md'
    
    # In CI environment, this should fail
    if [ "${CI:-false}" = "true" ]; then
        echo "CI detected, failing the build due to uncommitted README changes."
        exit 1
    fi
else
    echo "✅ All README files are up-to-date."
fi

echo "Documentation update complete!" 