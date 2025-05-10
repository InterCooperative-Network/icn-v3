#!/bin/bash
set -e

# ANSI color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                ICN v3 Federation Preflight Check               ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"

# Track overall status
ERRORS=0

# Helper function for section headers
section() {
  echo -e "\n${BLUE}▶ $1${NC}"
}

# Helper function for success/failure messages
check_result() {
  if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓${NC} $1"
  else
    echo -e "${RED}✗${NC} $1"
    ERRORS=$((ERRORS + 1))
  fi
}

# Helper function to check if command exists
command_exists() {
  command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
section "Checking prerequisites"

if command_exists cargo; then
  echo -e "${GREEN}✓${NC} Rust toolchain found"
else
  echo -e "${RED}✗${NC} Rust toolchain not found. Please install Rust: https://rustup.rs/"
  exit 1
fi

if command_exists docker; then
  echo -e "${GREEN}✓${NC} Docker found"
else
  echo -e "${RED}✗${NC} Docker not found. Please install Docker: https://docs.docker.com/get-docker/"
  exit 1
fi

if command_exists docker-compose || command_exists "docker compose"; then
  echo -e "${GREEN}✓${NC} Docker Compose found"
else
  echo -e "${RED}✗${NC} Docker Compose not found. Please install Docker Compose: https://docs.docker.com/compose/install/"
  exit 1
fi

# Check workspace builds
section "Checking Rust workspace builds"

echo "Running cargo check..."
cargo check --workspace --all-targets --all-features
check_result "Cargo check passed"

echo "Running cargo build..."
cargo build --workspace --release
check_result "Cargo build passed"

echo "Running clippy..."
cargo clippy --workspace --all-targets --all-features -- -D warnings
check_result "Clippy checks passed"

# Run tests
section "Running tests in parallel"

echo "Running parallel tests..."
RUST_LOG=info cargo test --workspace --all-features -- --test-threads=8
check_result "All tests passed"

# Check monitoring stack
section "Checking monitoring stack"

echo "Starting monitoring stack..."
if [ -f "monitoring/docker-compose.yml" ]; then
  # Check if containers are already running
  if docker ps | grep -q "icn-prometheus"; then
    echo -e "${YELLOW}!${NC} Monitoring stack already running, skipping startup"
  else
    cd monitoring && docker-compose up -d
    check_result "Monitoring stack started"
  fi
  
  # Wait for services to be ready
  echo "Waiting for services to be ready..."
  sleep 10
  
  # Check Prometheus
  echo "Checking Prometheus..."
  if curl -s http://localhost:9090/-/ready > /dev/null; then
    echo -e "${GREEN}✓${NC} Prometheus is ready"
  else
    echo -e "${RED}✗${NC} Prometheus is not ready"
    ERRORS=$((ERRORS + 1))
  fi
  
  # Check Grafana
  echo "Checking Grafana..."
  if curl -s http://localhost:3000/api/health > /dev/null; then
    echo -e "${GREEN}✓${NC} Grafana is ready"
  else
    echo -e "${RED}✗${NC} Grafana is not ready"
    ERRORS=$((ERRORS + 1))
  fi
else
  echo -e "${RED}✗${NC} monitoring/docker-compose.yml not found"
  ERRORS=$((ERRORS + 1))
fi

# Check metrics endpoint
section "Checking metrics endpoint"

# Start the service if it's not already running
echo "Starting ICN Agoranet service for metrics test..."
METRICS_PORT=8081
cargo run --release --bin icn-agoranet -- --metrics-addr=0.0.0.0:${METRICS_PORT} &
ICN_PID=$!

# Allow time for service to start
echo "Waiting for service to start..."
sleep 5

# Check metrics endpoint
echo "Checking metrics endpoint..."
if curl -s http://localhost:${METRICS_PORT}/metrics > /dev/null; then
  echo -e "${GREEN}✓${NC} Metrics endpoint is accessible"
else
  echo -e "${RED}✗${NC} Metrics endpoint is not accessible"
  ERRORS=$((ERRORS + 1))
fi

# Kill the service
kill $ICN_PID
sleep 2

# Check documentation
section "Checking documentation"

# Check if documentation files exist
echo "Checking documentation files..."
DOC_FILES=(
  "docs/monitoring/README.md"
  "docs/monitoring/setup.md"
  "docs/monitoring/metrics.md"
  "docs/monitoring/alerts.md"
  "docs/monitoring/dashboards.md"
)

for file in "${DOC_FILES[@]}"; do
  if [ -f "$file" ]; then
    echo -e "${GREEN}✓${NC} Found $file"
  else
    echo -e "${RED}✗${NC} Missing $file"
    ERRORS=$((ERRORS + 1))
  fi
done

# Check for images directory
if [ -d "docs/images" ]; then
  echo -e "${GREEN}✓${NC} Found images directory"
else
  echo -e "${YELLOW}!${NC} Missing docs/images directory. Screenshots should be added here."
fi

# Final summary
section "Preflight Check Summary"

if [ $ERRORS -eq 0 ]; then
  echo -e "${GREEN}✅ All checks passed! The ICN v3 Federation is ready.${NC}"
  echo -e "\nNext steps:"
  echo -e "1. Tag this state: ${YELLOW}git tag v3.0.0-alpha-federation-observability${NC}"
  echo -e "2. Start onboarding federation operators with the documentation"
  echo -e "3. Consider adding runtime metrics as the next milestone"
else
  echo -e "${RED}❌ Found $ERRORS issues that need to be addressed.${NC}"
fi

exit $ERRORS 