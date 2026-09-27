# ADR-007 — Mesh generation and quality boundary

## Status
Accepted.

## Context
Imported CAD tessellation is not automatically an analysis-quality mesh. The
solver must not own CAD meshing policy, and quality acceptance must be explicit
and reproducible.

## Decision
Introduce a separate `SurfaceMeshGenerator` interface and versioned quality
report. The reference generator produces deterministic triangular surface
meshes and conforming uniform refinement. Mandatory topology/edge failures are
errors; shape criteria such as minimum angle and aspect ratio are warnings
until an analysis profile promotes them.

Every run records options, implementation version, refinement depth, aggregate
metrics and triangle-level issue identifiers.

## Consequences
The reference implementation is intentionally simple and potentially
inefficient. Production meshing engines remain replaceable. Volume meshing,
analysis-element conversion, local sizing fields and geometry-to-load
imprinting are outside this pass.
