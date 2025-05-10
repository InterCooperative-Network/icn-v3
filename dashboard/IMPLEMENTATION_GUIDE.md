# ICN Dashboard API Implementation Guide

This guide provides practical steps for backend developers to implement the REST API required by the ICN Dashboard.

## Overview

The ICN Dashboard front-end is designed to work with a consistent REST API that provides access to receipt and token data. The API follows a standard structure where:

- Base URL: `/api/v1`
- Response format: JSON
- Authentication: Currently using simple HTTP (to be enhanced)
- Error handling: Standard HTTP status codes with JSON error messages

## Priority Endpoints

Implement these endpoints in the following order:

### 1. Receipt Endpoints

#### 1.1 List Receipts (`GET /receipts`)

```go
// Example Go implementation
func handleGetReceipts(w http.ResponseWriter, r *http.Request) {
    // Parse query parameters
    date := r.URL.Query().Get("date")
    executor := r.URL.Query().Get("executor")
    limit := parseIntParam(r.URL.Query().Get("limit"), 100)
    offset := parseIntParam(r.URL.Query().Get("offset"), 0)
    
    // Query database using the filters
    receipts, err := db.QueryReceipts(date, executor, limit, offset)
    if err != nil {
        http.Error(w, `{"error":"Failed to query receipts"}`, http.StatusInternalServerError)
        return
    }
    
    // Return JSON response
    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(receipts)
}
```

### 2. Token Endpoints

#### 2.1 Token Balances (`GET /tokens/balances`)

```go
// Example Go implementation
func handleGetTokenBalances(w http.ResponseWriter, r *http.Request) {
    // Parse query parameters
    did := r.URL.Query().Get("did")
    
    // Query database for balances
    var balances []TokenBalance
    var err error
    
    if did != "" {
        // Get balance for specific account
        balance, err := db.GetAccountBalance(did)
        if err == nil {
            balances = []TokenBalance{balance}
        }
    } else {
        // Get all balances
        balances, err = db.GetAllBalances()
    }
    
    if err != nil {
        http.Error(w, `{"error":"Failed to query token balances"}`, http.StatusInternalServerError)
        return
    }
    
    // Return JSON response
    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(balances)
}
```

#### 2.2 Token Transactions (`GET /tokens/transactions`)

```go
func handleGetTokenTransactions(w http.ResponseWriter, r *http.Request) {
    // Parse query parameters
    did := r.URL.Query().Get("did")
    date := r.URL.Query().Get("date")
    limit := parseIntParam(r.URL.Query().Get("limit"), 50)
    offset := parseIntParam(r.URL.Query().Get("offset"), 0)
    
    // Query database for transactions
    transactions, err := db.QueryTransactions(did, date, limit, offset)
    if err != nil {
        http.Error(w, `{"error":"Failed to query transactions"}`, http.StatusInternalServerError)
        return
    }
    
    // Return JSON response
    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(transactions)
}
```

## API Response Formats

The frontend expects these response formats:

### Receipts

```json
[
  {
    "task_cid": "bafy...",
    "executor": "did:icn:node123",
    "resource_usage": {"CPU": 250, "Memory": 1024},
    "timestamp": "2025-05-10T14:23:00Z",
    "signature": "...base64..."
  }
]
```

### Token Balances

```json
[
  {
    "did": "did:icn:alice",
    "balance": 60
  }
]
```

### Token Transactions

```json
[
  {
    "from": "did:icn:alice",
    "to": "did:icn:bob",
    "amount": 40,
    "timestamp": "2025-05-10T15:00:00Z"
  }
]
```

## Database Integration

The dashboard's API endpoints should connect to your existing storage layer. Focus on these integration points:

1. **For receipts**: Query your receipt storage system (likely DAG-indexed) to filter by date and executor
2. **For tokens**: Query your token ledger system to retrieve balances and transaction history

## CORS Configuration

Configure CORS headers to allow the dashboard to make requests:

```go
// Example CORS middleware
func corsMiddleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        w.Header().Set("Access-Control-Allow-Origin", "https://dashboard.icn.dev")
        w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
        
        if r.Method == "OPTIONS" {
            w.WriteHeader(http.StatusOK)
            return
        }
        
        next.ServeHTTP(w, r)
    })
}
```

## Testing

Test the API endpoints with the dashboard by:

1. Starting your API server locally on port 8080
2. Setting the dashboard's `NEXT_PUBLIC_API_URL` environment variable to `http://localhost:8080`
3. Running the dashboard and testing the interactive features

## Next Steps

After implementing the core endpoints above, consider adding:

1. **Real-time updates**: Implement a WebSocket endpoint for pushing new receipts and transactions
2. **Detailed statistics**: Add endpoints for aggregated metrics
3. **Authentication**: Add proper JWT-based authentication

For questions or assistance, contact the dashboard development team. 