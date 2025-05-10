# Token Minting in ICN

The ICN platform includes a robust resource economics system that tracks and manages various types of resources, including tokens. This document describes how tokens can be minted and managed within the ICN ecosystem.

## Overview

Tokens in ICN represent a valuable resource that can be used for various purposes:

- Rewarding participation in governance
- Allocating resources for computational tasks
- Tracking contributions to cooperative initiatives
- Managing rights and privileges within a federation

Token minting is a privileged operation that can only be performed in a governance context, such as during the execution of an approved proposal.

## Architecture

The token minting system consists of the following components:

### 1. Economics Engine

The core component that tracks resource usage and enforces limits:

```rust
pub struct Economics {
    policy: ResourceAuthorizationPolicy,
}
```

### 2. Resource Ledger

A shared ledger that tracks resource usage by identity (DID):

```rust
// Maps (DID, ResourceType) to amount
pub type ResourceLedger = HashMap<LedgerKey, u64>;

pub struct LedgerKey {
    pub did: String,
    pub resource_type: ResourceType,
}
```

### 3. Host Functions

WebAssembly host functions that expose economic operations:

```rust
// Check if in governance context (0=no, 1=yes)
pub fn host_is_governance_context() -> i32;

// Mint tokens (governance-only)
pub fn host_mint_token(recipient_ptr: i32, recipient_len: i32, amount: u64) -> i32;
```

## Token Minting Process

### In Cooperative Contract Language (CCL)

Token minting can be expressed in CCL using the `mint_token` block:

```ccl
actions {
  on "proposal.approved" {
    mint_token {
      type "governance_token"
      amount 100
      recipient "did:icn:participant123"
    }
  }
}
```

### In WebAssembly

The CCL compiler generates WASM code that:

1. Checks if execution is in a governance context
2. If yes, calls the token minting host function
3. Passes the recipient DID and amount

```wat
;; Pseudo-WebAssembly
(call $host_is_governance_context)
(if (result i32)
  (then
    ;; Mint tokens
    (i32.const 0)          ;; Recipient DID pointer
    (i32.const 25)         ;; Recipient DID length
    (i64.const 100)        ;; Amount
    (call $host_mint_token)
  )
  (else
    ;; Not in governance context
    (i32.const -1)         ;; Error code
  )
)
```

### In the Runtime

During execution, the `governance_execute_wasm` function enables the governance context:

```rust
// Execute a WASM binary with the given context in governance mode
pub fn governance_execute_wasm(&self, wasm_bytes: &[u8], context: VmContext) -> Result<ExecutionResult> {
    // Create host environment with governance context enabled
    let host_env = ConcreteHostEnvironment::new_governance(
        Arc::new(self.context.clone()),
        context.executor_did.parse().unwrap_or_else(|_| Did::from_str("did:icn:invalid").unwrap())
    );
    
    // ... execute the WASM module ...
}
```

## Token Accounting

Tokens are accounted for in the resource ledger. When tokens are minted for a DID, it reduces their token usage in the ledger:

```rust
pub fn mint(&self, recipient: &Did, rt: ResourceType, amt: u64, ledger: &RwLock<HashMap<LedgerKey, u64>>) -> i32 {
    // Only token type can be minted
    if rt != ResourceType::Token {
        return -3;
    }
    
    // Get the current usage and subtract the amount (minting reduces usage)
    let current = l.entry(key.clone()).or_insert(0);
    
    // Check for overflow
    if *current < amt {
        *current = 0;
    } else {
        *current -= amt;
    }
    
    0 // Success
}
```

## CLI Usage

The ICN CLI provides commands for working with tokens:

```bash
# Check token balance for a DID
icn-cli ledger show --did did:icn:user123 --resource TOKEN

# Mint tokens (governance context only)
icn-cli ledger mint --did did:icn:user123 --amount 100

# Execute a WASM file in governance context
icn-cli runtime execute --wasm tokens.wasm --governance
```

## Security Considerations

- Token minting is restricted to governance contexts
- Proposals that mint tokens require approval through the governance process
- The economics engine enforces policy limits on token minting
- All token operations are recorded in the ledger for auditability

## Future Enhancements

- Token transfer between DIDs
- Token delegation for resource usage
- Time-limited token grants
- Specialized token types for different purposes

## Conclusion

The ICN token minting system provides a secure, accountable way to manage valuable resources within the cooperative platform. By restricting minting to governance contexts and tracking usage in a shared ledger, it ensures that token allocation aligns with the collective decisions of the cooperative. 