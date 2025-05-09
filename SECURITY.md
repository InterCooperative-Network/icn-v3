# Security Policy

## Reporting a Vulnerability

The InterCooperative Network (ICN) takes security seriously. We appreciate the community's efforts in identifying and responsibly disclosing potential security vulnerabilities.

### How to Report

If you believe you've found a security vulnerability in the ICN codebase, please follow these steps:

1. **Do not disclose the vulnerability publicly** or to any third parties.
2. Submit details of the vulnerability directly to our security team via:
   - Email: security@intercooperative.network
   - Or via secure channels listed on our website

### What to Include

When reporting a vulnerability, please include:

* Description of the vulnerability
* Steps to reproduce
* Potential impact
* Any ideas for mitigation

### Security Principles

ICN's security model is built on these core principles:

1. **Cryptographic Provability**: All governance, economic, and identity events are cryptographically provable and forever replayable.
2. **Append-only DAG**: With Merkle roots and CID addressing guarantees immutability.
3. **Credential Lineage**: Combined with quorum attestation to prevent unauthorized forks.
4. **Redundant Storage**: Policies including quorum-verified caching and mutual pinning to ensure availability.

### Security Review Process

All code undergoes:
1. Peer review
2. Automated static analysis
3. Regular security audits
4. Cryptographic verification of critical components

## Vulnerability Disclosure Timeline

* **0 day**: Initial report received
* **1-2 days**: Acknowledgment of report
* **7-14 days**: Initial assessment completed
* **30-90 days**: Fix developed and tested (depending on severity)
* **Public disclosure**: After fix is deployed, with appropriate credit to discoverer

Thank you for helping keep the InterCooperative Network secure. 