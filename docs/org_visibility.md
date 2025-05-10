## Organization-Scoped Visibility and Authorization in ICN v3

This document describes the organization-scoped visibility and authorization system in the ICN v3 platform.

## Organization Hierarchy

ICN uses three complementary organization types, each with distinct responsibilities:

1. **Cooperative**
   – **Economic engines** of the network.
   – Manage production, trade, token issuance, and economic operations for their members.
   – Can belong to one or more federations to enable cross-cooperative marketplaces.

2. **Community**
   – **Governance and public-service bodies.**
   – Responsible for policy-making, dispute resolution, public goods (education, healthcare, infrastructure).
   – Operates within one cooperative, but may collaborate across cooperatives via federations.

3. **Federation**
   – **Coordination layer** between cooperatives and communities.
   – Facilitates interoperability (economic, governance), shared infrastructure, cross-domain dispute resolution, and network-wide standards.
   – Issues global credentials, mediates cross-coop transactions, and enforces multi-party governance.

## JWT-Based Authorization

All access in ICN is gated by JWTs carrying explicit organization scopes and roles:

```jsonc
{
  "sub": "did:icn:user123",                // User's DID
  "iss": "did:icn:federation:alpha",       // Issuer federation DID
  "exp": 1715370000,                       // Expiration timestamp
  "federation_ids": ["alpha"],             // Federations the token covers
  "coop_ids":       ["coop-econA"],        // Cooperatives (economic engines)
  "community_ids":  ["comm-govX"],         // Communities (governance bodies)
  "roles": {
    "alpha":        ["federation_admin"],  // Federation-level coordinators
    "coop-econA":   ["coop_operator"],     // Cooperative economic operators
    "comm-govX":    ["community_official"] // Community governance officials
  }
}
```

### Claims Explained

* **`sub`**: User's Decentralized Identifier (DID).
* **`iss`**: Token issuer (a federation DID).
* **`exp`**: UNIX expiration time.
* **`federation_ids`**: Federations the user may coordinate within.
* **`coop_ids`**: Cooperatives whose economic operations they may drive.
* **`community_ids`**: Communities whose governance they may influence.
* **`roles`**: Map of org DID → role(s), e.g.:

  * `federation_admin`: federation coordinators with authority to issue cross-coop policies
  * `coop_operator`: manages token minting, transfers, and economic parameters within a coop
  * `community_official`: governs public-service decisions and policy in a community

## Federation Token Issuance

Only federation coordinators (`federation_admin`) may mint and issue JWTs to grant scoped access:

```
POST /api/v1/federation/{federation_id}/tokens
```

**Headers:**

```
Authorization: Bearer {federation_admin_token}
```

**Body:**

```json
{
  "subject":       "did:icn:user123",
  "expires_in":    86400,
  "federation_ids":["alpha"],
  "coop_ids":      ["coop-econA", "coop-econB"],
  "community_ids": ["comm-govX"],
  "roles": {
    "coop-econA": ["coop_operator"],
    "comm-govX":  ["community_official"]
  }
}
```

**Rules:**

1. Must include the requesting federation's DID in `federation_ids`.
2. Can only grant roles within that federation's scope.
3. Default expiration is 24 hours.

## API Authorization Flow

1. **Client** sends `Authorization: Bearer {jwt}` on each request.
2. **Server** validates signature and `exp`.
3. **Server** extracts `federation_ids`, `coop_ids`, `community_ids`, and `roles`.
4. **Handlers** enforce that:

   * Economic endpoints (e.g. token mint, transfer) require a matching `coop_id` + `coop_operator`.
   * Governance endpoints (e.g. policy votes, public-service actions) require `community_id` + `community_official`.
   * Coordination endpoints (e.g. cross-coop operations) require `federation_id` + `federation_admin`.
5. **Access** is granted only if the token's claims cover the requested organizational scope and roles; otherwise HTTP 403.

## WebSocket Authentication

Real-time subscriptions also require JWTs:

```
ws://.../ws/org?token={jwt}
```

* On connect, the **token** is validated.
* Subscription channels are then limited to orgs in `federation_ids`, `coop_ids`, or `community_ids`, according to the user's roles.

## Organization-Scoped Resources

| Resource           | Scope Level                        |
| ------------------ | ---------------------------------- |
| Execution Receipts | Cooperative, Community             |
| Token Balances     | Federation, Cooperative, Community |
| Token Transactions | Federation, Cooperative, Community |

Clients must supply `federation_id=…`, `coop_id=…`, or `community_id=…` query params matching their token scopes.

### Examples

**Economic data in a coop:**

```http
GET /api/v1/balances?coop_id=coop-econA
Authorization: Bearer {jwt}
```

**Governance data in a community:**

```http
GET /api/v1/receipts?community_id=comm-govX
Authorization: Bearer {jwt}
```

**Federation coordination stats:**

```http
GET /api/v1/stats/coops?federation_id=alpha
Authorization: Bearer {jwt}
``` 