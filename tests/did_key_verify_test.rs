#[cfg(test)]
mod tests {
    use ed25519_dalek::Keypair;
    use icn_identity::did::{did_key_from_pk, pk_from_did_key};
    use rand::rngs::OsRng;

    #[test]
    fn verify_did_key_operations() {
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
                assert_eq!(original, extracted, "Round-trip failed - keys don't match");
                println!("Round-trip match: ✓ PASS");
            },
            Err(e) => {
                println!("ERROR: {:?}", e);
                panic!("Failed to extract public key from DID");
            }
        }
        
        // Try with known test vector
        let test_did = "did:key:z6MkhmJRJXAGspKnWHPWn6c7U8JdBdf1LXaTYZXSacHXSmzH";
        println!("\nTest with RFC vector: {}", test_did);
        match pk_from_did_key(test_did) {
            Ok(pk) => {
                println!("✓ Valid test DID decoded successfully");
                println!("Decoded public key: {:?}", pk.to_bytes());
            },
            Err(e) => {
                println!("✗ Failed to decode test DID: {:?}", e);
                panic!("Failed to decode test DID");
            }
        }
        
        // Output DID format information
        println!("\nDID Composition Details:");
        println!("- Prefix: did:key:z (multibase prefix for base58btc)");
        println!("- Multicodec prefix: 0xed01 (Ed25519 public key)");
        println!("- Key bytes: 32-byte Ed25519 public key");
        println!("- Encoding: multibase(base58btc, multicodec(ed25519-pub, raw-public-key-bytes))");
    }
} 