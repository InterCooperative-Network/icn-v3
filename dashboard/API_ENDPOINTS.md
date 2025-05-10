# ICN Dashboard API Endpoints

This document describes the API endpoints required by the ICN Dashboard. These endpoints should be implemented in the backend to support the dashboard's functionality.

## Base URL

All API endpoints are relative to the base URL specified in the environment variable `NEXT_PUBLIC_API_URL`, which defaults to `http://localhost:8080`.

## Authentication

Authentication requirements should be implemented according to the runtime API specifications. The dashboard currently does not handle authentication but can be modified to include auth headers as needed.

## Endpoints

### Federation Nodes

#### Get All Federation Nodes

```
GET /api/v1/federation/nodes
```

Returns a list of all federation nodes.

**Response**:
```json
[
  {
    "node_id": "node-1",
    "did": "did:icn:abcdef123456",
    "capabilities": {
      "available_memory_mb": 8192,
      "available_cpu_cores": 4,
      "available_storage_mb": 100000,
      "location": "us-west",
      "features": ["avx", "sse4"]
    },
    "status": "online",
    "last_seen": "2023-05-10T15:30:00Z"
  },
  ...
]
```

### Execution Receipts

#### Get Filtered Receipts

```
GET /api/v1/receipts
```

Query parameters:
- `date` (optional): Filter by ISO date (YYYY-MM-DD)
- `executor` (optional): Filter by executor DID
- `limit` (optional): Maximum number of receipts to return
- `offset` (optional): Pagination offset

Returns a list of execution receipts matching the filter criteria.

**Response**:
```json
[
  {
    "task_cid": "bafybeideputvakentvavfc1",
    "executor": "did:icn:node1",
    "resource_usage": {
      "CPU": 500,
      "Memory": 1024,
      "Storage": 5000
    },
    "timestamp": "2023-05-10T15:30:00Z",
    "signature": "0x1234567890abcdef"
  },
  ...
]
```

#### Get Latest Receipts

```
GET /api/v1/receipts/latest
```

Query parameters:
- `limit` (optional): Maximum number of receipts to return

Returns the most recent execution receipts.

#### Get Receipts by Executor

```
GET /api/v1/receipts/by-executor/{executorDid}
```

Path parameters:
- `executorDid`: DID of the executor node

Returns all receipts by a specific executor.

#### Get Receipts by Date

```
GET /api/v1/receipts/by-date/{date}
```

Path parameters:
- `date`: ISO date (YYYY-MM-DD)

Returns all receipts from a specific date.

#### Get Receipt by CID

```
GET /api/v1/receipts/{cid}
```

Path parameters:
- `cid`: Content ID of the receipt

Returns a specific receipt by its CID.

#### Get Receipt Statistics

```
GET /api/v1/receipts/stats
```

Query parameters:
- `date` (optional): Filter by ISO date (YYYY-MM-DD)
- `executor` (optional): Filter by executor DID

Returns statistics about execution receipts.

**Response**:
```json
{
  "total_receipts": 150,
  "avg_cpu_usage": 450,
  "avg_memory_usage": 1024,
  "avg_storage_usage": 5000,
  "receipts_by_executor": {
    "did:icn:node1": 50,
    "did:icn:node2": 75,
    "did:icn:node3": 25
  }
}
```

### Tokens

#### Get Token Balances

```
GET /api/v1/tokens/balances
```

Query parameters:
- `account` (optional): Filter by account DID
- `limit` (optional): Maximum number of accounts to return
- `offset` (optional): Pagination offset

Returns token balances for all accounts.

**Response**:
```json
[
  {
    "did": "did:icn:node1",
    "balance": 15000
  },
  ...
]
```

#### Get Token Transactions

```
GET /api/v1/tokens/transactions
```

Query parameters:
- `date` (optional): Filter by ISO date (YYYY-MM-DD)
- `account` (optional): Filter by account DID
- `limit` (optional): Maximum number of transactions to return
- `offset` (optional): Pagination offset

Returns token transactions matching the filter criteria.

**Response**:
```json
[
  {
    "id": "tx-1",
    "from_did": "did:icn:treasury",
    "to_did": "did:icn:node1",
    "amount": 500,
    "operation": "mint",
    "timestamp": "2023-05-10T15:30:00Z"
  },
  ...
]
```

#### Get Token Statistics

```
GET /api/v1/tokens/stats
```

Query parameters:
- `date` (optional): Filter by ISO date (YYYY-MM-DD)

Returns statistics about the token economy.

**Response**:
```json
{
  "total_minted": 60000,
  "total_burnt": 5000,
  "active_accounts": 5,
  "daily_volume": 2500
}
```

### Governance

#### Get Governance Proposals

```
GET /api/v1/governance/proposals
```

Returns a list of all governance proposals.

**Response**:
```json
[
  {
    "id": "prop-1",
    "title": "Increase computation resource limits",
    "description": "Proposal to increase the maximum compute resources per task from 1000 to 2000",
    "status": "active",
    "votes_for": 3,
    "votes_against": 1,
    "created_at": "2023-05-09T15:30:00Z",
    "expires_at": "2023-05-11T15:30:00Z"
  },
  ...
]
```

### DAG

#### Get DAG Nodes

```
GET /api/v1/dag/nodes
```

Query parameters:
- `type` (optional): Filter by event type
- `limit` (optional): Maximum number of nodes to return

Returns a list of DAG nodes.

**Response**:
```json
[
  {
    "cid": "bafybeideputvakentvavfc1",
    "content": "{\"type\":\"receipt\",\"data\":{\"task_id\":\"task-1\"}}",
    "event_type": "Receipt",
    "scope_id": "receipt/bafybeideputvakentvavfc1",
    "timestamp": 1683729000000,
    "parent_cids": []
  },
  ...
]
```

## Implementing Mock Endpoints

For development purposes, the dashboard includes mock implementations of these endpoints. When implementing the actual API, ensure that the response format matches the expected format as defined in this document.

The mock implementations can be found in the `lib/api.ts` file in the `getMockData` object.

## Error Handling

API endpoints should return appropriate HTTP status codes:

- `200 OK`: Request succeeded
- `400 Bad Request`: Invalid parameters
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Server error

Error responses should include a message explaining the error:

```json
{
  "error": "Invalid parameter: date format should be YYYY-MM-DD"
}
``` 