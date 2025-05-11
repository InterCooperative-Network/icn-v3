use ed25519_dalek::Keypair;
use icn_identity::did::{did_key_from_pk, pk_from_did_key};
use rand::rngs::OsRng;

fn main() {
    // Generate a random key
    let mut csprng = OsRng{};
    let keypair = Keypair::generate(&mut csprng);
    
    // Convert to DID
    let did = did_key_from_pk(&keypair.public);
    println!("Generated DID: {}", did);
    
    // Try to extract public key back
    match pk_from_did_key(&did) {
        Ok(pk) => {
            let original = keypair.public.to_bytes();
            let extracted = pk.to_bytes();
            println!("Original key: {:?}", original);
            println!("Extracted key: {:?}", extracted);
            println!("Round-trip match: {}", if original == extracted {"✓ PASS"} else {"✗ FAIL"});
        },
        Err(e) => println!("ERROR: {:?}", e)
    }
    
    // Try with known test vector
    let test_did = "did:key:z6MkhmJRJXAGspKnWHPWn6c7U8JdBdf1LXaTYZXSacHXSmzH";
    println!("\nTest with RFC vector: {}", test_did);
    match pk_from_did_key(test_did) {
        Ok(_) => println!("✓ Valid test DID decoded successfully"),
        Err(e) => println!("✗ Failed to decode test DID: {:?}", e)
    }
}
