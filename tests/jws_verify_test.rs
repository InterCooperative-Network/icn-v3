#[cfg(test)]
mod tests {
    use ed25519_dalek::Keypair;
    use icn_crypto::{sign_detached_jws, verify_detached_jws};
    use rand::rngs::OsRng;

    #[test]
    fn verify_jws_round_trip() {
        let mut csprng = OsRng{};
        let keypair = Keypair::generate(&mut csprng);
        let payload = b"ICN verification test";
        
        let jws = sign_detached_jws(payload, &keypair).unwrap();
        println!("Payload: {}", String::from_utf8_lossy(payload));
        println!("JWS: {}", jws);
        
        // Verify with our own code
        let result = verify_detached_jws(payload, &jws, &keypair.public);
        assert!(result.is_ok(), "JWS verification failed!");
        println!("Self-verification: ✓ PASS");

        // Verify the JWS format
        let parts: Vec<&str> = jws.split('.').collect();
        assert_eq!(parts.len(), 3, "JWS should have 3 parts");
        assert!(!parts[0].is_empty(), "Header should not be empty");
        assert!(parts[1].is_empty(), "Middle part should be empty in detached JWS");
        assert!(!parts[2].is_empty(), "Signature should not be empty");
        println!("JWS format check: ✓ PASS");
        
        // Format validation for downstream tools
        println!("JWS can be validated by external tools with:");
        println!("- Header: {}", parts[0]);
        println!("- Payload (base64): {}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload));
        println!("- Signature: {}", parts[2]);
    }
} 