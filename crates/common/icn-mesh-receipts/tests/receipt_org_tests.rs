use chrono::{Utc, DateTime, NaiveDateTime};
use icn_economics::ResourceType;
use icn_identity::KeyPair;
use icn_mesh_receipts::ExecutionReceipt;
use icn_types::org::{CooperativeId, CommunityId};
use icn_types::mesh::JobStatus;
use std::collections::HashMap;
use serde_cbor;
use serde_json;

#[test]
fn test_receipt_with_org_identifiers() {
    // Setup basic usage data
    let mut usage = HashMap::new();
    usage.insert(ResourceType::Cpu, 1000);
    
    // Create test cooperative and community IDs
    let coop_id = CooperativeId::new("coop-123");
    let community_id = CommunityId::new("community-456");
    
    // Generate a valid DID
    let kp = KeyPair::generate();
    
    // Fixed timestamps for deterministic testing
    let start_time = 1_672_502_400u64; // 2023-01-01 00:00:00 UTC
    let end_time = 1_672_506_000u64;   // 2023-01-01 01:00:00 UTC
    let end_dt: DateTime<Utc> = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(end_time as i64, 0), Utc);

    // Create a receipt with organization identifiers
    let receipt = ExecutionReceipt {
        job_id: "test-job-id".to_string(),
        executor: kp.did.clone(),
        status: JobStatus::Completed,
        result_data_cid: None,
        logs_cid: None,
        resource_usage: usage.clone(),
        execution_start_time: start_time,
        execution_end_time: end_time,
        execution_end_time_dt: end_dt,
        signature: vec![1, 2, 3, 4],
        coop_id: Some(coop_id.clone()),
        community_id: Some(community_id.clone()),
    };
    
    // Check that the organization IDs are stored correctly
    assert_eq!(receipt.coop_id.as_ref().unwrap().to_string(), "coop-123");
    assert_eq!(receipt.community_id.as_ref().unwrap().to_string(), "community-456");
    
    // Test JSON serialization/deserialization
    let json = serde_json::to_string(&receipt).unwrap();
    let deserialized: ExecutionReceipt = serde_json::from_str(&json).unwrap();
    
    assert_eq!(receipt.coop_id, deserialized.coop_id);
    assert_eq!(receipt.community_id, deserialized.community_id);
    
    // Test CBOR serialization/deserialization
    let cbor = serde_cbor::to_vec(&receipt).unwrap();
    let deserialized: ExecutionReceipt = serde_cbor::from_slice(&cbor).unwrap();
    
    assert_eq!(receipt.coop_id, deserialized.coop_id);
    assert_eq!(receipt.community_id, deserialized.community_id);
}

#[test]
fn test_cid_changes_with_different_orgs() {
    // Setup basic usage data
    let mut usage = HashMap::new();
    usage.insert(ResourceType::Cpu, 500);
    
    // Generate a valid DID
    let kp = KeyPair::generate();
    
    // Fixed timestamps
    let start_time = 1_672_502_400u64;
    let end_time = 1_672_506_000u64;
    let end_dt: DateTime<Utc> = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(end_time as i64, 0), Utc);

    // Create a receipt with no org IDs
    let receipt1 = ExecutionReceipt {
        job_id: "task-123".to_string(),
        executor: kp.did.clone(),
        status: JobStatus::Completed,
        result_data_cid: None,
        logs_cid: None,
        resource_usage: usage.clone(),
        execution_start_time: start_time,
        execution_end_time: end_time,
        execution_end_time_dt: end_dt,
        signature: vec![1, 2, 3, 4],
        coop_id: None,
        community_id: None,
    };
    
    // Create an identical receipt but with coop ID
    let mut receipt2 = receipt1.clone();
    receipt2.coop_id = Some(CooperativeId::new("coop-123"));
    
    // Create a receipt with both org IDs
    let mut receipt3 = receipt2.clone();
    receipt3.community_id = Some(CommunityId::new("community-456"));
    
    // Different org scopes should produce different CIDs
    let cid1 = receipt1.cid().unwrap();
    let cid2 = receipt2.cid().unwrap();
    let cid3 = receipt3.cid().unwrap();
    
    assert_ne!(cid1, cid2, "Receipt with coop ID should have different CID");
    assert_ne!(cid2, cid3, "Receipt with community ID should have different CID");
    assert_ne!(cid1, cid3, "Receipts with different org scopes should have different CIDs");
} 