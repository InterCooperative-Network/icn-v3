use ed25519_dalek::{Keypair, PublicKey, SecretKey};
use icn_crypto::{sign_detached_jws, verify_detached_jws};
use rand::rngs::OsRng;

fn main() {
    let mut csprng = OsRng{};
    let keypair = Keypair::generate(&mut csprng);
    let payload = b"ICN verification test";
    
    let jws = sign_detached_jws(payload, &keypair).unwrap();
    println!("Payload: {}", String::from_utf8_lossy(payload));
    println!("JWS: {}", jws);
    
    // Verify with our own code
    let result = verify_detached_jws(payload, &jws, &keypair.public);
    println!("Self-verification: {}", if result.is_ok() {"✓ PASS"} else {"✗ FAIL"});
}
