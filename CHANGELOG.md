# Changelog

All notable changes to the ICN v3 project are documented in this file.

## [v3.0.0-alpha-federation-observability] - 2023-07-15

### Added

#### Metrics Infrastructure
- Added comprehensive metrics collection using the `metrics` and `metrics-exporter-prometheus` crates
- Implemented histogram metrics for operation latency with appropriate bucket configuration
- Created counter metrics for all ledger operations with success/error status
- Added gauge metrics for entity counts and token volumes
- Implemented federation-specific labels for all metrics

#### Test Improvements
- Implemented schema-based test parallelization for PostgreSQL-backed ledger
- Added isolation of test contexts for improved reliability and speed

#### Monitoring
- Added Prometheus alert rules for latency, error rates, and resource utilization
- Created Grafana dashboard for federation operations monitoring
- Added Docker Compose setup for the complete monitoring stack
- Implemented health checks for service readiness

#### Documentation
- Comprehensive monitoring setup guide for federation operators
- Complete metrics reference documentation
- Alert tuning guide with recommended thresholds
- Dashboard usage guide with panel explanations

#### Tooling
- Added preflight validation script to verify system readiness
- Created monitoring stack setup helper script

### Fixed
- Fixed metrics gauge implementation in metrics module
- Resolved router type mismatches when integrating WebSocket routes
- Fixed proper handling of metrics counter increments

## [Unreleased]

### Added
- Runtime-Reputation Integration: Automatic reputation updates from runtime execution receipts
- Dashboard components for visualizing reputation activity and metrics
- Prometheus metrics for tracking reputation update success/failure
- Grafana dashboards for monitoring the reputation update pipeline
- Docker Compose configuration for reputation service in devnet

### Changed
- Updated dependency versions for reqwest, prometheus, and lazy_static
- Added multihash dependency with compatible version

### Fixed
- Fixed reputation integration build issues with missing dependencies
- Resolved CID version conflicts between crates
- Fixed multihash dependency and usage in reputation service
- Corrected KeyPair.did field access in reputation updater
- Added Dockerfile for the reputation service

## [0.1.0] - 2023-10-20

### Added
- Initial codebase structure
- DAG-based storage with CID addressing
- DID-based identity system
- Contract Chain Language (CCL) for governance
- Cooperative Virtual Machine (CoVM) for WASM execution
- Basic mesh networking with libp2p
- Verifiable Credentials for attestations
- TrustBundles for federation trust management

## [Earlier Changes] 