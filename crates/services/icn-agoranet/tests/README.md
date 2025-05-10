# PostgreSQL Ledger Integration Tests

This directory contains integration tests for the PostgreSQL-backed persistent ledger in the ICN platform.

## Prerequisites

- Docker and Docker Compose installed and running
- PostgreSQL 15 (either local or Docker container)
- Rust toolchain

## Running the Tests

### 1. Start the PostgreSQL test database

```bash
# From the tests directory
docker-compose -f docker-compose.test.yml up postgres-test -d
```

### 2. Run the integration tests

```bash
# From the repository root
cargo test -p icn-agoranet --test ledger_integration_test
```

If you don't have a PostgreSQL server available, you can skip the tests that require a database:

```bash
SKIP_DB_TESTS=1 cargo test -p icn-agoranet --test ledger_integration_test
```

### 3. Testing manually with the API

You can also start a test server and interact with it manually using tools like cURL or Postman:

```bash
# Start the test server
docker-compose -f docker-compose.test.yml up -d
```

Then you can interact with the API at http://localhost:8788

## Test Coverage

The integration tests cover:

1. **API Endpoint Testing (End-to-End):**
   - `POST /api/v1/federation/{id}/transfers` (single transfers)
   - `POST /api/v1/federation/{id}/transfers/batch` (batch transfers)
   - `GET /api/v1/federation/{id}/transfers/query` (with various query parameters)
   - `GET /api/v1/federation/{id}/ledger/stats` (statistics endpoint)

2. **Authorization and Role-Based Access Control (RBAC):**
   - Federation admin access
   - Cooperative operator access
   - Community official access
   - Regular user access
   - Cross-scope access attempts

3. **Error Testing:**
   - Insufficient balance
   - Invalid transfer parameters
   - Entity not found scenarios
   - Authorization failures

4. **Data Integrity Verification:**
   - Balance consistency after transfers
   - Transaction recording
   - Statistics tracking

## Adding New Tests

When adding new tests, follow these principles:

1. Each test should be isolated and not depend on other tests
2. Clean up any resources created during the test
3. Make use of the helper functions for setting up test data
4. Include both success and failure cases
5. Test all authorization boundaries

## Troubleshooting

- If tests fail with connection errors, ensure the PostgreSQL container is running: `docker ps`
- Check the PostgreSQL logs: `docker-compose -f docker-compose.test.yml logs postgres-test`
- Try resetting the database: `docker-compose -f docker-compose.test.yml down -v && docker-compose -f docker-compose.test.yml up -d postgres-test` 