use criterion::{criterion_group, criterion_main, Criterion};
use icn_identity::{KeyPair, VerifiableCredential};

fn bench_verify(c: &mut Criterion) {
    let kp = KeyPair::generate();
    let vc = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".into()],
        types: vec!["VerifiableCredential".into(), "ExampleCredential".into()],
        issuer: kp.did.clone(),
        issuance_date: chrono::Utc::now(),
        credential_subject: serde_json::json!({"id": kp.did.as_str()}),
        proof: None,
    };
    let signed = vc.sign(&kp).unwrap();

    c.bench_function("vc_verify", |b| {
        b.iter(|| signed.verify(&kp.pk).unwrap());
    });
}
criterion_group!(benches, bench_verify);
criterion_main!(benches); 