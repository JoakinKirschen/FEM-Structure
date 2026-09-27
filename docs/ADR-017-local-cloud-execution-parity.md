# ADR-017 — Common local/cloud execution and parity evidence

## Status
Accepted for Pass 17.

## Context
Local and cloud analyses must not become separate products with incompatible job
formats or unverifiable result claims. Network boundaries also introduce retries,
partial transfers, cancellation races and infrastructure metadata that do not exist
in a purely local call.

## Decision
Use one transport-neutral `ExecutionEnvelope` for local and remote workers. Inputs
and outputs are immutable content-addressed artifacts identified by SHA-256 and byte
length. The envelope records operation, parameters, requested resources, retry
policy, cancellation identity, deterministic profile, random seed and the requested
toolchain/container identity.

Cloud executors return an Ed25519-signed attestation over the envelope digest,
artifact descriptors, actual toolchain, region and timing. Trust is established by
an explicit key-ID trust store. Signatures prove origin and integrity; they do not
prove numerical correctness.

Parity is evaluated separately. JSON numbers use declared absolute and relative
tolerances:

`|local - cloud| <= absolute + relative * max(|local|, |cloud|)`

Non-numeric values remain exact unless their JSON pointers are explicitly ignored.
Comparison reports retain both artifact hashes and every out-of-tolerance path.

## Boundaries
- This pass supplies a filesystem content-addressed store and a `RemoteExecutor`
  trait, not a production HTTP/object-storage implementation.
- Retries apply only to failures explicitly marked retryable. Fatal errors stop
  immediately. Cancellation is checked before calls and during backoff.
- Toolchain identity includes optional executable, dependency-lock and container
  hashes. A production cloud scheduler must enforce, not merely record, these fields.
- Cross-platform bitwise equality is not promised.
- Secret-key custody, certificate rotation, timestamping and WORM retention remain
  deployment responsibilities.

## Consequences
The same job can be executed locally or submitted through a future cloud adapter
without changing its evidence contract. A valid attestation and a passing parity
report are independent requirements and can be audited independently.
