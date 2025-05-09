use icn_types::dag::DagNode;

#[test]
fn test_dag_cid_generation() {
    let node = DagNode {
        content: "Hello, ICN!".into(),
        parent: None,
    };

    let cid = node.cid().expect("CID generation failed");
    println!("CID: {}", cid);
}
