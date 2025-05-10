#!/bin/bash

# Initialize federation script
# Usage: ./scripts/init_federation.sh
# This script sets up a federation with three nodes, creates trusted signers,
# and anchors a TrustBundle to the DAG

set -e

echo "🚀 Initializing federation..."

# Create directories
mkdir -p ./data/keys
mkdir -p ./data/federation

# Generate 3 signer keypairs
echo "🔑 Generating signer keypairs..."
for i in {1..3}; do
  cargo run -p icn-cli -- keypair generate --output "./data/keys/signer-$i.json"
done

# Extract DIDs from the keypair files
SIGNER1_DID=$(jq -r .did ./data/keys/signer-1.json)
SIGNER2_DID=$(jq -r .did ./data/keys/signer-2.json)
SIGNER3_DID=$(jq -r .did ./data/keys/signer-3.json)

echo "👤 Signer 1 DID: $SIGNER1_DID"
echo "👤 Signer 2 DID: $SIGNER2_DID"
echo "👤 Signer 3 DID: $SIGNER3_DID"

# Create a federation trust bundle with the signers
echo "🔗 Creating federation trust bundle..."
cargo run -p icn-cli -- federation create \
  --name "Demo Federation" \
  --description "A demo federation created by script" \
  --signers "${SIGNER1_DID},${SIGNER2_DID},${SIGNER3_DID}" \
  --quorum-type majority \
  --output ./data/federation/trust-bundle.json

# Start the federation nodes (placeholder, would use docker-compose in real implementation)
echo "🌐 Starting federation nodes..."
echo "Node 1: Starting..."
echo "Node 2: Starting..."
echo "Node 3: Starting..."

# Anchor the trust bundle to the DAG
echo "📌 Anchoring trust bundle to DAG..."
TRUST_BUNDLE_CID=$(cargo run -p icn-cli -- federation anchor \
  --bundle ./data/federation/trust-bundle.json \
  --node-api http://localhost:7001 \
  --output ./data/federation/anchored-bundle.json)

echo "✅ Federation initialized successfully!"
echo "Trust Bundle CID: ${TRUST_BUNDLE_CID}"
echo ""
echo "To verify the trust bundle:"
echo "cargo run -p icn-cli -- federation verify --cid ${TRUST_BUNDLE_CID} --node-api http://localhost:7001" 