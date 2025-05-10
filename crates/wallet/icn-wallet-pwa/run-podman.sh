#!/bin/bash

# Default configuration
CONTAINER_NAME="icn-wallet-pwa"
PORT=3001
BIND_ADDRESS="127.0.0.1"
BUILD_MODE="dev"
HTTPS_ENABLED=false
CERT_DIR="./certs"
DATA_VOLUME="${CONTAINER_NAME}-data"

# Display help message
show_help() {
    echo "ICN Wallet PWA Development Container"
    echo ""
    echo "Usage: ./run-podman.sh [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -m, --mode MODE       Build mode: dev, test, or prod (default: dev)"
    echo "  -p, --port PORT       Port to bind (default: 3001)"
    echo "  -b, --bind ADDRESS    Address to bind (default: 127.0.0.1, use 0.0.0.0 for all interfaces)"
    echo "  -s, --https           Enable HTTPS with self-signed certificates"
    echo "  -c, --cert-dir DIR    Directory containing certificate files (default: ./certs)"
    echo "  -v, --volume NAME     Name of the persistent data volume (default: icn-wallet-pwa-data)"
    echo "  -d, --systemd         Generate systemd service file"
    echo "  -h, --help            Show this help message"
    echo ""
    echo "Examples:"
    echo "  ./run-podman.sh                          # Run in development mode"
    echo "  ./run-podman.sh -m prod                  # Run in production mode"
    echo "  ./run-podman.sh -b 0.0.0.0               # Allow access from other devices on LAN"
    echo "  ./run-podman.sh -s                       # Enable HTTPS with self-signed certificates"
    echo "  ./run-podman.sh -s -c /path/to/certs     # Use existing certificates"
    echo "  ./run-podman.sh -d                       # Generate systemd service file"
    echo ""
}

# Parse command-line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -m|--mode)
            BUILD_MODE="$2"
            shift 2
            ;;
        -p|--port)
            PORT="$2"
            shift 2
            ;;
        -b|--bind)
            BIND_ADDRESS="$2"
            shift 2
            ;;
        -s|--https)
            HTTPS_ENABLED=true
            shift
            ;;
        -c|--cert-dir)
            CERT_DIR="$2"
            shift 2
            ;;
        -v|--volume)
            DATA_VOLUME="$2"
            shift 2
            ;;
        -d|--systemd)
            GENERATE_SYSTEMD=true
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Check if Podman is installed
if ! command -v podman &> /dev/null; then
    echo "Error: Podman is not installed. Please install it first."
    echo "On Ubuntu/Debian: sudo apt-get install podman"
    echo "On Fedora: sudo dnf install podman"
    exit 1
fi

# Validate build mode
if [[ "$BUILD_MODE" != "dev" && "$BUILD_MODE" != "test" && "$BUILD_MODE" != "prod" ]]; then
    echo "Error: Invalid build mode. Use dev, test, or prod."
    exit 1
fi

# Set environment variables based on build mode
case "$BUILD_MODE" in
    dev)
        ENV_VARS="-e NODE_ENV=development"
        CMD="npm run dev"
        ;;
    test)
        ENV_VARS="-e NODE_ENV=test"
        CMD="npm run test"
        ;;
    prod)
        ENV_VARS="-e NODE_ENV=production"
        CMD="npm run start"
        ;;
esac

# Set up HTTPS if enabled
if [ "$HTTPS_ENABLED" = true ]; then
    # Create certificate directory if it doesn't exist
    mkdir -p "$CERT_DIR"
    
    # Check if certificates already exist
    if [ ! -f "$CERT_DIR/server.key" ] || [ ! -f "$CERT_DIR/server.crt" ]; then
        echo "Generating self-signed certificates..."
        
        # Check if openssl is installed
        if ! command -v openssl &> /dev/null; then
            echo "Error: OpenSSL is not installed. Please install it to generate certificates."
            exit 1
        fi
        
        # Generate self-signed certificates
        openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
            -keyout "$CERT_DIR/server.key" -out "$CERT_DIR/server.crt" \
            -subj "/CN=localhost" \
            -addext "subjectAltName = DNS:localhost,IP:127.0.0.1"
            
        echo "Self-signed certificates created in $CERT_DIR"
    else
        echo "Using existing certificates in $CERT_DIR"
    fi
    
    # Additional environment variables for HTTPS
    ENV_VARS="$ENV_VARS -e HTTPS=true -e SSL_KEY=/app/certs/server.key -e SSL_CERT=/app/certs/server.crt"
    
    # Adjust the command to use HTTPS in Next.js
    CMD="npx concurrently \"npx node https-server.js\" \"$CMD\""
fi

# Create data volume if it doesn't exist
if ! podman volume exists "$DATA_VOLUME"; then
    echo "Creating persistent data volume: $DATA_VOLUME"
    podman volume create "$DATA_VOLUME"
fi

# Create https-server.js file for HTTPS support
if [ "$HTTPS_ENABLED" = true ]; then
    cat > https-server.js << 'EOL'
const { createServer } = require('https');
const { parse } = require('url');
const next = require('next');
const fs = require('fs');

const dev = process.env.NODE_ENV !== 'production';
const app = next({ dev });
const handle = app.getRequestHandler();

const httpsOptions = {
  key: fs.readFileSync(process.env.SSL_KEY),
  cert: fs.readFileSync(process.env.SSL_CERT),
};

app.prepare().then(() => {
  createServer(httpsOptions, (req, res) => {
    const parsedUrl = parse(req.url, true);
    handle(req, res, parsedUrl);
  }).listen(process.env.PORT || 3001, process.env.BIND_ADDRESS || '0.0.0.0', (err) => {
    if (err) throw err;
    console.log(`> HTTPS Server running on https://${process.env.BIND_ADDRESS || '0.0.0.0'}:${process.env.PORT || 3001}`);
  });
});
EOL

    # Add concurrently as a dependency
    if ! grep -q "concurrently" package.json; then
        echo "Adding concurrently dependency for HTTPS support..."
        npm install --save-dev concurrently
    fi
fi

# Generate systemd service if requested
if [ "$GENERATE_SYSTEMD" = true ]; then
    echo "Generating systemd service files..."
    
    CONTAINER_NAME_CLEAN="${CONTAINER_NAME//-/_}"
    SERVICE_FILE="$CONTAINER_NAME.service"
    
    # Generate systemd service file
    podman generate systemd --name "$CONTAINER_NAME" --files --new
    
    echo "Systemd service file generated: $SERVICE_FILE"
    echo "To install the service:"
    echo "  cp $SERVICE_FILE ~/.config/systemd/user/"
    echo "  systemctl --user daemon-reload"
    echo "  systemctl --user enable --now $SERVICE_FILE"
    
    # Exit without running the container
    exit 0
fi

# Build the image
echo "Building ICN Wallet container image for '$BUILD_MODE' mode..."
podman build -t "$CONTAINER_NAME:$BUILD_MODE" -f Containerfile .

# Check if container is already running and stop it
if podman container exists "$CONTAINER_NAME"; then
    echo "Stopping existing container..."
    podman stop "$CONTAINER_NAME"
    podman rm "$CONTAINER_NAME"
fi

# Run the container
echo "Starting ICN Wallet container in '$BUILD_MODE' mode..."
echo "Server will be available at http${HTTPS_ENABLED:+s}://${BIND_ADDRESS}:${PORT}"

# Construct run command
RUN_CMD="podman run --name $CONTAINER_NAME \
    -p ${BIND_ADDRESS}:${PORT}:3001 \
    -v \"$(pwd):/app\" \
    -v \"${DATA_VOLUME}:/app/data\" \
    ${HTTPS_ENABLED:+-v \"$(realpath $CERT_DIR):/app/certs\"} \
    -e PORT=3001 \
    -e BIND_ADDRESS=$BIND_ADDRESS \
    $ENV_VARS \
    --rm \
    -it $CONTAINER_NAME:$BUILD_MODE"

# If in production mode, use a different CMD
if [ "$BUILD_MODE" = "prod" ]; then
    RUN_CMD="${RUN_CMD} ${CMD}"
fi

# Execute run command
eval $RUN_CMD

echo "Container stopped." 