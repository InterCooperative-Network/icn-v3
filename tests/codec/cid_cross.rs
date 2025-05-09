use cid::{Cid, Version};
use icn_types::dag::DagNode;
use multihash::{Code, MultihashDigest};

/// Test that our CID generation matches the expected value from the Go implementation
#[test]
fn test_cid_cross_language_compatibility() {
    // Create a deterministic DagNode for testing
    let test_data = b"cid-test";
    let dag_node = DagNode::new_leaf(test_data.to_vec());
    
    // Generate CID using our implementation
    let cid = dag_node.cid();
    
    // Generate the expected CID manually to verify our implementation
    let hash = Code::Sha2_256.digest(test_data);
    let expected_cid = Cid::new_v1(0x71, hash); // 0x71 is the dag-cbor codec
    
    assert_eq!(cid, expected_cid, "Generated CID does not match expected value");
    
    // Verify against the golden value from Go implementation
    let golden_cid = include_str!("golden_cid.txt").trim();
    let parsed_golden_cid = Cid::try_from(golden_cid).expect("Failed to parse golden CID");
    
    assert_eq!(
        cid, parsed_golden_cid,
        "Generated CID does not match golden CID from Go implementation"
    );
    
    // Verify CID version and codec
    assert_eq!(cid.version(), Version::V1);
    assert_eq!(cid.codec(), 0x71); // dag-cbor codec
    
    // Print the Base58 encoded CID for reference
    println!("Generated CID: {}", cid.to_string());
} 