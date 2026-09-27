# ADR-019: Open, reproducible assurance portal

## Status

Accepted for Pass 19.

## Context

A solver certificate cannot establish that every model, element, norm interpretation,
platform or future release is correct. A digital signature establishes provenance and
integrity, not engineering validity. Early market assurance therefore needs inspectable
claims with tightly bounded scope rather than a broad approval label.

## Decision

The platform publishes a versioned assurance catalog containing:

- immutable benchmark inputs and reference artifacts;
- explicit numeric acceptance ranges;
- a solver/version/platform execution matrix, including `not_run` rows;
- known limitations and workarounds;
- requirement-to-benchmark-to-evidence traceability;
- external report metadata with an explicit verification state.

Every downloadable artifact is SHA-256 addressed. Portal generation re-hashes source
files and fails closed on missing or modified evidence. The generated bundle stores
artifacts by digest and includes a manifest hash over the catalog identity and artifact
inventory.

Independent reports are not silently treated as certifications. `signature_verified`
is reserved and rejected until a detached-signature and trust-policy profile is
implemented. The first example is visibly marked `illustrative_unverified`.

## Consequences

Evidence can be mirrored, downloaded and independently checked without trusting a web
application. Failed and unexecuted matrix entries remain visible. Publication is
repeatable from source-controlled inputs.

SHA-256 integrity does not prove the truth of a report, the competence of its author,
or the suitability of the software for a project. Formal assessment remains scoped to
named versions, configurations and claims.
