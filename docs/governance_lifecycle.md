# ICN Governance Lifecycle

This document outlines the complete lifecycle of governance proposals in the InterCooperative Network (ICN), from creation to execution and anchoring in the DAG.

## Overview

The ICN governance system is built on principles of transparency, verifiability, and decentralization. The entire process is cryptographically secured and anchored on-chain, with all steps being independently verifiable.

The governance lifecycle consists of these key phases:

1. **Proposal Creation** - Authoring and submitting a proposal in CCL format
2. **Deliberation & Voting** - Quorum-based voting on the proposal
3. **Execution** - Running the approved proposal in the CoVM runtime
4. **Receipt Issuance** - Generating a verifiable credential proving execution
5. **DAG Anchoring** - Anchoring the receipt to the federation DAG

## Proposal Lifecycle Stages

### 1. Proposal Creation

A governance proposal starts as a CCL (Cooperative Constitutional Language) file, which defines the proposed governance action using a domain-specific language designed for cooperative governance.

1. Author creates a `.ccl` file defining the proposal
2. The proposal is compiled to intermediate DSL, then to WASM
3. The proposal is submitted to the federation via the AgoraNet p2p network
4. Both source CCL and compiled WASM are stored with CIDs

**Example using the CLI:**

```bash
# Create a new proposal from a CCL file
icn-cli proposal create --ccl-file budget_allocation.ccl --title "Q3 Budget Allocation" --output proposal.json
```

### 2. Deliberation & Voting

Once a proposal is created, it enters the deliberation and voting phase:

1. Federation members discuss the proposal via AgoraNet threads
2. Members cast weighted votes based on their governance stake
3. Votes are tallied according to the quorum model (Majority, Threshold, or Weighted)
4. Once quorum is reached, the proposal is marked as Approved or Rejected

**Example using the CLI:**

```bash
# Vote on a proposal
icn-cli proposal vote --proposal proposal.json --direction yes --weight 3

# Check the status of a proposal
icn-cli proposal status --proposal proposal.json
```

### 3. Execution

If approved, the proposal moves to the execution phase:

1. The compiled WASM module is loaded into the CoVM (Cooperative VM)
2. The execution runs in a sandboxed environment with:
   - Metered resource usage (fuel)
   - Host ABI access for controlled operations
   - Deterministic execution
3. During execution, the runtime can:
   - Anchor data to the federation DAG
   - Perform metered actions based on authorization
   - Record resource usage

**Example using the CLI:**

```bash
# Execute a proposal
icn-cli runtime execute --wasm proposal.wasm --proposal proposal.json --receipt receipt.json
```

### 4. Receipt Issuance

After successful execution, the system generates an ExecutionReceipt:

1. The receipt is a Verifiable Credential containing:
   - Proposal ID and execution metadata
   - Resource usage metrics
   - Anchored CIDs
   - Timestamp and DAG epoch
2. The receipt is signed by the executing federation
3. The receipt itself receives a CID

### 5. DAG Anchoring

Finally, the receipt is anchored to the federation DAG:

1. The receipt CID is added to the DAG
2. This creates an immutable, time-ordered record of execution
3. The anchoring provides proof of federation consensus
4. Any validator can verify the execution by:
   - Verifying the receipt signature
   - Re-executing the WASM to confirm deterministic output
   - Checking the DAG for proper anchoring

**Example using the CLI:**

```bash
# Verify a receipt
icn-cli runtime verify --receipt receipt.json
```

## Verification & Security

The entire process is designed to be independently verifiable:

- All artifacts (CCL, WASM, receipts) have deterministic CIDs
- Signatures can be verified against federated DIDs
- DAG anchoring provides timestamp proofs
- Execution is deterministic and can be replayed
- Resource metering prevents abuse

This ensures that all governance actions are transparent, accountable, and fully auditable by any participant in the network.

## Integrating with External Systems

The ExecutionReceipt system enables integration with external systems:

1. Wallets can verify receipts to confirm governance actions
2. External services can watch for specific DAG anchors
3. On-chain systems can react to verified governance decisions
4. Federation members can audit historical governance actions

This creates a secure bridge between cooperative governance and external execution environments.

## Testing and Simulation

For testing purposes, the CLI provides tools to simulate the entire governance lifecycle:

```bash
# Test workflow:

# 1. Compile CCL to DSL
icn-cli ccl compile-to-dsl --input test_proposal.ccl --output test_proposal.dsl

# 2. Compile DSL to WASM
icn-cli ccl compile-to-wasm --input test_proposal.ccl --output test_proposal.wasm

# 3. Execute the WASM
icn-cli runtime execute --wasm test_proposal.wasm --receipt test_receipt.json

# 4. Verify the receipt
icn-cli runtime verify --receipt test_receipt.json
``` 