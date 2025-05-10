#!/bin/bash
set -e

# ANSI color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║             ICN v3 Federation Monitoring Setup                ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"

# Check if Docker and Docker Compose are installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed. Please install Docker first.${NC}"
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo -e "${RED}Error: Docker Compose is not installed. Please install Docker Compose first.${NC}"
    exit 1
fi

COMPOSE_FILE="monitoring/docker-compose.yml"

# Check if compose file exists
if [ ! -f "$COMPOSE_FILE" ]; then
    echo -e "${RED}Error: $COMPOSE_FILE not found.${NC}"
    echo "Make sure you're running this script from the root of the ICN v3 repository."
    exit 1
fi

# Check if services are already running
if docker ps | grep -q "icn-prometheus" || docker ps | grep -q "icn-grafana"; then
    echo -e "${YELLOW}Some monitoring services are already running.${NC}"
    read -p "Do you want to stop and restart them? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Stopping existing services..."
        cd monitoring && docker-compose down
    else
        echo "Keeping existing services running. Setup aborted."
        exit 0
    fi
fi

# Start the monitoring stack
echo -e "\n${BLUE}▶ Starting monitoring stack...${NC}"
cd monitoring && docker-compose up -d
echo -e "${GREEN}✓ Monitoring stack started successfully!${NC}"

# Check services health
echo -e "\n${BLUE}▶ Checking service health...${NC}"
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

echo -e "\n${BLUE}▶ Service URLs:${NC}"
echo -e "- Prometheus: ${GREEN}http://localhost:9090${NC}"
echo -e "- Grafana: ${GREEN}http://localhost:3000${NC} (default login: admin/admin)"

echo -e "\n${BLUE}▶ Next steps:${NC}"
echo -e "1. Configure your ICN Agoranet instance to expose metrics on port 8081"
echo -e "2. Log in to Grafana and explore the 'ICN Federation Overview' dashboard"
echo -e "3. Add your federation-specific targets to Prometheus configuration"

echo -e "\n${GREEN}Setup complete!${NC}" 