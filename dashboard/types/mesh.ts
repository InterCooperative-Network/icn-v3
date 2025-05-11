// Defines the data structures for Mesh Compute features in the ICN Dashboard

/**
 * Represents a job on the ICN Mesh Network.
 */
export interface MeshJob {
    job_id: string;
    originator_did: string;
    originator_org_scope?: {
        federation_id?: string;
        coop_id?: string;
        community_id?: string;
    };
    params: {
        wasm_cid: string;
        function_name: string;
        required_resources_json: string; // JSON string, consider parsing on frontend
        qos_profile: string; // e.g., "Fast", "Cheap", "Balanced"
        max_acceptable_bid_icn?: number;
    };
    submitted_at: string; // ISO 8601 timestamp string
    // Frontend-augmented or API-derived status (optional):
    status?: 'Pending' | 'Executing' | 'Completed' | 'Failed' | 'InterestReceived';
}

/**
 * Represents interest expressed by an executor for a job.
 * Basic stub for now.
 */
export interface JobInterest {
    executor_did: string;
    job_id: string;
    // Future details: timestamp, bid_summary, etc.
}

/**
 * Represents an announcement that an execution receipt is available.
 */
export interface AnnouncedReceipt {
    job_id: string;
    receipt_cid: string;
    executor_did: string;
}

/**
 * Represents a full Execution Receipt.
 * This should mirror the backend ExecutionReceipt structure.
 */
export interface ExecutionReceipt {
    job_id: string;
    executor: string; // DID of the executor node
    status: string; // e.g., "CompletedSuccess", "CompletedFailure", "ExecutionError"
    result_data_cid?: string; // CID of the job's output data
    logs_cid?: string; // CID of the execution logs
    resource_usage: Record<string, number | string>; // e.g., { "CPU_CORES": 1, "MEMORY_MB": 512, "EXECUTION_TIME_MS": 2500 }
    execution_start_time: number; // Unix timestamp (seconds)
    execution_end_time: number; // Unix timestamp (seconds)
    signature: string; // Hex or base64 encoded signature of the receipt
    coop_id?: string;
    community_id?: string;
    // Any other fields that the backend receipt might include
}

/**
 * Represents the link between a Job ID and its Execution Receipt CID.
 */
export interface JobReceiptLink {
    job_id: string;
    receipt_cid?: string; // Optional because the receipt might not be available yet
} 