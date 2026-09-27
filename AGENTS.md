# AGENTS.md

## Scope

These instructions apply to the entire repository. More specific instructions may be
added only when a subsystem genuinely requires them; do not create one instruction file
per development pass.

## Mission

Develop the Structural Platform into a testable, maintainable structural-analysis
application. Treat the repository as an engineering software foundation, not as a
validated production design tool. Never claim regulatory approval, solver validation,
or fitness for safety-critical design without executed and independently reviewed
evidence.

## Sources of truth and documentation policy

- `README.md` — repository overview, user-facing quick start, and primary build commands.
- `docs/passes.md` — the single roadmap, migration history, validation log, and pass record.
- Existing `docs/ADR-*.md` files — established architectural decisions.
- Existing runbooks — operational procedures that genuinely need standalone ownership.
- `api/*.schema.json`, `api/operations.json`, tests, and fixtures — executable contracts.

Keep the documentation file count low:

- Do not create per-pass Markdown, local-validation JSON, implementation-summary,
  handover, status, checklist, or duplicate setup files.
- Update `README.md`, `docs/passes.md`, or an existing relevant document instead.
- Add a new ADR or runbook only for a durable cross-cutting decision or operational
  procedure that cannot be explained clearly in an existing file.
- Prefer concise sections and links over copying the same instructions into several files.

## Engineering rules

1. Implement real behavior rather than decorative controls, hard-coded demonstrations,
   or buttons that silently do nothing.
2. Keep rendering, model state, boundary conditions, loads, meshing, solving, and results
   connected through explicit typed interfaces.
3. A visual support or load is not sufficient: when analysis is offered, the same model
   data must reach the solver and be covered by tests.
4. Use explicit units through `structural-units`; do not pass ambiguous naked values
   across public boundaries.
5. Preserve deterministic serialization, stable identifiers, reproducible results, and
   backward-compatible versioned contracts.
6. Validate untrusted files and plugin or automation input. Fail closed where security,
   signatures, permissions, or release evidence are involved.
7. Avoid production-readiness claims based on illustrative fixtures or placeholder
   evidence.
8. Keep changes focused. Do not rewrite unrelated modules or generated artifacts.
9. Add or update tests with every behavioral change.
10. Record durable architectural decisions in an ADR and concise progress/history in
    `docs/passes.md`.

## Desktop application priorities

Desktop work must prioritize usefulness over mock-up appearance:

- Render nodes and members from the loaded document, not hard-coded preview geometry.
- Provide orbit, pan, zoom, fit, and standard engineering views.
- Synchronize viewport selection, model-tree selection, and the property inspector.
- Make supports and loads editable, removable, visible, and attached to actual model
  entities.
- Clearly distinguish assigned model data from solver-ready boundary conditions.
- Surface validation and solver errors to the user; do not swallow exceptions.
- Keep long-running import, mesh, and solve work off the UI thread with cancellation and
  progress reporting.
- Maintain undo/redo for user model edits.
- Ensure keyboard navigation, readable contrast, sensible scaling, and usable empty/error
  states.

## Repository map

- `apps/cli` — command-line entry point.
- `apps/automation-host` — automation process host.
- `apps/plugin-host` — plugin process host.
- `apps/desktop` — Windows WPF desktop application and desktop-core tests.
- `crates/domain` — core model and migration contracts.
- `crates/geometry-*` — geometry interfaces and implementations.
- `crates/mesh-*` — mesh interfaces and implementations.
- `crates/solver-*` — linear, nonlinear, and dynamic solver components.
- `crates/rules-*` — rules contracts and evaluation.
- `crates/automation-*`, `crates/plugin-*`, `crates/execution` — extensibility and
  execution boundaries.
- `crates/assurance`, `crates/audit`, `crates/release` — evidence and release controls.
- `bindings/python` — generated Python contract client and tests.
- `api` — JSON schemas and operation descriptions.
- `examples` — illustrative inputs and evidence; examples are not validation proof.
- `docs` — ADRs, consolidated pass history, and operational documentation.

## Required workflow

Before editing:

1. Read the relevant crate or desktop project, its tests, and applicable ADRs.
2. Identify the authoritative model and contract; do not duplicate state.
3. Define the smallest end-to-end behavior that can be verified.

While editing:

1. Keep public APIs versioned and document migration impact.
2. Prefer small, composable types and dependency injection at subsystem boundaries.
3. Add unit tests plus integration or fixture-based tests where data crosses boundaries.
4. Keep warnings at zero in touched code.
5. Update `docs/passes.md` only when the change affects roadmap, migration, or validation
   history.

Before completion, run the applicable checks.

### Rust workspace

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Python bindings

```bash
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
```

Confirm that generation produces no unintended diff.

### Windows desktop

Run on Windows with the supported .NET SDK:

```powershell
dotnet restore apps/desktop/Structural.Desktop.sln
dotnet build apps/desktop/Structural.Desktop.sln -c Release
dotnet run --project apps/desktop/tests/Structural.Desktop.Core.Tests/Structural.Desktop.Core.Tests.csproj -c Release
```

For a distributable x64 desktop folder:

```powershell
dotnet publish apps/desktop/src/Structural.Desktop.Wpf/Structural.Desktop.Wpf.csproj `
  -c Release -r win-x64 --self-contained true `
  -p:PublishSingleFile=true -o publish/StructuralDesktop
```

## Definition of done

A change is complete only when:

- the user-visible behavior works end to end;
- automated tests cover success and important failure paths;
- formatting, linting, and relevant test suites pass;
- schemas, examples, and migrations remain compatible or are deliberately versioned;
- documentation is updated without creating per-pass files;
- limitations and unvalidated engineering assumptions are stated plainly;
- packaging includes all required runtime files and startup errors are diagnosable.

If environmental limitations prevent a required check, report exactly which command was
not run and why. Never describe an unexecuted check as passing.


## Current desktop integration boundary (Passes 21–25)

The desktop application now has one typed node/member model, editable assignment data,
and a linear X-Z truss analysis path. Extend this path rather than creating parallel
preview models.

- Preserve `StructuralModel` as the desktop authority until a deliberate adapter to the
  Rust domain contract replaces it.
- Do not describe the axial-only X-Z truss solver as a general FEM/frame solver.
- New support/load controls must carry typed values into analysis and tests, not only
  add viewport glyphs.
- The next solver increment should introduce explicit analysis type selection and a
  versioned adapter to `solver-api`, rather than silently broadening the current solver.
- Continue recording future passes only in `docs/passes.md`.


## Pass 26 authoring boundary

- Extend the mutable `StructuralModel`; do not add a second desktop geometry model.
- Geometry edits must use undoable commands and refresh tree, viewport, validation, and analysis state.
- Store coordinates in metres. Display-unit conversion belongs at the UI boundary.
- Keep pass 26–30 records in `docs/passes.md`; do not add pass-specific documents.
