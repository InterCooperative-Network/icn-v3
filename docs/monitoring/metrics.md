# ICN Metrics Reference

This document provides a comprehensive reference for all metrics exposed by ICN Agoranet nodes.

## Metrics Naming Convention

All ICN Agoranet metrics follow a consistent naming convention:

```
icn_agoranet_<metric_name>_<unit>
```

For example: `icn_agoranet_transfer_latency_seconds`

## Core Metrics

### Operation Latency Metrics

| Metric Name | Type | Description | Labels | Unit |
|-------------|------|-------------|--------|------|
| `icn_agoranet_transfer_latency_seconds` | Histogram | Measures the time taken to complete ledger operations | `operation`, `federation`, `entity_type` | seconds |

The latency histogram uses the following buckets: 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0 seconds.

Example PromQL:
```
histogram_quantile(0.95, sum(rate(icn_agoranet_transfer_latency_seconds_bucket[5m])) by (le, operation, federation))
```

### Operation Counters

| Metric Name | Type | Description | Labels | Unit |
|-------------|------|-------------|--------|------|
| `icn_agoranet_operations_total` | Counter | Counts the number of operations performed | `operation`, `federation`, `entity_type`, `status`, `error_type` (if status="error") | count |

Example PromQL:
```
sum(rate(icn_agoranet_operations_total{status="success"}[5m])) by (operation, federation)
```

### Entity Metrics

| Metric Name | Type | Description | Labels | Unit |
|-------------|------|-------------|--------|------|
| `icn_agoranet_entities_count` | Gauge | Current count of entities by type | `federation`, `entity_type` | count |
| `icn_agoranet_federation_volume_total` | Gauge | Total token volume in the federation | `federation` | token units |

Example PromQL:
```
sum(icn_agoranet_entities_count) by (federation)
```

## Label Dimensions

### Federation ID

The `federation` label identifies the specific federation instance.

Example values:
- `test-federation`
- `prod-federation-001`

### Entity Type

The `entity_type` label identifies the type of entity being measured.

Example values:
- `account`
- `asset`
- `token`

### Operation Type

The `operation` label identifies the type of operation being performed.

Standard values:
- `transfer` - Single transfers
- `batch_transfer` - Batch transfer operations
- `query` - Query operations
- `balance` - Balance check operations
- `ensure_entity` - Entity creation/validation

### Status

The `status` label indicates the outcome of operations.

Values:
- `success` - Operation completed successfully
- `error` - Operation failed (with additional `error_type` label)

## Common PromQL Queries

### Error Rate

```
sum(rate(icn_agoranet_operations_total{status="error"}[5m])) by (operation, federation) 
  / 
sum(rate(icn_agoranet_operations_total[5m])) by (operation, federation)
```

### Average Latency

```
rate(icn_agoranet_transfer_latency_seconds_sum[5m]) / rate(icn_agoranet_transfer_latency_seconds_count[5m])
```

### Throughput (Operations per Second)

```
sum(rate(icn_agoranet_operations_total{status="success"}[1m])) by (operation, federation)
```

## Extending Metrics

Federation operators can extend the metrics collection by:

1. Adding custom metrics in the application code
2. Using Prometheus recording rules to pre-compute complex queries
3. Creating custom dashboards in Grafana

### Example Recording Rule

```yaml
groups:
  - name: derived_metrics
    rules:
      - record: icn:error_rate_5m
        expr: sum(rate(icn_agoranet_operations_total{status="error"}[5m])) by (operation, federation) / sum(rate(icn_agoranet_operations_total[5m])) by (operation, federation)
``` 