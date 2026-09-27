# Structural Desktop

The desktop application is a dependency-light C#/.NET 8 WPF client integrated through Pass 25, with Pass 26 model authoring started.

## Projects

- `Structural.Desktop.Core`: mutable typed line model, validation, geometry commands,
  deterministic model saving, assignments, undo/redo,
  command palette, process jobs, and linear X-Z truss analysis.
- `Structural.Desktop.Wpf`: interactive `Viewport3D`, synchronized model-tree selection,
  visible/editable supports and loads, standard views, and result visualization.
- `Structural.Desktop.Core.Tests`: package-free executable behavior tests.

## Run and test

```powershell
dotnet restore apps/desktop/Structural.Desktop.sln
dotnet build apps/desktop/Structural.Desktop.sln -c Release
dotnet run --project apps/desktop/tests/Structural.Desktop.Core.Tests/Structural.Desktop.Core.Tests.csproj -c Release
dotnet run --project apps/desktop/src/Structural.Desktop.Wpf/Structural.Desktop.Wpf.csproj -c Release
```

Use **New** or **Open model**, add nodes, edit their X/Y/Z coordinates, select the
first node with **Start member**, select the second node, and choose **Finish member**.
Geometry changes support undo/redo and **Save model** (`Ctrl+S`).

The built-in starter model is ready to analyse. For the bundled `samples/demo-model.json`,
assign a pinned support to `node-a`, a roller to `node-b`, and a downward point load to
`node-c`.

Assignments are saved in a separate `structural-desktop-assignments/0.2` artifact.
Geometry accepts `position: {x,y,z}` and `xyz_m: [x,y,z]`; members accept
`start_node_id`, `end_node_id`, optional `area_m2`, and optional
`youngs_modulus_pa`.

## Analysis boundary

The current desktop solver is linear, small-displacement, axial-only, and pin-jointed.
It projects members into the global X-Z plane. It is useful for testing end-to-end model,
support, load, solve, and result integration, but it is not a validated general frame,
shell, solid, dynamic, nonlinear, or design-code solver.

Future integrations should use a versioned adapter to the Rust domain and `solver-api`
rather than silently extending this demonstrator's assumptions.
