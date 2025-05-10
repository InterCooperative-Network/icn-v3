# ICN v3 Monitoring Architecture

This document outlines the monitoring architecture for the ICN v3 platform, explaining the components, how they interact, and how they integrate with the federation structure.

## Monitoring Stack Components

The ICN v3 monitoring stack consists of the following components:

### 1. Prometheus (Metrics Collection)

Prometheus is the backbone of the monitoring system, providing time-series data collection and storage. In the ICN architecture:

- Prometheus scrapes metrics from all federation components
- It supports hierarchical labeling that matches our federation > cooperative > community structure
- It stores historical metrics with configurable retention
- It provides alerting capabilities based on threshold rules

### 2. Grafana (Visualization)

Grafana provides the visualization layer for the monitoring stack:

- Federation-specific dashboards display key performance metrics
- Resource utilization dashboards track computational resources
- Economic dashboards visualize token flows and resource allocation
- Alerting dashboards highlight potential issues

### 3. Integration with ICN Components

Each ICN component exposes metrics via HTTP endpoints:

- Federation nodes expose metrics on port 8081
- Cooperative nodes expose metrics on port 8082
- Community nodes expose metrics on port 8083

## Deployment Modes

The monitoring stack can be deployed in two modes:

### 1. Development Mode

For local development and testing, the monitoring stack can be started with Docker Compose:

```bash
./scripts/setup_monitoring_stack.sh
```

This sets up a local environment with:
- Data stored in local directories under the project root
- Default configuration for a single federation node
- Easy startup/shutdown for development

### 2. Production Mode

For production environments, the monitoring stack is deployed using systemd:

```bash
sudo ./monitoring/install_monitoring.sh --federation-id myfederation
```

This provides:
- Systemd service for automatic startup and recovery
- Configuration stored in standard Linux directories
- Data persistence in dedicated volumes
- Production-ready security settings

## Directory Structure

The monitoring stack is organized as follows:

```
monitoring/
├── docker-compose.yml           # Container configuration
├── prometheus.yml.template      # Prometheus configuration template
├── prometheus-rules.yml         # Alerting rules
├── icn-monitoring.service       # Systemd service definition
├── monitoring.conf              # Environment configuration
├── install_monitoring.sh        # Production installer
├── dashboards/                  # Grafana dashboards
│   └── federation-overview.json # Federation overview dashboard
└── grafana-provisioning/        # Grafana configuration
    ├── dashboards/              # Dashboard provisioning
    └── datasources/             # Data source configuration
```

## Configuration Portability

The monitoring stack is designed to be portable across different federation deployments:

1. **Environment Variables**: All configuration is managed through environment variables
2. **Templates**: Configuration files use templates with placeholders
3. **Data Directories**: Data locations are configurable to match your environment
4. **Federation Awareness**: All metrics are labeled with federation, cooperative, and community IDs

## Metric Structure

ICN metrics follow a hierarchical structure that reflects the organization:

- `icn_federation_*` - Federation-level metrics
- `icn_cooperative_*` - Cooperative-level metrics
- `icn_community_*` - Community-level metrics

Each metric includes labels for federation ID, cooperative ID, and community ID as applicable, enabling flexible querying and filtering.

## Federation Integration

The monitoring system integrates with the federation structure through:

1. **Automatic Discovery**: Federation components register with the monitoring system
2. **Hierarchical Labels**: Metrics are labeled with organizational hierarchy
3. **Role-Based Dashboards**: Different views for federation, cooperative, and community operators
4. **Federation-Specific Thresholds**: Alert thresholds can be customized per federation

## Security Considerations

The monitoring stack includes several security features:

1. **Authentication**: Grafana requires authentication for access
2. **TLS Support**: HTTPS can be enabled for secure communication
3. **Network Isolation**: Components run in an isolated Docker network
4. **Firewall Integration**: Only necessary ports are exposed

## Extending the Monitoring Stack

The monitoring stack can be extended in several ways:

1. **Custom Dashboards**: Add federation-specific dashboards to the `dashboards/` directory
2. **Additional Metrics**: Extend Prometheus configuration to capture new metrics
3. **Alert Rules**: Add custom alerting rules in `prometheus-rules.yml`
4. **External Integration**: Connect to external monitoring systems via Prometheus exporters

## Conclusion

The ICN v3 monitoring stack provides a comprehensive, federation-aware monitoring solution that scales with your deployment. By leveraging industry-standard tools like Prometheus and Grafana, it delivers powerful metrics collection, visualization, and alerting capabilities while maintaining the hierarchical structure of the ICN platform. 