# ADR-0002: DAG Codec & CID Verification Strategy

## Status

ACCEPTED

## Date

2025-05-10

## Context

The InterCooperative Network (ICN) relies on an append-only DAG (Directed Acyclic Graph) with Merkle roots and Content Identifiers (CIDs) to guarantee immutability of the ledger. Since multiple implementations of the ICN protocol may exist across different languages and platforms, it's crucial to ensure that the CID generation and verification is consistent across all implementations.

Key considerations:
- The DAG structure must be serialized in a deterministic way
- CID generation must follow a standard format across all implementations
- We need a mechanism to verify cross-language compatibility
- The CID format should align with the IPLD standards

## Decision

We will use the following approach for DAG codec and CID verification:

1. **Data Serialization**:
   - Use CBOR (Concise Binary Object Representation) as the primary serialization format
   - Use the `dag-cbor` codec (0x71) for CID generation
   - Ensure deterministic encoding by sorting map keys and applying consistent encoding rules

2. **CID Generation**:
   - Use CIDv1 with the SHA-256 hashing algorithm
   - Follow the multiformat specification for CID generation
   - Encode CIDs as Base58 strings for human-readable representation

3. **Cross-Language Verification**:
   - Maintain a set of "golden vectors" - pre-computed CIDs for specific test data
   - Implement cross-tests between Rust and Go (reference implementation)
   - Use these golden vectors in CI to ensure compatibility

4. **Implementation Details**:
   - Use the `cid` crate in Rust, which aligns with the IPLD specifications
   - Implement `DagNode` with proper serialization and CID generation
   - Create deterministic test vectors to verify implementation

## Consequences

### Positive

- Ensures consistent CID generation across all implementations
- Enables cross-language interoperability for the ICN protocol
- Follows established standards (IPLD, multiformat) for content addressing
- Test vectors provide a mechanism to verify new implementations

### Negative

- Requires careful attention to serialization details to ensure determinism
- Adds complexity in maintaining golden vectors across implementations
- May require updates if the underlying standards evolve

### Neutral

- Specific version dependencies on codec libraries
- Need for additional test infrastructure to verify cross-language compatibility

## Implementation

1. Implement the `DagNode` structure in `icn-types` with proper CBOR serialization
2. Create the CID generation function following the multiformat specification
3. Generate golden vectors for test data in both Rust and Go implementations
4. Implement cross-tests to verify compatibility
5. Integrate verification into the CI pipeline

## Notes

Reference implementations and specifications:
- [IPLD CID Specification](https://github.com/multiformats/cid)
- [Multiformat Specification](https://github.com/multiformats/multiformat)
- [DAG-CBOR Specification](https://github.com/ipld/specs/blob/master/block-layer/codecs/dag-cbor.md)

Test vectors will be maintained in the `tests/codec/` directory, with corresponding test implementations in both Rust and Go. 