# ADR-004: Geometry-kernel boundary

- Status: Accepted
- Pass: 4

## Context

IFC and STEP import require geometric operations, but exposing a CAD kernel's
native object model would couple the structural domain, plugins and stored
projects to one vendor and ABI. Geometry repair can also change engineering
evidence unless tolerances and actions are explicit.

## Decision

1. Keep geometry in separate `structural-geometry-api` contracts.
2. Reference topology by stable UUIDs rather than native pointers or handles.
3. Represent B-rep and triangle meshes as versioned, serializable artifacts.
4. Require explicit linear, angular and relative tolerances.
5. Return validation issues with stable codes, severity and entity identity.
6. Never heal silently: return a new document plus a complete healing report.
7. Preserve source format, units, adapter identity and source hashes as provenance.
8. Access kernels through the `GeometryKernel` trait. A future FFI or service
   adapter must implement the same behavioral contract.
9. Keep the structural-domain crate independent of all CAD-kernel crates.
10. Treat tessellation as a derived artifact, not as the authoritative B-rep.

## Consequences

- OpenCascade, Parasolid, ACIS or another implementation can be replaced without
  changing the structural model.
- Geometry artifacts can be hashed, compared and audited independently.
- Importers must perform explicit mapping and cannot leak opaque vendor objects.
- The initial reference kernel supports only validation, limited healing and
  planar polygon tessellation; it is not suitable for production CAD geometry.
