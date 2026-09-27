# Collaboration example

Initialize an immutable repository:

```bash
cargo run -p structural-cli -- collab-init   examples/collaboration/model-snapshot.json   lead.engineer   ./collaboration-repository.json
```

Commit an object-level change:

```bash
cargo run -p structural-cli -- collab-commit   ./collaboration-repository.json main lead.engineer   examples/collaboration/move-node-changes.json   "Align grid A/1"   ./collaboration-repository-updated.json
```

Export a transport-neutral offline synchronization bundle:

```bash
cargo run -p structural-cli -- collab-sync-export   ./collaboration-repository-updated.json lead.engineer   examples/collaboration/known-revisions.json   ./collaboration-sync.json
```

The Rust API additionally supports branching, three-way object merges, explicit
conflict resolutions, review comments, role-based access checks, authenticated
XChaCha20-Poly1305 bundle encryption, and safe synchronization of divergent heads.
