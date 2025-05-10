#!/bin/bash
# Community Bootstrap Script for ICN v3
# --------------------------------------
# This script bootstraps a new ICN v3 Community within a Cooperative by:
# 1. Generating DIDs for community officials
# 2. Creating a community configuration from template
# 3. Registering the community with the cooperative and federation
# 4. Setting up public service configurations
# 5. Establishing initial community policies

set -e

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --federation)
      FEDERATION_ID="$2"
      shift 2
      ;;
    --cooperative)
      COOPERATIVE_ID="$2"
      shift 2
      ;;
    --community)
      COMMUNITY_ID="$2"
      shift 2
      ;;
    --name)
      COMMUNITY_NAME="$2"
      shift 2
      ;;
    --description)
      COMMUNITY_DESCRIPTION="$2"
      shift 2
      ;;
    --official-count)
      OFFICIAL_COUNT="$2"
      shift 2
      ;;
    --education-pct)
      EDUCATION_PCT="$2"
      shift 2
      ;;
    --healthcare-pct)
      HEALTHCARE_PCT="$2"
      shift 2
      ;;
    --infrastructure-pct)
      INFRASTRUCTURE_PCT="$2"
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
COMMUNITY_ID="${COMMUNITY_ID:-govX}"
COMMUNITY_NAME="${COMMUNITY_NAME:-GovX Community}"
COMMUNITY_DESCRIPTION="${COMMUNITY_DESCRIPTION:-A governance community in ICN v3}"
OFFICIAL_COUNT="${OFFICIAL_COUNT:-3}"
EDUCATION_PCT="${EDUCATION_PCT:-30}"
HEALTHCARE_PCT="${HEALTHCARE_PCT:-40}"
INFRASTRUCTURE_PCT="${INFRASTRUCTURE_PCT:-30}"

# Validate required parameters
if [ -z "$FEDERATION_ID" ]; then
  echo "Error: Federation ID is required (--federation)"
  exit 1
fi

if [ -z "$COOPERATIVE_ID" ]; then
  echo "Error: Cooperative ID is required (--cooperative)"
  exit 1
fi

# Validate percentage allocation
TOTAL_PCT=$((EDUCATION_PCT + HEALTHCARE_PCT + INFRASTRUCTURE_PCT))
if [ $TOTAL_PCT -ne 100 ]; then
  echo "Error: Resource percentages must add up to 100% (currently $TOTAL_PCT%)"
  exit 1
fi

# Create directories
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID"
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/credentials"
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/policies"
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/services"
mkdir -p "$CONFIG_DIR"

echo "🔵 Bootstrapping ICN v3 Community: $COMMUNITY_NAME ($COMMUNITY_ID) in Cooperative: $COOPERATIVE_ID"

# Step 1: Generate DIDs for community officials
echo "🔑 Generating DIDs for community officials..."

OFFICIAL_DIDS=()
for i in $(seq 1 $OFFICIAL_COUNT); do
  KEY_FILE="$DATA_DIR/community/$COMMUNITY_ID/credentials/official${i}_key.json"
  
  if [ ! -f "$KEY_FILE" ]; then
    echo "  Creating DID for Official $i..."
    # Generate a keypair and DID
    icn-cli identity generate --output "$KEY_FILE"
  else
    echo "  Using existing DID for Official $i..."
  fi
  
  # Extract the DID
  DID=$(icn-cli identity inspect --input "$KEY_FILE" --format json | jq -r '.did')
  OFFICIAL_DIDS+=("$DID")
  
  echo "  Official $i DID: $DID"
done

# Step 2: Create community configuration from template
echo "📄 Creating community configuration..."

# Get the template from cooperative templates
TEMPLATE_SOURCE="$DATA_DIR/cooperative/$COOPERATIVE_ID/templates/community_config.yaml"
if [ ! -f "$TEMPLATE_SOURCE" ]; then
  # Fall back to federation template
  TEMPLATE_SOURCE="$DATA_DIR/federation/$FEDERATION_ID/templates/community_config.yaml"
  
  # If still not found, use default template
  if [ ! -f "$TEMPLATE_SOURCE" ]; then
    TEMPLATE_SOURCE="$TEMPLATES_DIR/community_config.yaml"
  fi
fi

# Create the community config from template
cp "$TEMPLATE_SOURCE" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"

# Replace placeholders in the configuration
sed -i "s/\${COMMUNITY_ID}/$COMMUNITY_ID/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${COMMUNITY_NAME}/$COMMUNITY_NAME/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${COMMUNITY_DESCRIPTION}/$COMMUNITY_DESCRIPTION/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${COOPERATIVE_ID}/$COOPERATIVE_ID/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s#\${DATA_DIR}#$DATA_DIR#g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s#\${LOG_DIR}#$LOG_DIR#g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"

# Replace resource allocations
sed -i "s/\${EDUCATION_PCT}/$EDUCATION_PCT/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${HEALTHCARE_PCT}/$HEALTHCARE_PCT/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
sed -i "s/\${INFRASTRUCTURE_PCT}/$INFRASTRUCTURE_PCT/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"

# Replace official DIDs in the configuration
for i in $(seq 0 $(($OFFICIAL_COUNT - 1))); do
  j=$(($i + 1))
  sed -i "s/\${OFFICIAL${j}_DID}/${OFFICIAL_DIDS[$i]}/g" "$CONFIG_DIR/community_$COMMUNITY_ID.yaml"
done

# Step 3: Register the community with the cooperative and federation
echo "🌐 Registering community with cooperative and federation..."

# We need a cooperative operator key to register the community
COOPERATIVE_OPERATOR_KEY="$DATA_DIR/cooperative/$COOPERATIVE_ID/credentials/operator1_key.json"
if [ ! -f "$COOPERATIVE_OPERATOR_KEY" ]; then
  echo "  Warning: Cooperative operator key not found at $COOPERATIVE_OPERATOR_KEY"
  echo "  You will need to manually register this community with the cooperative."
else
  # Create a community registration proposal
  REGISTRATION_PROPOSAL="$DATA_DIR/community/$COMMUNITY_ID/registration_proposal.json"
  
  echo "  Creating community registration proposal..."
  icn-cli proposal create \
    --title "Register Community: $COMMUNITY_NAME" \
    --template "community_registration" \
    --params "community_id=$COMMUNITY_ID,community_name=$COMMUNITY_NAME,cooperative_id=$COOPERATIVE_ID,federation_id=$FEDERATION_ID" \
    --output "$REGISTRATION_PROPOSAL"
    
  # Sign the proposal with cooperative operator key
  echo "  Signing proposal with cooperative operator key..."
  icn-cli proposal vote \
    --proposal "$REGISTRATION_PROPOSAL" \
    --key-file "$COOPERATIVE_OPERATOR_KEY" \
    --direction "yes" \
    --output "$REGISTRATION_PROPOSAL"
    
  # Submit the proposal to the cooperative (assumes cooperative node is running)
  echo "  Submitting proposal to cooperative..."
  COOPERATIVE_ENDPOINT="http://localhost:9100"  # Default cooperative endpoint
  icn-cli cooperative submit-proposal \
    --endpoint "$COOPERATIVE_ENDPOINT" \
    --proposal "$REGISTRATION_PROPOSAL" \
    --wait-for-execution
fi

# Step 4: Set up public service configurations
echo "🏛️ Setting up public service configurations..."

# Create services configuration directory
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/services"

# Create education service configuration
cat > "$DATA_DIR/community/$COMMUNITY_ID/services/education.yaml" << EOF
# Education Service Configuration
service:
  name: education
  description: "Education services for the community"
  allocation_percentage: $EDUCATION_PCT
  
  programs:
    - name: schools
      description: "Community schools and educational facilities"
      allocation_percentage: 50
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: scholarships
      description: "Educational scholarships for community members"
      allocation_percentage: 30
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: training
      description: "Vocational and skills training programs"
      allocation_percentage: 20
      token_budget: 0  # Will be filled during runtime based on total allocation
      
  access_control:
    administrators:
$(for i in $(seq 0 $(($OFFICIAL_COUNT - 1))); do
  echo "      - ${OFFICIAL_DIDS[$i]}"
done)
    
    beneficiaries: []  # Will be populated based on membership policy
EOF

# Create healthcare service configuration
cat > "$DATA_DIR/community/$COMMUNITY_ID/services/healthcare.yaml" << EOF
# Healthcare Service Configuration
service:
  name: healthcare
  description: "Healthcare services for the community"
  allocation_percentage: $HEALTHCARE_PCT
  
  programs:
    - name: clinics
      description: "Community clinics and medical facilities"
      allocation_percentage: 40
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: medicine
      description: "Medicine and medical supplies"
      allocation_percentage: 30
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: emergency
      description: "Emergency medical services"
      allocation_percentage: 30
      token_budget: 0  # Will be filled during runtime based on total allocation
      
  access_control:
    administrators:
$(for i in $(seq 0 $(($OFFICIAL_COUNT - 1))); do
  echo "      - ${OFFICIAL_DIDS[$i]}"
done)
    
    beneficiaries: []  # Will be populated based on membership policy
EOF

# Create infrastructure service configuration
cat > "$DATA_DIR/community/$COMMUNITY_ID/services/infrastructure.yaml" << EOF
# Infrastructure Service Configuration
service:
  name: infrastructure
  description: "Infrastructure services for the community"
  allocation_percentage: $INFRASTRUCTURE_PCT
  
  programs:
    - name: roads
      description: "Community road network and transportation infrastructure"
      allocation_percentage: 40
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: utilities
      description: "Utilities including water, power, and communications"
      allocation_percentage: 40
      token_budget: 0  # Will be filled during runtime based on total allocation
      
    - name: maintenance
      description: "Maintenance of community facilities"
      allocation_percentage: 20
      token_budget: 0  # Will be filled during runtime based on total allocation
      
  access_control:
    administrators:
$(for i in $(seq 0 $(($OFFICIAL_COUNT - 1))); do
  echo "      - ${OFFICIAL_DIDS[$i]}"
done)
    
    beneficiaries: []  # Will be populated based on membership policy
EOF

# Step 5: Set up community policies
echo "📜 Setting up community policies..."

# Create policies directory
mkdir -p "$DATA_DIR/community/$COMMUNITY_ID/policies"

# Copy policy templates from the templates directory
if [ -d "$TEMPLATES_DIR/policies" ]; then
  echo "  Copying policy templates..."
  cp "$TEMPLATES_DIR/policies/"*.ccl "$DATA_DIR/community/$COMMUNITY_ID/policies/templates/"
  
  # Process each policy template
  for POLICY_FILE in "$DATA_DIR/community/$COMMUNITY_ID/policies/templates/"*.ccl; do
    POLICY_NAME=$(basename "$POLICY_FILE" .ccl)
    echo "  Processing $POLICY_NAME policy..."
    
    # Create a processed version with variables replaced
    PROCESSED_POLICY="$DATA_DIR/community/$COMMUNITY_ID/policies/${POLICY_NAME}.ccl"
    cp "$POLICY_FILE" "$PROCESSED_POLICY"
    
    # Replace variables in the policy
    sed -i "s/\${COMMUNITY_ID}/$COMMUNITY_ID/g" "$PROCESSED_POLICY"
    sed -i "s/\${COOPERATIVE_ID}/$COOPERATIVE_ID/g" "$PROCESSED_POLICY"
    sed -i "s/\${FEDERATION_ID}/$FEDERATION_ID/g" "$PROCESSED_POLICY"
    sed -i "s/\${EDUCATION_PCT}/$EDUCATION_PCT/g" "$PROCESSED_POLICY"
    sed -i "s/\${HEALTHCARE_PCT}/$HEALTHCARE_PCT/g" "$PROCESSED_POLICY"
    sed -i "s/\${INFRASTRUCTURE_PCT}/$INFRASTRUCTURE_PCT/g" "$PROCESSED_POLICY"
    
    # Set default membership verification type to vouching
    sed -i "s/\${IDENTITY_VERIFICATION_TYPE}/vouching/g" "$PROCESSED_POLICY"
    sed -i "s/\${MIN_VOUCHES}/2/g" "$PROCESSED_POLICY"
    sed -i "s/\${CONTRIBUTION_TYPE}/token/g" "$PROCESSED_POLICY"
    sed -i "s/\${MIN_CONTRIBUTION_AMOUNT}/100/g" "$PROCESSED_POLICY"
    
    # Set default quorum values
    sed -i "s/\${QUORUM_TYPE}/MAJORITY/g" "$PROCESSED_POLICY"
    sed -i "s/\${THRESHOLD}/3/g" "$PROCESSED_POLICY"
    sed -i "s/\${VOTING_PERIOD}/86400/g" "$PROCESSED_POLICY"
  done
  
  # Create proposal for each policy (to be executed when the community node is running)
  echo "  Creating policy deployment instructions..."
  cat > "$DATA_DIR/community/$COMMUNITY_ID/deploy_policies.sh" << EOF
#!/bin/bash
# Policy deployment script for Community $COMMUNITY_ID
# Run this after the community node is running

set -e

COMMUNITY_ID="$COMMUNITY_ID"
POLICY_DIR="$DATA_DIR/community/$COMMUNITY_ID/policies"
CREDENTIAL_DIR="$DATA_DIR/community/$COMMUNITY_ID/credentials"

echo "Deploying policies for Community \$COMMUNITY_ID"

for POLICY_FILE in "\$POLICY_DIR"/*.ccl; do
  POLICY_NAME=\$(basename "\$POLICY_FILE" .ccl)
  echo "  Deploying \$POLICY_NAME policy..."
  
  # Create proposal from CCL
  PROPOSAL_FILE="\$POLICY_DIR/\${POLICY_NAME}_proposal.json"
  icn-cli proposal create \\
    --ccl-file "\$POLICY_FILE" \\
    --title "\${POLICY_NAME^} Policy" \\
    --output "\$PROPOSAL_FILE"
  
  # Sign with official key
  icn-cli proposal vote \\
    --proposal "\$PROPOSAL_FILE" \\
    --key-file "\$CREDENTIAL_DIR/official1_key.json" \\
    --direction "yes" \\
    --output "\$PROPOSAL_FILE"
  
  # Submit to community node
  COMMUNITY_ENDPOINT="http://localhost:9200"  # Default community endpoint
  icn-cli community submit-proposal \\
    --endpoint "\$COMMUNITY_ENDPOINT" \\
    --proposal "\$PROPOSAL_FILE" \\
    --wait-for-execution
done

echo "✅ All policies deployed"
EOF

  chmod +x "$DATA_DIR/community/$COMMUNITY_ID/deploy_policies.sh"
fi

# Final output
echo "✅ Community bootstrap complete!"
echo "   Community ID: $COMMUNITY_ID"
echo "   Community Name: $COMMUNITY_NAME"
echo "   Cooperative ID: $COOPERATIVE_ID"
echo "   Federation ID: $FEDERATION_ID"
echo "   Configuration: $CONFIG_DIR/community_$COMMUNITY_ID.yaml"
echo "   Data Directory: $DATA_DIR/community/$COMMUNITY_ID"
echo ""
echo "To start the community node, run:"
echo "  icn-community-node --config $CONFIG_DIR/community_$COMMUNITY_ID.yaml"
echo ""
echo "After starting the node, deploy the policies with:"
echo "  $DATA_DIR/community/$COMMUNITY_ID/deploy_policies.sh" 