#!/bin/bash
set -e

# ANSI color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Default values - can be overridden by environment variables or command line arguments
DEFAULT_INSTALL_DIR="/opt/icn/monitoring"
DEFAULT_DATA_DIR="/var/lib/icn"
DEFAULT_CONFIG_DIR="/etc/icn"
DEFAULT_SYSTEMD_DIR="/etc/systemd/system"
DEFAULT_SERVICE_FILE="icn-monitoring.service"
DEFAULT_CONFIG_FILE="monitoring.conf"
DEFAULT_PROMETHEUS_PORT="9090"
DEFAULT_GRAFANA_PORT="3000"
DEFAULT_FEDERATION_ID="default-federation"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --data-dir)
      DATA_DIR="$2"
      shift 2
      ;;
    --config-dir)
      CONFIG_DIR="$2"
      shift 2
      ;;
    --federation-id)
      FEDERATION_ID="$2"
      shift 2
      ;;
    --federation-name)
      FEDERATION_NAME="$2"
      shift 2
      ;;
    --prometheus-port)
      PROMETHEUS_PORT="$2"
      shift 2
      ;;
    --grafana-port)
      GRAFANA_PORT="$2"
      shift 2
      ;;
    --federation-endpoints)
      FEDERATION_ENDPOINTS="$2"
      shift 2
      ;;
    --help)
      echo "Usage: $0 [OPTIONS]"
      echo "Install ICN Monitoring Stack"
      echo ""
      echo "Options:"
      echo "  --install-dir DIR        Installation directory (default: $DEFAULT_INSTALL_DIR)"
      echo "  --data-dir DIR           Data directory (default: $DEFAULT_DATA_DIR)"
      echo "  --config-dir DIR         Config directory (default: $DEFAULT_CONFIG_DIR)"
      echo "  --federation-id ID       Federation ID (default: $DEFAULT_FEDERATION_ID)"
      echo "  --federation-name NAME   Federation name"
      echo "  --prometheus-port PORT   Prometheus port (default: $DEFAULT_PROMETHEUS_PORT)"
      echo "  --grafana-port PORT      Grafana port (default: $DEFAULT_GRAFANA_PORT)"
      echo "  --federation-endpoints E Federation metrics endpoints (comma-separated)"
      echo "  --help                   Display this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      echo "Use --help for usage information"
      exit 1
      ;;
  esac
done

# Set defaults if not specified
INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
DATA_DIR="${DATA_DIR:-$DEFAULT_DATA_DIR}"
CONFIG_DIR="${CONFIG_DIR:-$DEFAULT_CONFIG_DIR}"
SYSTEMD_DIR="${SYSTEMD_DIR:-$DEFAULT_SYSTEMD_DIR}"
SERVICE_FILE="${SERVICE_FILE:-$DEFAULT_SERVICE_FILE}"
CONFIG_FILE="${CONFIG_FILE:-$DEFAULT_CONFIG_FILE}"
PROMETHEUS_PORT="${PROMETHEUS_PORT:-$DEFAULT_PROMETHEUS_PORT}"
GRAFANA_PORT="${GRAFANA_PORT:-$DEFAULT_GRAFANA_PORT}"
FEDERATION_ID="${FEDERATION_ID:-$DEFAULT_FEDERATION_ID}"
FEDERATION_NAME="${FEDERATION_NAME:-$FEDERATION_ID Federation}"
FEDERATION_ENDPOINTS="${FEDERATION_ENDPOINTS:-localhost:8081}"

CURRENT_DIR="$(pwd)"
PROMETHEUS_DATA_DIR="$DATA_DIR/prometheus"
GRAFANA_DATA_DIR="$DATA_DIR/grafana"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║             ICN v3 Federation Monitoring Installer            ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"

# Check if running as root
if [ "$EUID" -ne 0 ]; then
  echo -e "${YELLOW}This script should be run as root or with sudo privileges.${NC}"
  echo -e "Please run: sudo $0"
  exit 1
fi

# Check if Docker and Docker Compose are installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed. Please install Docker first.${NC}"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo -e "${RED}Error: Docker Compose is not installed. Please install Docker Compose first.${NC}"
    exit 1
fi

# Create required directories
echo -e "\n${BLUE}▶ Creating directories...${NC}"
mkdir -p "$INSTALL_DIR"
mkdir -p "$DATA_DIR"
mkdir -p "$CONFIG_DIR"
mkdir -p "$PROMETHEUS_DATA_DIR"
mkdir -p "$GRAFANA_DATA_DIR"
echo -e "${GREEN}✓ Directories created${NC}"

# Copy all monitoring files
echo -e "\n${BLUE}▶ Copying monitoring configuration files...${NC}"
cp -r "$CURRENT_DIR"/* "$INSTALL_DIR/"
echo -e "${GREEN}✓ Files copied to $INSTALL_DIR${NC}"

# Create environment configuration file
echo -e "\n${BLUE}▶ Creating environment configuration...${NC}"
cat > "$CONFIG_DIR/$CONFIG_FILE" << EOF
# ICN Monitoring Stack Configuration
# Generated on $(date)

# Installation directory
ICN_MONITORING_DIR=$INSTALL_DIR

# Federation identification
FEDERATION_ID=$FEDERATION_ID
FEDERATION_NAME=$FEDERATION_NAME

# Network configuration
PROMETHEUS_PORT=$PROMETHEUS_PORT
GRAFANA_PORT=$GRAFANA_PORT

# Data storage locations
PROMETHEUS_DATA_DIR=$PROMETHEUS_DATA_DIR
GRAFANA_DATA_DIR=$GRAFANA_DATA_DIR

# Admin credentials - Change these in production!
GRAFANA_ADMIN_PASSWORD=admin

# Federation metrics endpoints
# Comma-separated list of metrics endpoints to scrape
FEDERATION_ENDPOINTS=$FEDERATION_ENDPOINTS
COOPERATIVE_ENDPOINTS=
COMMUNITY_ENDPOINTS=
EOF
echo -e "${GREEN}✓ Configuration file created at $CONFIG_DIR/$CONFIG_FILE${NC}"

# Generate Prometheus configuration from template
echo -e "\n${BLUE}▶ Generating Prometheus configuration...${NC}"
# Process federation endpoints
FEDERATION_TARGETS=""
IFS=',' read -ra ENDPOINTS <<< "$FEDERATION_ENDPOINTS"
for ENDPOINT in "${ENDPOINTS[@]}"; do
  FEDERATION_TARGETS+="      - targets: [\"$ENDPOINT\"]\n        labels:\n          federation: \"$FEDERATION_ID\"\n          instance_type: \"federation\"\n"
done

# Create the prometheus.yml file from template
sed "s|\$FEDERATION_TARGETS|$FEDERATION_TARGETS|g" "$INSTALL_DIR/prometheus.yml.template" | \
sed "s|\$COOPERATIVE_TARGETS||g" | \
sed "s|\$COMMUNITY_TARGETS||g" > "$INSTALL_DIR/prometheus.yml"
echo -e "${GREEN}✓ Prometheus configuration generated${NC}"

# Install systemd service
echo -e "\n${BLUE}▶ Installing systemd service...${NC}"
cp "$INSTALL_DIR/$SERVICE_FILE" "$SYSTEMD_DIR/"
systemctl daemon-reload
echo -e "${GREEN}✓ Service file installed${NC}"

# Enable and start the service
echo -e "\n${BLUE}▶ Enabling and starting the service...${NC}"
systemctl enable "$SERVICE_FILE"
systemctl start "$SERVICE_FILE"
echo -e "${GREEN}✓ Service enabled and started${NC}"

# Verify service is running
echo -e "\n${BLUE}▶ Verifying installation...${NC}"
if systemctl is-active --quiet "$SERVICE_FILE"; then
    echo -e "${GREEN}✓ Service is running${NC}"
else
    echo -e "${RED}! Service failed to start${NC}"
    echo -e "Check the logs with: journalctl -u $SERVICE_FILE"
    exit 1
fi

# Wait for services to be ready
echo -e "\n${BLUE}▶ Waiting for services to be ready...${NC}"
sleep 10

# Check Prometheus
echo "Checking Prometheus..."
if curl -s "http://localhost:$PROMETHEUS_PORT/-/ready" &> /dev/null; then
    echo -e "${GREEN}✓ Prometheus is ready${NC}"
else
    echo -e "${YELLOW}! Prometheus might not be ready yet. Please check manually after a few seconds.${NC}"
fi

# Check Grafana
echo "Checking Grafana..."
if curl -s "http://localhost:$GRAFANA_PORT/api/health" &> /dev/null; then
    echo -e "${GREEN}✓ Grafana is ready${NC}"
else
    echo -e "${YELLOW}! Grafana might not be ready yet. Please check manually after a few seconds.${NC}"
fi

echo -e "\n${BLUE}▶ Installation Complete!${NC}"
echo -e "\n${BLUE}▶ Service URLs:${NC}"
echo -e "- Prometheus: ${GREEN}http://localhost:$PROMETHEUS_PORT${NC}"
echo -e "- Grafana: ${GREEN}http://localhost:$GRAFANA_PORT${NC} (default login: admin/admin)"

echo -e "\n${BLUE}▶ Configuration locations:${NC}"
echo -e "- Environment config: ${GREEN}$CONFIG_DIR/$CONFIG_FILE${NC}"
echo -e "- Prometheus config: ${GREEN}$INSTALL_DIR/prometheus.yml${NC}"
echo -e "- Systemd service: ${GREEN}$SYSTEMD_DIR/$SERVICE_FILE${NC}"

echo -e "\n${BLUE}▶ Next steps:${NC}"
echo -e "1. Update Grafana password in production environments"
echo -e "2. Add additional federation metrics endpoints to $CONFIG_DIR/$CONFIG_FILE"
echo -e "3. Restart the service after config changes: sudo systemctl restart $SERVICE_FILE"

echo -e "\n${GREEN}Installation complete!${NC}" 