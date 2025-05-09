# InterCooperative Network Governance Lifecycle

This document describes the complete lifecycle of governance in the InterCooperative Network (ICN), from proposal creation to execution and verification.

## Overview

The ICN governance process follows these key steps:

1. **Propose**: Create a proposal using the Cooperative Contract Language (CCL)
2. **Vote**: Federation members vote on the proposal
3. **Execute**: Approved proposals are executed in a sandboxed environment
4. **Anchor**: Execution receipts are anchored to the distributed ledger

## Governance Flow

```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│          │     │          │     │          │     │          │
│  Propose ├────►│   Vote   ├────►│ Execute  ├────►│  Anchor  │
│          │     │          │     │          │     │          │
└──────────┘     └──────────┘     └──────────┘     └──────────┘
     │                │                │                │
     ▼                ▼                ▼                ▼
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│   CCL    │     │  Quorum  │     │  CoVM    │     │   DAG    │
│  Source  │     │  Rules   │     │ Runtime  │     │  Receipt │
└──────────┘     └──────────┘     └──────────┘     └──────────┘
```

## The Governance Phases in Detail

### 1. Propose

A proposal is created using the Cooperative Contract Language (CCL), which is a domain-specific language designed for cooperative governance. CCL enables clear and predictable governance actions.

**Example:**

```bash
# Create a new budget allocation proposal
icn-cli proposal create --ccl-file budget.ccl --title "Q2 Budget Allocation"
```

This creates a proposal file that includes:
- The proposal ID
- The CCL source
- Metadata about the proposal

### 2. Vote

Once a proposal is created, federation members can vote on it. The ICN supports different voting mechanisms:
- Simple majority
- Weighted voting
- Threshold-based consensus

**Example:**

```bash
# Vote on a proposal
icn-cli proposal vote --proposal budget_proposal.json --direction yes
```

### 3. Execute

When a proposal reaches the required quorum, it can be executed. Execution happens in the Cooperative Virtual Machine (CoVM), which:
- Provides a sandboxed environment
- Tracks resource usage
- Implements a metered execution model
- Enforces access controls

**Example:**

```bash
# Execute an approved proposal
icn-cli runtime execute --wasm budget_wasm.wasm --receipt receipt.json
```

Alternatively, you can use the complete CCL pipeline:

```bash
# Execute a CCL file directly
icn-cli runtime execute-ccl --input budget.ccl --output receipt.json
```

### 4. Anchor

After successful execution, a verifiable receipt is generated and anchored to the distributed ledger. This receipt:
- Is cryptographically signed
- Contains execution metrics
- References the original proposal
- Provides an audit trail

**Example:**

```bash
# Verify a receipt
icn-cli runtime verify --receipt receipt.json
```

## Example: Budget Proposal Lifecycle

Let's walk through a complete example using a budget allocation proposal:

### 1. Create the Budget Proposal in CCL

```ccl
# budget.ccl
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
```

### 2. Create the Proposal

```bash
$ icn-cli proposal create --ccl-file budget.ccl --title "Q2 Budget Allocation"
Proposal created: 83a7b798-e1c2-4e9d-b11b-24c5a4c366a0
Output file: budget_proposal.json
```

### 3. Vote on the Proposal

```bash
$ icn-cli proposal vote --proposal budget_proposal.json --direction yes
Vote recorded for proposal 83a7b798-e1c2-4e9d-b11b-24c5a4c366a0
Current vote tally: 1 yes, 0 no
Quorum status: Pending (3 more votes needed)
```

### 4. Check Proposal Status

```bash
$ icn-cli proposal status --proposal budget_proposal.json
Proposal: Q2 Budget Allocation
ID: 83a7b798-e1c2-4e9d-b11b-24c5a4c366a0
State: Approved
Quorum status: MajorityReached
Votes: 4 yes, 1 no
```

### 5. Execute the Proposal

```bash
$ icn-cli runtime execute-ccl --input budget.ccl --output receipt.json
Executing CCL file
Source: budget.ccl

Step 1: Compiling CCL to DSL
Compiling budget.ccl to DSL...
DSL generation successful

Step 2: Compiling DSL to WASM
Compiling DSL to WASM...
WASM compilation successful

Step 3: Executing WASM
Executing WASM in CoVM...
Generating execution receipt...
Receipt saved to receipt.json

Execution Summary
Fuel used: 1234
Host calls: 5

Receipt CID: receipt-83a7b798-e1c2-4e9d-b11b-24c5a4c366a0

CCL Execution Pipeline Complete
```

### 6. Verify the Receipt

```bash
$ icn-cli runtime verify --receipt receipt.json
Receipt verification successful
Receipt ID: urn:icn:receipt:83a7b798-e1c2-4e9d-b11b-24c5a4c366a0
Executed by: did:icn:executor
Fuel used: 1234
Anchored to DAG at epoch: 2023-06-07T12:34:56Z
Signature valid: ✓
```

## Resource Metering

The ICN implements a fuel-based resource metering system to:
- Prevent runaway computations
- Ensure fair resource allocation
- Create accountable governance
- Provide transparency of execution costs

Each operation in the VM consumes an amount of fuel:
- Basic arithmetic: 1 fuel
- Memory allocation: 2 fuel per page
- Host function calls: 10+ fuel
- Storage operations: 100+ fuel

When a contract runs out of fuel, execution is halted and a receipt is generated with the partial execution metrics.

## Conclusion

The ICN governance lifecycle provides a transparent, verifiable, and accountable system for cooperative governance. The entire process from proposal creation to execution and verification is traceable and cryptographically secured.

By anchoring execution receipts to the distributed ledger, ICN ensures a permanent record of governance decisions and their implementation, enabling reliable accountability and auditability for all network participants. 