# ICN v3 Onboarding Bundle

This onboarding bundle contains everything you need to deploy and operate a real-world federation in the InterCooperative Network (ICN) v3. The bundle includes configuration templates, bootstrap scripts, security guides, and policy seeds.

## Bundle Contents

### Configuration Templates
- `templates/federation_config.yaml` - Federation configuration template
- `templates/cooperative_config.yaml` - Cooperative configuration template
- `templates/community_config.yaml` - Community configuration template
- `templates/policies/` - Policy templates in Cooperative Contract Language (CCL)
  - `resource_allocation.ccl` - Resource allocation policy template
  - `dispute_resolution.ccl` - Dispute resolution policy template
  - `membership.ccl` - Membership policy template

### Bootstrap Scripts
- `scripts/federation_bootstrap.sh` - Script to bootstrap a new federation
- `scripts/cooperative_bootstrap.sh` - Script to bootstrap a new cooperative within a federation
- `scripts/community_bootstrap.sh` - Script to bootstrap a new community within a cooperative
- `scripts/test_onboarding.sh` - Test script to validate the onboarding flow

### Documentation
- `docs/security_and_recovery.md` - Security best practices and disaster recovery procedures
- `docs/upgrade_migration.md` - Instructions for upgrading and migrating between versions

## Getting Started

### Prerequisites

Before using this onboarding bundle, ensure you have:

1. ICN v3 software installed on your nodes
2. Required system dependencies:
   - jq for JSON processing
   - OpenSSL for cryptographic operations
   - Sufficient disk space for DAG store (~50GB recommended)

### Testing the Onboarding Scripts

Before deploying in a production environment, you can validate the entire onboarding flow using the test script:

```bash
# Run the test script to validate the federation, cooperative, and community bootstrap scripts
./scripts/test_onboarding.sh
```

The test script will:
- Set up a test environment with mock components
- Execute all bootstrap scripts in sequence
- Verify the outputs and configurations
- Report success or detailed errors for troubleshooting

This validation helps ensure your environment is correctly set up before deploying real components.

### Quick Start

Follow these steps to deploy a complete ICN v3 federation:

1. **Bootstrap a Federation**

```bash
# Create the federation
./scripts/federation_bootstrap.sh \
  --federation-id myfederation \
  --federation-name "My ICN Federation" \
  --data-dir /var/lib/icn
```

2. **Bootstrap a Cooperative**

```bash
# Create a cooperative within the federation
./scripts/cooperative_bootstrap.sh \
  --federation myfederation \
  --cooperative mycooperative \
  --name "My Cooperative" \
  --token-symbol MYCOOP \
  --token-name "My Cooperative Token" \
  --data-dir /var/lib/icn
```

3. **Bootstrap a Community**

```bash
# Create a community within the cooperative
./scripts/community_bootstrap.sh \
  --federation myfederation \
  --cooperative mycooperative \
  --community mycommunity \
  --name "My Community" \
  --education-pct 30 \
  --healthcare-pct 40 \
  --infrastructure-pct 30 \
  --data-dir /var/lib/icn
```

4. **Setup Monitoring (Optional but Recommended)**

```bash
# Set up monitoring for your federation
cd ../monitoring
./setup_monitoring_stack.sh
```

For production deployments, it's recommended to install monitoring as a systemd service:

```bash
# Install monitoring as a systemd service
sudo ./monitoring/install_monitoring.sh
```

The monitoring stack provides:
- Real-time metrics dashboard for your federation
- Performance and health monitoring
- Resource utilization tracking
- Alerts and notifications

## ICN Structure Overview

The ICN v3 platform uses a three-level organizational structure:

1. **Federation** - The highest level organization that coordinates between cooperatives, issues global credentials, mediates cross-cooperative transactions, and enforces multi-party governance policies.

2. **Cooperative** - Economic engines of the network that manage production, trade, token issuance, and economic operations for their members.

3. **Community** - Governance and public-service bodies responsible for policy-making, dispute resolution, and public goods. Communities operate within one cooperative.

## Configuration Templates

All configuration templates use environment variable placeholders (`${VARIABLE_NAME}`) that are replaced during the bootstrap process. You can customize these templates before running the bootstrap scripts.

## Security Considerations

The security of your ICN deployment depends on the secure handling of private keys and DIDs. Always:

- Store federation admin keys securely, preferably in hardware security modules
- Rotate keys regularly as described in the security guide
- Implement network security best practices
- Monitor your deployment for unusual activities

See `docs/security_and_recovery.md` for comprehensive security guidance.

## Monitoring and Observability

This onboarding bundle is designed to work with the ICN v3 monitoring stack (Prometheus, Grafana). The bootstrap scripts configure monitoring endpoints, and the templates include the necessary settings for metrics exporters.

For detailed monitoring setup instructions, see the [Monitoring README](../monitoring/README.md).

## Need Help?

If you encounter issues or need additional help:

- Review the detailed documentation in the `docs/` directory
- Check the [ICN v3 official documentation](https://icn.org/docs)
- Join the [ICN community forum](https://forum.icn.org)

## Contributing

If you'd like to contribute to this onboarding bundle, please submit pull requests or open issues in the ICN v3 repository. 