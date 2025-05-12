# ICN Runtime-Reputation Integration

This document explains how the ICN Runtime automatically submits reputation updates based on execution receipts.

## Overview

When the ICN Runtime executes jobs and creates execution receipts, it can now automatically submit these receipts to the reputation service. This creates a direct feedback loop between execution and reputation, ensuring that node reputation scores accurately reflect actual execution performance.

## Key Components

1. **ReputationUpdater**: A trait defining the interface for submitting reputation updates.
   - `HttpReputationUpdater`: Implementation that sends HTTP requests to the reputation service
   - `NoopReputationUpdater`: No-op implementation for testing or when reputation updates are disabled

2. **Runtime**: Enhanced to include an optional reputation updater.
   - Automatically submits reputation records when a receipt is anchored

3. **RuntimeContextBuilder**: Allows configuring the reputation service URL.
   - `with_reputation_service(url)`: Sets the reputation service URL for automatic updates

## Configuring Reputation Integration

To enable automatic reputation updates, configure the RuntimeContext with both identity and reputation service:

```rust
let context = RuntimeContextBuilder::new()
    .with_identity(keypair)
    .with_reputation_service("http://reputation-service:8081/reputation")
    .build();

let runtime = Runtime::with_context(storage, context);
```

## Environment Variables

The reputation service URL can be configured with environment variables:

- `ICN_REPUTATION_SERVICE_URL`: The base URL of the reputation service (default: `http://localhost:8081/reputation`)

## Metrics

The following Prometheus metrics are exposed to monitor the reputation integration:

- `icn_runtime_reputation_updates_total`: Counter of total reputation update attempts
- `icn_runtime_reputation_updates_success`: Counter of successful reputation updates
- `icn_runtime_reputation_updates_failure`: Counter of failed reputation updates

## Fault Tolerance

The integration is designed to be fault-tolerant:

- If the reputation service is unavailable, receipt anchoring still succeeds
- Failed reputation updates are logged but don't affect the primary execution path
- All reputation-related errors are caught and reported via logs and metrics

## Testing

Use the `NoopReputationUpdater` for testing environments where reputation updates should be skipped:

```rust
let noop_updater = Arc::new(NoopReputationUpdater);
let runtime = Runtime::new(storage).with_reputation_updater(noop_updater);
```

For integration testing, use the `MockReputationUpdater` provided in the test module.

## Implementation Considerations

- Reputation updates are non-blocking and asynchronous
- The reputation service should be idempotent to handle potential duplicates
- Reputation failures don't affect the primary execution path 