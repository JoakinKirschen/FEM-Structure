# ADR-016 — Modal, buckling and dynamic analysis boundary

## Status
Accepted for Pass 16.

## Decision
Expose versioned dense-matrix contracts in `structural-solver-api` and place the
deterministic reference algorithms in a separate `structural-solver-dynamics` crate.
The contract supports modal extraction, linearized buckling and sampled-force
time-history integration without coupling callers to a future sparse backend.

Generalized symmetric eigenproblems are transformed with a Cholesky factor and
solved with a deterministic Jacobi rotation sequence. Modal vectors can be
mass-normalized or normalized by their largest absolute component. Time histories
use the Newmark family and explicitly record integration and equilibrium tolerances.

## Constraints
- Mass matrices must be symmetric positive definite.
- The Pass 16 reference buckling denominator matrix must also be symmetric positive
  definite; production signed/indefinite geometric stiffness formulations require a
  different eigensolver.
- Dense storage is limited to benchmarks and small verification models.
- Nonlinear dynamics, response spectra, seismic base motion and mode superposition
  are deferred.
- Floating-point equivalence is tolerance-based, not promised to be bitwise across
  different future backends.

## Consequences
Sparse CPU/GPU or vendor solvers can implement `DynamicStructuralSolver` while
retaining the same evidence model. Every result contains the solver identity,
normalization policy and tolerance declaration required for audit.
