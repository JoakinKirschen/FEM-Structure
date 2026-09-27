# ADR-005 — IFC import boundary and constrained reference adapter

## Status
Accepted for Pass 5.

## Context
IFC is a large, evolving schema and geometry can be represented through swept
solids, B-reps, booleans, mapped items, tessellations, curves, and vendor-specific
extensions. Treating every IFC entity as supported would create unsafe, silent
geometry loss. The core must also remain independent of one IFC toolkit.

## Decision
A format-neutral `GeometryImportAdapter` contract returns both a geometry
document and an evidence-rich import report. The Pass 5 IFC-SPF implementation
supports only a declared subset:

- IFC2X3/IFC4 header recognition;
- supported SI length units;
- Cartesian points and polylines;
- IFC4 Cartesian point lists and triangulated face sets;
- root identity and simple property-set traceability.

Target identifiers are deterministic UUIDv5 values. Every mapping stores its
source STEP id, type, optional GlobalId/name, source-record hash, optional
normalized source record, target ids, and extracted properties. Unsupported
geometry receives stable diagnostics and can be promoted from warning to error.

Malformed container syntax is a hard import failure. Entity-level mapping
problems remain in the returned report so partial results and evidence can be
inspected. The CLI writes artifacts before returning a non-zero result when
diagnostic errors exist.

## Consequences
A production-grade adapter can use IfcOpenShell, xBIM, or another certified/
validated toolkit without changing downstream contracts. The reference adapter
is auditable and useful for fixtures, but does not support placements,
representation-map transforms, swept solids, CSG, full B-rep topology, textures,
or structural-analysis-view semantics.

No import success claim implies engineering suitability. Users must review
diagnostics and imported geometry.
