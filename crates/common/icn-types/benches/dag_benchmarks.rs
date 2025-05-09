use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use icn_types::dag::{DagEventType, DagNodeBuilder};

fn benchmark_dag_node_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("dag_node_creation");

    // Simple node creation with only required fields
    group.bench_function("simple", |b| {
        b.iter(|| {
            DagNodeBuilder::new()
                .content("Test content".to_string())
                .event_type(DagEventType::Genesis)
                .scope_id("test_scope".to_string())
                .build()
                .unwrap()
        })
    });

    // More complex node creation with all fields
    group.bench_function("complex", |b| {
        // Create a parent node first
        let parent_node = DagNodeBuilder::new()
            .content("Parent content".to_string())
            .event_type(DagEventType::Genesis)
            .scope_id("test_scope".to_string())
            .build()
            .unwrap();

        let parent_cid = parent_node.cid().unwrap();

        b.iter(|| {
            DagNodeBuilder::new()
                .content("Child content".to_string())
                .parent(parent_cid)
                .event_type(DagEventType::Proposal)
                .scope_id("test_scope".to_string())
                .timestamp(1620000000000)
                .build()
                .unwrap()
        })
    });

    group.finish();
}

fn benchmark_dag_node_cid(c: &mut Criterion) {
    let mut group = c.benchmark_group("dag_node_cid");

    // Generate nodes of different sizes
    let sizes = [10, 100, 1000, 10000];

    for size in sizes.iter() {
        // Create content of specified size
        let content = "X".repeat(*size);

        let node = DagNodeBuilder::new()
            .content(content)
            .event_type(DagEventType::Genesis)
            .scope_id("test_scope".to_string())
            .build()
            .unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| node.cid())
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_dag_node_creation, benchmark_dag_node_cid);
criterion_main!(benches);
