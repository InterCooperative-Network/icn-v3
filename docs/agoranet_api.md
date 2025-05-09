# AgoraNet API Documentation

## Overview

The AgoraNet API provides a RESTful interface for managing threaded discussions, governance proposals, and voting within the InterCooperative Network (ICN). It follows a typical lifecycle:

1.  **Threads**: Discussions are initiated within threads. Users can create new threads to discuss specific topics.
2.  **Proposals**: Based on discussions in threads (or independently), users can create proposals for formal governance decisions. Each proposal has a defined scope, text, and a voting period.
3.  **Votes**: Once a proposal is open, eligible participants can cast votes (Approve, Reject, Abstain). Vote counts are updated in real-time.
4.  **Lifecycle Management**: Proposals transition through statuses (e.g., Open, Closed, Accepted, Rejected) based on voting outcomes and deadlines.

This document provides details on available endpoints, request/response formats, and `curl` examples for interacting with the API. For a live, interactive API specification, please visit the [Swagger UI](#swagger-ui) when self-hosting the service.

## Endpoints

| Method | Path                               | Description                                       | Example Payload / Query Params                 |
|--------|------------------------------------|---------------------------------------------------|------------------------------------------------|
| GET    | `/health`                          | Health check for the service.                     | N/A                                            |
| GET    | `/threads`                         | List all discussion threads (summaries).          | `?scope=coop.nw&limit=10`                      |
| POST   | `/threads`                         | Create a new discussion thread.                   | `NewThreadRequest` JSON body                   |
| GET    | `/threads/{id}`                    | Get details for a specific thread.                | Path param: `id` (string)                      |
| GET    | `/proposals`                       | List all governance proposals (summaries).        | `?scope=coop.nw.gov&status=Open&limit=10`      |
| POST   | `/proposals`                       | Create a new governance proposal.                 | `NewProposalRequest` JSON body                 |
| GET    | `/proposals/{id}`                  | Get details for a specific proposal.              | Path param: `id` (string)                      |
| POST   | `/votes`                           | Cast a vote on a proposal.                        | `NewVoteRequest` JSON body                     |
| GET    | `/proposals/{proposal_id}/votes`   | Get all votes for a specific proposal.            | Path param: `proposal_id` (string)             |

## Curl Examples

Below are `curl` examples for interacting with the AgoraNet API. Assume the API is running at `http://localhost:8787`.

### Health Check

**Request:**
```bash
curl -X GET http://localhost:8787/health
```

**Response:** (Status 200 OK, empty body)

### Threads

#### List Threads

**Request:**
```bash
curl -X GET "http://localhost:8787/threads?scope=coop.nw&limit=5"
```

**Response:** (Status 200 OK)
```json
[
  {
    "id": "thread_abc123",
    "title": "Discussion about new governance model",
    "created_at": "2024-01-01T12:00:00Z",
    "author_did": "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE",
    "scope": "coop.nw"
  }
  // ... more threads
]
```

#### Create Thread

**Request:**
```bash
curl -X POST http://localhost:8787/threads \\
  -H "Content-Type: application/json" \\
  -d \'{
    "title": "New Initiative: Community Outreach Program",
    "author_did": "did:key:z6Mkwq4x2m2n2Pv3qY6zXrT5qL8rC4sB1vN9jK2wF7gH3xZc",
    "scope": "coop.nw.outreach",
    "metadata": {
      "tags": ["community", "outreach"]
    }
  }\'
```

**Response:** (Status 201 Created)
```json
{
  "id": "thread_new123",
  "title": "New Initiative: Community Outreach Program",
  "created_at": "2024-03-15T10:00:00Z",
  "author_did": "did:key:z6Mkwq4x2m2n2Pv3qY6zXrT5qL8rC4sB1vN9jK2wF7gH3xZc",
  "scope": "coop.nw.outreach"
}
```

#### Get Thread Detail

**Request:**
```bash
curl -X GET http://localhost:8787/threads/thread_abc123
```

**Response:** (Status 200 OK)
```json
{
  "summary": {
    "id": "thread_abc123",
    "title": "Discussion about new governance model",
    "created_at": "2024-01-01T12:00:00Z",
    "author_did": "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE",
    "scope": "coop.nw"
  },
  "messages": [
    {
      "id": "msg_1",
      "author_did": "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwupk8vQT7GNz2wVXgE",
      "timestamp": "2024-01-01T12:00:00Z",
      "content": "Initial message..."
    }
    // ... more messages
  ]
}
```
**Response (Not Found):** (Status 404 Not Found)
```json
{
  "error": "Thread with id thread_notfound404 not found"
}
```

### Proposals

#### List Proposals

**Request:**
```bash
curl -X GET "http://localhost:8787/proposals?scope=coop.nw.governance&status=Open"
```

**Response:** (Status 200 OK)
```json
[
  {
    "id": "proposal_def456",
    "title": "Implement new fee structure",
    "scope": "coop.nw.governance",
    "status": "Open",
    "vote_counts": { "approve": 15, "reject": 3, "abstain": 2 },
    "voting_deadline": "2024-01-15T18:00:00Z"
  }
  // ... more proposals
]
```

#### Create Proposal

**Request:**
```bash
curl -X POST http://localhost:8787/proposals \\
  -H "Content-Type: application/json" \\
  -d \'{
    "title": "Fund Project Nebula",
    "full_text": "This proposal is to allocate 1000 ICN tokens to Project Nebula...",
    "scope": "coop.nw.funding",
    "linked_thread_id": "thread_abc123",
    "voting_deadline": "2024-04-01T23:59:59Z"
  }\'
```

**Response:** (Status 201 Created)
```json
{
  "id": "proposal_new789",
  "title": "Fund Project Nebula",
  "scope": "coop.nw.funding",
  "status": "Open",
  "vote_counts": { "approve": 0, "reject": 0, "abstain": 0 },
  "voting_deadline": "2024-04-01T23:59:59Z"
}
```

#### Get Proposal Detail

**Request:**
```bash
curl -X GET http://localhost:8787/proposals/proposal_def456
```

**Response:** (Status 200 OK)
```json
{
  "summary": {
    "id": "proposal_def456",
    "title": "Implement new fee structure",
    "scope": "coop.nw.governance",
    "status": "Open",
    "vote_counts": { "approve": 15, "reject": 3, "abstain": 2 },
    "voting_deadline": "2024-01-15T18:00:00Z"
  },
  "full_text": "This proposal outlines a new fee structure for the network...",
  "linked_thread_id": "thread_abc123"
}
```
**Response (Not Found):** (Status 404 Not Found)
```json
{
  "error": "Proposal with id proposal_notfound404 not found"
}
```

### Votes

#### Cast Vote

**Request:**
```bash
curl -X POST http://localhost:8787/votes \\
  -H "Content-Type: application/json" \\
  -d \'{
    "proposal_id": "proposal_def456",
    "voter_did": "did:key:z6Mkvo7G9p7K1N2P3q4R5s6T7u8V9wAxByCzD0E1F2G3H4J5",
    "vote_type": "Approve",
    "justification": "This proposal aligns with our strategic goals."
  }\'
```

**Response:** (Status 201 Created)
```json
{
  "proposal_id": "proposal_def456",
  "voter_did": "did:key:z6Mkvo7G9p7K1N2P3q4R5s6T7u8V9wAxByCzD0E1F2G3H4J5",
  "vote_type": "Approve",
  "timestamp": "2024-03-15T11:00:00Z",
  "justification": "This proposal aligns with our strategic goals."
}
```
**Response (Bad Request - e.g. already voted, proposal not open):** (Status 400 Bad Request)
```json
{
  "error": "Voter did:key:z6Mk... has already voted on proposal proposal_def456"
}
```
**Response (Proposal Not Found):** (Status 404 Not Found)
```json
{
  "error": "Proposal with id proposal_notfound404 not found"
}
```

#### Get Votes for Proposal

**Request:**
```bash
curl -X GET http://localhost:8787/proposals/proposal_def456/votes
```

**Response:** (Status 200 OK)
```json
{
  "proposal_id": "proposal_def456",
  "votes": [
    {
      "proposal_id": "proposal_def456",
      "voter_did": "did:key:z6Mkvo7G9p7K1N2P3q4R5s6T7u8V9wAxByCzD0E1F2G3H4J5",
      "vote_type": "Approve",
      "timestamp": "2024-03-15T11:00:00Z",
      "justification": "This proposal aligns with our strategic goals."
    },
    {
      "proposal_id": "proposal_def456",
      "voter_did": "did:key:z6MksAnotherVoterDidExample",
      "vote_type": "Reject",
      "timestamp": "2024-03-15T11:05:00Z",
      "justification": "I have concerns about the budget."
    }
    // ... more votes
  ]
}
```
**Response (Proposal Not Found):** (Status 404 Not Found)
```json
{
  "error": "Proposal with id proposal_notfound404 not found"
}
```

## Swagger UI

For an interactive API specification with the ability to try out endpoints directly in your browser, please refer to the Swagger UI documentation. When running the AgoraNet service locally, it is typically available at:

[http://localhost:8787/docs](http://localhost:8787/docs)

This interface is automatically generated from the API source code and provides the most up-to-date details on request parameters, response schemas, and authentication methods (if any). 