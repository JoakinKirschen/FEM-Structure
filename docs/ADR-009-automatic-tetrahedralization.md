# ADR-009: Automatic tetrahedralization behind a backend-neutral boundary

- Status: Accepted
- Pass: 9

## Context

Pass 8 standardized tetrahedral topology but could only create volumes by
extruding surface meshes. Automatic filling of closed solids requires stronger
boundary validation, quality evidence and replaceable geometry algorithms.

## Decision

`mesh-api` owns a `VolumeMeshGenerator` contract and serializable options,
reports, mappings and backend identity. Implementations consume a geometry
document whose B-rep can be tessellated or which already contains triangular
boundary meshes.

The dependency-free reference implementation uses a validated centroid fan and
deterministic 1-to-4 volume refinement. It is restricted to connected convex or
star-shaped solids. Production adapters may use constrained-Delaunay engines,
but must return the same platform contracts and evidence.

Invalid/open/non-manifold inputs fail explicitly. Cavities and multiple material
regions are rejected by the reference backend rather than silently ignored.

## Consequences

- Solver and reporting layers remain independent of native meshing libraries.
- Boundary/source provenance survives tetrahedralization.
- Backend licensing and deployment can vary without changing domain contracts.
- The reference backend is deterministic and testable, but is not a universal
  CAD tetrahedralizer.
