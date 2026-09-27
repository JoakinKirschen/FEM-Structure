# ADR-001: Modular monolith with optional services

## Status
Accepted for the initial implementation.

## Decision
Use Rust for the computational/domain core, with internal modules separated through
versioned contracts. Plan a C#/.NET desktop client and Python automation boundary.
Use optional services only for concerns that benefit from independent deployment:
collaboration, identity/licensing, ruleset distribution, marketplace operations,
and scalable remote execution.

## Why not microservices for the solver?
Finite-element workflows exchange large matrices, meshes, and result fields.
Splitting every capability across a network introduces serialization cost, latency,
more memory copies, distributed failure modes, and harder reproducibility.

## Boundary rule
A module becomes a service only after evidence shows a need for independent scaling,
security isolation, availability, ownership, or release cadence.
