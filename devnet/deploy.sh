#!/bin/bash
set -e

# ANSI color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║             ICN v3 Federation Test Deployment                 ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"

# Default configuration
NODE_COUNT=3
NETWORK_DELAY=0
WITH_MONITORING=true
CONTAINER_PREFIX="icn-test"
BASE_PORT=8080
METRICS_PORT_BASE=8081
DB_PORT_BASE=5432

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --nodes)
      NODE_COUNT="$2"
      shift 2
      ;;
    --network-delay)
      NETWORK_DELAY="$2"
      shift 2
      ;;
    --no-monitoring)
      WITH_MONITORING=false
      shift
      ;;
    --help)
      echo "Usage: $0 [options]"
      echo "Options:"
      echo "  --nodes N          Number of federation nodes to deploy (default: 3)"
      echo "  --network-delay D  Add network delay simulation (e.g., 50ms)"
      echo "  --no-monitoring    Deploy without monitoring stack"
      echo "  --help             Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Check prerequisites
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed. Please install Docker first.${NC}"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo -e "${RED}Error: Docker Compose is not installed. Please install Docker Compose first.${NC}"
    exit 1
fi

# Create directory structure
echo -e "\n${BLUE}▶ Creating directory structure...${NC}"
mkdir -p config
mkdir -p data/prometheus
mkdir -p data/grafana

# Generate federation configuration
echo -e "\n${BLUE}▶ Generating configuration for $NODE_COUNT nodes...${NC}"

# Generate node configurations
for i in $(seq 1 $NODE_COUNT); do
    NODE_CONFIG="config/node-$i.toml"
    echo "Generating $NODE_CONFIG..."
    
    cat > "$NODE_CONFIG" << EOF
[federation]
federation_id = "test-federation"
node_id = "node-$i"

[metrics]
metrics_addr = "0.0.0.0:$METRICS_PORT_BASE"

[database]
uri = "postgres://postgres:postgres@localhost:$((DB_PORT_BASE + $i - 1))/icn"
EOF
    
    echo -e "${GREEN}✓${NC} Created $NODE_CONFIG"
done

# Generate docker-compose.yml
echo -e "\n${BLUE}▶ Generating Docker Compose configuration...${NC}"

cat > docker-compose.yml << EOF
version: '3.7'

services:
EOF

# Add database services
for i in $(seq 1 $NODE_COUNT); do
    cat >> docker-compose.yml << EOF
  postgres-$i:
    image: postgres:14
    container_name: ${CONTAINER_PREFIX}-postgres-$i
    environment:
      POSTGRES_PASSWORD: postgres
      POSTGRES_USER: postgres
      POSTGRES_DB: icn
    ports:
      - "$((DB_PORT_BASE + $i - 1)):5432"
    volumes:
      - ./data/postgres-$i:/var/lib/postgresql/data
    networks:
      - federation

EOF
done

# Add node services
for i in $(seq 1 $NODE_COUNT); do
    cat >> docker-compose.yml << EOF
  node-$i:
    image: ${CONTAINER_PREFIX}-node:latest
    container_name: ${CONTAINER_PREFIX}-node-$i
    build:
      context: ..
      dockerfile: Dockerfile
    volumes:
      - ./config/node-$i.toml:/app/config.toml
    ports:
      - "$((BASE_PORT + $i - 1)):8080"
      - "$((METRICS_PORT_BASE + $i - 1)):$METRICS_PORT_BASE"
    depends_on:
      - postgres-$i
    command: ["--config", "/app/config.toml"]
    networks:
      - federation

EOF
done

# Add monitoring services if enabled
if [ "$WITH_MONITORING" = true ]; then
    # Generate Prometheus configuration
    echo "Generating Prometheus configuration..."
    
    mkdir -p config/prometheus
    
    # Create prometheus.yml
    cat > config/prometheus/prometheus.yml << EOF
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "prometheus-rules.yml"

scrape_configs:
  - job_name: 'icn-agoranet'
    scrape_interval: 5s
    static_configs:
      - targets: [
EOF

    # Add node targets
    for i in $(seq 1 $NODE_COUNT); do
        if [ $i -eq $NODE_COUNT ]; then
            echo "          'node-$i:$METRICS_PORT_BASE'" >> config/prometheus/prometheus.yml
        else
            echo "          'node-$i:$METRICS_PORT_BASE'," >> config/prometheus/prometheus.yml
        fi
    done

    cat >> config/prometheus/prometheus.yml << EOF
        ]
        labels:
          env: 'test-federation'
EOF

    # Copy alert rules
    cp ../monitoring/prometheus-rules.yml config/prometheus/

    # Add monitoring services to docker-compose.yml
    cat >> docker-compose.yml << EOF
  prometheus:
    image: prom/prometheus:v2.37.0
    container_name: ${CONTAINER_PREFIX}-prometheus
    volumes:
      - ./config/prometheus/prometheus.yml:/etc/prometheus/prometheus.yml
      - ./config/prometheus/prometheus-rules.yml:/etc/prometheus/prometheus-rules.yml
      - ./data/prometheus:/prometheus
    command:
      - --config.file=/etc/prometheus/prometheus.yml
      - --storage.tsdb.path=/prometheus
      - --web.console.libraries=/usr/share/prometheus/console_libraries
      - --web.console.templates=/usr/share/prometheus/consoles
      - --web.enable-lifecycle
    ports:
      - "9090:9090"
    networks:
      - federation

  grafana:
    image: grafana/grafana:9.1.0
    container_name: ${CONTAINER_PREFIX}-grafana
    volumes:
      - ./data/grafana:/var/lib/grafana
      - ../monitoring/grafana-dashboard-federation-overview.json:/var/lib/grafana/dashboards/federation-overview.json
      - ./config/grafana/provisioning:/etc/grafana/provisioning
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_USERS_ALLOW_SIGN_UP=false
      - GF_INSTALL_PLUGINS=grafana-piechart-panel
      - GF_SERVER_ROOT_URL=http://localhost:3000
    ports:
      - "3000:3000"
    depends_on:
      - prometheus
    networks:
      - federation
EOF

    # Set up Grafana provisioning
    mkdir -p config/grafana/provisioning/datasources
    mkdir -p config/grafana/provisioning/dashboards
    
    # Create datasource provisioning
    cat > config/grafana/provisioning/datasources/prometheus.yml << EOF
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
EOF

    # Create dashboard provisioning
    cat > config/grafana/provisioning/dashboards/dashboards.yml << EOF
apiVersion: 1

providers:
  - name: 'Federation Dashboards'
    folder: 'ICN'
    type: file
    options:
      path: /var/lib/grafana/dashboards
EOF
fi

# Add network configuration
cat >> docker-compose.yml << EOF
networks:
  federation:
    driver: bridge
EOF

echo -e "${GREEN}✓${NC} Generated Docker Compose configuration"

# Create load generation script
echo -e "\n${BLUE}▶ Creating load generation script...${NC}"

cat > generate_load.sh << 'EOF'
#!/bin/bash

TPS=10
PATTERN="steady"
DURATION=300 # 5 minutes

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --tps)
      TPS="$2"
      shift 2
      ;;
    --pattern)
      PATTERN="$2"
      shift 2
      ;;
    --duration)
      DURATION="$2"
      shift 2
      ;;
    --help)
      echo "Usage: $0 [options]"
      echo "Options:"
      echo "  --tps N        Transactions per second (default: 10)"
      echo "  --pattern P    Load pattern: steady, spikes, ramp (default: steady)"
      echo "  --duration N   Duration in seconds (default: 300)"
      echo "  --help         Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

echo "Generating load with pattern: $PATTERN at $TPS TPS for $DURATION seconds"

# Create some test accounts if they don't exist
echo "Creating test accounts..."
curl -X POST http://localhost:8080/api/v1/entities \
  -H "Content-Type: application/json" \
  -d '{"id":"test-account-1","type":"account","metadata":{}}'

curl -X POST http://localhost:8080/api/v1/entities \
  -H "Content-Type: application/json" \
  -d '{"id":"test-account-2","type":"account","metadata":{}}'

# Initialize end time
END_TIME=$(($(date +%s) + DURATION))

# Function to calculate sleep time based on TPS
calculate_sleep() {
  local current_tps=$1
  if [ "$current_tps" -le 0 ]; then
    echo 1  # Default to 1 second if TPS is invalid
  else
    echo "scale=4; 1 / $current_tps" | bc
  fi
}

# Main load generation loop
while [ $(date +%s) -lt $END_TIME ]; do
  current_tps=$TPS
  
  # Adjust TPS based on pattern
  case $PATTERN in
    spikes)
      # Random spikes between 0.5x and 2x the base TPS
      if [ $((RANDOM % 10)) -eq 0 ]; then
        current_tps=$((TPS * 2))
      elif [ $((RANDOM % 10)) -eq 1 ]; then
        current_tps=$((TPS / 2))
      fi
      ;;
    ramp)
      # Gradually increase TPS over time
      elapsed=$(($(date +%s) - (END_TIME - DURATION)))
      current_tps=$((TPS * elapsed / DURATION + 1))
      ;;
  esac
  
  # Calculate sleep time
  sleep_time=$(calculate_sleep $current_tps)
  
  # Generate a random amount
  amount=$((RANDOM % 100 + 1))
  
  # Randomly decide transfer direction
  if [ $((RANDOM % 2)) -eq 0 ]; then
    from="test-account-1"
    to="test-account-2"
  else
    from="test-account-2"
    to="test-account-1"
  fi
  
  # Execute transfer
  curl -s -X POST http://localhost:8080/api/v1/transfers \
    -H "Content-Type: application/json" \
    -d "{\"from\":\"$from\",\"to\":\"$to\",\"amount\":$amount}" > /dev/null
  
  # Sleep based on TPS
  sleep $sleep_time
done

echo "Load generation complete"
EOF

chmod +x generate_load.sh
echo -e "${GREEN}✓${NC} Created load generation script"

# Create run_tests script
echo -e "\n${BLUE}▶ Creating test script...${NC}"

cat > run_tests.sh << 'EOF'
#!/bin/bash

echo "Running federation tests..."

# Check if all nodes are healthy
echo "Checking node health..."
for i in $(seq 1 3); do
  if ! curl -s http://localhost:$((8080 + $i - 1))/health | grep -q "ok"; then
    echo "Node $i is not healthy"
    exit 1
  fi
done

# Test account creation
echo "Testing account creation..."
curl -X POST http://localhost:8080/api/v1/entities \
  -H "Content-Type: application/json" \
  -d '{"id":"test-account-3","type":"account","metadata":{}}'

# Test transfers
echo "Testing transfers..."
curl -X POST http://localhost:8080/api/v1/transfers \
  -H "Content-Type: application/json" \
  -d '{"from":"test-account-1","to":"test-account-3","amount":50}'

# Check balances
echo "Checking balances..."
curl -s http://localhost:8080/api/v1/balances/test-account-3

echo "Tests completed successfully"
EOF

chmod +x run_tests.sh
echo -e "${GREEN}✓${NC} Created test script"

# Build and start the stack
echo -e "\n${BLUE}▶ Building ICN node image...${NC}"
cd ..
docker build -t ${CONTAINER_PREFIX}-node .
cd devnet

echo -e "\n${BLUE}▶ Starting the federation test environment...${NC}"
docker-compose up -d

echo -e "\n${GREEN}✓${NC} Federation test environment is now running!"
echo -e "\nAccess points:"
echo -e "- Federation nodes: http://localhost:$BASE_PORT to http://localhost:$((BASE_PORT + NODE_COUNT - 1))"

if [ "$WITH_MONITORING" = true ]; then
    echo -e "- Prometheus: http://localhost:9090"
    echo -e "- Grafana: http://localhost:3000 (login: admin/admin)"
fi

echo -e "\nUseful commands:"
echo -e "- Generate load: ./generate_load.sh"
echo -e "- Run tests: ./run_tests.sh"
echo -e "- Stop environment: docker-compose down" 