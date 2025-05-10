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

export interface TokenTransaction {
  id: string;
  from_did: string;
  to_did: string;
  amount: number;
  operation: "mint" | "burn" | "transfer";
  timestamp: string;
}

export interface TokenStats {
  total_minted: number;
  total_burnt: number;
  active_accounts: number;
  daily_volume?: number;
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

// Filter types
export interface ReceiptFilter {
  date?: string;
  executor?: string;
  limit?: number;
  offset?: number;
}

export interface TokenFilter {
  date?: string;
  account?: string;
  limit?: number;
  offset?: number;
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

  async getFilteredReceipts(filter: ReceiptFilter): Promise<ExecutionReceipt[]> {
    const params = new URLSearchParams();
    
    if (filter.date) params.append('date', filter.date);
    if (filter.executor) params.append('executor', filter.executor);
    if (filter.limit) params.append('limit', filter.limit.toString());
    if (filter.offset) params.append('offset', filter.offset.toString());
    
    const { data } = await apiClient.get(`/api/v1/receipts?${params.toString()}`);
    return data;
  },

  async getReceiptsByExecutor(executorDid: string): Promise<ExecutionReceipt[]> {
    const { data } = await apiClient.get(`/api/v1/receipts/by-executor/${executorDid}`);
    return data;
  },

  async getReceiptsByDate(date: string): Promise<ExecutionReceipt[]> {
    const { data } = await apiClient.get(`/api/v1/receipts/by-date/${date}`);
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

  // Receipt statistics
  async getReceiptStats(filter?: ReceiptFilter): Promise<{
    total_receipts: number;
    avg_cpu_usage: number;
    avg_memory_usage: number;
    avg_storage_usage: number;
    receipts_by_executor: Record<string, number>;
  }> {
    const params = new URLSearchParams();
    
    if (filter?.date) params.append('date', filter.date);
    if (filter?.executor) params.append('executor', filter.executor);
    
    const { data } = await apiClient.get(`/api/v1/receipts/stats?${params.toString()}`);
    return data;
  },

  // Token ledger
  async getTokenBalances(filter?: TokenFilter): Promise<Token[]> {
    const params = new URLSearchParams();
    
    if (filter?.account) params.append('account', filter.account);
    if (filter?.limit) params.append('limit', filter.limit.toString());
    if (filter?.offset) params.append('offset', filter.offset.toString());
    
    const { data } = await apiClient.get(`/api/v1/tokens/balances?${params.toString()}`);
    return data;
  },

  async getTokenTransactions(filter?: TokenFilter): Promise<TokenTransaction[]> {
    const params = new URLSearchParams();
    
    if (filter?.date) params.append('date', filter.date);
    if (filter?.account) params.append('account', filter.account);
    if (filter?.limit) params.append('limit', filter.limit.toString());
    if (filter?.offset) params.append('offset', filter.offset.toString());
    
    const { data } = await apiClient.get(`/api/v1/tokens/transactions?${params.toString()}`);
    return data;
  },

  async getTokenStats(filter?: TokenFilter): Promise<TokenStats> {
    const params = new URLSearchParams();
    
    if (filter?.date) params.append('date', filter.date);
    
    const { data } = await apiClient.get(`/api/v1/tokens/stats?${params.toString()}`);
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

  filteredReceipts(filter: ReceiptFilter): ExecutionReceipt[] {
    const allReceipts = this.latestReceipts();
    
    // Create 50 receipts spanning the last 30 days for more varied data
    for (let i = 10; i < 50; i++) {
      const daysAgo = Math.floor(Math.random() * 30);
      allReceipts.push({
        task_cid: `bafybeideputvakentvavfc${i}`,
        executor: `did:icn:node${i % 3 + 1}`,
        resource_usage: {
          CPU: Math.floor(Math.random() * 1000) + 100,
          Memory: Math.floor(Math.random() * 2048) + 256,
          Storage: Math.floor(Math.random() * 10000) + 1000,
        },
        timestamp: new Date(Date.now() - daysAgo * 86400000).toISOString(),
        signature: "0x1234567890abcdef",
      });
    }
    
    let filtered = [...allReceipts];
    
    // Apply date filter
    if (filter.date) {
      filtered = filtered.filter(receipt => {
        const receiptDate = new Date(receipt.timestamp).toISOString().split('T')[0];
        return receiptDate === filter.date;
      });
    }
    
    // Apply executor filter
    if (filter.executor) {
      filtered = filtered.filter(receipt => 
        receipt.executor === filter.executor
      );
    }
    
    // Apply limit
    if (filter.limit) {
      filtered = filtered.slice(0, filter.limit);
    }
    
    return filtered;
  },
  
  receiptStats(filter?: ReceiptFilter): {
    total_receipts: number;
    avg_cpu_usage: number;
    avg_memory_usage: number;
    avg_storage_usage: number;
    receipts_by_executor: Record<string, number>;
  } {
    // Get receipts based on filter
    const receipts = filter ? this.filteredReceipts(filter) : this.latestReceipts();
    
    // Calculate stats
    let totalCpu = 0;
    let totalMemory = 0;
    let totalStorage = 0;
    const executorCounts: Record<string, number> = {};
    
    receipts.forEach(receipt => {
      totalCpu += receipt.resource_usage.CPU || 0;
      totalMemory += receipt.resource_usage.Memory || 0;
      totalStorage += receipt.resource_usage.Storage || 0;
      
      if (!executorCounts[receipt.executor]) {
        executorCounts[receipt.executor] = 0;
      }
      executorCounts[receipt.executor] += 1;
    });
    
    return {
      total_receipts: receipts.length,
      avg_cpu_usage: receipts.length ? Math.round(totalCpu / receipts.length) : 0,
      avg_memory_usage: receipts.length ? Math.round(totalMemory / receipts.length) : 0,
      avg_storage_usage: receipts.length ? Math.round(totalStorage / receipts.length) : 0,
      receipts_by_executor: executorCounts
    };
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

  tokenTransactions(filter?: TokenFilter): TokenTransaction[] {
    const transactions = [];
    const accounts = ["did:icn:node1", "did:icn:node2", "did:icn:node3", "did:icn:user1", "did:icn:user2"];
    const operations = ["mint", "burn", "transfer"] as const;
    
    // Generate 30 days of transactions
    for (let i = 0; i < 100; i++) {
      const daysAgo = Math.floor(Math.random() * 30);
      const operation = operations[Math.floor(Math.random() * operations.length)];
      const fromAccount = accounts[Math.floor(Math.random() * accounts.length)];
      let toAccount = accounts[Math.floor(Math.random() * accounts.length)];
      
      // Ensure from and to are different for transfers
      while (operation === "transfer" && fromAccount === toAccount) {
        toAccount = accounts[Math.floor(Math.random() * accounts.length)];
      }
      
      transactions.push({
        id: `tx-${i}`,
        from_did: operation === "burn" ? fromAccount : operation === "mint" ? "did:icn:treasury" : fromAccount,
        to_did: operation === "mint" ? toAccount : operation === "burn" ? "did:icn:treasury" : toAccount,
        amount: Math.floor(Math.random() * 1000) + 100,
        operation,
        timestamp: new Date(Date.now() - daysAgo * 86400000).toISOString()
      });
    }
    
    let filtered = [...transactions];
    
    // Apply date filter
    if (filter?.date) {
      filtered = filtered.filter(tx => {
        const txDate = new Date(tx.timestamp).toISOString().split('T')[0];
        return txDate === filter.date;
      });
    }
    
    // Apply account filter
    if (filter?.account) {
      filtered = filtered.filter(tx => 
        tx.from_did === filter.account || tx.to_did === filter.account
      );
    }
    
    // Apply limit
    if (filter?.limit) {
      filtered = filtered.slice(0, filter.limit);
    }
    
    return filtered;
  },

  tokenStats(filter?: TokenFilter): TokenStats {
    // Base stats
    const baseStats = {
      total_minted: 60000,
      total_burnt: 5000,
      active_accounts: 5,
    };
    
    // If no filter, return base stats
    if (!filter) {
      return baseStats;
    }
    
    // For date filter, calculate daily stats
    if (filter.date) {
      const transactions = this.tokenTransactions({ date: filter.date });
      
      let dailyMinted = 0;
      let dailyBurnt = 0;
      
      transactions.forEach(tx => {
        if (tx.operation === "mint") dailyMinted += tx.amount;
        if (tx.operation === "burn") dailyBurnt += tx.amount;
      });
      
      const activeAccounts = new Set();
      transactions.forEach(tx => {
        if (tx.from_did !== "did:icn:treasury") activeAccounts.add(tx.from_did);
        if (tx.to_did !== "did:icn:treasury") activeAccounts.add(tx.to_did);
      });
      
      return {
        ...baseStats,
        total_minted: baseStats.total_minted + dailyMinted,
        total_burnt: baseStats.total_burnt + dailyBurnt,
        active_accounts: activeAccounts.size,
        daily_volume: transactions.reduce((sum, tx) => sum + tx.amount, 0)
      };
    }
    
    return baseStats;
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