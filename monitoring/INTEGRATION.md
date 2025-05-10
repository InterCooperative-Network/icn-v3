# Integrating ICN Components with Monitoring

This guide explains how to configure ICN federation components to expose metrics and integrate with the monitoring stack.

## Overview

The ICN monitoring stack collects metrics from:

1. Federation nodes
2. Cooperative nodes 
3. Community nodes
4. DAG store
5. Ledger services
6. Token operations

## Configuration Steps

### 1. Enabling Metrics Endpoints

Each ICN component needs to be configured to expose a Prometheus-compatible metrics endpoint.

#### Federation Node

Edit your federation configuration file (e.g., `/var/lib/icn/config/federation_myfederation.yaml`):

```yaml
# Add or modify these settings
monitoring:
  enabled: true
  metrics_endpoint: "0.0.0.0:8081"  # Listen on all interfaces, port 8081
  metrics_path: "/metrics"
  export_interval_seconds: 15
```

#### Cooperative Node

Edit your cooperative configuration file (e.g., `/var/lib/icn/config/cooperative_mycooperative.yaml`):

```yaml
# Add or modify these settings
monitoring:
  enabled: true
  metrics_endpoint: "0.0.0.0:8082"  # Listen on all interfaces, port 8082
  metrics_path: "/metrics"
  export_interval_seconds: 15
```

#### Community Node

Edit your community configuration file (e.g., `/var/lib/icn/config/community_mycommunity.yaml`):

```yaml
# Add or modify these settings
monitoring:
  enabled: true
  metrics_endpoint: "0.0.0.0:8083"  # Listen on all interfaces, port 8083
  metrics_path: "/metrics"
  export_interval_seconds: 15
```

### 2. Configuring Prometheus for ICN Components

Edit the `prometheus.yml` file in your monitoring directory to include your ICN components:

```yaml
scrape_configs:
  - job_name: "icn_federation"
    static_configs:
      - targets: ["federation-node:8081"]  # Replace with your federation node address
        labels:
          federation: "myfederation"
          instance_type: "federation"

  - job_name: "icn_cooperative"
    static_configs:
      - targets: ["cooperative-node:8082"]  # Replace with your cooperative node address
        labels:
          federation: "myfederation"
          cooperative: "mycooperative"
          instance_type: "cooperative"

  - job_name: "icn_community"
    static_configs:
      - targets: ["community-node:8083"]  # Replace with your community node address
        labels:
          federation: "myfederation"
          cooperative: "mycooperative"
          community: "mycommunity"
          instance_type: "community"
```

### 3. Setting Up Network Access

Ensure that your network configuration allows:

1. Prometheus to access the metrics endpoints of your ICN nodes
2. Your browser to access Prometheus and Grafana

For production deployments, it's recommended to:
- Use TLS encryption for metrics endpoints
- Implement authentication for metrics access
- Apply proper firewall rules

## Available Metrics

### Federation Metrics

- `icn_federation_operations_total` - Total number of operations processed
- `icn_federation_operation_latency_seconds` - Operation latency in seconds
- `icn_federation_entities_count` - Number of entities by type
- `icn_federation_errors_total` - Total number of errors by type
- `icn_federation_dag_size_bytes` - Size of the DAG store in bytes
- `icn_federation_dag_operations_total` - Total number of DAG operations
- `icn_federation_resource_usage` - Resource allocation metrics

### Cooperative Metrics

- `icn_cooperative_operations_total` - Total operations by type
- `icn_cooperative_token_supply` - Current token supply
- `icn_cooperative_token_transfers_total` - Total token transfers
- `icn_cooperative_token_transfer_volume` - Volume of token transfers
- `icn_cooperative_errors_total` - Total errors by type
- `icn_cooperative_resource_usage` - Resource allocation metrics

### Community Metrics

- `icn_community_operations_total` - Total operations by type
- `icn_community_proposals_total` - Total governance proposals
- `icn_community_proposal_votes` - Votes on governance proposals
- `icn_community_services_usage` - Usage metrics for community services
- `icn_community_errors_total` - Total errors by type
- `icn_community_resource_usage` - Resource allocation metrics

## Verification

To verify that metrics are being properly collected:

1. Navigate to Prometheus (http://localhost:9090)
2. Go to the "Status" → "Targets" page to verify targets are up
3. Use the "Graph" page to execute a test query like `icn_federation_operations_total`
4. Check Grafana (http://localhost:3000) to see if the dashboard is populated with data

## Troubleshooting

### No Data in Dashboards

1. Verify ICN components are running and configured for metrics
2. Check Prometheus targets are up in the Prometheus UI
3. Verify network connectivity between Prometheus and ICN nodes
4. Check for any errors in Prometheus logs: `docker logs icn-prometheus`

### Cannot Access Metrics Endpoints

1. Verify the ICN component is running
2. Check if the metrics endpoint is correctly configured and enabled
3. Verify network accessibility (firewall rules, etc.)
4. Try accessing the metrics endpoint directly (e.g., `curl http://federation-node:8081/metrics`)

### Dashboard Shows Partial Data

1. Check which specific metrics are missing
2. Verify that the corresponding ICN component is properly configured
3. Check Prometheus for any scrape errors for that component
4. Verify the dashboard is configured to use the correct metrics 