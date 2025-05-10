#!/bin/bash

# Script to run PostgreSQL ledger integration tests

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
cd $SCRIPT_DIR

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "Error: Docker is not running"
    exit 1
fi

# Check if user wants to skip the database steps
if [ "$1" == "--skip-db" ]; then
    echo "Skipping database setup..."
    export SKIP_DB_TESTS=1
else
    # Start PostgreSQL test container
    echo "Starting PostgreSQL test container..."
    docker-compose -f docker-compose.test.yml up postgres-test -d

    # Wait for PostgreSQL to be ready
    echo "Waiting for PostgreSQL to be ready..."
    for i in {1..30}; do
        if docker exec $(docker-compose -f docker-compose.test.yml ps -q postgres-test) pg_isready -U postgres; then
            break
        fi
        echo "Waiting for PostgreSQL to be ready... $i/30"
        sleep 1
        if [ $i -eq 30 ]; then
            echo "Error: PostgreSQL did not start in time"
            docker-compose -f docker-compose.test.yml logs postgres-test
            docker-compose -f docker-compose.test.yml down
            exit 1
        fi
    done

    # Create test database if it doesn't exist
    echo "Ensuring test database exists..."
    docker exec $(docker-compose -f docker-compose.test.yml ps -q postgres-test) psql -U postgres -c "CREATE DATABASE icn_ledger_test;" || true
    
    # Set the database URL for tests
    export DATABASE_URL="postgres://postgres:postgres@localhost:5433/icn_ledger_test"
fi

# Go to repository root
cd ../../../..

# Run the integration tests
echo "Running integration tests..."
cargo test -p icn-agoranet --test ledger_integration_test -- --nocapture

# Clean up if we started the database
if [ -z "$SKIP_DB_TESTS" ]; then
    echo "Stopping PostgreSQL test container..."
    cd $SCRIPT_DIR
    docker-compose -f docker-compose.test.yml down
fi

echo "Integration tests completed!" 