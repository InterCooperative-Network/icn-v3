//! ICN Identity – DID & key tooling for the InterCooperative Network.
//!
//! - Supports `did:key` using Ed25519 (`multicodec: 0xED`, `multibase: Z-base58`).
//! - Provides `KeyPair` generation, signing, and verification.
//! - Zero `unsafe`; Clippy-clean; `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

mod did;
mod keypair;
mod vc;
#[cfg(test)]
mod tests;

pub use did::{Did, DidError};
pub use keypair::{KeyPair, Signature};
pub use vc::{VerifiableCredential, SignedCredential, CredentialError, Proof}; 