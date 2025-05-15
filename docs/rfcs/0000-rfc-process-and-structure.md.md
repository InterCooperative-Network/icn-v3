# RFC 0000: ICN RFC Process and Structure

**Status:** Proposed
**Author:** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —

---

## 0. Abstract

This RFC defines the structure, lifecycle, and purpose of the ICN RFC (Request for Comments) process. It serves as the canonical reference for how cooperative stakeholders can propose, deliberate, ratify, and track major changes to the InterCooperative Network (ICN). It formalizes a transparent, collaborative process that ensures alignment with ICN's principles: decentralization, cooperation, modularity, and long-term resilience.

---

## 1. Introduction

The InterCooperative Network (ICN) is an expansive federated system comprising compute mesh infrastructure, cooperative contract governance (CCL), economic engines (Mana, NFR, AK, G), and a reputation-driven execution pipeline. Given its modular architecture and evolving mission, the community needs a consistent, accessible mechanism for proposing and evaluating changes—across governance, economics, protocol design, system architecture, and implementation strategy.

RFCs are the formal vehicle for these proposals. This document outlines how they are authored, reviewed, and maintained.

---

## 2. Purpose of the RFC Process

The RFC process provides:

* **A shared protocol for proposals.** It ensures that technical and governance changes are introduced in a consistent, interpretable way.
* **Institutional memory.** All critical changes and their rationale are recorded.
* **Community deliberation.** RFCs open space for structured discussion, feedback, and consensus.
* **Federation-wide coordination.** RFCs standardize features and policies across independently operated cooperatives and communities.

RFCs may address architectural choices, CCL semantics, mana regeneration models, wallet trust primitives, reputation policy, federation governance, DAG topology, or any major component of ICN.

---

## 3. RFC Numbering

RFCs are assigned unique four-digit numbers by the RFC Editors upon submission:

* Start at `0001`. RFC `0000` is reserved for this document.
* Numbers are not reused, even if an RFC is rejected or withdrawn.
* File naming format: `####-title.md`, e.g. `0016-mesh-execution-pipeline.md`.

---

## 4. RFC Lifecycle and Status Codes

RFCs may adopt the following status labels:

* **Proposed** – Submitted but not yet reviewed.
* **Draft** – Being actively revised based on feedback.
* **Review** – Stable and open for final community input.
* **Last Call** – In final decision window.
* **Accepted** – Approved by the RFC Editors (in coordination with governance if necessary).
* **Active** – Currently implemented or governing ICN behavior.
* **Implemented** – Technically realized in code.
* **Rejected** – Declined with justification.
* **Withdrawn** – Retracted by authors.
* **Obsolete** – Superseded by a newer RFC.
* **Informational** – Non-normative, for reference.
* **Experimental** – For trials or limited deployments.

Status is declared at the top of the RFC document and maintained in version control.

---

## 5. RFC Template

Each RFC should adhere to this format:

```markdown
# RFC ####: [Title]

**Status:** [e.g. Draft]  
**Author(s):** [Name(s), optional contact]  
**Date:** [YYYY-MM-DD]  
**Version:** [Major.Minor]  
**Replaces:** [RFC #, if applicable]  
**Replaced By:** [RFC #, if applicable]  
**Related To:** [RFC #s or documents]  

## 0. Abstract
## 1. Introduction
## 2. Terminology (if needed)
## 3. Proposal / Specification
## 4. Rationale & Alternatives
## 5. Backward Compatibility
## 6. Security Considerations
## 7. Privacy Considerations
## 8. Economic Impact
## 9. Implementation Plan
## 10. Open Questions / Future Work
## 11. Acknowledgements
## 12. References
```

---

## 6. Submission and Review Process

1. **Drafting**: Author drafts RFC using the above template.
2. **Submission**: A pull request adds the RFC to `docs/rfcs/`.
3. **Number Assignment**: RFC Editors assign a number.
4. **Discussion**: Community feedback is collected in PR comments or designated channels (e.g., forum, Discord, federation meeting).
5. **Iteration**: Author refines RFC. Editors may upgrade status to `Draft`, `Review`, or `Last Call`.
6. **Approval**: RFC is either `Accepted`, `Rejected`, or `Withdrawn`. If accepted, implementation may begin.

---

## 7. Roles and Responsibilities

* **RFC Authors** – Draft the proposal, respond to discussion, and revise accordingly.
* **RFC Editors** – Maintain numbering, status, and quality control. Coordinate with ICN governance when proposals affect protocol-level behavior.
* **ICN Community** – Reviews, discusses, and builds consensus.
* **ICN Governance (via CCL or federation roles)** – May be required to formally ratify policy-altering RFCs.

---

## 8. RFC Scope

RFCs should be used for:

* Protocol changes (mesh protocol, DAG propagation, host ABI).
* Governance policy (proposal lifecycles, quorum logic).
* Economics (mana dynamics, token standards, incentive design).
* Identity and trust systems (credential formats, endorsement flows).
* System architecture (service topology, interface design).
* Observability standards (metrics, logs, dashboards).
* Implementation guidelines or contract language conventions.

They are **not required** for trivial implementation details, bugfixes, or UI tweaks.

---

## 9. RFC Evolution

Any change to this RFC process itself must be proposed as a new RFC. RFC 0000 may be marked `Obsolete` and replaced.

---

## 10. Conclusion

This document defines the foundation of deliberation in the ICN. It ensures that as ICN evolves—across its federated runtime, reputation infrastructure, cooperative economic models, decentralized proposals, and DAG-anchored trust layer—the community has a transparent, shared mechanism for thinking, discussing, and deciding together.

By writing things down, we remember what matters.
