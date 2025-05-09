#!/bin/bash
# ICN Governance Workflow Demo
# This demonstrates the complete governance lifecycle from proposal to execution

set -e

# Create output directory
OUTPUT_DIR="examples/output"
mkdir -p "$OUTPUT_DIR"

# Step 1: Compile CCL to DSL
echo -e "\e[36mSTEP 1: Compiling CCL to DSL\e[0m"
echo "ccl compile-to-dsl --input examples/ccl/budget.ccl --output $OUTPUT_DIR/budget.dsl"
echo "DSL file would contain:"
echo "-----------------------"
cat << 'EOF' > "$OUTPUT_DIR/budget.dsl"
// Generated DSL program from CCL
// Name: Q3 Budget Allocation
// Description: Allocate funds for Q3 2023 cooperative activities
// Version: 1.0.0

// Host imports
extern "C" {
    // Log a message to the host
    fn host_log_message(ptr: *const u8, len: usize);
    
    // Anchor a CID to the DAG
    fn host_anchor_to_dag(ptr: *const u8, len: usize) -> i32;
    
    // Check resource authorization
    fn host_check_resource_authorization(type_ptr: *const u8, type_len: usize, amount: i64) -> i32;
    
    // Record resource usage
    fn host_record_resource_usage(type_ptr: *const u8, type_len: usize, amount: i64);
}

// Program entrypoint
#[no_mangle]
pub extern "C" fn run() {
    log("Starting execution of Q3 Budget Allocation");
    
    // Anchor data to DAG
    log("Anchoring data: budget_q3_2023");
    let anchored = anchor_data("budget_q3_2023");
    if !anchored {
        log("Failed to anchor data");
    }
    
    // Perform a metered action
    log("Performing action: budget_allocation with amount 10000");
    if check_authorization("budget_allocation", 10000) {
        record_usage("budget_allocation", 10000);
        log("Action authorized and recorded");
    } else {
        log("Action not authorized");
    }
    
    // Mint tokens
    log("Minting 100 of participation_token to community_pool");
    if check_authorization("token_mint", 100) {
        record_usage("token_mint", 100);
        log("Token minting authorized and recorded");
    } else {
        log("Token minting not authorized");
    }
    
    log("Execution completed successfully");
}
EOF
cat "$OUTPUT_DIR/budget.dsl"

# Step 2: Compile DSL to WASM
echo -e "\n\e[36mSTEP 2: Compiling DSL to WASM\e[0m"
echo "ccl compile-to-wasm --input examples/ccl/budget.ccl --output $OUTPUT_DIR/budget.wasm"
echo "WASM binary would be generated"
touch "$OUTPUT_DIR/budget.wasm"  # Create empty WASM file as a placeholder

# Step 3: Create a proposal
echo -e "\n\e[36mSTEP 3: Creating a proposal\e[0m"
echo "proposal create --ccl-file examples/ccl/budget.ccl --title 'Q3 Budget Allocation' --output $OUTPUT_DIR/proposal.json"

cat << 'EOF' > "$OUTPUT_DIR/proposal.json"
{
  "id": "proposal-a1b2c3d4",
  "wasm_cid": "wasm-1234567890abcdef",
  "ccl_cid": "ccl-0987654321fedcba",
  "state": "Created",
  "quorum_status": "Pending"
}
EOF

echo -e "\e[32mProposal created:\e[0m"
cat "$OUTPUT_DIR/proposal.json"

# Step 4: Vote on the proposal
echo -e "\n\e[36mSTEP 4: Voting on the proposal\e[0m"
echo "proposal vote --proposal $OUTPUT_DIR/proposal.json --direction yes --weight 3"
echo -e "\e[32mVoted YES on proposal proposal-a1b2c3d4 with weight 3\e[0m"
echo -e "\e[32mProposal has been APPROVED\e[0m"

cat << 'EOF' > "$OUTPUT_DIR/proposal.json"
{
  "id": "proposal-a1b2c3d4",
  "wasm_cid": "wasm-1234567890abcdef",
  "ccl_cid": "ccl-0987654321fedcba",
  "state": "Approved",
  "quorum_status": "MajorityReached"
}
EOF

# Step 5: Execute the proposal
echo -e "\n\e[36mSTEP 5: Executing the proposal\e[0m"
echo "runtime execute --wasm $OUTPUT_DIR/budget.wasm --proposal $OUTPUT_DIR/proposal.json --receipt $OUTPUT_DIR/receipt.json"
echo -e "\e[32mExecution successful!\e[0m"
echo "Fuel used: 1234"
echo "Host calls: 5"
echo "IO bytes: 256"
echo "Anchored CIDs:"
echo "  - budget_q3_2023"
echo "Resource usage:"
echo "  - budget_allocation: 10000"
echo "  - token_mint: 100"

cat << 'EOF' > "$OUTPUT_DIR/receipt.json"
{
  "context": [
    "https://www.w3.org/2018/credentials/v1",
    "https://icn.network/credentials/execution-receipt/v1"
  ],
  "id": "urn:uuid:d9f7a89e-4c5b-47d3-8a62-1e9abc870def",
  "type": [
    "VerifiableCredential",
    "ExecutionReceiptCredential"
  ],
  "issuer": "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
  "issuance_date": "2023-10-15T15:30:00Z",
  "credential_subject": {
    "id": "proposal-a1b2c3d4",
    "proposal_id": "proposal-a1b2c3d4",
    "wasm_cid": "wasm-1234567890abcdef",
    "ccl_cid": "ccl-0987654321fedcba",
    "metrics": {
      "fuel_used": 1234,
      "host_calls": 5,
      "io_bytes": 256
    },
    "anchored_cids": [
      "budget_q3_2023"
    ],
    "resource_usage": [
      {
        "resource_type": "budget_allocation",
        "amount": 10000
      },
      {
        "resource_type": "token_mint",
        "amount": 100
      }
    ],
    "timestamp": 1697383800,
    "dag_epoch": 42,
    "receipt_cid": "receipt-9876543210fedcba"
  },
  "proof": {
    "type": "Ed25519Signature2020",
    "created": "2023-10-15T15:30:00Z",
    "verification_method": "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK#key-1",
    "proof_purpose": "assertionMethod",
    "jws": "eyJhbGciOiJFZERTQSIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..EXAMPLE_SIGNATURE"
  }
}
EOF

# Step 6: Verify the receipt
echo -e "\n\e[36mSTEP 6: Verifying the receipt\e[0m"
echo "runtime verify --receipt $OUTPUT_DIR/receipt.json"
echo -e "\e[32mReceipt verification successful!\e[0m"
echo "Receipt ID: urn:uuid:d9f7a89e-4c5b-47d3-8a62-1e9abc870def"
echo "Issuer: did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
echo "Proposal ID: proposal-a1b2c3d4"
echo "Timestamp: 1697383800"
echo "Metrics:"
echo "  Fuel used: 1234"
echo "  Host calls: 5"
echo "  IO bytes: 256"

# Summary
echo -e "\n\e[32mGovernance Lifecycle Demo Complete\e[0m"
echo -e "\e[32mSummary files in $OUTPUT_DIR:\e[0m"
echo "- proposal.json - The governance proposal (final approved state)"
echo "- receipt.json - The execution receipt (verifiable credential)"

# Make the script executable
chmod +x "$0" 