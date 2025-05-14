use criterion::{criterion_group, criterion_main, Criterion};
use icn_identity::{FederationMetadata, KeyPair, QuorumProof, QuorumType, TrustBundle};
use std::collections::HashMap;

fn bench_trustbundle_verify(c: &mut Criterion) {
    // Create 5 keypairs as signers
    let keypairs: Vec<KeyPair> = (0..5).map(|_| KeyPair::generate()).collect();

    // Create federation metadata
    let metadata = FederationMetadata {
        name: "Benchmark Federation".to_string(),
        description: Some("A federation for benchmarking".to_string()),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };

    // Create a trust bundle
    let mut bundle = TrustBundle::new(
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
        metadata,
    );

    // Calculate the hash for signing
    let bundle_hash = bundle.calculate_hash().unwrap();

    // Create signatures from 3 signers (majority)
    let signatures = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(&bundle_hash)),
        (keypairs[1].did.clone(), keypairs[1].sign(&bundle_hash)),
        (keypairs[2].did.clone(), keypairs[2].sign(&bundle_hash)),
    ];

    // Create a quorum proof
    let proof = QuorumProof::new(QuorumType::Majority, signatures);

    // Add the proof to the bundle
    bundle.add_quorum_proof(proof);

    // Create a map of trusted signer verifying keys
    let mut trusted_keys = HashMap::new();
    for kp in &keypairs {
        trusted_keys.insert(kp.did.clone(), kp.pk);
    }

    // Benchmark verification
    c.bench_function("trustbundle_verify", |b| {
        b.iter(|| bundle.verify(&trusted_keys).unwrap());
    });
}

criterion_group!(benches, bench_trustbundle_verify);
criterion_main!(benches);
