use icn_types::dag::{DagEventType, DagNodeBuilder};
use icn_types::error::DagError;

#[test]
fn test_dag_cid_generation() {
    let node = DagNodeBuilder::new()
        .content("Hello, ICN!".into())
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    let cid = node.cid().expect("CID generation failed");
    println!("CID: {}", cid);
}

#[test]
fn test_dag_node_builder_success() {
    let content = "Test content".to_string();
    let node = DagNodeBuilder::new()
        .content(content.clone())
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build()
        .expect("Building DagNode should succeed");

    assert_eq!(node.content, content);
    assert_eq!(node.parent, None);
    assert_eq!(node.event_type, DagEventType::Genesis);
    assert_eq!(node.scope_id, "test_scope");
}

#[test]
fn test_dag_node_builder_with_parent_success() {
    // First create a node to get a valid CID
    let first_node = DagNodeBuilder::new()
        .content("Parent node".into())
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    let parent_cid = first_node.cid().expect("CID generation failed");

    // Now create a node with that parent
    let child_node = DagNodeBuilder::new()
        .content("Child content".into())
        .parent(parent_cid)
        .event_type(DagEventType::Proposal)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    assert_eq!(child_node.content, "Child content");
    assert_eq!(child_node.parent, Some(parent_cid));
    assert_eq!(child_node.event_type, DagEventType::Proposal);
    assert_eq!(child_node.scope_id, "test_scope");
}

#[test]
fn test_dag_node_builder_missing_fields_fails() {
    // Missing content
    let result = DagNodeBuilder::new()
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build();

    assert!(result.is_err());
    match result {
        Err(DagError::InvalidStructure(_)) => {} // Expected
        other => panic!("Expected InvalidStructure error, got {:?}", other),
    }

    // Missing event_type
    let result = DagNodeBuilder::new()
        .content("Test content".into())
        .scope_id("test_scope".into())
        .build();

    assert!(result.is_err());
    match result {
        Err(DagError::InvalidStructure(_)) => {} // Expected
        other => panic!("Expected InvalidStructure error, got {:?}", other),
    }

    // Missing scope_id
    let result = DagNodeBuilder::new()
        .content("Test content".into())
        .event_type(DagEventType::Genesis)
        .build();

    assert!(result.is_err());
    match result {
        Err(DagError::InvalidStructure(_)) => {} // Expected
        other => panic!("Expected InvalidStructure error, got {:?}", other),
    }
}

#[test]
fn test_dag_node_to_builder_and_back() {
    // Create original node
    let original = DagNodeBuilder::new()
        .content("Original content".into())
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    // Convert to builder and back
    let rebuilt = original
        .builder()
        .build()
        .expect("Building from builder should succeed");

    // Verify equality
    assert_eq!(original, rebuilt);

    // Test with parent
    let parent_cid = original.cid().expect("CID generation failed");
    let original_with_parent = DagNodeBuilder::new()
        .content("Node with parent".into())
        .parent(parent_cid)
        .event_type(DagEventType::Proposal)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    let rebuilt_with_parent = original_with_parent
        .builder()
        .build()
        .expect("Building from builder should succeed");

    assert_eq!(original_with_parent, rebuilt_with_parent);
}

#[test]
fn test_dag_node_builder_with_parent_and_builder() {
    // First create a node to get a valid CID
    let first_node = DagNodeBuilder::new()
        .content("Parent node".into())
        .event_type(DagEventType::Genesis)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    let parent_cid = first_node.cid().expect("CID generation failed");

    // Now create a node with that parent
    let child_node = DagNodeBuilder::new()
        .content("Child content".into())
        .parent(parent_cid)
        .event_type(DagEventType::Proposal)
        .scope_id("test_scope".into())
        .build()
        .expect("Node creation failed");

    assert_eq!(child_node.content, "Child content");
    assert_eq!(child_node.parent, Some(parent_cid));
    assert_eq!(child_node.event_type, DagEventType::Proposal);
    assert_eq!(child_node.scope_id, "test_scope");

    // Create with parent
    let with_parent = DagNodeBuilder::new()
        .content("From builder".into())
        .parent(parent_cid)
        .event_type(DagEventType::Proposal)
        .scope_id("test_scope".into())
        .build()
        .expect("Failed to build from builder");

    assert_eq!(with_parent.content, "From builder");
    assert_eq!(with_parent.parent, Some(parent_cid));
    assert_eq!(with_parent.event_type, DagEventType::Proposal);
    assert_eq!(with_parent.scope_id, "test_scope");
}
