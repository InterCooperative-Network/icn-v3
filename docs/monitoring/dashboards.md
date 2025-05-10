# Dashboard Usage Guide

This guide explains how to use the ICN Federation dashboard in Grafana to monitor federation health, performance, and operations.

## Accessing the Dashboard

1. Navigate to the Grafana URL (default: http://localhost:3000)
2. Log in with your credentials (default: admin/admin)
3. From the left menu, click on Dashboards
4. Select "ICN Federation Overview"

## Dashboard Overview

The ICN Federation Overview dashboard is organized into three main sections:

1. **Federation Overview** - High-level performance and throughput
2. **Entities & Balances** - Entity counts and token volumes
3. **Errors & Reliability** - Error rates and distribution

![Dashboard Overview](../images/dashboard-overview.png)

*Note: This screenshot is a placeholder. In a production deployment, you may want to add actual screenshots of your dashboards.*

## Dashboard Panels Explained

### Federation Overview Section

#### Operation Throughput (TPS)

![Throughput Panel](../images/throughput-panel.png)

**What it shows:**
- Operations per second processed by the federation, broken down by operation type
- Higher is generally better, indicating more activity

**How to use it:**
- Monitor for sudden drops, which may indicate issues
- Use for capacity planning by observing peak throughput
- Track growth trends over time

**Common patterns:**
- Daily patterns with peak/trough cycles
- Spikes during batch processing
- Gradual growth as federation usage increases

#### Operation Latency

![Latency Panel](../images/latency-panel.png)

**What it shows:**
- 95th percentile (p95) and median (p50) latency for operations
- Lower is better, indicating faster operations

**How to use it:**
- Watch for sudden increases, which indicate performance problems
- Compare p95 to p50 to understand outliers versus typical performance
- Correlate with throughput to identify saturation points

**Common patterns:**
- Latency often increases with throughput
- Database-related operations may show higher latency
- Network issues can cause sudden spikes

### Entities & Balances Section

#### Entity Counts by Type

![Entity Counts Panel](../images/entity-counts-panel.png)

**What it shows:**
- Current count of entities by type in the federation
- Indicator of federation size and composition

**How to use it:**
- Track growth of accounts and other entities
- Validate expected entity counts after bulk operations
- Use for capacity planning

#### Total Transfer Volume

![Transfer Volume Panel](../images/transfer-volume-panel.png)

**What it shows:**
- Cumulative volume of tokens transferred in the federation
- Indicator of economic activity

**How to use it:**
- Monitor for unusual changes in slope
- Track economic activity in the federation
- Verify large transfers were processed correctly

### Errors & Reliability Section

#### Error Rate by Operation

![Error Rate Panel](../images/error-rate-panel.png)

**What it shows:**
- Percentage of operations that result in errors, by operation type
- Lower is better, ideally near zero

**How to use it:**
- Watch for increases above baseline
- Identify which operations are most error-prone
- Correlate with system changes or deployments

**Common patterns:**
- Temporary spikes during deployments
- Increased errors during high load
- Operation-specific patterns (e.g., transfers vs. queries)

#### Error Distribution

![Error Distribution Panel](../images/error-distribution-panel.png)

**What it shows:**
- Distribution of errors by error type over the last 24 hours
- Helps identify the most common error sources

**How to use it:**
- Focus remediation on the most frequent error types
- Track effectiveness of error reduction efforts
- Identify new error types that appear after changes

## Creating Additional Dashboards

You can create custom dashboards for specific monitoring needs:

1. In Grafana, click the "+" icon in the left sidebar
2. Select "Dashboard"
3. Add panels using the "Add panel" button
4. Configure each panel with appropriate PromQL queries

### Recommended Custom Dashboards

- **Federation Comparison Dashboard** - Compare metrics across multiple federations
- **Entity-Focused Dashboard** - Detailed metrics for specific entity types
- **System Resource Dashboard** - Focus on node-level metrics (CPU, memory, disk)
- **SLA Compliance Dashboard** - Focus on metrics related to service level agreements

## Dashboard Time Controls

![Time Controls](../images/time-controls.png)

- **Time Range Selector** - Adjust the visible time period (e.g., last 6 hours, last 7 days)
- **Refresh Rate** - Set automatic dashboard refresh interval
- **Time Zone** - Switch between local time and UTC

## Saving and Sharing Dashboards

### Saving Changes

After customizing a dashboard:

1. Click the save icon (disk) in the top right
2. Enter a name and optional description
3. Click "Save"

### Sharing Dashboards

To share a dashboard:

1. Click the share icon in the top navigation
2. Choose from:
   - **Link** - Get a direct URL
   - **Snapshot** - Create a point-in-time snapshot
   - **Export** - Download as JSON for version control

## Troubleshooting Dashboard Issues

### No Data Showing

1. Check the time range - ensure it covers a period with data
2. Verify Prometheus data source connection
3. Check that metrics are being collected
4. Inspect the PromQL in the panel configuration

### Incomplete Data

1. Check for gaps in the metrics collection
2. Verify all federation nodes are reporting metrics
3. Check for any scrape errors in Prometheus 