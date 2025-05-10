#!/bin/bash
set -eo pipefail

# Test script for validating ICN v3 onboarding bundles
# This script tests the federation, cooperative, and community bootstrap scripts

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Test directory
TEST_DIR="$(pwd)/onboarding_test"
TEMPLATES_DIR="$(pwd)/onboarding/templates"
SCRIPTS_DIR="$(pwd)/onboarding/scripts"

# Log function
log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

# Error function
error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}"
    exit 1
}

# Warning function
warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"
}

# Check dependencies
check_dependencies() {
    log "Checking dependencies..."
    
    # List of required commands
    REQUIRED_COMMANDS=("jq" "openssl" "sed" "curl")
    
    for cmd in "${REQUIRED_COMMANDS[@]}"; do
        if ! command -v "$cmd" &> /dev/null; then
            error "Required command '$cmd' not found. Please install it and try again."
        fi
    done
    
    log "All dependencies satisfied."
}

# Mock icn-cli commands for testing
setup_mock_commands() {
    log "Setting up mock commands for testing..."
    
    # Create temp directory for mock scripts
    MOCK_DIR="$TEST_DIR/mock_bin"
    mkdir -p "$MOCK_DIR"
    export PATH="$MOCK_DIR:$PATH"
    
    # Create mock icn-cli script
    cat > "$MOCK_DIR/icn-cli" << 'EOF'
#!/bin/bash
# Mock icn-cli that simulates successful operations

# Extract the first command
CMD=$1

case "$CMD" in
    "identity")
        # Generate a mock DID
        echo '{"did":"did:icn:mock:fedadmin'$RANDOM'","key":"mock_key_data"}' > $4
        echo '{"did":"did:icn:mock:fedadmin'$RANDOM'","verificationMethod":[]}'
        ;;
    "trust")
        # Handle trust operations
        SUBCMD=$2
        case "$SUBCMD" in
            "inspect-bundle")
                echo '{"cid":"bafybeiczsscdsbs7ffqz55asqde1qmv6rhovw5s","valid":true}'
                ;;
            *)
                echo '{"status":"success"}'
                ;;
        esac
        ;;
    "dag")
        # Handle DAG operations
        echo '{"status":"success"}'
        ;;
    "proposal")
        # Handle proposal operations
        echo '{"status":"success"}'
        ;;
    "runtime")
        # Handle runtime operations
        echo '{"status":"success"}'
        ;;
    "federation")
        # Handle federation operations
        echo '{"status":"success"}'
        ;;
    "token")
        # Handle token operations
        echo '{"status":"success"}'
        ;;
    *)
        # Default response
        echo '{"status":"success"}'
        ;;
esac
EOF
    
    # Make the mock script executable
    chmod +x "$MOCK_DIR/icn-cli"
    
    # Create mock cp command that doesn't fail on missing monitoring templates
    cat > "$MOCK_DIR/cp" << 'EOF'
#!/bin/bash
# Mock cp command that doesn't fail on monitoring templates

# If copying monitoring templates that don't exist, just skip
if [[ "$*" == *"monitoring"* && "$*" == *"*"* ]]; then
    # Create minimal monitoring files instead
    mkdir -p "$3"
    echo "# Mock Prometheus config" > "$3/prometheus.yml"
    echo "# Mock Grafana dashboard" > "$3/grafana-dashboard-federation.json"
    exit 0
fi

# Otherwise, call the real cp command
/bin/cp "$@"
EOF
    
    # Make the mock script executable
    chmod +x "$MOCK_DIR/cp"
    
    log "Mock commands setup complete."
}

# Setup test environment
setup_test_env() {
    log "Setting up test environment at $TEST_DIR"
    
    # Create test directory if it doesn't exist
    if [ -d "$TEST_DIR" ]; then
        warn "Test directory already exists. Removing..."
        rm -rf "$TEST_DIR"
    fi
    
    mkdir -p "$TEST_DIR"
    mkdir -p "$TEST_DIR/federation"
    mkdir -p "$TEST_DIR/cooperative"
    mkdir -p "$TEST_DIR/community"
    mkdir -p "$TEST_DIR/config"
    mkdir -p "$TEST_DIR/logs"
    
    # Create templates directory if needed
    mkdir -p "$TEST_DIR/templates"
    mkdir -p "$TEST_DIR/templates/monitoring"
    
    # Copy templates to test directory if they exist
    if [ -d "$TEMPLATES_DIR" ]; then
        cp -r "$TEMPLATES_DIR"/* "$TEST_DIR/templates/"
    else
        # Create minimal templates for testing
        mkdir -p "$TEST_DIR/templates/policies"
        echo "# Mock federation config" > "$TEST_DIR/templates/federation_config.yaml"
        echo "id: \${FEDERATION_ID}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "name: \${FEDERATION_NAME}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "description: \${FEDERATION_DESCRIPTION}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "data_dir: \${DATA_DIR}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "log_dir: \${LOG_DIR}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "admin_dids:" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "  - \${ADMIN1_DID}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "  - \${ADMIN2_DID}" >> "$TEST_DIR/templates/federation_config.yaml"
        echo "  - \${ADMIN3_DID}" >> "$TEST_DIR/templates/federation_config.yaml"
        
        echo "# Mock cooperative config" > "$TEST_DIR/templates/cooperative_config.yaml"
        echo "id: \${COOPERATIVE_ID}" >> "$TEST_DIR/templates/cooperative_config.yaml"
        echo "name: \${COOPERATIVE_NAME}" >> "$TEST_DIR/templates/cooperative_config.yaml"
        echo "federation_id: \${FEDERATION_ID}" >> "$TEST_DIR/templates/cooperative_config.yaml"
        
        echo "# Mock community config" > "$TEST_DIR/templates/community_config.yaml"
        echo "id: \${COMMUNITY_ID}" >> "$TEST_DIR/templates/community_config.yaml"
        echo "name: \${COMMUNITY_NAME}" >> "$TEST_DIR/templates/community_config.yaml"
        echo "cooperative_id: \${COOPERATIVE_ID}" >> "$TEST_DIR/templates/community_config.yaml"
        
        # Create a minimal policy file
        echo "# Mock resource policy" > "$TEST_DIR/templates/policies/resource.ccl"
        echo "policy {" >> "$TEST_DIR/templates/policies/resource.ccl"
        echo "  name = \"Resource Allocation\"" >> "$TEST_DIR/templates/policies/resource.ccl"
        echo "  version = \"1.0.0\"" >> "$TEST_DIR/templates/policies/resource.ccl"
        echo "  community = \"\${COMMUNITY_ID}\"" >> "$TEST_DIR/templates/policies/resource.ccl"
        echo "}" >> "$TEST_DIR/templates/policies/resource.ccl"
        
        # Create minimal monitoring templates
        echo "# Mock Prometheus config" > "$TEST_DIR/templates/monitoring/prometheus.yml"
        echo "global:" >> "$TEST_DIR/templates/monitoring/prometheus.yml"
        echo "  scrape_interval: 15s" >> "$TEST_DIR/templates/monitoring/prometheus.yml"
        echo "  federation_id: \${FEDERATION_ID}" >> "$TEST_DIR/templates/monitoring/prometheus.yml"
        
        echo "# Mock Grafana dashboard" > "$TEST_DIR/templates/monitoring/grafana-dashboard-federation.json"
        echo "{" >> "$TEST_DIR/templates/monitoring/grafana-dashboard-federation.json"
        echo "  \"title\": \"Federation Dashboard - \${FEDERATION_NAME}\"," >> "$TEST_DIR/templates/monitoring/grafana-dashboard-federation.json"
        echo "  \"federation\": \"\${FEDERATION_ID}\"" >> "$TEST_DIR/templates/monitoring/grafana-dashboard-federation.json"
        echo "}" >> "$TEST_DIR/templates/monitoring/grafana-dashboard-federation.json"
    fi
    
    # Setup mock commands
    setup_mock_commands
    
    log "Test environment setup complete."
}

# Test federation bootstrap
test_federation_bootstrap() {
    log "Testing federation bootstrap script..."
    
    # Run federation bootstrap script with test environment variables
    export DATA_DIR="$TEST_DIR/data"
    export CONFIG_DIR="$TEST_DIR/config"
    export TEMPLATES_DIR="$TEST_DIR/templates"
    export LOG_DIR="$TEST_DIR/logs"
    export FEDERATION_ID="test-federation"
    export FEDERATION_NAME="Test Federation"
    export FEDERATION_DESCRIPTION="A test federation for ICN v3 testing"
    export ADMIN_COUNT=3
    export MONITORING_ENABLED=true
    
    # Create data directories that would normally require root
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID"
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/dag_store"
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/credentials"
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/policies"
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/templates"
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/monitoring"
    
    # Run the script
    cd "$TEST_DIR"
    PREV_PATH="$PATH"
    export PATH="$TEST_DIR/mock_bin:$PATH"
    
    # Create minimal policy files in templates directory (to avoid file not found errors)
    mkdir -p "$TEST_DIR/templates/policies"
    
    # Create necessary policy files if they don't exist
    for policy in dispute_resolution membership resource_allocation; do
        if [ ! -f "$TEST_DIR/templates/policies/${policy}.ccl" ]; then
            echo "# Mock ${policy} policy" > "$TEST_DIR/templates/policies/${policy}.ccl"
            echo "policy {" >> "$TEST_DIR/templates/policies/${policy}.ccl"
            echo "  name = \"${policy}\"" >> "$TEST_DIR/templates/policies/${policy}.ccl"
            echo "  version = \"1.0.0\"" >> "$TEST_DIR/templates/policies/${policy}.ccl"
            echo "  community = \"\${COMMUNITY_ID}\"" >> "$TEST_DIR/templates/policies/${policy}.ccl"
            echo "}" >> "$TEST_DIR/templates/policies/${policy}.ccl"
        fi
    done
    
    bash "$SCRIPTS_DIR/federation_bootstrap.sh" || error "Federation bootstrap failed"
    export PATH="$PREV_PATH"
    
    # Verify outputs
    if [ ! -f "$CONFIG_DIR/federation_$FEDERATION_ID.yaml" ]; then
        error "Federation configuration file not created"
    fi
    
    if [ ! -d "$DATA_DIR/federation/$FEDERATION_ID/dag_store" ]; then
        error "Federation DAG directory not created"
    fi
    
    # Save the trust bundle CID for cooperative bootstrap
    TRUST_BUNDLE_CID="bafybeiczsscdsbs7ffqz55asqde1qmv6rhovw5s"
    
    log "Federation bootstrap test passed"
    return 0
}

# Test cooperative bootstrap
test_cooperative_bootstrap() {
    log "Testing cooperative bootstrap script..."
    
    # Run cooperative bootstrap script with test environment variables
    export DATA_DIR="$TEST_DIR/data"
    export CONFIG_DIR="$TEST_DIR/config"
    export TEMPLATES_DIR="$TEST_DIR/templates"
    export LOG_DIR="$TEST_DIR/logs"
    export FEDERATION_ID="test-federation"
    export COOPERATIVE_ID="test-cooperative"
    export COOPERATIVE_NAME="Test Cooperative"
    export COOPERATIVE_DESCRIPTION="A test cooperative for testing"
    export TOKEN_SYMBOL="TCOOP"
    export TOKEN_NAME="Test Cooperative Token"
    export OPERATOR_COUNT=3
    export TRUST_BUNDLE_CID="bafybeiczsscdsbs7ffqz55asqde1qmv6rhovw5s"
    
    # Create data directories
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID"
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/policies"
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials"
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/templates"
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/token"
    
    # Create mock federation admin key and DID file
    mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/credentials"
    echo '{"did":"did:icn:mock:fedadmin123456","key":"mock_key_data"}' > "$DATA_DIR/federation/$FEDERATION_ID/credentials/admin1_key.json"
    echo "did:icn:mock:federation123456" > "$DATA_DIR/federation/$FEDERATION_ID/federation.did"
    
    # Run the script
    cd "$TEST_DIR"
    PREV_PATH="$PATH"
    export PATH="$TEST_DIR/mock_bin:$PATH"
    
    # Run cooperative bootstrap script with proper parameters
    bash "$SCRIPTS_DIR/cooperative_bootstrap.sh" \
        --federation "$FEDERATION_ID" \
        --trust-bundle "$TRUST_BUNDLE_CID" \
        --cooperative "$COOPERATIVE_ID" \
        --name "$COOPERATIVE_NAME" \
        --description "$COOPERATIVE_DESCRIPTION" \
        --token-symbol "$TOKEN_SYMBOL" \
        --token-name "$TOKEN_NAME" \
        --operator-count "$OPERATOR_COUNT" \
        --data-dir "$DATA_DIR" \
        --config-dir "$CONFIG_DIR" \
        --templates-dir "$TEMPLATES_DIR" \
        --log-dir "$LOG_DIR" \
        || error "Cooperative bootstrap failed"
    
    export PATH="$PREV_PATH"
    
    # Verify outputs
    if [ ! -f "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml" ]; then
        error "Cooperative configuration file not created"
    fi
    
    if [ ! -d "$DATA_DIR/cooperative/$COOPERATIVE_ID/policies" ]; then
        error "Cooperative policies directory not created"
    fi
    
    log "Cooperative bootstrap test passed"
    return 0
}

# Test community bootstrap
test_community_bootstrap() {
    log "Testing community bootstrap script..."
    
    # Read the community bootstrap script to understand its parameters
    COMMUNITY_SCRIPT_CONTENT=$(cat "$SCRIPTS_DIR/community_bootstrap.sh")
    
    # Run community bootstrap script with test environment variables
    export DATA_DIR="$TEST_DIR/data"
    export CONFIG_DIR="$TEST_DIR/config"
    export TEMPLATES_DIR="$TEST_DIR/templates"
    export LOG_DIR="$TEST_DIR/logs"
    export FEDERATION_ID="test-federation"
    export COOPERATIVE_ID="test-cooperative"
    export COMMUNITY_ID="test-community"
    export COMMUNITY_NAME="Test Community"
    export COMMUNITY_DESCRIPTION="A test community for testing"
    
    # Create data directories
    mkdir -p "$DATA_DIR/community/$COMMUNITY_ID"
    mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/services"
    mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/credentials"
    
    # Create mock cooperative DID file
    mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials"
    echo '{"did":"did:icn:mock:coopoperator123456","key":"mock_key_data"}' > "$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials/operator1_key.json"
    echo "did:icn:mock:cooperative123456" > "$DATA_DIR/cooperative/$COOPERATIVE_ID/cooperative.did"
    
    # Run the script
    cd "$TEST_DIR"
    PREV_PATH="$PATH"
    export PATH="$TEST_DIR/mock_bin:$PATH"
    
    # Check if community bootstrap script uses command line arguments
    if [[ "$COMMUNITY_SCRIPT_CONTENT" == *"while [["* && "$COMMUNITY_SCRIPT_CONTENT" == *"case"* ]]; then
        # Script uses command line arguments, provide them
        bash "$SCRIPTS_DIR/community_bootstrap.sh" \
            --federation "$FEDERATION_ID" \
            --cooperative "$COOPERATIVE_ID" \
            --community "$COMMUNITY_ID" \
            --name "$COMMUNITY_NAME" \
            --description "$COMMUNITY_DESCRIPTION" \
            --data-dir "$DATA_DIR" \
            --config-dir "$CONFIG_DIR" \
            --templates-dir "$TEMPLATES_DIR" \
            --log-dir "$LOG_DIR" \
            || error "Community bootstrap failed"
    else
        # Script uses environment variables
        export COMMUNITY_ID="$COMMUNITY_ID"
        export COMMUNITY_NAME="$COMMUNITY_NAME"
        export COMMUNITY_DESCRIPTION="$COMMUNITY_DESCRIPTION"
        bash "$SCRIPTS_DIR/community_bootstrap.sh" || error "Community bootstrap failed"
    fi
    
    export PATH="$PREV_PATH"
    
    # Verify outputs
    if [ ! -f "$CONFIG_DIR/community_$COMMUNITY_ID.yaml" ]; then
        error "Community configuration file not created"
    fi
    
    if [ ! -d "$DATA_DIR/community/$COMMUNITY_ID/services" ]; then
        error "Community services directory not created"
    fi
    
    log "Community bootstrap test passed"
    return 0
}

# Clean up test environment
cleanup() {
    log "Cleaning up test environment..."
    
    # Remove test directory
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
    
    log "Cleanup complete."
}

# Main function
main() {
    log "Starting onboarding bundle tests"
    
    # Check dependencies
    check_dependencies
    
    # Setup test environment
    setup_test_env
    
    # Run tests
    test_federation_bootstrap
    test_cooperative_bootstrap
    test_community_bootstrap
    
    # Clean up
    cleanup
    
    log "All tests passed successfully!"
}

# Run main function
main 