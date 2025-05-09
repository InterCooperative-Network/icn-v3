use crate::error::IdentityError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::TrustError;
use crate::trust::{QuorumConfig, QuorumProof, QuorumRule};
use ed25519_dalek::PublicKey;
use std::collections::{HashSet};

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

impl Default for VerifiableCredentialBuilder {
    fn default() -> Self {
        Self::new()
    }
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
        let id = self
            .id
            .ok_or_else(|| IdentityError::InvalidCredential("ID is required".to_string()))?;
        let issuer = self
            .issuer
            .ok_or_else(|| IdentityError::InvalidCredential("Issuer is required".to_string()))?;
        let issuance_date = self.issuance_date.ok_or_else(|| {
            IdentityError::InvalidCredential("Issuance date is required".to_string())
        })?;
        let subject_id = self.subject_id.ok_or_else(|| {
            IdentityError::InvalidCredential("Subject ID is required".to_string())
        })?;
        let proof = self
            .proof
            .ok_or_else(|| IdentityError::InvalidCredential("Proof is required".to_string()))?;

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

impl TrustBundle {
    /// Verify all credentials in the bundle and validate the quorum
    pub fn verify(&self, config: &QuorumConfig) -> Result<bool, TrustError> {
        // 1. Extract all unique issuers (signers) from the credentials
        let mut signers = Vec::new();
        let mut unique_ids = HashSet::new();
        
        // 2. Verify each credential and collect signers
        for credential in &self.credentials {
            // Ensure each credential has a unique ID
            if !unique_ids.insert(&credential.id) {
                return Err(TrustError::InvalidBundle("Duplicate credential ID".to_string()));
            }
            
            // Add the issuer to signers
            signers.push(credential.issuer.clone());
            
            // Here we would verify the credential signature
            // This requires public keys for each issuer DID
            // For now, we'll just validate the bundle structure
        }
        
        // 3. Check for duplicate signers
        let unique_signers: HashSet<&String> = signers.iter().collect();
        if unique_signers.len() != signers.len() {
            return Err(TrustError::DuplicateSigners);
        }
        
        // 4. Validate the quorum against the config
        config.validate_quorum(&signers)
    }
    
    /// Create a new TrustBundle with a quorum proof
    pub fn new_with_proof(
        id: String,
        credentials: Vec<VerifiableCredential>,
        quorum_rule: QuorumRule,
    ) -> Self {
        Self {
            id,
            credentials,
            quorum_rule: serde_json::to_string(&quorum_rule).unwrap_or_default(),
            created: chrono::Utc::now().to_rfc3339(),
            expires: None,
        }
    }
    
    /// Add an expiration time to the bundle
    pub fn with_expiration(mut self, expires: &str) -> Self {
        self.expires = Some(expires.to_string());
        self
    }
    
    /// Extract the signers (issuers) from the bundle
    pub fn extract_signers(&self) -> Vec<String> {
        self.credentials
            .iter()
            .map(|credential| credential.issuer.clone())
            .collect()
    }
    
    /// Validate the quorum rule
    pub fn validate_quorum(&self, authorized_dids: &[String]) -> Result<bool, TrustError> {
        // Parse the quorum rule from string
        let quorum_rule: QuorumRule = serde_json::from_str(&self.quorum_rule)
            .map_err(|_| TrustError::InvalidBundle("Invalid quorum rule format".to_string()))?;
        
        // Create a config with the parsed rule
        let config = QuorumConfig {
            rule: quorum_rule,
            authorized_dids: authorized_dids.to_vec(),
        };
        
        // Get the signers from the bundle
        let signers = self.extract_signers();
        
        // Validate the quorum
        config.validate_quorum(&signers)
    }
}
