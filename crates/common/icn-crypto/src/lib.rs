pub mod jws;

pub use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
pub use jws::{sign_detached_jws, verify_detached_jws};
