# ADR-002: SI-backed typed quantities and boundary-only rounding

## Status
Accepted in Pass 2.

## Decision
The computational core uses distinct Rust types for length, force, stiffness, and
stress. Each quantity stores a coherent SI `f64` and serializes as an SI JSON number.
Unit conversions are explicit methods used by importers, UI, CLI, APIs, and reports.

The existing JSON field names (`xyz_m`, `force_n`, `stiffness_n_per_m`) remain
explicit about their wire units. This lets Pass 1 numeric payloads be read by the
Pass 2 domain types while compile-time dimensional checks improve internal code.

## Arithmetic
Only dimensionally valid operations are implemented. Examples:

- force / stiffness = length;
- stiffness × length = force;
- quantities of the same dimension may be added or subtracted.

Force cannot be added to length or stress. Compile-fail documentation tests guard
this contract.

## Validation
Trusted constants and internal arithmetic can use direct SI constructors. Untrusted
boundary values should use validated constructors that reject NaN and infinity.
Broader model validation belongs to Pass 3.

## Rounding
Solver inputs, state, convergence checks, and results are never rounded for display.
Rounding is explicit through a serializable `RoundingPolicy` at presentation/export
boundaries. A report must record the display unit and rounding policy it used. Run manifests
record the coherent computational unit system and any presentation rounding
applied to exported artifacts; raw solver artifacts remain unrounded.

## Consequences
This approach is intentionally smaller than a general symbolic unit algebra system.
Additional structural quantities will be introduced as the domain expands. Wire
schema changes remain versioned and require migration tests.
