#!/usr/bin/env pwsh
# ICN Governance Workflow Demo
# This demonstrates the complete governance lifecycle from proposal to execution

# Create output directory
$OutputDir = "examples/output"
if (!(Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir
}

# Step 1: Compile CCL to DSL
Write-Host "STEP 1: Compiling CCL to DSL" -ForegroundColor Cyan
Write-Host "ccl compile-to-dsl --input examples/ccl/budget.ccl --output $OutputDir/budget.dsl"
Write-Host "DSL file would contain:"
Write-Host "-----------------------" -ForegroundColor Gray
Write-Host @"
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
"@ -ForegroundColor DarkGray

# Step 2: Compile DSL to WASM
Write-Host "`nSTEP 2: Compiling DSL to WASM" -ForegroundColor Cyan
Write-Host "ccl compile-to-wasm --input examples/ccl/budget.ccl --output $OutputDir/budget.wasm"
Write-Host "WASM binary would be generated"

# Step 3: Create a proposal
Write-Host "`nSTEP 3: Creating a proposal" -ForegroundColor Cyan
Write-Host "proposal create --ccl-file examples/ccl/budget.ccl --title 'Q3 Budget Allocation' --output $OutputDir/proposal.json"

$proposalJson = @"
{
  "id": "proposal-a1b2c3d4",
  "wasm_cid": "wasm-1234567890abcdef",
  "ccl_cid": "ccl-0987654321fedcba",
  "state": "Created",
  "quorum_status": "Pending"
}
"@

$proposalJson | Out-File -FilePath "$OutputDir/proposal.json"
Write-Host "Proposal created:" -ForegroundColor Green
Write-Host $proposalJson -ForegroundColor DarkGray

# Step 4: Vote on the proposal
Write-Host "`nSTEP 4: Voting on the proposal" -ForegroundColor Cyan
Write-Host "proposal vote --proposal $OutputDir/proposal.json --direction yes --weight 3"
Write-Host "Voted YES on proposal proposal-a1b2c3d4 with weight 3" -ForegroundColor Green
Write-Host "Proposal has been APPROVED" -ForegroundColor Green

$approvedProposalJson = @"
{
  "id": "proposal-a1b2c3d4",
  "wasm_cid": "wasm-1234567890abcdef",
  "ccl_cid": "ccl-0987654321fedcba",
  "state": "Approved",
  "quorum_status": "MajorityReached"
}
"@

$approvedProposalJson | Out-File -FilePath "$OutputDir/proposal.json"

# Step 5: Execute the proposal
Write-Host "`nSTEP 5: Executing the proposal" -ForegroundColor Cyan
Write-Host "runtime execute --wasm $OutputDir/budget.wasm --proposal $OutputDir/proposal.json --receipt $OutputDir/receipt.json"
Write-Host "Execution successful!" -ForegroundColor Green
Write-Host "Fuel used: 1234" -ForegroundColor DarkGray
Write-Host "Host calls: 5" -ForegroundColor DarkGray
Write-Host "IO bytes: 256" -ForegroundColor DarkGray
Write-Host "Anchored CIDs:" -ForegroundColor DarkGray
Write-Host "  - budget_q3_2023" -ForegroundColor DarkGray
Write-Host "Resource usage:" -ForegroundColor DarkGray
Write-Host "  - budget_allocation: 10000" -ForegroundColor DarkGray
Write-Host "  - token_mint: 100" -ForegroundColor DarkGray

$receiptJson = @"
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
"@

$receiptJson | Out-File -FilePath "$OutputDir/receipt.json"

# Step 6: Verify the receipt
Write-Host "`nSTEP 6: Verifying the receipt" -ForegroundColor Cyan
Write-Host "runtime verify --receipt $OutputDir/receipt.json"
Write-Host "Receipt verification successful!" -ForegroundColor Green
Write-Host "Receipt ID: urn:uuid:d9f7a89e-4c5b-47d3-8a62-1e9abc870def" -ForegroundColor DarkGray
Write-Host "Issuer: did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK" -ForegroundColor DarkGray
Write-Host "Proposal ID: proposal-a1b2c3d4" -ForegroundColor DarkGray
Write-Host "Timestamp: 1697383800" -ForegroundColor DarkGray
Write-Host "Metrics:" -ForegroundColor DarkGray
Write-Host "  Fuel used: 1234" -ForegroundColor DarkGray
Write-Host "  Host calls: 5" -ForegroundColor DarkGray
Write-Host "  IO bytes: 256" -ForegroundColor DarkGray

# Summary
Write-Host "`nGovernance Lifecycle Demo Complete" -ForegroundColor Green
Write-Host "Summary files in $OutputDir:" -ForegroundColor Green
Write-Host "- proposal.json - The governance proposal (final approved state)" -ForegroundColor DarkGray
Write-Host "- receipt.json - The execution receipt (verifiable credential)" -ForegroundColor DarkGray 