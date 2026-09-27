# ADR-013: Stable local automation API and restricted Python scripting

## Status
Accepted for Pass 13.

## Context
The desktop shell and external engineering workflows need a stable integration
boundary. Calling ad-hoc CLI commands or linking implementation crates directly
would couple clients to internal data structures and make compatibility, audit,
cancellation and permissions difficult to govern.

## Decision
Introduce a transport-neutral JSON contract in `structural-automation-api`, a
reference dispatcher in `structural-automation-host`, and a local executable with
file and JSON-lines transports.

API version `1.0` exposes four initial operations:

- `system.describe`;
- `artifact.sha256`;
- `model.validate`;
- `rules.evaluate`.

Every request declares capabilities. The host independently maps each method to
its required capability and rejects missing grants. Paths are not accepted by the
v1 operations; callers pass JSON values or text, preventing ambient host-file access.

Python automation is a two-phase planner. A restricted Python process receives
JSON context, emits a value, and queues API operations. The Rust host then validates
and dispatches each operation. The runner uses Python isolated mode, clears the
environment, removes imports and dangerous builtins, creates a private temporary
working directory, and enforces timeout, script-size, output-size and operation-count
limits. Cancellation kills the child process and stops dispatch.

This language-level restriction is **not** a security boundary for hostile code.
Untrusted third-party scripts require the OS/container isolation planned for Pass 14.

Compatibility follows semantic versioning at the wire boundary:

- compatible optional additions retain major version 1;
- removals, renamed fields or changed meanings require major version 2;
- clients discover versions and operations through `system.describe`;
- checked-in v1 fixtures and generated-binding tests protect the contract.

## Consequences
Desktop, CI and Python clients can automate validated operations without importing
Rust crates. The initial API is intentionally small. Long-running mesh and solver
jobs should be added later as asynchronous job resources rather than blocking v1 calls.
