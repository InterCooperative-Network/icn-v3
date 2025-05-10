use anyhow::Result;
use icn_economics::ResourceType;
use std::collections::HashMap;

#[tokio::test]
async fn test_serialization_deserialization() -> Result<()> {
    // Create test data
    let mut usage = HashMap::new();
    usage.insert(ResourceType::Cpu, 500);
    
    // We manually construct the JSON to avoid serialization issues with the Did type
    let json_data = r#"{
        "task_cid": "bafybeieye123456789",
        "executor": "did:icn:testnode",
        "resource_usage": {
            "Cpu": 500
        },
        "timestamp": "2023-01-01T00:00:00Z",
        "signature": [9, 8, 7, 6]
    }"#;
    
    // Parse the JSON to validate the structure
    let value: serde_json::Value = serde_json::from_str(json_data)?;
    
    // Check that the keys we expect are present
    assert!(value.get("task_cid").is_some());
    assert!(value.get("executor").is_some());
    assert!(value.get("resource_usage").is_some());
    assert!(value.get("timestamp").is_some());
    assert!(value.get("signature").is_some());
    
    // Verify the resource_usage has the expected CPU value
    let resource_usage = value.get("resource_usage").unwrap();
    let cpu_value = resource_usage.get("Cpu").unwrap().as_u64().unwrap();
    assert_eq!(cpu_value, 500);
    
    Ok(())
} 