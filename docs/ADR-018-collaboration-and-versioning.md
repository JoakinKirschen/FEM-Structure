# ADR-018 — Immutable collaboration and offline synchronization

## Status

Accepted for Pass 18.

## Context

Engineering models must remain usable offline while allowing multiple engineers to
develop alternatives, review changes, and synchronize later. Replacing a model file
with a last-writer-wins upload would lose design intent and is unacceptable for an
auditable structural workflow.

## Decision

1. A repository stores content-hashed, immutable model revisions.
2. A revision contains a complete deterministic snapshot and one or two parent IDs.
3. Mutable branch names point to immutable revisions.
4. Commits use optimistic concurrency through an explicit expected branch head.
5. Merges compare stable domain-object UUIDs against a common ancestor.
6. Independent object edits merge automatically; concurrent edits to the same object
   require an explicit target/source/delete/custom resolution.
7. Comments and review states reference immutable revision and optional object IDs.
8. Repository actions are guarded by viewer, commenter, editor, maintainer and owner roles.
9. Offline bundles are hash-protected. Divergent branch heads are reported and never
   silently overwritten.
10. Sensitive bundles may be authenticated and encrypted with XChaCha20-Poly1305.
    Key generation, escrow, rotation, revocation and hardware-backed custody belong to
    deployment policy rather than this domain crate.

## Consequences

The complete-snapshot reference representation is simple to audit but not storage
optimal. Production persistence may use deltas or content-addressed chunks provided
that the externally visible revision and snapshot hashes remain identical.

This pass does not claim real-time CRDT editing. It provides safe offline-first
branching and deterministic reconciliation; presence, websocket transport and live
cursor exchange can be layered above it.
