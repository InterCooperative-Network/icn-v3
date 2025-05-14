//! ICN Identity – DID & key tooling for the InterCooperative Network.
//!
//! - Supports `did:key` using Ed25519 (`multicodec: 0xED`, `multibase: Z-base58`).
//! - Provides `KeyPair` generation, signing, and verification.
//! - Implements Verifiable Credentials with canonical serialization.
//! - Provides QuorumProof and TrustBundle for federation governance.
//! - Zero `unsafe`; Clippy-clean; `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

mod did;
mod identity_index;
mod keypair;
mod quorum;
mod scope_key;
#[cfg(test)]
mod tests;
mod trust_bundle;
mod trust_validator;
mod vc;

pub use did::{Did, DidError};
pub use identity_index::IdentityIndex;
pub use keypair::{KeyPair, Signature};
pub use quorum::{QuorumError, QuorumProof, QuorumType};
pub use scope_key::ScopeKey;
pub use trust_bundle::{FederationMetadata, TrustBundle, TrustBundleError};
pub use trust_validator::{TrustValidationError, TrustValidator};
pub use vc::{CredentialError, Proof, SignedCredential, VerifiableCredential};
