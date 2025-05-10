# ICN v3 Upgrade and Migration Guide

This guide provides instructions for upgrading an existing ICN deployment to new versions and migrating from previous ICN versions (v1, v2) to ICN v3.

## Table of Contents
- [Version Upgrade Process](#version-upgrade-process)
  - [Minor Version Upgrades](#minor-version-upgrades)
  - [Major Version Upgrades](#major-version-upgrades)
  - [Rollback Procedures](#rollback-procedures)
- [Migration from Previous ICN Versions](#migration-from-previous-icn-versions)
  - [ICN v1 to v3 Migration](#icn-v1-to-v3-migration)
  - [ICN v2 to v3 Migration](#icn-v2-to-v3-migration)
- [Schema and Data Migrations](#schema-and-data-migrations)
  - [DAG Store Migration](#dag-store-migration)
  - [Policy Migration](#policy-migration)
  - [Token State Migration](#token-state-migration)

## Version Upgrade Process

### Minor Version Upgrades

Minor version upgrades (e.g., v3.1.0 to v3.2.0) typically add new features and fix bugs without changing the underlying data structures or APIs significantly. These upgrades are generally backward compatible.

#### Preparation

1. **Read Release Notes**
   - Review the release notes for the new version
   - Note any new features, bug fixes, or configuration changes
   - Check for any deprecated features

2. **Backup Current System**
   ```bash
   # Create a full backup of all ICN data
   icn-cli backup create --type full --output /path/to/backups/
   
   # Backup configuration files
   cp -r /etc/icn /path/to/backups/config/
   ```

3. **Test in Development Environment**
   - Deploy the new version in a test environment
   - Verify that all functionality works as expected
   - Test any integration points with external systems

#### Upgrade Steps

1. **Stop ICN Services**
   ```bash
   # Stop all ICN nodes in reverse order
   systemctl stop icn-community-node
   systemctl stop icn-cooperative-node
   systemctl stop icn-federation-node
   ```

2. **Update Packages**
   ```bash
   # For package-based installations
   apt update
   apt install icn-federation-node icn-cooperative-node icn-community-node icn-cli
   
   # For container-based installations
   docker pull intercoopnet/federation-node:v3.2.0
   docker pull intercoopnet/cooperative-node:v3.2.0
   docker pull intercoopnet/community-node:v3.2.0
   ```

3. **Update Configuration**
   ```bash
   # Apply configuration changes if needed
   icn-cli config migrate --input /etc/icn/federation_alpha.yaml --output /etc/icn/federation_alpha.yaml
   ```

4. **Start Services**
   ```bash
   # Start services in correct order
   systemctl start icn-federation-node
   # Wait for federation to start completely
   sleep 10
   systemctl start icn-cooperative-node
   # Wait for cooperative to start completely
   sleep 10
   systemctl start icn-community-node
   ```

5. **Verify Upgrade**
   ```bash
   # Check version and status
   icn-cli federation status --endpoint http://localhost:9000
   icn-cli cooperative status --endpoint http://localhost:9100
   icn-cli community status --endpoint http://localhost:9200
   ```

### Major Version Upgrades

Major version upgrades (e.g., v3.0.0 to v4.0.0) typically involve significant changes to architecture, APIs, and data structures. These upgrades often require careful planning and execution.

#### Preparation

1. **Comprehensive Planning**
   - Create a detailed upgrade plan including:
     - Timeline with specific milestones
     - Resource allocation
     - Testing strategy
     - Rollback plan

2. **Create Full System Backup**
   ```bash
   # Create a full offline backup
   icn-cli backup create --type full --offline true --output /path/to/backups/
   
   # Verify backup integrity
   icn-cli backup verify --path /path/to/backups/
   ```

3. **Development Environment Testing**
   - Set up a complete test environment
   - Perform a trial upgrade
   - Document any issues and their resolutions
   - Perform load testing to ensure performance

#### Upgrade Steps

1. **Announce Maintenance Window**
   - Notify all users of the planned maintenance
   - Provide estimated downtime
   - Share key changes and improvements

2. **Stop All Services**
   ```bash
   # Stop all ICN nodes
   systemctl stop icn-community-node
   systemctl stop icn-cooperative-node
   systemctl stop icn-federation-node
   ```

3. **Install New Version**
   ```bash
   # For package-based installations
   apt update
   apt install icn-federation-node icn-cooperative-node icn-community-node icn-cli
   
   # For container-based installations
   docker pull intercoopnet/federation-node:v4.0.0
   docker pull intercoopnet/cooperative-node:v4.0.0
   docker pull intercoopnet/community-node:v4.0.0
   ```

4. **Migrate Data**
   ```bash
   # Run the data migration tool
   icn-cli migrate v3-to-v4 --data-dir /var/lib/icn
   ```

5. **Update Configuration**
   ```bash
   # Generate new configuration based on old
   icn-cli config migrate \
     --input /etc/icn/federation_alpha.yaml \
     --output /etc/icn/federation_alpha.v4.yaml \
     --version v4
   
   # Review and apply new configuration
   mv /etc/icn/federation_alpha.v4.yaml /etc/icn/federation_alpha.yaml
   ```

6. **Start Federation Service**
   ```bash
   systemctl start icn-federation-node
   
   # Verify federation node is running correctly
   icn-cli federation status --endpoint http://localhost:9000
   ```

7. **Update Trust Bundle**
   ```bash
   # Create a new v4 trust bundle
   icn-cli trust init-bundle-v4 \
     --federation-id alpha \
     --federation-name "Alpha Federation" \
     --import-from-v3 true \
     --output /var/lib/icn/federation/alpha/trust_bundle.json
   
   # Sign with admin keys
   icn-cli trust sign-bundle \
     --input /var/lib/icn/federation/alpha/trust_bundle.json \
     --key-file /var/lib/icn/federation/alpha/credentials/admin1_key.json \
     --output /var/lib/icn/federation/alpha/trust_bundle.json
   
   # Finalize and distribute
   icn-cli trust finalize-bundle \
     --input /var/lib/icn/federation/alpha/trust_bundle.json \
     --quorum-type "MAJORITY" \
     --output /var/lib/icn/federation/alpha/trust_bundle.json
   ```

8. **Start Cooperative and Community Services**
   ```bash
   # Start cooperative node
   systemctl start icn-cooperative-node
   
   # Verify cooperative node is running correctly
   icn-cli cooperative status --endpoint http://localhost:9100
   
   # Start community node
   systemctl start icn-community-node
   
   # Verify community node is running correctly
   icn-cli community status --endpoint http://localhost:9200
   ```

9. **Verify Upgrade**
   ```bash
   # Run comprehensive verification
   icn-cli system verify --federation-endpoint http://localhost:9000
   ```

### Rollback Procedures

In case of critical issues during the upgrade, follow these rollback procedures.

#### Minor Version Rollback

1. **Stop Services**
   ```bash
   systemctl stop icn-community-node
   systemctl stop icn-cooperative-node
   systemctl stop icn-federation-node
   ```

2. **Reinstall Previous Version**
   ```bash
   # For package-based installations
   apt install icn-federation-node=3.1.0 icn-cooperative-node=3.1.0 icn-community-node=3.1.0 icn-cli=3.1.0
   
   # For container-based installations
   docker pull intercoopnet/federation-node:v3.1.0
   docker pull intercoopnet/cooperative-node:v3.1.0
   docker pull intercoopnet/community-node:v3.1.0
   ```

3. **Restore Configuration**
   ```bash
   cp /path/to/backups/config/* /etc/icn/
   ```

4. **Start Services**
   ```bash
   systemctl start icn-federation-node
   sleep 10
   systemctl start icn-cooperative-node
   sleep 10
   systemctl start icn-community-node
   ```

#### Major Version Rollback

1. **Stop All Services**
   ```bash
   systemctl stop icn-community-node
   systemctl stop icn-cooperative-node
   systemctl stop icn-federation-node
   ```

2. **Reinstall Previous Version**
   ```bash
   # For package-based installations
   apt install icn-federation-node=3.0.0 icn-cooperative-node=3.0.0 icn-community-node=3.0.0 icn-cli=3.0.0
   
   # For container-based installations
   docker pull intercoopnet/federation-node:v3.0.0
   docker pull intercoopnet/cooperative-node:v3.0.0
   docker pull intercoopnet/community-node:v3.0.0
   ```

3. **Restore Data from Backup**
   ```bash
   # Restore data from backup
   icn-cli backup restore --path /path/to/backups/ --target /var/lib/icn
   
   # Restore configuration
   cp /path/to/backups/config/* /etc/icn/
   ```

4. **Start Services**
   ```bash
   systemctl start icn-federation-node
   sleep 30  # Allow more time for major version rollback
   systemctl start icn-cooperative-node
   sleep 30
   systemctl start icn-community-node
   ```

5. **Verify Rollback**
   ```bash
   # Run comprehensive verification
   icn-cli system verify --federation-endpoint http://localhost:9000
   ```

## Migration from Previous ICN Versions

### ICN v1 to v3 Migration

ICN v1 used a significantly different architecture than v3. Migration requires a complete transformation of the data model.

#### Preparation

1. **Export v1 Data**
   ```bash
   # Export v1 data to JSON format
   icn-v1-cli export --output /path/to/exports/v1_data.json
   ```

2. **Verify Export Completeness**
   ```bash
   # Verify export data
   icn-v1-cli verify-export --input /path/to/exports/v1_data.json
   ```

#### Migration Steps

1. **Install ICN v3**
   - Follow the standard ICN v3 installation instructions
   - Set up the basic federation structure

2. **Transform and Import Data**
   ```bash
   # Transform v1 data to v3 format
   icn-cli migrate v1-to-v3 \
     --input /path/to/exports/v1_data.json \
     --output /path/to/imports/v3_data.json \
     --federation-id alpha
   
   # Import transformed data
   icn-cli import \
     --input /path/to/imports/v3_data.json \
     --federation-id alpha
   ```

3. **Verify Migration**
   ```bash
   # Verify imported data
   icn-cli system verify --migration-verification true
   ```

### ICN v2 to v3 Migration

ICN v2 had a more similar architecture to v3, making migration more straightforward but still requiring careful handling.

#### Preparation

1. **Backup v2 System**
   ```bash
   # Create a full backup of v2 data
   icn-v2-cli backup create --type full --output /path/to/backups/icn-v2/
   ```

2. **Export Critical Data**
   ```bash
   # Export v2 key data
   icn-v2-cli credential export --output /path/to/exports/v2_credentials.json
   
   # Export v2 token state
   icn-v2-cli token export --output /path/to/exports/v2_tokens.json
   
   # Export v2 governance data
   icn-v2-cli governance export --output /path/to/exports/v2_governance.json
   ```

#### Migration Steps

1. **Install ICN v3**
   - Follow the standard ICN v3 installation instructions
   - Set up the basic federation structure

2. **Migrate Credentials**
   ```bash
   # Import and convert v2 credentials to v3 DIDs
   icn-cli credential import \
     --input /path/to/exports/v2_credentials.json \
     --federation-id alpha
   ```

3. **Migrate Token State**
   ```bash
   # Import and convert v2 token state
   icn-cli token import \
     --input /path/to/exports/v2_tokens.json \
     --federation-id alpha
   ```

4. **Migrate Governance**
   ```bash
   # Transform v2 governance data to v3 CCL
   icn-cli governance migrate \
     --input /path/to/exports/v2_governance.json \
     --output-dir /var/lib/icn/federation/alpha/policies/
   
   # Deploy migrated governance policies
   for policy in /var/lib/icn/federation/alpha/policies/*.ccl; do
     icn-cli proposal create --ccl-file "$policy" --auto-deploy true
   done
   ```

5. **Verify Migration**
   ```bash
   # Verify system integrity post-migration
   icn-cli system verify --federation-endpoint http://localhost:9000
   ```

## Schema and Data Migrations

### DAG Store Migration

The DAG store is the foundation of ICN's data integrity. Migration must maintain cryptographic verifiability.

```bash
# Export DAG nodes from v2
icn-v2-cli dag export --output /path/to/exports/v2_dag.json

# Import DAG nodes to v3 with signature verification
icn-cli dag import \
  --input /path/to/exports/v2_dag.json \
  --path /var/lib/icn/federation/alpha/dag_store \
  --verify-signatures true
```

### Policy Migration

Policy migration involves converting from previous policy formats to CCL.

```bash
# Convert v2 policies to CCL
icn-cli policy convert \
  --input /path/to/exports/v2_policies.json \
  --output-dir /var/lib/icn/federation/alpha/policies/ \
  --format ccl

# Validate generated CCL files
for policy in /var/lib/icn/federation/alpha/policies/*.ccl; do
  icn-cli ccl validate --input "$policy"
done
```

### Token State Migration

Token state migration requires careful accounting and verification.

```bash
# Export token ledger from v2
icn-v2-cli token export-ledger --output /path/to/exports/v2_token_ledger.json

# Import to v3 with double-entry verification
icn-cli token import-ledger \
  --input /path/to/exports/v2_token_ledger.json \
  --coop-id econA \
  --verify-balances true
```

## Post-Migration Verification

After any upgrade or migration, perform these verification steps:

1. **System Health Check**
   ```bash
   # Check all node statuses
   icn-cli federation status --endpoint http://localhost:9000
   icn-cli cooperative status --endpoint http://localhost:9100
   icn-cli community status --endpoint http://localhost:9200
   ```

2. **Governance Verification**
   ```bash
   # Verify governance mechanisms
   icn-cli proposal create \
     --title "Test Proposal" \
     --template "test" \
     --output /tmp/test_proposal.json
   
   # Sign and submit the proposal
   icn-cli proposal vote \
     --proposal /tmp/test_proposal.json \
     --key-file /var/lib/icn/federation/alpha/credentials/admin1_key.json \
     --direction "yes" \
     --output /tmp/test_proposal.json
   
   # Submit to federation
   icn-cli federation submit-proposal \
     --endpoint http://localhost:9000 \
     --proposal /tmp/test_proposal.json
   ```

3. **Token Operations**
   ```bash
   # Test token minting
   icn-cli token mint \
     --coop-id econA \
     --federation-id alpha \
     --amount 10 \
     --recipient "did:key:z123" \
     --key-file /var/lib/icn/cooperative/econA/credentials/operator1_key.json \
     --note "Test mint" \
     --dry-run
   
   # Test token transfer
   icn-cli token transfer \
     --coop-id econA \
     --federation-id alpha \
     --amount 1 \
     --from "did:key:z123" \
     --to "did:key:z456" \
     --key-file /var/lib/icn/cooperative/econA/credentials/operator1_key.json \
     --note "Test transfer" \
     --dry-run
   ```

## Conclusion

Upgrading and migrating ICN deployments requires careful planning, execution, and verification. By following this guide, you can ensure a smooth transition to new versions while maintaining data integrity and system security. 