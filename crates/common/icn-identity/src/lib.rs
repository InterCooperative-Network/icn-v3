//! ICN Identity – DID & key tooling for the InterCooperative Network.
//!
//! - Supports `did:key` using Ed25519 (`multicodec: 0xED`, `multibase: Z-base58`).
//! - Provides `KeyPair` generation, signing, and verification.
//! - Implements Verifiable Credentials with canonical serialization.
//! - Provides QuorumProof and TrustBundle for federation governance.
//! - Zero `unsafe`; Clippy-clean; `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

mod did;
mod keypair;
mod quorum;
mod trust_bundle;
mod trust_validator;
mod vc;
mod identity_index;
mod scope_key;
#[cfg(test)]
mod tests;

pub use did::{Did, DidError};
pub use keypair::{KeyPair, Signature};
pub use quorum::{QuorumError, QuorumProof, QuorumType};
pub use trust_bundle::{FederationMetadata, TrustBundle, TrustBundleError};
pub use trust_validator::{TrustValidator, TrustValidationError};
pub use vc::{VerifiableCredential, SignedCredential, CredentialError, Proof};
pub use identity_index::IdentityIndex;
pub use scope_key::ScopeKey; 