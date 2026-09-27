# ADR-014: Signed out-of-process plugins and fail-closed sandboxing

## Status
Accepted for Pass 14.

## Context
In-process extensions inherit all host privileges and can corrupt analysis state.
Pass 13's restricted Python runner is useful defense in depth but is not a security
boundary for untrusted code.

## Decision
Plugins use the permissively licensed `structural-plugin-sdk` JSON contract and run
as separate processes. Installation and execution require an Ed25519 signature from
a key in an explicit trust store. Manifests declare API ranges, capabilities,
resource limits, and minimum sandbox strength.

The host negotiates the automation API, grants no undeclared capability, validates
all protocol messages, bounds stdout/stderr and time, detects abnormal exits, and
writes an evidence-rich audit record.

Linux uses bubblewrap as the reference OS sandbox. Automatic selection fails closed
when no acceptable sandbox exists. Direct process execution requires both a
`process` manifest minimum and an explicit unsafe-development opt-in; the production
CLI does not enable that opt-in.

## Consequences
- Plugin crashes do not crash the host.
- Trust and capability decisions are reviewable and auditable.
- A signed plugin is authenticated, not proven safe.
- Windows and macOS require reviewed native sandbox backends before untrusted
  plugins can run there.
- Revocation, package distribution and marketplace governance remain Pass 20 work.
