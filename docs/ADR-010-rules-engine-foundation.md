# ADR-010: Versioned, data-driven rules engine foundation

- Status: Accepted
- Pass: 10

## Context

Engineering checks must identify the exact rule edition, national annex values,
interpretations, source clauses and analysis inputs used. Embedding jurisdictional
logic directly in solvers would make independent review, updates and reproducibility
difficult.

## Decision

Rule definitions are immutable, serializable packages behind `structural-rules-api`.
Each package declares its authority, standard edition, national annex, jurisdiction,
status, parameters, interpretation notes, applicability predicates and checks.

The reference engine evaluates explicit numeric limit checks. It performs no implicit
unit conversion. Every result carries clause references, resolved parameters,
interpretation-note identifiers and source artifact/JSON-pointer references. Package,
input and report hashes form deterministic evidence.

Packages contain reviewed rule data; they are not executable plugins. More expressive
formulae require a later typed expression language and independent validation.

## Consequences

- Solver output and rule evaluation remain separate and auditable.
- National annex values are data, not hard-coded branches.
- Missing context and units become explicit errors.
- Draft or demonstration packages can be tested without claiming regulatory approval.
- Production packages require governance, copyright review and competent-person approval.
