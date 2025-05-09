pub mod jws;

pub use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature};
pub use jws::{sign_detached_jws, verify_detached_jws}; 