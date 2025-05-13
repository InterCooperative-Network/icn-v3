---

RFC: 0020
Title: Observability & Metrics Pipeline
Author: Matt Faherty
Date: 2025-05-12
Status: Draft

# Summary

Defines the end-to-end observability architecture for ICN, covering metrics generation, collection, export, storage, dashboarding, and alerting across runtime, services, and frontend components.

# Motivation

To maintain operational excellence and rapid incident response, ICN requires a unified metrics pipeline that:

1. Captures pertinent performance and business metrics at each layer.
2. Ensures reliable transmission to a centralized store (Prometheus).
3. Provides real-time dashboards (Grafana, Recharts) and alerting rules.

# Goals

* Specify metric naming conventions and scoping by component and entity.
* Define instrumentation standards (libraries, labels, histogram buckets).
* Describe scrape, export, and retention policies.
* Outline dashboard and alert design patterns.

# Metrics Sources

1. **Runtime** (`icn-runtime`):

   * Execution latency (`runtime_execution_latency_seconds`).
   * Resource consumption (`runtime_mana_usage_total`).
   * Receipt anchoring events (`runtime_receipts_count`).
2. **Mesh Jobs Service** (`icn-mesh-jobs`):

   * Job insert latency (`jobs_insert_latency_seconds`).
   * Bid evaluation time (`jobs_bid_eval_latency_seconds`).
   * Assignment success/failure counters.
3. **Agoranet Service** (`icn-agoranet`):

   * Proposal submission rate (`agoranet_proposals_total`).
   * WebSocket connection counts.
4. **Reputation Service** (`icn-reputation`):

   * Profile fetch latency.
   * Leaderboard update rate.
5. **Frontend** (`dashboard`):

   * API request durations.
   * WebSocket disconnect/reconnects.

# Naming & Label Conventions

* Metrics use `snake_case` with component prefix: `<component>_<metric>_<unit>`.
* **Labels**: `instance`, `job_id`, `did`, `scope`, `environment`.

# Collection & Export

* **Prometheus** scrapes:

  * `:9100/metrics` on runtime and services.
  * `dashboard` via a Prometheus exporter embedded in Next.js API routes.
* **Pushgateway** for ephemeral batch jobs or CI metrics.

# Storage & Retention

* Retain raw metrics for 30 days.
* Downsampled metrics (histograms summaries) retained for 90 days.
* Use remote write to long-term storage (e.g., Cortex) for >90 days.

# Dashboards

* **Grafana**:

  * Federation Overview: mesh jobs, receipt throughput, node health.
  * Economics Dashboard: mana pools, transfer rates, policy enforcement.
  * Reputation Trends: historical scores, bid evaluation performance.
* **Dashboard UI**:

  * Recharts components tied to Prometheus via API gateway.
  * Real-time updates via WebSocket for critical KPIs.

# Alerting

* **Prometheus Alertmanager** rules:

  * High error rate (`>5%` errors over 5m).
  * Latency spikes (`p95 > threshold`).
  * Node unreachability (`no scrape for 2m`).
* **Channels**: Slack, PagerDuty, Email.

# Testing

* Integration tests to verify metric endpoints return valid Prometheus format.
* Dashboards snapshots validated in CI.
* Alert rules linted via `promtool`.

# Future Work

* Automated anomaly detection via ML on metrics.
* SLA reporting with periodic summaries.
* Synthetic monitoring (heartbeat jobs).

---
