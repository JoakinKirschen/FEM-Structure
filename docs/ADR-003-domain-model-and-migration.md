# ADR-003: Versioned aggregate model and explicit migration

## Status
Accepted in Pass 3.

## Decision
The core domain is a versioned `AnalysisInput` containing a `StructuralModel`
aggregate. Entity references use stable UUIDs. Materials, sections, topology,
supports, loads, coordinate systems and provenance are independent records rather
than solver-owned objects.

The domain stays independent of IFC, STEP, geometry-kernel and FEM-library types.
Adapters in later passes map external representations into these contracts.

## Validation
Validation is performed before solving and reports stable machine-readable issue
codes. It checks duplicate identifiers, dangling references, coordinate-system
handedness, supported topology sizes, finite values and basic physical ranges.
These checks establish model integrity, not engineering adequacy.

## Migration
Legacy schemas are not silently deserialized as current data. A migration boundary:

1. reads the source schema marker;
2. maps legacy entities into schema 0.3;
3. creates deterministic UUID-v5 identifiers for generated entities;
4. records migration steps;
5. validates the migrated model;
6. leaves the original artifact unchanged.

Legacy scalar loads are explicitly mapped to the global X component because the
Pass 1/2 solver had only one translational degree of freedom.

## Consequences
Schema 0.3 is intentionally richer than the demonstrator solver. Unsupported
entities remain auditable domain data and must never be silently ignored by a
production solver. Capability declarations and stricter analysis applicability
checks will be added with the FEM implementation.
