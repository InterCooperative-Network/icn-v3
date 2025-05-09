use crate::error::IdentityError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A Verifiable Credential subject containing claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct CredentialSubject {
    /// The DID of the subject
    pub id: String,
    /// Claims made about the subject
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub claims: HashMap<String, serde_json::Value>,
}

/// Proof attached to a Verifiable Credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct CredentialProof {
    /// Type of proof
    #[serde(rename = "type")]
    pub type_: String,
    /// Creation timestamp
    pub created: String,
    /// Verification method
    pub verification_method: String,
    /// Purpose of this proof
    pub proof_purpose: String,
    /// The JWS signature
    pub jws: String,
}

/// A Verifiable Credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct VerifiableCredential {
    /// The context for the credential
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    /// Unique ID for this credential
    pub id: String,
    /// Type of credential
    #[serde(rename = "type")]
    pub type_: Vec<String>,
    /// The issuer of the credential
    pub issuer: String,
    /// Issuance date
    pub issuance_date: String,
    /// Expiration date (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<String>,
    /// The credential subject containing the claims
    pub credential_subject: CredentialSubject,
    /// Proof of the credential
    pub proof: CredentialProof,
}

/// A TrustBundle containing a set of credentials for validating a governance event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct TrustBundle {
    /// Unique ID for this bundle
    pub id: String,
    /// Credentials included in this bundle
    pub credentials: Vec<VerifiableCredential>,
    /// The quorum rule applied to this bundle
    pub quorum_rule: String,
    /// Creation timestamp
    pub created: String,
    /// Expiration timestamp
    pub expires: Option<String>,
}

/// An ExecutionReceipt as a Verifiable Credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct ExecutionReceiptCredential {
    /// The base Verifiable Credential
    #[serde(flatten)]
    pub credential: VerifiableCredential,
    /// The CID of the execution event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_cid: Option<String>,
    /// Result of the execution
    pub success: bool,
    /// The output of the execution
    pub output: String,
    /// Resources consumed during execution
    pub resources_consumed: u64,
}

/// An AnchorCredential embedding a DAG root
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct AnchorCredential {
    /// The base Verifiable Credential
    #[serde(flatten)]
    pub credential: VerifiableCredential,
    /// The Merkle root of the DAG
    pub dag_root: String,
    /// The epoch number
    pub epoch: u64,
}

impl VerifiableCredential {
    /// Create a new builder for a Verifiable Credential
    pub fn builder() -> VerifiableCredentialBuilder {
        VerifiableCredentialBuilder::new()
    }
}

/// Builder for creating Verifiable Credential instances
pub struct VerifiableCredentialBuilder {
    context: Vec<String>,
    id: Option<String>,
    type_: Vec<String>,
    issuer: Option<String>,
    issuance_date: Option<String>,
    expiration_date: Option<String>,
    subject_id: Option<String>,
    claims: HashMap<String, serde_json::Value>,
    proof: Option<CredentialProof>,
}

impl VerifiableCredentialBuilder {
    /// Creates a new VerifiableCredentialBuilder with default context
    pub fn new() -> Self {
        Self {
            context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
            id: None,
            type_: vec!["VerifiableCredential".to_string()],
            issuer: None,
            issuance_date: None,
            expiration_date: None,
            subject_id: None,
            claims: HashMap::new(),
            proof: None,
        }
    }

    /// Sets the ID for the credential
    pub fn id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// Adds a type to the credential
    pub fn add_type(mut self, type_: String) -> Self {
        self.type_.push(type_);
        self
    }

    /// Sets the issuer for the credential
    pub fn issuer(mut self, issuer: String) -> Self {
        self.issuer = Some(issuer);
        self
    }

    /// Sets the issuance date for the credential
    pub fn issuance_date(mut self, issuance_date: String) -> Self {
        self.issuance_date = Some(issuance_date);
        self
    }

    /// Sets the expiration date for the credential
    pub fn expiration_date(mut self, expiration_date: String) -> Self {
        self.expiration_date = Some(expiration_date);
        self
    }

    /// Sets the subject ID for the credential
    pub fn subject_id(mut self, subject_id: String) -> Self {
        self.subject_id = Some(subject_id);
        self
    }

    /// Adds a claim to the credential
    pub fn add_claim(mut self, key: String, value: serde_json::Value) -> Self {
        self.claims.insert(key, value);
        self
    }

    /// Sets the proof for the credential
    pub fn proof(mut self, proof: CredentialProof) -> Self {
        self.proof = Some(proof);
        self
    }

    /// Builds a VerifiableCredential if all required fields are set
    pub fn build(self) -> Result<VerifiableCredential, IdentityError> {
        let id = self.id.ok_or_else(|| IdentityError::InvalidCredential("ID is required".to_string()))?;
        let issuer = self.issuer.ok_or_else(|| IdentityError::InvalidCredential("Issuer is required".to_string()))?;
        let issuance_date = self.issuance_date.ok_or_else(|| IdentityError::InvalidCredential("Issuance date is required".to_string()))?;
        let subject_id = self.subject_id.ok_or_else(|| IdentityError::InvalidCredential("Subject ID is required".to_string()))?;
        let proof = self.proof.ok_or_else(|| IdentityError::InvalidCredential("Proof is required".to_string()))?;

        Ok(VerifiableCredential {
            context: self.context,
            id,
            type_: self.type_,
            issuer,
            issuance_date,
            expiration_date: self.expiration_date,
            credential_subject: CredentialSubject {
                id: subject_id,
                claims: self.claims,
            },
            proof,
        })
    }
} 