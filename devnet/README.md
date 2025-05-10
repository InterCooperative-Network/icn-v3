# ICN v3 Federation Test Deployment

This directory contains scripts and configuration for deploying a complete ICN v3 federation test environment with monitoring.

## Overview

The test deployment includes:

- Multiple ICN Agoranet nodes in a federation
- Fully configured PostgreSQL database backends
- Complete monitoring stack (Prometheus + Grafana)
- Load generation tools for testing
- Network simulation utilities

## Quick Start

```bash
# Deploy the entire test federation with monitoring
./deploy.sh

# Open the federation dashboard
open http://localhost:3000/d/icn-federation-overview/icn-federation-overview

# Generate test load (transfers, entity creation)
./generate_load.sh
```

## Architecture

The test federation deployment creates the following components:

```
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ ICN Node 1      │  │ ICN Node 2      │  │ ICN Node 3      │
│                 │  │                 │  │                 │
│ ┌─────────────┐ │  │ ┌─────────────┐ │  │ ┌─────────────┐ │
│ │ PostgreSQL  │ │  │ │ PostgreSQL  │ │  │ │ PostgreSQL  │ │
│ └─────────────┘ │  │ └─────────────┘ │  │ └─────────────┘ │
└─────────────────┘  └─────────────────┘  └─────────────────┘
        │                    │                    │
        └───────────────────┼────────────────────┘
                            │
                    ┌───────────────┐
                    │  Monitoring   │
                    │               │
                    │ ┌─────────┐   │
                    │ │Prometheus│  │
                    │ └─────────┘   │
                    │               │
                    │ ┌─────────┐   │
                    │ │ Grafana │   │
                    │ └─────────┘   │
                    └───────────────┘
```

## Components

### Federation Nodes

The deployment includes multiple federation nodes, each with:
- ICN Agoranet service with metrics enabled
- Dedicated PostgreSQL database
- Network simulation capabilities

### Monitoring

The monitoring stack includes:
- Prometheus server scraping all federation nodes
- Pre-configured alert rules
- Grafana with federation dashboards

### Load Generation

The test environment includes tools for:
- Generating realistic transfer patterns
- Simulating entity creation/deletion
- Creating error conditions for alert testing

## Configuration

### Node Configuration

Each node is configured via `config/node-{N}.toml`:

```toml
[federation]
federation_id = "test-federation"
node_id = "node-1"

[metrics]
metrics_addr = "0.0.0.0:8081"
```

### Monitoring Configuration

Prometheus is configured to scrape all federation nodes:

```yaml
scrape_configs:
  - job_name: 'icn-agoranet'
    scrape_interval: 15s
    static_configs:
      - targets: ['icn-node-1:8081', 'icn-node-2:8081', 'icn-node-3:8081']
```

## Usage Examples

### Starting the Test Environment

```bash
# Start with default configuration
./deploy.sh

# Start with custom node count
./deploy.sh --nodes 5

# Start with network simulation
./deploy.sh --network-delay 50ms
```

### Running Tests

```bash
# Generate continuous load
./generate_load.sh --tps 100

# Generate a specific transaction pattern
./generate_load.sh --pattern spikes

# Run the full test suite
./run_tests.sh
```

### Monitoring

```bash
# View federation metrics
open http://localhost:9090

# Access Grafana dashboards
open http://localhost:3000
```

## Extending

To add components to the test deployment:

1. Add container definitions to `docker-compose.yml`
2. Update configuration in `config/`
3. Modify the deployment script as needed 