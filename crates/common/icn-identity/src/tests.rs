use crate::{Did, KeyPair};

#[test]
fn did_round_trip_ed25519() {
    let kp = KeyPair::generate();
    let did_str = kp.did.as_str().to_owned();

    // Re-parse DID → public key
    let pk = kp.did.to_ed25519().unwrap();
    assert_eq!(pk.to_bytes(), kp.pk.to_bytes());

    // Manually decode the multibase bytes
    let did2 = Did::new_ed25519(&pk);
    assert_eq!(did2.as_str(), did_str);
}

#[test]
fn sign_and_verify() {
    let kp = KeyPair::generate();
    let msg = b"ICN rocks";
    let sig = kp.sign(msg);
    assert!(kp.verify(msg, &sig));

    // Tamper
    let mut bad = sig.to_bytes();
    bad[0] ^= 0xFF;
    
    // In ed25519-dalek v2, from_bytes returns a Signature directly, not a Result
    let bad_sig = ed25519_dalek::Signature::from_bytes(&bad);
    assert!(!kp.verify(msg, &bad_sig));
}

#[test]
fn malformed_did_rejected() {
    // Random base58 string, wrong prefix.
    let bad = "did:key:zQ3shBAdummy";
    assert!(bad.parse::<Did>().is_err_or_none());
}

trait ErrOrNone<T, E> {
    fn is_err_or_none(self) -> bool;
}
impl<T, E> ErrOrNone<T, E> for Result<T, E> {
    fn is_err_or_none(self) -> bool { self.is_err() }
} 