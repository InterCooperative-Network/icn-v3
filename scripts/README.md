# ICN v3 Utility Scripts

This directory contains helpful scripts for working with the ICN v3 federation platform.

## Available Scripts

### Monitoring Setup
- **`setup_monitoring_stack.sh`**: Deploys the Prometheus and Grafana monitoring stack for federation metrics.
  ```bash
  ./scripts/setup_monitoring_stack.sh
  ```

### Validation
- **`preflight.sh`**: Runs a comprehensive validation of the ICN v3 codebase and services.
  ```bash
  ./scripts/preflight.sh
  ```
  This script checks:
  - Rust workspace builds and linting
  - Test execution
  - Monitoring stack deployment
  - Metrics endpoint accessibility
  - Documentation completeness

## Usage Examples

### First-time Setup
For a new deployment:
```bash
# Start from the repository root
git clone https://github.com/your-org/icn-v3.git
cd icn-v3

# Build and validate the codebase
./scripts/preflight.sh

# Deploy monitoring (if preflight validation passes)
./scripts/setup_monitoring_stack.sh

# Start the ICN node with metrics enabled
cargo run --release --bin icn-agoranet -- --metrics-addr=0.0.0.0:8081
```

### Monitoring Only
To deploy just the monitoring stack:
```bash
./scripts/setup_monitoring_stack.sh
```

### Validation Only
To run validation without deploying services:
```bash
./scripts/preflight.sh
``` 