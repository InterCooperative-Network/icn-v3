# ICN Federation Monitoring Stack

This directory contains the monitoring stack configuration for the ICN Federation. It includes:

- Prometheus for metrics collection
- Grafana for visualization dashboards

## Quick Setup

The simplest way to set up the monitoring stack is by running:

```bash
./scripts/setup_monitoring_stack.sh
```

This will start the monitoring stack using Docker Compose.

## Manual Setup

To start the monitoring stack manually:

```bash
cd monitoring
docker compose up -d
```

## Systemd Deployment

For production environments, you should deploy the monitoring stack as a systemd service:

### 1. Install the Systemd Service

```bash
# Copy files to the installation directory
sudo mkdir -p /home/icn/dev/icn-v3/monitoring
sudo cp -r * /home/icn/dev/icn-v3/monitoring/

# Install the systemd service
sudo cp icn-monitoring.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable icn-monitoring.service
sudo systemctl start icn-monitoring.service
```

### 2. Verify Installation

```bash
# Check service status
sudo systemctl status icn-monitoring.service

# Verify Prometheus is running
curl http://localhost:9090/-/ready

# Verify Grafana is running
curl http://localhost:3000/api/health
```

## Automated Installation

For a fully automated installation, use the provided installation script:

```bash
sudo ./install_monitoring.sh
```

This script will:
1. Create the necessary directories
2. Copy configuration files
3. Install and enable the systemd service
4. Verify the installation

## Accessing the Dashboards

- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (default login: admin/admin)

## Configuring Federation Metrics

Edit the `prometheus.yml` file to add your federation nodes and services:

```yaml
scrape_configs:
  - job_name: "icn_federation"
    static_configs:
      - targets: ["federation-node:8081"]
        labels:
          federation: "my-federation"
          instance_type: "federation"
```

For detailed instructions on integrating ICN components with monitoring, see the [Integration Guide](INTEGRATION.md).

## Available Dashboards

- **ICN Federation Overview** - Shows key metrics for your federation
- **ICN Cooperative Performance** - Details on cooperative operations
- **ICN Community Activity** - Community-level metrics

## Troubleshooting

- Check container logs: `docker logs icn-prometheus` or `docker logs icn-grafana`
- Check systemd logs: `journalctl -u icn-monitoring.service`
- Verify configurations: `docker compose config`

## Security Considerations

For production deployments, consider these security recommendations:

1. Change default Grafana credentials
2. Enable TLS for Prometheus and Grafana
3. Configure authentication for metrics endpoints
4. Apply appropriate firewall rules

See the [ICN Security Guide](../onboarding/docs/security_and_recovery.md) for more details. 