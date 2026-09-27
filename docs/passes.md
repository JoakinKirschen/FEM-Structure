# Consolidated development passes

This file is the single source of truth for historical pass plans, migrations, validation notes, and local validation evidence.

> **Documentation rule:** Do not create new `pass-N-*.md` or `pass-N-local-validation.json` files. Add future pass information to this file under the appropriate pass heading.

## Overall 20-pass plan

### 20-pass implementation plan

Each pass must leave the repository buildable, tested, documented, and migratable.

#### 1. Foundation — included in this starter
Create the Rust workspace, domain envelopes, solver contract, demonstrator solver,
audit hashing, CLI, tests, and architecture decisions.

#### 2. Units and dimensional safety — implemented
Introduced SI-based quantity types, unit conversion at boundaries, an explicit
boundary-only rounding policy, serialization tests, and compile-fail protection
against force/length/stress mix-ups.

#### 3. Structural domain model — implemented
Added nodes, members, shells, solids, materials, sections, load cases, combinations,
supports, coordinate systems, provenance, validation, and deterministic schema migration.

#### 4. Geometry kernel abstraction — implemented
Defined vendor-neutral geometry/B-rep/mesh contracts, explicit tolerances,
UUID-based topology, validation reports, deterministic healing records, geometry
provenance, and a small planar reference tessellator.

#### 5. IFC import adapter — implemented
Added a reusable import contract and a constrained IFC-SPF reference adapter with
schema/unit detection, point/polyline and triangulated-face-set mapping,
property/source traceability, deterministic round-trip identifiers, explicit
unsupported-geometry diagnostics, evidence hashes, and conformance fixtures.

#### 6. STEP import adapter — implemented in the Pass 7 workspace
Added constrained AP203/AP214/AP242 STEP-SPF ingestion behind the shared adapter
contract, SI unit conversion, polygonal planar-face mapping, deterministic source
traceability, unsupported-geometry diagnostics, and a conformance fixture.

#### 7. Meshing — implemented
Added versioned surface-meshing contracts, deterministic conforming refinement,
edge/area/angle/aspect metrics, issue codes, quality reports, evidence hashes,
tests, and CLI artifact generation.

#### 8. Generalized analysis meshing — implemented
Expanded the mesh boundary beyond triangles with line and quadrilateral surface
elements, wedge/hexahedral layered extrusion, common first-order volume topology,
dimension-aware quality evidence, deterministic identifiers, tests and CLI artifacts.

#### 9. Automatic tetrahedralization — implemented
Added a backend-neutral volume-meshing contract, closed-manifold boundary checks,
a deterministic convex/star-shaped reference tetrahedralizer, volume refinement,
tetrahedral quality metrics, provenance mappings, evidence and CLI integration.

The previously planned linear-static FEM expansion moves to the next solver-focused
increment; this roadmap amendment was requested to prioritize arbitrary-solid meshing.

#### 10. Norm/rules engine foundation — implemented
Added versioned rule-package contracts, national-annex parameters, interpretation
notes, applicability predicates, deterministic scalar limit checks, clause/input
traceability, package/report evidence hashes, fixtures, tests and CLI integration.

#### 11. Clause graph and dependency resolution — implemented
Expanded rule packages with typed clause nodes, semantic links, transitive clause
traceability, acyclic inter-rule dependencies, deterministic topological evaluation,
blocked outcomes, graph hashes, migration guidance and validation tests.

The previously planned reporting increment moves to Pass 12; later roadmap pass
numbers shift by one unless explicitly reprioritized.

#### 12. Desktop GUI — implemented
Added a C#/.NET 8 desktop solution with a platform-neutral interaction core and a
Windows WPF shell: deterministic JSON model tree, built-in 3-D viewport, undo/redo,
drag-and-drop loads/supports/materials, searchable command palette, cancellable
background CLI jobs, sidecar assignment persistence, tests and migration guidance.

#### 13. Stable automation API — implemented
Added a versioned JSON local API, one-shot and JSON-lines transports, generated
dependency-free Python bindings, capability-gated operations, cooperative
cancellation, restricted Python planning, resource limits, compatibility fixtures,
tests, evidence hashes, migration guidance and an explicit sandbox boundary.

#### 14. Plugin SDK and sandbox — implemented
Added a permissively licensed out-of-process SDK and example, Ed25519-signed
capability manifests, trust stores, bounded API negotiation, fail-closed sandbox
selection, Linux bubblewrap isolation, crash/timeout containment, and audit records.

#### 15. Nonlinear analysis — implemented
Added a versioned nonlinear solver contract, deterministic adaptive load stepping,
dual convergence criteria, bilinear material and cubic geometric nonlinearity,
converged-state restart points, full iteration diagnostics, evidence artifacts and
analytical regression benchmarks.

#### 16. Modal, buckling, and dynamic analysis — implemented
Added generalized symmetric eigenvalue extraction, mass/maximum-component mode
normalization, linearized reference buckling, Newmark time integration, multiple
damping models, explicit solver tolerances, CLI evidence, tests and benchmarks.

#### 17. Local/cloud execution parity — implemented
Added a common execution envelope, content-addressed artifact transfer,
container/toolchain identity, Ed25519-signed cloud attestations, retry/cancellation,
and tolerance-based structured result comparison.

#### 18. Collaboration and versioning — implemented
Use immutable model revisions, branching/merging at domain-object level, conflict
handling, comments/reviews, access control, encryption, and offline synchronization.

#### 19. Assurance and validation portal — implemented
Added hash-addressed benchmark and reference artifacts, explicit expected ranges,
solver/version/platform matrices, known limitations, traceability, external-report
verification states, downloadable content-addressed evidence bundles, and a static
portal generator. The example deliberately records unavailable execution as `not_run`
and labels its external-report fixture as illustrative and unverified.

#### 20. Production hardening and release — started
Added machine-readable, fail-closed release gates; content verification; stable-channel
signature verification; SBOM/provenance requirements; privacy-safe telemetry policy;
marketplace safeguards; threat-model, performance, fuzzing, backup/restore, migration,
incident-response, support and pilot evidence contracts; CI/fuzz scaffolding; examples,
runbooks, tests and migration guidance. Real profiling campaigns, restore drills,
external security review, HSM/KMS signing and selected-firm pilots remain deployment work.

## Pass 2

### Validation notes

#### Pass 2 validation record

##### Implemented checks

- Unit conversion unit tests for mm, kN, kN/m, and MPa.
- Typed Hooke-law tests (`Force / Stiffness -> Length`).
- A compile-fail documentation test for adding force to length.
- Rounding-policy tests.
- Non-finite boundary validation tests.
- SI serialization tests.
- A Pass 1 JSON compatibility fixture and deserialization test.
- Solver regression tests using typed quantities.

##### Execution status in the generation environment

The generation environment did not contain `cargo`, `rustc`, or `rustfmt`.
Therefore the Rust test suite and formatter could not be executed here. Run the
following in CI or on a development workstation before accepting the pass:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- demo-run ./out
```

No claim of a successful compiled test run is made by this record.


## Pass 3

### Validation notes

#### Pass 3 validation record

##### Intended checks

- domain unit tests for valid and invalid coordinate systems;
- dangling-reference detection for structural entities;
- material, section, shell, spring, support and load-value checks;
- deterministic Pass 1/2-to-Pass 3 migration;
- preservation of SI values during migration;
- solver regression using a Pass 3 load case;
- CLI generation of schema-0.3 input, result and run-manifest artifacts.

##### Important limitation

The generation environment did not contain `cargo`, `rustc`, or `rustfmt`.
Compilation, formatting and tests could therefore not be executed here.

Run before accepting the pass:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- demo-run ./out
```

No claim of a successful compiled test run is made by this record.


## Pass 4

### Validation notes

#### Pass 4 validation record

##### Scope

Pass 4 introduces geometry contracts and a deterministic reference kernel. It
does not claim CAD-kernel equivalence or production geometric robustness.

##### Automated checks included

- valid planar triangular B-rep validation;
- deterministic tessellation to one triangle;
- disconnected-wire detection;
- coincident-vertex healing;
- healing action recording;
- unresolved degenerate-edge reporting;
- existing domain migration and solver regression tests.

##### Manual acceptance commands

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- geometry-demo ./geometry-out
```

Confirm that all five geometry artifacts are created and that the validation
report contains no errors for the generated triangle.

##### Environment limitation

Cargo, rustc and rustfmt were not available in the generation environment.
The repository was therefore inspected structurally, but compilation, formatting
and execution must be performed in a Rust-enabled CI or developer environment.

##### Known limitations

- reference tessellation supports planar faces with one outer wire only;
- curved surfaces, openings and sewing are contracts only;
- healing merges coincident vertices but deliberately leaves resulting
  degenerate edges as explicit unresolved issues;
- no IFC or STEP parser is included yet;
- no structural analysis is performed from B-rep or mesh geometry.


## Pass 5

### Validation notes

#### Pass 5 validation record

##### Scope
IFC-SPF parsing, unit conversion, deterministic geometry identifiers,
triangulated geometry, property traceability, diagnostics, and evidence output.

##### Automated checks included

- parser handles semicolons inside IFC strings;
- top-level attribute splitting preserves nested aggregate commas;
- deterministic UUID generation;
- IFC4 fixture schema recognition;
- millimetre-to-metre conversion;
- triangulated-face-set mapping;
- GlobalId and name retention;
- `IfcPropertySingleValue` property-set extraction;
- repeated imports produce identical documents and reports.

##### Manual/CI commands

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- ifc-import \
  crates/ifc-adapter/tests/fixtures/ifc4-triangle.ifc ./ifc-out
```

Inspect `ifc-import-report.json` and verify that no error diagnostics are present.

##### Environment limitation
Cargo, Rustc and Rustfmt were not available in the generation environment.
Compilation, formatting and test execution must therefore be performed in a
Rust-enabled workstation or CI runner.

##### Known limitations
This pass is not a complete IFC implementation. Placements, transformations,
swept solids, CSG, advanced/faceted B-reps, mapped representations, polygonal
face sets, materials, structural loads/supports, and round-trip IFC writing are
not yet implemented. Unsupported geometry is diagnosed rather than approximated.


## Pass 6

### Validation notes

#### Pass 6 validation record

Pass 6 functionality was incorporated into the Pass 7 workspace because no
separate Pass 6 artifact was produced.

##### Included checks
- deterministic STEP UUID mapping;
- AP242 schema detection;
- millimetre-to-metre conversion;
- polygonal planar face mapping;
- source-record and file hashing;
- explicit unsupported-geometry diagnostics.

##### Execution status
Cargo, Rustc and Rustfmt were unavailable in the generation environment.
Execute `cargo test --workspace` in Rust-enabled CI before acceptance.


## Pass 7

### Validation notes

#### Pass 7 validation record

##### Acceptance targets
- invalid mesh options are rejected;
- the same input and options produce identical UUIDs and topology;
- shared-edge midpoints are reused during 1-to-4 refinement;
- refinement stops at the edge target or reports exhaustion as an error;
- missing vertices and degenerate triangles are errors;
- edge length, area, minimum angle and aspect ratio are reported;
- CLI outputs carry hashes of geometry, meshes and quality reports.

##### Automated tests
`structural-mesh-simple` contains deterministic refinement and quality tests.
The STEP adapter includes unit and integration fixture tests.

##### Execution status
Cargo, Rustc and Rustfmt were unavailable in the generation environment.
Execute:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- step-import \
  crates/step-adapter/tests/fixtures/ap242-triangle.stp ./step-out
cargo run -p structural-cli -- mesh-generate \
  ./step-out/step-geometry.json ./mesh-out
```

before accepting this pass.


## Pass 8

### Migration notes

#### Pass 7 to Pass 8 mesh migration

Pass 8 is additive.

- Existing `TriangleMesh`, `SurfaceMeshOptions`, `MeshQualityReport` and
  `SurfaceMeshGenerator` users remain supported.
- New integrations should use `AnalysisMesh` and `GeneralMeshGenerator` when
  line, quadrilateral or volume elements are required.
- A triangle mesh converts losslessly to `AnalysisMesh` by mapping vertices to
  nodes and triangles to `triangle3` elements.
- The generalized schema uses a connectivity vector because element topology
  determines its required length. Validation must reject mismatches.
- No automatic conversion from generalized volume elements back to
  `TriangleMesh` is defined, because that would discard volume connectivity.

### Validation notes

#### Pass 8 validation record

##### Scope
Expansion from triangular surface meshes to generalized line, surface and
extruded-volume finite-element topology.

##### Static acceptance criteria

- Pass 7 `SurfaceMeshGenerator` contracts remain present.
- Mesh schema version is advanced to `0.2`.
- Connectivity declares exact node counts for all supported topologies.
- Straight B-rep edges can be deterministically subdivided into line elements.
- Planar four-edge faces can remain quadrilateral rather than being forced into
  triangles.
- Triangle extrusion produces wedges.
- Quadrilateral extrusion produces hexahedra.
- Quality assessment supports one-, two- and three-dimensional measures.
- Missing nodes, invalid connectivity and zero measure are errors.
- Outputs include canonical SHA-256 evidence.
- The source Pass 7 workspace is not overwritten.

##### Included tests

- deterministic quadrilateral surface generation;
- quadrilateral-to-hexahedron extrusion;
- unit-cube volume recovery;
- line subdivision under a maximum edge length;
- declarations for tetrahedron, pyramid, wedge and hexahedron node counts;
- all existing Pass 1–7 tests are retained.

##### Execution status
Cargo, rustc and rustfmt were not installed in the generation environment.
Compilation and tests therefore require a Rust-enabled workstation or CI runner:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- geometry-demo ./geometry-out
cargo run -p structural-cli -- mesh-generate-general \
  ./geometry-out/geometry.json volume ./volume-mesh-out
```

##### Limitations
The CLI demonstration geometry is triangular and therefore extrudes to a wedge.
The quadrilateral/hexahedral path is covered by unit tests. No claim is made for
automatic unstructured tetrahedral, pyramid-transition or hex-dominant solid
meshing.


## Pass 9

### Migration notes

#### Pass 8 to Pass 9 migration

Pass 9 is additive.

- Workspace version changes from `0.8.0` to `0.9.0`.
- Mesh schema changes from `0.2` to `0.3`.
- Existing line, surface and extrusion APIs remain available.
- Consumers wanting automatic tetrahedralization should depend on
  `structural-mesh-api` and implement or select `VolumeMeshGenerator`.
- The bundled `ReferenceTetMesher` accepts only one connected closed
  convex/star-shaped boundary and rejects cavities and multiple regions.

### Validation notes

#### Pass 9 validation record

##### Automated checks

```bash
cargo fmt --all -- --check
cargo test --workspace
```

The `structural-mesh-tet` tests cover:

1. rejection of an open triangular boundary;
2. deterministic tetrahedralization of a closed tetrahedral solid;
3. positive element volumes and total-volume conservation;
4. volume-driven refinement with deterministic Steiner points.

##### Acceptance criteria

- Existing Pass 8 APIs and tests remain compatible.
- Every generated volume element is `Tetrahedron4`.
- Boundary edges are used exactly twice before meshing.
- Missing, duplicate, degenerate, open and non-manifold topology is diagnosed.
- Generated tetrahedra have positive signed volume.
- Output identifiers are stable for identical input/options.
- Boundary-facet mappings and evidence hashes are emitted.

##### Deferred production validation

Robust exact-predicate self-intersection detection, non-star-shaped constrained
Delaunay filling, cavities, multi-region interfaces, curved high-order elements,
boundary layers and independent benchmark certification remain future work.


## Pass 10

### Migration notes

#### Pass 9 to Pass 10 migration

Pass 10 is additive. Existing domain, geometry, mesh and solver artifacts remain valid.

New consumers may:

1. create a `RulePackage` using schema `0.1`;
2. create a `RuleEvaluationInput` using explicit SI units and source references;
3. call `structural_rules_engine::evaluate`;
4. retain the package snapshot, evaluation report and evidence together.

`AnalysisInput.rule_sets` can reference the SHA-256 of the exact package used. No
automatic migration inserts or selects a national annex: that remains an explicit
project decision.

### Validation notes

#### Pass 10 validation record

##### Scope

Pass 10 adds versioned rule-package contracts and a deterministic reference evaluator
for applicability and scalar limit checks.

##### Automated tests

- package and rule traceability;
- pass and fail outcomes;
- deterministic report/evidence generation;
- explicit rejection of implicit unit conversion.

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo run -p structural-cli -- rules-evaluate   crates/rules-engine/tests/fixtures/demo-be-package.json   crates/rules-engine/tests/fixtures/demo-evaluation-input.json   ./rules-out
```

Expected artifacts:

- `rule-package-snapshot.json`
- `rule-evaluation-report.json`
- `rule-evaluation-evidence.json`

##### Limitations

The included package is synthetic and must not be used for design. The engine currently
supports conjunction-only applicability and scalar limit comparisons. It does not yet
implement code-specific equations, combinations, reliability differentiation, fire,
fatigue, seismic or material-specific checks. Rust tooling was unavailable in the
generation environment, so CI must execute formatting and tests.


## Pass 11

### Migration notes

#### Pass 10 to Pass 11 migration

Pass 11 changes rule-package and evaluation-input schemas from `0.1` to `0.2`.

##### Package migration

1. Set `schema_version` to `0.2`.
2. Add `clause_graph` with `nodes` and `links`.
3. Add `clause_node_ids` and `depends_on` to each rule.
4. Move repeated flat clause references into reusable clause nodes where practical.
5. Validate that clause and rule dependency graphs are acyclic.
6. Update the package version and `supersedes` metadata.
7. Recompute and retain the package hash.

Empty `clause_graph`, `clause_node_ids`, and `depends_on` values preserve Pass 10
flat-reference behavior, but explicit migration is required because schema versions
are intentionally strict.

##### Evaluation input migration

Set `schema_version` to `0.2`. Numeric-input and context structures are unchanged.

##### Result migration

Consumers must handle:

- `blocked` as a fifth check status;
- `blocked_count`;
- `evaluation_order`;
- `clause_graph_sha256`;
- `direct_clause_node_ids`;
- `resolved_clauses`;
- resolved rule `dependencies`.

A blocked check is not a failed check: it was not evaluated because a declared
prerequisite did not reach the required status.

### Validation notes

#### Pass 11 validation record

##### Scope

Pass 11 adds a richer clause graph and deterministic dependency resolution to the
rules engine.

##### Automated coverage

- stable topological rule ordering;
- transitive clause dependency expansion;
- rule-dependency evidence;
- blocked downstream checks;
- `completed` dependency semantics;
- rule-cycle rejection;
- clause-cycle rejection;
- deterministic report/evidence generation;
- strict unit mismatch handling retained from Pass 10.

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace

cargo run -p structural-cli -- rules-evaluate \
  crates/rules-engine/tests/fixtures/demo-be-package.json \
  crates/rules-engine/tests/fixtures/demo-evaluation-input.json \
  ./rules-out
```

Expected artifacts remain:

- `rule-package-snapshot.json`
- `rule-evaluation-report.json`
- `rule-evaluation-evidence.json`

##### Limitations

The fixture remains synthetic and is not a Eurocode or Belgian National Annex
implementation. Semantic clause links are evidence/navigation metadata; only explicit
`dependencies` affect resolution. The engine still evaluates scalar limits rather
than typed formula graphs. Rust tooling was unavailable in the generation environment,
so formatting and tests must be executed in Rust-enabled CI.


## Pass 12

### Migration notes

#### Pass 12 migration

Pass 12 does not change Rust domain, mesh, solver, or rules JSON schemas.

##### Repository
A new .NET solution is located at `apps/desktop/Structural.Desktop.sln`. Existing
Cargo commands remain unchanged.

##### Desktop state
Drag-and-drop assignments are saved separately using
`structural-desktop-assignments/0.1`. They are not automatically written into the
source model. Consumers must treat these files as provisional UI commands rather
than authoritative analysis input.

##### Requirements
- .NET 8 SDK for core builds and tests.
- Windows with .NET Desktop Runtime for the WPF executable.
- Rust/Cargo available on `PATH` for jobs launched by the current shell.

### Validation notes

#### Pass 12 validation

##### Automated checks

```bash
cargo fmt --all -- --check
cargo test --workspace
dotnet build apps/desktop/src/Structural.Desktop.Core/Structural.Desktop.Core.csproj
dotnet run --project apps/desktop/tests/Structural.Desktop.Core.Tests
```

On Windows:

```powershell
dotnet build apps/desktop/Structural.Desktop.sln
dotnet run --project apps/desktop/src/Structural.Desktop.Wpf
```

The core contract runner covers assignment undo/redo, replacement restoration,
deterministic command-palette ordering and duplicate command rejection.

##### Manual acceptance
1. Open a structural JSON artifact and expand the model tree.
2. Select an entity and verify the selection label.
3. Drag a load, support and material onto tree items.
4. Undo and redo the assignments with toolbar buttons and keyboard shortcuts.
5. Open the command palette with Ctrl+K, filter and run a command.
6. Start Cargo workspace tests, observe job state and cancel a running job.
7. Save the assignment sidecar and verify schema `0.1`.
8. Confirm the 3-D viewport remains responsive while a job runs.

##### Limitations
The viewport displays a representative primitive rather than authoritative model
geometry. Tree drops target the selected item because stock WPF tree hit testing is
kept out of the core. Domain mutation, selection highlighting, camera controls,
result contours and signed desktop distribution are deferred.


## Pass 13

### Migration notes

#### Pass 13 migration guide

Pass 12 process invocations remain valid. New integrations should prefer the stable
automation executable.

##### Build

```bash
cargo build -p structural-automation
```

##### Discover the API

```bash
cargo run -p structural-automation -- describe ./automation-description.json
```

##### One-shot request

```bash
cargo run -p structural-automation -- call request.json response.json
```

##### JSON-lines local transport

```bash
cargo run -p structural-automation -- serve
```

Write one compact `ApiRequest` JSON object per line on standard input. One response
is written per valid input line.

##### Python client migration

Add `bindings/python` to `PYTHONPATH` and use `StructuralAutomationClient`. Clients
must request explicit capabilities and should check API major-version compatibility
through `describe()` during startup.

The desktop's existing `BackgroundJobManager` can launch `structural-automation`
without changing the Pass 12 UI command/job abstractions.

### Validation notes

#### Pass 13 validation

##### Rust

```bash
cargo fmt --all -- --check
cargo test --workspace
```

The tests cover v1 fixture deserialization, version discovery, capability denial,
unknown methods, pre-dispatch cancellation and deterministic hashing.

##### Python binding

```bash
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
python3 -m py_compile bindings/python/structural_api/*.py
```

##### Manual smoke tests

```bash
cargo run -p structural-automation -- describe /tmp/description.json

cargo run -p structural-automation -- python \
  examples/automation/script-request.json \
  /tmp/script-response.json
```

Expected script status is `completed` and the operation response contains the
SHA-256 of `Pass 13`.

##### Security checks

Confirm that restricted scripts cannot use `open`, `__import__`, `eval`, `exec` or
`compile`; confirm timeout and maximum-operation failures terminate cleanly. These
checks demonstrate defense in depth only. Use an OS/container sandbox for untrusted
scripts.

### Local validation evidence

```json
[
  {
    "command": "/usr/local/bin/python3 /mnt/data/structural-platform-pass13/bindings/python/generate.py",
    "returncode": 0,
    "stdout": "Python binding matches API 1.0 (4 operations)\n",
    "stderr": ""
  },
  {
    "command": "/usr/local/bin/python3 -m unittest discover -s /mnt/data/structural-platform-pass13/bindings/python/tests -v",
    "returncode": 0,
    "stdout": "",
    "stderr": "test_catalog_matches_binding_major (test_contract.ContractTests.test_catalog_matches_binding_major) ... ok\ntest_version_constants (test_contract.ContractTests.test_version_constants) ... ok\n\n----------------------------------------------------------------------\nRan 2 tests in 0.000s\n\nOK\n"
  },
  {
    "command": "/usr/local/bin/python3 -m py_compile /mnt/data/structural-platform-pass13/bindings/python/structural_api/__init__.py /mnt/data/structural-platform-pass13/bindings/python/structural_api/client.py",
    "returncode": 0,
    "stdout": "",
    "stderr": ""
  }
]
```


## Pass 14

### Migration notes

#### Pass 14 migration

Pass 13 automation clients remain wire-compatible. Pass 14 adds plugin facilities;
it does not change `structural-automation/1.0`.

To migrate an extension:

1. Implement the one-request/one-response contract from `structural-plugin-sdk`.
2. Create a `structural-plugin/1.0` manifest.
3. Declare the narrowest automation capabilities needed.
4. Select an API range containing host API `1.0`.
5. Sign the manifest with an Ed25519 publisher key.
6. Add the public key to a deployment-managed trust store.
7. Test under the same sandbox backend used in production.
8. Retain the generated plugin audit record with the engineering job evidence.

Unsigned manifests and unknown signing keys are rejected. OS-sandbox requirements
never downgrade to direct execution.

### Validation notes

#### Pass 14 validation

##### Automated checks

```bash
cargo fmt --all -- --check
cargo test --workspace
```

The tests cover manifest validation, signature verification and tamper detection,
API-range negotiation, capability denial, and fail-closed sandbox selection.

##### Linux integration exercise

Install bubblewrap, build the example, copy its binary beside the manifest, sign the
manifest using a non-production test key, create the matching trust store, then run:

```bash
cargo run -p structural-plugin-host-app -- run \
  ./plugin-package \
  ./plugin-package/manifest.signed.json \
  ./trust-store.json \
  ./examples/plugins/echo-plugin/invocation.json \
  ./plugin-audit.json
```

Verify that:

- an unsigned or modified manifest is rejected;
- an untrusted key is rejected;
- a missing capability is rejected before launch;
- a timeout or abnormal exit produces a contained result and audit status;
- a mismatched invocation ID is rejected as an invalid response;
- no network namespace is shared unless policy explicitly enables it;
- audit hashes and signer identity are retained with the job evidence.

The direct backend is intended only for controlled development and is not enabled
by the production CLI.


## Pass 15

### Migration notes

#### Pass 15 migration guide

Pass 15 is additive. Existing model, geometry, mesh, rules, automation and plugin
artifacts remain valid, and the original linear `StructuralSolver` trait is unchanged.

##### Workspace changes

- Workspace version: `0.14.0` → `0.15.0`
- New crate: `structural-solver-nonlinear`
- New API contract types in `structural-solver-api`
- Automation API minor version: `1.0` → `1.1` (additive `analysis.solve_nonlinear`)\n- New CLI commands: `nonlinear-run` and `nonlinear-resume`
- New schema: `api/nonlinear-analysis.schema.json`

##### Adopting the nonlinear API

Construct a `NonlinearAnalysisInput` using schema `0.1`. Every DOF has SI-valued
initial stiffness, reference force, optional yield force and hardening ratio, and
an optional cubic geometric coefficient. Configure load stepping and tolerances
explicitly with `NonlinearExecutionOptions`.

A restart point is not a generic model artifact. Resume only with the same analysis
ID, solver ID/version and ordered DOF definition. The host rejects incompatible or non-finite
restart state.

##### Compatibility

No existing JSON schema was changed. Consumers that do not use nonlinear analysis
require no migration. New consumers should treat unknown termination values and
future schema versions as incompatible until explicitly supported.

### Validation notes

#### Pass 15 validation

##### Automated checks

```bash
cargo fmt --all -- --check
cargo test --workspace
```

The nonlinear suite covers:

1. Elastic response: `k = 1000 N/m`, `F = 100 N`, expected `u = 0.1 m`.
2. Bilinear material response: `k = 1000 N/m`, `Fy = 10 N`,
   post-yield tangent `0.1 k`, `F = 20 N`, expected `u = 0.11 m`.
3. Cubic geometric response: `F(u) = 1000u + 100000u³`,
   `F = 200 N`, expected `u = 0.1 m`.
4. Restarted and uninterrupted paths produce the same final state.
5. Deterministic runs produce identical result structures.
6. Invalid and cross-analysis restart points are rejected.

The fixtures are analytical software-verification cases, not independent validation
of a general nonlinear finite-element solver.

##### CLI smoke test

```bash
cargo run -p structural-cli -- nonlinear-run   examples/nonlinear/bilinear-material.json   examples/nonlinear/options.json   ./nonlinear-out
```

Resume from the emitted converged restart:

```bash
cargo run -p structural-cli -- nonlinear-resume   examples/nonlinear/bilinear-material.json   examples/nonlinear/options.json   ./nonlinear-out/nonlinear-restart.json   ./nonlinear-resumed-out
```

The first command targets load factor 1.0, so a meaningful resume demonstration
should instead generate a restart with an options file whose target is below 1.0,
then resume with the provided target-1.0 options.

##### Review criteria

- no state from a failed iteration or rejected step is committed;
- both convergence criteria are reported for every iteration;
- every accepted step has a reproducible load factor and state;
- restart snapshots are produced only for accepted steps;
- non-positive tangents and exhausted cutbacks fail closed;
- result and input artifacts are SHA-256 linked by CLI evidence.

##### Scope limitation

The reference solver has independent scalar DOFs. It does not validate frame, shell
or solid geometric stiffness, multiaxial constitutive laws, contact, cyclic plasticity,
large rotations, bifurcation tracking or post-buckling continuation. Those require
separate benchmark and independent-validation programmes.

### Local validation evidence

```json
{
  "pass": 15,
  "source": "/mnt/data/structural-platform-pass14",
  "workspace_version": "0.15.0",
  "checks": {
    "json_parse": "passed",
    "toml_parse": "passed",
    "python_binding_generation": "passed",
    "python_contract_tests": "passed",
    "rust_format": "not_run_tool_unavailable",
    "rust_tests": "not_run_tool_unavailable"
  },
  "scope": [
    "nonlinear solver API",
    "reference nonlinear spring solver",
    "CLI",
    "automation API 1.1",
    "benchmarks",
    "documentation"
  ]
}
```


## Pass 16

### Migration notes

#### Pass 16 migration

Pass 16 is additive.

- Workspace version changes from `0.15.0` to `0.16.0`.
- Existing linear and nonlinear solver traits and JSON schemas are unchanged.
- New consumers may use `DynamicStructuralSolver` and the `0.1` dynamics schema.
- Add `structural-solver-dynamics` only where the dense reference implementation is
  required.
- Persist the supplied options with results because normalization and tolerances are
  part of the interpretation.
- Do not treat the reference buckling geometric matrix convention as a universal
  production assembly convention.

### Validation notes

#### Pass 16 validation

##### Automated checks
- analytical diagonal two-DOF modal eigenvalues and mass normalization;
- analytical diagonal generalized buckling factors;
- SDOF Newmark integration with equilibrium residual checks;
- repeated modal-run equality under the deterministic profile;
- shared API validation for matrix dimensions, symmetry, finite values, unique DOFs,
  positive time increments, damping parameters and stable Newmark settings.

##### Required local commands
```bash
cargo fmt --all -- --check
cargo test --workspace
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
```

##### Environment note
The delivery environment did not provide the Rust `cargo` executable. The repository
was therefore prepared for these checks, but Rust compilation and tests must be run
in CI or a Rust-enabled workstation before engineering use.

##### Limitations
These tests are implementation regression tests, not independent solver
qualification or structural-design validation.

### Local validation evidence

```json
{
  "pass": 16,
  "workspace_version": "0.16.0",
  "rust_toolchain_available": false,
  "commands_not_executed": [
    "cargo fmt --all -- --check",
    "cargo test --workspace"
  ],
  "reason": "The delivery environment did not provide rustc/cargo.",
  "static_checks": [
    "workspace member and dependency wiring reviewed",
    "JSON examples parsed",
    "documentation and schema versions reviewed"
  ]
}
```


## Pass 17

### Migration notes

#### Pass 17 migration guide

Pass 17 is additive. Existing solver inputs and Pass 16 CLI commands remain valid.

##### New workspace crate
Add `structural-execution` when an application packages or verifies local/cloud jobs.
Do not invent provider-specific job JSON. Construct `ExecutionEnvelope`, validate it,
upload its artifacts by hash, and retain the envelope digest.

##### Artifact migration
Existing path-based inputs may be uploaded to `ArtifactStore`; persist the returned
`ContentArtifact` descriptor in the envelope. Paths are transport concerns and must
not be used as cloud identities.

##### Result acceptance
Do not compare complete result files byte-for-byte when runtime metadata or valid
floating-point variation is expected. Create a reviewed `ComparisonProfile`, keep
ignored JSON pointers narrow, and archive the generated `ResultComparison`.

##### Attestations
Cloud deployments must provision an Ed25519 signing key outside the repository and
publish its public key under a stable key ID. Never use test signing keys in
production. Verify an attestation before accepting its output descriptors.

##### Compatibility
The execution schema starts at `structural-execution/1.0`. Breaking wire changes
require a new major schema. Additive optional fields may be introduced within the
major version only after old-reader compatibility tests exist.

### Validation notes

#### Pass 17 validation

##### Automated checks
- content-addressed artifact upload/download with SHA-256 revalidation;
- execution-envelope schema, digest and identity validation;
- Ed25519 cloud-attestation signing, verification and tamper rejection;
- absolute/relative JSON numerical comparison with exact structural comparison;
- explicitly ignored metadata paths;
- retryable failure recovery and attempt evidence;
- cancellation before remote dispatch;
- existing workspace regression tests.

##### Required local commands

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
```

##### Manual CLI checks

```bash
cargo run -p structural-cli -- execution-package   analysis.modal examples/execution/job-input.json ./execution-job

cargo run -p structural-cli -- execution-artifact-get   ./execution-job/artifacts <input-sha256> ./downloaded-input.json

cargo run -p structural-cli -- execution-compare   examples/execution/local-result.json   examples/execution/cloud-result.json   examples/execution/comparison-profile.json   ./parity-report.json
```

##### Environment note
The delivery environment did not provide `cargo` or `rustc`. Rust formatting,
compilation and tests must therefore run in CI or on a Rust-enabled workstation
before use.

##### Limitations
The included store is local filesystem infrastructure, and the remote executor is an
interface plus retry controller. Production authentication, HTTP transport, object
storage, scheduler enforcement, key custody and timestamping are not claimed.

### Local validation evidence

```json
{
  "pass": 17,
  "workspace_version": "0.17.0",
  "source_workspace": "structural-platform-pass16",
  "created_utc": "2026-09-27T00:00:00Z",
  "checks": [
    {
      "command": "cargo fmt --all -- --check",
      "status": "not_run",
      "reason": "cargo not installed in delivery environment"
    },
    {
      "command": "cargo test --workspace",
      "status": "not_run",
      "reason": "cargo not installed in delivery environment"
    },
    {
      "command": "python3 bindings/python/generate.py",
      "status": "not_run",
      "reason": "generated bindings unchanged in Pass 17"
    },
    {
      "command": "python3 -m unittest discover -s bindings/python/tests -v",
      "status": "not_run",
      "reason": "automation contract unchanged in Pass 17"
    }
  ],
  "static_review": [
    "execution envelope and JSON Schema added",
    "content-addressed artifact integrity checked on download",
    "attestation signature covers unsigned canonical payload digest",
    "retryable and fatal failures remain distinguishable",
    "comparison report retains hashes, tolerances and divergent JSON pointers"
  ],
  "engineering_release_blocker": "Run all required checks in a Rust-enabled CI environment before engineering use."
}
```


## Pass 18

### Migration notes

#### Pass 18 migration guide

Pass 18 adds collaboration contracts without changing existing model, solver,
automation, plugin or execution formats.

##### Adoption

1. Convert stable domain entities to `DomainObject` records with persistent UUIDs.
2. Create a `ModelSnapshot` and initialize a `Repository`.
3. Assign least-privilege roles.
4. Commit edits with the branch head observed when editing began.
5. On a stale-head response, fetch changes and perform an object-level merge.
6. Exchange `SyncBundle` objects through the chosen local, on-premise or cloud transport.
7. Encrypt bundles when transport or storage is not already protected.

##### Compatibility

Existing Pass 17 project files remain valid. Collaboration is an optional envelope.
Do not generate a new UUID merely because an object property changed; stable object
identity is required for meaningful three-way merging.

##### Security

Do not store raw collaboration encryption keys in repository JSON, logs, reports or
sync bundles. Production deployments must define identity federation, token expiry,
key rotation, revocation, backup recovery and server-side authorization independently
of the client-side role hints represented here.

### Validation notes

#### Pass 18 validation

##### Intended checks

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
```

##### Collaboration test coverage

- stale branch heads are rejected;
- revision and snapshot hashes are revalidated;
- independent edits merge automatically;
- same-object edits produce explicit conflicts;
- conflict resolutions create two-parent merge revisions;
- offline synchronization fast-forwards only when safe;
- divergent heads are reported rather than overwritten;
- encrypted bundles authenticate associated repository metadata;
- invalid roles, object references and deleted objects fail closed.

##### Environment limitation

The generation environment does not contain `cargo` or `rustc`, so Rust formatting,
compilation and tests could not be executed here. Static workspace membership, JSON
parsing, schema presence and documentation checks were run locally. Run the full
commands above before accepting this pass.

### Local validation evidence

```json
{
  "pass": 18,
  "workspace_version": "0.18.0",
  "source_workspace": "structural-platform-pass17",
  "created_utc": "2026-09-27T00:00:00Z",
  "toolchain_checks": [
    {
      "command": "cargo fmt --all -- --check",
      "status": "not_run",
      "reason": "cargo not installed in delivery environment"
    },
    {
      "command": "cargo test --workspace",
      "status": "not_run",
      "reason": "cargo and rustc not installed in delivery environment"
    },
    {
      "command": "python3 bindings/python/generate.py",
      "status": "not_run",
      "reason": "automation API contract unchanged in Pass 18"
    },
    {
      "command": "python3 -m unittest discover -s bindings/python/tests -v",
      "status": "not_run",
      "reason": "generated Python binding contract unchanged in Pass 18"
    }
  ],
  "static_checks": [
    {
      "check": "parse all JSON files",
      "status": "passed",
      "files_checked": 38,
      "errors": []
    },
    {
      "check": "parse all TOML files",
      "status": "passed",
      "files_checked": 27,
      "errors": []
    },
    {
      "check": "workspace contains collaboration crate exactly once",
      "status": "passed"
    },
    {
      "check": "workspace version is 0.18.0",
      "status": "passed"
    },
    {
      "check": "Pass 18 documentation and schemas present",
      "status": "passed"
    }
  ],
  "implemented_controls": [
    "immutable revision and snapshot hashes",
    "optimistic stale-head rejection",
    "domain-object three-way merge and explicit conflict resolution",
    "role-based authorization checks",
    "revision/object review comments",
    "safe offline fast-forward and divergent-head reporting",
    "XChaCha20-Poly1305 authenticated sync-bundle encryption"
  ],
  "engineering_release_blocker": "Run formatting, compilation, tests, dependency review and security review in Rust-enabled CI before engineering use."
}
```


## Pass 19

### Migration notes

#### Pass 19 migration

Pass 19 is additive.

##### Workspace

The new `structural-assurance` crate is a workspace member and the workspace version is
`0.19.0`. Consumers that pin workspace packages should refresh their lockfile.

##### New public contracts

- `api/assurance-catalog.schema.json`
- `api/assurance-evidence-bundle.schema.json`
- `structural_assurance::AssuranceCatalog`
- `structural_assurance::build_portal`

No Pass 18 collaboration documents are rewritten. Existing projects require no data
migration.

##### Publishing evidence

1. Place evidence below a controlled source directory.
2. Record byte length and lowercase SHA-256 for every artifact.
3. Declare expected ranges and limitations before publishing a run.
4. Use `not_run` when a platform/version pair was not executed.
5. Build the portal; generation fails on hash, range, cross-reference or path errors.
6. Publish the complete output directory, not only `index.html`.

Do not upgrade an external report to `signature_verified`: that state intentionally
fails closed until a trust and detached-signature profile is standardized.

### Validation notes

#### Pass 19 validation

##### Intended checks

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v

cargo run -p structural-cli -- assurance-build \
  examples/assurance/catalog.json examples/assurance ./assurance-portal
```

##### Assurance coverage

- duplicate and malformed identifiers are rejected;
- benchmark ranges must be finite and ordered;
- passing/failing status must agree with every declared metric range;
- `not_run` entries cannot carry fabricated results;
- unknown benchmark and limitation references fail;
- unsafe or parent-relative evidence paths fail;
- every source artifact is checked for SHA-256 and byte length;
- tampered artifacts prevent publication;
- the output contains a static index, catalog, manifest and content-addressed files;
- external reports retain an explicit verification state;
- unsupported `signature_verified` claims fail closed.

##### Environment limitation

The generation environment does not contain `cargo` or `rustc`, so Rust formatting,
compilation and tests could not be executed here. JSON/TOML parsing, cross-reference
checks, artifact hashes, workspace membership and archive integrity were checked
statically. Run the full commands above before publication or engineering use.

### Local validation evidence

```json
{
  "pass": 19,
  "generated_at_utc": "2026-09-27T18:30:00Z",
  "environment": {
    "cargo_available": false,
    "rustc_available": false
  },
  "checks": [
    {
      "name": "workspace_member",
      "status": "passed"
    },
    {
      "name": "toml_parse",
      "status": "passed"
    },
    {
      "name": "json_parse",
      "status": "passed"
    },
    {
      "name": "catalog_cross_references",
      "status": "passed"
    },
    {
      "name": "evidence_sha256_and_length",
      "status": "passed"
    },
    {
      "name": "rust_compile_and_tests",
      "status": "not_run",
      "reason": "cargo and rustc unavailable"
    }
  ],
  "disclaimer": "Static generation validation only; not engineering validation or certification."
}
```


## Pass 20

### Migration notes

#### Pass 20 migration

1. Pin the Pass 19 project and retain its hashes.
2. Upgrade workspace consumers from `0.19.0` to `0.20.0`.
3. Generate immutable release artifacts, SPDX SBOM and SLSA-compatible provenance.
4. Replace all illustrative Pass 20 evidence with executed CI/pilot evidence.
5. Run restore and migration rehearsals on copies of production-scale data.
6. Configure trusted release public keys; keep private keys in HSM/KMS.
7. Verify with `structural-cli release-verify`.
8. Promote pilot → beta → stable only after independent human approval.

No Pass 19 model or result artifact is silently rewritten.


## Integrated desktop passes 21–25

These five passes were implemented together as one coherent desktop increment. No
per-pass Markdown or local-validation files were created.

### Pass 21 — Authoritative desktop structural model

**Implemented**

- Added a typed `StructuralModel` containing nodes, line members, SI geometry, section
  area, and Young's modulus.
- Added tolerant import of both `position: {x,y,z}` and `xyz_m: [x,y,z]`.
- Added deterministic diagnostics for missing IDs, invalid coordinates, duplicate IDs,
  dangling member references, zero-reference members, and invalid member properties.
- Connected the model explorer and 3-D scene to the same parsed structural model.
- Replaced the one-member hard-coded preview with a stable three-member starter truss.

**Acceptance intent**

Opening `apps/desktop/samples/demo-model.json` must populate the tree and viewport from
the file rather than from preview geometry. Invalid references must remain visible as
diagnostics and must block analysis.

### Pass 22 — Viewport interaction and engineering views

**Implemented**

- Retained and integrated left-drag orbit, right-drag pan, wheel zoom, click selection,
  fit, isometric, front, top, and side views.
- Kept tree and viewport selection synchronized.
- Rendered a scaled grid, nodes, members, selected-object highlighting, material colours,
  support glyphs, load arrows, and a deformed-shape overlay.
- Added visually distinct fixed, pinned, and roller support symbols.

**Acceptance intent**

Every rendered node/member must map back to a model ID. Camera operations must not alter
model data, and viewport selection must select the corresponding explorer item.

### Pass 23 — Editable loads, supports, and undoable assignments

**Implemented**

- Added click and drag/drop assignment of point loads, equivalent member gravity loads,
  fixed supports, pinned supports, roller supports, steel, and concrete.
- Enforced target compatibility: supports and point loads target nodes; materials and
  equivalent gravity loads target members.
- Added editable load magnitude in kN and X/Z direction.
- Persisted load parameters in assignment schema `structural-desktop-assignments/0.2`.
- Made assignment removal and replacement undoable.

**Acceptance intent**

Changing a load must invalidate previous results, update its visible/listed value, and
feed the next analysis. Undo must restore the exact prior assignment.

### Pass 24 — Solver linkage and result visualization

**Implemented**

- Added a deterministic small-displacement linear pin-jointed truss solver in the
  desktop core.
- Assembled member stiffness from SI `area_m2` and `youngs_modulus_pa`.
- Applied point/equivalent member loads and pinned/fixed/roller constraints from the
  same assignments shown in the UI.
- Added instability, missing-support, missing-load, and invalid-model failures.
- Added node displacements, support reactions, member axial forces, maximum displacement,
  warning output, and an automatically scaled deformed shape.
- Used red for tension and blue for compression in the result overlay.

**Explicit boundary**

This solver projects geometry into the global X-Z plane and models axial-only,
pin-jointed members. A fixed and pinned support both restrain X/Z translation in this
truss formulation; a roller restrains Z only. Gravity is currently an entered total
equivalent member force split between end nodes. This is not a validated beam, shell,
solid, nonlinear, buckling, or code-checking workflow.

### Pass 25 — Usability, tests, and packaging readiness

**Implemented**

- Reworked the visual hierarchy, hover states, analysis action, assignment editor,
  validation pane, results pane, and status messages.
- Updated the bundled sample to a stable triangular truss with explicit SI properties.
- Added desktop-core tests for typed model parsing, dangling references, undoable
  removal, and the end-to-end truss equilibrium path.
- Updated `README.md` and this consolidated pass history without adding pass-specific
  documentation files.

**Required validation**

The generation environment did not provide `dotnet`, `cargo`, `rustc`, or `rustfmt`, so
no compilation or executable test is claimed here. Run on a Windows/.NET-capable agent:

```powershell
dotnet restore apps/desktop/Structural.Desktop.sln
dotnet build apps/desktop/Structural.Desktop.sln -c Release
dotnet run --project apps/desktop/tests/Structural.Desktop.Core.Tests/Structural.Desktop.Core.Tests.csproj -c Release
dotnet publish apps/desktop/src/Structural.Desktop.Wpf/Structural.Desktop.Wpf.csproj `
  -c Release -r win-x64 --self-contained true `
  -p:PublishSingleFile=true -o publish/StructuralDesktop
```

The wider repository checks remain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 -m unittest discover -s bindings/python/tests -v
```

### Pass 22 refinement — explicit viewport manipulation

The viewport interaction pass was refined without adding another documentation file.

- Added explicit Select, Orbit, and Pan tools with visible active state.
- Added CAD-style temporary gestures: middle-drag or Alt+left for orbit, right-drag or
  Shift+left for pan, and wheel zoom.
- Added Frame Selection and double-click framing.
- Added keyboard access: `S`, `O`, `P`, `F`, `Home`, and views `1`–`4`.
- Added a grid toggle, world-axis indicator, camera-distance status, and context-sensitive
  viewport guidance.
- Preserved click selection and model-tree synchronization.
- Kept all camera changes view-only; they do not mutate structural model data.

Validation required on Windows:

```powershell
dotnet build apps/desktop/Structural.Desktop.sln -c Release
dotnet run --project apps/desktop/tests/Structural.Desktop.Core.Tests/Structural.Desktop.Core.Tests.csproj -c Release
```

The current generation environment has no .NET SDK, so compilation is not claimed.



## Passes 26–30 — practical frame-analysis workflow

These passes extend the desktop path without creating separate pass documents.

### Pass 26 — model authoring and editing — started

Implemented in this increment:

- mutable authoritative `StructuralModel` operations with validation;
- create, move, and delete nodes;
- create and delete members by selecting their end nodes;
- cascade confirmation when deleting a node used by members;
- undo/redo commands for geometry changes;
- model Save/Save As using deterministic `structural-desktop/0.4` JSON;
- unsaved-change indication and close/open protection;
- coordinate editing in metres from the property panel;
- round-trip and command-history tests.

Still required before Pass 26 is complete:

- viewport drag-to-move with explicit work-plane/snapping controls;
- box and multi-selection;
- copy, mirror, subdivide, and duplicate tools;
- autosave/recovery;
- integrated persistence of assignments with the model rather than a sidecar export.

Validation status: static JSON/XML/source checks were run in the generation environment.
The .NET SDK was unavailable, so the required desktop build and executable test suite
must run in Windows CI before acceptance.

### Pass 27 — materials, sections, releases, and units — planned

Add typed material and European section libraries, custom engineering properties,
member local axes, end releases, multi-member assignment, and selectable display units
while retaining SI storage.

### Pass 28 — load cases, combinations, and boundary conditions — planned

Replace generic assignments with named load cases, nodal/member loads, self-weight,
load combinations, springs, prescribed displacements, and explicit support degrees of
freedom.

### Pass 29 — 2D/3D frame solver and analysis pipeline — planned

Introduce versioned analysis selection, frame elements, rotational degrees of freedom,
local/global transformations, equivalent nodal loads, releases, sparse assembly,
stability diagnostics, and benchmark tests.

### Pass 30 — results, verification, and dependable desktop release — planned

Add force/moment diagrams, reactions, tables, case/combination switching, exports,
traceability, benchmark evidence, autosave/crash recovery, and dependable Windows
packaging. No production-design claim is permitted without executed validation evidence.
