#!/bin/bash

# Set the container name
CONTAINER_NAME="icn-wallet-pwa"

# Check if Podman is installed
if ! command -v podman &> /dev/null; then
    echo "Podman is not installed. Please install it first."
    echo "On Ubuntu/Debian: sudo apt-get install podman"
    echo "On Fedora: sudo dnf install podman"
    exit 1
fi

# Build the image
echo "Building ICN Wallet container image..."
podman build -t $CONTAINER_NAME -f Containerfile .

# Check if container is already running and stop it
if podman container exists $CONTAINER_NAME; then
    echo "Stopping existing container..."
    podman stop $CONTAINER_NAME
    podman rm $CONTAINER_NAME
fi

# Run the container
echo "Starting ICN Wallet container..."
podman run --name $CONTAINER_NAME \
    -p 3001:3001 \
    -v "$(pwd):/app" \
    --rm \
    -it $CONTAINER_NAME

echo "Container stopped." 