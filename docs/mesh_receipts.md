# Mesh Receipts: Verifiable Compute Anchoring

This document explains the verifiable compute anchoring system in ICN, which allows nodes to submit cryptographically signed receipts of computation for inclusion in the DAG.

## Overview

Mesh Receipts provide:

1. **Verifiable proof of computation**: Cryptographically signed receipts verify that a specific executor performed a computation.
2. **Resource accounting**: Detailed resource usage tracking for economic incentives.
3. **Anchoring in the DAG**: Receipts become first-class nodes in the DAG, enabling querying and verification.

## Components

### ExecutionReceipt

```rust
pub struct ExecutionReceipt {
    pub task_cid: String,          // CID of the executed task
    pub executor: Did,             // DID of the executor node
    pub resource_usage: HashMap<ResourceType, u64>, // Resources consumed
    pub timestamp: DateTime<Utc>,  // When execution completed
    pub signature: Vec<u8>,        // Signature by executor
}
```

### Receipt Signing Process

1. A node performs a computation (WASM execution, etc.)
2. The node measures resource usage and creates an `ExecutionReceipt`
3. The node signs the receipt with its DID's private key
4. The signed receipt is anchored in the DAG via `host_anchor_receipt`

```rust
// Sign a receipt
let signature = sign_receipt(&receipt, &keypair)?;
receipt.signature = signature.to_bytes().to_vec();
```

### Anchoring Flow

When a receipt is submitted through the Host ABI, the following steps occur:

1. **Serialization**: The receipt is serialized as CBOR.
2. **Authentication**: Signature is verified against the executor's public key.
3. **CID Generation**: A deterministic CID is generated for the receipt.
4. **Node Creation**: A ReceiptNode wraps the receipt with metadata.
5. **DAG Insertion**: The ReceiptNode is stored in the DAG.

```rust
// Anchor a receipt from WASM
let result = host_anchor_receipt(receipt_ptr, receipt_len);
if result != 0 {
    // Handle error
}
```

## Host-ABI Interface

The `host_anchor_receipt` function exposes the anchoring capability to WASM modules:

```rust
/// Anchor a serialized ExecutionReceipt into the DAG.
/// ptr/len: receipt bytes; returns 0 on success.
pub fn host_anchor_receipt(ptr: u32, len: u32) -> i32;
```

Return codes:
- `0`: Success
- `-1`: Deserialization error
- `-2`: Out of bounds memory access
- `-3`: Memory not available
- `-10`: Executor mismatch
- `-11`: Invalid signature
- `-12`: Serialization error
- `-13`: CID error
- `-14`: DAG store error
- `-15`: Missing federation ID

## CLI Example

```bash
# Generate a receipt for a computation
$ meshctl compute run-wasm --task-cid "bafybei..." --output-receipt receipt.cbor

# Anchor a receipt to the DAG
$ meshctl mesh anchor-receipt --receipt receipt.cbor
Receipt anchored with CID: bafybeiezuipxlw3fb...
```

## Querying Receipts

Receipts can be queried through the DAG API:

```bash
# Query receipts for a specific task
$ meshctl dag query --type Receipt --filter 'task_cid="bafybei..."'

# View receipt details
$ meshctl receipt inspect bafybeiezuipxlw3fb...
```

## Security Considerations

1. **Identity Verification**: Only nodes with valid DIDs can anchor receipts.
2. **Signature Verification**: All receipts must have valid signatures.
3. **Task Validation**: Implementations should verify the task exists before accepting receipts.
4. **Receipt De-duplication**: Systems should prevent duplicate receipts for the same task.

## Future Work

- Receipt aggregation for batched verification
- Zero-knowledge proofs for private computations
- Consensus mechanisms for receipt validation
- Incentive alignment with economic models 