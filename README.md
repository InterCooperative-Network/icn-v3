# InterCooperative Network – v3

## Architecture Stack

The InterCooperative Network is built as a layered architecture designed to replace extractive capitalist and statist infrastructures with democratic, cooperative alternatives:

1. **Identity & Trust Layer**
   - DIDs (Ed25519 `did:key`) for every scope
   - Verifiable Credentials for roles, receipts, anchors
   - TrustBundles + QuorumProof types
   - LineageAttestations

2. **Governance Execution Layer**
   - Cooperative Virtual Machine (CoVM) – a lean, WASM-first runtime
   - Contract Chain Language (CCL) – declarative governance templates
   - `.dsl` programs compiled from `.ccl` and executed in CoVM

3. **Deliberation Layer – AgoraNet**
   - Threaded discussions feed directly into proposal lifecycles
   - Stores messages as DAG-linked objects

4. **Data & State Layer**
   - Versioned DAG Ledger – append-only, Merkle-rooted
   - Scoped redundancy
   - DAG checkpoints embedded in AnchorCredentials

5. **Networking Layer**
   - libp2p mesh (TCP/Noise/Yamux), IPv6-first with IPv4 fallback
   - Every node can act as a bootstrap peer

6. **Client Layer – ICN Wallet**
   - Mobile-first agent (Rust/WASM core)
   - Offline-first DAG caching & queued signatures

## Documentation

See the [RFC index](./rfcs/README.md) for detailed design decisions and project structure.

Key system documentation:
- [ICN Runtime Observability Guide](./docs/observability.md) - Monitoring, dashboards, and alerts.
- [ICN Reputation System Integration](./docs/reputation.md) - How execution outcomes translate to reputation.

## Services

- **AgoraNet API** — Threaded deliberation & governance endpoints ([docs](docs/agoranet_api.md))

## Federation Test Deployment

To deploy a fully-instrumented federation test environment:

```bash
cd devnet
./deploy.sh
```

This will:
- Launch multiple federation nodes
- Set up a PostgreSQL database for each node
- Deploy the complete monitoring stack (Prometheus + Grafana)
- Generate test load scripts for simulation

Access points:
- Federation API: http://localhost:8080
- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000 (login: admin/admin)

See [Federation Test Deployment](devnet/README.md) for more details.
