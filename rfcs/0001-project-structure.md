# RFC-0001: Project Structure

## Status: Accepted
## Date: 2025-05-09
## Authors: ICN Core Team

## Abstract

This RFC defines the project structure for the InterCooperative Network (ICN) codebase, establishing the organization of repositories, crates, and components based on the layered architecture design. It serves as the foundation for all future development work.

## Motivation

A well-defined project structure is critical for:
- Maintaining clear separation of concerns
- Enabling parallel development across teams
- Ensuring components follow the layered architecture
- Providing a coherent mental model for contributors
- Facilitating onboarding of new developers

## Technical Design

### Repository Map & Responsibilities

| Repository         | Purpose                                                                                        |
| ------------------ | ---------------------------------------------------------------------------------------------- |
| **icn-covm**       | Core CoVM runtime, Host ABI, CCL compiler, governance/economic opcodes                         |
| **icn-agoranet**   | Deliberation & proposal threading service, DAG message storage                                 |
| **icn-wallet**     | Mobile/desktop wallet for identity, tokens, credentials, and governance UX                     |
| **planetary-mesh** | Compute commons (identity-bound WASM task mesh) integrated as ICN shared execution layer       |
| **(dev tooling)**  | CLI, SDKs, docs generators, CI scripts (dev nodes are tooling wrappers, not core architecture) |

For the initial development phase, we're using a monorepo approach within the `icn-v3` repository.

### Crate Structure

The codebase is organized into the following high-level directories:

1. `crates/common/` - Core types and utilities shared across all components
   - `icn-types` - Core data structures (DAG, credentials, proofs)
   - `icn-crypto` - Cryptographic primitives and wrappers
   - `icn-identity` - DID and verifiable credential implementations

2. `crates/runtime/` - CoVM implementation
   - `covm-core` - Core VM implementation
   - `covm-host` - Host interface for CoVM
   - `covm-wasm` - WASM interpreter and runtime

3. `crates/ccl/` - Contract Chain Language
   - `ccl-compiler` - CCL to DSL compiler
   - `ccl-core` - CCL language primitives
   - `ccl-templates` - Standard governance templates

4. `crates/p2p/` - Networking layer
   - `icn-p2p-core` - libp2p mesh implementation
   - `icn-discovery` - Peer discovery mechanisms
   - `icn-transport` - Transport protocols

5. `crates/services/` - Network services
   - `agoranet` - Deliberation layer
   - `dag-sync` - DAG synchronization service
   - `federation` - Federation management

6. `crates/wallet/` - Wallet implementation
   - `wallet-core` - Core wallet functionality
   - `wallet-ui` - UI components
   - `wallet-mobile` - Mobile-specific code

7. `crates/tools/` - Development and diagnostic tools
   - `icn-cli` - Command-line interface
   - `icn-node` - Development node

### Development Workflows

The repository will use:
- GitHub Flow for pull requests and reviews
- Conventional Commits for commit messages
- Semantic versioning for releases
- ADRs (Architecture Decision Records) for significant design decisions
- RFCs for major feature proposals

## Implementation Plan

The implementation will follow the phased approach outlined in the development roadmap:

1. Phase 0: Project Genesis - Repository setup, toolchain configuration
2. Phase 1: Core Types & Cryptography - Implementation of foundational types
3. Phase 2: CoVM & CCL - Building the governance execution layer
4. Phase 3: AgoraNet & DAG - Implementing the deliberation and data layers
5. Phase 4: Wallet & UX - Developing the client interfaces
6. Phase 5: Federation & Network - Finalizing the federation layer

## Alternatives Considered

1. **Multiple repositories**: While this would provide cleaner separation, it would complicate development workflows and coordination during the initial development phase. We plan to move to a multi-repo structure once the core components stabilize.

2. **Language diversity**: While some components could benefit from languages other than Rust (e.g., TypeScript for web interfaces), the initial implementation will use Rust throughout for consistency and to leverage the WASM target for cross-platform compatibility.

## Impact and Risks

This structure establishes the foundation for all future development. The main risks include:

1. **Crate boundaries**: Improper boundaries could lead to circular dependencies
2. **Workspace organization**: Inefficient workspace setup could slow down build times
3. **Testing strategy**: Ensuring all layers can be tested independently and together

To mitigate these risks, we'll:
- Regularly review crate boundaries and dependencies
- Monitor build times and optimize workspace organization
- Establish comprehensive testing strategies at all levels 