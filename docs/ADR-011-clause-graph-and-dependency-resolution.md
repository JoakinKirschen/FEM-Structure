# ADR-011: Clause graph and deterministic dependency resolution

- Status: Accepted
- Pass: 11

## Context

Pass 10 linked checks directly to flat clause references. Real design standards have
scope clauses, normative prerequisites, national-annex refinements, interpretations,
cross-references, and checks that depend on earlier checks. Flat references cannot
show the complete reasoning path or prevent a downstream check from running after a
mandatory prerequisite fails.

## Decision

Rule-package schema `0.2` adds:

- typed clause nodes and semantic links;
- explicit acyclic clause dependencies;
- rule entry points into the clause graph;
- explicit rule dependencies with `pass`, `pass_or_not_applicable`, or `completed`
  requirements;
- deterministic lexical topological ordering;
- a distinct `blocked` outcome;
- resolved transitive clause traces and dependency outcomes in every check;
- a dedicated clause-graph hash in reports and evidence.

Clause dependencies define resolution order. Semantic links describe relationships
for review and reporting but do not silently add execution dependencies.

## Consequences

- Circular and missing references fail package validation.
- Evaluation order is reproducible even if source rule arrays are reordered.
- A failed prerequisite cannot be confused with a failed downstream engineering check.
- Reports expose both direct clause entry points and the complete resolved clause set.
- Package authors must migrate schema `0.1` packages explicitly to `0.2`.
- Formula evaluation and parameter-expression graphs remain future work.
