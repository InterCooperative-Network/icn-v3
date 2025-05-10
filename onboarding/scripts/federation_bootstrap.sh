#!/bin/bash
# Federation Bootstrap Script for ICN v3
# --------------------------------------
# This script bootstraps a new ICN v3 Federation by:
# 1. Generating DIDs for federation administrators
# 2. Creating a federation configuration from template
# 3. Initializing the federation's DAG store
# 4. Establishing trust roots and issuing initial credentials
# 5. Seeding governance policies

set -e

# Configuration
DATA_DIR="${DATA_DIR:-/var/lib/icn}"
CONFIG_DIR="${CONFIG_DIR:-$PWD/config}"
TEMPLATES_DIR="${TEMPLATES_DIR:-$PWD/templates}"
LOG_DIR="${LOG_DIR:-/var/log/icn}"
FEDERATION_ID="${FEDERATION_ID:-alpha}"
FEDERATION_NAME="${FEDERATION_NAME:-Alpha Federation}"
FEDERATION_DESCRIPTION="${FEDERATION_DESCRIPTION:-A test federation for ICN v3}"
ADMIN_COUNT="${ADMIN_COUNT:-3}"
MONITORING_ENABLED="${MONITORING_ENABLED:-true}"

# Create directories
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID"
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/dag_store"
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/credentials"
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/policies"
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/templates"
mkdir -p "$CONFIG_DIR"
mkdir -p "$LOG_DIR"

echo "🔵 Bootstrapping ICN v3 Federation: $FEDERATION_NAME ($FEDERATION_ID)"

# Step 1: Generate DIDs for federation administrators
echo "🔑 Generating DIDs for federation administrators..."

ADMIN_DIDS=()
for i in $(seq 1 $ADMIN_COUNT); do
  KEY_FILE="$DATA_DIR/federation/$FEDERATION_ID/credentials/admin${i}_key.json"
  
  if [ ! -f "$KEY_FILE" ]; then
    echo "  Creating DID for Admin $i..."
    # Generate a keypair and DID
    icn-cli identity generate --output "$KEY_FILE"
  else
    echo "  Using existing DID for Admin $i..."
  fi
  
  # Extract the DID
  DID=$(icn-cli identity inspect --input "$KEY_FILE" --format json | jq -r '.did')
  ADMIN_DIDS+=("$DID")
  
  echo "  Admin $i DID: $DID"
done

# Step 2: Create federation configuration from template
echo "📄 Creating federation configuration..."

# Create the federation config from template
cp "$TEMPLATES_DIR/federation_config.yaml" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"

# Replace placeholders in the configuration
sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"
sed -i "s/\${FEDERATION_NAME}/$FEDERATION_NAME/g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"
sed -i "s/\${FEDERATION_DESCRIPTION}/$FEDERATION_DESCRIPTION/g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"
sed -i "s#\${DATA_DIR}#$DATA_DIR#g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"
sed -i "s#\${LOG_DIR}#$LOG_DIR#g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"

# Replace admin DIDs in the configuration
for i in $(seq 0 $(($ADMIN_COUNT - 1))); do
  j=$(($i + 1))
  sed -i "s/\${ADMIN${j}_DID}/${ADMIN_DIDS[$i]}/g" "$CONFIG_DIR/federation_$FEDERATION_ID.yaml"
done

# Copy templates for cooperatives and communities
cp "$TEMPLATES_DIR/cooperative_config.yaml" "$DATA_DIR/federation/$FEDERATION_ID/templates/"
cp "$TEMPLATES_DIR/community_config.yaml" "$DATA_DIR/federation/$FEDERATION_ID/templates/"

# Step 3: Initialize the federation's DAG store
echo "💽 Initializing federation DAG store..."
icn-cli dag init --path "$DATA_DIR/federation/$FEDERATION_ID/dag_store"

# Step 4: Establish trust roots and issue initial credentials
echo "🔐 Establishing trust roots..."

# Create trust bundle with admin signatures
TRUST_BUNDLE_FILE="$DATA_DIR/federation/$FEDERATION_ID/trust_bundle.json"

# Initialize trust bundle with federation metadata
icn-cli trust init-bundle \
  --federation-id "$FEDERATION_ID" \
  --federation-name "$FEDERATION_NAME" \
  --output "$TRUST_BUNDLE_FILE"

# Add admin signatures to trust bundle
for i in $(seq 1 $ADMIN_COUNT); do
  KEY_FILE="$DATA_DIR/federation/$FEDERATION_ID/credentials/admin${i}_key.json"
  
  echo "  Adding signature from Admin $i..."
  icn-cli trust sign-bundle \
    --input "$TRUST_BUNDLE_FILE" \
    --key-file "$KEY_FILE" \
    --output "$TRUST_BUNDLE_FILE"
done

# Finalize the trust bundle with quorum proof
icn-cli trust finalize-bundle \
  --input "$TRUST_BUNDLE_FILE" \
  --quorum-type "MAJORITY" \
  --output "$TRUST_BUNDLE_FILE"

# Get the trust bundle CID
TRUST_BUNDLE_CID=$(icn-cli trust inspect-bundle --input "$TRUST_BUNDLE_FILE" --format json | jq -r '.cid')
echo "  Trust bundle created with CID: $TRUST_BUNDLE_CID"

# Anchor the trust bundle to the DAG store
icn-cli dag add \
  --path "$DATA_DIR/federation/$FEDERATION_ID/dag_store" \
  --input "$TRUST_BUNDLE_FILE" \
  --tag "trust_bundle:$FEDERATION_ID"

# Step 5: Seed governance policies
echo "📜 Seeding federation governance policies..."

# Copy policy templates
mkdir -p "$DATA_DIR/federation/$FEDERATION_ID/policies/templates"
cp "$TEMPLATES_DIR/policies/"*.ccl "$DATA_DIR/federation/$FEDERATION_ID/policies/templates/"

# Deploy basic governance policies
for POLICY_FILE in "$DATA_DIR/federation/$FEDERATION_ID/policies/templates/"*.ccl; do
  POLICY_NAME=$(basename "$POLICY_FILE" .ccl)
  echo "  Deploying $POLICY_NAME policy..."
  
  # Process the template (replace variables)
  PROCESSED_POLICY="$DATA_DIR/federation/$FEDERATION_ID/policies/${POLICY_NAME}.ccl"
  cp "$POLICY_FILE" "$PROCESSED_POLICY"
  sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$PROCESSED_POLICY"
  sed -i "s/\${COMMUNITY_ID}/federation/g" "$PROCESSED_POLICY"
  
  # Compile the policy to a governance proposal
  PROPOSAL_FILE="$DATA_DIR/federation/$FEDERATION_ID/policies/${POLICY_NAME}_proposal.json"
  icn-cli proposal create \
    --ccl-file "$PROCESSED_POLICY" \
    --title "${POLICY_NAME^} Policy" \
    --output "$PROPOSAL_FILE"
  
  # Sign the proposal with admin keys (simulate voting)
  for i in $(seq 1 $ADMIN_COUNT); do
    KEY_FILE="$DATA_DIR/federation/$FEDERATION_ID/credentials/admin${i}_key.json"
    
    icn-cli proposal vote \
      --proposal "$PROPOSAL_FILE" \
      --key-file "$KEY_FILE" \
      --direction "yes" \
      --output "$PROPOSAL_FILE"
  done
  
  # Execute the proposal to deploy the policy
  icn-cli runtime execute-ccl \
    --input "$PROCESSED_POLICY" \
    --output "$DATA_DIR/federation/$FEDERATION_ID/policies/${POLICY_NAME}_receipt.json"
done

# Step 6: Set up monitoring if enabled
if [ "$MONITORING_ENABLED" = true ]; then
  echo "📊 Setting up federation monitoring..."
  
  # Copy monitoring templates
  cp -r "$TEMPLATES_DIR/monitoring/"* "$DATA_DIR/federation/$FEDERATION_ID/monitoring/"
  
  # Set up Prometheus with federation-specific configuration
  PROMETHEUS_CONFIG="$DATA_DIR/federation/$FEDERATION_ID/monitoring/prometheus.yml"
  sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$PROMETHEUS_CONFIG"
  
  # Set up Grafana with federation-specific dashboards
  GRAFANA_DASHBOARD="$DATA_DIR/federation/$FEDERATION_ID/monitoring/grafana-dashboard-federation.json"
  sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$GRAFANA_DASHBOARD"
  sed -i "s/\${FEDERATION_NAME}/$FEDERATION_NAME/g" "$GRAFANA_DASHBOARD"
fi

# Final output
echo "✅ Federation bootstrap complete!"
echo "   Federation ID: $FEDERATION_ID"
echo "   Federation Name: $FEDERATION_NAME"
echo "   Trust Bundle CID: $TRUST_BUNDLE_CID"
echo "   Configuration: $CONFIG_DIR/federation_$FEDERATION_ID.yaml"
echo "   Data Directory: $DATA_DIR/federation/$FEDERATION_ID"
echo ""
echo "To start the federation node, run:"
echo "  icn-federation-node --config $CONFIG_DIR/federation_$FEDERATION_ID.yaml"
echo ""
echo "To bootstrap a new cooperative within this federation, run:"
echo "  ./cooperative_bootstrap.sh --federation $FEDERATION_ID --trust-bundle $TRUST_BUNDLE_CID" 