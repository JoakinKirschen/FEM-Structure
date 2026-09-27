# ADR-015: Nonlinear solution protocol and reference spring solver

- **Status:** Accepted
- **Date:** 2026-09-27

## Context

The platform needs nonlinear load stepping, explicit convergence controls, material
and geometric nonlinearities, resumable runs, and sufficient diagnostics for review.
The existing `StructuralSolver` contract returns only a linear domain result and
cannot represent an incremental equilibrium path.

A full nonlinear frame/shell/solid finite-element implementation would require
element formulations, constraint handling, sparse tangent assembly, constitutive
integration and substantially broader verification. Treating such an implementation
as complete in one pass would be unsafe.

## Decision

1. Keep the existing linear solver contract backward compatible.
2. Add a separate, versioned `NonlinearStructuralSolver` contract.
3. Use deterministic load control and Newton-Raphson equilibrium iterations.
4. Define convergence with both residual-force and displacement-increment criteria.
5. Permit adaptive increment growth after easy steps and deterministic bisection
   cutback after failed steps.
6. Persist restart points only after converged steps. A restart is accepted only
   when schema, solver identity, analysis identity, DOF count/order and finite state
   all match.
7. Implement a reference solver for independent translational spring DOFs:
   - optional one-dimensional bilinear isotropic-hardening material response;
   - optional cubic force-displacement term for geometric nonlinearity;
   - consistent material and geometric tangents.
8. Terminate rather than cross a non-positive tangent. Arc-length continuation,
   snap-through, contact, coupled DOFs and general finite elements are out of scope.
9. Record every nonlinear iteration, including rejected attempts, in deterministic
   diagnostics. Emit step history, restart state and hashes as separate CLI artifacts.
10. Maintain analytical regression benchmarks for elastic, bilinear and cubic cases,
    plus restart equivalence and deterministic-repeat tests.

## Consequences

The platform gains a testable nonlinear execution and evidence model without
claiming general nonlinear FEM capability. Future solvers can implement the same
contract or introduce a compatible schema revision. Restart files are solver-specific
and must not be edited or transferred between different analyses or solver IDs.
