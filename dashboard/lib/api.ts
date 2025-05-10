import axios from "axios";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080";

const apiClient = axios.create({
  baseURL: API_BASE_URL,
  headers: {
    "Content-Type": "application/json",
  },
});

// Types for our API responses
export interface ExecutionReceipt {
  task_cid: string;
  executor: string;
  resource_usage: Record<string, number>;
  timestamp: string;
  signature: string;
}

export interface ReceiptNode {
  receipt_cid: string;
  receipt_cbor: string;
  anchor_timestamp: number;
  federation_id: string;
}

export interface DagNode {
  cid: string;
  content: string;
  event_type: string;
  scope_id: string;
  timestamp: number;
  parent_cids: string[];
}

export interface FederationNode {
  node_id: string;
  did: string;
  capabilities: {
    available_memory_mb: number;
    available_cpu_cores: number;
    available_storage_mb: number;
    location?: string;
    features: string[];
  };
  status: "online" | "offline";
  last_seen: string;
}

export interface Token {
  did: string;
  balance: number;
}

export interface GovernanceProposal {
  id: string;
  title: string;
  description: string;
  status: "active" | "executed" | "rejected" | "expired";
  votes_for: number;
  votes_against: number;
  created_at: string;
  expires_at: string;
}

// API services
export const ICNApi = {
  // Federation nodes
  async getFederationNodes(): Promise<FederationNode[]> {
    const { data } = await apiClient.get("/api/v1/federation/nodes");
    return data;
  },

  // Receipts
  async getLatestReceipts(limit: number = 10): Promise<ExecutionReceipt[]> {
    const { data } = await apiClient.get(`/api/v1/receipts/latest?limit=${limit}`);
    return data;
  },

  async getReceiptsByExecutor(executorDid: string): Promise<ExecutionReceipt[]> {
    const { data } = await apiClient.get(`/api/v1/receipts/by-executor/${executorDid}`);
    return data;
  },

  async getReceiptsByCID(cid: string): Promise<ExecutionReceipt | null> {
    try {
      const { data } = await apiClient.get(`/api/v1/receipts/${cid}`);
      return data;
    } catch (error) {
      return null;
    }
  },

  // Token ledger
  async getTokenBalances(): Promise<Token[]> {
    const { data } = await apiClient.get("/api/v1/tokens/balances");
    return data;
  },

  async getTokenStats(): Promise<{
    total_minted: number;
    total_burnt: number;
    active_accounts: number;
  }> {
    const { data } = await apiClient.get("/api/v1/tokens/stats");
    return data;
  },

  // Governance
  async getGovernanceProposals(): Promise<GovernanceProposal[]> {
    const { data } = await apiClient.get("/api/v1/governance/proposals");
    return data;
  },

  // DAG access
  async getDagNodes(eventType?: string, limit: number = 10): Promise<DagNode[]> {
    const url = eventType
      ? `/api/v1/dag/nodes?type=${eventType}&limit=${limit}`
      : `/api/v1/dag/nodes?limit=${limit}`;
    const { data } = await apiClient.get(url);
    return data;
  },
};

// For demo/development, create mock data when the API is not available
export const getMockData = {
  federationNodes(): FederationNode[] {
    return [
      {
        node_id: "node-1",
        did: "did:icn:abcdef123456",
        capabilities: {
          available_memory_mb: 8192,
          available_cpu_cores: 4,
          available_storage_mb: 100000,
          location: "us-west",
          features: ["avx", "sse4"],
        },
        status: "online",
        last_seen: new Date().toISOString(),
      },
      {
        node_id: "node-2",
        did: "did:icn:ghijkl789012",
        capabilities: {
          available_memory_mb: 16384,
          available_cpu_cores: 8,
          available_storage_mb: 500000,
          location: "eu-central",
          features: ["avx", "sse4", "gpu"],
        },
        status: "online",
        last_seen: new Date().toISOString(),
      },
      {
        node_id: "node-3",
        did: "did:icn:mnopqr345678",
        capabilities: {
          available_memory_mb: 4096,
          available_cpu_cores: 2,
          available_storage_mb: 50000,
          location: "asia-east",
          features: ["avx"],
        },
        status: "offline",
        last_seen: new Date(Date.now() - 86400000).toISOString(), // 1 day ago
      },
    ];
  },

  latestReceipts(): ExecutionReceipt[] {
    const receipts = [];
    for (let i = 0; i < 10; i++) {
      receipts.push({
        task_cid: `bafybeideputvakentvavfc${i}`,
        executor: `did:icn:node${i % 3 + 1}`,
        resource_usage: {
          CPU: Math.floor(Math.random() * 1000) + 100,
          Memory: Math.floor(Math.random() * 2048) + 256,
          Storage: Math.floor(Math.random() * 10000) + 1000,
        },
        timestamp: new Date(Date.now() - i * 3600000).toISOString(),
        signature: "0x1234567890abcdef",
      });
    }
    return receipts;
  },

  tokenBalances(): Token[] {
    return [
      { did: "did:icn:node1", balance: 15000 },
      { did: "did:icn:node2", balance: 25000 },
      { did: "did:icn:node3", balance: 5000 },
      { did: "did:icn:user1", balance: 3000 },
      { did: "did:icn:user2", balance: 7000 },
    ];
  },

  tokenStats() {
    return {
      total_minted: 60000,
      total_burnt: 5000,
      active_accounts: 5,
    };
  },

  governanceProposals(): GovernanceProposal[] {
    return [
      {
        id: "prop-1",
        title: "Increase computation resource limits",
        description: "Proposal to increase the maximum compute resources per task from 1000 to 2000",
        status: "active",
        votes_for: 3,
        votes_against: 1,
        created_at: new Date(Date.now() - 86400000).toISOString(),
        expires_at: new Date(Date.now() + 86400000).toISOString(),
      },
      {
        id: "prop-2",
        title: "Add new node to federation",
        description: "Add node with DID did:icn:newnode to the federation",
        status: "executed",
        votes_for: 4,
        votes_against: 0,
        created_at: new Date(Date.now() - 7 * 86400000).toISOString(),
        expires_at: new Date(Date.now() - 4 * 86400000).toISOString(),
      },
      {
        id: "prop-3",
        title: "Update token distribution algorithm",
        description: "Change the token distribution algorithm to be more fair for smaller nodes",
        status: "rejected",
        votes_for: 1,
        votes_against: 3,
        created_at: new Date(Date.now() - 14 * 86400000).toISOString(),
        expires_at: new Date(Date.now() - 7 * 86400000).toISOString(),
      },
    ];
  },

  dagNodes(): DagNode[] {
    return [
      {
        cid: "bafybeideputvakentvavfc1",
        content: JSON.stringify({ type: "receipt", data: { task_id: "task-1" } }),
        event_type: "Receipt",
        scope_id: "receipt/bafybeideputvakentvavfc1",
        timestamp: Date.now() - 3600000,
        parent_cids: [],
      },
      {
        cid: "bafybeideputvakentvavfc2",
        content: JSON.stringify({ type: "receipt", data: { task_id: "task-2" } }),
        event_type: "Receipt",
        scope_id: "receipt/bafybeideputvakentvavfc2",
        timestamp: Date.now() - 7200000,
        parent_cids: [],
      },
      {
        cid: "bafybeideputvakentvavfc3",
        content: JSON.stringify({ type: "governance", data: { proposal_id: "prop-1" } }),
        event_type: "Governance",
        scope_id: "governance/prop-1",
        timestamp: Date.now() - 86400000,
        parent_cids: [],
      },
    ];
  },
}; 