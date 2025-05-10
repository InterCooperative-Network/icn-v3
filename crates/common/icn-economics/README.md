# ICN Economics

The `icn-economics` crate provides resource tracking, authorization, and accounting for the ICN platform.

## Overview

The economics system enables resource metering and policy enforcement for computational resources in WebAssembly execution environments. It tracks resource usage by identity (DID) and enforces authorization policies.

## Core Components

### ResourceType

An enum representing different types of resources that can be tracked:

- **CPU**: Computational resources
- **Memory**: Memory allocation
- **IO**: Input/output operations
- **Token**: General-purpose token resources

### ResourceAuthorizationPolicy

A policy configuration that defines resource limits:

- `max_cpu`: Maximum allowed CPU usage
- `max_memory`: Maximum allowed memory usage
- `token_allowance`: Maximum allowed token usage

### Economics

The main engine that enforces resource limits and tracks usage:

- `authorize(did, resource_type, amount)`: Checks if a DID is authorized to use resources
- `record(did, resource_type, amount, ledger)`: Records resource usage in the ledger
- `get_usage(did, resource_type, ledger)`: Retrieves usage for a specific DID and resource
- `get_total_usage(resource_type, ledger)`: Retrieves total usage across all DIDs

### LedgerKey

A composite key for the resource ledger that associates DIDs with resource types:

```rust
pub struct LedgerKey {
    pub did: String,
    pub resource_type: ResourceType,
}
```

## Integration with WASM Runtime

The economics system is integrated with the WASM runtime through host functions:

1. `host_check_resource_authorization`: Checks if a resource usage is authorized
2. `host_record_resource_usage`: Records resource usage in the ledger

These functions are exposed to WASM modules and can be called from within CCL files.

## Flow Diagram

```mermaid
sequenceDiagram
    participant CCL as CCL Workflow
    participant WASM as WASM Runtime
    participant Host as Host Environment
    participant Economics as Economics Engine
    participant Ledger as Resource Ledger

    CCL->>WASM: perform_metered_action()
    WASM->>Host: host_check_resource_authorization(resource, amount)
    Host->>Economics: authorize(did, resource, amount)
    Economics->>Host: authorization result
    Host->>WASM: return result
    
    WASM->>Host: host_record_resource_usage(resource, amount)
    Host->>Economics: record(did, resource, amount, ledger)
    Economics->>Ledger: update usage
    Ledger-->>Economics: updated
    Economics->>Host: recording result
    Host->>WASM: return result
```

## Usage in CCL

CCL files can use metered actions to consume resources:

```
// Check if enough resources are available
if (check_resource_authorization(ResourceType.CPU, 100)) {
  // Perform CPU-intensive operation
  perform_action("compute_hash", 100);
  
  // Record the usage
  record_resource_usage(ResourceType.CPU, 100);
}
```

## Per-Identity Resource Tracking

Resources are tracked per DID, allowing for:

- Different quotas for different identities
- Resource isolation between cooperatives
- Accountability for resource consumption

## CLI Integration

The economics system integrates with the ICN CLI:

- `icn-cli ledger show --resource Token`: Query the ledger
- `icn-cli coop mint --resource Token --amount 100`: Mint tokens for a cooperative 