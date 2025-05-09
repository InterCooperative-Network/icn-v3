use icn_types::dag::{DagNode, DagNodeBuilder};
use icn_types::error::DagError;

#[test]
fn test_dag_cid_generation() {
    let node = DagNode {
        content: "Hello, ICN!".into(),
        parent: None,
    };

    let cid = node.cid().expect("CID generation failed");
    println!("CID: {}", cid);
}

#[test]
fn test_dag_node_builder_success() {
    let content = "Test content".to_string();
    let node = DagNodeBuilder::new()
        .content(content.clone())
        .build()
        .expect("Building DagNode should succeed");

    assert_eq!(node.content, content);
    assert_eq!(node.parent, None);
}

#[test]
fn test_dag_node_builder_with_parent_success() {
    // First create a node to get a valid CID
    let first_node = DagNode {
        content: "Parent node".into(),
        parent: None,
    };
    let parent_cid = first_node.cid().expect("CID generation failed");

    // Now create a node with that parent
    let content = "Child content".to_string();
    let node = DagNodeBuilder::new()
        .content(content.clone())
        .parent(parent_cid.clone())
        .build()
        .expect("Building DagNode should succeed");

    assert_eq!(node.content, content);
    assert_eq!(node.parent, Some(parent_cid));
}

#[test]
fn test_dag_node_builder_missing_content_fails() {
    let result = DagNodeBuilder::new().build();

    assert!(result.is_err());
    match result {
        Err(DagError::InvalidStructure(_)) => {} // Expected
        other => panic!("Expected InvalidStructure error, got {:?}", other),
    }
}

#[test]
fn test_dag_node_to_builder_and_back() {
    // Create original node
    let original = DagNode {
        content: "Original content".into(),
        parent: None,
    };

    // Convert to builder and back
    let rebuilt = original
        .builder()
        .build()
        .expect("Building from builder should succeed");

    // Verify equality
    assert_eq!(original, rebuilt);

    // Test with parent
    let parent_cid = original.cid().expect("CID generation failed");
    let original_with_parent = DagNode {
        content: "Node with parent".into(),
        parent: Some(parent_cid.clone()),
    };

    let rebuilt_with_parent = original_with_parent
        .builder()
        .build()
        .expect("Building from builder should succeed");

    assert_eq!(original_with_parent, rebuilt_with_parent);
}
