# Structural Platform — Integrated Pass 25

Pass 25 integrates five desktop-focused increments on top of the Pass 20 platform
foundation. The Windows application now loads typed node/member geometry, supports
interactive viewport navigation and selection, edits visible loads/supports, and runs a
small linear X-Z truss analysis with deformed-shape and force/reaction results.

## Added

- `structural-release` crate with a versioned release-candidate contract;
- fail-closed gates for threat modeling, performance budgets, fuzzing, backup/restore,
  migrations, SBOM/provenance, incident response, support, marketplace governance,
  privacy and pilot acceptance;
- SHA-256 and byte-length verification for every declared release artifact;
- trusted Ed25519 verification for stable-channel release manifests;
- explicit, expiring and attributable control waivers;
- opt-in, data-minimised telemetry policy checks;
- marketplace signing, sandbox, review, scanning and revocation policy checks;
- release CLI, JSON Schema, tests, fuzz target, CI scaffolding and operational runbooks;
- an illustrative pilot candidate with hash-addressed evidence.



## Windows desktop quick start

```powershell
dotnet restore apps/desktop/Structural.Desktop.sln
dotnet build apps/desktop/Structural.Desktop.sln -c Release
dotnet run --project apps/desktop/src/Structural.Desktop.Wpf/Structural.Desktop.Wpf.csproj -c Release
```

In the built-in starter truss, click an object to select it, middle-drag or
Alt+left-drag to orbit, right-drag or Shift+left-drag to pan, and use the wheel to zoom.
The viewport also provides explicit Select, Orbit, Pan, Fit, Frame, standard-view, and
grid controls. Select assignments to edit load magnitude/direction, then choose
**Run analysis**. The desktop solver is deliberately limited to linear, pin-jointed members
projected into the global X-Z plane. It is a transparent integration demonstrator, not a
validated general-purpose frame/FEM solver.

## Documentation

- Contributor and coding-agent instructions: [`AGENTS.md`](AGENTS.md)
- Consolidated pass plan, migrations, and validation history:
  [`docs/passes.md`](docs/passes.md)

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 bindings/python/generate.py
python3 -m unittest discover -s bindings/python/tests -v
```

## Verify the illustrative Pass 20 pilot candidate

```bash
cargo run -p structural-cli -- release-verify   examples/release/release-candidate.json   examples/release   examples/release/release-trust-store.json   ./release-verification.json
```

## Important status

This is a production-readiness foundation, not a declaration that the product is ready
for structural design or public release. The repository does **not** claim completed
performance campaigns, long-running fuzz campaigns, restore drills, external penetration
testing, HSM/KMS signing, regulatory approval, or successful pilots with engineering firms.
The example evidence identifies planned work and must be replaced by executed, reviewed
evidence before promotion. Solver suitability remains subject to independent validation
and qualified engineering review.


## Desktop authoring status

The current desktop increment starts Pass 26: nodes can be added and moved by numeric
coordinates, members can be created between selected nodes, geometry deletion is
undoable, and models can be saved/reopened as `structural-desktop/0.4` JSON. The
existing analysis remains an axial-only X–Z truss demonstrator; Passes 27–30 are
tracked in `docs/passes.md`.
