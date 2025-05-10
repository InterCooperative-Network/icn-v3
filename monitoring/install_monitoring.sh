#!/bin/bash
set -e

# ANSI color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

INSTALL_DIR="/home/matt/dev/icn-v3/monitoring"
SYSTEMD_DIR="/etc/systemd/system"
SERVICE_FILE="icn-monitoring.service"
CURRENT_DIR="$(pwd)"

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

# Create installation directory
echo -e "\n${BLUE}▶ Creating installation directory...${NC}"
mkdir -p "$INSTALL_DIR"

# Copy all monitoring files
echo -e "\n${BLUE}▶ Copying monitoring configuration files...${NC}"
cp -r "$CURRENT_DIR"/* "$INSTALL_DIR/"
echo -e "${GREEN}✓ Files copied to $INSTALL_DIR${NC}"

# Install systemd service
echo -e "\n${BLUE}▶ Installing systemd service...${NC}"
cp "$CURRENT_DIR/$SERVICE_FILE" "$SYSTEMD_DIR/"
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
sleep 5

# Check Prometheus
echo "Checking Prometheus..."
if curl -s http://localhost:9090/-/ready &> /dev/null; then
    echo -e "${GREEN}✓ Prometheus is ready${NC}"
else
    echo -e "${YELLOW}! Prometheus might not be ready yet. Please check manually after a few seconds.${NC}"
fi

# Check Grafana
echo "Checking Grafana..."
if curl -s http://localhost:3000/api/health &> /dev/null; then
    echo -e "${GREEN}✓ Grafana is ready${NC}"
else
    echo -e "${YELLOW}! Grafana might not be ready yet. Please check manually after a few seconds.${NC}"
fi

echo -e "\n${BLUE}▶ Installation Complete!${NC}"
echo -e "\n${BLUE}▶ Service URLs:${NC}"
echo -e "- Prometheus: ${GREEN}http://localhost:9090${NC}"
echo -e "- Grafana: ${GREEN}http://localhost:3000${NC} (default login: admin/admin)"

echo -e "\n${BLUE}▶ Next steps:${NC}"
echo -e "1. Configure your ICN Agoranet instance to expose metrics on port 8081"
echo -e "2. Log in to Grafana and explore the 'ICN Federation Overview' dashboard"
echo -e "3. Add your federation-specific targets to Prometheus configuration at:"
echo -e "   ${GREEN}$INSTALL_DIR/prometheus.yml${NC}"

echo -e "\n${GREEN}Installation complete!${NC}" 