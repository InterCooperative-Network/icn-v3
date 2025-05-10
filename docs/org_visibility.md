# Organization-Scoped Visibility and Authorization in ICN v3

This document describes the organization-scoped visibility and authorization system in the ICN v3 platform.

## Organization Hierarchy

The ICN platform uses a hierarchical organization structure:

1. **Federation** - The top-level organization type that encompasses multiple cooperatives
2. **Cooperative** - A mid-level organization that belongs to one or more federations and contains multiple communities
3. **Community** - The smallest organization unit within a cooperative

This hierarchy is used to control access to resources and data within the platform.

## JWT-Based Authorization

The platform uses JWT (JSON Web Tokens) for authorization with organization-scoped claims:

```json
{
  "sub": "did:icn:user123",
  "iss": "federation:fed1",
  "exp": 1692086400,
  "federation_ids": ["fed1", "fed2"],
  "coop_ids": ["coop1", "coop2"],
  "community_ids": ["comm1", "comm2"],
  "roles": {
    "fed1": ["federation_admin"],
    "coop1": ["admin", "member"],
    "comm1": ["member"]
  }
}
```

### Claims Explained

- `sub`: The user's DID (Decentralized Identifier)
- `iss`: The issuer of the token (typically a federation)
- `exp`: Token expiration timestamp
- `federation_ids`: List of federation IDs the user has access to
- `coop_ids`: List of cooperative IDs the user has access to
- `community_ids`: List of community IDs the user has access to
- `roles`: Map of organization IDs to role lists

## Federation Token Management

Federations have the authority to manage JWT tokens for users, granting them specific organization scopes and roles.

### Token Issuance Endpoint

```
POST /api/v1/federation/{federation_id}/tokens
```

**Headers:**
- `Authorization: Bearer {federation_admin_token}`

**Request Body:**
```json
{
  "subject": "did:icn:user123",
  "expires_in": 86400,
  "federation_ids": ["fed1"],
  "coop_ids": ["coop1", "coop2"],
  "community_ids": ["comm1"],
  "roles": {
    "coop1": ["member"],
    "comm1": ["member"]
  }
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_at": 1692086400,
  "token_id": "jti-7b22686f7374..."
}
```

### Token Revocation Endpoint

```
POST /api/v1/federation/{federation_id}/tokens/revoke
```

**Headers:**
- `Authorization: Bearer {federation_admin_token}`

**Request Body:**
```json
{
  "jti": "jti-7b22686f7374...",  // Either specify jti
  "subject": "did:icn:user123",  // Or specify subject to revoke all their tokens
  "reason": "Unauthorized access detected"
}
```

**Response:**
```json
{
  "revoked": true,
  "revoked_at": 1692086400,
  "jti": "jti-7b22686f7374...",
  "subject": "did:icn:user123"
}
```

### Token Rotation Endpoint

Token rotation allows for refreshing a token with new scopes or expiration:

```
POST /api/v1/federation/{federation_id}/tokens/rotate
```

**Headers:**
- `Authorization: Bearer {federation_admin_token}`

**Request Body:**
```json
{
  "current_jti": "jti-7b22686f7374...",
  "subject": "did:icn:user123",
  "expires_in": 86400,
  "federation_ids": ["fed1"],
  "coop_ids": ["coop1"],
  "community_ids": ["comm1"],
  "roles": {
    "coop1": ["member"]
  },
  "reason": "Scope change"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_at": 1692086400,
  "token_id": "jti-9c33797d7f..."
}
```

### Federation Admin Role

To manage tokens, the requesting user must have:
1. Access to the federation (their token must include the federation_id in the federation_ids claim)
2. The "federation_admin" role for that federation

### Token Management Rules

1. A federation admin can only issue/revoke tokens that include their federation's ID
2. Admins cannot grant access to federations they don't control
3. Tokens must have an expiration date (default is 24 hours)
4. Revoked tokens are added to a revocation list that is checked on each request
5. The system periodically cleans up the revocation list (tokens older than 30 days)

## API Authorization Flow

1. Client includes the JWT token in the `Authorization` header of each request: `Authorization: Bearer {token}`
2. Server validates the token signature and expiration
3. Server extracts organization scopes from the token claims
4. API handlers check if the user has access to the requested resources based on:
   - Organization membership (federation_ids, coop_ids, community_ids)
   - Roles for specific operations
5. Access is granted or denied based on the token's claims

## WebSocket Authentication

WebSocket connections are also authenticated using JWT tokens:

1. Client connects to WebSocket endpoint with token query parameter: `?token={jwt_token}`
2. Server validates the token on connection
3. Client can only subscribe to channels within their authorized organization scopes

## Organization-Scoped Resources

The following resources are organization-scoped:

1. **Execution Receipts** - Scoped to cooperatives and communities
2. **Token Balances** - Scoped to federations, cooperatives, and communities
3. **Token Transactions** - Scoped to federations, cooperatives, and communities

Each resource can be queried with organization scope parameters, and users can only access resources within their authorized scopes.

## Examples

### Accessing Cooperative Resources

```
GET /api/v1/receipts?coop_id=coop1
Authorization: Bearer {token}
```

Only users with coop1 in their `coop_ids` claim can access these resources.

### Accessing Federation-Level Statistics

```
GET /api/v1/stats/tokens?federation_id=fed1
Authorization: Bearer {token}
```

Only users with fed1 in their `federation_ids` claim can access these statistics.

## Implementation Notes

The authorization system is implemented with careful consideration of performance and security:

1. JWT validation is performed for every API request
2. Organization scope checks are efficient and optimized for frequent access patterns
3. Role-based permissions are checked only for administrative operations
4. Token issuance is limited to federation administrators to maintain security 