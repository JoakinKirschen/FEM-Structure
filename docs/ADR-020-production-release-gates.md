# ADR-020 — Production hardening and release gates

## Decision

Pass 20 introduces a fail-closed, machine-readable release candidate and evidence gate.
A release is a set of immutable, hash-addressed artifacts. Required controls cover threat
modeling, performance, fuzzing, backup/restore, migration, SBOM/provenance, incident
response, support, marketplace governance, privacy and pilot acceptance.

Stable releases additionally require a trusted Ed25519 signature. Signing keys belong in
an HSM/KMS; the repository contains verification contracts, not production private keys.

## Limits

A passing gate proves that declared evidence is present, internally consistent and
untampered. It does not prove solver correctness, regulatory compliance, absence of
vulnerabilities, successful pilot deployment, or fitness for structural design.
