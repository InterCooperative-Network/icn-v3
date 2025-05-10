# Monitoring Setup Guide

This guide will walk you through deploying and configuring the ICN federation monitoring stack.

## Prerequisites

- Docker and Docker Compose installed
- ICN Agoranet node(s) running and configured to expose metrics
- Network connectivity between monitoring system and ICN nodes
- Basic familiarity with Prometheus and Grafana concepts

## Quick Start

The ICN monitoring stack is packaged as a Docker Compose configuration in the `monitoring/` directory. To deploy:

1. Clone the repository (if you haven't already):
   ```
   git clone https://github.com/your-org/icn-v3.git
   cd icn-v3
   ```

2. Ensure the metrics port is accessible on your ICN Agoranet instance(s).

3. Deploy the monitoring stack:
   ```
   cd monitoring
   docker-compose up -d
   ```

4. Verify the services are running:
   ```
   docker-compose ps
   ```

5. Access the interfaces:
   - Prometheus: http://localhost:9090
   - Grafana: http://localhost:3000 (default login: admin/admin)

## Configuration

### Configuring Metrics Endpoints

Edit the `monitoring/prometheus.yml` file to add or update the targets for your ICN Agoranet instances:

```yaml
scrape_configs:
  - job_name: 'icn-agoranet'
    scrape_interval: 15s
    static_configs:
      - targets: ['icn-node-1:8081', 'icn-node-2:8081']
        labels:
          env: 'production'
```

Replace `icn-node-1:8081` with the actual hostname/IP and port where your ICN nodes expose metrics.

### Configuring Alert Manager

1. Create or edit `monitoring/alertmanager.yml`:

```yaml
route:
  group_by: ['alertname', 'federation']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  receiver: 'default'

receivers:
- name: 'default'
  email_configs:
  - to: 'alerts@example.com'
```

2. Update the Docker Compose file to include AlertManager:

```yaml
alertmanager:
  image: prom/alertmanager:v0.24.0
  container_name: icn-alertmanager
  volumes:
    - ./alertmanager.yml:/etc/alertmanager/alertmanager.yml
  ports:
    - "9093:9093"
  restart: unless-stopped
  networks:
    - monitoring
```

3. Restart the monitoring stack:
   ```
   docker-compose down
   docker-compose up -d
   ```

## Persistent Storage

By default, the Docker Compose setup includes volumes for Prometheus and Grafana data. For production deployments, consider:

1. Using host directories instead of Docker volumes
2. Setting up regular backups of the data
3. Configuring retention settings in Prometheus

Example host directory mount:

```yaml
prometheus:
  volumes:
    - /data/prometheus:/prometheus
```

## Security Considerations

For production deployments:

1. Enable authentication for Prometheus (using a reverse proxy)
2. Change the default Grafana admin password
3. Use HTTPS with valid certificates
4. Configure network security to restrict access to monitoring endpoints
5. Set up proper authorization in Grafana

## Troubleshooting

### Common Issues

1. **Can't connect to metrics endpoint**
   - Verify the ICN node is running
   - Check firewall rules
   - Ensure metrics are enabled in ICN configuration

2. **No data in Grafana**
   - Verify Prometheus is scraping targets successfully
   - Check Prometheus target status page
   - Verify Grafana data source is configured correctly

3. **Alerts not firing**
   - Check Prometheus rules configuration
   - Verify alert conditions are met
   - Inspect the AlertManager configuration

For more assistance, please consult the [ICN Federation support channels](https://icn.xyz/support). 