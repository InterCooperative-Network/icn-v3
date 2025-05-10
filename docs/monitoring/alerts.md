# Alert Tuning Guide

This guide helps federation operators understand, configure, and tune alerts for ICN Agoranet deployments.

## Alert Philosophy

The ICN monitoring system follows these principles for alerts:

1. **Actionable** - Alerts should indicate conditions that require human intervention
2. **Relevant** - Alerts should be tailored to the specific deployment characteristics
3. **Timely** - Alerts should fire early enough to prevent service degradation
4. **Clear** - Alert messages should clearly indicate what's wrong and suggest remediation steps

## Default Alert Rules

The default Prometheus alert rules are defined in `monitoring/prometheus-rules.yml` and are organized into two groups:

1. **icn_agoranet_alerts** - Federation operation alerts
2. **icn_agoranet_system_alerts** - System-level alerts

### Key Performance Alerts

| Alert Name | Description | Default Threshold | Severity |
|------------|-------------|-------------------|----------|
| ICNLedgerHighLatency | 95th percentile latency exceeds threshold | 500ms for 2m | warning |
| ICNLedgerCriticalLatency | 95th percentile latency exceeds critical threshold | 1s for 1m | critical |
| ICNLedgerErrorRateHigh | Error rate exceeds threshold | 5% for 5m | warning |
| ICNLedgerErrorRateCritical | Error rate exceeds critical threshold | 15% for 2m | critical |

### System Health Alerts

| Alert Name | Description | Default Threshold | Severity |
|------------|-------------|-------------------|----------|
| ICNHighCPUUsage | CPU utilization exceeds threshold | 80% for 5m | warning |
| ICNHighMemoryUsage | Memory utilization exceeds threshold | 90% for 5m | warning |
| ICNAPIEndpointDown | API endpoint is not responding | Down for 1m | critical |

### Anomaly Detection Alerts

| Alert Name | Description | Default Threshold | Severity |
|------------|-------------|-------------------|----------|
| ICNLedgerBalanceAnomaly | Unusual transfer volume growth | >10000 units/hour for 5m | warning |
| ICNLedgerEntityCountAnomaly | Unusual entity creation rate | >100 entities/hour for 5m | warning |

## Tuning Alert Thresholds

Adjust alert thresholds based on your specific deployment characteristics and service level objectives (SLOs).

### Performance Alert Tuning

To adjust latency thresholds, modify the following in `prometheus-rules.yml`:

```yaml
- alert: ICNLedgerHighLatency
  expr: histogram_quantile(0.95, sum(rate(icn_agoranet_transfer_latency_seconds_bucket[5m])) by (le, operation, federation)) > 0.5
  for: 2m
  # Change 0.5 to your desired threshold in seconds
  # Change 2m to your desired duration
```

#### Recommended Tuning Process

1. Establish performance baselines during normal operation
2. Set warning thresholds at 2x normal baseline
3. Set critical thresholds at 4x normal baseline or at SLA breach levels
4. Adjust "for" duration based on acceptable alerting sensitivity

### Error Rate Tuning

To adjust error rate thresholds:

```yaml
- alert: ICNLedgerErrorRateHigh
  expr: sum(rate(icn_agoranet_operations_total{status="error"}[5m])) by (operation, federation) / sum(rate(icn_agoranet_operations_total[5m])) by (operation, federation) > 0.05
  for: 5m
  # Change 0.05 (5%) to your desired threshold
```

#### Error Rate Guidelines

| Federation Type | Warning Threshold | Critical Threshold |
|-----------------|-------------------|-------------------|
| Production      | 1-3%              | 5-10%             |
| Staging/Test    | 5-10%             | 15-25%            |
| Development     | 10-15%            | 25-40%            |

## Alert Notification Channels

Configure AlertManager to route notifications to appropriate channels based on severity and federation:

```yaml
route:
  group_by: ['alertname', 'federation']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  receiver: 'default'
  routes:
  - match:
      severity: critical
    receiver: 'pager'
    continue: true

receivers:
- name: 'default'
  email_configs:
  - to: 'alerts@example.com'
- name: 'pager'
  pagerduty_configs:
  - service_key: '<your-pagerduty-key>'
```

## Alert Response Procedures

For each alert, we recommend creating a response procedure:

### ICNLedgerHighLatency

**Symptoms:**
- Operations taking longer than expected
- User complaints about slow transactions

**Investigation Steps:**
1. Check the dashboard for increasing latency trends
2. Verify database performance metrics
3. Check system resource utilization
4. Look for increased network latency

**Remediation:**
1. Scale up database resources if needed
2. Investigate and resolve any network issues
3. Consider load balancing or throttling if system is overloaded

## Alert Silencing and Maintenance

During planned maintenance or known issues, silence alerts using:

```
# Using Prometheus API
curl -X POST -g \
  --data '{"matchers":[{"name":"federation","value":"test-federation"}],"startsAt":"2023-01-01T15:00:00Z","endsAt":"2023-01-01T17:00:00Z","comment":"Planned maintenance"}' \
  http://localhost:9093/api/v2/silences
```

Or use the AlertManager UI to create silences. 