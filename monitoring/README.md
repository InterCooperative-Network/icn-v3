# ICN Federation Monitoring Stack

This directory contains the monitoring stack configuration for the ICN Federation. It includes:

- Prometheus for metrics collection
- Grafana for visualization dashboards
- Systemd integration for production environments
- Portable configuration for federation deployment

## Quick Setup

The simplest way to set up the monitoring stack is by running:

```bash
./scripts/setup_monitoring_stack.sh
```

This will start the monitoring stack using Docker Compose in development mode.

## Manual Setup

To start the monitoring stack manually:

```bash
cd monitoring
docker compose up -d
```

## Systemd Deployment

For production environments, you should deploy the monitoring stack as a systemd service:

### Automated Installation (Recommended)

For a quick and portable installation, use the provided installer script:

```bash
sudo ./install_monitoring.sh --federation-id myfederation --federation-name "My Federation"
```

The script supports the following options:

```
Usage: ./install_monitoring.sh [OPTIONS]
Install ICN Monitoring Stack

Options:
  --install-dir DIR        Installation directory (default: /opt/icn/monitoring)
  --data-dir DIR           Data directory (default: /var/lib/icn)
  --config-dir DIR         Config directory (default: /etc/icn)
  --federation-id ID       Federation ID (default: default-federation)
  --federation-name NAME   Federation name
  --prometheus-port PORT   Prometheus port (default: 9090)
  --grafana-port PORT      Grafana port (default: 3000)
  --federation-endpoints E Federation metrics endpoints (comma-separated)
  --help                   Display this help message
```

This installer will:
1. Create necessary directories with proper permissions
2. Copy and configure all monitoring components
3. Create a systemd service for automatic startup
4. Generate configuration based on your federation settings
5. Start and verify the monitoring services

### Manual Installation

If you prefer to install components manually:

```bash
# Copy files to the installation directory
sudo mkdir -p /opt/icn/monitoring
sudo cp -r * /opt/icn/monitoring/

# Create data directories
sudo mkdir -p /var/lib/icn/prometheus
sudo mkdir -p /var/lib/icn/grafana

# Create environment configuration
sudo mkdir -p /etc/icn
sudo cp monitoring.conf /etc/icn/

# Install the systemd service
sudo cp icn-monitoring.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable icn-monitoring.service
sudo systemctl start icn-monitoring.service
```

### Verify Installation

```bash
# Check service status
sudo systemctl status icn-monitoring.service

# Verify Prometheus is running
curl http://localhost:9090/-/ready

# Verify Grafana is running
curl http://localhost:3000/api/health
```

## Accessing the Dashboards

- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (default login: admin/admin)

## Configuring Federation Metrics

Edit the environment configuration to add your federation nodes and services:

```bash
sudo nano /etc/icn/monitoring.conf
```

Update the endpoints:

```
# Federation metrics endpoints
FEDERATION_ENDPOINTS=node1:8081,node2:8081
COOPERATIVE_ENDPOINTS=coop1:8082,coop2:8082
COMMUNITY_ENDPOINTS=community1:8083
```

Then restart the service:

```bash
sudo systemctl restart icn-monitoring.service
```

## Available Dashboards

- **ICN Federation Overview** - Shows key metrics for your federation
- **ICN Cooperative Performance** - Details on cooperative operations
- **ICN Community Activity** - Community-level metrics

## Troubleshooting

- Check container logs: `docker logs icn-prometheus` or `docker logs icn-grafana`
- Check systemd logs: `journalctl -u icn-monitoring.service`
- Verify configurations: `docker compose config`
- Check environment configuration: `cat /etc/icn/monitoring.conf`

## Security Considerations

For production deployments, consider these security recommendations:

1. Change default Grafana credentials in the environment config
2. Enable TLS for Prometheus and Grafana
3. Configure authentication for metrics endpoints
4. Apply appropriate firewall rules

See the [ICN Security Guide](../onboarding/docs/security_and_recovery.md) for more details. 