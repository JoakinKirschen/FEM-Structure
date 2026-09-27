# ADR-006 — STEP import boundary

## Status
Accepted for Pass 6 continuity work included in Pass 7.

## Decision
Implement STEP as an adapter behind `GeometryImportAdapter`. The reference
adapter supports an explicit polygonal planar subset and emits diagnostics for
unsupported representations. It converts source lengths to internal SI metres,
uses deterministic identifiers, and preserves source hashes and entity maps.

## Rationale
A bounded adapter avoids coupling the domain to a CAD kernel or silently
approximating advanced STEP geometry. A production OpenCascade or commercial
adapter can later replace it without changing the domain contract.

## Consequences
Curved surfaces, trimmed curves, manifold solid B-reps and assemblies require a
future production adapter. Unsupported entities remain visible in audit data.
