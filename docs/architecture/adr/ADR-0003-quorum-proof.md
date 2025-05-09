# ADR-0003: Quorum Proofs & Trust Validation

## Status

Accepted

## Context

The InterCooperative Network (ICN) relies on distributed governance across multiple federated nodes. A critical aspect of distributed governance is reaching consensus on governance actions. We need a mechanism to represent when a sufficient quorum of participants has agreed to validate a governance action.

Key considerations include:

1. **Flexible Quorum Models**: Different governance structures may require different quorum models - majority vote, weighted voting, thresholds, etc.
2. **Auditability**: Governance actions must be auditable, with clear records of who approved what.
3. **Composability**: Quorum proofs should be composable with our existing Verifiable Credentials model.
4. **Efficiency**: Validation should be as lightweight as possible while remaining secure.
5. **Cross-Chain Compatibility**: Proofs should be usable across different chains and systems.

## Decision

We will implement a Quorum Proof system with the following characteristics:

1. **TrustBundles as Containers**: We'll use TrustBundle structures to hold collections of credentials that represent approvals from different authorities.

2. **Flexible Quorum Rules**:
   - `Majority`: More than 50% of authorized participants
   - `Threshold(u8)`: Specified percentage of authorized participants
   - `Weighted(HashMap<Did, u32>)`: Different weights for different participants

3. **Quorum Verification Process**:
   - Validation of all credentials in a bundle
   - Elimination of duplicate signers
   - Checking quorum satisfaction against the specified rule
   - Verification that signers are authorized

4. **Integration with VCs**:
   - Each approval is represented as a Verifiable Credential
   - The TrustBundle acts as a container of these credentials
   - The bundle itself can be anchored to a content-addressable DAG

## Consequences

### Positive

1. **Flexible Governance Models**: Organizations can define rules that match their governance structures.
2. **Cryptographic Verifiability**: All approvals are individually signed, allowing granular verification.
3. **Federation-Friendly**: The model works across federated systems without requiring centralized authorities.
4. **Audit Trail**: Provides a complete, immutable record of governance decisions.
5. **Separation of Concerns**: Decouples rule definition from rule enforcement.

### Negative

1. **Implementation Complexity**: More complex than simple threshold schemes.
2. **Verification Overhead**: Verifying multiple signatures is more compute-intensive than simpler schemes.
3. **Key Management**: Requires robust key management for all participants.

### Neutral

1. **Storage Requirements**: Bundles of proofs require more storage than simple threshold signatures, but provide much richer information.

## Technical Details

### QuorumRule Types

```rust
enum QuorumRule {
    Majority,
    Threshold(u8), // 0-100 percentage
    Weighted {
        weights: HashMap<String, u32>,
        threshold: u32,
    },
}
```

### QuorumConfig Structure

```rust
struct QuorumConfig {
    rule: QuorumRule,
    authorized_dids: Vec<String>,
}
```

### TrustBundle Verification

```rust
impl TrustBundle {
    pub fn verify(&self, config: &QuorumConfig) -> Result<bool, TrustError> {
        // 1. Extract signers from credentials
        // 2. Verify all credentials are valid
        // 3. Check for duplicate signers
        // 4. Validate against quorum rule
    }
}
```

## Alternatives Considered

1. **Simple Multisignature**: A basic M-of-N multisig scheme would be simpler but less flexible.
2. **On-Chain Governance**: Moving all governance on-chain would provide tighter integration but limit interoperability.
3. **Centralized Authority**: Having a central authority validate governance would be simpler but contradict our federated model.

## References

- [W3C Verifiable Credentials](https://www.w3.org/TR/vc-data-model/)
- [DIF Presentation Exchange](https://identity.foundation/presentation-exchange/)
- [RFC 7515: JSON Web Signature (JWS)](https://tools.ietf.org/html/rfc7515) 