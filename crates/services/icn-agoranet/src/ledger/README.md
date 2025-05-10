# ICN Ledger System

The ICN Ledger System provides a persistent, reliable tracking system for economic transfers between entities within federations, cooperatives, communities, and users in the Internet of Cooperation Network.

## Architecture

The ledger system follows a modular, trait-based design:

1. The `LedgerStore` trait defines a common interface for all ledger implementations
2. Current implementations:
   - `PostgresLedgerStore`: Production-ready PostgreSQL-based implementation
   - In-memory implementation (for testing and development)

This design allows for:
1. Horizontal scaling of the ledger across database instances
2. Future migration to a DAG-based system without changing the API
3. Easy testing with in-memory implementations

## Database Schema

The PostgreSQL implementation uses the following schema:

- **entities**: Tracks all participants in the system
  - `entity_type` (Federation, Cooperative, Community, User)
  - `entity_id` (DID or organization ID)
  - `federation_id` (governing federation)
  - `metadata` (additional entity details)

- **balances**: Current balance state
  - `entity_type`
  - `entity_id`
  - `balance`
  - `last_updated`

- **transfers**: Transaction history with DAG-friendly structure
  - `tx_id` (UUID)
  - `federation_id`
  - `from_type` and `from_id`
  - `to_type` and `to_id`
  - `amount`
  - `fee`
  - `initiator` (DID of user initiating the transfer)
  - `timestamp`
  - `memo`
  - DAG-related fields:
    - `parent_tx_ids` (references to parent transactions)
    - `status`
    - `consensus_data`
    - `merkle_proof`
    - `signature`

- **federation_stats**: Aggregated statistics for federations
  - `federation_id`
  - `total_transfers`
  - `total_volume`
  - `total_fees`
  - `last_updated`

## Main Operations

The ledger system supports:

1. **Balance Management**
   - Get current balances for entities
   - Update balances atomically during transfers

2. **Transfer Processing**
   - Process single transfers with validation
   - Process batch transfers
   - Handle fees

3. **Query Capabilities**
   - Find specific transfers by ID
   - Query transfers by:
     - Federation
     - Entity (from/to)
     - Amount range
     - Time range
     - With pagination support

4. **Statistics**
   - Get system-wide statistics
   - Get federation-specific statistics

## Future Enhancements

The system is designed to support:

1. **Decentralization**: The DAG-friendly schema will allow future migration to a distributed ledger
2. **Cryptographic Verification**: Fields for signatures and merkle proofs are already in place
3. **Consensus**: The consensus_data field can store consensus-related information

## Testing

Integration tests are available to verify the PostgreSQL implementation:

```bash
# Set up a test database
createdb icn_ledger_test

# Run PostgreSQL integration tests
TEST_PG=true TEST_DATABASE_URL="postgres://postgres:postgres@localhost:5432/icn_ledger_test" cargo test pg_ledger_tests
```

The tests automatically create required tables via migrations and validate:
- Entity creation and balance management
- Single transfers and validation
- Error handling for insufficient balances
- Batch transfers with partial success
- Query capabilities
- Statistics collection 