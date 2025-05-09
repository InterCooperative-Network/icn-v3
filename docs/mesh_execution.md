# Planetary Mesh Compute Execution

This document describes the distributed compute execution model for the InterCooperative Network (ICN) using the Planetary Mesh.

## Architecture Overview

The Planetary Mesh is a distributed compute network built on top of the ICN's trust framework. It provides a way for cooperatives to:

1. Submit compute jobs to the network
2. Bid on and execute jobs
3. Verify execution results via DAG anchoring
4. Enforce economic policies using resource tokens

```
┌───────────────┐       ┌──────────────┐       ┌────────────────┐
│               │       │              │       │                │
│  CCL Program  │──────▶│  WASM Module │──────▶│  Job Manifest  │
│               │       │              │       │                │
└───────────────┘       └──────────────┘       └────────────────┘
                                                       │
                                                       ▼
┌───────────────────────────────────────────────────────────────────┐
│                                                                   │
│                       Planetary Mesh Network                      │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
       │                        │                         │
       ▼                        ▼                         ▼
┌─────────────┐         ┌─────────────┐           ┌─────────────┐
│             │         │             │           │             │
│   Node 1    │         │   Node 2    │           │   Node 3    │
│             │         │             │           │             │
└─────────────┘         └─────────────┘           └─────────────┘
       │                        │                         │
       └────────────────────────┼─────────────────────────┘
                                │
                                ▼
                        ┌───────────────┐
                        │               │
                        │ DAG Anchoring │
                        │               │
                        └───────────────┘
                                │
                                ▼
                       ┌─────────────────┐
                       │                 │
                       │ Federation Node │
                       │                 │
                       └─────────────────┘
```

## Core Components

### 1. Resource Tokens

Resource tokens represent computational resources that can be used within the network. Each token has:

- `resource_type`: The type of resource (compute, storage, bandwidth)
- `amount`: The quantity of the resource
- `scope`: The context in which the resource can be used
- `expires_at`: Optional expiration timestamp
- `issuer`: Optional DID of the token issuer

Resource authorization policies control how these tokens are used:

- `AllowAll`: Permit all access
- `Quota(u64)`: Enforce a maximum usage limit
- `RateLimit`: Limit usage rate over time
- `PermitList`: Allow only specific DIDs

### 2. Job Manifest

A job manifest describes a computation task:

- Unique identifier
- Submitter DID
- Description
- WASM module CID
- Resource requirements
- Priority level
- Resource token
- Trust requirements

### 3. Execution System

Jobs are processed through a bidding system:

1. Job is submitted to the network
2. Nodes submit bids based on resource availability
3. Submitter or automated system selects a winning bid
4. Selected node executes the WASM module
5. Results and receipt are anchored to the DAG
6. Federation nodes verify and store the receipt

### 4. CCL Integration

CCL (Cooperative Constitutional Language) can trigger compute tasks:

```ccl
execution {
  // Submit a computation job
  submit_job(
    wasm_cid: "bafybeih7q27itb576mtmy5yzggkfzqnfj5dis4h2og6epvyvjyvcedwmze",
    description: "Data processing task",
    resource_type: "compute",
    resource_amount: 1000,
    priority: "medium"
  );
}
```

The CCL compiler translates this to a WASM module that uses the `host_submit_job` host function.

## Workflow Example

1. **Job Creation and Submission**

   ```bash
   $ meshctl submit-job --wasm example.wasm --description "Data analysis" --resource-amount 500
   Job submitted successfully
   Job ID: 3a9f2b8c-d147-42e1-8f39-9875d2e9f6a7
   ```

2. **Bidding Process**

   ```bash
   $ meshctl get-bids --job-id 3a9f2b8c-d147-42e1-8f39-9875d2e9f6a7
   
   Node ID         Bid      Est. Time     Reputation    Location
   ----------------------------------------------------------------
   mesh-node-2     80       120s          95            us-east
   mesh-node-3     90       100s          88            eu-west
   ```

3. **Execution**

   ```bash
   $ meshctl accept-bid --job-id 3a9f2b8c-d147-42e1-8f39-9875d2e9f6a7 --node-id mesh-node-2
   Bid accepted successfully
   
   Job execution progress:
   Status: Assigned
   Status: Running
   Status: Completed
   Receipt CID: receipt:4f3d7e29-6a1b-4e93-8c2d-b4e8f0d19a35
   Receipt anchored to DAG ✓
   Receipt verified by federation ✓
   ```

4. **Verify Results**

   The execution receipt contains:
   - Job ID
   - Executor node ID and DID
   - Execution metrics
   - Resource usage
   - Timestamps
   - Receipt CID in the DAG
   - Federation verification status

## Economic Model

1. **Resource Allocation**: Cooperatives can allocate resources for specific purposes
2. **Metering**: All resource usage is metered and recorded
3. **Scoped Access**: Resources are scoped to specific contexts
4. **Policy Enforcement**: Policies control resource usage
5. **Bidding System**: Market-based approach for resource allocation

## Security Model

1. **Trust Framework**: Built on ICN's identity and credential system
2. **Verifiable Execution**: All results are anchored and verifiable
3. **Authorization**: Resource usage is authorized via policy enforcement
4. **Transparency**: All nodes can verify execution receipts
5. **Federation Oversight**: Federation nodes provide additional verification

## Future Extensions

1. **Specialized Compute**: Support for GPU, ML accelerators, etc.
2. **Data Locality**: Optimize job placement based on data location
3. **Privacy-Preserving Computation**: Confidential computing via TEEs
4. **Reputation System**: Enhanced reputation tracking for nodes
5. **Composable Jobs**: Allow jobs to spawn sub-jobs for complex workflows 