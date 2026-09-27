# ADR-008 — Generalized finite-element mesh topology

## Status
Accepted.

## Context
Pass 7 exposed only triangular surface meshes. Beam, shell and solid solvers need
a common topology model without forcing every adapter to translate through
triangles. A production volume mesher is outside the current reference
implementation, but the platform boundary must be able to represent and assess
its output.

## Decision
Add a versioned `AnalysisMesh` contract alongside the compatible Pass 7
`TriangleMesh` contract. Elements explicitly declare one of seven first-order
topologies: line, triangle, quadrilateral, tetrahedron, pyramid, wedge or
hexahedron. Connectivity uses stable UUID node references.

Add a `GeneralMeshGenerator` boundary with line, surface and extruded-volume
targets. The reference implementation:

1. subdivides straight B-rep edges into conforming line elements;
2. preserves planar four-sided faces as quadrilateral elements;
3. falls back to deterministic triangular tessellation for other supported faces;
4. extrudes triangles to wedges and quadrilaterals to hexahedra in deterministic
   layers.

Quality evidence records counts by topology and per-element edge, aspect and
dimension-appropriate measure (length, area or volume). Invalid connectivity,
missing nodes and degenerate elements are errors.

## Consequences
Solver adapters can consume non-triangular topologies without depending on a
specific meshing vendor. The old surface API remains stable. The reference
extruder is deliberately not an arbitrary solid filler: tetrahedralization,
hex-dominant meshing, pyramid transitions, curved high-order elements and
Jacobian-based acceptance require future specialized implementations.
