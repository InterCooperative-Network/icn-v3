#!/bin/bash
# run_governance_pipeline.sh - Demonstrates the complete governance pipeline

set -e # Exit on error

# Print section header
print_header() {
    echo
    echo "==========================================="
    echo "  $1"
    echo "==========================================="
    echo
}

# Check for cargo and required tools
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is required but not installed."
    exit 1
fi

# Set paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
EXAMPLES_DIR="$ROOT_DIR/examples"
OUTPUT_DIR="$ROOT_DIR/target/governance-demo"
CCL_FILE="$EXAMPLES_DIR/budget.ccl"
DSL_FILE="$OUTPUT_DIR/budget.dsl"
WASM_FILE="$OUTPUT_DIR/budget.wasm"
RECEIPT_FILE="$OUTPUT_DIR/receipt.json"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if budget.ccl exists, create it if not
if [ ! -f "$CCL_FILE" ]; then
    print_header "Creating example CCL file"
    mkdir -p "$EXAMPLES_DIR"
    cat > "$CCL_FILE" << EOF
# budget.ccl - Example budget allocation
proposal "Q2 Budget Allocation" {
  scope "icn/finance"
  
  allocate {
    project "infrastructure" {
      amount 5000 USD
      category "maintenance"
    }
    
    project "outreach" {
      amount 3000 USD
      category "marketing"
    }
  }
}
EOF
    echo "Created example CCL file at $CCL_FILE"
fi

# Build the CLI tool first
print_header "Building ICN CLI"
cargo build --package icn-cli

# Path to the CLI executable
ICN_CLI="$ROOT_DIR/target/debug/icn-cli"

# Execute the complete governance pipeline
print_header "Starting Governance Pipeline"
echo "Input CCL file: $CCL_FILE"
echo "Output directory: $OUTPUT_DIR"

# Step 1: Compile to DSL
print_header "Step 1: Compiling CCL to DSL"
"$ICN_CLI" ccl compile-to-dsl --input "$CCL_FILE" --output "$DSL_FILE"
echo "DSL file generated: $DSL_FILE"

# Step 2: Compile to WASM
print_header "Step 2: Compiling DSL to WASM"
"$ICN_CLI" ccl compile-to-wasm --input "$DSL_FILE" --output "$WASM_FILE"
echo "WASM file generated: $WASM_FILE"

# Step 3: Execute WASM
print_header "Step 3: Executing WASM"
"$ICN_CLI" runtime execute --wasm "$WASM_FILE" --receipt "$RECEIPT_FILE"
echo "Execution receipt: $RECEIPT_FILE"

# Step 4: Verify Receipt
print_header "Step 4: Verifying Receipt"
"$ICN_CLI" runtime verify --receipt "$RECEIPT_FILE"

# Done
print_header "Governance Pipeline Complete"
echo "A complete governance lifecycle was demonstrated:"
echo "1. CCL -> DSL compilation"
echo "2. DSL -> WASM compilation"
echo "3. Execution in CoVM"
echo "4. Receipt generation and verification"
echo
echo "All artifacts are in: $OUTPUT_DIR"
echo

# Make the file executable
chmod +x "$SCRIPT_DIR/run_governance_pipeline.sh" 