# RFC 0017: Token Flow and Treasury Interaction

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0010 (Mana), RFC 0013 (Economics Engine), RFC 0015 (Policy Enforcer)

---

## 0. Abstract

This RFC defines the flow of tokenized value within ICN — including transfer, distribution, treasury interactions, and cooperative-level accounting. It introduces a flexible ledger abstraction, scoped treasury logic, and outlines how value movement is recorded and policy-constrained.

---

## 1. Introduction

ICN supports a range of token types to facilitate incentives, value distribution, and mutual credit among federated cooperatives. Unlike traditional cryptocurrencies, these tokens:

* May be scoped to specific cooperatives or communities
* Are managed via cooperative-defined policies
* Are non-speculative and role-aligned

This RFC documents the flow, representation, and treasury roles for tokenized systems such as:

* **ICN-F** (federation credits)
* **ICN-R** (reputation-linked dividends)
* **Coop Credits** (local currencies)

---

## 2. Terminology

* **Token Transfer** – Movement of fungible value between scoped DIDs or cooperatives
* **LedgerKey** – Scoped resource identifier for balances
* **Treasury** – A managed account with mint/burn authority and redistribution logic
* **FlowEvent** – A single atomic value movement, tracked in the ledger

---

## 3. Token Types

### 3.1 Federation Tokens (ICN-F)

* Issued by federations
* Used for inter-coop contribution and reward
* Backed by policy and treasury logic

### 3.2 Reward Tokens (ICN-R)

* Linked to reputation achievements
* May be used for governance staking or dividends

### 3.3 Local Credits

* Community/coop-specific units
* Enable timebanking, mutual aid, internal accounting

---

## 4. Transfer Mechanism

Token flows are performed via ledger API or runtime:

```rust
fn transfer(from: &LedgerKey, to: &LedgerKey, amount: u64) -> Result<()>;
```

All transfers:

* Must be authorized by the policy enforcer
* Emit `FlowEvent` records
* May be linked to job execution, governance votes, or proposal triggers

---

## 5. Treasury Model

Treasuries are scoped per cooperative or federation:

```rust
pub struct Treasury {
    pub scope: ScopeKey,
    pub authorized_mint: Vec<Did>,
    pub distribution_policy: Option<DistributionRule>,
}
```

Treasuries may:

* Mint tokens in response to proposals
* Burn tokens as reputation slashing
* Trigger periodic redistribution (e.g. dividend or surplus share)

---

## 6. Policy Enforcement

All token flows pass through the `ResourcePolicyEnforcer`:

* Maximum transfer limits
* Role-based permissions
* Time-locked transactions (e.g. vesting)
* Validations via host ABI or proposal runtime

---

## 7. Runtime Integration

CCL contracts can:

* Mint/burn scoped tokens (if authorized)
* Initiate transfers within job logic
* Query balances via host ABI (planned)

Treasury behavior may be invoked:

* On proposal success
* Via governance triggers
* As reward hooks from execution receipts

---

## 8. Ledger Interface

```rust
pub trait TokenLedger {
    fn get_balance(&self, key: &LedgerKey) -> Result<u64>;
    fn transfer(&self, from: &LedgerKey, to: &LedgerKey, amount: u64) -> Result<()>;
    fn mint(&self, key: &LedgerKey, amount: u64) -> Result<()>;
    fn burn(&self, key: &LedgerKey, amount: u64) -> Result<()>;
}
```

---

## 9. Observability

Transfers and treasury events emit:

* `token_flow_total{token="icn-f"}`
* `token_balance{scope=..., resource=...}`
* `treasury_distribution_events{policy=...}`

---

## 10. Rationale and Alternatives

This design supports modular, federated economic value without relying on speculative external assets. Cooperative and community tokens increase autonomy and trust alignment.

Alternatives like smart contract-based treasuries or global tokens were rejected due to rigidity and trust centralization.

---

## 11. Backward Compatibility

This RFC documents primitives present in the economics engine, ledger adapters, and scoped policy enforcement already implemented.

---

## 12. Open Questions and Future Work

* DAO-style treasury proposal workflows?
* Cross-scope token bridges (federation ↔ community)?
* Integration with ICN wallet UI?

---

## 13. Acknowledgements

Thanks to contributors building scoped economic logic, policy enforcement systems, and early coop credit prototypes.

---

## 14. References

* \[RFC 0013: Economics Engine and Resource Types]
* \[RFC 0015: Policy Enforcer]
* \[RFC 0033: Voting and Threshold Definitions (planned)]

---

**Filename:** `0017-token-flow-and-treasury-interaction.md`
