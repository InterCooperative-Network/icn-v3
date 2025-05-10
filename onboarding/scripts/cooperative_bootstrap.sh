#!/bin/bash
# Cooperative Bootstrap Script for ICN v3
# --------------------------------------
# This script bootstraps a new ICN v3 Cooperative within a Federation by:
# 1. Generating DIDs for cooperative operators
# 2. Creating a cooperative configuration from template
# 3. Registering the cooperative with the federation
# 4. Setting up token parameters and minting capabilities
# 5. Establishing initial economic policies

set -e

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --federation)
      FEDERATION_ID="$2"
      shift 2
      ;;
    --trust-bundle)
      TRUST_BUNDLE_CID="$2"
      shift 2
      ;;
    --cooperative)
      COOPERATIVE_ID="$2"
      shift 2
      ;;
    --name)
      COOPERATIVE_NAME="$2"
      shift 2
      ;;
    --description)
      COOPERATIVE_DESCRIPTION="$2"
      shift 2
      ;;
    --token-symbol)
      TOKEN_SYMBOL="$2"
      shift 2
      ;;
    --token-name)
      TOKEN_NAME="$2"
      shift 2
      ;;
    --operator-count)
      OPERATOR_COUNT="$2"
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
    --templates-dir)
      TEMPLATES_DIR="$2"
      shift 2
      ;;
    --log-dir)
      LOG_DIR="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Configuration with defaults
DATA_DIR="${DATA_DIR:-/var/lib/icn}"
CONFIG_DIR="${CONFIG_DIR:-$PWD/config}"
TEMPLATES_DIR="${TEMPLATES_DIR:-$PWD/templates}"
LOG_DIR="${LOG_DIR:-/var/log/icn}"
FEDERATION_ID="${FEDERATION_ID:-alpha}"
COOPERATIVE_ID="${COOPERATIVE_ID:-econA}"
COOPERATIVE_NAME="${COOPERATIVE_NAME:-EconA Cooperative}"
COOPERATIVE_DESCRIPTION="${COOPERATIVE_DESCRIPTION:-An economic cooperative in ICN v3}"
TOKEN_SYMBOL="${TOKEN_SYMBOL:-ECOA}"
TOKEN_NAME="${TOKEN_NAME:-EconA Token}"
OPERATOR_COUNT="${OPERATOR_COUNT:-3}"
TRUST_BUNDLE_CID="${TRUST_BUNDLE_CID:-}"

# Validate required parameters
if [ -z "$FEDERATION_ID" ]; then
  echo "Error: Federation ID is required (--federation)"
  exit 1
fi

if [ -z "$TRUST_BUNDLE_CID" ]; then
  echo "Error: Trust bundle CID is required (--trust-bundle)"
  exit 1
fi

# Create directories
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID"
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials"
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/policies"
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/templates"
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/token"
mkdir -p "$CONFIG_DIR"

echo "🔵 Bootstrapping ICN v3 Cooperative: $COOPERATIVE_NAME ($COOPERATIVE_ID) in Federation: $FEDERATION_ID"

# Step 1: Generate DIDs for cooperative operators
echo "🔑 Generating DIDs for cooperative operators..."

OPERATOR_DIDS=()
for i in $(seq 1 $OPERATOR_COUNT); do
  KEY_FILE="$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials/operator${i}_key.json"
  
  if [ ! -f "$KEY_FILE" ]; then
    echo "  Creating DID for Operator $i..."
    # Generate a keypair and DID
    icn-cli identity generate --output "$KEY_FILE"
  else
    echo "  Using existing DID for Operator $i..."
  fi
  
  # Extract the DID
  DID=$(icn-cli identity inspect --input "$KEY_FILE" --format json | jq -r '.did')
  OPERATOR_DIDS+=("$DID")
  
  echo "  Operator $i DID: $DID"
done

# Step 2: Create cooperative configuration from template
echo "📄 Creating cooperative configuration..."

# Get the template from federation templates
TEMPLATE_SOURCE="$DATA_DIR/federation/$FEDERATION_ID/templates/cooperative_config.yaml"
if [ ! -f "$TEMPLATE_SOURCE" ]; then
  # Fall back to default template
  TEMPLATE_SOURCE="$TEMPLATES_DIR/cooperative_config.yaml"
fi

# Create the cooperative config from template
cp "$TEMPLATE_SOURCE" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"

# Replace placeholders in the configuration
sed -i "s/\${COOPERATIVE_ID}/$COOPERATIVE_ID/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s/\${COOPERATIVE_NAME}/$COOPERATIVE_NAME/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s/\${COOPERATIVE_DESCRIPTION}/$COOPERATIVE_DESCRIPTION/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s/\${TOKEN_SYMBOL}/$TOKEN_SYMBOL/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s/\${TOKEN_NAME}/$TOKEN_NAME/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s#\${DATA_DIR}#$DATA_DIR#g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
sed -i "s#\${LOG_DIR}#$LOG_DIR#g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"

# Replace operator DIDs in the configuration
for i in $(seq 0 $(($OPERATOR_COUNT - 1))); do
  j=$(($i + 1))
  sed -i "s/\${OPERATOR${j}_DID}/${OPERATOR_DIDS[$i]}/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
  # Use the same DIDs for minters (can be changed later)
  sed -i "s/\${MINTER${j}_DID}/${OPERATOR_DIDS[$i]}/g" "$CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
done

# Copy community template for future use
cp "$TEMPLATES_DIR/community_config.yaml" "$DATA_DIR/cooperative/$COOPERATIVE_ID/templates/"

# Step 3: Register the cooperative with the federation
echo "🌐 Registering cooperative with federation..."

# We need a federation admin key to register the cooperative
FEDERATION_ADMIN_KEY="$DATA_DIR/federation/$FEDERATION_ID/credentials/admin1_key.json"
if [ ! -f "$FEDERATION_ADMIN_KEY" ]; then
  echo "  Warning: Federation admin key not found at $FEDERATION_ADMIN_KEY"
  echo "  You will need to manually register this cooperative with the federation."
else
  # Create a cooperative registration proposal
  REGISTRATION_PROPOSAL="$DATA_DIR/cooperative/$COOPERATIVE_ID/registration_proposal.json"
  
  echo "  Creating cooperative registration proposal..."
  icn-cli proposal create \
    --title "Register Cooperative: $COOPERATIVE_NAME" \
    --template "cooperative_registration" \
    --params "coop_id=$COOPERATIVE_ID,coop_name=$COOPERATIVE_NAME,federation_id=$FEDERATION_ID" \
    --output "$REGISTRATION_PROPOSAL"
    
  # Sign the proposal with federation admin key
  echo "  Signing proposal with federation admin key..."
  icn-cli proposal vote \
    --proposal "$REGISTRATION_PROPOSAL" \
    --key-file "$FEDERATION_ADMIN_KEY" \
    --direction "yes" \
    --output "$REGISTRATION_PROPOSAL"
    
  # Submit the proposal to the federation (assumes federation node is running)
  echo "  Submitting proposal to federation..."
  FEDERATION_ENDPOINT="http://localhost:9000"  # Default federation endpoint
  icn-cli federation submit-proposal \
    --endpoint "$FEDERATION_ENDPOINT" \
    --proposal "$REGISTRATION_PROPOSAL" \
    --wait-for-execution
fi

# Step 4: Set up token parameters and minting capabilities
echo "💰 Setting up token parameters..."

# Generate token configuration
TOKEN_CONFIG="$DATA_DIR/cooperative/$COOPERATIVE_ID/token/config.json"
cat > "$TOKEN_CONFIG" << EOF
{
  "symbol": "$TOKEN_SYMBOL",
  "name": "$TOKEN_NAME",
  "decimals": 6,
  "initial_supply": 1000000,
  "minting_policy": {
    "requires_quorum": true,
    "authorized_minters": [
$(for i in $(seq 0 $(($OPERATOR_COUNT - 1))); do
  echo "      \"${OPERATOR_DIDS[$i]}\""
  if [ $i -lt $(($OPERATOR_COUNT - 1)) ]; then
    echo ","
  fi
done)
    ]
  },
  "transfer_policy": {
    "permit_external": true,
    "external_federations": []
  }
}
EOF

echo "  Token configuration created at $TOKEN_CONFIG"

# Create initial token supply (if federation node is running)
echo "  Creating initial token supply..."
icn-cli token mint \
  --config "$TOKEN_CONFIG" \
  --coop-id "$COOPERATIVE_ID" \
  --federation-id "$FEDERATION_ID" \
  --amount 1000000 \
  --recipient "${OPERATOR_DIDS[0]}" \
  --key-file "$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials/operator1_key.json" \
  --note "Initial token supply" \
  --dry-run

echo "  Note: To actually mint tokens, run the above command without --dry-run when the federation node is running."

# Step 5: Establish initial economic policies
echo "📜 Setting up initial economic policies..."

# Create economic policy directory
mkdir -p "$DATA_DIR/cooperative/$COOPERATIVE_ID/policies/economic"

# Create a simple token issuance policy
cat > "$DATA_DIR/cooperative/$COOPERATIVE_ID/policies/economic/token_issuance.ccl" << EOF
# Token Issuance Policy
proposal "Token Issuance Policy" {
  scope "cooperative/${COOPERATIVE_ID}/economic/token"
  
  token_policy "issuance" {
    symbol "${TOKEN_SYMBOL}"
    name "${TOKEN_NAME}"
    decimals 6
    
    minting {
      requires_quorum true
      
      authorized_minters [
$(for i in $(seq 0 $(($OPERATOR_COUNT - 1))); do
  echo "        \"${OPERATOR_DIDS[$i]}\""
  if [ $i -lt $(($OPERATOR_COUNT - 1)) ]; then
    echo ","
  fi
done)
      ]
      
      limits {
        max_per_transaction 1000000
        max_per_day 10000000
      }
    }
    
    transfers {
      permit_external true
      requires_approval false
      
      limits {
        max_per_transaction 1000000
        max_per_day 5000000
      }
    }
  }
  
  access_control {
    role "coop_operator" {
      permission "modify_policy" {
        grant true
      }
    }
  }
}
EOF

# Final output
echo "✅ Cooperative bootstrap complete!"
echo "   Cooperative ID: $COOPERATIVE_ID"
echo "   Cooperative Name: $COOPERATIVE_NAME"
echo "   Federation ID: $FEDERATION_ID"
echo "   Configuration: $CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
echo "   Data Directory: $DATA_DIR/cooperative/$COOPERATIVE_ID"
echo ""
echo "To start the cooperative node, run:"
echo "  icn-cooperative-node --config $CONFIG_DIR/cooperative_$COOPERATIVE_ID.yaml"
echo ""
echo "To bootstrap a new community within this cooperative, run:"
echo "  ./community_bootstrap.sh --federation $FEDERATION_ID --cooperative $COOPERATIVE_ID" 